<script setup lang="ts">
import { CHANGED_LABEL, CHANGED_HINT } from '../utils/vocab'
import { ref, computed } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import {
  PhArrowSquareUpRight,
  PhArrowsClockwise,
  PhCaretDown,
  PhArrowFatLineUp,
  PhArrowFatLineDown,
  PhPencilSimple,
} from '@phosphor-icons/vue'
import PublishDiffPanel from './PublishDiffPanel.vue'
import BulkRepublishSheet from './BulkRepublishSheet.vue'
import type { MarkdownFile } from '../types'
import { useToasts } from '../composables/useUiState'

/**
 * Triage view for posts where the vault source has diverged from the live
 * copy. Shows every MODIFIED post in one place with inline diff + per-row
 * Republish, plus "Republish all". A row's Republish opens that post's
 * review sheet in the Preview tab (same path as the toolbar, with deploy
 * tracking); "Republish all" gets one sheet listing every post.
 */

const props = defineProps<{ files: MarkdownFile[] }>()
const emit = defineEmits<{
  'select-file': [file: MarkdownFile]
  'review-file': [file: MarkdownFile]
  published: []
}>()

const toasts = useToasts()

const sort = ref<'most-recent-edit' | 'biggest-delta' | 'oldest-publish'>('most-recent-edit')
const expandedDiffs = ref<Set<string>>(new Set())
const bulkPublishing = ref(false)
const showBulkConfirm = ref(false)

const modifiedFiles = computed(() => {
  const list = props.files.filter((f) =>
    f.warnings.includes('Modified since publish'),
  )
  const sorted = [...list]
  switch (sort.value) {
    case 'biggest-delta':
      sorted.sort((a, b) => Math.abs(wordDelta(b)) - Math.abs(wordDelta(a)))
      break
    case 'oldest-publish':
      sorted.sort((a, b) => (a.published_date || 0) - (b.published_date || 0))
      break
    case 'most-recent-edit':
    default:
      sorted.sort((a, b) => b.modified - a.modified)
  }
  return sorted
})

function wordDelta(file: MarkdownFile): number {
  if (file.published_word_count == null) return 0
  return file.word_count - file.published_word_count
}

// Display only: the backend derives the real destination from file.path.
function slugFor(file: MarkdownFile): string {
  const baseName = file.filename.replace('.md', '')
  const yearMatch = file.path.match(/\/blog\/(\d{4})\//)
  if (yearMatch) return `${yearMatch[1]}/${baseName}`
  return file.path.includes('/blog/') ? baseName : baseName
}

function toggleDiff(file: MarkdownFile) {
  const k = file.path
  if (expandedDiffs.value.has(k)) {
    expandedDiffs.value.delete(k)
  } else {
    expandedDiffs.value.add(k)
  }
  // Force reactivity
  expandedDiffs.value = new Set(expandedDiffs.value)
}

async function republishAll(paths: string[]) {
  if (bulkPublishing.value) return
  bulkPublishing.value = true
  let ok = 0
  let fail = 0
  for (const path of paths) {
    try {
      await invoke('publish_file', {
        sourcePath: path,
        targetId: null,
      })
      ok++
    } catch (e) {
      console.warn('bulk republish failed for', path, e)
      fail++
    }
  }
  bulkPublishing.value = false
  if (fail === 0) {
    toasts.success(`Republished ${ok} post${ok === 1 ? '' : 's'}`)
  } else {
    toasts.warn(`Republished ${ok} · ${fail} failed`)
  }
  emit('published')
}

function formatAge(ts: number): string {
  const seconds = Math.floor(Date.now() / 1000 - ts)
  if (seconds < 60) return 'just now'
  const minutes = Math.floor(seconds / 60)
  if (minutes < 60) return `${minutes}m ago`
  const hours = Math.floor(minutes / 60)
  if (hours < 24) return `${hours}h ago`
  const days = Math.floor(hours / 24)
  if (days === 1) return 'yesterday'
  if (days < 7) return `${days}d ago`
  if (days < 30) return `${Math.floor(days / 7)}w ago`
  if (days < 365) return `${Math.floor(days / 30)}mo ago`
  return `${Math.floor(days / 365)}y ago`
}
</script>

<template>
  <div class="modified-view">
    <div class="modified-header">
      <h2 :data-tip="CHANGED_HINT">{{ CHANGED_LABEL }} <span class="count">{{ modifiedFiles.length }}</span></h2>
      <div class="header-actions">
        <select v-model="sort" class="sort-select" data-no-drag>
          <option value="most-recent-edit">Recently edited</option>
          <option value="biggest-delta">Biggest delta</option>
          <option value="oldest-publish">Oldest publish</option>
        </select>
        <button
          v-if="modifiedFiles.length > 0"
          class="bulk-btn"
          :disabled="bulkPublishing"
          @click="showBulkConfirm = true"
        >
          {{ bulkPublishing ? 'Republishing…' : `Republish all (${modifiedFiles.length})` }}
        </button>
      </div>
    </div>

    <div v-if="modifiedFiles.length === 0" class="empty-state">
      <PhArrowsClockwise :size="32" weight="duotone" class="empty-icon" />
      <div class="empty-title">Everything matches</div>
      <div class="empty-sub">Every live post is the same in your vault and on ejfox.com. Edit one in Obsidian and it shows up here.</div>
    </div>

    <div v-else class="modified-list">
      <div v-for="file in modifiedFiles" :key="file.path" class="modified-row">
        <div class="row-main">
          <div class="row-info" @click="emit('select-file', file)">
            <div class="row-title-line">
              <span class="row-title">{{ file.title || file.filename.replace(/\.md$/, '') }}</span>
              <span v-if="file.published_word_count != null" class="row-delta">
                <PhArrowFatLineUp
                  v-if="wordDelta(file) > 0"
                  :size="9"
                  weight="bold"
                  class="delta-up"
                />
                <PhArrowFatLineDown
                  v-else-if="wordDelta(file) < 0"
                  :size="9"
                  weight="bold"
                  class="delta-down"
                />
                <span :class="{ pos: wordDelta(file) > 0, neg: wordDelta(file) < 0 }">
                  {{ wordDelta(file) > 0 ? '+' : '' }}{{ wordDelta(file).toLocaleString() }}w
                </span>
              </span>
            </div>
            <div class="row-meta">
              <span class="row-slug">{{ slugFor(file) }}</span>
              <span class="row-time">
                <PhPencilSimple :size="9" weight="duotone" />
                edited {{ formatAge(file.modified) }}
              </span>
              <span v-if="file.published_date" class="row-time">
                published {{ formatAge(file.published_date) }}
              </span>
            </div>
          </div>
          <div class="row-actions">
            <button class="row-btn ghost" @click="toggleDiff(file)" data-no-drag>
              <PhCaretDown
                :size="10"
                weight="bold"
                class="caret"
                :class="{ open: expandedDiffs.has(file.path) }"
              />
              {{ expandedDiffs.has(file.path) ? 'Hide diff' : 'Diff' }}
            </button>
            <a
              v-if="file.published_url"
              :href="file.published_url"
              target="_blank"
              class="row-btn ghost icon-only"
              data-tip="View live"
              data-no-drag
            >
              <PhArrowSquareUpRight :size="11" weight="bold" />
            </a>
            <button class="row-btn primary" @click="emit('review-file', file)" data-no-drag>
              <PhArrowsClockwise :size="11" weight="bold" />
              Republish
            </button>
          </div>
        </div>
        <PublishDiffPanel
          v-if="expandedDiffs.has(file.path)"
          :file-path="file.path"
          :open="true"
          @close="toggleDiff(file)"
        />
      </div>
    </div>

    <BulkRepublishSheet
      :show="showBulkConfirm"
      :files="modifiedFiles"
      @close="showBulkConfirm = false"
      @confirm="republishAll"
    />
  </div>
</template>

<style scoped>
.modified-view {
  display: flex;
  flex-direction: column;
  height: 100%;
  overflow: hidden;
}

.modified-header {
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 14px 16px 10px;
  border-bottom: 1px solid var(--border);
}
.modified-header h2 {
  font-size: 13px;
  font-weight: 600;
  margin: 0;
  flex: 1;
}
.modified-header .count {
  color: var(--warning);
  background: color-mix(in srgb, var(--warning) 15%, transparent);
  padding: 2px 7px;
  border-radius: 999px;
  font-size: 11px;
  font-weight: 600;
  margin-left: 4px;
}

.header-actions {
  display: flex;
  gap: 6px;
  align-items: center;
}

.sort-select {
  font-size: 10px;
  padding: 3px 6px;
  background: var(--bg-tertiary);
  color: var(--text-secondary);
  border: 1px solid var(--border);
  border-radius: 4px;
  font-family: inherit;
  cursor: pointer;
}

.bulk-btn {
  padding: 4px 10px;
  font-size: 10px;
  font-weight: 500;
  background: var(--warning);
  color: #000;
  border: none;
  border-radius: 4px;
  cursor: pointer;
  font-family: inherit;
}
.bulk-btn:hover:not(:disabled) {
  background: color-mix(in srgb, var(--warning) 88%, white);
}
.bulk-btn:disabled {
  opacity: 0.6;
  cursor: not-allowed;
}

.empty-state {
  flex: 1;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 8px;
  color: var(--text-tertiary);
  padding: 32px;
}
.empty-icon {
  color: var(--success);
}
.empty-title {
  font-size: 14px;
  font-weight: 600;
  color: var(--text-primary);
}
.empty-sub {
  font-size: 11px;
  color: var(--text-tertiary);
}

.modified-list {
  flex: 1;
  overflow-y: auto;
}

.modified-row {
  border-bottom: 1px solid var(--border);
}

.row-main {
  display: flex;
  align-items: stretch;
  gap: 8px;
  padding: 10px 16px;
}

.row-info {
  flex: 1;
  display: flex;
  flex-direction: column;
  gap: 3px;
  cursor: pointer;
  min-width: 0;
}
.row-info:hover .row-title {
  color: var(--accent);
}

.row-title-line {
  display: flex;
  align-items: center;
  gap: 6px;
}
.row-title {
  font-size: 12px;
  font-weight: 500;
  color: var(--text-primary);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.row-delta {
  display: inline-flex;
  align-items: center;
  gap: 2px;
  font-family: 'SF Mono', monospace;
  font-size: 10px;
  font-variant-numeric: tabular-nums;
  font-weight: 600;
  padding: 1px 5px;
  border-radius: 3px;
  background: var(--bg-tertiary);
  flex-shrink: 0;
}
.row-delta .pos {
  color: var(--success);
}
.row-delta .neg {
  color: var(--danger);
}
.row-delta .delta-up {
  color: var(--success);
}
.row-delta .delta-down {
  color: var(--danger);
}

.row-meta {
  display: flex;
  flex-wrap: wrap;
  gap: 10px;
  font-size: 10px;
  color: var(--text-tertiary);
  font-family: 'SF Mono', monospace;
}
.row-meta .row-slug {
  color: var(--text-secondary);
}
.row-meta .row-time {
  display: inline-flex;
  align-items: center;
  gap: 3px;
}

.row-actions {
  display: flex;
  align-items: center;
  gap: 4px;
  flex-shrink: 0;
}

.row-btn {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  padding: 4px 9px;
  font-size: 10px;
  font-weight: 500;
  border-radius: 4px;
  cursor: pointer;
  border: none;
  font-family: inherit;
  text-decoration: none;
  transition: background 0.12s;
}
.row-btn.icon-only {
  padding: 4px 6px;
}
.row-btn.ghost {
  background: var(--bg-tertiary);
  color: var(--text-secondary);
  border: 1px solid var(--border);
}
.row-btn.ghost:hover {
  background: var(--hover-bg);
  color: var(--text-primary);
}
.row-btn.primary {
  background: var(--accent);
  color: var(--accent-contrast);
}
.row-btn.primary:hover:not(:disabled) {
  background: var(--accent-strong);
}
.row-btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}
.row-btn .caret {
  transition: transform 0.15s;
}
.row-btn .caret.open {
  transform: rotate(180deg);
}
</style>
