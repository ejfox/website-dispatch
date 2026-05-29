<script setup lang="ts">
import { ref, watch, computed, nextTick, markRaw, onMounted } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { PhCheckCircle, PhLinkSimple, PhImageSquare, PhTrophy, PhCaretDown } from '@phosphor-icons/vue'
import LintReceipt from './LintReceipt.vue'
import LocalMediaFixer from './LocalMediaFixer.vue'
import BacklinksGraph from './BacklinksGraph.vue'
import AltTextReviewer from './AltTextReviewer.vue'
import SyndicationWizard from './SyndicationWizard.vue'
import OgImagePicker from './OgImagePicker.vue'
import StatusBanner from './StatusBanner.vue'
import MetadataPanel from './MetadataPanel.vue'
import ActionToolbar from './ActionToolbar.vue'
import PublishConfirmModal from './PublishConfirmModal.vue'
import WebmentionStatus from './WebmentionStatus.vue'
import ResizeHandle from './ResizeHandle.vue'
import { useResizable } from '../composables/useResizable'
import { unified } from 'unified'
import remarkParse from 'remark-parse'
import remarkGfm from 'remark-gfm'
import remarkRehype from 'remark-rehype'
import rehypeRaw from 'rehype-raw'
import rehypeStringify from 'rehype-stringify'
import { remarkMermaid } from '../utils/remarkMermaid'
import { remarkObsidianWikilinks } from '../utils/remarkObsidianWikilinks'
import { renderMermaidIn } from '../utils/mermaidRenderer'
import { Menu, MenuItem, PredefinedMenuItem } from '@tauri-apps/api/menu'
import { useLocalStorage } from '@vueuse/core'
import type { MarkdownFile, Backlink, LocalMediaRef, PostAnalytics } from '../types'
import { useTagSuggestions } from '../composables/useTagSuggestions'
import { usePublishing } from '../composables/usePublishing'
import { usePostActions } from '../composables/usePostActions'
import { useAppConfig } from '../composables/useAppConfig'
import { useGitStatus } from '../composables/useGitStatus'

const props = defineProps<{ file: MarkdownFile }>()
const emit = defineEmits<{ published: []; 'jump-to-path': [path: string] }>()

// Config (shared singleton)
const { appConfig, enabledEditors, publishTargets, hasMultipleTargets } = useAppConfig()
const selectedTargetId = useLocalStorage<string | null>('dispatch-target', null)
const altTextCollapsed = useLocalStorage('dispatch-alttext-collapsed', true)

// Drag the seam between the metadata stack (status banner / header / lint /
// alt-text / OG / action toolbar) and the rendered content below. Default of
// 0 means "natural sizing" — the stack flows to its content height. Once the
// user drags, we record a pixel height which becomes the cap; metadata
// scrolls within that. Double-click the divider to return to natural sizing.
const metaStackRef = ref<HTMLDivElement | null>(null)
const {
  size: metaHeight,
  dragging: metaDragging,
  start: startMetaResize,
  reset: resetMetaHeight,
} = useResizable('dispatch-preview-meta-height', {
  default: 0,
  min: 80,
  max: () => window.innerHeight - 220,
  axis: 'y',
  getStartSize: () => metaStackRef.value?.offsetHeight,
})

function getActiveTargetId(): string | undefined {
  if (!hasMultipleTargets.value) return undefined
  return selectedTargetId.value || undefined
}

const activeTargetDomain = computed(() => {
  const targets = publishTargets.value
  if (!targets.length) return ''
  const id = selectedTargetId.value
  const picked =
    (id && targets.find((t) => t.id === id)) ||
    targets.find((t) => t.is_default) ||
    targets[0]
  return picked?.domain || ''
})

// Wrapped in `markRaw` so Vue doesn't deep-proxy the unified pipeline (which
// is a big graph of plugin closures, AST schemas, and trie tables). Without
// `markRaw`, every file switch pays for re-tracking thousands of nested
// objects — measurable on long posts.
// Tiny HTML-escape used by the render-fallback fallback below. Kept inline
// so the file is self-contained for this hot path.
function escapeHtml(s: string): string {
  return s
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    .replace(/'/g, '&#39;')
}

const markdownProcessor = computed(() =>
  markRaw(
    unified()
      .use(remarkParse)
      .use(remarkGfm)
      .use(remarkObsidianWikilinks, { baseUrl: activeTargetDomain.value })
      .use(remarkMermaid)
      .use(remarkRehype, { allowDangerousHtml: true })
      .use(rehypeRaw)
      .use(rehypeStringify, { allowDangerousHtml: true }),
  ),
)

// LRU-ish cache of rendered HTML keyed on `path|modified`. Flipping between
// two posts (or any post you've recently viewed) becomes instant — no
// re-running the unified pipeline. ~80-150ms savings per cache hit on
// long posts. Invalidates automatically when the file is edited because
// the `modified` mtime changes.
//
// We also cache the parsed skeleton blocks here so the next load doesn't
// need to re-parse — useful on cold cache-key misses where the layout is
// still likely close to the cached version.
type SkeletonBlock = {
  type: 'heading' | 'paragraph' | 'code' | 'list' | 'quote' | 'image' | 'hr'
  level?: number // for headings: 1..6
  lines: number // how many wrapped lines to render
  shortLast?: boolean // last line ~50% (true for prose paragraphs)
}
type CacheEntry = { stripped: string; rendered: string; skeleton: SkeletonBlock[] }
const renderCache = new Map<string, CacheEntry>()
const RENDER_CACHE_MAX = 30
function renderCacheKey(file: { path: string; modified: number }) {
  // Include the active publish-target domain because remarkObsidianWikilinks
  // bakes it into the rendered HTML — same file rendered against a
  // different target should not share cache.
  return `${file.path}|${file.modified}|${activeTargetDomain.value}`
}
/**
 * Cheap block-level parser of markdown body text. Returns a structural
 * outline used to render an accurate loading skeleton — not a full
 * markdown parser. We only need to know "what KIND of block is here, and
 * how many lines does it roughly take." We deliberately don't use the
 * unified pipeline here because the whole point is to have something
 * before unified finishes processing.
 *
 * Wraps long paragraphs at ~70 chars/line for skeleton purposes, which
 * roughly matches the rendered prose column at default zoom.
 */
const CHARS_PER_LINE = 70
function parseSkeleton(markdown: string): SkeletonBlock[] {
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

function cacheRender(key: string, entry: CacheEntry) {
  // Evict oldest if over the cap. Map preserves insertion order, so the
  // first key is the oldest. Re-set to bump recency on hits.
  if (renderCache.size >= RENDER_CACHE_MAX && !renderCache.has(key)) {
    const first = renderCache.keys().next().value
    if (first !== undefined) renderCache.delete(first)
  }
  renderCache.delete(key) // re-insert to mark as most-recent
  renderCache.set(key, entry)
}

/**
 * Secondary cache keyed by file path ONLY (no mtime). The full render
 * cache invalidates on every edit, but the structural outline is usually
 * still close to the previous version — close enough for a skeleton.
 * This lets the second-visit-after-edit show an accurate-shaped skeleton
 * instead of falling back to generic random widths.
 */
const skeletonCache = new Map<string, SkeletonBlock[]>()
const SKELETON_CACHE_MAX = 50
function cacheSkeleton(path: string, blocks: SkeletonBlock[]) {
  if (skeletonCache.size >= SKELETON_CACHE_MAX && !skeletonCache.has(path)) {
    const first = skeletonCache.keys().next().value
    if (first !== undefined) skeletonCache.delete(first)
  }
  skeletonCache.delete(path)
  skeletonCache.set(path, blocks)
}

function selectTarget(id: string) {
  selectedTargetId.value = id
}

const content = ref('')
const renderedContent = ref('')
// Surface render failures distinctly from the silent-empty state. When set,
// the preview pane shows a tight error card with the message and the file
// path — beats "looks broken, no idea why" by a mile.
const renderError = ref<{ stage: string; message: string } | null>(null)
const backlinks = ref<Backlink[]>([])
const loadingBacklinks = ref(false)
const obsidianConnected = ref(false)

// Git status with auto-polling (VueUse useIntervalFn handles cleanup)
const { gitStatus } = useGitStatus(10000)
const copyFeedback = ref<string | null>(null)
const localMedia = ref<LocalMediaRef[]>([])
const loadingLocalMedia = ref(false)
const showMediaFixer = ref(false)
const metadataExpanded = ref(false)

/**
 * Loading-state machinery for the preview pane. Three principles:
 *
 *   1. Don't flash a skeleton on fast loads — wait ~120ms before showing
 *      anything. The eye reads ≤100ms as "instant", so a skeleton that
 *      appears for 50ms is just noise.
 *   2. Cross-fade between skeleton ↔ rendered content via Vue Transition,
 *      not a hard swap.
 *   3. Skeleton shape varies per post (word count → number of lines) so
 *      switching files doesn't show the same fake outline every time.
 */
const previewLoading = ref(false)
const showSkeleton = ref(false)
const showLoadingIndicator = ref(false)
let skeletonTimer: number | null = null
let progressTimer: number | null = null

function beginPreviewLoad() {
  previewLoading.value = true
  if (skeletonTimer !== null) window.clearTimeout(skeletonTimer)
  if (progressTimer !== null) window.clearTimeout(progressTimer)
  showSkeleton.value = false
  showLoadingIndicator.value = false
  // 120ms: long enough to skip the flash on fast cache-miss renders,
  // short enough that slower posts don't feel frozen.
  skeletonTimer = window.setTimeout(() => {
    if (previewLoading.value) showSkeleton.value = true
  }, 120)
  // The top progress bar shows slightly earlier (60ms) because it's
  // unobtrusive — a thin sliver at the top of the pane.
  progressTimer = window.setTimeout(() => {
    if (previewLoading.value) showLoadingIndicator.value = true
  }, 60)
}

function endPreviewLoad() {
  previewLoading.value = false
  showSkeleton.value = false
  showLoadingIndicator.value = false
  if (skeletonTimer !== null) {
    window.clearTimeout(skeletonTimer)
    skeletonTimer = null
  }
  if (progressTimer !== null) {
    window.clearTimeout(progressTimer)
    progressTimer = null
  }
}

/**
 * Block-level skeleton model. Derived from (in priority order):
 *   1. The full render-cache entry (always 100% accurate — same content)
 *   2. The path-only skeleton cache (last known structure — close after
 *      small edits, drifts after big rewrites)
 *   3. A live parse of `content.value` if it's already arrived but
 *      markdown is still processing
 *   4. A generic word-count-derived fallback for true first visits
 *
 * Tier 4 is the only one that's not structurally accurate — and it only
 * runs once per file. After the first render, the structure is cached
 * forever for that path.
 */
const skeletonBlocks = computed<SkeletonBlock[]>(() => {
  // Tier 1: exact cache hit
  const exact = renderCache.get(renderCacheKey(props.file))
  if (exact) return exact.skeleton

  // Tier 2: path-only skeleton cache (any prior visit, mtime irrelevant)
  const recent = skeletonCache.get(props.file.path)
  if (recent && recent.length) return recent

  // Tier 3: content already arrived (IPC done, markdown still processing)
  if (content.value) return parseSkeleton(content.value)

  // Tier 4: first visit, no content yet — synthesize a believable shape
  // from the file's word_count metadata so layout shift is minimal.
  return synthesizeSkeleton(props.file.word_count || 600, props.file.path)
})

/**
 * Generate a plausible-looking outline when we have nothing but a word
 * count. Mixes headings + paragraphs in a realistic blog-post rhythm.
 * Deterministic per path so the same file always shows the same shape.
 */
function synthesizeSkeleton(words: number, path: string): SkeletonBlock[] {
  const seed = hashCode(path)
  const rand = (i: number) => ((seed + i * 2654435761) >>> 0) / 0xffffffff
  const blocks: SkeletonBlock[] = [
    { type: 'heading', level: 1, lines: 1 },
  ]
  let remaining = words
  let i = 0
  while (remaining > 0) {
    // Insert a subhead every ~3-4 paragraphs
    if (blocks.length > 2 && i % 4 === 3) {
      blocks.push({ type: 'heading', level: 2, lines: 1 })
    }
    // Paragraph length varies 60-180 words
    const paraWords = Math.min(remaining, Math.round(60 + rand(i) * 120))
    const paraChars = paraWords * 5.5 // rough chars-per-word
    const paraLines = Math.max(1, Math.ceil(paraChars / CHARS_PER_LINE))
    blocks.push({ type: 'paragraph', lines: paraLines, shortLast: true })
    remaining -= paraWords
    i++
    if (i > 20) break // cap synthetic skeletons
  }
  return blocks
}

function hashCode(s: string): number {
  let h = 0
  for (let i = 0; i < s.length; i++) {
    h = (h << 5) - h + s.charCodeAt(i)
    h |= 0
  }
  return h
}

/**
 * Width for a non-final wrapped line in a paragraph/list/quote — varies
 * subtly per block+line so the skeleton doesn't look like a fake bar
 * chart. Stable across re-renders (deterministic from path + indices).
 */
function skelLineWidth(blockIdx: number, lineIdx: number, block: SkeletonBlock): number {
  const seed = hashCode(props.file.path) + blockIdx * 7919 + lineIdx * 131
  const r = ((seed >>> 0) % 14) - 7 // [-7, +6]
  if (block.type === 'list') return Math.min(85, 55 + r) // list items are shorter
  if (block.type === 'code') return Math.min(95, 60 + r) // code wraps unevenly
  return Math.min(99, 93 + r)
}

/** Last-line width for prose paragraphs — looks natural at 35-60%. */
function skelLastLineWidth(blockIdx: number): number {
  const seed = hashCode(props.file.path) + blockIdx * 7919
  return 35 + ((seed >>> 0) % 26) // [35, 60]
}

// Image / video breakdown of localMedia — surfaced to AltTextReviewer
// so its empty state can give a real next-step ("upload N images") instead
// of a dead-end "0 images found" message.
const localImageCount = computed(() => localMedia.value.filter((m) => m.media_type !== 'video').length)
const localVideoCount = computed(() => localMedia.value.filter((m) => m.media_type === 'video').length)

// Derive the upload folder (Cloudinary path / R2 prefix) from the post's
// vault location. Mirrors the logic in App.vue's drag-drop handler so
// screenshots dropped onto Dispatch and screenshots fixed via the
// LocalMediaFixer modal land in the same place.
const mediaUploadFolder = computed(() => {
  const sd = props.file.source_dir
  if (!sd) return 'blog'
  const m = sd.match(/(\d{4})/)
  return m ? `blog/${m[1]}` : sd
})

// AltTextReviewer's empty state offers "Upload to Cloudinary" — wire that
// to close the Describe modal and open the uploader in one motion.
function onOpenLocalFixerFromAltText() {
  showAltTextReviewer.value = false
  showMediaFixer.value = true
}

// LocalMediaFixer's success state offers "Now describe images" — wire that
// to close the uploader and open the Describe modal so the second step is
// one click away.
function onOpenAltTextFromFixer() {
  showMediaFixer.value = false
  showAltTextReviewer.value = true
}

// Tell the parent something changed so it can refetch the post (the
// markdown source was rewritten and Cloudinary URLs are now in place).
function onLocalMediaFixed() {
  emit('published')
}

function showCopyFeedback(msg: string) {
  copyFeedback.value = msg
  setTimeout(() => {
    copyFeedback.value = null
  }, 2000)
}

// Publishing composable
const {
  publishing,
  justPublished,
  justPublishedGlow,
  showSuccess,
  successMessage,
  isMilestoneToast,
  showPublishConfirm,
  publishConfirmRepublish,
  showSyndicationWizard,
  showAltTextReviewer,
  publishContext,
  openPublishConfirm,
  closePublishConfirm,
  publish,
  publishUnlisted,
  showSuccessToast,
  onSyndicationQueued,
  onAltTextApplied,
} = usePublishing({
  getSlug: () => slug.value,
  getFilePath: () => props.file.path,
  getFileIsSafe: () => props.file.is_safe,
  getActiveTargetId,
  isPasswordProtected: () => isPasswordProtected.value,
  isUnlisted: () => isUnlisted.value,
  onPublished: () => emit('published'),
  // Auto-fire webmentions after publish/republish. Bridgy Fed forwarding
  // is opt-in via Settings → Connections (default off).
  onPublishSuccess: () => {
    const bridgyFed = appConfig.value?.webmentions_bridgy_fed === true
    autoTriggerOnPublish(bridgyFed)
  },
})

// Tag suggestions composable
const { availableTags, suggestedTags, addingTag, fetchAvailableTags, analyzeTags, addTag } = useTagSuggestions({
  getFilePath: () => props.file.path,
  getFileTags: () => props.file.tags,
  onFeedback: showCopyFeedback,
  onRefresh: () => emit('published'),
})

async function showBacklinkMenu(link: Backlink, e: MouseEvent) {
  e.preventDefault()
  const menu = await Menu.new({
    items: [
      await MenuItem.new({
        text: 'Open in Obsidian',
        action: () => invoke('open_in_obsidian', { path: link.path }),
      }),
      await MenuItem.new({
        text: 'Reveal in Finder',
        action: () => invoke('open_in_app', { path: link.path, app: 'Finder' }).catch(() => {}),
      }),
      await PredefinedMenuItem.new({ item: 'Separator' }),
      await MenuItem.new({
        text: 'Copy Path',
        action: () => navigator.clipboard.writeText(link.path),
      }),
      await MenuItem.new({
        text: 'Copy Title',
        action: () => navigator.clipboard.writeText(link.title || link.path),
      }),
    ],
  })
  await menu.popup()
}

// Re-render markdown if the active publish target (and thus base URL) changes
watch(activeTargetDomain, async () => {
  if (!content.value) return
  try {
    const result = await markdownProcessor.value.process(content.value)
    renderedContent.value = String(result)
    nextTick(() => renderMermaidIn(document))
  } catch {
    /* leave stale render */
  }
})

// Pulled out of `watch` into an explicit function so we can call it from
// both onMounted and the prop watcher. Vue's `watch(...,{immediate:true})`
// has been unreliable here — on certain HMR + mount-order combinations
// the immediate callback wouldn't fire and the pane would stay blank
// with the watcher silently never running. Belt-and-suspenders: fire it
// explicitly on mount AND on every props.file change.
async function loadFileContent(file: MarkdownFile | null) {
    console.log('[render] loadFileContent for', file?.path ?? '<no file>')
    if (!file) return

    // ── Perf instrumentation ──────────────────────────────────────────
    // Phased timing so we can see where a slow file-switch is actually
    // slow. T0=switch fired, T1=content IPC returned, T2=markdown
    // processed, T3=DOM painted. Logged via console (Tauri dev surfaces
    // webview console output to the terminal). Rust's get_file_content
    // logs its own slice with the same [perf] prefix.
    const switchT0 = performance.now()
    const switchName = file.filename || file.path
    const perf = (phase: string, fromT0 = switchT0) => {
      const ms = (performance.now() - fromT0).toFixed(1)
      console.log(`[perf] file-switch ${phase} ${ms}ms · ${switchName}`)
    }

    // Reset refs synchronously so the next paint shows a clean slate
    // before any IPC round-trips return. Without this you briefly see
    // the OLD post's analytics / backlinks under the NEW post's metadata.
    justPublished.value = null
    backlinks.value = []
    localMedia.value = []
    postStats.value = null
    pageviewSeries.value = []
    showMediaFixer.value = false
    webmentionReport.value = null
    content.value = ''
    renderedContent.value = ''
    renderError.value = null
    loadingBacklinks.value = true
    loadingLocalMedia.value = true
    loadingStats.value = !!file.published_url
    beginPreviewLoad()

    // Fire-and-forget: the preview server is on localhost; the frontend
    // talks to it directly. (We used to also `invoke('set_preview_file')`
    // which round-tripped through Tauri's IPC and then made the SAME HTTP
    // call on the Rust side via `reqwest::blocking` — pure duplicate work
    // that hogged an IPC worker thread on every file switch.)
    fetch('http://127.0.0.1:6419/set-file', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ path: file.path }),
    }).catch(() => {})

    // Cache hit short-circuits everything below: zero IPC for content,
    // zero markdown processing. Flipping between recently-viewed posts
    // feels instant.
    const cacheKey = renderCacheKey(file)
    const cached = renderCache.get(cacheKey)
    // Treat empty cached renders as a miss — these are the "frequently
    // broken" symptom: once an empty render got cached, every subsequent
    // visit hit the cache and silently showed nothing. Re-renders are
    // cheap relative to permanently-broken UX.
    if (cached && cached.rendered && cached.rendered.trim()) {
      content.value = cached.stripped
      renderedContent.value = cached.rendered
      perf('cache-hit TOTAL')
      // Cache hit is synchronous → no skeleton flash should appear.
      endPreviewLoad()
      // Mermaid blocks in cached HTML may not have been re-processed if
      // the viewer unmounted between renders. The renderer skips blocks
      // already marked processed, so this is cheap.
      nextTick(() => renderMermaidIn(document))
      void (async () => {
        await fetchAvailableTags()
        suggestedTags.value = analyzeTags(cached.stripped, file.tags || [])
      })()
    } else {
      // Cache miss — kick off content + markdown. The other IPCs below
      // (backlinks, local media, analytics, pageviews) fire in parallel
      // regardless of cache state.
      //
      // The `file.path === props.file.path` guards prevent a stale render:
      // if the user clicks post A then quickly clicks post B, the in-flight
      // markdown processor for A would otherwise overwrite B's content
      // when it finally resolves. Each step bails out if the active file
      // has changed since this watcher run started.
      invoke('get_file_content', { path: file.path })
        .then(async (raw) => {
          if (file.path !== props.file.path) return
          perf('content-ipc done')
          const stripped = (raw as string).replace(/^---\n[\s\S]*?\n---\n*/, '')
          content.value = stripped
          // Real-world guard: a fully empty file (or one that's only
          // frontmatter) used to render blank with no signal. Surface it.
          if (!stripped.trim()) {
            renderedContent.value = ''
            renderError.value = {
              stage: 'empty',
              message:
                'File has no body content — only frontmatter (or empty).',
            }
            endPreviewLoad()
            return
          }
          await nextTick()
          if (file.path !== props.file.path) return
          try {
            const markdownT0 = performance.now()
            const result = await markdownProcessor.value.process(stripped)
            if (file.path !== props.file.path) return
            const rendered = String(result)
            // Guard: never cache an empty render — that's how the pane
            // started returning empty on every subsequent visit. Show the
            // raw content as a fallback so the user gets *something*
            // instead of a blank pane.
            if (!rendered.trim()) {
              console.warn(
                '[render] markdown processor produced empty output for',
                file.path,
                '(', stripped.length, 'chars in)',
              )
              renderedContent.value =
                `<pre class="render-fallback">${escapeHtml(stripped)}</pre>`
              renderError.value = {
                stage: 'empty-render',
                message:
                  'Markdown processor produced no output — showing raw text. Check console for plugin errors.',
              }
              endPreviewLoad()
              return
            }
            renderedContent.value = rendered
            console.log(
              `[perf] file-switch markdown-only ${(performance.now() - markdownT0).toFixed(1)}ms · ${switchName}`,
            )
            perf('TOTAL (rendered)')
            const skeleton = parseSkeleton(stripped)
            cacheRender(cacheKey, { stripped, rendered, skeleton })
            cacheSkeleton(file.path, skeleton)
            endPreviewLoad()
            nextTick(() => renderMermaidIn(document))
          } catch (err) {
            if (file.path !== props.file.path) return
            console.error('[render] markdown processor threw', err, 'on', file.path)
            // Show the raw content with the error so the user can still
            // see what they wrote AND know what went wrong.
            renderedContent.value =
              `<pre class="render-fallback">${escapeHtml(stripped)}</pre>`
            renderError.value = {
              stage: 'process',
              message: err instanceof Error ? err.message : String(err),
            }
            endPreviewLoad()
          }
          await fetchAvailableTags()
          if (file.path !== props.file.path) return
          suggestedTags.value = analyzeTags(stripped, file.tags || [])
        })
        .catch((e) => {
          if (file.path !== props.file.path) return
          console.error('[render] get_file_content failed', e, 'on', file.path)
          content.value = `Error: ${e}`
          renderedContent.value = ''
          renderError.value = {
            stage: 'read',
            message: e instanceof Error ? e.message : String(e),
          }
          endPreviewLoad()
        })
    }

    invoke('get_backlinks', { filename: file.filename })
      .then((res) => {
        if (file.path !== props.file.path) return
        backlinks.value = res as Backlink[]
      })
      .catch((e) => console.log('Backlinks unavailable:', e))
      .finally(() => {
        if (file.path === props.file.path) loadingBacklinks.value = false
      })

    invoke('get_local_media', { path: file.path })
      .then((res) => {
        if (file.path !== props.file.path) return
        localMedia.value = res as LocalMediaRef[]
      })
      .catch((e) => console.log('Local media detection unavailable:', e))
      .finally(() => {
        if (file.path === props.file.path) loadingLocalMedia.value = false
      })

    if (file.published_url) {
      invoke('get_post_analytics', { url: file.published_url, days: 30 })
        .then((res) => {
          if (file.path !== props.file.path) return
          postStats.value = res as PostAnalytics
        })
        .catch(() => {
          if (file.path === props.file.path) postStats.value = null
        })
        .finally(() => {
          if (file.path === props.file.path) loadingStats.value = false
        })
      invoke('get_post_pageview_series', { url: file.published_url, days: 30 })
        .then((res) => {
          if (file.path !== props.file.path) return
          pageviewSeries.value = (res as number[]) || []
        })
        .catch(() => {
          if (file.path === props.file.path) pageviewSeries.value = []
        })
    }
}

// Two triggers — both call the same loader so we can't miss the initial
// render no matter the mount/HMR sequence:
//   - watch fires on every subsequent props.file change
//   - onMounted guarantees the first load happens after mount even if
//     the watch's immediate behavior misbehaved (which it has been doing)
watch(() => props.file, (file) => loadFileContent(file))
onMounted(() => loadFileContent(props.file))

// Check Obsidian API status on mount
invoke('check_obsidian_api').then((connected: unknown) => {
  obsidianConnected.value = connected as boolean
})

// Format filename into title, handling date-based names specially
const formatTitle = (filename: string): string => {
  const baseName = filename.replace(/\.md$/, '')
  const datePattern = /^(\d{4}-\d{2}-\d{2})(-.*)?$/
  const dateMatch = baseName.match(datePattern)
  if (dateMatch) {
    const datePart = dateMatch[1]
    const suffix = dateMatch[2]
    if (suffix) {
      const suffixTitle = suffix
        .slice(1)
        .split('-')
        .map((w) => w.charAt(0).toUpperCase() + w.slice(1))
        .join(' ')
      return `${datePart} ${suffixTitle}`
    }
    return datePart
  }
  return baseName.replace(/-/g, ' ').replace(/\b\w/g, (c) => c.toUpperCase())
}

const title = computed(() => props.file.title || formatTitle(props.file.filename))
const titleIsDerived = computed(() => !props.file.title)

// Extract <year>/<slug> from the file's path. website2 organizes processed
// posts as content/processed/<year>/<slug>.json, so OG generation needs the
// year prefix to find the JSON. Returns empty string for anything outside
// blog/ (e.g. week-notes, drafts) so consumers like OgImagePicker can opt
// out via `v-if="slug"` instead of triggering ENOENT on a non-blog file.
const slug = computed(() => {
  const baseName = props.file.filename.replace('.md', '')
  const yearMatch = props.file.path.match(/\/blog\/(\d{4})\//)
  if (yearMatch) return `${yearMatch[1]}/${baseName}`
  return props.file.path.includes('/blog/') ? baseName : ''
})

const targetUrl = computed(() => {
  const targets = publishTargets.value
  const target = targets.find((t) => t.id === selectedTargetId.value) || targets.find((t) => t.is_default) || targets[0]
  const domain = target
    ? (appConfig.value as any)?.publish_targets
        ?.find((t: any) => t.id === target.id)
        ?.domain?.replace(/^https?:\/\//, '') || 'ejfox.com'
    : 'ejfox.com'
  // slug already includes year (e.g. "2013/the-magazine-..."), so just append.
  return `${domain}/blog/${slug.value}`
})

const isLive = computed(() => !!props.file.published_url || !!justPublished.value)
const liveUrl = computed(() => props.file.published_url || justPublished.value)
const hasUnpublishedChanges = computed(() => props.file.warnings.includes('Modified since publish'))
const lintWarnings = computed(() => props.file.warnings.filter((w) => w !== 'Modified since publish'))

// Alt text detection
const missingAltTextCount = computed(() => {
  const w = props.file.warnings.find((w) => w.startsWith('Missing alt text'))
  if (!w) return 0
  const match = w.match(/\((\d+)\)/)
  return match ? parseInt(match[1]) : 0
})

// Visibility states
const isUnlisted = computed(() => props.file.unlisted || !!props.file.password)
const isPasswordProtected = computed(() => !!props.file.password)
const visibilityLabel = computed(() => {
  if (isPasswordProtected.value) return 'PASSWORD'
  if (isUnlisted.value) return 'UNLISTED'
  return null
})

// Post actions composable (crown, webmentions, unpublish)
const {
  sendingWebmentions,
  webmentionReport,
  isCrowned,
  crowning,
  unpublishing,
  crownPost,
  triggerWebmentions,
  autoTriggerOnPublish,
  unpublish,
} = usePostActions({
  slug,
  getLiveUrl: () => liveUrl.value,
  isLive: () => isLive.value,
  getActiveTargetId,
  showSuccessToast,
  onRefresh: () => emit('published'),
})

// Analytics
const postStats = ref<PostAnalytics | null>(null)
const loadingStats = ref(false)
const pageviewSeries = ref<number[]>([])

// Derived stats — Umami gives us pageviews/visitors/visits/bounces/totaltime;
// the more interesting numbers are time-on-page and bounce rate.
const avgTimeOnPage = computed(() => {
  const s = postStats.value
  if (!s || s.visits === 0 || s.totaltime === 0) return null
  return Math.round(s.totaltime / s.visits) // seconds
})
const bounceRate = computed(() => {
  const s = postStats.value
  if (!s || s.visits === 0) return null
  return s.bounces / s.visits
})
const fmtDuration = (sec: number) => {
  if (sec < 60) return `${sec}s`
  const m = Math.floor(sec / 60)
  const s = sec % 60
  return s ? `${m}m ${s}s` : `${m}m`
}
const fmtCount = (n: number) =>
  n >= 1000 ? `${(n / 1000).toFixed(1)}k` : String(n)
const sparkPath = computed(() => {
  const pts = pageviewSeries.value
  if (pts.length < 2) return ''
  const W = 120
  const H = 22
  const max = Math.max(...pts, 1)
  return pts
    .map((v, i) => {
      const x = (i / (pts.length - 1)) * W
      const y = H - (v / max) * H
      return `${i === 0 ? 'M' : 'L'}${x.toFixed(1)} ${y.toFixed(1)}`
    })
    .join(' ')
})

// Scheduling
const showSchedulePicker = ref(false)
const scheduleDate = ref('')

const isScheduled = computed(() => !!props.file.publish_at && !isLive.value)

async function schedulePublish() {
  if (!scheduleDate.value) return
  const isoDate = new Date(scheduleDate.value).toISOString()
  try {
    await invoke('schedule_publish', { path: props.file.path, publishAt: isoDate })
    showSchedulePicker.value = false
    scheduleDate.value = ''
    showCopyFeedback('Scheduled!')
    emit('published')
  } catch (e) {
    alert(`Schedule failed: ${e}`)
  }
}

async function cancelSchedule() {
  try {
    await invoke('cancel_schedule', { path: props.file.path })
    showCopyFeedback('Schedule cancelled')
    emit('published')
  } catch (e) {
    alert(`Cancel failed: ${e}`)
  }
}

function copyUrl() {
  if (liveUrl.value) {
    navigator.clipboard.writeText(liveUrl.value)
    showCopyFeedback('Copied!')
  }
}

function copyUrlAndPassword() {
  if (liveUrl.value && props.file.password) {
    const text = `Here's the draft: ${liveUrl.value}\nPassword: ${props.file.password}`
    navigator.clipboard.writeText(text)
    showCopyFeedback('Copied with password!')
  }
}

// Expose methods for parent component
defineExpose({ openPublishConfirm })

async function openInObsidian() {
  await invoke('open_in_obsidian', { path: props.file.path })
}

async function openInEditor(appName: string) {
  await invoke('open_in_app', { path: props.file.path, app: appName })
}

async function openPreview() {
  await invoke('open_preview')
}
</script>

<template>
  <div class="panel" :class="{ live: isLive, 'just-published': justPublishedGlow }">
    <!-- Success Toast -->
    <Transition name="toast">
      <div v-if="showSuccess" class="success-toast" :class="{ milestone: isMilestoneToast }">
        <PhCheckCircle v-if="!isMilestoneToast" :size="13" weight="fill" />
        <PhTrophy v-else :size="15" weight="fill" />
        <span>{{ successMessage }}</span>
      </div>
    </Transition>

    <!-- Copy Feedback -->
    <Transition name="fade">
      <div v-if="copyFeedback" class="copy-feedback">
        {{ copyFeedback }}
      </div>
    </Transition>

    <!-- Resizable metadata stack. Default height is 0 (natural sizing — flows
         to content); once the user drags the divider it becomes a fixed cap
         with internal scroll. Double-click the divider to reset to natural. -->
    <div
      ref="metaStackRef"
      class="metadata-stack"
      :class="{ sized: metaHeight > 0, resizing: metaDragging }"
      :style="metaHeight > 0 ? { height: metaHeight + 'px' } : {}"
    >
    <!-- Status Banner -->
    <StatusBanner
      :is-live="isLive"
      :is-scheduled="isScheduled"
      :is-unlisted="isUnlisted"
      :is-password-protected="isPasswordProtected"
      :has-unpublished-changes="hasUnpublishedChanges"
      :is-safe="file.is_safe"
      :warnings="file.warnings"
      :live-url="liveUrl"
      :publish-at="file.publish_at"
      :visibility-label="visibilityLabel"
      :publishing="publishing"
      :file-path="file.path"
      @copy-url="copyUrl"
      @copy-url-password="copyUrlAndPassword"
      @republish="publish(true)"
      @cancel-schedule="cancelSchedule"
    />

    <!-- Webmention status — auto-fires after publish/republish, surfaced
         inline so the user sees an outcome without an extra click. -->
    <WebmentionStatus
      v-if="isLive && (sendingWebmentions || webmentionReport)"
      :report="webmentionReport"
      :sending="sendingWebmentions"
      @resend="triggerWebmentions({ bridgyFed: appConfig?.webmentions_bridgy_fed === true, force: true })"
    />

    <!-- Header -->
    <div class="header">
      <template v-if="titleIsDerived && file.dek">
        <h1>{{ file.dek }}</h1>
        <p class="title-hint">{{ slug }}</p>
      </template>
      <template v-else>
        <h1 :class="{ 'derived-title': titleIsDerived }">{{ title }}</h1>
        <p v-if="titleIsDerived" class="title-hint">Title derived from filename</p>
        <p v-if="file.dek" class="dek">{{ file.dek }}</p>
      </template>
    </div>

    <!-- Analytics strip — visible up top whenever the post is live. Shows
         pageviews, visitors, avg time on page, bounce rate, and a 30-day
         sparkline of daily views. -->
    <div v-if="isLive && (postStats || loadingStats)" class="analytics-strip">
      <template v-if="loadingStats">
        <span class="muted">analytics…</span>
      </template>
      <template v-else-if="postStats">
        <span class="stat-big">
          <strong>{{ fmtCount(postStats.pageviews) }}</strong>
          <span class="stat-unit">views</span>
        </span>
        <span v-if="postStats.visitors" class="stat">
          <strong>{{ fmtCount(postStats.visitors) }}</strong>
          <span class="stat-unit">visitors</span>
        </span>
        <span v-if="avgTimeOnPage" class="stat">
          <strong>{{ fmtDuration(avgTimeOnPage) }}</strong>
          <span class="stat-unit">avg</span>
        </span>
        <span v-if="bounceRate !== null" class="stat" :class="{ warn: bounceRate > 0.7 }">
          <strong>{{ Math.round(bounceRate * 100) }}%</strong>
          <span class="stat-unit">bounce</span>
        </span>
        <svg
          v-if="sparkPath"
          class="sparkline"
          viewBox="0 0 120 22"
          preserveAspectRatio="none"
          :data-tip="`${pageviewSeries.length}-day daily pageviews`"
        >
          <path :d="sparkPath" fill="none" stroke="currentColor" stroke-width="1.25" />
        </svg>
        <span class="stat-period">last 30d</span>
      </template>
    </div>

    <!-- Info / Metadata -->
    <MetadataPanel
      :file="file"
      :obsidian-connected="obsidianConnected"
      :git-status="gitStatus"
      :post-stats="postStats"
      :loading-stats="loadingStats"
      :suggested-tags="suggestedTags"
      :available-tags="availableTags"
      :adding-tag="addingTag"
      :metadata-expanded="metadataExpanded"
      :has-unpublished-changes="hasUnpublishedChanges"
      :is-unlisted="isUnlisted"
      :is-password-protected="isPasswordProtected"
      @toggle-metadata="metadataExpanded = !metadataExpanded"
      @add-tag="addTag"
    />

    <!-- Lint Receipt (only when warnings exist) -->
    <LintReceipt :warnings="lintWarnings" />

    <!-- Media health: Local upload (prerequisite) → Alt text (depends on URLs).
         Order matters — Local Media must come before Alt Text because alt text
         generation needs publicly-hosted URLs. The cross-references in the
         hint copy spell this out so users don't dead-end in the Describe modal. -->

    <!-- Local Media (Step 1 of media flow) -->
    <div v-if="localImageCount > 0 || localVideoCount > 0" class="media-section">
      <div class="media-section-header">
        <span class="label">
          <PhImageSquare :size="10" weight="duotone" />
          Local Media
          <span v-if="missingAltTextCount > 0" class="step-pill">step 1 of 2</span>
        </span>
        <span class="count" :class="{ warning: localImageCount > 0 }">{{ localImageCount + localVideoCount }}</span>
        <button v-if="localImageCount > 0" @click.stop="showMediaFixer = true" class="section-btn primary">
          Upload to Cloudinary
        </button>
      </div>
      <div class="media-section-hint">
        <template v-if="localImageCount > 0 && localVideoCount > 0">
          {{ localImageCount }} image{{ localImageCount === 1 ? '' : 's' }} +
          {{ localVideoCount }} video{{ localVideoCount === 1 ? '' : 's' }} live in your vault.
          Uploading rewrites the markdown refs to public URLs.
        </template>
        <template v-else-if="localImageCount > 0">
          {{ localImageCount }} image{{ localImageCount === 1 ? '' : 's' }} live in your vault.
          Uploading rewrites the markdown refs to public URLs.
        </template>
        <template v-else>
          {{ localVideoCount }} video{{ localVideoCount === 1 ? '' : 's' }} in your vault — videos
          stay local; nothing to upload here.
        </template>
      </div>
    </div>

    <!-- Alt Text (Step 2 of media flow) -->
    <div v-if="missingAltTextCount > 0" class="media-section">
      <div class="media-section-header">
        <button class="section-toggle" @click="altTextCollapsed = !altTextCollapsed" :aria-expanded="!altTextCollapsed">
          <PhCaretDown :size="9" weight="bold" class="caret" :class="{ collapsed: altTextCollapsed }" />
          <span class="label">
            <PhImageSquare :size="10" weight="duotone" />
            Alt Text
            <span v-if="localImageCount > 0" class="step-pill">step 2 of 2</span>
          </span>
          <span class="count" :class="{ warning: localImageCount === 0 }">{{ missingAltTextCount }}</span>
        </button>
        <button
          @click.stop="showAltTextReviewer = true"
          class="section-btn"
          :class="localImageCount > 0 ? 'ghost' : 'primary'"
          :title="localImageCount > 0 ? 'Open describer (will prompt to upload local images first)' : ''"
        >
          Describe
        </button>
      </div>
      <div v-if="!altTextCollapsed" class="media-section-hint">
        <template v-if="localImageCount > 0">
          Local images need a public URL before they can be described —
          run <strong>Upload to Cloudinary</strong> above first.
        </template>
        <template v-else>
          {{ missingAltTextCount }} image{{ missingAltTextCount === 1 ? '' : 's' }} ready to
          describe with AI.
        </template>
      </div>
    </div>

    <!-- Alt Text Reviewer Modal -->
    <AltTextReviewer
      v-if="showAltTextReviewer"
      :file-path="file.path"
      :count="missingAltTextCount"
      :local-image-count="localImageCount"
      :local-video-count="localVideoCount"
      @close="showAltTextReviewer = false"
      @applied="onAltTextApplied"
      @open-local-fixer="onOpenLocalFixerFromAltText"
    />

    <!-- Backlinks -->
    <div v-if="backlinks.length || loadingBacklinks" class="backlinks">
      <div class="backlinks-header">
        <span class="label">
          <PhLinkSimple :size="10" weight="duotone" />
          Backlinks
        </span>
        <span class="count">{{ loadingBacklinks ? '...' : backlinks.length }}</span>
      </div>
      <div v-if="loadingBacklinks" class="backlinks-loading">Loading...</div>
      <template v-else>
        <BacklinksGraph
          v-if="backlinks.length"
          :backlinks="backlinks"
          :current-title="props.file.title || ''"
          @select="(path: string) => emit('jump-to-path', path)"
        />
        <div class="backlinks-list">
        <div
          v-for="link in backlinks"
          :key="link.path"
          class="backlink-item"
          @contextmenu="showBacklinkMenu(link, $event)"
        >
          <span class="backlink-title">{{ link.title || link.path }}</span>
          <span v-if="link.context" class="backlink-context">{{ link.context }}</span>
        </div>
        </div>
      </template>
    </div>

    <!-- OG Image (only after publish) -->
    <!-- OG picker shows for any post with a usable slug, not just live ones —
         picking an OG before publish is a natural part of the publish flow. -->
    <OgImagePicker v-if="slug" :slug="slug" @picked="() => {}" />

    <!-- Local Media Fixer (the modal — surface lives in the Local Media section above) -->
    <LocalMediaFixer
      v-if="showMediaFixer"
      :file-path="file.path"
      :local-media="localMedia"
      :folder="mediaUploadFolder"
      @close="showMediaFixer = false"
      @fixed="onLocalMediaFixed"
      @open-alt-text="onOpenAltTextFromFixer"
    />

    <!-- Toolbar dock — sticks to the bottom of the metadata-stack scroll
         region so the publish CTA is always visible, even when the user
         has sized the meta area tighter than its natural content. Without
         this, ActionToolbar was the last child of an overflow:auto column
         and got pushed below the scroll viewport, clipping the publish
         button. -->
    <div class="toolbar-dock">
      <ActionToolbar
        :enabled-editors="enabledEditors"
        :publish-targets="publishTargets"
        :has-multiple-targets="hasMultipleTargets"
        :selected-target-id="selectedTargetId"
        :is-live="isLive"
        :live-url="liveUrl"
        :is-crowned="isCrowned"
        :crowning="crowning"
        :unpublishing="unpublishing"
        :publishing="publishing"
        :is-safe="file.is_safe"
        :is-scheduled="isScheduled"
        :is-unlisted="isUnlisted"
        @open-obsidian="openInObsidian"
        @open-editor="openInEditor"
        @open-preview="openPreview"
        @select-target="selectTarget"
        @show-syndication="showSyndicationWizard = true"
        @crown-post="crownPost"
        @unpublish="unpublish"
        @open-publish-confirm="openPublishConfirm"
        @publish-unlisted="publishUnlisted"
        @toggle-schedule="showSchedulePicker = !showSchedulePicker"
      />

      <!-- Schedule Picker -->
      <div v-if="showSchedulePicker" class="schedule-picker">
        <input
          type="datetime-local"
          v-model="scheduleDate"
          class="schedule-input"
          :min="new Date().toISOString().slice(0, 16)"
        />
        <button @click="schedulePublish" :disabled="!scheduleDate" class="btn accent">Confirm Schedule</button>
        <button @click="showSchedulePicker = false" class="btn">Cancel</button>
      </div>
    </div>
    </div>
    <!-- /metadata-stack -->

    <!-- Drag this to give the rendered preview more (or less) room. Double-
         click to return the metadata stack to natural (content-fit) height. -->
    <ResizeHandle
      axis="y"
      :active="metaDragging"
      data-tip="drag to resize · double-click to reset"
      @down="startMetaResize"
      @reset="resetMetaHeight"
    />

    <!-- Syndication Wizard Modal -->
    <SyndicationWizard
      v-if="showSyndicationWizard && liveUrl"
      :post-url="liveUrl"
      :title="title"
      :slug="slug"
      :dek="file.dek"
      :tags="file.tags"
      :content-type="file.content_type"
      :visibility="isPasswordProtected ? 'protected' : isUnlisted ? 'unlisted' : 'public'"
      @close="showSyndicationWizard = false"
      @queued="onSyndicationQueued"
    />


    <!-- Publish Confirmation -->
    <PublishConfirmModal
      :show="showPublishConfirm"
      :file="file"
      :slug="slug"
      :target-url="targetUrl"
      :publish-context="publishContext"
      :is-republish="publishConfirmRepublish"
      @close="closePublishConfirm"
      @confirm="(isRepublish: boolean) => publish(isRepublish)"
    />

    <!-- Content divider — plain tracked-caps label, no decorative glyph.
         Mail uses the same restraint on its section labels. -->
    <div class="content-divider">
      <span>CONTENT</span>
    </div>

    <!-- Preview -->
    <div class="preview">
      <!-- Thin macOS-style indeterminate progress bar pinned to the top
           of the preview pane. Only renders while content is loading AND
           the load is taking long enough to be worth showing — fast loads
           never flash it. -->
      <div v-if="showLoadingIndicator" class="preview-progress" aria-hidden="true">
        <div class="preview-progress-track"></div>
      </div>

      <!-- Render error card — surfaces what would otherwise be a silent
           blank pane. Sits above the rendered content so the user sees
           the cause and, when applicable, the raw text fallback below. -->
      <div v-if="renderError" class="render-error-card">
        <div class="render-error-stage">{{ renderError.stage }} failed</div>
        <div class="render-error-msg">{{ renderError.message }}</div>
        <div class="render-error-path">{{ file.path }}</div>
      </div>

      <Transition name="preview-fade" mode="out-in">
        <div
          v-if="renderedContent"
          key="content"
          class="rendered-content"
          v-html="renderedContent"
        ></div>
        <div
          v-else-if="showSkeleton"
          key="skeleton"
          class="preview-skeleton"
          aria-busy="true"
          aria-live="polite"
        >
          <div
            v-for="(block, bi) in skeletonBlocks"
            :key="bi"
            class="skel-block"
            :class="`skel-${block.type}`"
          >
            <template v-if="block.type === 'heading'">
              <div class="skel-line skel-heading" :class="`skel-h${block.level}`" />
            </template>
            <template v-else-if="block.type === 'hr'">
              <div class="skel-hr-line" />
            </template>
            <template v-else-if="block.type === 'image'">
              <div class="skel-image" />
            </template>
            <template v-else-if="block.type === 'code'">
              <div class="skel-code">
                <div
                  v-for="n in block.lines"
                  :key="n"
                  class="skel-line skel-code-line"
                  :style="{ width: skelLineWidth(bi, n, block) + '%' }"
                />
              </div>
            </template>
            <template v-else-if="block.type === 'list'">
              <div
                v-for="n in block.lines"
                :key="n"
                class="skel-line skel-list-item"
                :style="{ width: skelLineWidth(bi, n, block) + '%' }"
              />
            </template>
            <template v-else-if="block.type === 'quote'">
              <div class="skel-quote">
                <div
                  v-for="n in block.lines"
                  :key="n"
                  class="skel-line"
                  :style="{
                    width:
                      block.shortLast && n === block.lines && block.lines > 1
                        ? '45%'
                        : skelLineWidth(bi, n, block) + '%',
                  }"
                />
              </div>
            </template>
            <template v-else>
              <div
                v-for="n in block.lines"
                :key="n"
                class="skel-line"
                :style="{
                  width:
                    block.shortLast && n === block.lines && block.lines > 1
                      ? skelLastLineWidth(bi) + '%'
                      : skelLineWidth(bi, n, block) + '%',
                }"
              />
            </template>
          </div>
        </div>
      </Transition>
    </div>
  </div>
</template>

<style scoped>
.panel {
  flex: 1;
  display: flex;
  flex-direction: column;
  min-width: 0;
  min-height: 0;
  overflow: hidden;
  background: var(--bg-primary);
}

/* Wrapper around the status banner / header / lint / alt-text / OG /
   action toolbar. Default behavior is "no cap" — flows to content. When
   the user drags the ResizeHandle below, `.sized` is added and the wrapper
   becomes a fixed-height scroll region so the rendered content gets more
   room. Double-click the divider clears the cap. */
.metadata-stack {
  flex-shrink: 0;
  display: flex;
  flex-direction: column;
  min-height: 0;
}
.metadata-stack.sized {
  overflow-y: auto;
}

/* Pin the publish CTA + schedule picker to the bottom of the visible
 * meta-stack viewport. With `.sized` (user-dragged height), the stack
 * scrolls vertically; without `.toolbar-dock` sticky, the toolbar
 * would scroll off the bottom and the publish button could disappear
 * behind the meta-stack resize edge. */
.toolbar-dock {
  position: sticky;
  bottom: 0;
  background: var(--bg-solid);
  /* Soft fade-up so content scrolling under doesn't look chopped. */
  box-shadow: 0 -8px 12px -8px var(--bg-solid);
  z-index: 2;
}
.metadata-stack.resizing {
  /* Disable any internal transitions mid-drag so the panes track 1:1. */
  transition: none;
  /* No backdrop-filter on the scroll container — the parent window is
     opaque, so the blur was decorative only and cost a GPU pass per frame
     while scrolling long posts. */
  animation: panelEnter 0.25s cubic-bezier(0.16, 1, 0.3, 1);
}

@keyframes panelEnter {
  from {
    opacity: 0.8;
    transform: translateX(4px);
  }
  to {
    opacity: 1;
    transform: translateX(0);
  }
}

.panel.live {
  background: linear-gradient(180deg, color-mix(in srgb, var(--success) 8%, transparent) 0%, var(--bg-primary) 200px);
}

/* Schedule Picker */
.schedule-picker {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 8px 16px;
  background: var(--bg-tertiary);
  border-bottom: 1px solid var(--border);
}

.schedule-input {
  font-size: 12px;
  font-family: 'SF Mono', monospace;
  background: var(--bg-secondary);
  border: 1px solid var(--border-light);
  border-radius: 4px;
  padding: 4px 8px;
  color: var(--text-primary);
  color-scheme: dark;
}

@media (prefers-color-scheme: light) {
  .schedule-input {
    color-scheme: light;
  }
}

/* Header */
.header {
  padding: 12px 16px 8px;
}

.header h1 {
  /* Mail's email subject is ~17px semibold — bigger than UI text, smaller
     than blog-post body. Reads as "this is what you're looking at" without
     shouting. */
  font-size: 17px;
  font-weight: 600;
  line-height: 1.25;
  letter-spacing: -0.01em;
  margin: 0;
}

.header h1.derived-title {
  color: var(--text-secondary);
  font-style: italic;
}

.title-hint {
  font-size: 9px;
  color: var(--text-tertiary);
  margin: 2px 0 0 0;
  font-style: normal;
}

.header .dek {
  font-size: 12px;
  color: var(--text-secondary);
  margin: 4px 0 0 0;
  line-height: 1.4;
  font-style: italic;
}

/* Success Toast */
.success-toast {
  /* Ephemeral confirmation — quieted from a giant green-on-black slab to
     a small translucent pill, accent text only. Reads like a macOS HUD
     notification (think volume-key overlay) rather than a webby toast. */
  position: absolute;
  top: 50%;
  left: 50%;
  transform: translate(-50%, -50%);
  background: color-mix(in srgb, var(--bg-solid) 88%, transparent);
  color: var(--success);
  padding: 10px 18px;
  border-radius: 10px;
  font-size: 13px;
  font-weight: 600;
  z-index: 100;
  border: 1px solid color-mix(in srgb, var(--success) 30%, transparent);
  box-shadow:
    0 8px 24px rgba(0, 0, 0, 0.35),
    0 0 0 1px color-mix(in srgb, var(--success) 15%, transparent);
  backdrop-filter: blur(20px);
  -webkit-backdrop-filter: blur(20px);
  display: flex;
  align-items: center;
  gap: 8px;
  white-space: nowrap;
}

.success-toast.milestone {
  background: color-mix(in srgb, var(--bg-solid) 88%, transparent);
  color: var(--warning);
  padding: 12px 22px;
  border-radius: 10px;
  font-size: 14px;
  border-color: color-mix(in srgb, var(--warning) 30%, transparent);
  box-shadow:
    0 8px 28px rgba(0, 0, 0, 0.4),
    0 0 0 1px color-mix(in srgb, var(--warning) 18%, transparent);
}

.toast-enter-active {
  transition: all 0.4s cubic-bezier(0.34, 1.56, 0.64, 1);
}

.toast-leave-active {
  transition: all 0.2s ease-out;
}

.toast-enter-from {
  opacity: 0;
  transform: translate(-50%, -50%) scale(0.8);
}

.toast-leave-to {
  opacity: 0;
  transform: translate(-50%, -50%) scale(0.95);
}

.toast-enter-to {
  animation: celebrate 0.4s cubic-bezier(0.34, 1.56, 0.64, 1) forwards;
}

@keyframes celebrate {
  0% {
    transform: translate(-50%, -50%) scale(0.8);
  }
  50% {
    transform: translate(-50%, -50%) scale(1.08);
  }
  100% {
    transform: translate(-50%, -50%) scale(1);
  }
}

/* Panel glow on publish — quieted from a 60px inset green glow to a
   thinner edge wash that fades quickly. Apple animations are brief and
   restrained; a 1.2s neon flood reads like a web animation. */
.panel.just-published {
  animation: successGlow 0.8s ease-out forwards;
}

@keyframes successGlow {
  0% {
    box-shadow: inset 0 0 24px color-mix(in srgb, var(--success) 18%, transparent);
  }
  100% {
    box-shadow: none;
  }
}

/* Copy Feedback */
.copy-feedback {
  position: fixed;
  bottom: 20px;
  right: 20px;
  background: var(--text-primary);
  color: var(--bg-solid);
  padding: 8px 16px;
  border-radius: 6px;
  font-size: 11px;
  font-weight: 500;
  z-index: 100;
  box-shadow: 0 4px 12px rgba(0, 0, 0, 0.3);
}

.fade-enter-active,
.fade-leave-active {
  transition: all 0.2s ease;
}

.fade-enter-from,
.fade-leave-to {
  opacity: 0;
  transform: translateY(8px);
}

/* Shared section label/count styles */
.label {
  font-size: 10px;
  font-weight: 600;
  color: var(--text-tertiary);
  text-transform: uppercase;
  letter-spacing: 0.75px;
}
.count {
  font-size: 10px;
  font-weight: 600;
  color: var(--text-secondary);
}
.count.warning {
  color: var(--warning);
}

/* Backlinks */
.backlinks {
  padding: 8px 16px;
  border-bottom: 1px solid var(--border);
  max-height: 120px;
  overflow-y: auto;
}

.backlinks-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-bottom: 6px;
}

.backlinks-loading {
  font-size: 10px;
  color: var(--text-tertiary);
}
.backlinks-list {
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.backlink-item {
  display: flex;
  flex-direction: column;
  gap: 1px;
  padding: 4px 6px;
  background: var(--bg-tertiary);
  border-radius: 4px;
}

.backlink-title {
  font-size: 11px;
  font-weight: 500;
  color: var(--text-primary);
}
.backlink-context {
  font-size: 9px;
  color: var(--text-tertiary);
  font-family: 'SF Mono', monospace;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

/* Media health sections (Local Media + Alt Text) — shared shell so both
   feel like steps of the same flow. */
.media-section {
  padding: 8px 16px;
  border-bottom: 1px solid var(--border);
}

.media-section-header {
  display: flex;
  align-items: center;
  gap: 8px;
  margin-bottom: 4px;
}

.section-toggle {
  display: flex;
  align-items: center;
  gap: 8px;
  flex: 1;
  background: none;
  border: none;
  padding: 0;
  cursor: pointer;
  color: inherit;
  font: inherit;
  text-align: left;
}

.section-toggle .caret {
  transition: transform 0.15s;
  color: var(--text-tertiary);
}
.section-toggle .caret.collapsed {
  transform: rotate(-90deg);
}

.media-section-hint {
  font-size: 10px;
  color: var(--text-tertiary);
  line-height: 1.4;
}
.media-section-hint strong {
  color: var(--text-secondary);
  font-weight: 600;
}

/* Sequence indicator — "step 1 of 2" / "step 2 of 2" — only renders when
   both sections are visible. Quieted from a filled pill to plain muted
   tracked-caps text; the section label itself is the headline, the step
   info is just an annotation. */
.step-pill {
  margin-left: 6px;
  font-size: 9px;
  font-weight: 500;
  letter-spacing: 0.06em;
  text-transform: uppercase;
  color: var(--text-tertiary);
  opacity: 0.7;
}

/* Inline section buttons (Upload to Cloudinary / Describe). Primary uses
   the macOS accent; ghost is a subtle secondary for "you can do this but
   it's not the recommended next step right now." */
.section-btn {
  margin-left: auto;
  padding: 3px 10px;
  font-size: 10px;
  font-weight: 500;
  border: none;
  border-radius: 4px;
  cursor: pointer;
  font-family: inherit;
  transition: background 0.15s;
  white-space: nowrap;
}
.section-btn.primary {
  background: var(--accent);
  color: var(--accent-contrast);
}
.section-btn.primary:hover {
  background: var(--accent-strong);
}
.section-btn.ghost {
  background: var(--bg-tertiary);
  color: var(--text-secondary);
  border: 1px solid var(--border);
}
.section-btn.ghost:hover {
  background: var(--hover-bg);
  color: var(--text-primary);
}

/* Schedule Picker Buttons — same macOS push-button language as
   ActionToolbar's .btn. Color-mix darken on hover, no lift, no
   brightness filter. */
.btn {
  padding: 5px 12px;
  border: none;
  border-radius: 5px;
  font-size: 11px;
  font-weight: 500;
  cursor: pointer;
  text-decoration: none;
  background: var(--bg-tertiary);
  color: var(--text-primary);
  transition: background 0.12s ease, color 0.12s ease;
  min-height: 24px;
  display: inline-flex;
  align-items: center;
  gap: 4px;
}

.btn:hover {
  background: color-mix(in srgb, var(--bg-tertiary) 70%, white);
}
.btn:active {
  background: color-mix(in srgb, var(--bg-tertiary) 90%, black);
}
.btn.accent {
  background: color-mix(in srgb, var(--success) 20%, var(--bg-tertiary));
  color: var(--success);
  font-weight: 600;
}
.btn:disabled {
  background: var(--bg-tertiary);
  color: var(--text-tertiary);
  cursor: not-allowed;
  filter: none;
}

/* Content Divider */
.content-divider {
  padding: 4px 16px;
  background: var(--bg-tertiary);
  border-bottom: 1px solid var(--border);
}

.content-divider span {
  font-size: 8px;
  font-weight: 600;
  text-transform: uppercase;
  letter-spacing: 0.75px;
  color: var(--text-tertiary);
  display: inline-flex;
  align-items: center;
  gap: 4px;
}

/* Preview */
.preview {
  flex: 1;
  overflow-y: auto;
  padding: 12px 16px;
  scroll-behavior: smooth;
}

.preview pre {
  font-family: 'SF Mono', 'Menlo', monospace;
  font-size: 10.5px;
  line-height: 1.6;
  color: var(--text-secondary);
  white-space: pre-wrap;
  word-wrap: break-word;
  margin: 0;
  tab-size: 2;
}

/* --- Block-accurate loading skeleton -------------------------------------
   The skeleton is generated by parsing the markdown source (or recalled
   from cache). Each block type — heading, paragraph, code, list, quote,
   image, hr — gets dedicated styling so the loading state visually
   matches what's about to render. Cross-fades to the real content via
   Vue Transition (see `.preview-fade-*` below). */
.preview-skeleton {
  display: flex;
  flex-direction: column;
  padding-top: 8px;
}

.skel-block {
  display: flex;
  flex-direction: column;
  gap: 6px;
}
/* Spacing between blocks mirrors the rhythm of rendered prose. */
.skel-block + .skel-block {
  margin-top: 14px;
}
.skel-block.skel-heading + .skel-block,
.skel-block + .skel-block.skel-heading {
  margin-top: 18px;
}
.skel-block.skel-paragraph + .skel-block.skel-paragraph {
  margin-top: 10px;
}

.skel-line {
  height: 11px;
  border-radius: 3px;
  background: linear-gradient(
    90deg,
    var(--hover-bg) 0%,
    color-mix(in srgb, var(--accent) 6%, var(--hover-bg)) 50%,
    var(--hover-bg) 100%
  );
  background-size: 200% 100%;
  animation: skel-shimmer 1.4s linear infinite;
}

/* Headings — bigger, slightly darker block to match rendered heading
   weight. Level controls height. */
.skel-line.skel-heading {
  background: linear-gradient(
    90deg,
    color-mix(in srgb, var(--text-secondary) 14%, var(--hover-bg)) 0%,
    color-mix(in srgb, var(--accent) 12%, var(--hover-bg)) 50%,
    color-mix(in srgb, var(--text-secondary) 14%, var(--hover-bg)) 100%
  );
  background-size: 200% 100%;
  animation: skel-shimmer 1.4s linear infinite;
}
.skel-line.skel-h1 {
  height: 28px;
  width: 70%;
  margin-bottom: 4px;
}
.skel-line.skel-h2 {
  height: 22px;
  width: 55%;
  margin-bottom: 2px;
}
.skel-line.skel-h3 {
  height: 18px;
  width: 45%;
}
.skel-line.skel-h4,
.skel-line.skel-h5,
.skel-line.skel-h6 {
  height: 15px;
  width: 38%;
}

/* Code block — distinct monospace-feeling container with a few lines. */
.skel-code {
  padding: 10px 12px;
  background: color-mix(in srgb, var(--bg-tertiary) 70%, transparent);
  border: 1px solid var(--border);
  border-radius: 6px;
  display: flex;
  flex-direction: column;
  gap: 5px;
}
.skel-line.skel-code-line {
  height: 9px;
  border-radius: 2px;
}

/* List items — slight left indent + bullet dot. */
.skel-block.skel-list {
  padding-left: 14px;
  gap: 7px;
}
.skel-line.skel-list-item {
  height: 10px;
  position: relative;
}
.skel-line.skel-list-item::before {
  content: '';
  position: absolute;
  left: -10px;
  top: 50%;
  width: 4px;
  height: 4px;
  border-radius: 50%;
  background: color-mix(in srgb, var(--text-tertiary) 60%, transparent);
  transform: translateY(-50%);
}

/* Blockquote — indented + accent stripe on the left. */
.skel-quote {
  padding-left: 12px;
  border-left: 2px solid color-mix(in srgb, var(--accent) 35%, transparent);
  display: flex;
  flex-direction: column;
  gap: 5px;
  padding-top: 2px;
  padding-bottom: 2px;
}
.skel-quote .skel-line {
  height: 10px;
}

/* Image placeholder — wider box at typical image proportions. */
.skel-image {
  height: 180px;
  border-radius: 6px;
  background: linear-gradient(
    90deg,
    var(--hover-bg) 0%,
    color-mix(in srgb, var(--accent) 6%, var(--hover-bg)) 50%,
    var(--hover-bg) 100%
  );
  background-size: 200% 100%;
  animation: skel-shimmer 1.4s linear infinite;
}

/* HR — thin centered divider. */
.skel-hr-line {
  height: 1px;
  background: var(--border);
  margin: 6px 0;
}

@keyframes skel-shimmer {
  from {
    background-position: 100% 0;
  }
  to {
    background-position: -100% 0;
  }
}

/* Vue cross-fade between skeleton and rendered content. `mode="out-in"`
   means the leaving element finishes before the entering one starts —
   no visual overlap. */
.preview-fade-enter-active,
.preview-fade-leave-active {
  transition: opacity 0.16s ease;
}
.preview-fade-enter-from,
.preview-fade-leave-to {
  opacity: 0;
}

/* --- Thin top-of-pane progress bar ---------------------------------------
   macOS-style indeterminate stripe. Pinned to the top edge of the
   preview area, only renders after 60ms of loading so it doesn't flicker
   on cache hits or fast renders. */
.preview-progress {
  position: sticky;
  top: 0;
  left: 0;
  right: 0;
  height: 2px;
  margin: -12px -16px 8px;
  overflow: hidden;
  background: var(--hover-bg);
  z-index: 1;
}
.preview-progress-track {
  height: 100%;
  width: 30%;
  background: linear-gradient(
    90deg,
    transparent 0%,
    var(--accent) 50%,
    transparent 100%
  );
  animation: preview-progress-slide 1.1s ease-in-out infinite;
}
@keyframes preview-progress-slide {
  0% {
    transform: translateX(-100%);
  }
  100% {
    transform: translateX(400%);
  }
}

@media (prefers-reduced-motion: reduce) {
  .skel-line,
  .preview-progress-track {
    animation-duration: 3s;
  }
}

.analytics-strip {
  display: flex;
  align-items: center;
  gap: 14px;
  flex-wrap: wrap;
  padding: 8px 0;
  margin: -2px 0 8px;
  border-top: 1px solid var(--border, #1a1a1a);
  border-bottom: 1px solid var(--border, #1a1a1a);
  font-size: 11px;
  color: var(--text-secondary, #aaa);
  font-variant-numeric: tabular-nums;
}
.analytics-strip .stat,
.analytics-strip .stat-big {
  display: inline-flex;
  align-items: baseline;
  gap: 4px;
}
.analytics-strip .stat-big strong {
  font-size: 14px;
  color: var(--text-primary, #fff);
  font-weight: 700;
}
.analytics-strip .stat strong {
  font-size: 12px;
  color: var(--text-primary, #fff);
  font-weight: 600;
}
.analytics-strip .stat-unit {
  color: var(--text-tertiary, #777);
  font-size: 10px;
  text-transform: lowercase;
}
.analytics-strip .stat.warn strong {
  color: var(--warning, var(--warning));
}
.analytics-strip .sparkline {
  height: 22px;
  width: 120px;
  flex-shrink: 0;
  color: var(--text-secondary, #aaa);
  opacity: 0.8;
}
.analytics-strip .stat-period {
  margin-left: auto;
  font-size: 10px;
  color: var(--text-tertiary, #666);
  letter-spacing: 0.5px;
  text-transform: uppercase;
}
.analytics-strip .muted {
  color: var(--text-tertiary, #666);
  font-size: 10px;
}

/* Render-error card — never silent again. Soft warning wash, monospace
 * stage label, full file path so the user can locate the offending file
 * outside the app if needed. */
.render-error-card {
  margin: 12px 16px 0;
  padding: 10px 12px;
  border-radius: 6px;
  background: color-mix(in srgb, var(--warning) 10%, transparent);
  border-left: 3px solid var(--warning);
  font-size: 11px;
  line-height: 1.45;
  color: var(--text-secondary, #aaa);
}
.render-error-stage {
  font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
  font-size: 9.5px;
  letter-spacing: 0.05em;
  text-transform: uppercase;
  color: var(--warning);
  margin-bottom: 2px;
}
.render-error-msg {
  color: var(--text-primary, #fff);
}
.render-error-path {
  margin-top: 4px;
  font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
  font-size: 10px;
  color: var(--text-tertiary, #666);
  word-break: break-all;
}
</style>

<!-- Non-scoped rendered content styles (v-html content is not affected by scoped CSS) -->
<style src="../styles/rendered-content.css"></style>
