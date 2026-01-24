<script setup lang="ts">
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

defineProps<{
  files: MarkdownFile[]
  selected: MarkdownFile | null
  loading: boolean
}>()

const emit = defineEmits<{ select: [file: MarkdownFile] }>()

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

    <div v-if="loading" class="loading">Loading...</div>

    <div v-else class="list">
      <button
        v-for="file in files"
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
            <span v-if="file.published_url" class="live-badge">LIVE</span>
            <span class="title">{{ formatTitle(file) }}</span>
          </div>
          <div class="meta">
            <span v-if="file.source_dir" class="dir">{{ file.source_dir }}/</span>
            <span v-if="file.published_date" class="pub-date">pub {{ formatAge(file.published_date) }}</span>
            <span v-else>{{ formatAge(file.created) }} → {{ formatAge(file.modified) }}</span>
            <span>{{ file.word_count }}w</span>
            <span v-if="file.warnings.length" class="warn">{{ file.warnings.length }}</span>
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
  background: var(--bg-secondary);
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

.item:hover { background: var(--bg-tertiary); }

.item.selected {
  background: var(--accent);
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

.item.selected .live-badge {
  background: #fff;
  color: var(--accent);
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

.warn {
  color: var(--warning);
}

.pub-date {
  color: var(--success);
}
</style>
