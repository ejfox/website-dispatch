<script setup lang="ts">
import { ref, watch, computed, nextTick } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { PhCheckCircle, PhLinkSimple, PhImageSquare, PhTextAa, PhTrophy } from '@phosphor-icons/vue'
import LintReceipt from './LintReceipt.vue'
import LocalMediaSection from './LocalMediaSection.vue'
import AltTextReviewer from './AltTextReviewer.vue'
import SyndicationWizard from './SyndicationWizard.vue'
import OgImagePicker from './OgImagePicker.vue'
import StatusBanner from './StatusBanner.vue'
import MetadataPanel from './MetadataPanel.vue'
import ActionToolbar from './ActionToolbar.vue'
import PublishConfirmModal from './PublishConfirmModal.vue'
import WebmentionReportComponent from './WebmentionReport.vue'
import { unified } from 'unified'
import remarkParse from 'remark-parse'
import remarkGfm from 'remark-gfm'
import remarkRehype from 'remark-rehype'
import rehypeRaw from 'rehype-raw'
import rehypeStringify from 'rehype-stringify'
import { useLocalStorage } from '@vueuse/core'
import type { MarkdownFile, Backlink, LocalMediaRef, PostAnalytics } from '../types'
import { useTagSuggestions } from '../composables/useTagSuggestions'
import { usePublishing } from '../composables/usePublishing'
import { usePostActions } from '../composables/usePostActions'
import { useAppConfig } from '../composables/useAppConfig'
import { useGitStatus } from '../composables/useGitStatus'

const markdownProcessor = unified()
  .use(remarkParse)
  .use(remarkGfm)
  .use(remarkRehype, { allowDangerousHtml: true })
  .use(rehypeRaw)
  .use(rehypeStringify)

const props = defineProps<{ file: MarkdownFile }>()
const emit = defineEmits<{ published: [] }>()

// Config (shared singleton)
const { appConfig, enabledEditors, publishTargets, hasMultipleTargets } = useAppConfig()
const selectedTargetId = useLocalStorage<string | null>('dispatch-target', null)

function getActiveTargetId(): string | undefined {
  if (!hasMultipleTargets.value) return undefined
  return selectedTargetId.value || undefined
}

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
})

// Tag suggestions composable
const { availableTags, suggestedTags, addingTag, fetchAvailableTags, analyzeTags, addTag } = useTagSuggestions({
  getFilePath: () => props.file.path,
  getFileTags: () => props.file.tags,
  onFeedback: showCopyFeedback,
  onRefresh: () => emit('published'),
})

watch(
  () => props.file,
  async (file) => {
    if (!file) return

    // Wait for next tick to ensure component is fully mounted before invoking Tauri
    await nextTick()

    justPublished.value = null
    backlinks.value = []
    localMedia.value = []
    postStats.value = null
    showMediaFixer.value = false
    webmentionReport.value = null

    // Set this file as the preview target (Node server for accurate rendering)
    fetch('http://127.0.0.1:6419/set-file', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ path: file.path }),
    }).catch(() => {
      /* preview server may not be running - non-critical */
    })

    // Also tell Rust server (fallback)
    invoke('set_preview_file', { path: file.path })

    try {
      const raw: string = await invoke('get_file_content', { path: file.path })
      // Strip YAML frontmatter from preview since metadata is shown above
      content.value = raw.replace(/^---\n[\s\S]*?\n---\n*/, '')
      // Render markdown to HTML
      try {
        const result = await markdownProcessor.process(content.value)
        renderedContent.value = String(result)
      } catch {
        renderedContent.value = ''
      }
    } catch (e) {
      content.value = `Error: ${e}`
      renderedContent.value = ''
    }

    // Fetch backlinks + local media + analytics in parallel
    loadingBacklinks.value = true
    loadingLocalMedia.value = true
    loadingStats.value = !!file.published_url
    await Promise.all([
      invoke('get_backlinks', { filename: file.filename })
        .then((res) => {
          backlinks.value = res as Backlink[]
        })
        .catch((e) => {
          console.log('Backlinks unavailable:', e)
        })
        .finally(() => {
          loadingBacklinks.value = false
        }),
      invoke('get_local_media', { path: file.path })
        .then((res) => {
          localMedia.value = res as LocalMediaRef[]
        })
        .catch((e) => {
          console.log('Local media detection unavailable:', e)
        })
        .finally(() => {
          loadingLocalMedia.value = false
        }),
      ...(file.published_url
        ? [
            invoke('get_post_analytics', { url: file.published_url, days: 30 })
              .then((res) => {
                postStats.value = res as PostAnalytics
              })
              .catch(() => {
                postStats.value = null
              })
              .finally(() => {
                loadingStats.value = false
              }),
          ]
        : []),
    ])

    // Analyze content for tag suggestions
    await fetchAvailableTags()
    suggestedTags.value = analyzeTags(content.value, file.tags || [])
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
// year prefix to find the JSON. Falls back to bare filename for posts that
// aren't in a year directory (rare/legacy).
const slug = computed(() => {
  const baseName = props.file.filename.replace('.md', '')
  const yearMatch = props.file.path.match(/\/blog\/(\d{4})\//)
  return yearMatch ? `${yearMatch[1]}/${baseName}` : baseName
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
      @copy-url="copyUrl"
      @copy-url-password="copyUrlAndPassword"
      @republish="publish(true)"
      @cancel-schedule="cancelSchedule"
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

    <!-- Alt Text -->
    <div v-if="missingAltTextCount > 0" class="alt-text-section">
      <div class="alt-text-header">
        <span class="label">
          <PhImageSquare :size="10" weight="duotone" />
          Alt Text
        </span>
        <span class="count warning">{{ missingAltTextCount }}</span>
        <button @click="showAltTextReviewer = true" class="fix-btn">Describe</button>
      </div>
      <div class="alt-text-hint">{{ missingAltTextCount }} image(s) need descriptions</div>
    </div>

    <!-- Alt Text Reviewer Modal -->
    <AltTextReviewer
      v-if="showAltTextReviewer"
      :file-path="file.path"
      :count="missingAltTextCount"
      @close="showAltTextReviewer = false"
      @applied="onAltTextApplied"
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
      <div v-else class="backlinks-list">
        <div v-for="link in backlinks" :key="link.path" class="backlink-item">
          <span class="backlink-title">{{ link.title || link.path }}</span>
          <span v-if="link.context" class="backlink-context">{{ link.context }}</span>
        </div>
      </div>
    </div>

    <!-- OG Image (only after publish) -->
    <OgImagePicker v-if="isLive" :slug="slug" @picked="() => {}" />

    <!-- Local Media -->
    <LocalMediaSection
      :local-media="localMedia"
      :loading-local-media="loadingLocalMedia"
      :show-media-fixer="showMediaFixer"
      :file-path="file.path"
      @show-fixer="showMediaFixer = true"
      @close-fixer="showMediaFixer = false"
      @media-fixed="$emit('published')"
    />

    <!-- Toolbar -->
    <ActionToolbar
      :enabled-editors="enabledEditors"
      :publish-targets="publishTargets"
      :has-multiple-targets="hasMultipleTargets"
      :selected-target-id="selectedTargetId"
      :is-live="isLive"
      :live-url="liveUrl"
      :sending-webmentions="sendingWebmentions"
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
      @trigger-webmentions="triggerWebmentions(false)"
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

    <!-- Webmention Results -->
    <WebmentionReportComponent
      v-if="webmentionReport"
      :report="webmentionReport"
      :sending-webmentions="sendingWebmentions"
      @close="webmentionReport = null"
      @resend-bridgy="triggerWebmentions(true)"
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
  backdrop-filter: blur(12px);
  -webkit-backdrop-filter: blur(12px);
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
  background: linear-gradient(135deg, #f59e0b, #f97316);
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

/* Alt Text */
.alt-text-section {
  padding: 8px 16px;
  border-bottom: 1px solid var(--border);
}

.alt-text-header {
  display: flex;
  align-items: center;
  gap: 8px;
  margin-bottom: 4px;
}

.alt-text-hint {
  font-size: 10px;
  color: var(--text-tertiary);
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
</style>

<!-- Non-scoped rendered content styles (v-html content is not affected by scoped CSS) -->
<style src="../styles/rendered-content.css"></style>
