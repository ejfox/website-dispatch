/**
 * Markdown render machinery — worker, render cache, and skeleton cache — at
 * MODULE scope so it lives for the whole app session.
 *
 * This used to sit at the top of FilePreview.vue's `<script setup>`, which runs
 * per component instance. FilePreview lives in a `v-else-if` tab chain, so
 * visiting Media/Activity/Journal and coming back unmounts and remounts it —
 * which recreated the Worker (re-loading the entire unified/remark/rehype
 * pipeline bundle into a fresh thread) and threw away the render cache, making
 * every post cold again. Hoisting here means one worker for the session and a
 * cache that persists across tab switches.
 *
 * The only piece left in the component is `renderCacheKey`, because it depends
 * on the component's reactive active-publish-target domain.
 */

// ── Web Worker ──────────────────────────────────────────────────────────────
// The whole unified pipeline runs off the main thread so file switching never
// blocks the UI. Requests are tagged with a monotonic id; the main thread
// throws away any response whose id doesn't match the currently-active request.
const renderWorker = new Worker(
  new URL('../workers/markdownWorker.ts', import.meta.url),
  { type: 'module' },
)
let nextRenderId = 0
let activeRenderId = -1
const pendingRenders = new Map<
  number,
  { resolve: (html: string) => void; reject: (e: Error) => void }
>()

/** True if `id` is the most recently issued render — used to drop stale
 *  responses when the user clicks another file mid-render. */
export function isActiveRender(id: number): boolean {
  return id === activeRenderId
}

renderWorker.addEventListener('message', (e: MessageEvent) => {
  const { id, html, error } = e.data as { id: number; html?: string; error?: string }
  const pending = pendingRenders.get(id)
  if (!pending) return
  pendingRenders.delete(id)
  if (error) pending.reject(new Error(error))
  else pending.resolve(html ?? '')
})

// If the worker itself fails — a module-load error or a serialization failure —
// it emits an `error`/`messageerror` event, NOT a per-request message. Without
// these handlers the render promise never settles: the preview pane spins on
// its skeleton forever and the shimmer animation quietly burns CPU. Reject
// every in-flight render so the caller falls back to raw content.
function failAllPending(reason: string) {
  const err = new Error(reason)
  for (const [, pending] of pendingRenders) pending.reject(err)
  pendingRenders.clear()
}
let workerDead = false
renderWorker.addEventListener('error', (e: ErrorEvent) => {
  workerDead = true
  const where = e.filename ? ` @ ${e.filename}:${e.lineno}:${e.colno}` : ''
  console.error(`[render] markdown worker crashed: ${e.message || 'failed to load'}${where}`)
  failAllPending(`markdown worker error: ${e.message || 'worker failed to load'}${where}`)
})
renderWorker.addEventListener('messageerror', () => {
  console.error('[render] markdown worker messageerror (uncloneable payload)')
  failAllPending('markdown worker messageerror')
})

// Hard ceiling on a single render. The pipeline processes a long post in well
// under a second, so anything past this means the worker is wedged.
const RENDER_TIMEOUT_MS = 10000

export function renderMarkdownInWorker(content: string, baseUrl: string): {
  id: number
  promise: Promise<string>
} {
  const id = ++nextRenderId
  activeRenderId = id
  // A module-load crash kills the worker permanently — every later postMessage
  // just goes nowhere and times out after 10s. Once we know it's dead, fail
  // fast so we hit the server fallback immediately instead of stalling.
  if (workerDead) {
    return { id, promise: Promise.reject(new Error('markdown worker is dead (earlier crash)')) }
  }
  const promise = new Promise<string>((resolve, reject) => {
    const timer = window.setTimeout(() => {
      if (pendingRenders.delete(id)) {
        reject(
          new Error(
            `markdown render timed out after ${RENDER_TIMEOUT_MS}ms — worker unresponsive`,
          ),
        )
      }
    }, RENDER_TIMEOUT_MS)
    pendingRenders.set(id, {
      resolve: (html) => {
        window.clearTimeout(timer)
        resolve(html)
      },
      reject: (e) => {
        window.clearTimeout(timer)
        reject(e)
      },
    })
  })
  renderWorker.postMessage({ id, content, baseUrl })
  return { id, promise }
}

// ── Server fallback ─────────────────────────────────────────────────────────
// The client-side worker can fail outright in the desktop webview — a module
// worker has no `document`, and some pipeline deps touch it. The preview server
// renders the SAME file through website2's real pipeline and is already primed
// by the /set-file POST in loadFileContent, so poll its /content until this
// file's render lands. Returns null on timeout so the caller falls back to raw.
export async function fetchServerRenderedHtml(
  filePath: string,
  timeoutMs = 8000,
): Promise<string | null> {
  const deadline = performance.now() + timeoutMs
  const wantBase = filePath.split('/').pop() || ''
  while (performance.now() < deadline) {
    try {
      const res = await fetch('http://127.0.0.1:6419/content')
      const data = (await res.json()) as {
        html?: string
        filename?: string
        processing?: boolean
      }
      if (
        data.html &&
        data.html.trim() &&
        (!data.filename || data.filename === wantBase)
      ) {
        return data.html
      }
    } catch {
      // server not up yet / transient — keep polling
    }
    await new Promise((r) => setTimeout(r, 150))
  }
  return null
}

// ── Render cache ────────────────────────────────────────────────────────────
// LRU-ish cache of rendered HTML keyed on `path|modified|domain`. Flipping
// between recently-viewed posts becomes instant — no re-running the pipeline.
// Invalidates automatically when the file is edited (mtime changes). `skeleton`
// is optional: the render is cached synchronously while the skeleton parse is
// deferred off the click→paint path and patched in later.
export type SkeletonBlock = {
  type: 'heading' | 'paragraph' | 'code' | 'list' | 'quote' | 'image' | 'hr'
  level?: number // for headings: 1..6
  lines: number // how many wrapped lines to render
  shortLast?: boolean // last line ~50% (true for prose paragraphs)
}
export type CacheEntry = { stripped: string; rendered: string; skeleton?: SkeletonBlock[] }
export const renderCache = new Map<string, CacheEntry>()
const RENDER_CACHE_MAX = 30

export function cacheRender(key: string, entry: CacheEntry) {
  // Evict oldest if over the cap. Map preserves insertion order, so the first
  // key is the oldest. Re-set to bump recency on hits.
  if (renderCache.size >= RENDER_CACHE_MAX && !renderCache.has(key)) {
    const first = renderCache.keys().next().value
    if (first !== undefined) renderCache.delete(first)
  }
  renderCache.delete(key) // re-insert to mark as most-recent
  renderCache.set(key, entry)
}

// ── Skeleton parser + cache ─────────────────────────────────────────────────
// Cheap block-level parser of markdown body text. Returns a structural outline
// used to render an accurate loading skeleton — not a full markdown parser.
export const CHARS_PER_LINE = 70
export function parseSkeleton(markdown: string): SkeletonBlock[] {
  if (!markdown) return []
  const blocks: SkeletonBlock[] = []
  const lines = markdown.split('\n')
  let i = 0
  while (i < lines.length) {
    const line = lines[i]
    const trimmed = line.trim()

    if (!trimmed) {
      i++
      continue
    }

    // Heading
    const heading = trimmed.match(/^(#{1,6})\s+(.+)$/)
    if (heading) {
      blocks.push({
        type: 'heading',
        level: heading[1].length,
        lines: 1,
      })
      i++
      continue
    }

    // Horizontal rule
    if (/^([-*_])\1\1+\s*$/.test(trimmed)) {
      blocks.push({ type: 'hr', lines: 1 })
      i++
      continue
    }

    // Fenced code block
    if (trimmed.startsWith('```') || trimmed.startsWith('~~~')) {
      const fence = trimmed.slice(0, 3)
      let codeLines = 0
      i++
      while (i < lines.length && !lines[i].trimStart().startsWith(fence)) {
        codeLines++
        i++
      }
      i++ // skip closing fence
      blocks.push({ type: 'code', lines: Math.max(1, codeLines) })
      continue
    }

    // Blockquote — collapse contiguous `>` lines into one block
    if (trimmed.startsWith('>')) {
      let quoteText = ''
      while (i < lines.length && lines[i].trim().startsWith('>')) {
        quoteText += ' ' + lines[i].replace(/^\s*>\s?/, '')
        i++
      }
      const wrapped = Math.max(1, Math.ceil(quoteText.trim().length / CHARS_PER_LINE))
      blocks.push({ type: 'quote', lines: wrapped, shortLast: true })
      continue
    }

    // List — collapse contiguous list items into one block
    if (/^([-*+]|\d+\.)\s+/.test(trimmed)) {
      let listItems = 0
      while (i < lines.length) {
        const t = lines[i].trim()
        if (!t) break
        if (!/^([-*+]|\d+\.)\s+/.test(t)) break
        listItems++
        i++
      }
      blocks.push({ type: 'list', lines: listItems })
      continue
    }

    // Standalone image (line that's only `![…](…)`)
    if (/^!\[[^\]]*\]\([^)]+\)\s*$/.test(trimmed)) {
      blocks.push({ type: 'image', lines: 1 })
      i++
      continue
    }

    // Paragraph — accumulate until blank line or block-starting line.
    let para = trimmed
    let j = i + 1
    while (j < lines.length) {
      const next = lines[j].trim()
      if (!next) break
      if (/^#{1,6}\s/.test(next) || /^([-*_])\1\1+\s*$/.test(next)) break
      if (next.startsWith('```') || next.startsWith('~~~')) break
      if (next.startsWith('>')) break
      if (/^([-*+]|\d+\.)\s+/.test(next)) break
      para += ' ' + next
      j++
    }
    const wrappedLines = Math.max(1, Math.ceil(para.length / CHARS_PER_LINE))
    blocks.push({ type: 'paragraph', lines: wrappedLines, shortLast: true })
    i = j
  }
  return blocks
}

// Secondary cache keyed by file path ONLY (no mtime). The full render cache
// invalidates on every edit, but the structural outline is usually still close
// to the previous version — close enough for a skeleton.
export const skeletonCache = new Map<string, SkeletonBlock[]>()
const SKELETON_CACHE_MAX = 50
export function cacheSkeleton(path: string, blocks: SkeletonBlock[]) {
  if (skeletonCache.size >= SKELETON_CACHE_MAX && !skeletonCache.has(path)) {
    const first = skeletonCache.keys().next().value
    if (first !== undefined) skeletonCache.delete(first)
  }
  skeletonCache.delete(path)
  skeletonCache.set(path, blocks)
}

// Populate the skeleton caches OFF the click→paint critical path. parseSkeleton
// regex-walks the entire post body and its output only feeds the loading
// skeleton of some future load — so running it before the paint the user is
// waiting on was pure latency. Defer to idle, then patch the render-cache entry
// so subsequent exact hits get an accurate skeleton.
export function scheduleSkeletonCache(key: string, path: string, stripped: string) {
  const run = () => {
    const skeleton = parseSkeleton(stripped)
    cacheSkeleton(path, skeleton)
    const entry = renderCache.get(key)
    if (entry) entry.skeleton = skeleton
  }
  if (typeof (window as any).requestIdleCallback === 'function') {
    ;(window as any).requestIdleCallback(run, { timeout: 500 })
  } else {
    setTimeout(run, 0)
  }
}
