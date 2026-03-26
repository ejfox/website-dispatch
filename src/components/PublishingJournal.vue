<script setup lang="ts">
import { ref, onMounted, computed } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import {
  PhFlame, PhCalendarBlank, PhTrendUp, PhPencilSimple,
  PhLightbulb, PhClock, PhTextAa, PhCheckCircle,
  PhCircleWavy, PhBroadcast, PhArrowCounterClockwise,
  PhTrash, PhEye
} from '@phosphor-icons/vue'

interface Milestone {
  id: string
  label: string
  description: string
  achieved_at: string | null
}

interface JournalStats {
  current_streak_days: number
  longest_streak_days: number
  current_streak_start: string | null
  current_weekly_streak: number
  longest_weekly_streak: number
  total_publishes: number
  total_republishes: number
  total_unpublishes: number
  total_words_published: number
  unique_posts_published: number
  publishes_this_week: number
  publishes_last_week: number
  words_this_week: number
  words_last_week: number
  publishes_this_month: number
  avg_publishes_per_week: number
  avg_words_per_post: number
  most_active_hour: number | null
  most_active_day_of_week: string | null
  publish_hour_distribution: number[]
  milestones: Milestone[]
  last_publish_at: string | null
  days_since_last_publish: number | null
}

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

interface Nudge {
  message: string
  kind: string
}

interface VaultSignal {
  kind: string
  message: string
  path: string | null
  metadata: Record<string, string>
}

interface VaultPulse {
  total_files: number
  total_words: number
  drafts_count: number
  blog_count: number
  signals: VaultSignal[]
  tag_cloud: [string, number][]
  recent_edits: { path: string; filename: string; title: string | null; word_count: number; modified: number; source_dir: string; is_publishable: boolean }[]
}

const stats = ref<JournalStats | null>(null)
const entries = ref<JournalEntry[]>([])
const nudge = ref<Nudge | null>(null)
const pulse = ref<VaultPulse | null>(null)
const loading = ref(true)
const backfilling = ref(false)

const earnedMilestones = computed(() =>
  stats.value?.milestones.filter(m => m.achieved_at) || []
)

const unearnedMilestones = computed(() =>
  stats.value?.milestones.filter(m => !m.achieved_at) || []
)

const hourMax = computed(() =>
  Math.max(...(stats.value?.publish_hour_distribution || [0]), 1)
)

const monthlyMax = computed(() =>
  Math.max(...(stats.value?.monthly_history || []).map((m: any) => m.words), 1)
)

const currentMonth = computed(() => {
  const d = new Date()
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}`
})

const weekDelta = computed(() => {
  if (!stats.value) return null
  const diff = stats.value.publishes_this_week - stats.value.publishes_last_week
  if (diff > 0) return `+${diff}`
  if (diff < 0) return `${diff}`
  return null
})

const vaultSignals = computed(() =>
  pulse.value?.signals.filter(s => s.kind === 'active_draft') || []
)

const growingDrafts = computed(() =>
  pulse.value?.signals.filter(s => s.kind === 'growing_draft') || []
)

const dormantSignals = computed(() =>
  pulse.value?.signals.filter(s => s.kind === 'dormant_idea') || []
)

function formatWords(n: number): string {
  if (n >= 1000) return `${(n / 1000).toFixed(1)}k`
  return `${n}`
}

function formatHour(h: number): string {
  if (h === 0) return '12a'
  if (h < 12) return `${h}a`
  if (h === 12) return '12p'
  return `${h - 12}p`
}

function formatAge(ts: string): string {
  const seconds = Math.floor((Date.now() - new Date(ts).getTime()) / 1000)
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
    case 'publish': return PhBroadcast
    case 'republish': return PhArrowCounterClockwise
    case 'unpublish': return PhTrash
    default: return PhBroadcast
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
        count: isFuture ? -1 : (heatmapData.value.get(key) || 0),
        today: isToday,
      })
    }
    grid.push(col)
  }
  return grid
})

const heatmapMax = computed(() =>
  Math.max(...Array.from(heatmapData.value.values()), 1)
)

function heatmapColor(count: number): string {
  if (count < 0) return 'transparent' // future
  if (count === 0) return 'rgba(255, 255, 255, 0.04)'
  const t = Math.min(count / heatmapMax.value, 1)
  // Green ramp: dim -> bright
  if (t <= 0.25) return 'rgba(48, 209, 88, 0.2)'
  if (t <= 0.5) return 'rgba(48, 209, 88, 0.4)'
  if (t <= 0.75) return 'rgba(48, 209, 88, 0.65)'
  return 'rgba(48, 209, 88, 0.9)'
}

async function loadData() {
  loading.value = true
  try {
    const [s, e, n, p, h] = await Promise.all([
      invoke<JournalStats>('get_journal_stats'),
      invoke<JournalEntry[]>('get_journal_entries', { limit: 30 }),
      invoke<Nudge | null>('get_journal_nudge'),
      invoke<VaultPulse>('get_vault_pulse'),
      invoke<[string, number][]>('get_journal_heatmap', { days: HEATMAP_WEEKS * 7 }),
    ])
    stats.value = s
    entries.value = e
    nudge.value = n
    pulse.value = p
    heatmapData.value = new Map(h)
  } catch (e) {
    console.error('Journal load error:', e)
  }
  loading.value = false
}

onMounted(async () => {
  await loadData()
  // Auto-backfill on first run
  if (stats.value && stats.value.total_publishes === 0) {
    await doBackfill()
  }
})
</script>

<template>
  <div class="journal" v-if="!loading && stats">
    <!-- Streak Header -->
    <div class="streak-header">
      <div class="streak-number">
        <PhFlame :size="18" weight="fill" :class="{ active: stats.current_streak_days > 0 }" />
        <span class="streak-val">{{ stats.current_streak_days }}</span>
        <span class="streak-label">day streak</span>
      </div>
      <div class="streak-meta">
        <span v-if="stats.longest_streak_days > stats.current_streak_days" class="streak-best">
          best: {{ stats.longest_streak_days }}d
        </span>
        <span v-if="stats.current_weekly_streak > 0" class="streak-weekly">
          {{ stats.current_weekly_streak }}w weekly
        </span>
      </div>
    </div>

    <!-- Heatmap -->
    <div class="heatmap">
      <div class="heatmap-day-labels">
        <span>M</span><span></span><span>W</span><span></span><span>F</span><span></span><span></span>
      </div>
      <div class="heatmap-grid">
        <div v-for="(week, wi) in heatmapGrid" :key="wi" class="heatmap-col">
          <div
            v-for="(cell, di) in week"
            :key="cell.date"
            class="heatmap-cell"
            :class="{ today: cell.today }"
            :style="{ background: heatmapColor(cell.count) }"
            :data-tip="cell.count > 0 ? `${cell.date}: ${cell.count} publish${cell.count > 1 ? 'es' : ''}` : ''"
          ></div>
        </div>
      </div>
    </div>

    <!-- Nudge -->
    <div v-if="nudge" class="nudge" :class="nudge.kind">
      <PhLightbulb v-if="nudge.kind === 'encouragement' || nudge.kind === 'celebration'" :size="11" weight="fill" />
      <PhFlame v-else-if="nudge.kind === 'streak'" :size="11" weight="fill" />
      <PhClock v-else :size="11" weight="fill" />
      <span>{{ nudge.message }}</span>
    </div>

    <!-- This Week -->
    <div class="week-stats">
      <div class="stat-card">
        <span class="stat-val">{{ stats.publishes_this_week }}</span>
        <span class="stat-label">posts this week</span>
        <span v-if="weekDelta" class="stat-delta" :class="{ up: weekDelta.startsWith('+') }">{{ weekDelta }}</span>
      </div>
      <div class="stat-card">
        <span class="stat-val">{{ formatWords(stats.words_this_week) }}</span>
        <span class="stat-label">words this week</span>
      </div>
      <div class="stat-card">
        <span class="stat-val">{{ stats.total_publishes }}</span>
        <span class="stat-label">total publishes</span>
      </div>
      <div class="stat-card">
        <span class="stat-val">{{ formatWords(stats.total_words_published) }}</span>
        <span class="stat-label">total words</span>
      </div>
    </div>

    <!-- Monthly Word Count -->
    <div v-if="stats.monthly_history && stats.monthly_history.length > 1" class="monthly-section">
      <div class="section-label">Words per month</div>
      <div class="monthly-chart">
        <div
          v-for="m in [...stats.monthly_history].reverse()"
          :key="m.month"
          class="month-col"
        >
          <div class="month-bar-wrap">
            <div
              class="month-bar"
              :style="{ height: `${(m.words / monthlyMax) * 100}%` }"
              :class="{ current: m.month === currentMonth }"
            ></div>
          </div>
          <span class="month-label">{{ m.month.split('-')[1] }}</span>
          <span class="month-words">{{ formatWords(m.words) }}</span>
        </div>
      </div>
    </div>

    <!-- Hour Distribution Sparkline -->
    <div v-if="stats.total_publishes > 5" class="rhythm-section">
      <div class="section-label">Publishing rhythm</div>
      <div class="hour-chart">
        <div
          v-for="(count, hour) in stats.publish_hour_distribution"
          :key="hour"
          class="hour-bar-wrap"
          :data-tip="count > 0 ? `${formatHour(hour)}: ${count}` : ''"
        >
          <div
            class="hour-bar"
            :style="{ height: `${(count / hourMax) * 100}%` }"
            :class="{ peak: hour === stats.most_active_hour }"
          ></div>
        </div>
      </div>
      <div class="hour-labels">
        <span>12a</span><span>6a</span><span>12p</span><span>6p</span>
      </div>
      <div class="rhythm-insights">
        <span v-if="stats.most_active_hour !== null">
          Peak: {{ formatHour(stats.most_active_hour) }}
        </span>
        <span v-if="stats.most_active_day_of_week">
          Most active: {{ stats.most_active_day_of_week }}s
        </span>
        <span>
          Avg: {{ stats.avg_publishes_per_week.toFixed(1) }}/week
        </span>
      </div>
    </div>

    <!-- Growing Drafts — the big ones brewing -->
    <div v-if="growingDrafts.length > 0" class="vault-section growing">
      <div class="section-label"><PhTrendUp :size="10" weight="bold" /> Growing</div>
      <div class="signal-list">
        <div v-for="signal in growingDrafts" :key="signal.path" class="signal-item growing-item">
          <PhPencilSimple :size="10" weight="fill" />
          <span class="signal-text">{{ signal.message }}</span>
        </div>
      </div>
    </div>

    <!-- Vault Pulse: Active Drafts -->
    <div v-if="vaultSignals.length > 0" class="vault-section">
      <div class="section-label"><PhEye :size="10" weight="bold" /> Vault pulse</div>
      <div class="signal-list">
        <div v-for="signal in vaultSignals" :key="signal.path" class="signal-item">
          <PhPencilSimple :size="10" weight="duotone" />
          <span class="signal-text">{{ signal.message }}</span>
        </div>
      </div>
    </div>

    <!-- Vault Pulse: Dormant Ideas -->
    <div v-if="dormantSignals.length > 0" class="vault-section dormant">
      <div class="section-label"><PhLightbulb :size="10" weight="bold" /> Dormant ideas</div>
      <div class="signal-list">
        <div v-for="signal in dormantSignals" :key="signal.path" class="signal-item">
          <PhClock :size="10" weight="duotone" />
          <span class="signal-text">{{ signal.message }}</span>
        </div>
      </div>
    </div>

    <!-- Milestones -->
    <div class="milestones-section" v-if="earnedMilestones.length > 0">
      <div class="section-label">Milestones</div>
      <div class="milestone-grid">
        <div
          v-for="m in earnedMilestones"
          :key="m.id"
          class="milestone earned"
          :data-tip="m.description"
        >
          <PhCheckCircle :size="14" weight="fill" />
          <span>{{ m.label }}</span>
        </div>
        <div
          v-for="m in unearnedMilestones.slice(0, 3)"
          :key="m.id"
          class="milestone locked"
          :data-tip="m.description"
        >
          <PhCircleWavy :size="14" weight="duotone" />
          <span>{{ m.label }}</span>
        </div>
      </div>
    </div>

    <!-- Activity Log -->
    <div class="activity-section">
      <div class="section-label">Recent activity</div>
      <div v-if="entries.length === 0" class="empty-log">
        <p>No publishing activity yet.</p>
        <button v-if="!backfilling" @click="doBackfill" class="backfill-btn">
          Import from git history
        </button>
        <span v-else class="backfill-loading">Importing...</span>
      </div>
      <div v-else class="activity-log">
        <div v-for="entry in entries" :key="entry.id" class="log-entry">
          <component :is="eventIcon(entry.event)" :size="10" weight="fill" :class="entry.event" />
          <span class="log-title">{{ entry.title || entry.slug }}</span>
          <span v-if="entry.word_count > 0" class="log-words">{{ formatWords(entry.word_count) }}w</span>
          <span class="log-time">{{ formatAge(entry.timestamp) }}</span>
        </div>
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

/* Streak Header */
.streak-header {
  display: flex;
  align-items: baseline;
  justify-content: space-between;
  padding-bottom: 10px;
  border-bottom: 1px solid var(--border);
}

.streak-number {
  display: flex;
  align-items: center;
  gap: 6px;
}

.streak-number svg {
  color: var(--text-tertiary);
}

.streak-number svg.active {
  color: #f59e0b;
}

.streak-val {
  font-size: 32px;
  font-weight: 700;
  font-variant-numeric: tabular-nums;
  color: var(--text-primary);
  line-height: 1;
  letter-spacing: -0.02em;
}

.streak-label {
  font-size: 11px;
  color: var(--text-tertiary);
}

.streak-meta {
  display: flex;
  gap: 10px;
  font-size: 9px;
  font-family: 'SF Mono', monospace;
  color: var(--text-tertiary);
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
  box-shadow: inset 0 0 0 1px rgba(255, 255, 255, 0.3);
}

/* Nudge */
.nudge {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 7px 10px;
  border-radius: 6px;
  font-size: 10px;
  line-height: 1.3;
  color: var(--text-secondary);
  background: rgba(255, 255, 255, 0.03);
  border: 1px solid color-mix(in srgb, var(--border) 60%, transparent);
}

.nudge.celebration {
  color: var(--success);
  background: rgba(48, 209, 88, 0.06);
  border-color: rgba(48, 209, 88, 0.15);
}
.nudge.streak {
  color: #f59e0b;
  background: rgba(245, 158, 11, 0.06);
  border-color: rgba(245, 158, 11, 0.15);
}
.nudge.encouragement {
  color: #818cf8;
  background: rgba(129, 140, 248, 0.06);
  border-color: rgba(129, 140, 248, 0.15);
}

/* Week Stats */
.week-stats {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 6px;
}

.stat-card {
  background: rgba(255, 255, 255, 0.03);
  border: 1px solid color-mix(in srgb, var(--border) 60%, transparent);
  border-radius: 6px;
  padding: 10px 10px 8px;
  display: flex;
  flex-direction: column;
  gap: 1px;
}

.stat-val {
  font-size: 20px;
  font-weight: 600;
  font-variant-numeric: tabular-nums;
  color: var(--text-primary);
  line-height: 1.1;
  letter-spacing: -0.02em;
}

.stat-label {
  font-size: 9px;
  color: var(--text-tertiary);
}

.stat-delta {
  font-size: 9px;
  font-family: 'SF Mono', monospace;
  color: var(--text-tertiary);
}

.stat-delta.up {
  color: var(--success);
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

/* Hour Distribution */
.rhythm-section {
  padding-bottom: 10px;
  border-bottom: 1px solid var(--border);
}

.hour-chart {
  display: flex;
  align-items: flex-end;
  height: 40px;
  gap: 1px;
  padding: 0 1px;
}

.hour-bar-wrap {
  flex: 1;
  height: 100%;
  display: flex;
  align-items: flex-end;
}

.hour-bar {
  width: 100%;
  background: rgba(255, 255, 255, 0.08);
  border-radius: 2px 2px 0 0;
  min-height: 0;
  transition: height 0.4s cubic-bezier(0.16, 1, 0.3, 1);
}

.hour-bar.peak {
  background: rgba(129, 140, 248, 0.7);
}

.hour-labels {
  display: flex;
  justify-content: space-between;
  font-size: 8px;
  color: var(--text-tertiary);
  margin-top: 2px;
  font-family: 'SF Mono', monospace;
}

.rhythm-insights {
  display: flex;
  gap: 12px;
  margin-top: 6px;
  font-size: 9px;
  font-family: 'SF Mono', monospace;
  color: var(--text-tertiary);
}

/* Monthly Words */
.monthly-section {
  padding-bottom: 10px;
  border-bottom: 1px solid var(--border);
}

.monthly-chart {
  display: flex;
  gap: 4px;
  align-items: flex-end;
}

.month-col {
  flex: 1;
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 2px;
}

.month-bar-wrap {
  width: 100%;
  height: 48px;
  display: flex;
  align-items: flex-end;
}

.month-bar {
  width: 100%;
  background: rgba(255, 255, 255, 0.08);
  border-radius: 3px 3px 0 0;
  min-height: 2px;
  transition: height 0.4s cubic-bezier(0.16, 1, 0.3, 1);
}

.month-bar.current {
  background: rgba(48, 209, 88, 0.5);
}

.month-label {
  font-size: 8px;
  font-family: 'SF Mono', monospace;
  color: var(--text-tertiary);
}

.month-words {
  font-size: 8px;
  font-family: 'SF Mono', monospace;
  color: var(--text-tertiary);
  opacity: 0.7;
}

/* Vault Pulse */
.vault-section {
  padding-bottom: 10px;
  border-bottom: 1px solid var(--border);
}

.signal-list {
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.signal-item {
  display: flex;
  align-items: flex-start;
  gap: 6px;
  font-size: 10px;
  color: var(--text-secondary);
  line-height: 1.4;
  padding: 4px 8px;
  border-radius: 4px;
  background: rgba(255, 255, 255, 0.02);
}

.signal-item svg {
  flex-shrink: 0;
  margin-top: 2px;
  color: rgba(129, 140, 248, 0.5);
}

.vault-section.dormant .signal-item svg {
  color: var(--text-tertiary);
  opacity: 0.6;
}

.vault-section .section-label { color: rgba(129, 140, 248, 0.7); }
.vault-section.growing .section-label { color: var(--success); }
.vault-section.dormant .section-label { color: var(--text-tertiary); opacity: 0.7; }

.growing-item {
  background: rgba(48, 209, 88, 0.04) !important;
  border: 1px solid rgba(48, 209, 88, 0.08);
}

.growing-item svg {
  color: rgba(48, 209, 88, 0.6) !important;
}

.signal-text {
  flex: 1;
}

/* Milestones */
.milestones-section {
  padding-bottom: 10px;
  border-bottom: 1px solid var(--border);
}

.milestone-grid {
  display: flex;
  flex-wrap: wrap;
  gap: 4px;
}

.milestone {
  display: flex;
  align-items: center;
  gap: 3px;
  padding: 3px 8px;
  border-radius: 10px;
  font-size: 8.5px;
  font-weight: 500;
  letter-spacing: 0.01em;
}

.milestone.earned {
  background: rgba(48, 209, 88, 0.1);
  color: var(--success);
  border: 1px solid rgba(48, 209, 88, 0.15);
}

.milestone.locked {
  background: transparent;
  color: var(--text-tertiary);
  opacity: 0.35;
  border: 1px solid var(--border);
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
}

.log-entry:last-child {
  border-bottom: none;
}

.log-entry svg.publish { color: var(--success); }
.log-entry svg.republish { color: #818cf8; }
.log-entry svg.unpublish { color: var(--text-tertiary); }

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
  margin-top: 8px;
  padding: 5px 12px;
  font-size: 10px;
  background: var(--bg-tertiary);
  border: 1px solid var(--border);
  border-radius: 4px;
  color: var(--text-secondary);
  cursor: pointer;
}

.backfill-btn:hover {
  background: var(--accent);
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
