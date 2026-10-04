// Grouped UI-state composables: toast notifications, command-palette
// history (recency ranking), and resizable-panel size persistence.

import { ref, readonly } from 'vue'
import { useLocalStorage } from '@vueuse/core'
import { invoke } from '@tauri-apps/api/core'

// ── Toasts ──────────────────────────────────────────────────────────────────

export type ToastKind = 'success' | 'info' | 'warn' | 'error'

export interface Toast {
  id: number
  kind: ToastKind
  message: string
  detail?: string
  /** Action button label, e.g. "Retry" / "Copy Error". */
  action?: { label: string; run: () => void | Promise<void> }
  /** Auto-dismiss after this many ms. 0 = sticky (user must dismiss). */
  ttl: number
}

// Singleton state — one toast stack across the whole app.
const toasts = ref<Toast[]>([])
let nextId = 1

function playSound(name: string) {
  invoke('play_system_sound', { name }).catch(() => {})
}

function push(t: Omit<Toast, 'id'>): number {
  const id = nextId++
  toasts.value.push({ ...t, id })
  if (t.ttl > 0) {
    setTimeout(() => dismiss(id), t.ttl)
  }
  return id
}

function dismiss(id: number) {
  toasts.value = toasts.value.filter((t) => t.id !== id)
}

function clear() {
  toasts.value = []
}

export function useToasts() {
  return {
    toasts: readonly(toasts),
    dismiss,
    clear,

    success: (message: string, detail?: string) => push({ kind: 'success', message, detail, ttl: 3000 }),

    info: (message: string, detail?: string) => push({ kind: 'info', message, detail, ttl: 3000 }),

    warn: (message: string, detail?: string) => {
      playSound('Tink')
      return push({ kind: 'warn', message, detail, ttl: 5000 })
    },

    /** Errors stick until dismissed and offer a "Copy Error" action by default. */
    error: (message: string, detail?: string) => {
      playSound('Sosumi')
      const action = detail
        ? {
            label: 'Copy Error',
            run: () => navigator.clipboard.writeText(detail),
          }
        : undefined
      return push({ kind: 'error', message, detail, action, ttl: 0 })
    },

    /** Custom toast — full control. */
    push,
  }
}

// ── Command-palette history ─────────────────────────────────────────────────

/**
 * Persistent usage tracking for the command palette.
 *
 * - **Frecency**: each item's score = uses × recency-decay. Items you ran
 *   recently AND often float to the top. Stale items decay out.
 * - **Recents**: last N actions you ran, in order. Surfaced at the top of
 *   the palette when the query is empty.
 *
 * Stored in localStorage so the personalization survives restarts.
 */

interface UsageRecord {
  count: number
  /** Last-used timestamp (ms). */
  last: number
}

const HALF_LIFE_DAYS = 14 // score halves every two weeks of disuse
const RECENTS_CAP = 5

export function usePaletteHistory() {
  const usage = useLocalStorage<Record<string, UsageRecord>>('dispatch-palette-usage', {})
  const recents = useLocalStorage<string[]>('dispatch-palette-recents', [])

  function record(id: string) {
    // File-row "select file" actions get their own bucket per slug — but
    // that would explode the dictionary. Skip files; only persist actions.
    if (id.startsWith('file:')) return
    const now = Date.now()
    const existing = usage.value[id] || { count: 0, last: now }
    usage.value = {
      ...usage.value,
      [id]: { count: existing.count + 1, last: now },
    }

    // Update recents (move-to-front, dedupe, cap at N).
    recents.value = [id, ...recents.value.filter((r) => r !== id)].slice(0, RECENTS_CAP)
  }

  /**
   * Frecency score for ranking. 0 if never used. Higher is better.
   * Uses exponential decay so a command used 10× last year scores below
   * one used 3× this week.
   */
  function frecency(id: string): number {
    const rec = usage.value[id]
    if (!rec) return 0
    const ageDays = (Date.now() - rec.last) / 86_400_000
    const decay = Math.pow(0.5, ageDays / HALF_LIFE_DAYS)
    return rec.count * decay
  }

  return {
    record,
    frecency,
    recents,
  }
}

// ── Resizable panels ────────────────────────────────────────────────────────

/**
 * Drag-to-resize a panel, macOS style. Persists the size to localStorage and
 * exposes a `dragging` flag so the layout can suppress CSS transitions while
 * the user is actively dragging (otherwise the panel rubber-bands).
 *
 * Generalizes the hand-rolled handler that used to live in GearPanel.vue.
 * Uses Pointer events so it works with trackpad + captures the pointer even
 * when it slips outside the thin handle during a fast drag.
 *
 * @param key     localStorage key (e.g. 'dispatch-sidebar-width')
 * @param options default / min / max bounds and axis.
 *   - axis 'x' resizes width (col-resize), 'y' resizes height (row-resize)
 *   - max may be a number or a getter, for bounds relative to window size
 *   - invert: true when the handle sits on the far edge and dragging *toward*
 *     the panel origin should grow it (GearPanel's bottom-anchored detail pane)
 */
export function useResizable(
  key: string,
  options: {
    default: number
    min: number
    max: number | (() => number)
    axis?: 'x' | 'y'
    invert?: boolean
    /** When set, the start size is read fresh on each drag (e.g. from the
     *  DOM via offsetHeight). Lets a panel use natural sizing until the user
     *  first drags, without the divider jumping on grab. */
    getStartSize?: () => number | null | undefined
  }
) {
  const { default: def, min, axis = 'x', invert = false, getStartSize } = options
  const size = useLocalStorage(key, def)
  const dragging = ref(false)

  const resolveMax = () =>
    typeof options.max === 'function' ? options.max() : options.max

  const clamp = (v: number) => Math.max(min, Math.min(resolveMax(), v))

  function start(e: PointerEvent) {
    e.preventDefault()
    const startPos = axis === 'x' ? e.clientX : e.clientY
    const measured = getStartSize?.()
    const startSize = measured != null && measured > 0 ? measured : size.value
    const dir = invert ? -1 : 1

    const onMove = (ev: PointerEvent) => {
      const pos = axis === 'x' ? ev.clientX : ev.clientY
      size.value = clamp(startSize + (pos - startPos) * dir)
    }
    const onUp = () => {
      window.removeEventListener('pointermove', onMove)
      window.removeEventListener('pointerup', onUp)
      document.body.style.userSelect = ''
      document.body.style.cursor = ''
      dragging.value = false
    }

    dragging.value = true
    document.body.style.userSelect = 'none'
    document.body.style.cursor = axis === 'x' ? 'col-resize' : 'row-resize'
    window.addEventListener('pointermove', onMove)
    window.addEventListener('pointerup', onUp)
  }

  /** Double-click a handle to snap back to the default size. */
  function reset() {
    size.value = def
  }

  return { size, dragging, start, reset }
}
