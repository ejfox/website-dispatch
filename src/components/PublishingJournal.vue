<script setup lang="ts">
import { parseDate } from '../utils/formatting'
import { ref, onMounted, computed } from 'vue'
import { invoke } from '@tauri-apps/api/core'

import { Menu, MenuItem, PredefinedMenuItem } from '@tauri-apps/api/menu'

const emit = defineEmits<{ 'jump-to-slug': [slug: string] }>()

async function showHeatmapCellMenu(cell: { date: string; count: number }, e: MouseEvent) {
  e.preventDefault()
  if (cell.count < 0) return // future cell
  // Find entries published on this date.
  const matches = entries.value.filter((entry) => {
    return entry.timestamp.startsWith(cell.date)
  })
  const items_: any[] = [
    await MenuItem.new({
      text: cell.count === 0 ? `No posts on ${cell.date}` : `${cell.count} post${cell.count > 1 ? 's' : ''} on ${cell.date}`,
      enabled: false,
      action: () => {},
    }),
  ]
  if (matches.length) {
    items_.push(await PredefinedMenuItem.new({ item: 'Separator' }))
    for (const entry of matches.slice(0, 8)) {
      items_.push(
        await MenuItem.new({
          text: `Jump to "${entry.title || entry.slug}"`,
          action: () => emit('jump-to-slug', entry.slug),
        }),
      )
    }
  }
  items_.push(
    await PredefinedMenuItem.new({ item: 'Separator' }),
    await MenuItem.new({
      text: 'Copy Date',
      action: () => navigator.clipboard.writeText(cell.date),
    }),
  )
  const menu = await Menu.new({ items: items_ })
  await menu.popup()
}

import { PhBroadcast, PhArrowCounterClockwise, PhTrash } from '@phosphor-icons/vue'
import UiButton from './ui/UiButton.vue'

interface JournalEntry {
  id: number
  timestamp: string
  event: string
  slug: string
  title: string | null
  word_count: number
  tags: string
  content_type: string
  url: string | null
  target_id: string | null
  visibility: string
}

const entries = ref<JournalEntry[]>([])
const loading = ref(true)
const backfilling = ref(false)

function formatWords(n: number): string {
  if (n >= 1000) return `${(n / 1000).toFixed(1)}k`
  return `${n}`
}

function formatAge(ts: string): string {
  const then = parseDate(ts)
  if (!then) return ''
  const seconds = Math.floor((Date.now() - then.getTime()) / 1000)
  const minutes = Math.floor(seconds / 60)
  const hours = Math.floor(minutes / 60)
  const days = Math.floor(hours / 24)
  if (seconds < 60) return 'just now'
  if (minutes < 60) return `${minutes}m ago`
  if (hours < 24) return `${hours}h ago`
  if (days === 1) return 'yesterday'
  if (days < 7) return `${days}d ago`
  return `${Math.floor(days / 7)}w ago`
}

function eventIcon(event: string) {
  switch (event) {
    case 'publish':
      return PhBroadcast
    case 'republish':
      return PhArrowCounterClockwise
    case 'unpublish':
      return PhTrash
    default:
      return PhBroadcast
  }
}

async function doBackfill() {
  backfilling.value = true
  try {
    const count = await invoke<number>('backfill_journal')
    if (count > 0) {
      // Reload everything
      await loadData()
    }
  } catch (e) {
    console.error('Backfill failed:', e)
  }
  backfilling.value = false
}

// Heatmap
const heatmapData = ref<Map<string, number>>(new Map())
const HEATMAP_WEEKS = 13

const heatmapGrid = computed(() => {
  // Build a 7-row x HEATMAP_WEEKS-col grid, Mon=0 at top, Sun=6 at bottom
  // Each cell is { date: "YYYY-MM-DD", count: number, today: boolean }
  const today = new Date()
  const grid: { date: string; count: number; today: boolean }[][] = []

  // Start from the Monday of (HEATMAP_WEEKS-1) weeks ago
  const todayDay = (today.getDay() + 6) % 7 // 0=Mon
  const startDate = new Date(today)
  startDate.setDate(startDate.getDate() - todayDay - (HEATMAP_WEEKS - 1) * 7)

  for (let week = 0; week < HEATMAP_WEEKS; week++) {
    const col: { date: string; count: number; today: boolean }[] = []
    for (let day = 0; day < 7; day++) {
      const d = new Date(startDate)
      d.setDate(d.getDate() + week * 7 + day)
      const key = d.toISOString().slice(0, 10)
      const isToday = key === today.toISOString().slice(0, 10)
      const isFuture = d > today
      col.push({
        date: key,
        count: isFuture ? -1 : heatmapData.value.get(key) || 0,
        today: isToday,
      })
    }
    grid.push(col)
  }
  return grid
})

const heatmapMax = computed(() => Math.max(...Array.from(heatmapData.value.values()), 1))

function heatmapColor(count: number): string {
  if (count < 0) return 'transparent' // future
  if (count === 0) return 'var(--fill)'
  const t = Math.min(count / heatmapMax.value, 1)
  // One-hue ramp from --ramp (dim -> bright); tracks light/dark.
  const pct = t <= 0.25 ? 25 : t <= 0.5 ? 45 : t <= 0.75 ? 70 : 95
  return `color-mix(in srgb, var(--ramp) ${pct}%, transparent)`
}

async function loadData() {
  loading.value = true
  try {
    const [e, h] = await Promise.all([
      invoke<JournalEntry[]>('get_journal_entries', { limit: 30 }),
      invoke<[string, number][]>('get_journal_heatmap', { days: HEATMAP_WEEKS * 7 }),
    ])
    entries.value = e
    heatmapData.value = new Map(h)
  } catch (e) {
    console.error('Journal load error:', e)
  }
  loading.value = false
}

onMounted(async () => {
  await loadData()
  // Backfill is idempotent (skips slugs already recorded) — always run so
  // newly-published posts get picked up from the website2 manifest.
  await doBackfill()
})
</script>

<template>
  <div class="journal" v-if="!loading">
    <!-- Heatmap -->
    <div class="heatmap">
      <div class="heatmap-day-labels">
        <span>M</span>
        <span></span>
        <span>W</span>
        <span></span>
        <span>F</span>
        <span></span>
        <span></span>
      </div>
      <div class="heatmap-grid">
        <div v-for="(week, wi) in heatmapGrid" :key="wi" class="heatmap-col">
          <div
            v-for="cell in week"
            :key="cell.date"
            class="heatmap-cell"
            :class="{ today: cell.today }"
            :style="{ background: heatmapColor(cell.count) }"
            :data-tip="cell.count > 0 ? `${cell.date}: ${cell.count} publish${cell.count > 1 ? 'es' : ''}` : ''"
            @contextmenu="showHeatmapCellMenu(cell, $event)"
          ></div>
        </div>
      </div>
    </div>

    <!-- Activity Log -->
    <div class="activity-section">
      <div class="section-label">Recent activity</div>
      <div v-if="entries.length === 0" class="empty-log">
        <p>No publishing activity yet.</p>
        <UiButton v-if="!backfilling" size="small" class="backfill-btn" label="Import from git history" @click="doBackfill" />
        <span v-else class="backfill-loading">Importing...</span>
      </div>
      <div v-else class="activity-log">
        <button
          v-for="entry in entries"
          :key="entry.id"
          class="log-entry"
          :title="`Jump to ${entry.title || entry.slug}`"
          @click="emit('jump-to-slug', entry.slug)"
        >
          <component :is="eventIcon(entry.event)" :size="10" weight="fill" :class="entry.event" />
          <span class="log-title">{{ entry.title || entry.slug }}</span>
          <span v-if="entry.word_count > 0" class="log-words">{{ formatWords(entry.word_count) }}w</span>
          <span class="log-time">{{ formatAge(entry.timestamp) }}</span>
        </button>
      </div>
    </div>
  </div>

  <!-- Loading -->
  <div v-else-if="loading" class="journal-loading">Loading journal...</div>
</template>

<style scoped>
.journal {
  padding: 12px 16px;
  overflow-y: auto;
  height: 100%;
  display: flex;
  flex-direction: column;
  gap: 14px;
}

/* Heatmap */
.heatmap {
  display: flex;
  gap: 3px;
  padding-bottom: 10px;
  border-bottom: 1px solid var(--border);
}

.heatmap-day-labels {
  display: flex;
  flex-direction: column;
  gap: 2px;
  width: 10px;
  flex-shrink: 0;
}

.heatmap-day-labels span {
  font-size: 7px;
  font-family: 'SF Mono', monospace;
  color: var(--text-tertiary);
  opacity: 0.5;
  line-height: 10px;
  height: 10px;
  text-align: right;
}

.heatmap-grid {
  display: flex;
  gap: 2px;
  flex: 1;
}

.heatmap-col {
  display: flex;
  flex-direction: column;
  gap: 2px;
  flex: 1;
}

.heatmap-cell {
  width: 100%;
  aspect-ratio: 1;
  max-height: 10px;
  border-radius: 2px;
  transition: background 0.2s ease;
}

.heatmap-cell.today {
  box-shadow: inset 0 0 0 1px var(--label-2);
}

/* Section Labels */
.section-label {
  font-size: 9px;
  font-weight: 600;
  text-transform: uppercase;
  letter-spacing: 0.75px;
  color: var(--text-tertiary);
  margin-bottom: 6px;
  display: flex;
  align-items: center;
  gap: 4px;
}

/* Activity Log */
.activity-section {
  flex: 1;
  min-height: 0;
}

.activity-log {
  display: flex;
  flex-direction: column;
  gap: 0;
}

.log-entry {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 5px 0;
  font-size: 10px;
  border-bottom: 1px solid color-mix(in srgb, var(--border) 30%, transparent);
  background: none;
  border-left: none;
  border-right: none;
  border-top: none;
  width: 100%;
  text-align: left;
  color: inherit;
  cursor: pointer;
}
.log-entry:hover {
  background: color-mix(in srgb, var(--text-primary) 6%, transparent);
}

.log-entry:last-child {
  border-bottom: none;
}

.log-entry svg.publish {
  color: var(--success);
}
.log-entry svg.republish {
  color: var(--accent);
}
.log-entry svg.unpublish {
  color: var(--text-tertiary);
}

.log-title {
  flex: 1;
  color: var(--text-secondary);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.log-words {
  font-family: 'SF Mono', monospace;
  font-size: 9px;
  color: var(--text-tertiary);
}

.log-time {
  font-family: 'SF Mono', monospace;
  font-size: 9px;
  color: var(--text-tertiary);
  flex-shrink: 0;
}

.empty-log {
  text-align: center;
  padding: 20px;
  color: var(--text-tertiary);
  font-size: 11px;
}

.backfill-btn {
  margin-top: var(--space-2);
}

.backfill-loading {
  color: var(--text-tertiary);
  font-size: 10px;
}

.journal-loading {
  padding: 40px 20px;
  text-align: center;
  color: var(--text-tertiary);
  font-size: 11px;
}
</style>
