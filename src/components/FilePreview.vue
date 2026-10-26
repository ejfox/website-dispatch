<script setup lang="ts">
import { ref, watch, computed, nextTick, onMounted } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { message } from '@tauri-apps/plugin-dialog'
import { PhCheckCircle } from '@phosphor-icons/vue'
import { PanelRight } from 'lucide-vue-next'
import LintReceipt from './LintReceipt.vue'
import LocalMediaFixer from './LocalMediaFixer.vue'
import AltTextReviewer from './AltTextReviewer.vue'
import SyndicationWizard from './SyndicationWizard.vue'
import OgImagePicker from './OgImagePicker.vue'
import StatusBanner from './StatusBanner.vue'
import MetadataPanel from './MetadataPanel.vue'
import ActionToolbar from './ActionToolbar.vue'
import UiButton from './ui/UiButton.vue'
import InlineTip from './ui/InlineTip.vue'
import { useTip, invalidateTip, requestHelp } from '../composables/useTips'
import { CHANGED_HINT } from '../utils/vocab'
import PublishConfirmModal from './PublishConfirmModal.vue'
import PublishDiffPanel from './PublishDiffPanel.vue'
import WebmentionStatus from './WebmentionStatus.vue'
import {
  useToasts,
  useDeployStatus,
  useInspector,
  INSPECTOR_WIDTH,
  INSPECTOR_COLLAPSE_BELOW,
} from '../composables/useUiState'
import { buildInlineDiffHtml, type InlineDiffHtml } from '../utils/inlineDiffDom'
// Markdown processing now lives in src/workers/markdownWorker.ts — see
// `renderWorker` / `renderMarkdownInWorker` below. The pipeline imports
// (unified / remark-* / rehype-*) used to live here and run on the main
// thread, which was the single biggest cause of file-switch jank.
import { renderMermaidIn } from '../utils/mermaidRenderer'
import { perfTrace } from '../utils/perfTrace'
// Worker + render/skeleton caches now live at module scope (survive tab
// switches) — see utils/markdownRender.ts.
import {
  renderMarkdownInWorker,
  fetchServerRenderedHtml,
  isActiveRender,
  renderCache,
  skeletonCache,
  cacheRender,
  parseSkeleton,
  scheduleSkeletonCache,
  CHARS_PER_LINE,
  type SkeletonBlock,
} from '../utils/markdownRender'
import { Menu, MenuItem, PredefinedMenuItem } from '@tauri-apps/api/menu'
import { useLocalStorage, useElementSize } from '@vueuse/core'
import type { MarkdownFile, Backlink, LocalMediaRef, PostAnalytics, DeskToday, PublishDiff } from '../types'
import { useTagSuggestions } from '../composables/useTagSuggestions'
import { usePublishing, usePostActions, deployFailureTitle } from '../composables/usePublishFlow'
import type { ReviewMode } from '../utils/publishReview'
import { useAppConfig, useGitStatus } from '../composables/useVaultData'

const props = defineProps<{ file: MarkdownFile }>()
const emit = defineEmits<{ published: []; 'jump-to-path': [path: string] }>()

// Config (shared singleton)
const { appConfig, enabledEditors, publishTargets, hasMultipleTargets } = useAppConfig()
const selectedTargetId = useLocalStorage<string | null>('dispatch-target', null)

// ── Inspector column ────────────────────────────────────────────────────────
// The pane is two columns: the rendered post, and a fixed-width inspector
// (status, actions, details) with its own scroll. Below ~620px the column
// auto-collapses; the tab-strip toggle then opens it as an overlay. The
// open/closed choice is shared with App.vue's toggle and View › Toggle
// Inspector (⌥⌘I) through useInspector().
const inspector = useInspector()
const splitRef = ref<HTMLDivElement | null>(null)
const { width: paneWidth } = useElementSize(splitRef)
watch(paneWidth, (w) => {
  if (w > 0) inspector.setNarrow(w < INSPECTOR_COLLAPSE_BELOW)
})
const inspectorStyle = { width: `${INSPECTOR_WIDTH}px` }

// "See changes" in the Status section shows the since-last-publish diff
// above the post, where there is room to read it. Reset on file switch.
const diffOpen = ref(false)
watch(() => props.file.path, () => (diffOpen.value = false))

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

function renderCacheKey(file: { path: string; modified: number }) {
  // Include the active publish-target domain because remarkObsidianWikilinks
  // bakes it into the rendered HTML — same file rendered against a
  // different target should not share cache.
  return `${file.path}|${file.modified}|${activeTargetDomain.value}`
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
  // Tier 1: exact cache hit (skeleton may still be pending its deferred
  // computation — fall through to the cheaper tiers if so)
  const exact = renderCache.get(renderCacheKey(props.file))
  if (exact && exact.skeleton) return exact.skeleton

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
  currentDeploy,
  dismissDeploy,
  recheckDeploy,
  showSuccess,
  successMessage,
  showPublishConfirm,
  publishConfirmMode,
  showSyndicationWizard,
  showAltTextReviewer,
  openPublishConfirm,
  closePublishConfirm,
  publish,
  publishUnlisted,
  showSuccessToast,
  onSyndicationQueued,
  onAltTextApplied,
} = usePublishing({
  getFilePath: () => props.file.path,
  getFileIsSafe: () => props.file.is_safe,
  getActiveTargetId,
  isPasswordProtected: () => isPasswordProtected.value,
  isUnlisted: () => isUnlisted.value,
  onPublished: () => emit('published'),
  // Auto-fire webmentions after publish/republish. Bridgy Fed forwarding
  // is opt-in via Settings → Connections (default off).
  getTitle: () => props.file.title || slug.value || null,
  // The toast outlives a file switch: only reopen the sheet for the same
  // post, otherwise go back to it first.
  onRetry: (isRepublish: boolean, path: string) =>
    path === props.file.path ? requestPublish(isRepublish) : emit('jump-to-path', path),
  onPublishSuccess: (url: string) => afterPublish(url),
  // Webmentions wait for the deploy to be confirmed live (page answers 200),
  // and only if the review sheet's toggle was left on.
  onLive: (url: string, { webmentions }) => {
    if (!webmentions) return
    const bridgyFed = appConfig.value?.webmentions_bridgy_fed === true
    autoTriggerOnPublish(bridgyFed, url)
  },
})

// ── Share links (UTM-tagged for Umami) ─────────────────────────────────────
const postSources = ref<{ x: string; y: number }[]>([])
async function copyShareLink(source: string, medium: string) {
  const url = liveUrl.value
  if (!url) return
  try {
    const tagged = await invoke<string>('share_link', { url, source, medium })
    await navigator.clipboard.writeText(tagged)
    toasts.success(`Copied link for ${source === 'x' ? 'X' : source}`, tagged)
  } catch (e) {
    toasts.error("Couldn't copy the link", String(e))
  }
}

// ── Publish = ship ──────────────────────────────────────────────────────────
// A piece with `bench:` frontmatter came from the chart desk. Publishing it is
// the day's ship: log `desk shipped <bench> <url> [syndication urls…]`. For a
// Dispatch piece the syndication wizard opens first (EJ confirms the send),
// and the ship is logged when it closes, with whatever links it produced.
const toasts = useToasts()
const pendingShip = ref<{ bench: string; url: string } | null>(null)
const syndicatedUrls = ref<string[]>([])
const autoWizard = ref(false)

function afterPublish(url: string) {
  const bench = props.file.bench
  const firstPublish = !props.file.published_url // props not refreshed yet
  if (props.file.content_type === 'dispatch' && firstPublish) {
    pendingShip.value = bench ? { bench, url } : null
    syndicatedUrls.value = []
    autoWizard.value = true
    showSyndicationWizard.value = true
  } else if (bench) {
    shipToDesk(bench, [url], !firstPublish)
  }
}

async function shipToDesk(bench: string, urls: string[], onlyIfUnshipped = false) {
  try {
    const today = await invoke<DeskToday>('desk_today')
    const story = today.stories.find((s) => s.id === bench)
    if (story?.shipped) return // already logged
    if (!story && onlyIfUnshipped) return // republish of an older piece
  } catch {
    /* desk unreachable — still try to log */
  }
  try {
    const msg = await invoke<string>('desk_shipped', { id: bench, urls })
    toasts.success('Shipped', msg || undefined)
  } catch (e) {
    const id: number = toasts.push({
      kind: 'error',
      message: "Published, but the ship didn't get logged on the desk",
      detail: String(e),
      action: {
        label: 'Retry',
        run: () => {
          toasts.dismiss(id)
          shipToDesk(bench, urls)
        },
      },
      ttl: 0,
    })
  }
}

function onWizardSyndicated(urls: string[]) {
  syndicatedUrls.value = urls
}

function onWizardClosed() {
  showSyndicationWizard.value = false
  autoWizard.value = false
  const ship = pendingShip.value
  pendingShip.value = null
  if (ship) shipToDesk(ship.bench, [ship.url, ...syndicatedUrls.value])
}

// ── Draft gate + alt-text prep before publishing ───────────────────────────
const showDraftPrompt = ref(false)
const draftPromptRepublish = ref(false)
const draftPromptMode = ref<ReviewMode>('publish')
const clearingDraft = ref(false)

// Every way in (toolbar, MODIFIED banner, Unlisted button, ⌘↩, palette,
// Modified tab) lands here, then on the review sheet.
function requestPublish(isRepublish = false, mode: ReviewMode = 'publish') {
  if (props.file.draft) {
    draftPromptRepublish.value = isRepublish
    draftPromptMode.value = mode
    showDraftPrompt.value = true
    return
  }
  openPublishConfirm(isRepublish, mode)
}

async function clearDraftAndContinue() {
  clearingDraft.value = true
  try {
    await invoke('remove_draft', { path: props.file.path })
    showDraftPrompt.value = false
    emit('published') // refresh the list so the DRAFT badge goes away
    openPublishConfirm(draftPromptRepublish.value, draftPromptMode.value)
  } catch (e) {
    toasts.error("Couldn't clear draft", String(e))
  }
  clearingDraft.value = false
}

function onDraftPromptKey(e: KeyboardEvent) {
  if (e.key === 'Enter') {
    e.preventDefault()
    clearDraftAndContinue()
  } else if (e.key === 'Escape') {
    showDraftPrompt.value = false
  }
}
// Mirror this file's deploy into the shared store so its file-list row can
// show a deploying (pulsing accent) or failed (red) dot. A null deploy on
// the same path means it was dismissed; a path change just means we moved.
const deployStatusStore = useDeployStatus()
// One-line state for the compact bar shown when the inspector is hidden.
const miniStatus = computed(() => {
  const phase = currentDeploy.value?.phase
  if (phase === 'deploying') return { key: 'deploying', word: 'Deploying…' }
  if (phase === 'hidden') return { key: 'scheduled', word: 'Deployed, hidden for now' }
  if (phase === 'failed') {
    const f = currentDeploy.value?.failure
    return { key: f === 'timeout' || f === 'untracked' ? 'attention' : 'broken', word: deployFailureTitle(f) }
  }
  if (isScheduled.value) return { key: 'scheduled', word: 'Scheduled' }
  if (isLive.value)
    return hasUnpublishedChanges.value
      ? { key: 'attention', word: 'Changed since live' }
      : { key: 'live', word: 'Live' }
  return { key: 'draft', word: 'Not published' }
})

watch([() => props.file.path, currentDeploy], ([path, d], [oldPath]) => {
  if (d) deployStatusStore.track(path, d)
  else if (path === oldPath) deployStatusStore.clear(path)
})

watch(showDraftPrompt, (open) => {
  if (open) window.addEventListener('keydown', onDraftPromptKey)
  else window.removeEventListener('keydown', onDraftPromptKey)
})

// Dispatch pieces need image_alt with image (website contract). Generate and
// save it before publishing rather than blocking.
async function publishWithPrep(
  isRepublish: boolean,
  opts: { webmentions: boolean; hiddenUntil: string | null },
  mode: ReviewMode,
) {
  if (props.file.content_type === 'dispatch') {
    try {
      const alt = await invoke<string | null>('ensure_image_alt', { path: props.file.path })
      if (alt) console.log('[publish] image_alt ready:', alt)
    } catch (e) {
      closePublishConfirm()
      const id: number = toasts.push({
        kind: 'error',
        message: "Didn't publish: the image needs alt text",
        detail: String(e),
        action: {
          label: 'Retry',
          run: () => {
            toasts.dismiss(id)
            publishWithPrep(isRepublish, opts, mode)
          },
        },
        ttl: 0,
      })
      return
    }
  }
  if (mode === 'unlisted') await publishUnlisted(isRepublish, opts)
  else await publish(isRepublish, opts)
}

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

// Re-render markdown if the active publish target (and thus base URL) changes.
// Same worker path as the file-switch flow.
watch(activeTargetDomain, async (domain) => {
  if (!content.value) return
  try {
    const { id: renderId, promise } = renderMarkdownInWorker(content.value, domain)
    const html = await promise
    if (!isActiveRender(renderId)) return
    renderedContent.value = html
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
    // Cross into the shared click→paint tracer. The delta from the click (set
    // in FileList.onRowClick) to here is everything between the mousedown and
    // this function running: emit, App.vue @select, the selectedFile watchers,
    // and Vue re-rendering the sidebar. Often the surprising part of the lag.
    perfTrace.mark('loadContent start')

    // Reset *metadata* refs synchronously so the next paint shows a clean
    // slate (without this you briefly see OLD analytics / backlinks under
    // NEW metadata). BUT: leave `renderedContent` and `content` alone —
    // we want the previous post's body visible until the new one is
    // actually ready, like Mail / Notes do. Blanking on click made every
    // switch feel like a multi-second load even when the underlying work
    // was sub-100ms.
    justPublished.value = null
    backlinks.value = []
    localMedia.value = []
    postStats.value = null
    pageviewSeries.value = []
    showMediaFixer.value = false
    webmentionReport.value = null
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
      perfTrace.mark('cache hit · ref set')
      perfTrace.paintAfter('painted')
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
          perfTrace.mark('content IPC done')
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
            const { id: renderId, promise } = renderMarkdownInWorker(
              stripped,
              activeTargetDomain.value,
            )
            const rendered = await promise
            // Stale-render guard: if the user clicked another file mid-
            // worker, throw away this response. activeRenderId tracks
            // the latest request — earlier ones are dead by definition.
            if (file.path !== props.file.path || !isActiveRender(renderId)) return
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
            perfTrace.mark('markdown worker done')
            renderedContent.value = rendered
            console.log(
              `[perf] file-switch markdown-only ${(performance.now() - markdownT0).toFixed(1)}ms · ${switchName}`,
            )
            perf('TOTAL (rendered)')
            perfTrace.mark('rendered · ref set')
            // Flush after the browser actually paints the new post.
            perfTrace.paintAfter('painted')
            // Cache the render synchronously (cheap, keeps re-clicks instant);
            // defer the expensive skeleton parse off the paint path.
            cacheRender(cacheKey, { stripped, rendered })
            endPreviewLoad()
            nextTick(() => renderMermaidIn(document))
            scheduleSkeletonCache(cacheKey, file.path, stripped)
          } catch (err) {
            if (file.path !== props.file.path) return
            console.error('[render] markdown processor threw', err, 'on', file.path)
            // The client-side worker failed. Before giving up to raw text,
            // try the preview server (website2's real pipeline) which was
            // already primed via /set-file above. This is the reliable path
            // when the webview worker can't run (missing `document`, etc.).
            const serverHtml = await fetchServerRenderedHtml(file.path)
            if (file.path !== props.file.path) return
            if (serverHtml && serverHtml.trim()) {
              renderedContent.value = serverHtml
              perfTrace.mark('server fallback · ref set')
              perfTrace.paintAfter('painted (server)')
              cacheRender(cacheKey, { stripped, rendered: serverHtml })
              renderError.value = null
              endPreviewLoad()
              nextTick(() => renderMermaidIn(document))
              scheduleSkeletonCache(cacheKey, file.path, stripped)
            } else {
              // Last resort: show the raw content plus the error so the user
              // can still see what they wrote AND know what went wrong.
              renderedContent.value =
                `<pre class="render-fallback">${escapeHtml(stripped)}</pre>`
              renderError.value = {
                stage: 'process',
                message: err instanceof Error ? err.message : String(err),
              }
              endPreviewLoad()
            }
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

    // Non-critical IPCs (backlinks, local-media detection, analytics,
    // pageview series) used to fire in parallel with the content load.
    // Even though they ran async, they competed for the IPC channel and —
    // when Umami was slow or unreachable — held tokio workers for many
    // seconds. Defer them behind a microtask + requestIdleCallback so the
    // content render lands first and the user sees the post immediately.
    // Each one still bails early via the stale-file guard if the user
    // clicked away.
    const fireSecondary = () => {
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
        postSources.value = []
        invoke<{ x: string; y: number }[]>('get_post_sources', { url: file.published_url, days: 30 })
          .then((res) => {
            if (file.path === props.file.path) postSources.value = res || []
          })
          .catch(() => {
            /* optional: needs Umami */
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
    // requestIdleCallback isn't available on all webviews — fall back to
    // setTimeout with a short delay if missing. Either way: content
    // renders FIRST, secondaries kick in once the main thread breathes.
    if (typeof (window as any).requestIdleCallback === 'function') {
      ;(window as any).requestIdleCallback(fireSecondary, { timeout: 500 })
    } else {
      setTimeout(fireSecondary, 50)
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
  // Dispatch pieces: bare slug → content/dispatch/<slug>.md (no year).
  if (props.file.content_type === 'dispatch') return baseName
  const yearMatch = props.file.path.match(/\/blog\/(\d{4})\//)
  if (yearMatch) return `${yearMatch[1]}/${baseName}`
  return props.file.path.includes('/blog/') ? baseName : ''
})

const isLive = computed(() => !!props.file.published_url || !!justPublished.value)
const liveUrl = computed(() => props.file.published_url || justPublished.value)
const hasUnpublishedChanges = computed(() => props.file.warnings.includes('Modified since publish'))
const lintWarnings = computed(() => props.file.warnings.filter((w) => w !== 'Modified since publish'))

// ── Inline diff mode (D) ───────────────────────────────────────────────
// The post stays typeset exactly like Preview; changed words are marked in
// place. Direction is "vault → live": what pressing publish would do.
// Additions are vault-only words; removals are words that are live NOW and
// would be lost — the case that matters when the live copy carries fixes the
// vault lacks. Both sides go through the same worker renderer (passive
// renders, so the main Preview render is never made stale) and are diffed per
// block, then per word, by utils/inlineDiff.ts.
const diffOn = ref(false)
const diffLoading = ref(false)
const diffError = ref<string | null>(null)
const diffResult = ref<InlineDiffHtml | null>(null)
let diffRun = 0

const canDiff = computed(() => isLive.value && hasUnpublishedChanges.value)

async function computeInlineDiff() {
  const run = ++diffRun
  const path = props.file.path
  diffLoading.value = true
  diffError.value = null
  try {
    const pd = (await invoke('get_publish_diff', { filePath: path })) as PublishDiff
    if (run !== diffRun) return
    if (pd.published_body == null) throw new Error(pd.error || 'No published copy found.')
    const raw = (await invoke('get_file_content', { path })) as string
    const vaultBody = raw.replace(/^---\n[\s\S]*?\n---\n*/, '')
    const domain = activeTargetDomain.value
    const [liveHtml, vaultHtml] = await Promise.all([
      renderMarkdownInWorker(pd.published_body, domain, { passive: true }).promise,
      renderMarkdownInWorker(vaultBody, domain, { passive: true }).promise,
    ])
    if (run !== diffRun || path !== props.file.path) return
    diffResult.value = buildInlineDiffHtml(liveHtml, vaultHtml)
    nextTick(() => renderMermaidIn(document))
  } catch (e: any) {
    if (run !== diffRun) return
    diffResult.value = null
    diffError.value = typeof e === 'string' ? e : (e?.message ?? String(e))
  } finally {
    if (run === diffRun) diffLoading.value = false
  }
}

function toggleDiff() {
  if (diffOn.value) return exitDiff()
  if (!canDiff.value) return false
  diffOn.value = true
  void computeInlineDiff()
  return true
}

/** Leave diff mode. Returns true if it was on (so Escape can stop there). */
function exitDiff(): boolean {
  if (!diffOn.value) return false
  diffOn.value = false
  diffRun++
  diffResult.value = null
  diffError.value = null
  diffLoading.value = false
  return true
}

// New file, or the post stopped being "changed since live" (just published,
// or the edit was reverted): drop out of diff mode.
watch(
  () => props.file.path,
  () => exitDiff(),
)
watch(canDiff, (ok) => {
  if (!ok) exitDiff()
})
// Vault edit while diffing (file mtime moves): recompute in place.
watch(
  () => props.file.modified,
  () => {
    if (diffOn.value) void computeInlineDiff()
  },
)

// Alt text detection
const missingAltTextCount = computed(() => {
  const w = props.file.warnings.find((w) => w.startsWith('Missing alt text'))
  if (!w) return 0
  const match = w.match(/\((\d+)\)/)
  return match ? parseInt(match[1]) : 0
})

// ── First-use tips (see composables/useTips.ts) ───────────────────────────
// Each shows only in its context, one at a time, and retires once you do the
// thing it describes.
// Two placements share the 'inspector' tip: inside the inspector when it's
// open, and under the one-line bar (whose Details button opens it) when it's
// hidden. Opening it by hand (Details, the tab-strip toggle, ⌥⌘I) retires it.
const inspectorTip = useTip('inspector', () => inspector.visible.value)
const inspectorHiddenTip = useTip('inspector', () => !inspector.visible.value)
watch([inspector.open, inspector.overlay], ([o, ov], [po, pov]) => {
  if (o !== po || (ov && !pov)) invalidateTip('inspector')
})
const publishTip = useTip('publish', () => !isLive.value && props.file.is_safe && !props.file.publish_at)
const changedTip = useTip('changed', () => isLive.value && hasUnpublishedChanges.value)
const altTextTip = useTip('alt-text', () => missingAltTextCount.value > 0 && inspector.visible.value)
// The social-card picker remembers whether it's folded; the tip only makes
// sense while it is.
const ogCollapsed = useLocalStorage('dispatch-og-collapsed', true)
const socialCardTip = useTip(
  'social-card',
  () => !!slug.value && props.file.content_type !== 'dispatch' && ogCollapsed.value && inspector.visible.value,
)
const syndicateTip = useTip(
  'syndicate',
  () => isLive.value && !hasUnpublishedChanges.value && !props.file.password && inspector.visible.value,
)
watch(showPublishConfirm, (open) => open && invalidateTip('publish'))
watch([diffOpen, publishing], ([d, p]) => (d || p) && isLive.value && invalidateTip('changed'))
watch(showAltTextReviewer, (open) => open && invalidateTip('alt-text'))
watch(ogCollapsed, (c) => !c && invalidateTip('social-card'))
watch(showSyndicationWizard, (open) => open && invalidateTip('syndicate'))

// ── Photo-post treatment ─────────────────────────────────────────────────
// A post the vault marks `type: photos` (content_type set in vault.rs). These
// get a contact-sheet gallery up top + a media/alt-text-first summary banner.
const isPhotoPost = computed(() => props.file.content_type === 'photos')

// Every image URL in the rendered body, pulled straight from the HTML we're
// about to show — the gallery and the inline render stay in sync by construction.
const galleryImages = computed<string[]>(() => {
  if (!isPhotoPost.value || !renderedContent.value) return []
  const urls: string[] = []
  const re = /<img[^>]+src="([^"]+)"/g
  let m: RegExpExecArray | null
  while ((m = re.exec(renderedContent.value))) urls.push(m[1])
  return urls
})

// Small cropped Cloudinary variant for gallery cells (keeps the grid light).
function galleryThumb(url: string): string {
  if (url.includes('res.cloudinary.com') && url.includes('/upload/')) {
    return url.replace('/upload/', '/upload/w_320,h_320,c_fill,q_auto,f_auto/')
  }
  return url
}

// Visibility states
const isUnlisted = computed(() => props.file.unlisted || !!props.file.password)
const isPasswordProtected = computed(() => !!props.file.password)
const visibilityLabel = computed(() => {
  if (isPasswordProtected.value) return 'PASSWORD'
  if (isUnlisted.value) return 'UNLISTED'
  return null
})

// Post actions composable (Vue-page conversion, webmentions, unpublish)
const {
  sendingWebmentions,
  webmentionReport,
  isVuePage,
  converting,
  unpublishing,
  convertToVuePage,
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
    showCopyFeedback('Will publish then')
    emit('published')
  } catch (e) {
    await message(String(e), { title: 'Couldn’t set the time', kind: 'error' })
  }
}

async function cancelSchedule() {
  try {
    await invoke('cancel_schedule', { path: props.file.path })
    showCopyFeedback('Won’t publish later')
    emit('published')
  } catch (e) {
    await message(String(e), { title: 'Couldn’t cancel publishing later', kind: 'error' })
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
/** Menu-bar entry points (useMenuActions). Each opens what the matching
 *  inspector control opens; the menu enables them only when they apply. */
function openSyndication() {
  if (liveUrl.value) showSyndicationWizard.value = true
}
function openSchedulePicker() {
  showSchedulePicker.value = true
}

defineExpose({
  openPublishConfirm: requestPublish,
  unpublish,
  openSyndication,
  openSchedulePicker,
  toggleDiff,
  exitDiff,
  canDiff,
  diffOn,
})

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
      <div v-if="showSuccess" class="success-toast">
        <PhCheckCircle :size="13" weight="fill" />
        <span>{{ successMessage }}</span>
      </div>
    </Transition>

    <!-- Copy Feedback -->
    <Transition name="fade">
      <div v-if="copyFeedback" class="copy-feedback">
        {{ copyFeedback }}
      </div>
    </Transition>

    <!-- Two columns: the post, and the inspector. Below ~620px the inspector
         becomes an overlay panel (see useInspector / .is-narrow). -->
    <!-- Inspector hidden (by choice or because the pane is narrow): keep the
         post's state and its one primary action reachable in a single line. -->
    <div v-if="!inspector.visible.value" class="insp-mini">
      <span class="status-dot" :data-status="miniStatus.key" />
      <span class="insp-mini-word">{{ miniStatus.word }}</span>
      <span class="insp-mini-spacer" />
      <UiButton
        variant="primary"
        size="small"
        :disabled="publishing"
        :label="publishing ? 'Publishing…' : isLive ? 'Update live post…' : file.is_safe ? 'Publish…' : 'See what to fix…'"
        @click="requestPublish(isLive)"
      />
      <UiButton
        variant="plain"
        size="small"
        label="Details"
        :icon="PanelRight"
        aria-label="Show Inspector"
        data-tip="Show Inspector (⌥⌘I)"
        @click="inspector.toggle()"
      />
    </div>

    <InlineTip v-if="!inspector.visible.value && inspectorHiddenTip.show.value" class="mini-tip" @close="inspectorHiddenTip.close()">
      Status, links, photos and the social card are under Details, or press <kbd>⌥⌘I</kbd>.
    </InlineTip>

    <div ref="splitRef" class="preview-split" :class="{ 'is-narrow': inspector.narrow.value }">
      <!-- ── The post ─────────────────────────────────────────────────────── -->
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

        <!-- What changed since the last publish — opened from the inspector's
             Status section, shown here where a diff has room to breathe. -->
        <PublishDiffPanel
          v-if="isLive && hasUnpublishedChanges"
          class="preview-diff"
          :file-path="file.path"
          :open="diffOpen"
          @close="diffOpen = false"
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

        <!-- Contact-sheet gallery for photo posts — the images ARE the post, so
             lead with a grid. The full inline render stays below for captions and
             any prose between shots. -->
        <div v-if="isPhotoPost && galleryImages.length" class="photo-gallery">
          <a
            v-for="(src, gi) in galleryImages"
            :key="gi"
            class="gallery-cell"
            :href="src"
            target="_blank"
            rel="noopener"
            :title="`Open image ${gi + 1} of ${galleryImages.length}`"
          >
            <img :src="galleryThumb(src)" loading="lazy" alt="" />
          </a>
        </div>

        <!-- Inline diff (D): marks what publishing would change, in place.
             Additions are vault-only words; struck words are live-only and
             publishing would remove them. -->
        <div v-if="isLive" class="diff-bar">
          <span>{{ diffOn ? 'Changes · vault → live' : '' }}</span>
          <button
            class="diff-toggle"
            :class="{ on: diffOn }"
            :disabled="!canDiff && !diffOn"
            :aria-pressed="diffOn"
            :data-tip="
              diffOn
                ? 'Back to the plain preview (D or esc)'
                : canDiff
                  ? 'Mark what publishing would change, inline (D)'
                  : 'Live copy matches the vault — nothing to diff'
            "
            @click="toggleDiff"
          >
            Diff <kbd>D</kbd>
          </button>
        </div>
        <div v-if="diffOn" class="diff-summary" role="status">
          <template v-if="diffLoading && !diffResult">Comparing vault with the live copy…</template>
          <template v-else-if="diffError">Diff unavailable: {{ diffError }}</template>
          <template v-else-if="diffResult">
            <span class="diff-summary-line">
              Publishing would change the live post:
              <span class="diff-summary-num">{{ diffResult.wordsAdded }}</span> word{{ diffResult.wordsAdded === 1 ? '' : 's' }} added
              ·
              <span class="diff-summary-num">{{ diffResult.wordsRemoved }}</span> removed
            </span>
            <span v-if="diffResult.wordsRemoved > 0" class="diff-summary-warn">
              <del class="diff-del">struck</del> words are live now and exist only there; publishing removes them.
            </span>
            <span v-else-if="diffResult.wordsAdded === 0" class="diff-summary-line muted">No word changes; only markup or whitespace differs.</span>
          </template>
        </div>

      <Transition name="preview-fade">
        <div
          v-if="diffOn && diffResult"
          key="diff"
          class="rendered-content diff-mode"
          v-html="diffResult.html"
        ></div>
        <div
          v-else-if="renderedContent"
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

      <!-- ── Inspector ────────────────────────────────────────────────────── -->
      <aside v-if="inspector.visible.value" class="inspector" :style="inspectorStyle" aria-label="Inspector">
        <InlineTip v-if="inspectorTip.show.value" class="insp-tip" @close="inspectorTip.close()">
          This is the Inspector: where the post stands, what to fix, and how to share it
          (<kbd>⌥⌘I</kbd> hides it).
        </InlineTip>
        <!-- Status -->
        <section class="insp-section">
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
            :deploy="currentDeploy"
            :diff-open="diffOpen"
            @dismiss-deploy="dismissDeploy(file.path)"
            @recheck-deploy="recheckDeploy(file.path)"
            @copy-url="copyUrl"
            @copy-url-password="copyUrlAndPassword"
            @republish="requestPublish(true)"
            @cancel-schedule="cancelSchedule"
            @toggle-diff="diffOpen = !diffOpen"
          />

          <!-- Webmention status — auto-fires after publish/republish, surfaced
               inline so the user sees an outcome without an extra click. -->
          <WebmentionStatus
            v-if="isLive && (sendingWebmentions || webmentionReport)"
            :report="webmentionReport"
            :sending="sendingWebmentions"
            @resend="triggerWebmentions({ bridgyFed: appConfig?.webmentions_bridgy_fed === true, force: true })"
          />
          <InlineTip v-if="changedTip.show.value" @close="changedTip.close()">
            {{ CHANGED_HINT }}: See changes shows the diff, and Update live post sends your edits.
          </InlineTip>
        </section>

        <!-- Actions: the one primary, then Open in / Share / More -->
        <section class="insp-section">
          <InlineTip
            v-if="publishTip.show.value"
            action="How publishing works"
            @close="publishTip.close()"
            @action="requestHelp('overview')"
          >
            Publish shows you a review, then commits the note to the site’s repo and pushes it, and
            ejfox.com has it about 3 minutes later.
          </InlineTip>
          <ActionToolbar
            :enabled-editors="enabledEditors"
            :publish-targets="publishTargets"
            :has-multiple-targets="hasMultipleTargets"
            :selected-target-id="selectedTargetId"
            :is-live="isLive"
            :live-url="liveUrl"
            :is-vue-page="isVuePage || file.content_type === 'dispatch' /* no Vue-page conversion for dispatch */"
            :converting="converting"
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
            @copy-share-link="copyShareLink"
            @convert-to-vue-page="convertToVuePage"
            @unpublish="unpublish"
            @open-publish-confirm="requestPublish"
            @publish-unlisted="requestPublish(false, 'unlisted')"
            @toggle-schedule="showSchedulePicker = !showSchedulePicker"
          />
          <InlineTip v-if="syndicateTip.show.value" @close="syndicateTip.close()">
            Bluesky and Mastodon only hear about a post when you send it with Syndicate, the last button
            under Share.
          </InlineTip>

          <!-- Schedule Picker (More › Publish later…) -->
          <div v-if="showSchedulePicker" class="schedule-picker">
            <span class="insp-label">Publish later</span>
            <input
              type="datetime-local"
              v-model="scheduleDate"
              class="schedule-input"
              aria-label="Publish at"
              :min="new Date().toISOString().slice(0, 16)"
            />
            <div class="schedule-actions">
              <UiButton size="small" variant="plain" label="Cancel" @click="showSchedulePicker = false" />
              <!-- Secondary: the primary above is still on screen. -->
              <UiButton size="small" :disabled="!scheduleDate" label="Publish at this time" @click="schedulePublish" />
            </div>
            <span class="insp-note">Dispatch publishes it at the time you pick, if it still has nothing to fix and isn’t a draft (keep Dispatch running).</span>
          </div>
        </section>

        <!-- Details -->
        <section class="insp-section">
          <span class="insp-label">Details</span>
          <div class="insp-list">
            <MetadataPanel
              :file="file"
              :obsidian-connected="obsidianConnected"
              :git-status="gitStatus"
              :suggested-tags="suggestedTags"
              :available-tags="availableTags"
              :adding-tag="addingTag"
              :metadata-expanded="metadataExpanded"
              :has-unpublished-changes="hasUnpublishedChanges"
              :is-unlisted="isUnlisted"
              :is-password-protected="isPasswordProtected"
              @toggle-metadata="metadataExpanded = !metadataExpanded"
              @add-tag="addTag"
            >

            <!-- Media health: Local upload (prerequisite) → Alt text (depends on
                 URLs). Order matters — local media must be hosted before alt text
                 can be generated, so Upload sits above Describe and the note
                 spells out the dependency. -->
            <div v-if="localImageCount > 0 || localVideoCount > 0" class="insp-kv insp-kv-action">
              <span class="insp-key">In vault</span>
              <span class="insp-val" :class="{ 'is-attention': localImageCount > 0 }">
                <template v-if="localImageCount > 0">{{ localImageCount }} image{{ localImageCount === 1 ? '' : 's' }}</template>
                <template v-if="localImageCount > 0 && localVideoCount > 0"> + </template>
                <template v-if="localVideoCount > 0">{{ localVideoCount }} video{{ localVideoCount === 1 ? '' : 's' }}</template>
              </span>
              <UiButton
                v-if="localImageCount > 0"
                size="small"
                label="Upload"
                title="Upload to Cloudinary; rewrites the markdown refs to public URLs"
                @click.stop="showMediaFixer = true"
              />
            </div>
            <span v-if="localImageCount > 0 || localVideoCount > 0" class="insp-note">
              <template v-if="localImageCount > 0">Uploading rewrites the refs to public URLs.</template>
              <template v-if="localImageCount > 0 && missingAltTextCount > 0"> Do it before describing.</template>
              <template v-if="localImageCount === 0">Videos stay local; nothing to upload.</template>
            </span>

            <div v-if="file.image_count > 0 || missingAltTextCount > 0" class="insp-kv insp-kv-action">
              <span class="insp-key">Photos</span>
              <span class="insp-val" :title="`${file.image_count} image${file.image_count === 1 ? '' : 's'} in the post`">
                <span v-if="missingAltTextCount > 0" class="is-attention-text">{{ missingAltTextCount }} missing alt</span>
                <template v-else>{{ file.image_count }} <span class="is-muted-text">· alt text complete</span></template>
              </span>
              <UiButton
                v-if="missingAltTextCount > 0"
                size="small"
                :variant="localImageCount > 0 ? 'plain' : 'secondary'"
                label="Describe"
                :title="localImageCount > 0 ? 'Open the describer (it will ask to upload local images first)' : 'Describe the images with AI'"
                @click.stop="showAltTextReviewer = true"
              />
            </div>
            <InlineTip v-if="altTextTip.show.value" class="insp-inline-tip" @close="altTextTip.close()">
              Describe drafts alt text for each image with AI, and nothing is saved until you approve it.
            </InlineTip>

            <!-- Analytics (Umami, last 30 days) while the post is live. -->
            <template v-if="isLive && (postStats || loadingStats)">
              <div class="insp-kv">
                <span class="insp-key">Views</span>
                <span v-if="loadingStats" class="insp-val is-muted">…</span>
                <span v-else-if="postStats" class="insp-val">
                  {{ fmtCount(postStats.pageviews) }}
                  <span v-if="postStats.visitors" class="is-muted-text">· {{ fmtCount(postStats.visitors) }} visitors</span>
                  <span class="is-muted-text">· 30d</span>
                </span>
              </div>
              <div v-if="postStats && (avgTimeOnPage || bounceRate !== null)" class="insp-kv">
                <span class="insp-key">Reading</span>
                <span class="insp-val">
                  <template v-if="avgTimeOnPage">{{ fmtDuration(avgTimeOnPage) }} avg</template>
                  <span v-if="bounceRate !== null" :class="bounceRate > 0.7 ? 'is-attention-text' : 'is-muted-text'">
                    <template v-if="avgTimeOnPage">· </template>{{ Math.round(bounceRate * 100) }}% bounce
                  </span>
                </span>
              </div>
              <div v-if="sparkPath" class="insp-kv is-spark">
                <span class="insp-key">Daily</span>
                <span class="insp-val">
                  <svg
                    class="sparkline"
                    viewBox="0 0 120 22"
                    preserveAspectRatio="none"
                    role="img"
                    :aria-label="`${pageviewSeries.length}-day daily pageviews`"
                  >
                    <path :d="sparkPath" fill="none" stroke="currentColor" stroke-width="1.25" />
                  </svg>
                </span>
              </div>
              <!-- Where the traffic came from (utm_source). The tagged share
                   links live under Share › Copy link. -->
              <div v-if="liveUrl && postSources.length" class="insp-kv">
                <span class="insp-key">From</span>
                <span class="insp-val sources" title="Visits by utm_source, last 30 days">
                  <span v-for="s in postSources.slice(0, 5)" :key="s.x" class="source">
                    {{ s.x }} <strong>{{ fmtCount(s.y) }}</strong>
                  </span>
                </span>
              </div>
            </template>

            <!-- Backlinks -->
            <div v-if="backlinks.length || loadingBacklinks" class="insp-kv">
              <span class="insp-key">Backlinks</span>
              <span class="insp-val" :class="{ 'is-muted': loadingBacklinks }">
                {{ loadingBacklinks ? '…' : backlinks.length }}
              </span>
            </div>
            <!-- Click jumps to the linking note; right-click for Obsidian / Finder / copy. -->
            <div v-if="!loadingBacklinks && backlinks.length" class="backlinks-list">
              <button
                v-for="link in backlinks"
                :key="link.path"
                class="backlink-item"
                type="button"
                :title="link.path"
                @click="emit('jump-to-path', link.path)"
                @contextmenu="showBacklinkMenu(link, $event)"
              >
                <span class="backlink-title">{{ link.title || link.path }}</span>
                <span v-if="link.context" class="backlink-context">{{ link.context }}</span>
              </button>
            </div>
            </MetadataPanel>
          </div>
        </section>

        <!-- What to fix (only when there are issues) -->
        <LintReceipt :warnings="lintWarnings" />

        <!-- Social card. Shows for any post with a usable slug, not just live
             ones — picking an OG before publish is part of the publish flow. -->
        <InlineTip v-if="socialCardTip.show.value" class="insp-tip" @close="socialCardTip.close()">
          The social card is the picture people see when this link is shared; open Social card below to
          pick one.
        </InlineTip>
        <OgImagePicker v-if="slug && file.content_type !== 'dispatch'" :slug="slug" @picked="() => {}" />
      </aside>
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

    <!-- Syndication Wizard Modal -->
    <SyndicationWizard
      v-if="showSyndicationWizard && liveUrl"
      :post-url="liveUrl"
      :title="title"
      :slug="slug"
      :source-path="file.path"
      :dek="file.dek"
      :tags="file.tags"
      :content-type="file.content_type"
      :visibility="isPasswordProtected ? 'protected' : isUnlisted ? 'unlisted' : 'public'"
      :quick="autoWizard"
      @close="onWizardClosed"
      @queued="onSyndicationQueued"
      @syndicated="onWizardSyndicated"
    />

    <!-- Draft gate: a `draft: true` piece asks before publishing. -->
    <div v-if="showDraftPrompt" class="draft-overlay" @click.self="showDraftPrompt = false">
      <div class="draft-modal">
        <div class="draft-title">This piece is still a draft</div>
        <div class="draft-body">
          Its frontmatter says <code>draft: true</code>. Remove that and continue to publish?
        </div>
        <div class="draft-actions">
          <UiButton variant="plain" @click="showDraftPrompt = false">Not yet <kbd>esc</kbd></UiButton>
          <UiButton variant="primary" :disabled="clearingDraft" @click="clearDraftAndContinue">
            {{ clearingDraft ? 'Clearing…' : 'Remove draft & publish' }} <kbd>⏎</kbd>
          </UiButton>
        </div>
      </div>
    </div>

    <!-- Publish Confirmation -->
    <PublishConfirmModal
      :show="showPublishConfirm"
      :file="file"
      :target-id="getActiveTargetId() ?? null"
      :mode="publishConfirmMode"
      @close="closePublishConfirm"
      @confirm="(o) => publishWithPrep(o.isUpdate, { webmentions: o.webmentions, hiddenUntil: o.hiddenUntil }, publishConfirmMode)"
    />
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

/* ── Two columns: the post and the inspector ───────────────────────────── */
.insp-mini {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  padding: var(--space-2) var(--gutter);
  border-bottom: 1px solid var(--separator);
  flex-shrink: 0;
}
.insp-mini-word {
  font-size: 12px;
  color: var(--label);
  white-space: nowrap;
}
.insp-mini-spacer {
  flex: 1;
}

.preview-split {
  flex: 1;
  display: flex;
  align-items: stretch;
  min-height: 0;
  min-width: 0;
  position: relative;
}

/* The post. Scrolls on its own; wide code blocks scroll inside themselves
   (rendered-content.css), so this column never scrolls sideways. */
.preview {
  flex: 1;
  min-width: 0;
  overflow-y: auto;
  overflow-x: hidden;
  padding: var(--space-4) var(--gutter) var(--space-6);
  scroll-behavior: smooth;
}

/* The inspector: fixed width, its own scroll, one hairline on the left.
   Sections are .insp-section (global.css) — padding and hairlines come from
   there so every section shares one left edge. */
.inspector {
  flex: 0 0 auto;
  min-width: 0;
  display: flex;
  flex-direction: column;
  overflow-y: auto;
  overflow-x: hidden;
  border-left: 1px solid var(--separator);
  background: var(--bg-solid);
}
/* First-use tips that sit between inspector sections. */
.insp-tip {
  flex-shrink: 0;
  margin: var(--space-3) var(--gutter) 0;
}
.mini-tip {
  flex-shrink: 0;
  margin: var(--space-2) var(--space-3) 0;
}
.insp-inline-tip {
  margin-top: var(--space-1);
}

/* The last section's hairline would double the panel's bottom edge. */
.inspector > :last-child {
  border-bottom: none;
}

/* Narrow pane: the inspector floats over the post instead of taking a
   column. useInspector() flips `visible` for this case; the toggle in the
   tab strip opens and closes it. */
.preview-split.is-narrow .inspector {
  position: absolute;
  top: 0;
  right: 0;
  bottom: 0;
  z-index: 5;
  box-shadow: var(--shadow-lg);
}

/* Webmention chip inside the Status section: drop its standalone strip look. */
.inspector :deep(.wm-chip) {
  padding: 0;
  border: none;
  background: none;
}

/* The diff sits above the post, inside the reading column's gutters. */
.preview-diff {
  margin: 0 0 var(--space-4);
  border-radius: 6px;
  overflow: hidden;
}

/* No .panel.live wash: the preview is the reader's view and carries no
   status color. Status lives in the inspector and the list row. */

/* Key/value rows that end in a small button. */
.insp-list {
  display: flex;
  flex-direction: column;
  gap: var(--space-1);
  min-width: 0;
}
.insp-kv-action {
  align-items: center;
}
.insp-kv-action > .insp-val {
  line-height: 22px;
}
.insp-kv-action > .ui-btn {
  flex-shrink: 0;
}
.is-attention-text {
  color: var(--ink-attention);
}
.is-muted-text {
  color: var(--label-2);
}

/* Analytics rows */
.insp-kv.is-spark {
  align-items: center;
}
.sparkline {
  display: block;
  width: 100%;
  height: 22px;
  color: var(--label-2);
}
.sources {
  display: flex;
  flex-wrap: wrap;
  gap: 2px var(--space-2);
  color: var(--label-2);
}
.sources .source strong {
  color: var(--label);
  font-weight: 600;
}

/* Backlinks */
.backlinks-list {
  display: flex;
  flex-direction: column;
  gap: var(--space-1);
  min-width: 0;
}
.backlink-item {
  display: flex;
  flex-direction: column;
  align-items: stretch;
  gap: 1px;
  padding: 4px 6px;
  background: var(--fill);
  border: none;
  border-radius: 4px;
  min-width: 0;
  width: 100%;
  font: inherit;
  text-align: left;
  color: inherit;
  cursor: pointer;
}
.backlink-item:hover {
  background: var(--fill-2);
}
.backlink-title {
  font-size: 11px;
  font-weight: 500;
  color: var(--label);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}
.backlink-context {
  font-size: 9px;
  color: var(--text-tertiary);
  font-family: 'SF Mono', monospace;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

/* Schedule Picker (stacked: the datetime input is wider than a row allows) */
.schedule-picker {
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
  padding-top: var(--space-2);
  border-top: 1px solid var(--separator);
  min-width: 0;
}
.schedule-input {
  width: 100%;
  min-width: 0;
  height: 28px;
  font-size: 12px;
  font-family: 'SF Mono', monospace;
  background: var(--control-bg);
  border: 1px solid var(--separator);
  border-radius: 6px;
  padding: 0 var(--space-2);
  color: var(--label);
  color-scheme: dark;
}
@media (prefers-color-scheme: light) {
  .schedule-input {
    color-scheme: light;
  }
}
.schedule-actions {
  display: flex;
  justify-content: flex-end;
  gap: var(--space-2);
}

/* Header */
.header {
  padding: 0 0 var(--space-2);
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
  color: var(--label);
  padding: 10px 18px;
  border-radius: 10px;
  font-size: 13px;
  font-weight: 600;
  z-index: 100;
  border: 1px solid var(--separator);
  box-shadow: var(--shadow-md);
  backdrop-filter: blur(20px);
  -webkit-backdrop-filter: blur(20px);
  display: flex;
  align-items: center;
  gap: 8px;
  white-space: nowrap;
}

/* Neutral card; only the check glyph is green. */
.success-toast svg {
  color: var(--status-live);
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
    box-shadow: inset 0 0 24px var(--tint-live);
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
  box-shadow: var(--shadow-md);
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

/* Contact-sheet gallery */
.photo-gallery {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(96px, 1fr));
  gap: 4px;
  padding: 0 0 var(--space-3);
}
.photo-gallery .gallery-cell {
  aspect-ratio: 1;
  overflow: hidden;
  border-radius: 4px;
  background: color-mix(in srgb, var(--text-primary) 5%, transparent);
}
.photo-gallery .gallery-cell img {
  width: 100%;
  height: 100%;
  object-fit: cover;
  display: block;
  transition: transform 0.15s ease;
}
.photo-gallery .gallery-cell:hover img {
  transform: scale(1.05);
}

.diff-bar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  padding: 6px 16px;
  font-size: 10px;
  letter-spacing: 0.06em;
  text-transform: uppercase;
  color: var(--text-tertiary);
}

.diff-toggle {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  background: none;
  border: 1px solid var(--border);
  border-radius: 3px;
  padding: 1px 6px;
  font-size: 10px;
  color: var(--text-secondary);
  cursor: pointer;
}
.diff-toggle kbd {
  font-family: 'SF Mono', monospace;
  font-size: 9px;
  color: var(--text-tertiary);
}
.diff-toggle:hover:not(:disabled) {
  background: var(--hover-bg);
  color: var(--text-primary);
}
.diff-toggle.on {
  background: var(--text-primary);
  border-color: var(--text-primary);
  color: var(--bg-primary);
}
.diff-toggle.on kbd {
  color: inherit;
  opacity: 0.6;
}
.diff-toggle:disabled {
  opacity: 0.4;
  cursor: default;
}

.diff-summary {
  display: flex;
  flex-direction: column;
  gap: 2px;
  padding: 6px 16px;
  border-bottom: 1px solid var(--border);
  font-size: 11px;
  color: var(--text-secondary);
}
.diff-summary-num {
  font-family: 'SF Mono', monospace;
  font-variant-numeric: tabular-nums;
  color: var(--text-primary);
  font-weight: 600;
}
.diff-summary-warn {
  color: var(--text-primary);
}
.diff-summary .muted {
  color: var(--text-tertiary);
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
    var(--fill-2) 50%,
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
    color-mix(in srgb, var(--text-secondary) 24%, var(--hover-bg)) 50%,
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
    var(--fill-2) 50%,
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

/* Concurrent cross-fade (no mode="out-in" — the leaving element fading
 * before the entering element could even start was costing ~320ms of
 * mandatory delay on every file switch, even with sub-50ms content
 * processing. Now they overlap. Shorter duration too — 100ms feels
 * snappier without losing the visual cue. */
.preview-fade-enter-active,
.preview-fade-leave-active {
  transition: opacity 0.10s ease;
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
  margin: calc(-1 * var(--space-4)) calc(-1 * var(--gutter)) var(--space-2);
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
    var(--label-3) 50%,
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

/* Render-error card — never silent again. Soft warning wash, monospace
 * stage label, full file path so the user can locate the offending file
 * outside the app if needed. */
.render-error-card {
  margin: 0 0 var(--space-3);
  padding: 10px 12px;
  border-radius: 6px;
  /* A render failure is broken, not "needs attention". */
  background: var(--tint-broken);
  border-left: 3px solid var(--status-broken);
  font-size: 11px;
  line-height: 1.45;
  color: var(--text-secondary);
}
.render-error-stage {
  font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
  font-size: 9.5px;
  letter-spacing: 0.05em;
  text-transform: uppercase;
  color: var(--label);
  margin-bottom: 2px;
}
.render-error-msg {
  color: var(--text-primary);
}
.render-error-path {
  margin-top: 4px;
  font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
  font-size: 10px;
  color: var(--text-tertiary);
  word-break: break-all;
}

/* Draft gate */
.draft-overlay {
  position: fixed;
  inset: 0;
  z-index: 210;
  background: var(--scrim);
  backdrop-filter: blur(4px);
  display: flex;
  align-items: center;
  justify-content: center;
}
.draft-modal {
  width: 380px;
  max-width: 92vw;
  background: var(--modal-bg);
  border: 1px solid var(--border-light);
  border-radius: 12px;
  padding: 18px;
  box-shadow: var(--shadow-lg);
}
.draft-title {
  font-size: 15px;
  font-weight: 650;
  margin-bottom: 6px;
}
.draft-body {
  font-size: 12.5px;
  line-height: 1.5;
  color: var(--text-secondary);
}
.draft-actions {
  display: flex;
  justify-content: flex-end;
  gap: var(--space-2);
  margin-top: var(--space-4);
}
.draft-actions kbd {
  font-size: 9px;
  opacity: 0.75;
  margin-left: 4px;
}

</style>

<!-- Non-scoped rendered content styles (v-html content is not affected by scoped CSS) -->
<style src="../styles/rendered-content.css"></style>
