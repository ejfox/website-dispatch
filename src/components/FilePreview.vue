<script setup lang="ts">
import { ref, watch, computed, nextTick, markRaw } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { PhCheckCircle, PhLinkSimple, PhImageSquare, PhTextAa, PhTrophy, PhCaretDown } from '@phosphor-icons/vue'
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

function selectTarget(id: string) {
  selectedTargetId.value = id
}

const content = ref('')
const renderedContent = ref('')
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

// Image / video breakdown of localMedia — surfaced to AltTextReviewer
// so its empty state can give a real next-step ("upload N images") instead
// of a dead-end "0 images found" message.
const localImageCount = computed(() => localMedia.value.filter((m) => m.media_type !== 'video').length)
const localVideoCount = computed(() => localMedia.value.filter((m) => m.media_type === 'video').length)

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

watch(
  () => props.file,
  async (file) => {
    if (!file) return

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
    loadingBacklinks.value = true
    loadingLocalMedia.value = true
    loadingStats.value = !!file.published_url

    // Fire-and-forget: preview servers don't gate any UI render.
    fetch('http://127.0.0.1:6419/set-file', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ path: file.path }),
    }).catch(() => {})
    invoke('set_preview_file', { path: file.path })

    // Kick off ALL IPC calls in parallel — none of them depend on each
    // other, and we want the metadata header to paint as soon as Vue's
    // reactivity flushes (which happens before any of these resolve).
    //
    // We DON'T await the whole batch as a unit; each ref updates as its
    // own request returns, so the user sees a progressive fill instead
    // of a wait-then-flash render.

    invoke('get_file_content', { path: file.path })
      .then(async (raw) => {
        const stripped = (raw as string).replace(/^---\n[\s\S]*?\n---\n*/, '')
        content.value = stripped
        // Yield a frame so the unstyled-content paint can happen, then
        // run the heavy unified processor. For long posts this turns a
        // ~120ms stall into a ~16ms first-paint.
        await nextTick()
        try {
          const result = await markdownProcessor.value.process(stripped)
          renderedContent.value = String(result)
          nextTick(() => renderMermaidIn(document))
        } catch {
          renderedContent.value = ''
        }
        // Tag analysis depends on content; run it after first paint.
        await fetchAvailableTags()
        suggestedTags.value = analyzeTags(stripped, file.tags || [])
      })
      .catch((e) => {
        content.value = `Error: ${e}`
        renderedContent.value = ''
      })

    invoke('get_backlinks', { filename: file.filename })
      .then((res) => {
        backlinks.value = res as Backlink[]
      })
      .catch((e) => console.log('Backlinks unavailable:', e))
      .finally(() => {
        loadingBacklinks.value = false
      })

    invoke('get_local_media', { path: file.path })
      .then((res) => {
        localMedia.value = res as LocalMediaRef[]
      })
      .catch((e) => console.log('Local media detection unavailable:', e))
      .finally(() => {
        loadingLocalMedia.value = false
      })

    if (file.published_url) {
      invoke('get_post_analytics', { url: file.published_url, days: 30 })
        .then((res) => {
          postStats.value = res as PostAnalytics
        })
        .catch(() => {
          postStats.value = null
        })
        .finally(() => {
          loadingStats.value = false
        })
      invoke('get_post_pageview_series', { url: file.published_url, days: 30 })
        .then((res) => {
          pageviewSeries.value = (res as number[]) || []
        })
        .catch(() => {
          pageviewSeries.value = []
        })
    }
  },
  { immediate: true },
)

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
      @close="showMediaFixer = false"
      @fixed="onLocalMediaFixed"
      @open-alt-text="onOpenAltTextFromFixer"
    />

    <!-- Toolbar -->
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

    <!-- Content Divider -->
    <div class="content-divider">
      <span>
        <PhTextAa :size="10" weight="duotone" />
        CONTENT
      </span>
    </div>

    <!-- Preview -->
    <div class="preview">
      <div v-if="renderedContent" class="rendered-content" v-html="renderedContent"></div>
      <pre v-else>{{ content }}</pre>
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
  font-size: 15px;
  font-weight: 600;
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
  position: absolute;
  top: 50%;
  left: 50%;
  transform: translate(-50%, -50%);
  background: var(--success);
  color: #000;
  padding: 16px 32px;
  border-radius: 12px;
  font-size: 16px;
  font-weight: 600;
  z-index: 100;
  box-shadow:
    0 8px 32px rgba(48, 209, 88, 0.4),
    0 0 0 1px rgba(48, 209, 88, 0.2);
  display: flex;
  align-items: center;
  gap: 8px;
  white-space: nowrap;
}

.success-toast.milestone {
  background: linear-gradient(135deg, var(--warning), var(--warning));
  color: #000;
  padding: 20px 36px;
  border-radius: 14px;
  font-size: 17px;
  box-shadow:
    0 8px 40px rgba(245, 158, 11, 0.45),
    0 0 0 1px rgba(245, 158, 11, 0.3),
    0 0 80px rgba(245, 158, 11, 0.1);
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

/* Panel glow on publish */
.panel.just-published {
  animation: successGlow 1.2s ease-out forwards;
}

@keyframes successGlow {
  0% {
    box-shadow: inset 0 0 60px color-mix(in srgb, var(--success) 30%, transparent);
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

/* Sequence pill — "step 1 of 2" / "step 2 of 2" — only renders when both
   sections are visible so a single-step flow stays uncluttered. */
.step-pill {
  margin-left: 6px;
  padding: 1px 6px;
  font-size: 9px;
  font-weight: 500;
  letter-spacing: 0.04em;
  text-transform: uppercase;
  color: var(--text-tertiary);
  background: var(--bg-tertiary);
  border-radius: 999px;
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

/* Schedule Picker Buttons */
.btn {
  padding: 8px 16px;
  border: none;
  border-radius: 6px;
  font-size: 11px;
  font-weight: 500;
  cursor: pointer;
  text-decoration: none;
  background: var(--bg-tertiary);
  color: var(--text-primary);
  transition: all 0.15s cubic-bezier(0.34, 1.56, 0.64, 1);
  min-height: 28px;
  display: inline-flex;
  align-items: center;
  gap: 4px;
}

.btn:hover {
  filter: brightness(1.1);
  transform: translateY(-1px);
}
.btn:active {
  transform: translateY(0);
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
</style>

<!-- Non-scoped rendered content styles (v-html content is not affected by scoped CSS) -->
<style src="../styles/rendered-content.css"></style>
