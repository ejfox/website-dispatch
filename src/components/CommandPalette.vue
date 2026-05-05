<script setup lang="ts">
import { ref, computed, watch, nextTick } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import {
  PhFile,
  PhRocket,
  PhLink,
  PhFolder,
  PhArrowsClockwise,
  PhEye,
  PhPencil,
  PhMagnifyingGlass,
  PhListBullets,
  PhSquaresFour,
  PhBookOpen,
  PhWrench,
  PhPlus,
  PhCommand,
} from '@phosphor-icons/vue'
import type { MarkdownFile } from '../types'

interface Action {
  id: string
  label: string
  hint?: string
  /** Group label for sectioning. */
  section: 'Actions' | 'Recent'
  icon: any
  /** Whether this action is currently invokable (e.g. needs a selected file). */
  enabled: boolean
  /** Keyboard shortcut hint shown right-aligned. */
  shortcut?: string
  run: () => unknown
}

const props = defineProps<{
  show: boolean
  files: MarkdownFile[]
  selectedFile: MarkdownFile | null
}>()

const emit = defineEmits<{
  close: []
  selectFile: [file: MarkdownFile]
  newPost: []
  refresh: []
  showJournal: []
  showHelp: []
  togglePanel: [panel: 'preview' | 'media' | 'journal' | 'gear']
  publish: []
}>()

const query = ref('')
const selectedIdx = ref(0)
const inputRef = ref<HTMLInputElement | null>(null)
const listRef = ref<HTMLUListElement | null>(null)

// ---------- Static action catalog ---------------------------------------

const actions = computed<Action[]>(() => {
  const f = props.selectedFile
  const hasFile = !!f
  const isLive = !!f?.published_url
  return [
    {
      id: 'new_post',
      label: 'New Post',
      section: 'Actions',
      icon: PhPlus,
      enabled: true,
      shortcut: '⌘N',
      run: () => emit('newPost'),
    },
    {
      id: 'refresh',
      label: 'Refresh File List',
      section: 'Actions',
      icon: PhArrowsClockwise,
      enabled: true,
      shortcut: '⌘R',
      run: () => emit('refresh'),
    },
    {
      id: 'publish',
      label: isLive ? 'Republish' : 'Publish',
      hint: f ? f.title || f.filename : 'no file selected',
      section: 'Actions',
      icon: PhRocket,
      enabled: hasFile && f.is_safe,
      shortcut: '⌘↩',
      run: () => emit('publish'),
    },
    {
      id: 'open_obsidian',
      label: 'Open in Obsidian',
      hint: f?.filename,
      section: 'Actions',
      icon: PhBookOpen,
      enabled: hasFile,
      shortcut: '⌘⇧O',
      run: () => f && invoke('open_in_obsidian', { path: f.path }),
    },
    {
      id: 'open_editor',
      label: 'Open in Editor',
      hint: f?.filename,
      section: 'Actions',
      icon: PhPencil,
      enabled: hasFile,
      run: () => f && invoke('open_in_app', { path: f.path, app: 'iA Writer' }),
    },
    {
      id: 'reveal_finder',
      label: 'Reveal in Finder',
      hint: f?.filename,
      section: 'Actions',
      icon: PhFolder,
      enabled: hasFile,
      shortcut: '⌘⇧R',
      run: () => f && invoke('open_in_app', { path: f.path, app: 'Finder' }).catch(() => {}),
    },
    {
      id: 'view_site',
      label: 'View on Site',
      hint: f?.published_url || undefined,
      section: 'Actions',
      icon: PhEye,
      enabled: isLive,
      run: () => f?.published_url && window.open(f.published_url, '_blank'),
    },
    {
      id: 'copy_url',
      label: 'Copy Public URL',
      hint: f?.published_url || undefined,
      section: 'Actions',
      icon: PhLink,
      enabled: isLive,
      shortcut: '⌘⇧L',
      run: () => f?.published_url && navigator.clipboard.writeText(f.published_url),
    },
    {
      id: 'copy_path',
      label: 'Copy File Path',
      hint: f?.path,
      section: 'Actions',
      icon: PhLink,
      enabled: hasFile,
      run: () => f && navigator.clipboard.writeText(f.path),
    },
    {
      id: 'show_help',
      label: 'Show Help',
      section: 'Actions',
      icon: PhCommand,
      enabled: true,
      shortcut: '⌘/',
      run: () => emit('showHelp'),
    },
    {
      id: 'panel_preview',
      label: 'Show Preview Panel',
      section: 'Actions',
      icon: PhFile,
      enabled: true,
      shortcut: '⌘1',
      run: () => emit('togglePanel', 'preview'),
    },
    {
      id: 'panel_media',
      label: 'Show Media Panel',
      section: 'Actions',
      icon: PhSquaresFour,
      enabled: true,
      shortcut: '⌘2',
      run: () => emit('togglePanel', 'media'),
    },
    {
      id: 'panel_journal',
      label: 'Show Journal Panel',
      section: 'Actions',
      icon: PhListBullets,
      enabled: true,
      shortcut: '⌘3',
      run: () => emit('togglePanel', 'journal'),
    },
    {
      id: 'panel_gear',
      label: 'Show Gear Panel',
      section: 'Actions',
      icon: PhWrench,
      enabled: true,
      shortcut: '⌘4',
      run: () => emit('togglePanel', 'gear'),
    },
  ]
})

// ---------- Search + ranking --------------------------------------------

function score(haystack: string, needle: string): number {
  if (!needle) return 1
  const h = haystack.toLowerCase()
  const n = needle.toLowerCase()
  if (h === n) return 1000
  if (h.startsWith(n)) return 500
  const i = h.indexOf(n)
  if (i >= 0) return 100 - i
  // Subsequence match (each char of needle in order)
  let hi = 0
  for (const c of n) {
    const found = h.indexOf(c, hi)
    if (found < 0) return 0
    hi = found + 1
  }
  return 10
}

const fileItems = computed<Action[]>(() => {
  const top = [...props.files].sort((a, b) => b.modified - a.modified).slice(0, 50)
  return top.map<Action>((file) => ({
    id: `file:${file.path}`,
    label: file.title || file.filename,
    hint: file.filename,
    section: 'Recent',
    icon: PhFile,
    enabled: true,
    run: () => emit('selectFile', file),
  }))
})

const results = computed(() => {
  const q = query.value.trim()
  const all = [...actions.value.filter((a) => a.enabled), ...fileItems.value]
  if (!q) {
    // No query: show actions first, then most-recent files (capped).
    return all.slice(0, 30)
  }
  const scored = all
    .map((item) => {
      const labelScore = score(item.label, q)
      const hintScore = item.hint ? score(item.hint, q) * 0.4 : 0
      return { item, score: Math.max(labelScore, hintScore) }
    })
    .filter((x) => x.score > 0)
    .sort((a, b) => b.score - a.score)
  return scored.slice(0, 30).map((x) => x.item)
})

// Group results for sectioned display.
const grouped = computed(() => {
  const sections: { name: string; items: Action[] }[] = []
  let current: { name: string; items: Action[] } | null = null
  for (const item of results.value) {
    if (!current || current.name !== item.section) {
      current = { name: item.section, items: [] }
      sections.push(current)
    }
    current.items.push(item)
  }
  return sections
})

// ---------- Keyboard ----------------------------------------------------

watch(
  () => props.show,
  (open) => {
    if (open) {
      query.value = ''
      selectedIdx.value = 0
      nextTick(() => inputRef.value?.focus())
    }
  },
)

watch(query, () => {
  selectedIdx.value = 0
})

function onKey(e: KeyboardEvent) {
  if (e.key === 'Escape') {
    emit('close')
  } else if (e.key === 'ArrowDown') {
    e.preventDefault()
    selectedIdx.value = Math.min(selectedIdx.value + 1, results.value.length - 1)
    scrollSelectedIntoView()
  } else if (e.key === 'ArrowUp') {
    e.preventDefault()
    selectedIdx.value = Math.max(selectedIdx.value - 1, 0)
    scrollSelectedIntoView()
  } else if (e.key === 'Enter') {
    e.preventDefault()
    runSelected()
  }
}

function scrollSelectedIntoView() {
  nextTick(() => {
    const el = listRef.value?.querySelector(`[data-idx="${selectedIdx.value}"]`)
    el?.scrollIntoView({ block: 'nearest' })
  })
}

async function runSelected() {
  const item = results.value[selectedIdx.value]
  if (!item) return
  emit('close')
  await item.run()
}

function runItem(item: Action) {
  emit('close')
  item.run()
}

// Flat index helper for sectioned rendering
function flatIdx(sectionIdx: number, itemIdx: number) {
  let i = 0
  for (let s = 0; s < sectionIdx; s++) i += grouped.value[s].items.length
  return i + itemIdx
}
</script>

<template>
  <div v-if="show" class="palette-overlay" @click.self="emit('close')" @keydown="onKey">
    <div class="palette" role="dialog" aria-label="Command Palette">
      <div class="palette-input-row">
        <PhMagnifyingGlass :size="16" class="palette-icon" />
        <input
          ref="inputRef"
          v-model="query"
          class="palette-input"
          placeholder="Type a command or filename…"
          @keydown="onKey"
        />
        <kbd class="palette-esc">esc</kbd>
      </div>
      <ul ref="listRef" class="palette-list">
        <template v-for="(section, sIdx) in grouped" :key="section.name">
          <li class="palette-section-header">{{ section.name }}</li>
          <li
            v-for="(item, iIdx) in section.items"
            :key="item.id"
            :data-idx="flatIdx(sIdx, iIdx)"
            class="palette-item"
            :class="{ selected: flatIdx(sIdx, iIdx) === selectedIdx }"
            @mouseenter="selectedIdx = flatIdx(sIdx, iIdx)"
            @click="runItem(item)"
          >
            <component :is="item.icon" :size="14" class="palette-item-icon" />
            <div class="palette-item-body">
              <span class="palette-item-label">{{ item.label }}</span>
              <span v-if="item.hint" class="palette-item-hint">{{ item.hint }}</span>
            </div>
            <kbd v-if="item.shortcut" class="palette-shortcut">{{ item.shortcut }}</kbd>
          </li>
        </template>
        <li v-if="!results.length" class="palette-empty">No matches</li>
      </ul>
    </div>
  </div>
</template>

<style scoped>
.palette-overlay {
  position: fixed;
  inset: 0;
  background: rgba(0, 0, 0, 0.4);
  backdrop-filter: blur(4px);
  -webkit-backdrop-filter: blur(4px);
  z-index: 200;
  display: flex;
  align-items: flex-start;
  justify-content: center;
  padding-top: 88px;
  animation: fade-in 150ms cubic-bezier(0.16, 1, 0.3, 1);
}

.palette {
  width: 580px;
  max-width: 90vw;
  max-height: 60vh;
  background: var(--modal-bg);
  backdrop-filter: blur(24px) saturate(180%);
  -webkit-backdrop-filter: blur(24px) saturate(180%);
  border: 1px solid var(--border-light);
  border-radius: 12px;
  overflow: hidden;
  box-shadow: var(--shadow-lg);
  display: flex;
  flex-direction: column;
  animation: scale-in 180ms cubic-bezier(0.16, 1, 0.3, 1);
}

.palette-input-row {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 10px 14px;
  border-bottom: 1px solid var(--border);
}

.palette-icon {
  color: var(--text-tertiary);
  flex: none;
}

.palette-input {
  flex: 1;
  font-size: 15px;
  background: transparent;
  border: none;
  color: var(--text-primary);
  outline: none;
  font-family: inherit;
}

.palette-input::placeholder {
  color: var(--text-tertiary);
}

.palette-esc {
  font-family: 'SF Mono', monospace;
  font-size: 10px;
  padding: 2px 6px;
  background: var(--kbd-bg);
  border: 1px solid var(--kbd-border);
  border-radius: 4px;
  color: var(--text-tertiary);
}

.palette-list {
  list-style: none;
  margin: 0;
  padding: 6px 6px 8px;
  overflow-y: auto;
  flex: 1;
}

.palette-section-header {
  font-size: 10px;
  font-weight: 600;
  text-transform: uppercase;
  letter-spacing: 0.5px;
  color: var(--text-tertiary);
  padding: 8px 10px 4px;
  user-select: none;
}

.palette-item {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 7px 10px;
  border-radius: 6px;
  cursor: default;
  user-select: none;
}

.palette-item.selected {
  background: AccentColor;
  color: AccentColorText;
}

.palette-item.selected .palette-item-icon,
.palette-item.selected .palette-item-hint,
.palette-item.selected .palette-shortcut {
  color: inherit;
  opacity: 0.85;
}

.palette-item-icon {
  color: var(--text-secondary);
  flex: none;
}

.palette-item-body {
  flex: 1;
  min-width: 0;
  display: flex;
  align-items: baseline;
  gap: 8px;
  overflow: hidden;
}

.palette-item-label {
  font-size: 13px;
  font-weight: 500;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.palette-item-hint {
  font-size: 11px;
  color: var(--text-tertiary);
  font-family: 'SF Mono', monospace;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  flex-shrink: 1;
  min-width: 0;
}

.palette-shortcut {
  font-family: 'SF Mono', monospace;
  font-size: 10px;
  padding: 1px 6px;
  background: var(--kbd-bg);
  border: 1px solid var(--kbd-border);
  border-radius: 4px;
  color: var(--text-tertiary);
  flex: none;
}

.palette-empty {
  padding: 20px;
  text-align: center;
  color: var(--text-tertiary);
  font-size: 12px;
}

@keyframes fade-in {
  from {
    opacity: 0;
  }
  to {
    opacity: 1;
  }
}

@keyframes scale-in {
  from {
    opacity: 0;
    transform: translateY(-8px) scale(0.96);
  }
  to {
    opacity: 1;
    transform: translateY(0) scale(1);
  }
}
</style>
