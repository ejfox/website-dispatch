<script setup lang="ts">
import { ref, watch, computed, onBeforeUnmount, nextTick } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { Command } from 'lucide-vue-next'
import { PhLockSimple, PhEye, PhEyeSlash, PhClock, PhArrowSquareUpRight, PhArrowsClockwise, PhKeyboard, PhShieldWarning, PhCheck, PhCheckCircle, PhFolder, PhCalendarBlank, PhGitBranch, PhCircleWavy, PhChartBar, PhGlobe, PhTag, PhLinkSimple, PhImageSquare, PhFilmSlate, PhWarningCircle, PhTextAa, PhPlay, PhArrowSquareOut, PhNotePencil, PhTrash, PhTrophy } from '@phosphor-icons/vue'
import LocalMediaFixer from './LocalMediaFixer.vue'
import AltTextReviewer from './AltTextReviewer.vue'
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
const isMilestoneToast = ref(false)
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

// Alt text review modal
const showAltTextReviewer = ref(false)

// Syndication (Mastodon, etc.)
interface SyndicationResult {
  platform: string
  success: boolean
  url: string | null
  error: string | null
}
const syndicating = ref(false)
const syndicationResults = ref<SyndicationResult[]>([])

// Webmentions
interface WebmentionResult {
  target: string
  endpoint: string | null
  status: string
  message: string | null
}
interface WebmentionReport {
  source: string
  results: WebmentionResult[]
  total_links: number
  sent: number
  no_endpoint: number
  errors: number
}
const sendingWebmentions = ref(false)
const webmentionReport = ref<WebmentionReport | null>(null)

// Crown
const isCrowned = ref(false)
const crowning = ref(false)
const crownHue = ref(220)

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
  webmentionReport.value = null

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

// Alt text detection
const missingAltTextCount = computed(() => {
  const w = props.file.warnings.find(w => w.startsWith('Missing alt text'))
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

async function openPublishConfirm(isRepublish = false) {
  publishConfirmRepublish.value = isRepublish
  publishConfirmStep.value = 1
  publishConfirmChecked.value = false
  publishConfirmText.value = ''
  showPublishConfirm.value = true
  // Fetch journal stats for context line
  try {
    journalStatsCache.value = await invoke('get_journal_stats')
  } catch (_) { /* ok */ }
}

function closePublishConfirm() {
  showPublishConfirm.value = false
}

async function publish(isRepublish = false) {
  // For fresh publish, require is_safe. For republish, allow it.
  if (!isRepublish && !props.file.is_safe) return
  publishing.value = true
  try {
    // Snapshot milestones before publish
    let milestonesBefore: string[] = []
    try {
      const statsBefore = await invoke<any>('get_journal_stats')
      milestonesBefore = (statsBefore.milestones || [])
        .filter((m: any) => m.achieved_at)
        .map((m: any) => m.id)
    } catch (_) { /* journal may not be ready */ }

    const url = await invoke<string>('publish_file', {
      sourcePath: props.file.path,
      slug: slug.value,
      targetId: getActiveTargetId() || null,
    })
    justPublished.value = url

    // Check for newly earned milestones
    let newMilestone: { label: string; description: string } | null = null
    try {
      const statsAfter = await invoke<any>('get_journal_stats')
      const earned = (statsAfter.milestones || []).filter((m: any) => m.achieved_at)
      const fresh = earned.find((m: any) => !milestonesBefore.includes(m.id))
      if (fresh) newMilestone = { label: fresh.label, description: fresh.description }
    } catch (_) { /* ok */ }

    // Show success toast — milestone takes priority
    isMilestoneToast.value = !!newMilestone
    if (newMilestone) {
      successMessage.value = `${newMilestone.label}! ${newMilestone.description}`
    } else {
      const visibilityContext = isPasswordProtected.value ? ' (protected)' :
                                isUnlisted.value ? ' (unlisted)' : ''
      successMessage.value = isRepublish ? 'Republished!' : `Published${visibilityContext}!`
    }
    showSuccess.value = true
    justPublishedGlow.value = true
    setTimeout(() => { showSuccess.value = false }, newMilestone ? 5000 : 3000)
    setTimeout(() => { justPublishedGlow.value = false }, 1500)

    // Delay refresh slightly to let filesystem settle
    setTimeout(() => emit('published'), 500)
  } catch (e) {
    alert(`Failed: ${e}`)
  }
  publishing.value = false
}

async function syndicatePost() {
  if (!liveUrl.value) return
  syndicating.value = true
  try {
    const results = await invoke<SyndicationResult[]>('syndicate_post', {
      postUrl: liveUrl.value,
      title: title.value,
      slug: slug.value,
      tags: props.file.tags,
      dek: props.file.dek || null,
      contentType: props.file.content_type,
      visibility: isPasswordProtected.value ? 'protected' : isUnlisted.value ? 'unlisted' : 'public',
    })
    syndicationResults.value = results
    const success = results.filter(r => r.success)
    if (success.length > 0) {
      successMessage.value = `Shared to ${success.map(r => r.platform).join(', ')}!`
      showSuccess.value = true
      setTimeout(() => { showSuccess.value = false }, 3000)
    }
  } catch (e) {
    syndicationResults.value = [{ platform: 'error', success: false, url: null, error: `${e}` }]
  }
  syndicating.value = false
}

function onAltTextApplied() {
  showAltTextReviewer.value = false
  successMessage.value = 'Alt text applied!'
  showSuccess.value = true
  setTimeout(() => { showSuccess.value = false }, 3000)
  setTimeout(() => emit('published'), 500)
}

async function publishUnlisted() {
  if (!props.file.is_safe) return
  try {
    await invoke('set_frontmatter', { path: props.file.path, key: 'unlisted', value: 'true' })
    await publish(false)
  } catch (e) {
    alert(`Failed to set unlisted: ${e}`)
  }
}

// Check crown status when slug changes
watch(slug, async (s) => {
  if (!s) { isCrowned.value = false; return }
  try {
    isCrowned.value = await invoke<boolean>('is_post_crowned', { slug: s })
  } catch { isCrowned.value = false }
}, { immediate: true })

async function crownPost() {
  if (!slug.value || crowning.value) return
  crowning.value = true
  try {
    const path = await invoke<string>('crown_post', { slug: slug.value, hue: crownHue.value })
    isCrowned.value = true
    successMessage.value = `Crowned! Edit ${path.split('/').slice(-3).join('/')}`
    showSuccess.value = true
    setTimeout(() => { showSuccess.value = false }, 5000)
  } catch (e) {
    alert(`Crown failed: ${e}`)
  }
  crowning.value = false
}

async function triggerWebmentions(bridgyFed = false) {
  const url = liveUrl.value
  if (!url || sendingWebmentions.value) return
  sendingWebmentions.value = true
  webmentionReport.value = null
  try {
    const report = await invoke<WebmentionReport>('send_webmentions', {
      postUrl: url,
      bridgyFed,
      targetId: getActiveTargetId() || null,
    })
    webmentionReport.value = report
    if (report.sent > 0) {
      successMessage.value = `Sent ${report.sent} webmention${report.sent > 1 ? 's' : ''}!`
      showSuccess.value = true
      setTimeout(() => { showSuccess.value = false }, 3000)
    }
  } catch (e) {
    alert(`Webmention error: ${e}`)
  }
  sendingWebmentions.value = false
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

// Journal context for publish confirmation
const journalStatsCache = ref<any>(null)
const publishContext = computed(() => {
  const s = journalStatsCache.value
  if (!s) return null
  const parts: string[] = []
  const nth = s.total_publishes + 1
  if (nth > 1) parts.push(`This will be publish #${nth}`)
  if (s.current_streak_days >= 2) parts.push(`${s.current_streak_days}-day streak`)
  if (s.words_this_month > 1000) parts.push(`${(s.words_this_month / 1000).toFixed(1)}k words this month`)
  return parts.length > 0 ? parts.join(' · ') : null
})

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
    <div v-if="isLive && hasUnpublishedChanges" class="banner modified">
      <span class="banner-text">MODIFIED</span>
      <span v-if="visibilityLabel" class="visibility-badge">{{ visibilityLabel }}</span>
      <span>Source changed since last publish</span>
      <button @click="publish(true)" :disabled="publishing">{{ publishing ? '...' : 'Republish' }}</button>
    </div>
    <div v-else-if="isLive && isPasswordProtected" class="banner protected">
      <span class="banner-text"><PhLockSimple :size="13" weight="bold" /> PROTECTED</span>
      <a :href="liveUrl!" target="_blank">{{ liveUrl }}</a>
      <button @click="copyUrlAndPassword" data-tip="Copy URL + password to share">Copy + Pass</button>
      <button @click="copyUrl" data-tip="Copy URL only">Copy URL</button>
    </div>
    <div v-else-if="isLive && isUnlisted" class="banner unlisted">
      <span class="banner-text"><PhEye :size="13" weight="bold" /> UNLISTED</span>
      <a :href="liveUrl!" target="_blank">{{ liveUrl }}</a>
      <button @click="copyUrl" data-tip="Share this link — not indexed anywhere">Copy</button>
    </div>
    <div v-else-if="isLive" class="banner live">
      <span class="banner-text"><PhCheckCircle :size="13" weight="fill" /> LIVE</span>
      <a :href="liveUrl!" target="_blank">{{ liveUrl }}</a>
      <button @click="copyUrl">Copy</button>
    </div>
    <div v-else-if="isScheduled" class="banner scheduled">
      <span class="banner-text"><PhClock :size="13" weight="bold" /> SCHEDULED</span>
      <span>{{ formatScheduledTime(file.publish_at!) }}</span>
      <button @click="cancelSchedule">Cancel</button>
    </div>
    <div v-else-if="!file.is_safe" class="banner warn">
      {{ file.warnings.join(' · ') }}
    </div>
    <div v-else-if="isPasswordProtected" class="banner ready protected-ready">
      <span class="visibility-badge"><PhLockSimple :size="12" weight="bold" /> PASSWORD</span>
      <span class="visibility-hint">Link + password required to view</span>
    </div>
    <div v-else-if="isUnlisted" class="banner ready unlisted-ready">
      <span class="visibility-badge"><PhEye :size="12" weight="bold" /> UNLISTED</span>
      <span class="visibility-hint">Link only — won't appear in listings or feeds</span>
    </div>
    <div v-else class="banner ready public-ready">
      <span class="visibility-badge"><PhCheckCircle :size="10" weight="fill" /> PUBLIC</span>
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
        <span class="info-chip"><PhTextAa :size="9" weight="duotone" /> {{ file.word_count }}w</span>
        <span class="date-sep">&middot;</span>
        <span class="info-chip"><PhTag :size="9" weight="duotone" /> {{ file.tags.length }} tags</span>
        <span class="date-sep">&middot;</span>
        <span class="info-chip"><PhCalendarBlank :size="9" weight="duotone" /> {{ file.date ? formatDate(file.date) : formatDateCompact(file.created) }}</span>
      </div>

      <!-- Detail rows (collapsed by default) -->
      <div v-show="metadataExpanded" class="info-detail">
        <div v-if="file.content_type === 'weeknote'" class="row">
          <span class="label">Type</span>
          <span class="weeknote-type">Week Note</span>
        </div>
        <div class="row">
          <span class="label"><PhFolder :size="10" weight="duotone" /> Source</span>
          <code>{{ file.source_dir || '.' }}/{{ file.filename }}</code>
        </div>
        <div class="row">
          <span class="label"><PhCalendarBlank :size="10" weight="duotone" /> Dates</span>
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
          <span class="label"><PhCircleWavy :size="10" weight="duotone" /> Obsidian</span>
          <span :class="obsidianConnected ? 'connected' : 'disconnected'">
            {{ obsidianConnected ? 'connected' : 'not connected' }}
          </span>
        </div>
        <div class="row">
          <span class="label"><PhGitBranch :size="10" weight="duotone" /> Git</span>
          <span v-if="!gitStatus">checking...</span>
          <span v-else-if="gitStatus.ok" class="connected">
            {{ gitStatus.branch }}
          </span>
          <span v-else class="git-warning" :title="gitStatus.error || ''">
            {{ gitStatus.error }}
          </span>
        </div>
        <div v-if="postStats || loadingStats" class="row">
          <span class="label"><PhChartBar :size="10" weight="duotone" /> Analytics</span>
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
          <span class="label"><PhGlobe :size="10" weight="duotone" /> Visibility</span>
          <span v-if="isPasswordProtected" class="protected-text" :title="`Password: ${file.password}`">
            <PhLockSimple :size="11" weight="bold" /> Protected
          </span>
          <span v-else-if="isUnlisted" class="unlisted-text">
            <PhEye :size="11" weight="bold" /> Unlisted
          </span>
          <span v-else class="public-text">
            Public
          </span>
        </div>
      </div>

      <!-- Always visible: actionable metadata -->
      <div v-if="file.tags.length" class="row">
        <span class="label"><PhTag :size="10" weight="duotone" /> Tags</span>
        <span class="tags-list">{{ file.tags.join(', ') }}</span>
      </div>
      <div v-if="suggestedTags.length > 0" class="row suggested-tags-row">
        <span class="label"><PhTag :size="10" weight="duotone" /> Suggest</span>
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
        <div v-for="warning in lintWarnings" :key="warning" class="lint-receipt-item" :class="{ privacy: warning.startsWith('[privacy]') }">
          <span class="lint-receipt-bullet" :class="{ privacy: warning.startsWith('[privacy]') }">
            <PhShieldWarning v-if="warning.startsWith('[privacy]')" :size="12" weight="fill" />
            <template v-else>&bull;</template>
          </span>
          <span class="lint-receipt-text">{{ warning.startsWith('[privacy]') ? warning.slice(9) : warning }}</span>
        </div>
      </div>
      <div class="lint-receipt-footer">Dispatch</div>
    </div>

    <!-- Alt Text -->
    <div v-if="missingAltTextCount > 0" class="alt-text-section">
      <div class="alt-text-header">
        <span class="label"><PhImageSquare :size="10" weight="duotone" /> Alt Text</span>
        <span class="count warning">{{ missingAltTextCount }}</span>
        <button @click="showAltTextReviewer = true" class="fix-btn">
          Describe
        </button>
      </div>
      <div class="alt-text-hint">
        {{ missingAltTextCount }} image(s) need descriptions
      </div>
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
        <span class="label"><PhLinkSimple :size="10" weight="duotone" /> Backlinks</span>
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
        <span class="label"><PhImageSquare :size="10" weight="duotone" /> Media</span>
        <span class="count warning">{{ loadingLocalMedia ? '...' : localMedia.length }}</span>
        <button v-if="localMedia.length > 0" @click="showMediaFixer = true" class="fix-btn">
          Fix
        </button>
      </div>
      <div v-if="loadingLocalMedia" class="local-media-loading">Scanning...</div>
      <div v-else class="local-media-list">
        <div v-for="media in localMedia.slice(0, 3)" :key="media.path + media.line_number" class="local-media-item">
          <span class="media-type"><PhFilmSlate v-if="media.media_type === 'video'" :size="12" weight="duotone" /><PhImageSquare v-else :size="12" weight="duotone" /></span>
          <span class="media-path">{{ media.path }}</span>
          <span v-if="!media.resolved_path" class="missing"><PhWarningCircle :size="10" weight="fill" /> not found</span>
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
          :data-tip="`Open in ${editor.name}`"
        >
          <PhNotePencil :size="12" weight="duotone" /> {{ editor.name }}
        </button>
        <button @click="openPreview" class="tool-btn" data-tip="Open local preview server">
          <PhPlay :size="12" weight="fill" /> Preview
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
          <a :href="liveUrl!" target="_blank" class="btn"><PhArrowSquareOut :size="12" weight="bold" /> View</a>
          <button
            @click="triggerWebmentions(false)"
            :disabled="sendingWebmentions"
            class="btn webmention-btn"
            data-tip="Send webmentions to linked sites"
          >
            <PhGlobe :size="12" weight="bold" /> {{ sendingWebmentions ? 'Sending...' : 'Webmention' }}
          </button>
          <button
            @click="syndicatePost"
            :disabled="syndicating || syndicationResults.some(r => r.success)"
            class="btn syndicate-btn"
            data-tip="Share to Mastodon"
          >
            <PhArrowSquareUpRight :size="12" weight="bold" /> {{ syndicating ? 'Posting...' : syndicationResults.some(r => r.success) ? 'Shared' : 'Share' }}
          </button>
          <button
            v-if="!isCrowned"
            @click="crownPost"
            :disabled="crowning"
            class="btn crown-btn"
            data-tip="Create interactive Vue page takeover"
          >
            <PhTrophy :size="12" weight="bold" /> {{ crowning ? 'Crowning...' : 'Crown' }}
          </button>
          <span v-else class="crowned-badge" data-tip="This post has a Vue page takeover">
            <PhTrophy :size="10" weight="fill" /> Crowned
          </span>
          <button @click="unpublish" :disabled="unpublishing" class="btn">
            <PhTrash :size="12" weight="bold" /> {{ unpublishing ? '...' : 'Unpublish' }}
          </button>
          <button @click="openPublishConfirm(true)" :disabled="publishing" class="btn accent">
            <PhArrowsClockwise :size="12" weight="bold" /> {{ publishing ? '...' : 'Republish' }}
          </button>
        </template>
        <template v-else>
          <button
            v-if="file.is_safe && !isScheduled"
            @click="showSchedulePicker = !showSchedulePicker"
            class="btn"
          >
            <PhClock :size="12" weight="bold" /> Schedule
          </button>
          <button
            v-if="file.is_safe && !isUnlisted && !publishing"
            @click="publishUnlisted"
            class="btn publish-unlisted-btn"
          >
            <PhEyeSlash :size="12" weight="bold" /> Unlisted
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

    <!-- Webmention Results -->
    <!-- Syndication results -->
    <div v-if="syndicationResults.length > 0" class="syndication-report">
      <div v-for="r in syndicationResults" :key="r.platform" class="syndication-item" :class="{ success: r.success, error: !r.success }">
        <span v-if="r.success" class="syndication-icon">&#10003;</span>
        <span v-else class="syndication-icon">&#10007;</span>
        <span class="syndication-platform">{{ r.platform }}</span>
        <a v-if="r.url" :href="r.url" target="_blank" class="syndication-link">view</a>
        <span v-if="r.error" class="syndication-error">{{ r.error }}</span>
      </div>
    </div>

    <div v-if="webmentionReport" class="webmention-report">
      <div class="webmention-header">
        <span class="webmention-title"><PhGlobe :size="12" weight="bold" /> Webmentions</span>
        <span class="webmention-stats">
          <span v-if="webmentionReport.sent" class="wm-sent">{{ webmentionReport.sent }} sent</span>
          <span v-if="webmentionReport.no_endpoint" class="wm-none">{{ webmentionReport.no_endpoint }} no endpoint</span>
          <span v-if="webmentionReport.errors" class="wm-err">{{ webmentionReport.errors }} failed</span>
        </span>
        <button class="wm-close" @click="webmentionReport = null">&times;</button>
      </div>
      <div class="webmention-list">
        <div
          v-for="r in webmentionReport.results"
          :key="r.target"
          class="wm-item"
          :class="r.status"
        >
          <span class="wm-status-dot"></span>
          <a :href="r.target" target="_blank" class="wm-target">{{ r.target.replace(/^https?:\/\//, '').split('/').slice(0, 2).join('/') }}</a>
          <span v-if="r.message" class="wm-msg">{{ r.message }}</span>
        </div>
      </div>
      <button
        v-if="!sendingWebmentions"
        class="btn webmention-btn wm-bridgy"
        @click="triggerWebmentions(true)"
        data-tip="Also send to Bridgy Fed for fediverse"
      >
        <PhGlobe :size="11" weight="bold" /> Resend + Bridgy Fed
      </button>
    </div>

    <!-- Publish Confirmation -->
    <Transition name="pub-modal">
      <div v-if="showPublishConfirm" class="pub-overlay" @click.self="closePublishConfirm" @keydown.escape="closePublishConfirm" tabindex="-1">
        <div class="pub-modal">
          <div class="pub-modal-step-indicator">
            <div class="pub-step" :class="{ active: publishConfirmStep === 1, done: publishConfirmStep === 2 }">
              <span class="pub-step-num">1</span>
              <span class="pub-step-label">Review</span>
            </div>
            <div class="pub-step-line" :class="{ active: publishConfirmStep === 2 }"></div>
            <div class="pub-step" :class="{ active: publishConfirmStep === 2 }">
              <span class="pub-step-num">2</span>
              <span class="pub-step-label">Confirm</span>
            </div>
          </div>

          <div v-if="publishConfirmStep === 1" class="pub-modal-body">
            <div class="pub-modal-icon">
              <PhArrowsClockwise v-if="publishConfirmRepublish" :size="28" weight="light" />
              <PhArrowSquareUpRight v-else :size="28" weight="light" />
            </div>
            <h2 class="pub-modal-title">{{ publishConfirmRepublish ? 'Republish' : 'Ready to publish?' }}</h2>
            <p class="pub-modal-subtitle">This will push to your live website.</p>
            <p v-if="publishContext" class="pub-modal-context">{{ publishContext }}</p>

            <div class="pub-detail-card">
              <div class="pub-detail-row">
                <span class="pub-detail-label">File</span>
                <code class="pub-detail-value">{{ file.filename }}</code>
              </div>
              <div class="pub-detail-divider"></div>
              <div class="pub-detail-row">
                <span class="pub-detail-label">URL</span>
                <code class="pub-detail-value pub-detail-url">{{ targetUrl }}</code>
              </div>
              <div class="pub-detail-divider"></div>
              <div class="pub-detail-row">
                <span class="pub-detail-label">Words</span>
                <code class="pub-detail-value">{{ file.word_count.toLocaleString() }}</code>
              </div>
              <div v-if="file.content_type === 'weeknote'" class="pub-detail-divider"></div>
              <div v-if="file.content_type === 'weeknote'" class="pub-detail-row">
                <span class="pub-detail-label">Type</span>
                <span class="pub-detail-value pub-weeknote-badge">Week Note</span>
              </div>
            </div>

            <label class="pub-checkbox" :class="{ checked: publishConfirmChecked }">
              <input type="checkbox" v-model="publishConfirmChecked" />
              <span class="pub-checkbox-box">
                <PhCheck v-if="publishConfirmChecked" :size="13" weight="bold" />
              </span>
              <span>I reviewed links, media, and metadata.</span>
            </label>
          </div>

          <div v-else class="pub-modal-body">
            <div class="pub-modal-icon">
              <PhKeyboard :size="28" weight="light" />
            </div>
            <h2 class="pub-modal-title">Type slug to confirm</h2>
            <p class="pub-modal-subtitle">
              Type <code class="pub-slug-hint">{{ slug }}</code> to publish.
            </p>
            <input
              class="pub-slug-input"
              v-model="publishConfirmText"
              :placeholder="slug"
              autofocus
              @keydown.enter="publishConfirmText.trim() === slug && (closePublishConfirm(), publish(publishConfirmRepublish))"
            />
            <div class="pub-slug-match" v-if="publishConfirmText.length > 0">
              <span v-if="publishConfirmText.trim() === slug" class="pub-match-yes">Match!</span>
              <span v-else class="pub-match-no">{{ publishConfirmText.length }}/{{ slug.length }}</span>
            </div>
          </div>

          <div class="pub-modal-footer">
            <button class="pub-btn-cancel" @click="closePublishConfirm">
              Cancel <kbd>esc</kbd>
            </button>
            <button
              v-if="publishConfirmStep === 1"
              class="pub-btn-go"
              :disabled="!publishConfirmChecked"
              @click="publishConfirmStep = 2"
            >
              Continue
            </button>
            <button
              v-else
              class="pub-btn-go pub-btn-publish"
              :disabled="publishConfirmText.trim() !== slug"
              @click="closePublishConfirm(); publish(publishConfirmRepublish)"
            >
              {{ publishConfirmRepublish ? 'Republish' : 'Publish' }}
            </button>
          </div>
        </div>
      </div>
    </Transition>

    <!-- Content Divider -->
    <div class="content-divider"><span><PhTextAa :size="10" weight="duotone" /> CONTENT</span></div>

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

.banner-text {
  display: flex;
  align-items: center;
  gap: 5px;
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
  display: inline-flex;
  align-items: center;
  gap: 3px;
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

.info-chip {
  display: inline-flex;
  align-items: center;
  gap: 3px;
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
  width: 68px;
  color: var(--text-tertiary);
  flex-shrink: 0;
  display: flex;
  align-items: center;
  gap: 3px;
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

.lint-receipt-item.privacy {
  background: rgba(239, 68, 68, 0.1);
  padding: 2px 6px;
  border-radius: 3px;
  border-left: 2px solid #ef4444;
}

.lint-receipt-bullet.privacy {
  color: #ef4444;
  font-size: 10px;
}

.lint-receipt-item.privacy .lint-receipt-text {
  color: #ef4444;
  font-weight: 500;
}

.lint-receipt-footer {
  margin-top: 8px;
  text-align: right;
  text-transform: uppercase;
  letter-spacing: 0.2em;
  color: var(--text-tertiary);
  font-size: 8px;
}

/* Publish confirm modal — redesigned */
.pub-overlay {
  position: fixed;
  inset: 0;
  background: rgba(0, 0, 0, 0.5);
  backdrop-filter: blur(8px);
  -webkit-backdrop-filter: blur(8px);
  z-index: 200;
  display: flex;
  align-items: center;
  justify-content: center;
}

.pub-modal {
  width: 420px;
  max-width: 90vw;
  background: var(--modal-bg);
  backdrop-filter: blur(24px) saturate(180%);
  -webkit-backdrop-filter: blur(24px) saturate(180%);
  border: 1px solid var(--border-light);
  border-radius: 16px;
  box-shadow: var(--shadow-lg), 0 0 0 1px rgba(255,255,255,0.05);
  overflow: hidden;
}

/* Step indicator */
.pub-modal-step-indicator {
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 0;
  padding: 16px 24px 0;
}

.pub-step {
  display: flex;
  align-items: center;
  gap: 6px;
  opacity: 0.35;
  transition: all 0.3s ease;
}

.pub-step.active, .pub-step.done {
  opacity: 1;
}

.pub-step-num {
  width: 20px;
  height: 20px;
  border-radius: 50%;
  display: flex;
  align-items: center;
  justify-content: center;
  font-size: 10px;
  font-weight: 700;
  background: var(--bg-tertiary);
  color: var(--text-tertiary);
  transition: all 0.3s ease;
}

.pub-step.active .pub-step-num {
  background: var(--success);
  color: #000;
}

.pub-step.done .pub-step-num {
  background: var(--success);
  color: #000;
}

.pub-step-label {
  font-size: 10px;
  font-weight: 500;
  text-transform: uppercase;
  letter-spacing: 0.5px;
  color: var(--text-tertiary);
}

.pub-step.active .pub-step-label {
  color: var(--text-primary);
}

.pub-step-line {
  width: 40px;
  height: 2px;
  background: var(--border);
  margin: 0 10px;
  border-radius: 1px;
  transition: background 0.3s ease;
}

.pub-step-line.active {
  background: var(--success);
}

/* Modal body */
.pub-modal-body {
  padding: 20px 24px;
  display: flex;
  flex-direction: column;
  align-items: center;
  text-align: center;
}

.pub-modal-icon {
  margin-bottom: 8px;
  color: var(--success);
  width: 44px;
  height: 44px;
  border-radius: 12px;
  background: color-mix(in srgb, var(--success) 12%, transparent);
  display: flex;
  align-items: center;
  justify-content: center;
}

.pub-modal-title {
  margin: 0;
  font-size: 16px;
  font-weight: 600;
  color: var(--text-primary);
}

.pub-modal-subtitle {
  margin: 4px 0 16px;
  font-size: 12px;
  color: var(--text-tertiary);
}

.pub-modal-context {
  margin: -10px 0 14px;
  font-size: 10px;
  font-family: 'SF Mono', monospace;
  color: var(--success);
  opacity: 0.8;
  letter-spacing: 0.01em;
}

/* Detail card */
.pub-detail-card {
  width: 100%;
  background: var(--bg-tertiary);
  border: 1px solid var(--border);
  border-radius: 10px;
  padding: 2px 0;
  margin-bottom: 16px;
}

.pub-detail-row {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: 8px 14px;
}

.pub-detail-label {
  font-size: 10px;
  font-weight: 600;
  text-transform: uppercase;
  letter-spacing: 0.5px;
  color: var(--text-tertiary);
  flex-shrink: 0;
}

.pub-detail-value {
  font-size: 11px;
  font-family: 'SF Mono', monospace;
  color: var(--text-primary);
  text-align: right;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  max-width: 260px;
}

.pub-detail-url {
  color: var(--success);
}

.pub-detail-divider {
  height: 1px;
  background: var(--border);
  margin: 0 14px;
}

.pub-weeknote-badge {
  font-family: -apple-system, BlinkMacSystemFont, sans-serif;
  font-size: 10px;
  font-weight: 600;
  color: #f59e0b;
  background: rgba(245, 158, 11, 0.15);
  padding: 2px 8px;
  border-radius: 4px;
}

/* Custom checkbox */
.pub-checkbox {
  display: flex;
  align-items: center;
  gap: 10px;
  font-size: 12px;
  color: var(--text-secondary);
  cursor: pointer;
  padding: 10px 14px;
  border-radius: 8px;
  background: transparent;
  border: 1px solid var(--border);
  width: 100%;
  transition: all 0.2s ease;
}

.pub-checkbox:hover {
  background: var(--bg-tertiary);
}

.pub-checkbox.checked {
  border-color: var(--success);
  background: color-mix(in srgb, var(--success) 8%, transparent);
}

.pub-checkbox input {
  display: none;
}

.pub-checkbox-box {
  width: 18px;
  height: 18px;
  border-radius: 4px;
  border: 2px solid var(--border-light);
  display: flex;
  align-items: center;
  justify-content: center;
  flex-shrink: 0;
  transition: all 0.2s ease;
  color: #fff;
}

.pub-checkbox.checked .pub-checkbox-box {
  background: var(--success);
  border-color: var(--success);
}

/* Slug confirmation input */
.pub-slug-hint {
  font-family: 'SF Mono', monospace;
  font-size: 11px;
  background: var(--bg-tertiary);
  padding: 2px 6px;
  border-radius: 4px;
  color: var(--success);
}

.pub-slug-input {
  width: 100%;
  padding: 12px 14px;
  font-size: 14px;
  font-family: 'SF Mono', monospace;
  background: var(--bg-tertiary);
  border: 2px solid var(--border);
  border-radius: 10px;
  color: var(--text-primary);
  outline: none;
  text-align: center;
  transition: border-color 0.2s ease;
}

.pub-slug-input:focus {
  border-color: var(--success);
}

.pub-slug-match {
  margin-top: 6px;
  font-size: 11px;
  font-weight: 500;
}

.pub-match-yes {
  color: var(--success);
}

.pub-match-no {
  color: var(--text-tertiary);
  font-family: 'SF Mono', monospace;
}

/* Footer */
.pub-modal-footer {
  padding: 12px 20px;
  border-top: 1px solid var(--border);
  display: flex;
  justify-content: space-between;
  align-items: center;
}

.pub-btn-cancel {
  padding: 8px 14px;
  font-size: 11px;
  font-weight: 500;
  background: transparent;
  border: none;
  color: var(--text-tertiary);
  cursor: pointer;
  display: flex;
  align-items: center;
  gap: 6px;
  border-radius: 8px;
  transition: all 0.15s ease;
}

.pub-btn-cancel:hover {
  background: var(--bg-tertiary);
  color: var(--text-secondary);
}

.pub-btn-cancel kbd {
  font-size: 9px;
  padding: 1px 4px;
  background: var(--bg-tertiary);
  border: 1px solid var(--border);
  border-radius: 3px;
  font-family: 'SF Mono', monospace;
  color: var(--text-tertiary);
}

.pub-btn-go {
  padding: 8px 20px;
  font-size: 12px;
  font-weight: 600;
  background: var(--success);
  color: #000;
  border: none;
  border-radius: 8px;
  cursor: pointer;
  transition: all 0.2s cubic-bezier(0.34, 1.56, 0.64, 1);
}

.pub-btn-go:hover:not(:disabled) {
  transform: translateY(-1px);
  box-shadow: 0 4px 12px color-mix(in srgb, var(--success) 40%, transparent);
}

.pub-btn-go:active:not(:disabled) {
  transform: translateY(0);
}

.pub-btn-go:disabled {
  opacity: 0.3;
  cursor: not-allowed;
}

.pub-btn-publish {
  padding: 8px 24px;
}

/* Modal transition */
.pub-modal-enter-active,
.pub-modal-leave-active {
  transition: all 0.25s cubic-bezier(0.16, 1, 0.3, 1);
}

.pub-modal-enter-active .pub-modal,
.pub-modal-leave-active .pub-modal {
  transition: all 0.25s cubic-bezier(0.16, 1, 0.3, 1);
}

.pub-modal-leave-active {
  transition: all 0.15s ease;
}

.pub-modal-leave-active .pub-modal {
  transition: all 0.15s ease;
}

.pub-modal-enter-from,
.pub-modal-leave-to {
  opacity: 0;
}

.pub-modal-enter-from .pub-modal {
  transform: scale(0.95) translateY(8px);
  opacity: 0;
}

.pub-modal-leave-to .pub-modal {
  transform: scale(0.98) translateY(4px);
  opacity: 0;
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
  display: inline-flex;
  align-items: center;
  gap: 2px;
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
  box-shadow: 0 8px 32px rgba(48, 209, 88, 0.4),
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
  box-shadow: 0 8px 40px rgba(245, 158, 11, 0.45),
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

.alt-text-header .label {
  font-size: 10px;
  font-weight: 600;
  color: var(--text-tertiary);
  text-transform: uppercase;
  letter-spacing: 0.75px;
}

.alt-text-header .count {
  font-size: 10px;
  font-weight: 600;
}

.alt-text-header .count.warning {
  color: var(--warning);
}

.alt-text-hint {
  font-size: 10px;
  color: var(--text-tertiary);
}

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
  display: inline-flex;
  align-items: center;
  justify-content: center;
  gap: 5px;
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

.tool-btn svg {
  flex-shrink: 0;
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

.publish-unlisted-btn {
  background: rgba(99, 102, 241, 0.15) !important;
  color: #818cf8 !important;
  border: 1px solid rgba(99, 102, 241, 0.3) !important;
}

.publish-unlisted-btn:hover {
  background: rgba(99, 102, 241, 0.25) !important;
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

/* Crown button */
.crown-btn {
  background: rgba(245, 158, 11, 0.15) !important;
  color: #f59e0b !important;
  border: 1px solid rgba(245, 158, 11, 0.2) !important;
}
.crown-btn:hover:not(:disabled) {
  background: rgba(245, 158, 11, 0.25) !important;
}

.crowned-badge {
  display: inline-flex;
  align-items: center;
  gap: 3px;
  font-size: 9px;
  font-weight: 600;
  color: #f59e0b;
  padding: 3px 8px;
  border-radius: 6px;
  background: rgba(245, 158, 11, 0.1);
  letter-spacing: 0.03em;
}

/* Webmention styles */
.webmention-btn {
  background: rgba(99, 102, 241, 0.15) !important;
  color: #818cf8 !important;
  border: 1px solid rgba(99, 102, 241, 0.2) !important;
}

.webmention-btn:hover:not(:disabled) {
  background: rgba(99, 102, 241, 0.25) !important;
}

/* Syndication */
.syndication-report {
  padding: 8px 16px;
  border-bottom: 1px solid var(--border);
}

.syndication-item {
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: 10px;
}

.syndication-item.success .syndication-icon { color: #22c55e; }
.syndication-item.error .syndication-icon { color: #ef4444; }
.syndication-platform { font-weight: 600; text-transform: capitalize; }
.syndication-link { color: var(--text-tertiary); text-decoration: underline; }
.syndication-error { color: var(--warning); }

.syndicate-btn { color: #a78bfa !important; }

.webmention-report {
  margin: 0 16px 8px;
  background: var(--bg-tertiary);
  border: 1px solid var(--border);
  border-radius: 6px;
  overflow: hidden;
}

.webmention-header {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 6px 10px;
  border-bottom: 1px solid var(--border);
  font-size: 10px;
}

.webmention-title {
  font-weight: 600;
  color: #818cf8;
  display: flex;
  align-items: center;
  gap: 4px;
}

.webmention-stats {
  flex: 1;
  display: flex;
  gap: 8px;
  font-family: 'SF Mono', monospace;
  font-size: 9px;
}

.wm-sent { color: var(--success); }
.wm-none { color: var(--text-tertiary); }
.wm-err { color: var(--error, #ef4444); }

.wm-close {
  background: none;
  border: none;
  color: var(--text-tertiary);
  cursor: pointer;
  font-size: 14px;
  padding: 0 2px;
  line-height: 1;
}

.webmention-list {
  max-height: 120px;
  overflow-y: auto;
}

.wm-item {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 3px 10px;
  font-size: 9px;
  font-family: 'SF Mono', monospace;
  border-bottom: 1px solid color-mix(in srgb, var(--border) 50%, transparent);
}

.wm-item:last-child { border-bottom: none; }

.wm-status-dot {
  width: 5px;
  height: 5px;
  border-radius: 50%;
  flex-shrink: 0;
}

.wm-item.sent .wm-status-dot { background: var(--success); }
.wm-item.no_endpoint .wm-status-dot { background: var(--text-tertiary); }
.wm-item.error .wm-status-dot { background: var(--error, #ef4444); }

.wm-target {
  color: var(--text-secondary);
  text-decoration: none;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  flex: 1;
}

.wm-target:hover { color: var(--text-primary); }

.wm-msg {
  color: var(--text-tertiary);
  flex-shrink: 0;
}

.wm-bridgy {
  margin: 6px 10px;
  font-size: 9px;
}
</style>
