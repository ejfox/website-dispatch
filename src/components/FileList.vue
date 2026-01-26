<script setup lang="ts">
import { ref, computed } from 'vue'

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

const props = defineProps<{
  files: MarkdownFile[]
  selected: MarkdownFile | null
  loading: boolean
}>()

const emit = defineEmits<{ select: [file: MarkdownFile] }>()

const filter = ref<'all' | 'published' | 'drafts'>('all')
const sort = ref<'modified' | 'created' | 'title' | 'words'>('modified')

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
      case 'modified':
        return b.modified - a.modified
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

function formatTitle(file: MarkdownFile): string {
  return file.title || file.filename.replace(/\.md$/, '').replace(/-/g, ' ')
}

function formatAge(ts: number): string {
  const days = Math.floor((Date.now() / 1000 - ts) / 86400)
  if (days === 0) return 'today'
  if (days < 30) return `${days}d`
  if (days < 365) return `${Math.floor(days / 30)}mo`
  return `${Math.floor(days / 365)}y`
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
  <aside class="sidebar">
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
      >Drafts ({{ counts.drafts }})</button>
    </div>

    <div class="sort-row">
      <button :class="{ active: sort === 'modified' }" @click="sort = 'modified'">Modified</button>
      <button :class="{ active: sort === 'created' }" @click="sort = 'created'">Created</button>
      <button :class="{ active: sort === 'title' }" @click="sort = 'title'">Title</button>
      <button :class="{ active: sort === 'words' }" @click="sort = 'words'">Words</button>
    </div>

    <div v-if="loading" class="loading">Loading...</div>

    <div v-else class="list">
      <button
        v-for="file in filteredFiles"
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
            <span v-if="file.published_url && file.warnings.includes('Modified since publish')" class="modified-badge">MODIFIED</span>
            <span v-else-if="file.published_url" class="live-badge">LIVE</span>
            <span class="title">{{ formatTitle(file) }}</span>
          </div>
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
            <span>{{ file.word_count }}w</span>
          </div>
        </div>
      </button>
    </div>
  </aside>
</template>

<style scoped>
.sidebar {
  width: 300px;
  min-width: 300px;
  border-right: 1px solid var(--border);
  background: rgba(25, 25, 28, 0.75);
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
  letter-spacing: 0.5px;
  background: transparent;
  border: none;
  border-right: 1px solid var(--border);
  color: var(--text-tertiary);
  cursor: pointer;
  transition: all 0.15s ease;
}

.filters button:last-child {
  border-right: none;
}

.filters button:hover {
  background: rgba(255, 255, 255, 0.05);
  color: var(--text-secondary);
}

.filters button.active {
  background: rgba(255, 255, 255, 0.1);
  color: var(--text-primary);
  font-weight: 600;
}

.sort-row {
  display: flex;
  gap: 0;
  border-bottom: 1px solid var(--border);
  background: rgba(0, 0, 0, 0.15);
}

.sort-row button {
  flex: 1;
  padding: 4px 6px;
  font-size: 8px;
  font-weight: 500;
  text-transform: uppercase;
  letter-spacing: 0.3px;
  background: transparent;
  border: none;
  border-right: 1px solid var(--border);
  color: var(--text-tertiary);
  cursor: pointer;
  transition: all 0.15s ease;
}

.sort-row button:last-child {
  border-right: none;
}

.sort-row button:hover {
  background: rgba(255, 255, 255, 0.05);
  color: var(--text-secondary);
}

.sort-row button.active {
  background: rgba(255, 255, 255, 0.08);
  color: var(--text-primary);
}

.loading {
  padding: 20px;
  text-align: center;
  color: var(--text-tertiary);
}

.list {
  flex: 1;
  overflow-y: auto;
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
}

.item:hover { background: rgba(255, 255, 255, 0.05); }

.item.selected {
  background: rgba(10, 132, 255, 0.85);
  backdrop-filter: blur(8px);
  -webkit-backdrop-filter: blur(8px);
}

.item.selected .title,
.item.selected .meta,
.item.selected .dir {
  color: #fff;
}

.item.published {
  border-left: 3px solid var(--success);
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
  font-weight: 700;
  background: var(--success);
  color: #000;
  padding: 1px 4px;
  border-radius: 2px;
  flex-shrink: 0;
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

.item.selected .live-badge,
.item.selected .modified-badge {
  background: #fff;
  color: #333;
}

.title {
  font-size: 11px;
  font-weight: 500;
  color: var(--text-primary);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
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
</style>
