import { useLocalStorage } from '@vueuse/core'

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
