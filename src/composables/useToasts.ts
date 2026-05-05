import { ref, readonly } from 'vue'
import { invoke } from '@tauri-apps/api/core'

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
