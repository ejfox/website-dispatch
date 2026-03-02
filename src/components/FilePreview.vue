<script setup lang="ts">
import { ref, watch, computed, onBeforeUnmount, nextTick } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { Command } from 'lucide-vue-next'
import LocalMediaFixer from './LocalMediaFixer.vue'
import { unified } from 'unified'
import remarkParse from 'remark-parse'
import remarkGfm from 'remark-gfm'
import remarkRehype from 'remark-rehype'
import rehypeRaw from 'rehype-raw'
import rehypeStringify from 'rehype-stringify'

const markdownProcessor = unified()
  .use(remarkParse)
  .use(remarkGfm)
  .use(remarkRehype, { allowDangerousHtml: true })
  .use(rehypeRaw)
  .use(rehypeStringify)

interface MarkdownFile {
  path: string
  filename: string
  title: string | null
  dek: string | null
  date: string | null
  tags: string[]
  created: number
  modified: number
  word_count: number
  is_safe: boolean
  warnings: string[]
  published_url: string | null
  published_date: number | null
  source_dir: string
  unlisted: boolean
  password: string | null
  publish_at: string | null
  content_type: string
}

interface Backlink {
  path: string
  title: string | null
  context: string
}

interface LocalMediaRef {
  original_text: string
  path: string
  resolved_path: string | null
  alt_text: string | null
  media_type: string
  line_number: number
}

interface EditorConfig { name: string; app_name: string; enabled: boolean }
interface PublishTargetConfig { id: string; name: string; is_default: boolean }
interface AppConfig {
  default_editor: string
  editors: EditorConfig[]
  publish_targets: PublishTargetConfig[]
}

const props = defineProps<{ file: MarkdownFile }>()
const emit = defineEmits<{ published: [] }>()

// Config
const appConfig = ref<AppConfig | null>(null)
const selectedTargetId = ref<string | null>(localStorage.getItem('dispatch-target') || null)

invoke('get_app_config').then((c) => { appConfig.value = c as AppConfig }).catch(() => {})

const enabledEditors = computed(() =>
  (appConfig.value?.editors || []).filter(e => e.enabled)
)

const publishTargets = computed(() => appConfig.value?.publish_targets || [])
const hasMultipleTargets = computed(() => publishTargets.value.length > 1)

function getActiveTargetId(): string | undefined {
  if (!hasMultipleTargets.value) return undefined
  return selectedTargetId.value || undefined
}

function selectTarget(id: string) {
  selectedTargetId.value = id
  localStorage.setItem('dispatch-target', id)
}

interface GitStatus {
  ok: boolean
  branch: string
  error: string | null
  dirty_files: string[]
  has_conflicts: boolean
}

const content = ref('')
const renderedContent = ref('')
const publishing = ref(false)
const justPublished = ref<string | null>(null)
const backlinks = ref<Backlink[]>([])
const loadingBacklinks = ref(false)
const obsidianConnected = ref(false)
const gitStatus = ref<GitStatus | null>(null)
const showSuccess = ref(false)
const successMessage = ref('')
const copyFeedback = ref<string | null>(null)
const unpublishing = ref(false)
const showPublishConfirm = ref(false)
const publishConfirmStep = ref<1 | 2>(1)
const publishConfirmChecked = ref(false)
const publishConfirmText = ref('')
const publishConfirmRepublish = ref(false)
const localMedia = ref<LocalMediaRef[]>([])
const loadingLocalMedia = ref(false)
const showMediaFixer = ref(false)
const justPublishedGlow = ref(false)
const metadataExpanded = ref(false)

// Tag suggestions
const availableTags = ref<Record<string, number>>({})
const suggestedTags = ref<string[]>([])
const loadingTags = ref(false)

// Fetch available tags from website
async function fetchAvailableTags() {
  if (Object.keys(availableTags.value).length > 0) return // Already loaded
  loadingTags.value = true
  try {
    // Use config domain for fetching tags
    const defaultTarget = (appConfig.value as any)?.publish_targets?.find((t: any) => t.is_default)
    const domain = defaultTarget?.domain || 'https://ejfox.com'
    const res = await fetch(`${domain}/content-tags.json`)
    availableTags.value = await res.json()
  } catch (e) {
    console.log('Could not fetch tags:', e)
  }
  loadingTags.value = false
}

// Analyze content and suggest tags based on keyword matches
function analyzeTags(text: string, existingTags: string[]): string[] {
  const tags = Object.keys(availableTags.value)
  if (tags.length === 0) return []

  const textLower = text.toLowerCase()
  const suggestions: { tag: string; score: number }[] = []

  // Keywords that map to tags (tag -> keywords to look for)
  const tagKeywords: Record<string, string[]> = {
    'politics': ['trump', 'biden', 'election', 'congress', 'democrat', 'republican', 'vote', 'political', 'government', 'ice', 'immigration'],
    'coding': ['code', 'programming', 'javascript', 'python', 'typescript', 'function', 'api', 'software', 'developer'],
    'photography': ['photo', 'camera', 'lens', 'shot', 'film', 'photograph'],
    'art': ['painting', 'drawing', 'canvas', 'artist', 'creative', 'sketch'],
    'music': ['song', 'album', 'band', 'guitar', 'piano', 'spotify', 'playlist'],
    'design': ['design', 'ui', 'ux', 'interface', 'layout', 'figma'],
    'writing': ['write', 'essay', 'blog', 'draft', 'author', 'story'],
    'personal': ['i feel', 'my life', 'personal', 'journal', 'diary', 'reflection'],
    'activism': ['protest', 'march', 'activist', 'movement', 'justice', 'rights'],
    'travel': ['travel', 'trip', 'flight', 'hotel', 'vacation', 'city', 'country'],
    'video': ['video', 'film', 'documentary', 'youtube', 'cinema'],
    'javascript': ['javascript', 'js', 'node', 'npm', 'react', 'vue'],
    'dataviz': ['visualization', 'chart', 'graph', 'd3', 'data viz'],
    'security': ['security', 'hack', 'encrypt', 'password', 'vulnerability'],
  }

  for (const tag of tags) {
    // Skip if already has this tag
    if (existingTags.map(t => t.toLowerCase()).includes(tag.toLowerCase())) continue

    let score = 0

    // Direct tag name match in content
    if (textLower.includes(tag.toLowerCase())) {
      score += 3
    }

    // Check keyword mappings
    const keywords = tagKeywords[tag] || []
    for (const keyword of keywords) {
      if (textLower.includes(keyword)) {
        score += 2
      }
    }

    // Boost by usage count (popular tags slightly preferred)
    const usageBoost = Math.min(availableTags.value[tag] / 50, 0.5)
    score += usageBoost

    if (score > 0) {
      suggestions.push({ tag, score })
    }
  }

  // Sort by score and return top 5
  return suggestions
    .sort((a, b) => b.score - a.score)
    .slice(0, 5)
    .map(s => s.tag)
}

// Add tag to file's frontmatter
const addingTag = ref(false)
async function addTag(tag: string) {
  if (addingTag.value) return
  addingTag.value = true
  try {
    await invoke('add_tag_to_file', { path: props.file.path, tag })
    showCopyFeedback(`Added: ${tag}`)
    // Remove from suggestions since it's now in the file
    suggestedTags.value = suggestedTags.value.filter(t => t !== tag)
    // Trigger refresh to update the file's tag list
    emit('published')
  } catch (e) {
    showCopyFeedback(`Failed to add tag`)
    console.error('Failed to add tag:', e)
  }
  addingTag.value = false
}

// Fetch tags on mount
fetchAvailableTags()

watch(() => props.file, async (file) => {
  if (!file) return

  // Wait for next tick to ensure component is fully mounted before invoking Tauri
  await nextTick()

  justPublished.value = null
  backlinks.value = []
  localMedia.value = []
  postStats.value = null
  showMediaFixer.value = false

  // Set this file as the preview target (Node server for accurate rendering)
  fetch('http://127.0.0.1:6419/set-file', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ path: file.path })
  }).catch(() => { /* preview server may not be running - non-critical */ })

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
      .then((res) => { backlinks.value = res as Backlink[] })
      .catch((e) => { console.log('Backlinks unavailable:', e) })
      .finally(() => { loadingBacklinks.value = false }),
    invoke('get_local_media', { path: file.path })
      .then((res) => { localMedia.value = res as LocalMediaRef[] })
      .catch((e) => { console.log('Local media detection unavailable:', e) })
      .finally(() => { loadingLocalMedia.value = false }),
    ...(file.published_url ? [
      invoke('get_post_analytics', { url: file.published_url, days: 30 })
        .then((res) => { postStats.value = res as PostAnalytics })
        .catch(() => { postStats.value = null })
        .finally(() => { loadingStats.value = false }),
    ] : []),
  ])

  // Analyze content for tag suggestions
  await fetchAvailableTags()
  suggestedTags.value = analyzeTags(content.value, file.tags || [])
}, { immediate: true })

// Check Obsidian API status on mount
invoke('check_obsidian_api').then((connected: unknown) => {
  obsidianConnected.value = connected as boolean
})

// Check git status
async function checkGitStatus() {
  try {
    gitStatus.value = await invoke('get_git_status') as GitStatus
  } catch (e) {
    console.error('Git status check failed:', e)
  }
}
checkGitStatus()
// Refresh git status periodically
const gitStatusInterval = setInterval(checkGitStatus, 10000)
onBeforeUnmount(() => clearInterval(gitStatusInterval))

// Format filename into title, handling date-based names specially
const formatTitle = (filename: string): string => {
  const baseName = filename.replace(/\.md$/, '')
  const datePattern = /^(\d{4}-\d{2}-\d{2})(-.*)?$/
  const dateMatch = baseName.match(datePattern)
  if (dateMatch) {
    const datePart = dateMatch[1]
    const suffix = dateMatch[2]
    if (suffix) {
      const suffixTitle = suffix.slice(1).split('-').map(w => w.charAt(0).toUpperCase() + w.slice(1)).join(' ')
      return `${datePart} ${suffixTitle}`
    }
    return datePart
  }
  return baseName.replace(/-/g, ' ').replace(/\b\w/g, c => c.toUpperCase())
}

const title = computed(() => props.file.title || formatTitle(props.file.filename))
const titleIsDerived = computed(() => !props.file.title)

const slug = computed(() => props.file.filename.replace('.md', ''))

const targetUrl = computed(() => {
  const targets = publishTargets.value
  const target = targets.find(t => t.id === selectedTargetId.value) || targets.find(t => t.is_default) || targets[0]
  const domain = target ? (appConfig.value as any)?.publish_targets?.find((t: any) => t.id === target.id)?.domain?.replace(/^https?:\/\//, '') || 'ejfox.com' : 'ejfox.com'
  return `${domain}/blog/${new Date().getFullYear()}/${slug.value}`
})

const isLive = computed(() => !!props.file.published_url || !!justPublished.value)
const liveUrl = computed(() => props.file.published_url || justPublished.value)
const hasUnpublishedChanges = computed(() => props.file.warnings.includes('Modified since publish'))
const lintWarnings = computed(() =>
  props.file.warnings.filter(w => w !== 'Modified since publish')
)

// Visibility states
const isUnlisted = computed(() => props.file.unlisted || !!props.file.password)
const isPasswordProtected = computed(() => !!props.file.password)
const visibilityLabel = computed(() => {
  if (isPasswordProtected.value) return 'PASSWORD'
  if (isUnlisted.value) return 'UNLISTED'
  return null
})

function formatDate(iso: string): string {
  try {
    const d = new Date(iso)
    return d.toLocaleDateString('en-US', { year: 'numeric', month: 'short', day: 'numeric' })
  } catch { return iso }
}

function formatAgeShort(ts: number): string {
  const seconds = Math.floor(Date.now() / 1000 - ts)
  const minutes = Math.floor(seconds / 60)
  const hours = Math.floor(minutes / 60)
  const days = Math.floor(hours / 24)

  if (seconds < 60) return 'now'
  if (minutes < 60) return `${minutes}m ago`
  if (hours < 24) return `${hours}h ago`
  if (days === 1) return 'yesterday'
  if (days < 7) return `${days}d ago`
  if (days < 30) return `${Math.floor(days / 7)}w ago`
  if (days < 365) return `${Math.floor(days / 30)}mo ago`
  return `${Math.floor(days / 365)}y ago`
}

function formatDateCompact(ts: number): string {
  const d = new Date(ts * 1000)
  return d.toLocaleDateString('en-US', { month: 'short', day: 'numeric', year: 'numeric' })
}

function openPublishConfirm(isRepublish = false) {
  publishConfirmRepublish.value = isRepublish
  publishConfirmStep.value = 1
  publishConfirmChecked.value = false
  publishConfirmText.value = ''
  showPublishConfirm.value = true
}

function closePublishConfirm() {
  showPublishConfirm.value = false
}

async function publish(isRepublish = false) {
  // For fresh publish, require is_safe. For republish, allow it.
  if (!isRepublish && !props.file.is_safe) return
  publishing.value = true
  try {
    const url = await invoke<string>('publish_file', {
      sourcePath: props.file.path,
      slug: slug.value,
      targetId: getActiveTargetId() || null,
    })
    justPublished.value = url

    // Show success toast with visibility context
    const visibilityContext = isPasswordProtected.value ? ' (protected)' :
                              isUnlisted.value ? ' (unlisted)' : ''
    successMessage.value = isRepublish ? 'Republished!' : `Published${visibilityContext}!`
    showSuccess.value = true
    justPublishedGlow.value = true
    setTimeout(() => { showSuccess.value = false }, 3000)
    setTimeout(() => { justPublishedGlow.value = false }, 1500)

    // Delay refresh slightly to let filesystem settle
    setTimeout(() => emit('published'), 500)
  } catch (e) {
    alert(`Failed: ${e}`)
  }
  publishing.value = false
}

async function unpublish() {
  if (!isLive.value || !slug.value || unpublishing.value) return
  const confirmText = `Unpublish "${slug.value}" and move it back to drafts?`
  if (!confirm(confirmText)) return
  unpublishing.value = true
  try {
    await invoke('unpublish_file', { slug: slug.value, targetId: getActiveTargetId() || null })
    successMessage.value = 'Unpublished — moved to drafts'
    showSuccess.value = true
    setTimeout(() => { showSuccess.value = false }, 3000)
    // Refresh list
    setTimeout(() => emit('published'), 500)
  } catch (e) {
    alert(`Failed: ${e}`)
  }
  unpublishing.value = false
}

// Analytics
interface PostAnalytics {
  pageviews: number
  visitors: number
  visits: number
  bounces: number
  totaltime: number
}
const postStats = ref<PostAnalytics | null>(null)
const loadingStats = ref(false)

// Scheduling
const showSchedulePicker = ref(false)
const scheduleDate = ref('')

const isScheduled = computed(() => !!props.file.publish_at && !isLive.value)

function formatScheduledTime(isoStr: string): string {
  try {
    const d = new Date(isoStr)
    return d.toLocaleString('en-US', {
      month: 'short', day: 'numeric', year: 'numeric',
      hour: 'numeric', minute: '2-digit'
    })
  } catch { return isoStr }
}

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

function showCopyFeedback(msg: string) {
  copyFeedback.value = msg
  setTimeout(() => { copyFeedback.value = null }, 1500)
}

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
        {{ successMessage }}
      </div>
    </Transition>

    <!-- Copy Feedback -->
    <Transition name="fade">
      <div v-if="copyFeedback" class="copy-feedback">
        {{ copyFeedback }}
      </div>
    </Transition>

    <!-- Status Banner -->
    <div v-if="isLive && hasUnpublishedChanges" class="banner modified">
      <span class="banner-text">MODIFIED</span>
      <span v-if="visibilityLabel" class="visibility-badge">{{ visibilityLabel }}</span>
      <span>Source changed since last publish</span>
      <button @click="publish(true)" :disabled="publishing">{{ publishing ? '...' : 'Republish' }}</button>
    </div>
    <div v-else-if="isLive && isPasswordProtected" class="banner protected">
      <span class="banner-text">🔒 PROTECTED</span>
      <a :href="liveUrl!" target="_blank">{{ liveUrl }}</a>
      <button @click="copyUrlAndPassword" title="Copy URL + password to share">Copy + Pass</button>
      <button @click="copyUrl" title="Copy URL only">Copy URL</button>
    </div>
    <div v-else-if="isLive && isUnlisted" class="banner unlisted">
      <span class="banner-text">👁 UNLISTED</span>
      <a :href="liveUrl!" target="_blank">{{ liveUrl }}</a>
      <button @click="copyUrl" title="Share this link — not indexed anywhere">Copy</button>
    </div>
    <div v-else-if="isLive" class="banner live">
      <span class="banner-text">✓ LIVE</span>
      <a :href="liveUrl!" target="_blank">{{ liveUrl }}</a>
      <button @click="copyUrl">Copy</button>
    </div>
    <div v-else-if="isScheduled" class="banner scheduled">
      <span class="banner-text">⏱ SCHEDULED</span>
      <span>{{ formatScheduledTime(file.publish_at!) }}</span>
      <button @click="cancelSchedule">Cancel</button>
    </div>
    <div v-else-if="!file.is_safe" class="banner warn">
      {{ file.warnings.join(' · ') }}
    </div>
    <div v-else-if="isPasswordProtected" class="banner ready protected-ready">
      <span class="visibility-badge">🔒 PASSWORD</span>
      <span class="visibility-hint">Link + password required to view</span>
    </div>
    <div v-else-if="isUnlisted" class="banner ready unlisted-ready">
      <span class="visibility-badge">👁 UNLISTED</span>
      <span class="visibility-hint">Link only — won't appear in listings or feeds</span>
    </div>
    <div v-else class="banner ready public-ready">
      <span class="visibility-badge">✓ PUBLIC</span>
      <span class="visibility-hint">Will appear in listings, feeds, and search</span>
    </div>

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

    <!-- Info -->
    <div class="info">
      <!-- Summary bar (always visible, clickable) -->
      <div class="info-summary" @click="metadataExpanded = !metadataExpanded">
        <span class="info-toggle" :class="{ expanded: metadataExpanded }">&#9654;</span>
        <span>{{ file.word_count }}w</span>
        <span class="date-sep">&middot;</span>
        <span>{{ file.tags.length }} tags</span>
        <span class="date-sep">&middot;</span>
        <span>{{ file.date ? formatDate(file.date) : formatDateCompact(file.created) }}</span>
      </div>

      <!-- Detail rows (collapsed by default) -->
      <div v-show="metadataExpanded" class="info-detail">
        <div v-if="file.content_type === 'weeknote'" class="row">
          <span class="label">Type</span>
          <span class="weeknote-type">Week Note</span>
        </div>
        <div class="row">
          <span class="label">Source</span>
          <code>{{ file.source_dir || '.' }}/{{ file.filename }}</code>
        </div>
        <div class="row">
          <span class="label">Dates</span>
          <span class="dates-compact">
            <span>c: {{ formatAgeShort(file.created) }}</span>
            <span class="date-sep">&middot;</span>
            <span :class="{ 'modified-highlight': hasUnpublishedChanges }">e: {{ formatAgeShort(file.modified) }}</span>
            <template v-if="file.published_date">
              <span class="date-sep">&middot;</span>
              <span class="published">p: {{ formatAgeShort(file.published_date) }}</span>
            </template>
          </span>
        </div>
        <div class="row">
          <span class="label">Obsidian</span>
          <span :class="obsidianConnected ? 'connected' : 'disconnected'">
            {{ obsidianConnected ? 'connected' : 'not connected' }}
          </span>
        </div>
        <div class="row">
          <span class="label">Git</span>
          <span v-if="!gitStatus">checking...</span>
          <span v-else-if="gitStatus.ok" class="connected">
            {{ gitStatus.branch }} ✓
          </span>
          <span v-else class="git-warning" :title="gitStatus.error || ''">
            {{ gitStatus.error }}
          </span>
        </div>
        <div v-if="postStats || loadingStats" class="row">
          <span class="label">Analytics</span>
          <span v-if="loadingStats" class="muted">loading...</span>
          <span v-else-if="postStats" class="analytics-inline">
            {{ postStats.pageviews }} views &middot;
            {{ postStats.visitors }} visitors
            <template v-if="postStats.totaltime > 0">
              &middot; {{ Math.round(postStats.totaltime / Math.max(postStats.visits, 1)) }}s avg
            </template>
          </span>
        </div>
        <div class="row">
          <span class="label">Visibility</span>
          <span v-if="isPasswordProtected" class="protected-text" :title="`Password: ${file.password}`">
            🔒 Protected
          </span>
          <span v-else-if="isUnlisted" class="unlisted-text">
            👁 Unlisted
          </span>
          <span v-else class="public-text">
            ✓ Public
          </span>
        </div>
      </div>

      <!-- Always visible: actionable metadata -->
      <div v-if="file.tags.length" class="row">
        <span class="label">Tags</span>
        <span class="tags-list">{{ file.tags.join(', ') }}</span>
      </div>
      <div v-if="suggestedTags.length > 0" class="row suggested-tags-row">
        <span class="label">Suggest</span>
        <div class="suggested-tags">
          <button
            v-for="tag in suggestedTags"
            :key="tag"
            class="tag-chip"
            @click="addTag(tag)"
            :disabled="addingTag"
            :title="`Click to add '${tag}' to frontmatter`"
          >
            <span class="tag-plus">+</span>
            {{ tag }}
            <span class="tag-count">{{ availableTags[tag] }}</span>
          </button>
        </div>
      </div>
    </div>

    <!-- Lint Receipt (only when warnings exist) -->
    <div v-if="lintWarnings.length" class="lint-receipt">
      <div class="lint-receipt-header">
        <span>Lint Receipt</span>
        <span class="lint-receipt-count">{{ lintWarnings.length }}</span>
      </div>
      <div class="lint-receipt-divider"></div>
      <div class="lint-receipt-list">
        <div v-for="warning in lintWarnings" :key="warning" class="lint-receipt-item">
          <span class="lint-receipt-bullet">•</span>
          <span class="lint-receipt-text">{{ warning }}</span>
        </div>
      </div>
      <div class="lint-receipt-footer">Dispatch</div>
    </div>

    <!-- Backlinks -->
    <div v-if="backlinks.length || loadingBacklinks" class="backlinks">
      <div class="backlinks-header">
        <span class="label">Backlinks</span>
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

    <!-- Local Media -->
    <div v-if="localMedia.length > 0 || loadingLocalMedia" class="local-media-section">
      <div class="local-media-header">
        <span class="label">Local Media</span>
        <span class="count warning">{{ loadingLocalMedia ? '...' : localMedia.length }}</span>
        <button v-if="localMedia.length > 0" @click="showMediaFixer = true" class="fix-btn">
          Fix
        </button>
      </div>
      <div v-if="loadingLocalMedia" class="local-media-loading">Scanning...</div>
      <div v-else class="local-media-list">
        <div v-for="media in localMedia.slice(0, 3)" :key="media.path + media.line_number" class="local-media-item">
          <span class="media-type">{{ media.media_type === 'video' ? '🎬' : '🖼' }}</span>
          <span class="media-path">{{ media.path }}</span>
          <span v-if="!media.resolved_path" class="missing">not found</span>
        </div>
        <div v-if="localMedia.length > 3" class="local-media-more">
          +{{ localMedia.length - 3 }} more
        </div>
      </div>
    </div>

    <!-- Local Media Fixer Modal -->
    <LocalMediaFixer
      v-if="showMediaFixer"
      :file-path="file.path"
      :local-media="localMedia"
      @close="showMediaFixer = false"
      @fixed="$emit('published')"
    />

    <!-- Toolbar -->
    <div class="toolbar">
      <div class="toolbar-open">
        <button
          v-for="editor in enabledEditors"
          :key="editor.app_name"
          @click="editor.app_name === 'Obsidian' ? openInObsidian() : openInEditor(editor.app_name)"
          class="tool-btn"
          :title="`Open in ${editor.name}`"
        >
          <span class="tool-icon">{{ editor.name.slice(0, 2) }}</span> {{ editor.name }}
        </button>
        <button @click="openPreview" class="tool-btn" title="Live Preview">
          <span class="tool-icon">&#9654;</span> Preview
        </button>
      </div>
      <div class="toolbar-actions">
        <select
          v-if="hasMultipleTargets"
          class="target-select"
          :value="selectedTargetId || publishTargets.find(t => t.is_default)?.id"
          @change="selectTarget(($event.target as HTMLSelectElement).value)"
        >
          <option v-for="t in publishTargets" :key="t.id" :value="t.id">
            {{ t.name }}
          </option>
        </select>
        <template v-if="isLive">
          <a :href="liveUrl!" target="_blank" class="btn">View &rarr;</a>
          <button @click="unpublish" :disabled="unpublishing" class="btn">
            {{ unpublishing ? '...' : 'Unpublish' }}
          </button>
          <button @click="openPublishConfirm(true)" :disabled="publishing" class="btn accent">
            {{ publishing ? '...' : 'Republish' }}
          </button>
        </template>
        <template v-else>
          <button
            v-if="file.is_safe && !isScheduled"
            @click="showSchedulePicker = !showSchedulePicker"
            class="btn"
          >
            ⏱ Schedule
          </button>
          <button
            @click="openPublishConfirm(false)"
            :disabled="!file.is_safe || publishing"
            class="btn accent publish-btn"
            :class="{ disabled: !file.is_safe, full: !file.is_safe || isScheduled }"
          >
            <span>{{ publishing ? 'Publishing...' : file.is_safe ? 'Publish' : 'Fix issues to publish' }}</span>
            <kbd v-if="file.is_safe && !publishing" class="shortcut-hint">
              <Command :size="10" />&crarr;
            </kbd>
          </button>
        </template>
      </div>
    </div>

    <!-- Schedule Picker -->
    <div v-if="showSchedulePicker" class="schedule-picker">
      <input
        type="datetime-local"
        v-model="scheduleDate"
        class="schedule-input"
        :min="new Date().toISOString().slice(0, 16)"
      />
      <button @click="schedulePublish" :disabled="!scheduleDate" class="btn accent">
        Confirm Schedule
      </button>
      <button @click="showSchedulePicker = false" class="btn">Cancel</button>
    </div>

    <!-- Publish Confirmation -->
    <div v-if="showPublishConfirm" class="modal-overlay">
      <div class="publish-confirm">
        <div class="publish-confirm-header">
          <h2>{{ publishConfirmRepublish ? 'Republish Confirmation' : 'Publish Confirmation' }}</h2>
          <button class="close-btn" @click="closePublishConfirm">&times;</button>
        </div>
        <div class="publish-confirm-body">
          <div v-if="publishConfirmStep === 1" class="confirm-step">
            <p class="confirm-title">Confirm this post is ready to go live.</p>
            <div class="confirm-row">
              <span class="label">File</span>
              <code>{{ file.source_dir || '.' }}/{{ file.filename }}</code>
            </div>
            <div class="confirm-row">
              <span class="label">Target</span>
              <code>{{ targetUrl }}</code>
            </div>
            <label class="confirm-check">
              <input type="checkbox" v-model="publishConfirmChecked" />
              I reviewed links, media, and metadata.
            </label>
          </div>
          <div v-else class="confirm-step">
            <p class="confirm-title">Type the slug to confirm:</p>
            <input
              class="confirm-input"
              v-model="publishConfirmText"
              :placeholder="slug"
              autofocus
            />
          </div>
        </div>
        <div class="publish-confirm-footer">
          <button class="btn" @click="closePublishConfirm">Cancel</button>
          <button
            v-if="publishConfirmStep === 1"
            class="btn accent"
            :disabled="!publishConfirmChecked"
            @click="publishConfirmStep = 2"
          >
            Continue
          </button>
          <button
            v-else
            class="btn accent"
            :disabled="publishConfirmText.trim() !== slug"
            @click="closePublishConfirm(); publish(publishConfirmRepublish)"
          >
            {{ publishConfirmRepublish ? 'Republish' : 'Publish' }}
          </button>
        </div>
      </div>
    </div>

    <!-- Content Divider -->
    <div class="content-divider"><span>CONTENT</span></div>

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

/* Banner */
.banner {
  padding: 8px 16px;
  font-size: 11px;
  display: flex;
  align-items: center;
  gap: 12px;
  transition: all 0.2s ease;
}

.banner.live {
  background: var(--success);
  color: #000;
}

.banner.live .banner-text {
  font-weight: 700;
}

.banner.live a {
  color: #000;
  opacity: 0.8;
  text-decoration: none;
  flex: 1;
}

.banner.live a:hover {
  opacity: 1;
  text-decoration: underline;
}

.banner.live button {
  background: rgba(0,0,0,0.2);
  border: none;
  color: #000;
  padding: 2px 8px;
  border-radius: 3px;
  font-size: 10px;
  cursor: pointer;
}

.banner.scheduled {
  background: rgba(255, 159, 10, 0.15);
  color: var(--warning);
}

.banner.scheduled button {
  background: rgba(255, 159, 10, 0.2);
  border: none;
  color: var(--warning);
  padding: 2px 8px;
  border-radius: 3px;
  font-size: 10px;
  cursor: pointer;
}

.banner.scheduled button:hover {
  background: rgba(255, 159, 10, 0.35);
}

.analytics-inline {
  font-variant-numeric: tabular-nums;
  font-feature-settings: 'tnum';
  color: var(--text-secondary);
}

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

.banner.warn {
  background: rgba(255, 159, 10, 0.15);
  color: var(--warning);
}

.banner.modified {
  background: var(--warning);
  color: #000;
}

.banner.modified .banner-text {
  font-weight: 700;
}

.banner.modified button {
  background: rgba(0,0,0,0.2);
  border: none;
  color: #000;
  padding: 2px 8px;
  border-radius: 3px;
  font-size: 10px;
  cursor: pointer;
  margin-left: auto;
}

.banner.ready {
  background: var(--accent);
  color: var(--text-secondary);
}

.banner.unlisted {
  background: #6366f1;
  color: #fff;
}

.banner.unlisted .banner-text {
  font-weight: 700;
}

.banner.unlisted a {
  color: rgba(255, 255, 255, 0.8);
  text-decoration: none;
  flex: 1;
}

.banner.unlisted a:hover {
  color: #fff;
  text-decoration: underline;
}

.banner.unlisted button {
  background: rgba(0,0,0,0.2);
  border: none;
  color: #fff;
  padding: 2px 8px;
  border-radius: 3px;
  font-size: 10px;
  cursor: pointer;
}

.banner.protected {
  background: #8b5cf6;
  color: #fff;
}

.banner.protected .banner-text {
  font-weight: 700;
}

.banner.protected a {
  color: rgba(255, 255, 255, 0.8);
  text-decoration: none;
  flex: 1;
}

.banner.protected a:hover {
  color: #fff;
  text-decoration: underline;
}

.banner.protected button {
  background: rgba(0,0,0,0.2);
  border: none;
  color: #fff;
  padding: 2px 8px;
  border-radius: 3px;
  font-size: 10px;
  cursor: pointer;
  margin-left: 4px;
}

.visibility-badge {
  font-size: 8px;
  font-weight: 700;
  padding: 2px 5px;
  border-radius: 3px;
  background: var(--accent);
}

.unlisted-ready .visibility-badge {
  background: #6366f1;
  color: #fff;
}

.protected-ready .visibility-badge {
  background: #8b5cf6;
  color: #fff;
}

.public-ready .visibility-badge {
  background: var(--success);
  color: #000;
}

.visibility-hint {
  font-size: 10px;
  color: var(--text-tertiary);
  margin-left: auto;
}

.unlisted-text {
  color: #6366f1;
}

.protected-text {
  color: #8b5cf6;
}

.public-text {
  color: var(--success);
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

/* Info */
.info {
  padding: 0 16px 12px;
  display: flex;
  flex-direction: column;
  gap: 4px;
  border-bottom: 1px solid var(--border);
}

.info-summary {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 4px 0;
  font-size: 10px;
  color: var(--text-secondary);
  cursor: pointer;
  user-select: none;
}

.info-summary:hover {
  color: var(--text-primary);
}

.info-toggle {
  font-size: 8px;
  color: var(--text-tertiary);
  transition: transform 0.15s ease;
  display: inline-block;
}

.info-toggle.expanded {
  transform: rotate(90deg);
}

.info-detail {
  display: flex;
  flex-direction: column;
  gap: 4px;
  padding: 4px 0 4px 4px;
  margin-bottom: 4px;
  border-left: 2px solid var(--border-light);
}

.row {
  display: flex;
  font-size: 11px;
  gap: 12px;
}

.label {
  width: 55px;
  color: var(--text-tertiary);
  flex-shrink: 0;
}

.weeknote-type {
  color: #f59e0b;
  font-weight: 600;
  font-size: 10px;
}

/* Lint Receipt */
.lint-receipt {
  margin: 10px 16px 12px;
  padding: 10px 12px;
  background: var(--bg-tertiary);
  border: 1px dashed var(--border-light);
  border-radius: 4px;
  font-family: 'SF Mono', 'Menlo', monospace;
  font-size: 10px;
  color: var(--text-secondary);
}

.lint-receipt-header {
  display: flex;
  justify-content: space-between;
  text-transform: uppercase;
  letter-spacing: 0.12em;
  color: var(--text-tertiary);
  font-size: 9px;
}

.lint-receipt-count {
  font-variant-numeric: tabular-nums;
}

.lint-receipt-divider {
  height: 1px;
  margin: 6px 0 8px;
  background: repeating-linear-gradient(
    90deg,
    var(--border-light) 0 4px,
    transparent 4px 8px
  );
}

.lint-receipt-list {
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.lint-receipt-item {
  display: flex;
  gap: 6px;
  align-items: flex-start;
}

.lint-receipt-bullet {
  color: var(--warning);
  line-height: 1;
}

.lint-receipt-text {
  color: var(--text-primary);
}

.lint-receipt-footer {
  margin-top: 8px;
  text-align: right;
  text-transform: uppercase;
  letter-spacing: 0.2em;
  color: var(--text-tertiary);
  font-size: 8px;
}

/* Publish confirm modal */
.publish-confirm {
  width: 520px;
  max-width: 92vw;
  background: var(--modal-bg);
  border: 1px solid var(--border-light);
  border-radius: 12px;
  box-shadow: var(--shadow-lg);
  overflow: hidden;
}

.publish-confirm-header {
  padding: 12px 16px;
  border-bottom: 1px solid var(--border);
  display: flex;
  align-items: center;
  gap: 12px;
}

.publish-confirm-header h2 {
  margin: 0;
  font-size: 13px;
  font-weight: 600;
}

.publish-confirm-body {
  padding: 14px 16px;
  display: flex;
  flex-direction: column;
  gap: 12px;
}

.confirm-title {
  font-size: 12px;
  color: var(--text-secondary);
  margin: 0;
}

.confirm-row {
  display: flex;
  gap: 10px;
  font-size: 11px;
}

.confirm-check {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 11px;
  color: var(--text-secondary);
}

.confirm-check input {
  accent-color: var(--success);
}

.confirm-input {
  width: 100%;
  padding: 10px 12px;
  font-size: 12px;
  font-family: 'SF Mono', monospace;
  background: var(--bg-tertiary);
  border: 1px solid var(--border-light);
  border-radius: 6px;
  color: var(--text-primary);
  outline: none;
}

.confirm-input:focus {
  border-color: var(--success);
}

.publish-confirm-footer {
  padding: 10px 16px;
  border-top: 1px solid var(--border);
  display: flex;
  justify-content: flex-end;
  gap: 8px;
}

/* Suggested tags */
.suggested-tags-row {
  align-items: flex-start;
}

.suggested-tags {
  display: flex;
  flex-wrap: wrap;
  gap: 4px;
}

.tag-chip {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  padding: 2px 8px;
  font-size: 10px;
  font-weight: 500;
  background: var(--accent);
  border: 1px solid var(--border);
  border-radius: 12px;
  color: var(--text-secondary);
  cursor: pointer;
  transition: all 0.15s cubic-bezier(0.34, 1.56, 0.64, 1);
}

.tag-chip:hover {
  background: var(--bg-tertiary);
  border-color: var(--border-light);
  color: var(--text-primary);
  transform: scale(1.05);
}

.tag-chip:active {
  transform: scale(0.95);
}

.tag-chip:disabled {
  opacity: 0.5;
  cursor: wait;
}

.tag-plus {
  font-weight: 600;
  color: var(--success);
  margin-right: 2px;
}

.tag-count {
  font-size: 8px;
  opacity: 0.5;
  font-weight: 400;
}

.tags-list {
  color: var(--text-secondary);
}

.row code {
  font-family: 'SF Mono', monospace;
  font-size: 10px;
  color: var(--text-secondary);
}

.dates-compact {
  display: flex;
  align-items: center;
  gap: 4px;
  font-size: 10px;
  color: var(--text-secondary);
  flex-wrap: wrap;
}

.date-sep {
  color: var(--text-tertiary);
  opacity: 0.5;
}

.missing {
  color: var(--warning);
}

.connected, .published {
  color: var(--success);
}

.modified-highlight {
  color: var(--warning);
  font-weight: 500;
}

.disconnected {
  color: var(--warning);
  opacity: 0.7;
}

.git-warning {
  color: var(--warning);
  font-size: 10px;
}

/* Success Toast - Enhanced celebration */
.success-toast {
  position: absolute;
  top: 50%;
  left: 50%;
  transform: translate(-50%, -50%);
  background: var(--success);
  color: #000;
  padding: 20px 40px;
  border-radius: 16px;
  font-size: 20px;
  font-weight: 600;
  z-index: 100;
  box-shadow: 0 8px 32px rgba(48, 209, 88, 0.4),
              0 0 0 1px rgba(48, 209, 88, 0.2);
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
  0% { transform: translate(-50%, -50%) scale(0.8); }
  50% { transform: translate(-50%, -50%) scale(1.08); }
  100% { transform: translate(-50%, -50%) scale(1); }
}

/* Panel glow on publish */
.panel.just-published {
  animation: successGlow 1.2s ease-out forwards;
}

@keyframes successGlow {
  0% { box-shadow: inset 0 0 60px color-mix(in srgb, var(--success) 30%, transparent); }
  100% { box-shadow: none; }
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

.backlinks-header .label {
  font-size: 10px;
  font-weight: 600;
  color: var(--text-tertiary);
  text-transform: uppercase;
  letter-spacing: 0.75px;
}

.backlinks-header .count {
  font-size: 10px;
  color: var(--text-secondary);
  font-weight: 600;
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

/* Local Media */
.local-media-section {
  padding: 8px 16px;
  border-bottom: 1px solid var(--border);
}

.local-media-header {
  display: flex;
  align-items: center;
  gap: 8px;
  margin-bottom: 6px;
}

.local-media-header .label {
  font-size: 10px;
  font-weight: 600;
  color: var(--text-tertiary);
  text-transform: uppercase;
  letter-spacing: 0.75px;
}

.local-media-header .count {
  font-size: 10px;
  font-weight: 600;
}

.local-media-header .count.warning {
  color: var(--warning);
}

.fix-btn {
  margin-left: auto;
  padding: 3px 10px;
  font-size: 10px;
  font-weight: 500;
  background: var(--warning);
  color: #000;
  border: none;
  border-radius: 4px;
  cursor: pointer;
}

.fix-btn:hover {
  filter: brightness(1.1);
}

.local-media-loading {
  font-size: 10px;
  color: var(--text-tertiary);
}

.local-media-list {
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.local-media-item {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 4px 6px;
  background: color-mix(in srgb, var(--warning) 15%, transparent);
  border-radius: 4px;
  font-size: 10px;
}

.media-type {
  flex-shrink: 0;
}

.media-path {
  flex: 1;
  font-family: 'SF Mono', monospace;
  color: var(--text-secondary);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.local-media-item .missing {
  color: var(--warning);
  font-size: 9px;
}

.local-media-more {
  font-size: 10px;
  color: var(--text-tertiary);
  text-align: center;
  padding: 4px;
}

/* Toolbar */
.toolbar {
  padding: 8px 16px;
  display: flex;
  flex-direction: column;
  gap: 6px;
  border-bottom: 1px solid var(--border);
}

.toolbar-open {
  display: flex;
  gap: 4px;
}

.tool-btn {
  flex: 1;
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 4px;
  padding: 4px 6px;
  height: 26px;
  font-size: 10px;
  font-weight: 500;
  color: var(--text-secondary);
  background: var(--bg-secondary);
  border: 1px solid var(--border);
  border-radius: 4px;
  cursor: pointer;
  transition: all 0.15s cubic-bezier(0.34, 1.56, 0.64, 1);
}

.tool-btn:hover {
  background: var(--bg-tertiary);
  border-color: var(--border-light);
  color: var(--text-primary);
}

.tool-btn:active {
  transform: scale(0.97);
}

.tool-icon {
  font-size: 10px;
  font-weight: 700;
  font-family: 'SF Mono', 'Menlo', monospace;
  letter-spacing: -0.5px;
}

.toolbar-actions {
  display: flex;
  gap: 8px;
  align-items: center;
}

.target-select {
  padding: 4px 8px;
  font-size: 10px;
  background: var(--bg-tertiary);
  border: 1px solid var(--border);
  border-radius: 4px;
  color: var(--text-secondary);
  cursor: pointer;
  font-family: 'SF Mono', monospace;
}

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

.btn.full {
  flex: 1;
}

.btn.disabled,
.btn:disabled {
  background: var(--bg-tertiary);
  color: var(--text-tertiary);
  cursor: not-allowed;
  filter: none;
}

.publish-btn {
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 8px;
  background: var(--success) !important;
  color: #000 !important;
}

.publish-btn:hover:not(:disabled) {
  background: color-mix(in srgb, var(--success) 90%, #fff) !important;
}

.publish-btn:disabled {
  background: var(--bg-tertiary) !important;
  color: var(--text-tertiary) !important;
}

.shortcut-hint {
  display: inline-flex;
  align-items: center;
  gap: 2px;
  padding: 2px 5px;
  background: rgba(0, 0, 0, 0.15);
  border: none;
  border-radius: 3px;
  font-size: 9px;
  font-family: 'SF Mono', monospace;
  color: inherit;
  opacity: 0.8;
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
}

/* Preview */
.preview {
  flex: 1;
  overflow-y: auto;
  padding: 12px 16px;
  scroll-behavior: smooth;
}

.preview::-webkit-scrollbar {
  width: 6px;
}

.preview::-webkit-scrollbar-track {
  background: transparent;
}

.preview::-webkit-scrollbar-thumb {
  background: var(--border-light);
  border-radius: 3px;
}

.preview::-webkit-scrollbar-thumb:hover {
  background: var(--text-tertiary);
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

/* Rendered markdown content */
.rendered-content {
  font-family: Georgia, 'Times New Roman', serif;
  font-size: 13px;
  line-height: 1.6;
  color: var(--text-secondary);
}

.rendered-content :deep(h1) {
  font-size: 18px;
  font-weight: 700;
  margin: 16px 0 8px;
  color: var(--text-primary);
}

.rendered-content :deep(h2) {
  font-size: 15px;
  font-weight: 600;
  margin: 14px 0 6px;
  color: var(--text-primary);
}

.rendered-content :deep(h3) {
  font-size: 13px;
  font-weight: 600;
  margin: 12px 0 4px;
  color: var(--text-primary);
}

.rendered-content :deep(h4),
.rendered-content :deep(h5),
.rendered-content :deep(h6) {
  font-size: 12px;
  font-weight: 600;
  margin: 10px 0 4px;
  color: var(--text-secondary);
}

.rendered-content :deep(p) {
  margin: 0 0 8px;
}

.rendered-content :deep(a) {
  color: #5b9bd5;
  text-decoration: underline;
  text-underline-offset: 2px;
}

.rendered-content :deep(a:hover) {
  color: #7bb8eb;
}

.rendered-content :deep(code) {
  font-family: 'SF Mono', 'Menlo', monospace;
  font-size: 11px;
  padding: 1px 4px;
  background: var(--bg-tertiary);
  border-radius: 3px;
  color: var(--text-primary);
}

.rendered-content :deep(pre) {
  background: var(--bg-tertiary);
  border: 1px solid var(--border);
  border-radius: 4px;
  padding: 8px 10px;
  margin: 8px 0;
  overflow-x: auto;
}

.rendered-content :deep(pre code) {
  padding: 0;
  background: none;
  font-size: 10.5px;
  line-height: 1.5;
}

.rendered-content :deep(blockquote) {
  border-left: 3px solid var(--border-light);
  margin: 8px 0;
  padding: 4px 12px;
  color: var(--text-tertiary);
  font-style: italic;
}

.rendered-content :deep(ul),
.rendered-content :deep(ol) {
  margin: 4px 0 8px;
  padding-left: 20px;
}

.rendered-content :deep(li) {
  margin: 2px 0;
}

.rendered-content :deep(img) {
  max-width: 100%;
  height: auto;
  border-radius: 4px;
  margin: 8px 0;
}

.rendered-content :deep(hr) {
  border: none;
  border-top: 1px solid var(--border);
  margin: 12px 0;
}

.rendered-content :deep(table) {
  width: 100%;
  border-collapse: collapse;
  margin: 8px 0;
  font-size: 11px;
}

.rendered-content :deep(th),
.rendered-content :deep(td) {
  border: 1px solid var(--border);
  padding: 4px 8px;
  text-align: left;
}

.rendered-content :deep(th) {
  background: var(--bg-tertiary);
  font-weight: 600;
}
</style>
