/**
 * Click→paint tracer for the file-switch interaction.
 *
 * The existing `[perf] file-switch …` logs in FilePreview only cover the middle
 * of the story — from `loadFileContent` start to when the rendered HTML is
 * assigned to a ref. They miss the two spans a user actually feels:
 *
 *   1. click → loadFileContent start  (emit, App.vue @select, the selectedFile
 *      watchers firing their invokes, and Vue re-rendering the 180-row sidebar)
 *   2. rendered-ref-set → real paint  (Vue patching the post into the DOM, then
 *      the browser doing layout + paint)
 *
 * This tracer brackets the entire span. `begin()` is called at the click,
 * `mark()` at each pipeline boundary, and `paintAfter()` schedules a double
 * requestAnimationFrame so the final measurement lands *after* the browser has
 * actually painted the new frame — then it flushes one grouped table.
 *
 * One interaction is timed at a time (you click one row). A fresh `begin()`
 * resets state, so an interaction that ends without a paint (error/empty
 * branch) is simply overwritten by the next click — no leak, just no report.
 *
 * Dev-only: no-ops unless import.meta.env.DEV.
 */
interface Phase {
  name: string
  at: number // ms since begin()
}

const DEV = import.meta.env?.DEV ?? false

let t0 = 0
let label = ''
let phases: Phase[] = []
let live = false

export const perfTrace = {
  /** Start timing a new interaction (call at the click). */
  begin(l: string) {
    if (!DEV) return
    t0 = performance.now()
    label = l
    phases = []
    live = true
  },

  /** Record a boundary crossing, timestamped relative to begin(). */
  mark(name: string) {
    if (!DEV || !live) return
    phases.push({ name, at: performance.now() - t0 })
  },

  /**
   * Measure time until the frame *after* the next paint, then flush the report.
   * Double-rAF: the first callback runs just before the upcoming paint, the
   * second at the start of the following frame — by which point the frame we
   * cared about is on screen.
   */
  paintAfter(name = 'paint') {
    if (!DEV || !live) return
    requestAnimationFrame(() => {
      requestAnimationFrame(() => {
        if (!live) return
        phases.push({ name, at: performance.now() - t0 })
        this.flush()
      })
    })
  },

  /** Print one grouped table of per-phase deltas + cumulative time. */
  flush() {
    if (!DEV || !live) return
    live = false
    const total = phases.length ? phases[phases.length - 1].at : 0
    let prev = 0
    const rows = phases.map((p) => {
      const delta = p.at - prev
      prev = p.at
      return { phase: p.name, 'Δ ms': +delta.toFixed(1), 't ms': +p.at.toFixed(1) }
    })
    const color = total > 200 ? '#f87171' : total > 80 ? '#fbbf24' : '#34d399'
    // Plain one-liner first — this is what shows up legibly in the Tauri dev
    // terminal (the webview→stdout bridge doesn't render console.table). Each
    // segment is the Δ since the previous mark, so the biggest number is the
    // phase to blame.
    const chain = rows.map((r) => `${r.phase} +${r['Δ ms']}`).join('  →  ')
    // eslint-disable-next-line no-console
    console.log(`[perf] click→paint ${total.toFixed(0)}ms · ${label}  ::  ${chain}`)
    // Pretty grouped table for the webview devtools console.
    // eslint-disable-next-line no-console
    console.groupCollapsed(
      `%c⏱ click→paint ${total.toFixed(0)}ms · ${label}`,
      `color:${color};font-weight:bold`,
    )
    // eslint-disable-next-line no-console
    console.table(rows)
    // eslint-disable-next-line no-console
    console.groupEnd()
  },
}
