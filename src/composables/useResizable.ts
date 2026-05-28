import { ref } from 'vue'
import { useLocalStorage } from '@vueuse/core'

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
