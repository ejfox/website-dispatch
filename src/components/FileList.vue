<script setup lang="ts">
import { ref, computed } from 'vue'

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

const props = defineProps<{
  files: MarkdownFile[]
  selected: MarkdownFile | null
  loading: boolean
  compact?: boolean
}>()

const emit = defineEmits<{ select: [file: MarkdownFile] }>()

const filter = ref<'all' | 'published' | 'drafts'>('all')
const sort = ref<'recent' | 'created' | 'title' | 'words'>('recent')

const filteredFiles = computed(() => {
  let result = props.files

  // Filter
  if (filter.value === 'published') {
    result = result.filter(f => f.published_url)
  } else if (filter.value === 'drafts') {
    result = result.filter(f => !f.published_url)
  }

  // Sort
  result = [...result].sort((a, b) => {
    switch (sort.value) {
      case 'recent':
        // Use published_date if available, otherwise modified
        const aRecent = a.published_date || a.modified
        const bRecent = b.published_date || b.modified
        return bRecent - aRecent
      case 'created':
        return b.created - a.created
      case 'title':
        return formatTitle(a).localeCompare(formatTitle(b))
      case 'words':
        return b.word_count - a.word_count
      default:
        return 0
    }
  })

  return result
})

const counts = computed(() => ({
  all: props.files.length,
  published: props.files.filter(f => f.published_url).length,
  drafts: props.files.filter(f => !f.published_url).length
}))

// Group files by time period (only for 'recent' sort)
const groupedFiles = computed(() => {
  if (sort.value !== 'recent') {
    return [{ label: null, files: filteredFiles.value }]
  }

  const now = Date.now() / 1000
  const dayAgo = now - 86400
  const weekAgo = now - 86400 * 7
  const monthAgo = now - 86400 * 30

  const groups: { label: string | null; files: MarkdownFile[] }[] = []

  const today: MarkdownFile[] = []
  const thisWeek: MarkdownFile[] = []
  const thisMonth: MarkdownFile[] = []
  const earlier: MarkdownFile[] = []

  for (const file of filteredFiles.value) {
    const ts = file.published_date || file.modified
    if (ts > dayAgo) {
      today.push(file)
    } else if (ts > weekAgo) {
      thisWeek.push(file)
    } else if (ts > monthAgo) {
      thisMonth.push(file)
    } else {
      earlier.push(file)
    }
  }

  if (today.length) groups.push({ label: 'Today', files: today })
  if (thisWeek.length) groups.push({ label: 'This Week', files: thisWeek })
  if (thisMonth.length) groups.push({ label: 'This Month', files: thisMonth })
  if (earlier.length) groups.push({ label: 'Earlier', files: earlier })

  return groups
})

function formatTitle(file: MarkdownFile): string {
  return file.title || file.filename.replace(/\.md$/, '').replace(/-/g, ' ')
}

function formatAge(ts: number): string {
  const seconds = Math.floor(Date.now() / 1000 - ts)
  const minutes = Math.floor(seconds / 60)
  const hours = Math.floor(minutes / 60)
  const days = Math.floor(hours / 24)

  if (seconds < 60) return 'just now'
  if (minutes < 60) return `${minutes}m`
  if (hours < 24) return `${hours}h`
  if (days === 1) return 'yesterday'
  if (days < 7) return `${days}d`
  if (days < 30) return `${Math.floor(days / 7)}w`
  if (days < 365) return `${Math.floor(days / 30)}mo`
  return `${Math.floor(days / 365)}y`
}

function formatWordCount(count: number): string {
  if (count >= 1000) return `${(count / 1000).toFixed(1)}k`
  return `${count}`
}

function getAgeColor(ts: number): string {
  const days = Math.min(Math.floor((Date.now() / 1000 - ts) / 86400), 365)
  const t = days / 365
  if (t < 0.1) return '#fde724'
  if (t < 0.25) return '#f89540'
  if (t < 0.4) return '#e45a31'
  if (t < 0.55) return '#c42d52'
  if (t < 0.7) return '#8b1a79'
  if (t < 0.85) return '#4e179a'
  return '#30123b'
}
</script>

<template>
  <aside class="sidebar" :class="{ compact }">
    <div class="header">
      {{ files.length }} files
      <span class="live">{{ files.filter(f => f.published_url).length }} LIVE</span>
    </div>

    <div class="filters">
      <button
        :class="{ active: filter === 'all' }"
        @click="filter = 'all'"
      >All ({{ counts.all }})</button>
      <button
        :class="{ active: filter === 'published' }"
        @click="filter = 'published'"
      >Live ({{ counts.published }})</button>
      <button
        :class="{ active: filter === 'drafts' }"
        @click="filter = 'drafts'"
      >Unpublished ({{ counts.drafts }})</button>
    </div>

    <div class="sort-row">
      <button :class="{ active: sort === 'recent' }" @click="sort = 'recent'">Recent</button>
      <button :class="{ active: sort === 'created' }" @click="sort = 'created'">Created</button>
      <button :class="{ active: sort === 'title' }" @click="sort = 'title'">Title</button>
      <button :class="{ active: sort === 'words' }" @click="sort = 'words'">Words</button>
    </div>

    <div v-if="loading" class="loading">Loading...</div>

    <div v-else class="list">
      <template v-for="group in groupedFiles" :key="group.label">
        <div v-if="group.label" class="group-header">
          {{ group.label }}
          <span class="group-count">{{ group.files.length }}</span>
        </div>
        <button
          v-for="file in group.files"
          :key="file.path"
          class="item"
          :class="{
            selected: selected?.path === file.path,
            published: !!file.published_url
          }"
          :style="{ '--age': getAgeColor(file.created) }"
          @click="emit('select', file)"
        >
          <div class="age-bar"></div>
          <div class="content">
            <div class="row">
              <span v-if="file.password && !file.published_url" class="protected-badge-draft" title="Will be password-protected">🔒</span>
              <span v-else-if="file.unlisted && !file.published_url" class="unlisted-badge-draft" title="Will be unlisted">👁</span>
              <span class="title" :title="formatTitle(file)">{{ formatTitle(file) }}</span>
              <span v-if="file.published_url && file.warnings.includes('Modified since publish')" class="modified-badge">MODIFIED</span>
              <span v-else-if="file.published_url && file.password" class="protected-badge">🔒 PROTECTED</span>
              <span v-else-if="file.published_url && file.unlisted" class="unlisted-badge">👁 UNLISTED</span>
              <span v-else-if="file.published_url" class="live-badge">✓ LIVE</span>
            </div>
            <div v-if="file.dek" class="dek">{{ file.dek }}</div>
            <div class="meta">
              <span v-if="file.source_dir" class="dir">{{ file.source_dir }}/</span>
              <template v-if="file.published_date">
                <span class="pub-date">published {{ formatAge(file.published_date) }}</span>
                <span v-if="file.warnings.includes('Modified since publish')" class="edit-date">edited {{ formatAge(file.modified) }}</span>
              </template>
              <template v-else>
                <span class="create-date">created {{ formatAge(file.created) }}</span>
                <span class="edit-date">edited {{ formatAge(file.modified) }}</span>
              </template>
              <span class="word-count">{{ formatWordCount(file.word_count) }}w</span>
            </div>
          </div>
        </button>
      </template>
    </div>
  </aside>
</template>

<style scoped>
.sidebar {
  flex: 0.382; /* Golden ratio: smaller section */
  min-width: 280px;
  max-width: 500px;
  border-right: 1px solid var(--border);
  /* Arc-style soft gradient */
  background: linear-gradient(
    180deg,
    var(--bg-secondary) 0%,
    color-mix(in srgb, var(--bg-secondary) 95%, var(--bg-tertiary)) 100%
  );
  backdrop-filter: blur(16px) saturate(180%);
  -webkit-backdrop-filter: blur(16px) saturate(180%);
  display: flex;
  flex-direction: column;
}

.header {
  padding: 8px 12px;
  font-size: 10px;
  color: var(--text-tertiary);
  border-bottom: 1px solid var(--border);
  display: flex;
  justify-content: space-between;
}

.header .live {
  color: var(--success);
  font-weight: 600;
  display: flex;
  align-items: center;
  gap: 4px;
}

.header .live::before {
  content: '';
  width: 6px;
  height: 6px;
  background: var(--success);
  border-radius: 50%;
  animation: pulse 2s ease-in-out infinite;
}

@keyframes pulse {
  0%, 100% { opacity: 1; transform: scale(1); }
  50% { opacity: 0.5; transform: scale(0.8); }
}

.filters {
  display: flex;
  gap: 0;
  border-bottom: 1px solid var(--border);
}

.filters button {
  flex: 1;
  padding: 6px 8px;
  font-size: 9px;
  font-weight: 500;
  text-transform: uppercase;
  letter-spacing: 0.75px;
  background: transparent;
  border: none;
  border-right: 1px solid var(--border);
  color: var(--text-tertiary);
  cursor: pointer;
  transition: all var(--transition-fast, 0.15s cubic-bezier(0.34, 1.56, 0.64, 1));
  position: relative;
}

.filters button::after {
  content: '';
  position: absolute;
  bottom: 0;
  left: 50%;
  width: 0;
  height: 2px;
  background: var(--text-primary);
  transition: all 0.2s ease;
  transform: translateX(-50%);
}

.filters button.active::after {
  width: 60%;
}

.filters button:last-child {
  border-right: none;
}

.filters button:hover {
  background: var(--accent);
  color: var(--text-secondary);
}

.filters button.active {
  background: var(--bg-tertiary);
  color: var(--text-primary);
  font-weight: 600;
}

.sort-row {
  display: flex;
  gap: 0;
  border-bottom: 1px solid var(--border);
  background: var(--bg-tertiary);
}

.sort-row button {
  flex: 1;
  padding: 4px 6px;
  font-size: 8px;
  font-weight: 500;
  text-transform: uppercase;
  letter-spacing: 0.5px;
  background: transparent;
  border: none;
  border-right: 1px solid var(--border);
  color: var(--text-tertiary);
  cursor: pointer;
  transition: all var(--transition-fast, 0.15s cubic-bezier(0.34, 1.56, 0.64, 1));
}

.sort-row button:last-child {
  border-right: none;
}

.sort-row button:hover {
  background: var(--accent);
  color: var(--text-secondary);
}

.sort-row button.active {
  background: var(--accent);
  color: var(--text-primary);
}

.loading {
  padding: 40px 20px;
  text-align: center;
  color: var(--text-tertiary);
  font-size: 11px;
}

.loading::after {
  content: '';
  display: inline-block;
  width: 12px;
  height: 12px;
  margin-left: 8px;
  border: 2px solid var(--border-light);
  border-top-color: var(--text-secondary);
  border-radius: 50%;
  animation: spin 0.8s linear infinite;
}

@keyframes spin {
  to { transform: rotate(360deg); }
}

.list {
  flex: 1;
  overflow-y: auto;
  scroll-behavior: smooth;
}

.group-header {
  padding: 6px 12px;
  font-size: 9px;
  font-weight: 600;
  text-transform: uppercase;
  letter-spacing: 0.75px;
  color: var(--text-tertiary);
  background: var(--bg-tertiary);
  border-bottom: 1px solid var(--border);
  display: flex;
  justify-content: space-between;
  align-items: center;
  position: sticky;
  top: 0;
  z-index: 1;
}

.group-count {
  font-weight: 400;
  opacity: 0.7;
}

.list::-webkit-scrollbar {
  width: 6px;
}

.list::-webkit-scrollbar-track {
  background: transparent;
}

.list::-webkit-scrollbar-thumb {
  background: var(--border-light);
  border-radius: 3px;
}

.list::-webkit-scrollbar-thumb:hover {
  background: var(--text-tertiary);
}

.item {
  width: 100%;
  display: flex;
  gap: 0;
  padding: 0;
  border: none;
  border-bottom: 1px solid var(--border);
  background: transparent;
  cursor: pointer;
  text-align: left;
  transition: all var(--transition-fast, 0.15s cubic-bezier(0.34, 1.56, 0.64, 1));
}

.item:hover {
  background: var(--accent);
}

.item:active {
  transform: scale(0.995);
}

.item.selected {
  background: var(--selection-bg);
  backdrop-filter: blur(8px);
  -webkit-backdrop-filter: blur(8px);
  box-shadow: var(--shadow-glow, 0 0 0 1px var(--selection-bg), 0 2px 8px color-mix(in srgb, var(--selection-bg) 30%, transparent));
}

.item.selected .title,
.item.selected .meta,
.item.selected .dir {
  color: var(--selection-text, #fff);
}

.item.published {
  /* Subtle indicator - no screaming green border */
}

.item.published.selected {
  border-left-color: #fff;
}

.age-bar {
  width: 4px;
  background: var(--age);
  flex-shrink: 0;
}

.item.published .age-bar {
  display: none;
}

.content {
  flex: 1;
  padding: 6px 10px;
  min-width: 0;
}

.row {
  display: flex;
  align-items: center;
  gap: 6px;
}

.live-badge {
  font-size: 8px;
  font-weight: 500;
  background: transparent;
  color: var(--success);
  padding: 1px 4px;
  border-radius: 2px;
  flex-shrink: 0;
  opacity: 0.7;
}

.modified-badge {
  font-size: 8px;
  font-weight: 700;
  background: var(--warning);
  color: #000;
  padding: 1px 4px;
  border-radius: 2px;
  flex-shrink: 0;
}

.unlisted-badge {
  font-size: 8px;
  font-weight: 700;
  background: #6366f1;
  color: #fff;
  padding: 1px 4px;
  border-radius: 2px;
  flex-shrink: 0;
}

.protected-badge {
  font-size: 8px;
  font-weight: 700;
  background: #8b5cf6;
  color: #fff;
  padding: 1px 4px;
  border-radius: 2px;
  flex-shrink: 0;
}

.unlisted-badge-draft {
  font-size: 9px;
  flex-shrink: 0;
  padding: 1px 4px;
  border-radius: 2px;
  background: rgba(99, 102, 241, 0.2);
  color: #6366f1;
}

.protected-badge-draft {
  font-size: 9px;
  flex-shrink: 0;
  padding: 1px 4px;
  border-radius: 2px;
  background: rgba(139, 92, 246, 0.2);
  color: #8b5cf6;
}

.item.selected .live-badge,
.item.selected .modified-badge,
.item.selected .unlisted-badge,
.item.selected .protected-badge {
  background: #fff;
  color: #333;
}

.dek {
  font-size: 9px;
  color: var(--text-tertiary);
  margin-top: 3px;
  line-height: 1.4;
  display: -webkit-box;
  -webkit-line-clamp: 2;
  -webkit-box-orient: vertical;
  overflow: hidden;
  font-style: italic;
  opacity: 0.8;
}

.item.selected .dek {
  color: color-mix(in srgb, var(--selection-text) 70%, transparent);
}

.title {
  flex: 1;
  font-size: 11px;
  font-weight: 500;
  color: var(--text-primary);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  min-width: 0;
}

.meta {
  margin-top: 2px;
  font-size: 9px;
  font-family: 'SF Mono', monospace;
  color: var(--text-tertiary);
  display: flex;
  gap: 8px;
}

.dir {
  color: var(--text-tertiary);
  max-width: 60px;
  overflow: hidden;
  text-overflow: ellipsis;
}

.pub-date {
  color: var(--success);
}

.create-date {
  color: var(--text-tertiary);
}

.edit-date {
  color: var(--text-tertiary);
}

.word-count {
  font-variant-numeric: tabular-nums;
  font-feature-settings: 'tnum';
  opacity: 0.7;
}

/* Compact Mode */
.sidebar.compact .item .content {
  padding: 4px 10px;
}

.sidebar.compact .dek {
  display: none;
}

.sidebar.compact .meta {
  display: none;
}

.sidebar.compact .item .row {
  gap: 4px;
}

.sidebar.compact .age-bar {
  width: 2px;
}

.sidebar.compact .group-header {
  padding: 4px 10px;
  font-size: 8px;
}
</style>
