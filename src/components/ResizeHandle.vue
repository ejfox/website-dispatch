<script setup lang="ts">
/**
 * A native-feeling resize divider. Thin visual line, wide invisible hit target
 * (the thing that makes macOS resizers feel good — you don't have to pixel-hunt
 * a 1px line). Pair with useResizable() which owns the size + drag math.
 *
 * The parent positions this absolutely on the seam and binds `start` / `reset`:
 *   <ResizeHandle axis="x" :active="dragging" @down="start" @reset="reset" />
 */
defineProps<{
  axis?: 'x' | 'y'
  /** true while a drag is in progress — keeps the line lit */
  active?: boolean
}>()

const emit = defineEmits<{
  (e: 'down', ev: PointerEvent): void
  (e: 'reset'): void
}>()
</script>

<template>
  <div
    class="resize-handle"
    :class="[`axis-${axis ?? 'x'}`, { active }]"
    @pointerdown="emit('down', $event)"
    @dblclick="emit('reset')"
  >
    <div class="line" />
  </div>
</template>

<style scoped>
/* Wide hit target, thin visual. The handle element is the grab zone; the
   inner `.line` is the hairline you actually see.
   Position-agnostic: works in flow as a flex sibling (Media Library, GearPanel)
   or absolutely positioned by the parent on a grid seam (App.vue sidebar). */
.resize-handle {
  display: flex;
  align-items: stretch;
  justify-content: center;
  flex-shrink: 0;
  z-index: 20;
}
.axis-x {
  width: 9px;
  cursor: col-resize;
}
.axis-y {
  height: 9px;
  cursor: row-resize;
  flex-direction: column;
}

.line {
  background: transparent;
  transition: background 0.12s ease;
}
.axis-x .line {
  width: 1px;
}
.axis-y .line {
  height: 1px;
}

/* Light up on hover / during drag with the system accent. */
.resize-handle:hover .line,
.resize-handle.active .line {
  background: var(--accent);
}

/* The global [data-tip]::after positions the HUD below-center of its host.
   That works for buttons but not for a divider that spans an entire window
   height (axis-x) or panel width (axis-y) — the bubble would land far off
   the visible target. Re-anchor each axis to sit close to the handle. */
.axis-x[data-tip]::after {
  top: 50%;
  left: calc(100% + 6px);
  transform: translateY(-50%);
}
.axis-x[data-tip]:hover::after {
  transform: translateY(-50%);
}
.axis-y[data-tip]::after {
  top: auto;
  bottom: calc(100% + 4px);
  left: 50%;
  transform: translateX(-50%);
}
.axis-y[data-tip]:hover::after {
  transform: translateX(-50%);
}
</style>
