<script setup lang="ts">
import { ref, computed } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { Menu, MenuItem, PredefinedMenuItem } from '@tauri-apps/api/menu'
import {
  PhLockSimple,
  PhEye,
  PhClock,
  PhCheckCircle,
  PhFolderOpen,
  PhPencilSimple,
  PhTextAa,
  PhCalendarBlank,
  PhTextAlignLeft,
  PhListNumbers,
  PhStack,
  PhBroadcast,
  PhFileText,
  PhClockCounterClockwise,
  PhNotebook,
} from '@phosphor-icons/vue'
import type { MarkdownFile } from '../types'

const props = defineProps<{
  files: MarkdownFile[]
  selected: MarkdownFile | null
  loading: boolean
  compact?: boolean
}>()

const emit = defineEmits<{ select: [file: MarkdownFile] }>()

const filter = ref<'all' | 'published' | 'drafts' | 'scheduled'>('all')
const sort = ref<'recent' | 'created' | 'title' | 'words'>('recent')
const showWeeknotes = ref(true)

const filteredFiles = computed(() => {
  let result = props.files

  // Filter
  if (filter.value === 'published') {
    result = result.filter((f) => f.published_url)
  } else if (filter.value === 'drafts') {
    result = result.filter((f) => !f.published_url)
  } else if (filter.value === 'scheduled') {
    result = result.filter((f) => f.publish_at && !f.published_url)
  }

  // Hide weeknotes if toggled off
  if (!showWeeknotes.value) {
    result = result.filter((f) => f.content_type !== 'weeknote')
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
        const aDate = a.date ? new Date(a.date).getTime() / 1000 : a.created
        const bDate = b.date ? new Date(b.date).getTime() / 1000 : b.created
        return bDate - aDate
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
  published: props.files.filter((f) => f.published_url).length,
  drafts: props.files.filter((f) => !f.published_url).length,
  scheduled: props.files.filter((f) => f.publish_at && !f.published_url).length,
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

async function showContextMenu(file: MarkdownFile, e: MouseEvent) {
  e.preventDefault()
  const items = [
    await MenuItem.new({ text: 'Open in Obsidian', action: () => invoke('open_in_obsidian', { path: file.path }) }),
    await MenuItem.new({
      text: 'Open in Editor',
      action: () => invoke('open_in_app', { path: file.path, app: 'iA Writer' }),
    }),
    await PredefinedMenuItem.new({ item: 'Separator' }),
    await MenuItem.new({ text: 'Copy Path', action: () => navigator.clipboard.writeText(file.path) }),
  ]
  if (file.published_url) {
    items.push(
      await PredefinedMenuItem.new({ item: 'Separator' }),
      await MenuItem.new({ text: 'View on Site', action: () => window.open(file.published_url!, '_blank') }),
      await MenuItem.new({ text: 'Copy URL', action: () => navigator.clipboard.writeText(file.published_url!) }),
    )
  }
  const menu = await Menu.new({ items })
  await menu.popup()
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
    <div class="control-bar" data-tauri-drag-region>
      <div class="filters">
        <button :class="{ active: filter === 'all' }" @click="filter = 'all'">
          <PhStack :size="9" weight="bold" />
          {{ counts.all }}
        </button>
        <button :class="{ active: filter === 'published' }" @click="filter = 'published'">
          <PhBroadcast :size="9" weight="bold" />
          {{ counts.published }}
        </button>
        <button :class="{ active: filter === 'drafts' }" @click="filter = 'drafts'">
          <PhFileText :size="9" weight="bold" />
          {{ counts.drafts }}
        </button>
        <button v-if="counts.scheduled > 0" :class="{ active: filter === 'scheduled' }" @click="filter = 'scheduled'">
          <PhClock :size="9" weight="bold" />
          {{ counts.scheduled }}
        </button>
      </div>
      <div class="sort-divider"></div>
      <div class="sort-row">
        <button :class="{ active: sort === 'recent' }" @click="sort = 'recent'" data-tip="Recent">
          <PhClockCounterClockwise :size="9" weight="bold" />
        </button>
        <button :class="{ active: sort === 'created' }" @click="sort = 'created'" data-tip="Created">
          <PhCalendarBlank :size="9" weight="bold" />
        </button>
        <button :class="{ active: sort === 'title' }" @click="sort = 'title'" data-tip="Title">
          <PhTextAlignLeft :size="9" weight="bold" />
        </button>
        <button :class="{ active: sort === 'words' }" @click="sort = 'words'" data-tip="Words">
          <PhListNumbers :size="9" weight="bold" />
        </button>
        <div class="sort-divider"></div>
        <button
          :class="{ active: showWeeknotes }"
          class="weeknote-toggle"
          @click="showWeeknotes = !showWeeknotes"
          data-tip="Week Notes"
        >
          <PhNotebook :size="9" weight="bold" />
        </button>
      </div>
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
            published: !!file.published_url,
            weeknote: file.content_type === 'weeknote',
          }"
          :style="{ '--age': getAgeColor(file.created) }"
          @click="emit('select', file)"
          @contextmenu="showContextMenu(file, $event)"
        >
          <div class="age-bar"></div>
          <div class="content">
            <div class="row">
              <span v-if="file.content_type === 'weeknote'" class="weeknote-badge">WEEK</span>
              <span
                v-if="file.password && !file.published_url"
                class="protected-badge-draft"
                data-tip="Will be password-protected"
              >
                <PhLockSimple :size="10" weight="fill" />
              </span>
              <span
                v-else-if="file.unlisted && !file.published_url"
                class="unlisted-badge-draft"
                data-tip="Will be unlisted"
              >
                <PhEye :size="10" weight="fill" />
              </span>
              <span class="title" :title="formatTitle(file)">{{ formatTitle(file) }}</span>
              <span
                v-if="file.published_url && file.warnings.includes('Modified since publish')"
                class="modified-badge"
              >
                MODIFIED
              </span>
              <span v-else-if="file.published_url && file.password" class="protected-badge">
                <PhLockSimple :size="9" weight="bold" />
                PROTECTED
              </span>
              <span v-else-if="file.published_url && file.unlisted" class="unlisted-badge">
                <PhEye :size="9" weight="bold" />
                UNLISTED
              </span>
              <span v-else-if="file.published_url" class="live-badge">
                <PhCheckCircle :size="9" weight="fill" />
                LIVE
              </span>
              <span v-else-if="file.publish_at" class="scheduled-badge">
                <PhClock :size="9" weight="bold" />
                SCHEDULED
              </span>
            </div>
            <div v-if="file.dek" class="dek">{{ file.dek }}</div>
            <div class="filename">{{ file.source_dir ? file.source_dir + '/' : '' }}{{ file.filename }}</div>
            <div class="meta">
              <span v-if="file.source_dir" class="dir">
                <PhFolderOpen :size="8" weight="duotone" />
                {{ file.source_dir }}/
              </span>
              <template v-if="file.published_date">
                <span class="pub-date">
                  <PhBroadcast :size="8" weight="fill" />
                  {{ formatAge(file.published_date) }}
                </span>
                <span v-if="file.warnings.includes('Modified since publish')" class="edit-date">
                  <PhPencilSimple :size="8" weight="fill" />
                  {{ formatAge(file.modified) }}
                </span>
              </template>
              <template v-else>
                <span class="create-date">
                  <PhCalendarBlank :size="8" weight="duotone" />
                  {{ formatAge(file.created) }}
                </span>
                <span class="edit-date">
                  <PhPencilSimple :size="8" weight="duotone" />
                  {{ formatAge(file.modified) }}
                </span>
              </template>
              <span class="word-count">
                <PhTextAa :size="8" weight="duotone" />
                {{ formatWordCount(file.word_count) }}w
              </span>
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

/* Unified control bar: filters + sort in one row */
.control-bar {
  display: flex;
  align-items: flex-end;
  border-bottom: 1px solid var(--border);
  background: var(--bg-tertiary);
  flex-shrink: 0;
  -webkit-app-region: drag;
  /* Traffic light inset: 12px position + 54px buttons + 12px margin */
  padding-left: 78px;
  height: 44px;
  padding-top: 8px;
  padding-bottom: 4px;
}

.filters {
  display: flex;
  flex: 1;
}

.filters button {
  padding: 5px 8px;
  font-size: 9px;
  font-weight: 500;
  background: transparent;
  border: none;
  color: var(--text-tertiary);
  cursor: pointer;
  transition: all var(--transition-fast, 0.15s cubic-bezier(0.34, 1.56, 0.64, 1));
  display: inline-flex;
  align-items: center;
  gap: 3px;
  font-variant-numeric: tabular-nums;
  -webkit-app-region: no-drag;
}

.filters button:hover {
  color: var(--text-secondary);
}

.filters button.active {
  color: var(--text-primary);
  font-weight: 600;
}

.sort-divider {
  width: 1px;
  height: 14px;
  background: var(--border-light);
  flex-shrink: 0;
}

.sort-row {
  display: flex;
  gap: 0;
}

.sort-row button {
  padding: 5px 6px;
  background: transparent;
  border: none;
  color: var(--text-tertiary);
  cursor: pointer;
  transition: all var(--transition-fast, 0.15s cubic-bezier(0.34, 1.56, 0.64, 1));
  display: inline-flex;
  align-items: center;
  justify-content: center;
  opacity: 0.5;
  -webkit-app-region: no-drag;
}

.sort-row button:hover {
  opacity: 1;
  color: var(--text-secondary);
}

.sort-row button.active {
  opacity: 1;
  color: var(--text-primary);
}

.sort-row .sort-divider {
  width: 1px;
  height: 10px;
  background: var(--border-light);
  flex-shrink: 0;
  align-self: center;
}

.weeknote-toggle.active {
  color: #f59e0b !important;
}

.weeknote-toggle[data-tip]::after {
  left: auto;
  right: 0;
  transform: translateX(0) translateY(-2px);
}

.weeknote-toggle[data-tip]:hover::after {
  transform: translateX(0) translateY(0);
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
  to {
    transform: rotate(360deg);
  }
}

.list {
  flex: 1;
  overflow-y: auto;
  scroll-behavior: smooth;
}

.group-header {
  padding: 5px 12px;
  font-size: 9px;
  font-weight: 600;
  text-transform: uppercase;
  letter-spacing: 0.6px;
  color: var(--text-tertiary);
  background: color-mix(in srgb, var(--bg-tertiary) 80%, var(--bg-secondary));
  backdrop-filter: blur(12px);
  -webkit-backdrop-filter: blur(12px);
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
  opacity: 0.5;
  font-family: 'SF Mono', monospace;
  font-size: 8px;
}

.item {
  width: 100%;
  display: flex;
  gap: 0;
  padding: 0;
  border: none;
  border-bottom: 1px solid color-mix(in srgb, var(--border) 70%, transparent);
  background: transparent;
  cursor: pointer;
  text-align: left;
  transition: background 0.1s ease;
}

.item:hover {
  background: rgba(255, 255, 255, 0.05);
}

.item:active {
  background: rgba(255, 255, 255, 0.05);
}

.item.selected {
  background: rgba(10, 132, 255, 0.15);
  border-left: 2px solid rgba(10, 132, 255, 0.8);
  border-bottom-color: rgba(255, 255, 255, 0.04);
}

.item.selected .content {
  padding-left: 8px;
}

.item.selected .title {
  color: #fff;
}

.item.selected .filename {
  opacity: 0.35;
}

.item.selected .meta,
.item.selected .dir {
  color: var(--text-tertiary);
}

.item.published {
  /* Subtle indicator - no screaming green border */
}

.item.published.selected {
  border-left-color: var(--success);
}

.age-bar {
  width: 3px;
  background: var(--age);
  flex-shrink: 0;
  opacity: 0.8;
}

.item.published .age-bar {
  display: none;
}

.content {
  flex: 1;
  padding: 7px 10px;
  min-width: 0;
}

.row {
  display: flex;
  align-items: center;
  gap: 6px;
}

.live-badge {
  font-size: 7.5px;
  font-weight: 600;
  background: rgba(48, 209, 88, 0.12);
  color: var(--success);
  padding: 1px 5px;
  border-radius: 8px;
  flex-shrink: 0;
  display: inline-flex;
  align-items: center;
  gap: 2px;
  letter-spacing: 0.3px;
}

.modified-badge {
  font-size: 7.5px;
  font-weight: 600;
  background: rgba(255, 159, 10, 0.15);
  color: var(--warning);
  padding: 1px 5px;
  border-radius: 8px;
  flex-shrink: 0;
  letter-spacing: 0.3px;
}

.unlisted-badge {
  font-size: 7.5px;
  font-weight: 600;
  background: rgba(99, 102, 241, 0.15);
  color: #818cf8;
  padding: 1px 5px;
  border-radius: 8px;
  flex-shrink: 0;
  display: inline-flex;
  align-items: center;
  gap: 2px;
  letter-spacing: 0.3px;
}

.protected-badge {
  font-size: 7.5px;
  font-weight: 600;
  background: rgba(139, 92, 246, 0.15);
  color: #a78bfa;
  padding: 1px 5px;
  border-radius: 8px;
  flex-shrink: 0;
  display: inline-flex;
  align-items: center;
  gap: 2px;
  letter-spacing: 0.3px;
}

.weeknote-badge {
  font-size: 7.5px;
  font-weight: 600;
  background: rgba(245, 158, 11, 0.12);
  color: #f59e0b;
  padding: 1px 5px;
  border-radius: 8px;
  flex-shrink: 0;
  letter-spacing: 0.5px;
}

.item.weeknote .age-bar {
  background: #f59e0b !important;
  opacity: 0.6;
  display: block;
}

.item.weeknote.published .age-bar {
  display: block;
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

.scheduled-badge {
  font-size: 8px;
  font-weight: 500;
  padding: 1px 4px;
  border-radius: 2px;
  background: rgba(255, 159, 10, 0.2);
  color: var(--warning);
}

/* Badges keep their own colors when selected — row is subtle now */

.filename {
  font-size: 8.5px;
  font-family: 'SF Mono', monospace;
  color: var(--text-tertiary);
  opacity: 0.45;
  margin-top: 2px;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  letter-spacing: 0.01em;
}

/* .item.selected .filename inherits from the rule above */

.dek {
  font-size: 9px;
  color: var(--text-tertiary);
  margin-top: 2px;
  line-height: 1.35;
  display: -webkit-box;
  -webkit-line-clamp: 2;
  -webkit-box-orient: vertical;
  overflow: hidden;
  font-style: italic;
  opacity: 0.65;
}

.item.selected .dek {
  color: var(--text-tertiary);
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
  letter-spacing: -0.01em;
}

.meta {
  margin-top: 2px;
  font-size: 9px;
  font-family: 'SF Mono', monospace;
  color: var(--text-tertiary);
  display: flex;
  gap: 8px;
  align-items: center;
}

.dir {
  color: var(--text-tertiary);
  max-width: 80px;
  overflow: hidden;
  text-overflow: ellipsis;
  display: inline-flex;
  align-items: center;
  gap: 2px;
}

.pub-date {
  color: var(--success);
  display: inline-flex;
  align-items: center;
  gap: 2px;
}

.create-date {
  color: var(--text-tertiary);
  display: inline-flex;
  align-items: center;
  gap: 2px;
}

.edit-date {
  color: var(--text-tertiary);
  display: inline-flex;
  align-items: center;
  gap: 2px;
}

.word-count {
  font-variant-numeric: tabular-nums;
  font-feature-settings: 'tnum';
  opacity: 0.7;
  display: inline-flex;
  align-items: center;
  gap: 2px;
}

/* Compact Mode */
.sidebar.compact .item .content {
  padding: 4px 10px;
}

.sidebar.compact .filename {
  display: none;
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
