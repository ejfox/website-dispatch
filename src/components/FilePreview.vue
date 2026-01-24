<script setup lang="ts">
import { ref, watch, computed } from 'vue'
import { invoke } from '@tauri-apps/api/tauri'

interface MarkdownFile {
  path: string
  filename: string
  title: string | null
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
}

interface Backlink {
  path: string
  title: string | null
  context: string
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

watch(() => props.file, async (file) => {
  justPublished.value = null
  backlinks.value = []

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

const title = computed(() =>
  props.file.title || props.file.filename.replace(/\.md$/, '').replace(/-/g, ' ')
)

const slug = computed(() => props.file.filename.replace('.md', ''))

const targetUrl = computed(() =>
  `ejfox.com/blog/${new Date().getFullYear()}/${slug.value}`
)

const isLive = computed(() => !!props.file.published_url || !!justPublished.value)
const liveUrl = computed(() => props.file.published_url || justPublished.value)
const hasUnpublishedChanges = computed(() => props.file.warnings.includes('Modified since publish'))

function formatAge(ts: number): string {
  const days = Math.floor((Date.now() / 1000 - ts) / 86400)
  if (days === 0) return 'today'
  if (days === 1) return 'yesterday'
  if (days < 7) return `${days} days ago`
  if (days < 30) return `${Math.floor(days / 7)} weeks ago`
  if (days < 365) return `${Math.floor(days / 30)} months ago`
  return `${Math.floor(days / 365)} years ago`
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

    // Show success toast
    successMessage.value = isRepublish ? 'Republished!' : 'Published!'
    showSuccess.value = true
    setTimeout(() => { showSuccess.value = false }, 3000)

    // Delay refresh slightly to let filesystem settle
    setTimeout(() => emit('published'), 500)
  } catch (e) {
    alert(`Failed: ${e}`)
  }
  publishing.value = false
}

function copyUrl() {
  if (liveUrl.value) navigator.clipboard.writeText(liveUrl.value)
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
  <div class="panel" :class="{ live: isLive }">
    <!-- Success Toast -->
    <Transition name="toast">
      <div v-if="showSuccess" class="success-toast">
        {{ successMessage }}
      </div>
    </Transition>

    <!-- Status Banner -->
    <div v-if="isLive && hasUnpublishedChanges" class="banner modified">
      <span class="banner-text">MODIFIED</span>
      <span>Source changed since last publish</span>
      <button @click="publish(true)" :disabled="publishing">{{ publishing ? '...' : 'Republish' }}</button>
    </div>
    <div v-else-if="isLive" class="banner live">
      <span class="banner-text">LIVE</span>
      <a :href="liveUrl!" target="_blank">{{ liveUrl }}</a>
      <button @click="copyUrl">Copy</button>
    </div>
    <div v-else-if="!file.is_safe" class="banner warn">
      {{ file.warnings.join(' · ') }}
    </div>
    <div v-else class="banner ready">
      Ready to publish
    </div>

    <!-- Header -->
    <div class="header">
      <h1>{{ title }}</h1>
    </div>

    <!-- Info -->
    <div class="info">
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
        <span>{{ file.tags.join(', ') }}</span>
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

    <!-- Actions -->
    <div class="actions">
      <template v-if="isLive">
        <a :href="liveUrl!" target="_blank" class="btn">View on site →</a>
        <button @click="publish(true)" :disabled="publishing" class="btn accent">
          {{ publishing ? '...' : 'Republish' }}
        </button>
      </template>
      <template v-else>
        <button
          @click="publish"
          :disabled="!file.is_safe || publishing"
          class="btn accent full"
          :class="{ disabled: !file.is_safe }"
        >
          {{ publishing ? 'Publishing...' : file.is_safe ? 'Publish Now' : 'Fix issues to publish' }}
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
  background: rgba(20, 20, 22, 0.6);
  backdrop-filter: blur(12px);
  -webkit-backdrop-filter: blur(12px);
}

.panel.live {
  background: linear-gradient(180deg, rgba(48, 209, 88, 0.08) 0%, rgba(20, 20, 22, 0.6) 200px);
}

/* Banner */
.banner {
  padding: 8px 16px;
  font-size: 11px;
  display: flex;
  align-items: center;
  gap: 12px;
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
  background: rgba(10, 132, 255, 0.1);
  color: var(--accent);
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

.row code {
  font-family: 'SF Mono', monospace;
  font-size: 10px;
  color: var(--text-secondary);
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
  font-size: 18px;
  font-weight: 600;
  z-index: 100;
  box-shadow: 0 8px 32px rgba(48, 209, 88, 0.4);
}

.toast-enter-active,
.toast-leave-active {
  transition: all 0.3s ease;
}

.toast-enter-from,
.toast-leave-to {
  opacity: 0;
  transform: translate(-50%, -50%) scale(0.8);
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
  letter-spacing: 0.5px;
}

.backlinks-header .count {
  font-size: 10px;
  color: var(--accent);
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
  padding: 8px 4px;
  background: rgba(255, 255, 255, 0.05);
  border: 1px solid rgba(255, 255, 255, 0.06);
  border-radius: 6px;
  cursor: pointer;
  transition: all 0.15s ease;
}

.open-btn:hover {
  background: rgba(255, 255, 255, 0.1);
  border-color: rgba(255, 255, 255, 0.1);
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
}

.open-btn .label {
  font-size: 9px;
  color: var(--text-tertiary);
}

.open-btn.preview {
  background: rgba(10, 132, 255, 0.15);
  border-color: rgba(10, 132, 255, 0.2);
}

.open-btn.preview .icon {
  color: var(--accent);
}

.open-btn.preview:hover {
  background: rgba(10, 132, 255, 0.25);
  border-color: rgba(10, 132, 255, 0.3);
}

/* Actions */
.actions {
  padding: 10px 16px;
  display: flex;
  gap: 8px;
  border-bottom: 1px solid var(--border);
}

.btn {
  padding: 6px 14px;
  border: none;
  border-radius: 4px;
  font-size: 11px;
  font-weight: 500;
  cursor: pointer;
  text-decoration: none;
  background: var(--bg-tertiary);
  color: var(--text-primary);
}

.btn:hover {
  filter: brightness(1.1);
}

.btn.accent {
  background: var(--accent);
  color: #fff;
  box-shadow: 0 2px 8px rgba(10, 132, 255, 0.3);
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

/* Preview */
.preview {
  flex: 1;
  overflow-y: auto;
  padding: 12px 16px;
}

.preview pre {
  font-family: 'SF Mono', monospace;
  font-size: 10px;
  line-height: 1.5;
  color: var(--text-tertiary);
  white-space: pre-wrap;
  word-wrap: break-word;
  margin: 0;
}
</style>
