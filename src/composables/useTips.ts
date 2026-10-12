// First-use tips, modeled on Apple's TipKit.
//
// A tip is one plain sentence about a capability that isn't obvious from the
// UI. The rules TipKit uses, and that this follows:
//
//   eligible     a tip shows only in the context it's about (`when`), never
//                on a timer or at launch
//   invalidated  a tip retires for good when you *do* the thing it teaches
//                (`invalidateTip`), not only when you close it
//   one at a time  at most one tip on screen; when several are eligible the
//                one listed first in TIPS wins
//   quiet        after you close a tip, no other tip appears for a little
//                while, so they don't chain
//   max shows    a tip ignored across MAX_SESSIONS launches retires itself
//
// State lives in localStorage under one key. Every read and write is wrapped:
// in a private window or with site data blocked, tips simply show this
// session and are forgotten, which is fine.
//
// QA: `?tips=reset` clears everything on load; `?tips=all` shows every
// eligible tip at once and ignores the rules above (TipKit's
// showAllTipsForTesting).
//
// Wiring left to other surfaces: `resetTips()` is the Help › "Show Tips
// Again" / Settings action; `tipsEnabled` is a Settings switch if wanted.

import { ref, reactive, computed, watch, onUnmounted, getCurrentInstance, type Ref } from 'vue'

/** Every tip, in priority order. The copy lives at the call site; this is
 *  the registry the arbiter ranks by. */
export const TIPS = [
  'list-states',
  'inspector',
  'publish',
  'changed',
  'alt-text',
  'social-card',
  'syndicate',
  'drop-media',
  'palette',
] as const
export type TipId = (typeof TIPS)[number]

const STORAGE_KEY = 'dispatch-tips-v1'
/** A tip shown in this many separate launches without a click retires. */
const MAX_SESSIONS = 5
/** Quiet period after a tip is closed before another may appear. */
const QUIET_MS = 20_000
/** How long a tip's context must hold before it appears, so arrowing
 *  through posts with j/k doesn't flash tips. */
const SETTLE_MS = 600

interface TipStore {
  /** id → when it was closed or invalidated (ms). */
  done: Partial<Record<TipId, { at: number; reason: 'closed' | 'action' | 'ignored' }>>
  /** id → number of launches it has been shown in. */
  sessions: Partial<Record<TipId, number>>
  /** Named event counters for event rules (e.g. posts selected). */
  events: Record<string, number>
  enabled: boolean
}

function blank(): TipStore {
  return { done: {}, sessions: {}, events: {}, enabled: true }
}

function load(): TipStore {
  try {
    const raw = localStorage.getItem(STORAGE_KEY)
    if (!raw) return blank()
    const parsed = JSON.parse(raw)
    return { ...blank(), ...parsed }
  } catch {
    return blank()
  }
}

function save() {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(store))
  } catch {
    /* storage unavailable: tips are per-session only */
  }
}

const params = typeof location !== 'undefined' ? new URLSearchParams(location.search) : null
const testMode = params?.get('tips') ?? null
if (testMode === 'reset') {
  try {
    localStorage.removeItem(STORAGE_KEY)
  } catch {
    /* ignore */
  }
}
const showAll = testMode === 'all'

const store = reactive<TipStore>(load())
// Session counts as they were at launch, so a tip's own showing this session
// can't retire it mid-session.
const sessionsAtLaunch: Partial<Record<TipId, number>> = { ...store.sessions }
const shownThisSession = new Set<TipId>()

/** Tips whose context currently holds (after the settle delay), counted
 *  per placement so one placement letting go doesn't hide another. */
const candidates = reactive(new Map<TipId, number>())
function addCandidate(id: TipId) {
  candidates.set(id, (candidates.get(id) ?? 0) + 1)
}
function dropCandidate(id: TipId) {
  const n = (candidates.get(id) ?? 0) - 1
  if (n > 0) candidates.set(id, n)
  else candidates.delete(id)
}
const quietUntil = ref(0)
const now = ref(Date.now())

function isRetired(id: TipId): boolean {
  if (showAll) return false
  if (store.done[id]) return true
  return (sessionsAtLaunch[id] ?? 0) >= MAX_SESSIONS
}

/** The one tip allowed on screen right now. */
const current = computed<TipId | null>(() => {
  if (!store.enabled && !showAll) return null
  if (!showAll && now.value < quietUntil.value) return null
  for (const id of TIPS) {
    if (candidates.has(id) && !isRetired(id)) return id
  }
  return null
})

function markShown(id: TipId) {
  if (shownThisSession.has(id)) return
  shownThisSession.add(id)
  store.sessions[id] = (store.sessions[id] ?? 0) + 1
  save()
}

/** Close a tip for good. `reason: 'action'` is TipKit's
 *  `.actionPerformed`: the user did the thing, so the tip has nothing left
 *  to teach. */
export function invalidateTip(id: TipId, reason: 'closed' | 'action' = 'action') {
  if (store.done[id]) return
  const wasShowing = current.value === id
  store.done[id] = { at: Date.now(), reason }
  save()
  // Only a tip the user actually saw starts the quiet period.
  if (wasShowing && !showAll) {
    quietUntil.value = Date.now() + QUIET_MS
    setTimeout(() => (now.value = Date.now()), QUIET_MS + 50)
  }
}

/** Count an event for event-based rules ("after 4 posts opened…"). */
export function recordTipEvent(name: string) {
  store.events[name] = (store.events[name] ?? 0) + 1
  save()
}

export function tipEventCount(name: string): number {
  return store.events[name] ?? 0
}

/** Forget every closed tip, so they show again in context. The
 *  Help menu / Settings "Show Tips Again" action. */
export function resetTips() {
  store.done = {}
  store.sessions = {}
  store.events = {}
  for (const k of Object.keys(sessionsAtLaunch)) delete sessionsAtLaunch[k as TipId]
  shownThisSession.clear()
  quietUntil.value = 0
  store.enabled = true
  save()
}

/** Global on/off (a Settings switch, if one is wanted). */
export const tipsEnabled = computed({
  get: () => store.enabled,
  set: (v: boolean) => {
    store.enabled = v
    save()
  },
})

/**
 * Bind a tip to a component. `when` is the context rule; the tip shows when
 * it holds, it's this tip's turn, and the tip isn't retired.
 *
 *   const tip = useTip('alt-text', () => missingAltTextCount.value > 0)
 *   <InlineTip v-if="tip.show.value" @close="tip.close()">…</InlineTip>
 */
export function useTip(id: TipId, when: () => boolean): { show: Ref<boolean>; close: () => void } {
  let timer: ReturnType<typeof setTimeout> | null = null
  // This placement's own context. Two placements may share an id (the
  // inspector tip lives in the inspector, or at its toggle when hidden);
  // only the one whose context holds draws it.
  const ready = ref(false)

  const stop = watch(
    when,
    (ok) => {
      if (timer) clearTimeout(timer)
      timer = null
      if (!ok) {
        if (ready.value) dropCandidate(id)
        ready.value = false
        return
      }
      if (ready.value) return
      timer = setTimeout(() => {
        ready.value = true
        addCandidate(id)
      }, SETTLE_MS)
    },
    { immediate: true },
  )

  const show = computed(() => ready.value && (showAll ? !isRetired(id) : current.value === id))
  watch(show, (s) => s && markShown(id), { immediate: true })

  if (getCurrentInstance()) {
    onUnmounted(() => {
      stop()
      if (timer) clearTimeout(timer)
      if (ready.value) dropCandidate(id)
    })
  }

  return { show, close: () => invalidateTip(id, 'closed') }
}

// ── Help request ────────────────────────────────────────────────────────────
// A tip deep inside FilePreview can link to Help without threading an emit
// through every parent. App.vue watches this and opens the overlay.
export const helpRequest = ref<{ section: 'overview' | 'shortcuts'; at: number } | null>(null)
export function requestHelp(section: 'overview' | 'shortcuts' = 'overview') {
  helpRequest.value = { section, at: Date.now() }
}
