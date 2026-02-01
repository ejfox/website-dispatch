<script setup lang="ts">
import { ref, watch, computed } from 'vue'
import { invoke } from '@tauri-apps/api/tauri'
import { ExternalLink, Copy, Lock, Eye, Check, AlertTriangle, Command } from 'lucide-vue-next'
import LocalMediaFixer from './LocalMediaFixer.vue'

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

const props = defineProps<{ file: MarkdownFile }>()
const emit = defineEmits<{ published: [] }>()

interface GitStatus {
  ok: boolean
  branch: string
  error: string | null
  dirty_files: string[]
  has_conflicts: boolean
}

const content = ref('')
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

// Tag suggestions
const availableTags = ref<Record<string, number>>({})
const suggestedTags = ref<string[]>([])
const loadingTags = ref(false)

// Fetch available tags from website
async function fetchAvailableTags() {
  if (Object.keys(availableTags.value).length > 0) return // Already loaded
  loadingTags.value = true
  try {
    const res = await fetch('https://ejfox.com/content-tags.json')
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
  justPublished.value = null
  backlinks.value = []
  localMedia.value = []
  showMediaFixer.value = false

  // Set this file as the preview target (Node server for accurate rendering)
  fetch('http://127.0.0.1:6419/set-file', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ path: file.path })
  }).catch(() => {})

  // Also tell Rust server (fallback)
  invoke('set_preview_file', { path: file.path })

  try {
    content.value = await invoke('get_file_content', { path: file.path })
  } catch (e) {
    content.value = `Error: ${e}`
  }

  // Fetch backlinks from Obsidian API
  loadingBacklinks.value = true
  try {
    backlinks.value = await invoke('get_backlinks', { filename: file.filename })
  } catch (e) {
    console.log('Backlinks unavailable:', e)
  }
  loadingBacklinks.value = false

  // Fetch local media references
  loadingLocalMedia.value = true
  try {
    localMedia.value = await invoke('get_local_media', { path: file.path })
  } catch (e) {
    console.log('Local media detection unavailable:', e)
  }
  loadingLocalMedia.value = false

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
setInterval(checkGitStatus, 10000)

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

const targetUrl = computed(() =>
  `ejfox.com/blog/${new Date().getFullYear()}/${slug.value}`
)

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

function formatAge(ts: number): string {
  const seconds = Math.floor(Date.now() / 1000 - ts)
  const minutes = Math.floor(seconds / 60)
  const hours = Math.floor(minutes / 60)
  const days = Math.floor(hours / 24)

  if (seconds < 60) return 'just now'
  if (minutes < 60) return `${minutes} min ago`
  if (hours < 24) return `${hours} hours ago`
  if (days === 1) return 'yesterday'
  if (days < 7) return `${days} days ago`
  if (days < 30) return `${Math.floor(days / 7)} weeks ago`
  if (days < 365) return `${Math.floor(days / 30)} months ago`
  return `${Math.floor(days / 365)} years ago`
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
      slug: slug.value
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
    await invoke('unpublish_file', { slug: slug.value })
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
  // Pass full path to backend which knows the vault path
  await invoke('open_in_obsidian', { path: props.file.path })
}

async function openInIAWriter() {
  await invoke('open_in_app', { path: props.file.path, app: 'iA Writer' })
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
      <h1 :class="{ 'derived-title': titleIsDerived }">{{ title }}</h1>
      <p v-if="titleIsDerived" class="title-hint">Title derived from filename</p>
      <p v-if="file.dek" class="dek">{{ file.dek }}</p>
    </div>

    <!-- Info -->
  <div class="info">
      <div v-if="liveUrl" class="row live-url-row">
        <span class="label">Live URL</span>
        <div class="live-url">
          <code>{{ liveUrl }}</code>
          <button class="mini-btn" @click="copyUrl">Copy</button>
        </div>
      </div>
      <div class="row">
        <span class="label">Source</span>
        <code>{{ file.source_dir || '.' }}/{{ file.filename }}</code>
      </div>
      <div class="row">
        <span class="label">Target</span>
        <code>{{ targetUrl }}</code>
      </div>
      <div class="row">
        <span class="label">Created</span>
        <span>{{ formatAge(file.created) }}</span>
      </div>
      <div class="row">
        <span class="label">Edited</span>
        <span :class="{ 'modified-highlight': hasUnpublishedChanges }">{{ formatAge(file.modified) }}</span>
      </div>
      <div v-if="file.published_date" class="row">
        <span class="label">Published</span>
        <span class="published">{{ formatAge(file.published_date) }}</span>
      </div>
      <div class="row">
        <span class="label">Date</span>
        <span :class="{ missing: !file.date }">{{ file.date || 'none' }}</span>
      </div>
      <div class="row">
        <span class="label">Words</span>
        <span>{{ file.word_count }}</span>
      </div>
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
    </div>

    <!-- Lint Receipt -->
    <div class="lint-receipt">
      <div class="lint-receipt-header">
        <span>Lint Receipt</span>
        <span class="lint-receipt-count">{{ lintWarnings.length }}</span>
      </div>
      <div class="lint-receipt-divider"></div>
      <div v-if="lintWarnings.length" class="lint-receipt-list">
        <div v-for="warning in lintWarnings" :key="warning" class="lint-receipt-item">
          <span class="lint-receipt-bullet">•</span>
          <span class="lint-receipt-text">{{ warning }}</span>
        </div>
      </div>
      <div v-else class="lint-receipt-ok">All clear</div>
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

    <!-- Open In -->
    <div class="open-in">
      <button @click="openInObsidian" class="open-btn" title="Open in Obsidian">
        <span class="icon">O</span>
        <span class="label">Obsidian</span>
      </button>
      <button @click="openInIAWriter" class="open-btn" title="Open in iA Writer">
        <span class="icon">iA</span>
        <span class="label">iA Writer</span>
      </button>
      <button @click="openPreview" class="open-btn preview" title="Live Preview">
        <span class="icon">▶</span>
        <span class="label">Preview</span>
      </button>
    </div>

    <!-- Publish Confirmation -->
    <div v-if="showPublishConfirm" class="modal-overlay">
      <div class="publish-confirm">
        <div class="publish-confirm-header">
          <h2>{{ publishConfirmRepublish ? 'Republish Confirmation' : 'Publish Confirmation' }}</h2>
          <button class="close-btn" @click="closePublishConfirm">×</button>
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

    <!-- Actions -->
    <div class="actions">
      <template v-if="isLive">
        <a :href="liveUrl!" target="_blank" class="btn">View on site →</a>
        <button @click="unpublish" :disabled="unpublishing" class="btn">
          {{ unpublishing ? '...' : 'Unpublish' }}
        </button>
        <button @click="openPublishConfirm(true)" :disabled="publishing" class="btn accent">
          {{ publishing ? '...' : 'Republish' }}
        </button>
      </template>
      <template v-else>
        <button
          @click="openPublishConfirm(false)"
          :disabled="!file.is_safe || publishing"
          class="btn accent full publish-btn"
          :class="{ disabled: !file.is_safe }"
        >
          <span>{{ publishing ? 'Publishing...' : file.is_safe ? 'Publish' : 'Fix issues to publish' }}</span>
          <kbd v-if="file.is_safe && !publishing" class="shortcut-hint">
            <Command :size="10" />↵
          </kbd>
        </button>
      </template>
    </div>

    <!-- Preview -->
    <div class="preview">
      <pre>{{ content }}</pre>
    </div>
  </div>
</template>

<style scoped>
.panel {
  flex: 1;
  display: flex;
  flex-direction: column;
  min-width: 0;
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

/* Lint Receipt */
.lint-receipt {
  margin: 10px 16px 12px;
  padding: 10px 12px;
  background: color-mix(in srgb, var(--bg-secondary) 70%, #f7f2e8);
  border: 1px dashed var(--border-light);
  border-radius: 6px;
  font-family: 'SF Mono', monospace;
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
    color-mix(in srgb, var(--border-light) 80%, transparent) 0 6px,
    transparent 6px 10px
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

.lint-receipt-ok {
  color: var(--success);
  letter-spacing: 0.06em;
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

.live-url-row {
  align-items: center;
}

.live-url {
  display: flex;
  align-items: center;
  gap: 8px;
  min-width: 0;
}

.live-url code {
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  max-width: 260px;
}

.mini-btn {
  padding: 2px 6px;
  font-size: 9px;
  border: 1px solid var(--border);
  border-radius: 4px;
  background: var(--bg-tertiary);
  color: var(--text-secondary);
  cursor: pointer;
}

.mini-btn:hover {
  color: var(--text-primary);
  border-color: var(--border-light);
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
  color: var(--text-tertiary);
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

/* Open In */
.open-in {
  padding: 8px 16px;
  display: flex;
  gap: 6px;
  border-bottom: 1px solid var(--border);
}

.open-btn {
  flex: 1;
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 2px;
  padding: 10px 4px;
  background: var(--accent);
  border: 1px solid var(--border);
  border-radius: 8px;
  cursor: pointer;
  transition: all 0.15s cubic-bezier(0.34, 1.56, 0.64, 1);
  min-height: 48px;
}

.open-btn:hover {
  background: var(--bg-tertiary);
  border-color: var(--border-light);
  transform: translateY(-1px);
}

.open-btn:active {
  transform: translateY(0);
}

.open-btn .icon {
  font-size: 12px;
  font-weight: 700;
  color: var(--text-secondary);
  font-family: 'SF Mono', monospace;
  transition: transform 0.15s ease;
}

.open-btn:hover .icon {
  transform: scale(1.1);
}

.open-btn .label {
  font-size: 9px;
  color: var(--text-tertiary);
  transition: color 0.15s ease;
}

.open-btn:hover .label {
  color: var(--text-secondary);
}

.open-btn.preview {
  background: var(--bg-tertiary);
  border-color: var(--border-light);
}

.open-btn.preview .icon {
  color: var(--text-secondary);
}

.open-btn.preview:hover {
  background: var(--bg-secondary);
  border-color: var(--border-light);
}

/* Actions */
.actions {
  padding: 10px 16px;
  display: flex;
  gap: 8px;
  border-bottom: 1px solid var(--border);
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
  background: var(--bg-tertiary);
  color: var(--text-primary);
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
  font-family: 'SF Mono', 'Fira Code', monospace;
  font-size: 10px;
  line-height: 1.6;
  color: var(--text-tertiary);
  white-space: pre-wrap;
  word-wrap: break-word;
  margin: 0;
  tab-size: 2;
}
</style>
