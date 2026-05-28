import { ref, computed, watch, nextTick, type Ref } from 'vue'
import { useElementSize } from '@vueuse/core'

/**
 * NSToolbar-style responsive overflow. When the container is too narrow to
 * fit every item, returns the indices of the items that should be hidden so
 * the caller can render a `»` chevron in their place. The chevron's click
 * handler is the caller's job — typically it pops a native Tauri menu
 * containing the hidden items.
 *
 * Usage:
 *   const filterRefs = ref<HTMLElement[]>([])
 *   const { hiddenIndices, remeasure } = useOverflowMenu(filtersRef, filterRefs)
 *   // <button v-for ... :ref="(el) => (filterRefs.value[i] = el)"
 *   //         :class="{ overflowed: hiddenIndices.includes(i) }">
 *   // In CSS: .overflowed { display: none; }
 *
 * Caller-side requirements:
 *   - The container should have `overflow: hidden` so any briefly-overflowing
 *     items during a measurement pass don't paint past the edge.
 *   - Items should have `flex-shrink: 0` so they don't silently shrink instead
 *     of overflowing (measurement assumes natural width).
 */
export function useOverflowMenu(
  containerRef: Ref<HTMLElement | null>,
  itemsRef: Ref<(HTMLElement | null)[]>,
  options: { reserve?: number } = {}
) {
  const reserve = options.reserve ?? 28 // px held back for the chevron button
  const naturalWidths = ref<number[]>([])
  const { width: containerWidth } = useElementSize(containerRef)

  // Measurement happens with every item visible (when `measuring` is true,
  // hiddenIndices returns [], so the template paints all items). After the
  // next paint we cache offsetWidth as each item's "natural" width.
  const measuring = ref(true)

  function captureWidths() {
    naturalWidths.value = itemsRef.value.map((el) => el?.offsetWidth ?? 0)
    measuring.value = false
  }

  /** Call after the underlying item set or their labels change. */
  function remeasure() {
    measuring.value = true
    nextTick(captureWidths)
  }

  // Initial measure on mount + whenever the item count changes.
  watch(
    () => itemsRef.value.length,
    () => remeasure(),
    { immediate: true, flush: 'post' },
  )

  const hiddenIndices = computed<number[]>(() => {
    if (measuring.value) return []
    if (!naturalWidths.value.length || !containerWidth.value) return []

    const total = naturalWidths.value.reduce((a, b) => a + b, 0)
    if (total <= containerWidth.value) return []

    // Doesn't all fit — reserve room for the chevron, then walk left-to-right
    // marking everything past the cap as hidden.
    const cap = containerWidth.value - reserve
    const hidden: number[] = []
    let used = 0
    for (let i = 0; i < naturalWidths.value.length; i++) {
      used += naturalWidths.value[i]
      if (used > cap) hidden.push(i)
    }
    return hidden
  })

  return { hiddenIndices, remeasure }
}
