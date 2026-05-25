<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import {
  PhPencilSimple,
  PhRocketLaunch,
  PhArrowsClockwise,
  PhBroadcast,
  PhArrowsCounterClockwise,
} from '@phosphor-icons/vue'
import type { MarkdownFile } from '../types'

/**
 * Unified activity feed across the vault: recent edits, publishes/republishes,
 * and syndications, merged chronologically. Powered entirely by Tauri commands
 * that already exist — no new persistence layer needed for v1.
 *
 * Webmentions aren't logged persistently (yet), so they don't appear here.
 * That's a known gap; add a SQLite table for webmention attempts when needed.
 */

interface JournalEntry {
  id: number
  timestamp: string
  event: string // "publish" | "republish" | "unpublish"
  slug: string
  title: string | null
  word_count: number
  tags: string
  content_type: string
  url: string | null
  target_id: string | null
  visibility: string
}

interface SyndicationQueueItem {
  id: number
  post_slug: string
  post_title: string
  post_url: string
  platform: string
  status: string
  sent_at: string | null
  created_at: string
}

type EventKind = 'edit' | 'publish' | 'republish' | 'unpublish' | 'syndicate'

interface ActivityEvent {
  kind: EventKind
  /** epoch seconds */
  ts: number
  title: string
  slug?: string
  filePath?: string
  /** Subtitle line — varies by kind. */
  detail?: string
  /** Click target, when present. */
  url?: string
}

const props = defineProps<{
  files: MarkdownFile[]
}>()

const emit = defineEmits<{ 'select-file': [file: MarkdownFile]; 'jump-to-slug': [slug: string] }>()

const journal = ref<JournalEntry[]>([])
const syndications = ref<SyndicationQueueItem[]>([])
const loading = ref(false)
const filter = ref<'all' | 'edits' | 'publishes' | 'syndications'>('all')

/** Try-parse — journal/queue timestamps are ISO strings; vault `modified`
 *  is already epoch seconds. Returns epoch seconds (UTC). */
function parseTs(s: string | number): number {
  if (typeof s === 'number') return s
  const d = new Date(s)
  return Math.floor(d.getTime() / 1000)
}

async function loadActivity() {
  loading.value = true
  try {
    const [j, s] = await Promise.all([
      invoke<JournalEntry[]>('get_journal_entries', { limit: 80 }).catch(() => []),
      invoke<SyndicationQueueItem[]>('get_syndication_queue', { status: null, limit: 80 }).catch(() => []),
    ])
    journal.value = j
    syndications.value = s
  } finally {
    loading.value = false
  }
}

onMounted(loadActivity)

// Hot-reload when the vault changes — same signal the file list uses, no
// extra plumbing required. Stops listening on unmount.
let unlistenVault: UnlistenFn | null = null
listen('vault-changed', () => loadActivity()).then((u) => (unlistenVault = u))
onUnmounted(() => unlistenVault?.())

const merged = computed<ActivityEvent[]>(() => {
  const events: ActivityEvent[] = []

  // Edits — derived from vault scan. Only show files modified in the last 30
  // days so the feed doesn't trail off into ancient history.
  const thirtyDaysAgo = Math.floor(Date.now() / 1000) - 60 * 60 * 24 * 30
  for (const f of props.files) {
    if (f.modified < thirtyDaysAgo) continue
    events.push({
      kind: 'edit',
      ts: f.modified,
      title: f.title || f.filename.replace(/\.md$/, ''),
      slug: f.filename.replace(/\.md$/, ''),
      filePath: f.path,
      detail: f.source_dir,
    })
  }

  // Publishes / republishes / unpublishes
  for (const e of journal.value) {
    const kind: EventKind =
      e.event === 'unpublish' ? 'unpublish' : e.event === 'republish' ? 'republish' : 'publish'
    events.push({
      kind,
      ts: parseTs(e.timestamp),
      title: e.title || e.slug,
      slug: e.slug,
      detail:
        e.visibility && e.visibility !== 'public' ? `${e.visibility}${e.url ? ` · ${shortUrl(e.url)}` : ''}` : undefined,
      url: e.url ?? undefined,
    })
  }

  // Syndications — only sent items (others are still in queue / scheduled).
  for (const q of syndications.value) {
    if (q.status !== 'sent' || !q.sent_at) continue
    events.push({
      kind: 'syndicate',
      ts: parseTs(q.sent_at),
      title: q.post_title || q.post_slug,
      slug: q.post_slug,
      detail: `→ ${q.platform}`,
    })
  }

  events.sort((a, b) => b.ts - a.ts)
  return events
})

const filtered = computed(() => {
  switch (filter.value) {
    case 'edits':
      return merged.value.filter((e) => e.kind === 'edit')
    case 'publishes':
      return merged.value.filter((e) => e.kind === 'publish' || e.kind === 'republish' || e.kind === 'unpublish')
    case 'syndications':
      return merged.value.filter((e) => e.kind === 'syndicate')
    default:
      return merged.value
  }
})

const grouped = computed(() => {
  const now = Math.floor(Date.now() / 1000)
  const groups: { label: string; events: ActivityEvent[] }[] = []
  const buckets: Record<string, ActivityEvent[]> = {
    Today: [],
    Yesterday: [],
    'This week': [],
    'Earlier this month': [],
    Older: [],
  }
  for (const e of filtered.value) {
    const age = now - e.ts
    if (age < 60 * 60 * 24) buckets['Today'].push(e)
    else if (age < 60 * 60 * 48) buckets['Yesterday'].push(e)
    else if (age < 60 * 60 * 24 * 7) buckets['This week'].push(e)
    else if (age < 60 * 60 * 24 * 30) buckets['Earlier this month'].push(e)
    else buckets['Older'].push(e)
  }
  for (const label of Object.keys(buckets)) {
    if (buckets[label].length) groups.push({ label, events: buckets[label] })
  }
  return groups
})

function formatRelative(ts: number): string {
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

function shortUrl(u: string): string {
  return u.replace(/^https?:\/\//, '').split('/').slice(0, 2).join('/')
}

function handleClick(e: ActivityEvent) {
  if (e.filePath) {
    const f = props.files.find((x) => x.path === e.filePath)
    if (f) {
      emit('select-file', f)
      return
    }
  }
  if (e.slug) emit('jump-to-slug', e.slug)
}
</script>

<template>
  <div class="activity-feed">
    <div class="activity-header">
      <h2>Activity</h2>
      <button class="refresh-btn" :class="{ spinning: loading }" @click="loadActivity" data-tip="Refresh">
        <PhArrowsCounterClockwise :size="12" weight="bold" />
      </button>
    </div>

    <div class="activity-filters">
      <button :class="{ active: filter === 'all' }" @click="filter = 'all'">All</button>
      <button :class="{ active: filter === 'edits' }" @click="filter = 'edits'">Edits</button>
      <button :class="{ active: filter === 'publishes' }" @click="filter = 'publishes'">Publishes</button>
      <button :class="{ active: filter === 'syndications' }" @click="filter = 'syndications'">Syndications</button>
    </div>

    <div v-if="loading && grouped.length === 0" class="activity-state">Loading…</div>
    <div v-else-if="grouped.length === 0" class="activity-state">
      <span class="muted">Nothing recent. Edit a post or publish something and it'll show up here.</span>
    </div>

    <div v-else class="activity-groups">
      <div v-for="group in grouped" :key="group.label" class="activity-group">
        <div class="group-label">{{ group.label }}</div>
        <div
          v-for="(event, idx) in group.events"
          :key="event.kind + (event.filePath || event.slug || idx)"
          class="activity-row"
          :class="event.kind"
          @click="handleClick(event)"
        >
          <span class="event-icon">
            <PhPencilSimple v-if="event.kind === 'edit'" :size="12" weight="bold" />
            <PhRocketLaunch v-else-if="event.kind === 'publish'" :size="12" weight="bold" />
            <PhArrowsClockwise v-else-if="event.kind === 'republish'" :size="12" weight="bold" />
            <PhBroadcast v-else-if="event.kind === 'syndicate'" :size="12" weight="bold" />
            <PhArrowsCounterClockwise v-else :size="12" weight="bold" />
          </span>
          <span class="event-verb">{{
            event.kind === 'edit'
              ? 'edited'
              : event.kind === 'publish'
                ? 'published'
                : event.kind === 'republish'
                  ? 'republished'
                  : event.kind === 'unpublish'
                    ? 'unpublished'
                    : 'syndicated'
          }}</span>
          <span class="event-title">{{ event.title }}</span>
          <span v-if="event.detail" class="event-detail">{{ event.detail }}</span>
          <span class="event-time">{{ formatRelative(event.ts) }}</span>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.activity-feed {
  display: flex;
  flex-direction: column;
  height: 100%;
  overflow: hidden;
}

.activity-header {
  display: flex;
  align-items: center;
  padding: 14px 16px 10px;
  border-bottom: 1px solid var(--border);
}
.activity-header h2 {
  font-size: 13px;
  font-weight: 600;
  margin: 0;
  flex: 1;
}

.refresh-btn {
  background: none;
  border: none;
  color: var(--text-tertiary);
  cursor: pointer;
  padding: 4px;
  border-radius: 4px;
  display: inline-flex;
}
.refresh-btn:hover {
  background: var(--hover-bg);
  color: var(--text-primary);
}
.refresh-btn.spinning svg {
  animation: spin 0.8s linear infinite;
}
@keyframes spin {
  to {
    transform: rotate(360deg);
  }
}

.activity-filters {
  display: flex;
  gap: 4px;
  padding: 8px 16px;
  border-bottom: 1px solid var(--border);
}
.activity-filters button {
  padding: 3px 10px;
  font-size: 10px;
  background: transparent;
  border: 1px solid var(--border);
  border-radius: 4px;
  color: var(--text-secondary);
  cursor: pointer;
  font-family: inherit;
}
.activity-filters button:hover {
  background: var(--hover-bg);
  color: var(--text-primary);
}
.activity-filters button.active {
  background: var(--accent);
  color: var(--accent-contrast);
  border-color: var(--accent);
}

.activity-state {
  padding: 24px 16px;
  font-size: 12px;
  color: var(--text-tertiary);
  text-align: center;
}
.activity-state .muted {
  color: var(--text-tertiary);
}

.activity-groups {
  flex: 1;
  overflow-y: auto;
  padding: 4px 0 12px;
}

.activity-group + .activity-group {
  margin-top: 4px;
}

.group-label {
  padding: 10px 16px 4px;
  font-size: 9px;
  font-weight: 600;
  letter-spacing: 0.08em;
  text-transform: uppercase;
  color: var(--text-tertiary);
}

.activity-row {
  display: grid;
  grid-template-columns: 18px auto 1fr auto auto;
  align-items: baseline;
  gap: 8px;
  padding: 5px 16px;
  font-size: 11px;
  color: var(--text-secondary);
  cursor: pointer;
  border-left: 2px solid transparent;
  transition: background 0.1s;
}
.activity-row:hover {
  background: var(--hover-bg);
  border-left-color: var(--accent);
  color: var(--text-primary);
}

.event-icon {
  color: var(--text-tertiary);
  display: inline-flex;
  align-items: center;
}
.activity-row.publish .event-icon {
  color: var(--success);
}
.activity-row.republish .event-icon {
  color: var(--accent);
}
.activity-row.unpublish .event-icon {
  color: var(--danger);
}
.activity-row.syndicate .event-icon {
  color: var(--accent);
}

.event-verb {
  font-size: 10px;
  color: var(--text-tertiary);
  font-weight: 500;
  text-transform: lowercase;
}

.event-title {
  font-weight: 500;
  color: var(--text-primary);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.event-detail {
  font-size: 10px;
  color: var(--text-tertiary);
  font-family: 'SF Mono', monospace;
}

.event-time {
  font-size: 10px;
  color: var(--text-tertiary);
  font-variant-numeric: tabular-nums;
  text-align: right;
  white-space: nowrap;
}
</style>
