<script setup lang="ts">
// The Desk — today's chart-desk bench, the streak, and the on-deck stories.
// Data comes from the `desk` CLI (Rust runs it off the UI thread). The last
// good snapshot is cached so the view paints instantly on launch, then
// refreshes in the background.
import { ref, computed, onMounted, onUnmounted } from 'vue'
import { invoke, convertFileSrc } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { useLocalStorage } from '@vueuse/core'
import type { DeskToday, DeskStory, MarkdownFile } from '../types'

const props = defineProps<{ files: MarkdownFile[] }>()
const emit = defineEmits<{ started: [notePath: string] }>()

// Today's piece, once it's live: share it with UTM-tagged links.
const livePiece = computed(() => {
  const id = desk.value?.bench?.id
  return id ? props.files.find((f) => f.bench === id && f.published_url) ?? null : null
})
const copied = ref<string | null>(null)
async function copyShare(source: string, medium: string) {
  const url = livePiece.value?.published_url
  if (!url) return
  try {
    const tagged = await invoke<string>('share_link', { url, source, medium })
    await navigator.clipboard.writeText(tagged)
    copied.value = source
    setTimeout(() => (copied.value = null), 1800)
  } catch (e) {
    actionError.value = { message: `Couldn't copy the link: ${e}`, retry: () => copyShare(source, medium) }
  }
}

const cached = useLocalStorage<DeskToday | null>('dispatch-desk-today', null, {
  serializer: { read: (v) => (v ? JSON.parse(v) : null), write: (v) => JSON.stringify(v) },
})
const desk = ref<DeskToday | null>(cached.value)
const loading = ref(false)
const loadError = ref<string | null>(null)
const loadedAt = ref(0)

async function load() {
  loading.value = true
  try {
    const d = await invoke<DeskToday>('desk_today')
    desk.value = d
    cached.value = d
    loadError.value = null
    loadedAt.value = Date.now()
  } catch (e) {
    loadError.value = String(e)
  }
  loading.value = false
}

const bench = computed<DeskStory | null>(() => desk.value?.bench ?? null)
const onDeck = computed(() =>
  (desk.value?.stories ?? []).filter((s) => s.id !== bench.value?.id && s.status !== 'ABANDONED'),
)

// ── Header ────────────────────────────────────────────────────────────────
const todayIso = computed(() => desk.value?.date ?? '')
const slotLabel = computed(() => {
  const d = desk.value
  if (!d) return ''
  if (d.shipped_today) return 'shipped today'
  if (d.slot.open) return 'slot open'
  return `making slot opens ${d.slot.start}`
})
function dayLetter(iso: string) {
  return new Date(`${iso}T12:00:00`).toLocaleDateString(undefined, { weekday: 'narrow' })
}

// Before 6:45 the desk hasn't been stocked yet.
const beforeStock = computed(() => {
  const n = new Date()
  return n.getHours() * 60 + n.getMinutes() < 6 * 60 + 45
})

// ── Bench card ────────────────────────────────────────────────────────────
const chartSrc = computed(() =>
  bench.value?.chart_png ? `${convertFileSrc(bench.value.chart_png)}?v=${loadedAt.value}` : null,
)
const chartBroken = ref(false)

const findingLead = computed(() => {
  const f = bench.value?.finding?.trim() ?? ''
  const nl = f.indexOf('\n')
  if (nl > 0) return f.slice(0, nl).trim()
  const m = f.match(/^(.+?[.!?])\s/)
  return m ? m[1] : f
})
const findingRest = computed(() => {
  const f = bench.value?.finding?.trim() ?? ''
  return f.slice(findingLead.value.length).trim()
})

const pickedAngle = ref<number | null>(null)
function pickAngle(i: number) {
  pickedAngle.value = pickedAngle.value === i ? null : i
  const a = bench.value?.angles[i]
  if (a && pickedAngle.value === i) navigator.clipboard.writeText(a).catch(() => {})
}

const checkPill = computed(() => {
  const c = bench.value?.check
  if (jobRunning('check', bench.value?.id)) return { label: 'checking…', tone: 'busy' }
  if (!c) return { label: 'not fact-checked', tone: 'muted' }
  const s = (c.status || '').toUpperCase()
  const tone = s.includes('OK') || s.includes('VERIFIED') ? 'ok' : s.includes('NEEDS') ? 'warn' : 'bad'
  const counts = c.claims ? ` · ${c.confirmed ?? 0}/${c.claims} claims` : ''
  return { label: `${c.status}${counts}`, tone }
})

// ── Actions ───────────────────────────────────────────────────────────────
const starting = ref(false)
const actionError = ref<{ message: string; retry: () => void } | null>(null)

async function startPiece() {
  const b = bench.value
  if (!b || starting.value) return
  starting.value = true
  actionError.value = null
  try {
    const title = pickedAngle.value != null ? b.angles[pickedAngle.value] : null
    const res = await invoke<{ note: string; existed: boolean }>('desk_start_piece', { id: b.id, title })
    emit('started', res.note)
  } catch (e) {
    actionError.value = { message: String(e), retry: startPiece }
  }
  starting.value = false
}

function openPath(path: string | null | undefined) {
  if (!path) return
  invoke('desk_open', { path }).catch((e) => {
    actionError.value = { message: String(e), retry: () => openPath(path) }
  })
}

// Long jobs (check / build) — tracked by "kind:id"; results arrive as events.
const running = ref<Record<string, boolean>>({})
const jobNotes = ref<Record<string, { ok: boolean; text: string }>>({})
function jobRunning(kind: string, id?: string | null) {
  return !!id && !!running.value[`${kind}:${id}`]
}

async function startJob(kind: 'check' | 'build', story: DeskStory) {
  if (jobRunning(kind, story.id)) return
  if (kind === 'build') {
    const ok = window.confirm(
      `Build "${story.slug}"?\n\nThis runs the full desk pipeline: it takes a few minutes and uses AI credits.`,
    )
    if (!ok) return
  }
  delete jobNotes.value[`${kind}:${story.id}`]
  running.value[`${kind}:${story.id}`] = true
  try {
    await invoke('desk_start_job', { kind, id: story.id })
  } catch (e) {
    running.value[`${kind}:${story.id}`] = false
    jobNotes.value[`${kind}:${story.id}`] = { ok: false, text: String(e) }
  }
}

// ── Keyboard (only while the Desk is showing) ─────────────────────────────
function onKey(e: KeyboardEvent) {
  if (e.metaKey || e.ctrlKey || e.altKey) return
  const t = e.target as HTMLElement | null
  if (t && (['INPUT', 'TEXTAREA', 'SELECT'].includes(t.tagName) || t.isContentEditable)) return
  if (document.querySelector('[class$="-overlay"]')) return
  const b = bench.value
  if (e.key === 'Enter' && b) {
    e.preventDefault()
    startPiece()
  } else if (e.key === 's' && b?.chart_html) {
    openPath(b.chart_html)
  } else if (e.key === 'x' && b?.can_check) {
    startJob('check', b)
  } else if (e.key === 'f' && b) {
    openPath(b.dir)
  } else if (['1', '2', '3', '4', '5'].includes(e.key) && b) {
    const i = parseInt(e.key) - 1
    if (i < b.angles.length) pickAngle(i)
  }
}

const disposers: Array<() => void> = []
onMounted(async () => {
  window.addEventListener('keydown', onKey)
  load()
  try {
    const jobs = await invoke<{ kind: string; id: string }[]>('desk_jobs')
    for (const j of jobs) running.value[`${j.kind}:${j.id}`] = true
  } catch {
    /* none */
  }
  disposers.push(await listen('desk-changed', () => load()))
  disposers.push(
    await listen<{ kind: string; id: string; state: string; summary?: string; error?: string }>(
      'desk-job',
      ({ payload: p }) => {
        const key = `${p.kind}:${p.id}`
        if (p.state === 'running') {
          running.value[key] = true
          return
        }
        running.value[key] = false
        jobNotes.value[key] =
          p.state === 'done'
            ? { ok: true, text: p.summary || (p.kind === 'check' ? 'Fact-check finished.' : 'Built.') }
            : { ok: false, text: p.error || 'That didn’t work.' }
      },
    ),
  )
})
onUnmounted(() => {
  window.removeEventListener('keydown', onKey)
  disposers.forEach((d) => d())
})
</script>

<template>
  <div class="desk">
    <!-- Header: streak · week · slot -->
    <header class="desk-head">
      <div class="streak" :class="{ zero: !desk?.streak }">
        <span class="streak-n">{{ desk?.streak ?? '–' }}</span>
        <span class="streak-label">day streak</span>
      </div>
      <div class="week" v-if="desk">
        <div
          v-for="d in desk.week"
          :key="d.date"
          class="day"
          :class="{ shipped: d.shipped, today: d.date === todayIso }"
          :data-tip="`${d.date}${d.shipped ? ' · shipped' : ''}`"
        >
          <span class="dot"></span>
          <span class="day-letter">{{ dayLetter(d.date) }}</span>
        </div>
      </div>
      <div class="slot" :class="{ open: desk?.slot.open && !desk?.shipped_today, done: desk?.shipped_today }">
        {{ slotLabel }}
      </div>
      <button class="refresh" :class="{ spinning: loading }" data-tip="Refresh the desk" @click="load">↻</button>
    </header>

    <!-- Couldn't read the desk -->
    <div v-if="loadError && !desk" class="notice bad">
      <div>{{ loadError }}</div>
      <button class="btn" @click="load">Try again</button>
    </div>
    <div v-else-if="loadError" class="notice subtle">
      Showing the last desk snapshot — {{ loadError }}
      <button class="link-btn" @click="load">Retry</button>
    </div>

    <div v-if="!desk && loading" class="skeleton">
      <div class="sk-block"></div>
      <div class="sk-line"></div>
      <div class="sk-line short"></div>
    </div>

    <!-- Shipped today: quiet celebration -->
    <div v-if="desk?.shipped_today" class="shipped-banner">
      <span class="shipped-mark">◆</span>
      Shipped today. That’s the day’s work — the streak is {{ desk.streak }}.
    </div>

    <!-- Today's piece is live: tagged share links -->
    <div v-if="livePiece" class="live-piece">
      <div class="live-url">
        <span class="live-dot"></span>
        {{ livePiece.published_url?.replace(/^https?:\/\//, '') }}
      </div>
      <div class="live-actions">
        <button class="btn" @click="copyShare('x', 'social')">
          {{ copied === 'x' ? 'Copied' : 'Copy link for X' }}
        </button>
        <button class="btn" @click="copyShare('newsletter', 'email')">
          {{ copied === 'newsletter' ? 'Copied' : 'Copy link for newsletter' }}
        </button>
      </div>
    </div>

    <!-- Today's bench -->
    <section v-if="bench" class="bench">
      <div class="bench-meta">
        <span class="label">Today’s bench</span>
        <span v-if="bench.star" class="star" :data-tip="bench.star === '★' ? 'Starred pick' : 'Pick'">{{ bench.star }}</span>
        <span v-if="bench.coverage" class="coverage" :data-tip="'Coverage: ' + bench.coverage">{{ bench.coverage }}</span>
        <span class="pill" :data-tone="checkPill.tone">{{ checkPill.label }}</span>
      </div>

      <div class="chart" v-if="chartSrc && !chartBroken" @click="openPath(bench.chart_html)" data-tip="Open the sketch (s)">
        <img :src="chartSrc" alt="Today's chart sketch" @error="chartBroken = true" />
      </div>

      <h2 class="finding-lead">{{ findingLead }}</h2>
      <p v-if="findingRest" class="finding-rest">{{ findingRest }}</p>

      <div v-if="bench.angles.length" class="angles">
        <div class="mini-label">Angles <span class="hint">click to copy · picked one becomes the title</span></div>
        <button
          v-for="(a, i) in bench.angles"
          :key="i"
          class="angle"
          :class="{ picked: pickedAngle === i }"
          @click="pickAngle(i)"
        >
          <kbd>{{ i + 1 }}</kbd>
          <span>{{ a }}</span>
        </button>
      </div>

      <div v-if="bench.question_for_ej" class="question">
        <div class="mini-label">Question for you</div>
        {{ bench.question_for_ej }}
      </div>

      <div class="actions">
        <button class="btn primary big" :disabled="starting" @click="startPiece">
          {{ starting ? 'Starting…' : 'Start piece' }} <kbd>⏎</kbd>
        </button>
        <button class="btn" :disabled="!bench.chart_html" @click="openPath(bench.chart_html)">Sketch <kbd>s</kbd></button>
        <button
          class="btn"
          :disabled="!bench.can_check || jobRunning('check', bench.id)"
          @click="startJob('check', bench)"
        >
          {{ jobRunning('check', bench.id) ? 'Checking…' : 'Fact-check my version' }} <kbd>x</kbd>
        </button>
        <button class="btn" @click="openPath(bench.dir)">Folder <kbd>f</kbd></button>
      </div>

      <div v-if="actionError" class="notice bad">
        <div>{{ actionError.message }}</div>
        <button class="btn" @click="actionError.retry()">Try again</button>
      </div>

      <div v-if="jobRunning('check', bench.id)" class="notice busy">
        Fact-checking your version — this takes a few minutes. You can keep working.
      </div>
      <div
        v-else-if="jobNotes['check:' + bench.id]"
        class="notice"
        :class="jobNotes['check:' + bench.id].ok ? 'subtle' : 'bad'"
      >
        {{ jobNotes['check:' + bench.id].text }}
      </div>

      <div v-if="bench.check" class="verdict" :data-tone="checkPill.tone">
        <div class="verdict-head">{{ bench.check.status }}</div>
        <div v-if="bench.check.summary" class="verdict-summary">{{ bench.check.summary }}</div>
        <ul v-if="bench.check.issues?.length" class="issues">
          <li v-for="(iss, i) in bench.check.issues" :key="i">{{ iss }}</li>
        </ul>
      </div>

      <details v-if="bench.caveats" class="caveats">
        <summary>Caveats</summary>
        <div class="caveats-body">{{ bench.caveats }}</div>
      </details>
    </section>

    <!-- Empty states -->
    <section v-else-if="desk && !loading" class="empty">
      <template v-if="beforeStock && !desk.stories.length">
        <div class="empty-title">Nothing yet</div>
        <div class="empty-sub">The desk stocks the bench around 6:30.</div>
      </template>
      <template v-else-if="!desk.shipped_today">
        <div class="empty-title">No bench today</div>
        <div class="empty-sub" v-if="onDeck.length">Build one of the on-deck stories below.</div>
        <div class="empty-sub" v-else>Nothing came in from the sources today.</div>
      </template>
    </section>

    <!-- On deck -->
    <section v-if="onDeck.length" class="ondeck">
      <div class="mini-label">On deck</div>
      <div v-for="s in onDeck" :key="s.id" class="deck-row">
        <div class="deck-text">
          <div class="deck-q">
            <span v-if="s.star" class="star">{{ s.star }}</span>
            {{ s.question || s.slug }}
          </div>
          <div class="deck-meta">
            {{ s.status.toLowerCase() }}<template v-if="s.coverage"> · {{ s.coverage }}</template>
          </div>
          <div
            v-if="jobNotes['build:' + s.id]"
            class="deck-note"
            :class="{ bad: !jobNotes['build:' + s.id].ok }"
          >
            {{ jobNotes['build:' + s.id].text }}
          </div>
        </div>
        <button
          v-if="s.can_build"
          class="btn"
          :disabled="jobRunning('build', s.id)"
          @click="startJob('build', s)"
        >
          {{ jobRunning('build', s.id) ? 'Building…' : 'Build' }}
        </button>
      </div>
    </section>
  </div>
</template>

<style scoped>
.desk {
  height: 100%;
  overflow-y: auto;
  overflow-x: hidden;
  padding: 16px 20px 28px;
  display: flex;
  flex-direction: column;
  gap: 16px;
  max-width: 760px;
}

/* Header */
.desk-head {
  display: flex;
  align-items: center;
  gap: 18px;
  flex-wrap: wrap;
}
.streak {
  display: flex;
  align-items: baseline;
  gap: 6px;
}
.streak-n {
  font-size: 44px;
  font-weight: 700;
  line-height: 1;
  font-variant-numeric: tabular-nums;
  color: var(--text-primary);
}
.streak.zero .streak-n {
  color: var(--text-tertiary);
}
.streak-label {
  font-size: 11px;
  color: var(--text-secondary);
  text-transform: uppercase;
  letter-spacing: 0.6px;
}
.week {
  display: flex;
  gap: 8px;
}
.day {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 3px;
}
.dot {
  width: 11px;
  height: 11px;
  border-radius: 50%;
  border: 1.5px solid var(--border-light);
  background: transparent;
}
.day.shipped .dot {
  background: var(--success);
  border-color: var(--success);
}
.day.today .dot {
  outline: 2px solid var(--accent);
  outline-offset: 2px;
}
.day-letter {
  font-size: 9px;
  color: var(--text-tertiary);
}
.slot {
  font-size: 12px;
  font-weight: 600;
  padding: 4px 10px;
  border-radius: 12px;
  background: var(--hover-bg);
  color: var(--text-secondary);
}
.slot.open {
  background: color-mix(in srgb, var(--accent) 16%, transparent);
  color: var(--accent);
}
.slot.done {
  background: color-mix(in srgb, var(--success) 16%, transparent);
  color: var(--success);
}
.desk-head {
  position: relative;
  padding-right: 28px;
}
.refresh {
  position: absolute;
  top: 0;
  right: 0;
  background: none;
  border: none;
  color: var(--text-tertiary);
  font-size: 16px;
  cursor: pointer;
  padding: 2px 6px;
  border-radius: 6px;
}
.refresh:hover {
  color: var(--text-primary);
  background: var(--hover-bg);
}
.refresh.spinning {
  animation: spin 0.9s linear infinite;
}
@keyframes spin {
  to {
    transform: rotate(360deg);
  }
}

/* Bench */
.bench {
  display: flex;
  flex-direction: column;
  gap: 12px;
}
.bench-meta {
  display: flex;
  align-items: center;
  gap: 8px;
  flex-wrap: wrap;
}
.label,
.mini-label {
  font-size: 10px;
  font-weight: 700;
  text-transform: uppercase;
  letter-spacing: 0.7px;
  color: var(--text-tertiary);
}
.mini-label {
  margin-bottom: 6px;
}
.mini-label .hint {
  text-transform: none;
  letter-spacing: 0;
  font-weight: 400;
  margin-left: 6px;
}
.star {
  color: var(--warning);
  font-size: 13px;
}
.coverage {
  font-size: 10px;
  padding: 1px 7px;
  border-radius: 8px;
  background: var(--hover-bg);
  color: var(--text-secondary);
}
.pill {
  font-size: 10px;
  font-weight: 600;
  padding: 2px 8px;
  border-radius: 8px;
  background: var(--hover-bg);
  color: var(--text-tertiary);
}
.pill[data-tone='ok'] {
  background: color-mix(in srgb, var(--success) 16%, transparent);
  color: var(--success);
}
.pill[data-tone='warn'] {
  background: color-mix(in srgb, var(--warning) 16%, transparent);
  color: var(--warning);
}
.pill[data-tone='bad'] {
  background: color-mix(in srgb, var(--danger) 16%, transparent);
  color: var(--danger);
}
.pill[data-tone='busy'] {
  background: color-mix(in srgb, var(--accent) 16%, transparent);
  color: var(--accent);
}
.chart {
  border-radius: 8px;
  overflow: hidden;
  border: 1px solid var(--border);
  cursor: zoom-in;
}
.chart img {
  display: block;
  width: 100%;
  max-height: 260px;
  object-fit: contain;
}
.finding-lead {
  font-size: 21px;
  line-height: 1.3;
  font-weight: 650;
  color: var(--text-primary);
  text-wrap: pretty;
}
.finding-rest {
  font-size: 13px;
  line-height: 1.55;
  color: var(--text-secondary);
}
.angles {
  display: flex;
  flex-direction: column;
  gap: 6px;
}
.angle {
  display: flex;
  gap: 8px;
  align-items: flex-start;
  text-align: left;
  font: inherit;
  font-size: 12.5px;
  line-height: 1.45;
  color: var(--text-primary);
  background: var(--hover-bg);
  border: 1px solid var(--border);
  border-radius: 8px;
  padding: 8px 10px;
  cursor: pointer;
}
.angle:hover {
  border-color: var(--border-light);
}
.angle.picked {
  border-color: var(--accent);
  background: color-mix(in srgb, var(--accent) 12%, transparent);
}
.question {
  font-size: 13px;
  line-height: 1.5;
  color: var(--text-primary);
  padding: 10px 12px;
  border-left: 3px solid var(--accent);
  background: var(--hover-bg);
  border-radius: 0 8px 8px 0;
}
.actions {
  display: flex;
  gap: 8px;
  flex-wrap: wrap;
  align-items: center;
}
.btn {
  font: inherit;
  font-size: 12px;
  font-weight: 500;
  display: inline-flex;
  align-items: center;
  gap: 6px;
  padding: 6px 12px;
  border-radius: 7px;
  border: 1px solid var(--border-light);
  background: var(--hover-bg);
  color: var(--text-primary);
  cursor: pointer;
}
.btn:hover:not(:disabled) {
  background: var(--active-bg);
}
.btn:disabled {
  opacity: 0.45;
  cursor: default;
}
.btn.primary {
  background: var(--accent);
  border-color: var(--accent);
  color: var(--accent-contrast);
}
.btn.primary:hover:not(:disabled) {
  background: var(--accent-strong);
}
.btn.big {
  font-size: 13.5px;
  font-weight: 650;
  padding: 8px 16px;
}
kbd {
  font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
  font-size: 9.5px;
  padding: 0 4px;
  border-radius: 3px;
  background: var(--kbd-bg);
  border: 1px solid var(--kbd-border);
  color: inherit;
  opacity: 0.8;
}
.btn.primary kbd {
  background: rgba(255, 255, 255, 0.2);
  border-color: rgba(255, 255, 255, 0.3);
}
.link-btn {
  background: none;
  border: none;
  color: var(--accent);
  cursor: pointer;
  font: inherit;
  padding: 0 4px;
}

/* Notices */
.notice {
  font-size: 12px;
  line-height: 1.5;
  padding: 10px 12px;
  border-radius: 8px;
  display: flex;
  gap: 10px;
  align-items: center;
  justify-content: space-between;
}
.notice.bad {
  background: color-mix(in srgb, var(--danger) 12%, transparent);
  color: var(--text-primary);
  border: 1px solid color-mix(in srgb, var(--danger) 35%, transparent);
}
.notice.busy {
  background: color-mix(in srgb, var(--accent) 10%, transparent);
  color: var(--text-secondary);
}
.notice.subtle {
  background: var(--hover-bg);
  color: var(--text-secondary);
}
.verdict {
  border: 1px solid var(--border-light);
  border-radius: 8px;
  padding: 10px 12px;
  font-size: 12.5px;
  line-height: 1.5;
}
.verdict[data-tone='ok'] {
  border-color: color-mix(in srgb, var(--success) 45%, transparent);
}
.verdict[data-tone='warn'] {
  border-color: color-mix(in srgb, var(--warning) 45%, transparent);
}
.verdict[data-tone='bad'] {
  border-color: color-mix(in srgb, var(--danger) 45%, transparent);
}
.verdict-head {
  font-weight: 700;
  margin-bottom: 4px;
}
.verdict-summary {
  color: var(--text-secondary);
}
.issues {
  margin: 6px 0 0 18px;
  color: var(--text-primary);
}
.caveats summary {
  font-size: 11px;
  color: var(--text-tertiary);
  cursor: pointer;
}
.caveats-body {
  white-space: pre-wrap;
  font-size: 11.5px;
  line-height: 1.55;
  color: var(--text-secondary);
  margin-top: 6px;
}

/* Live piece */
.live-piece {
  display: flex;
  flex-direction: column;
  gap: 8px;
  padding: 10px 12px;
  border-radius: 8px;
  border: 1px solid color-mix(in srgb, var(--success) 35%, transparent);
}
.live-url {
  font-size: 12px;
  color: var(--text-primary);
  word-break: break-all;
  display: flex;
  align-items: center;
  gap: 6px;
}
.live-dot {
  width: 7px;
  height: 7px;
  border-radius: 50%;
  background: var(--success);
  flex-shrink: 0;
}
.live-actions {
  display: flex;
  gap: 8px;
  flex-wrap: wrap;
}

/* Shipped / empty */
.shipped-banner {
  font-size: 13px;
  color: var(--text-secondary);
  padding: 10px 12px;
  border-radius: 8px;
  background: color-mix(in srgb, var(--success) 9%, transparent);
}
.shipped-mark {
  color: var(--success);
  margin-right: 6px;
}
.empty {
  padding: 28px 0 8px;
}
.empty-title {
  font-size: 20px;
  font-weight: 650;
  color: var(--text-primary);
}
.empty-sub {
  font-size: 13px;
  color: var(--text-secondary);
  margin-top: 4px;
}

/* On deck */
.ondeck {
  border-top: 1px solid var(--border);
  padding-top: 14px;
}
.deck-row {
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 8px 0;
  border-bottom: 1px solid var(--border);
}
.deck-row:last-child {
  border-bottom: none;
}
.deck-text {
  flex: 1;
  min-width: 0;
}
.deck-q {
  font-size: 13px;
  line-height: 1.4;
  color: var(--text-primary);
}
.deck-meta {
  font-size: 10.5px;
  color: var(--text-tertiary);
  margin-top: 2px;
}
.deck-note {
  font-size: 11px;
  color: var(--text-secondary);
  margin-top: 4px;
}
.deck-note.bad {
  color: var(--danger);
}

/* Skeleton */
.skeleton {
  display: flex;
  flex-direction: column;
  gap: 10px;
}
.sk-block,
.sk-line {
  background: var(--hover-bg);
  border-radius: 8px;
  animation: pulse 1.4s ease-in-out infinite;
}
.sk-block {
  height: 200px;
}
.sk-line {
  height: 18px;
}
.sk-line.short {
  width: 60%;
}
@keyframes pulse {
  50% {
    opacity: 0.5;
  }
}
</style>
