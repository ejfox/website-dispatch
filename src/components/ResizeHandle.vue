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
   inner `.line` is the hairline you actually see. */
.resize-handle {
  position: absolute;
  z-index: 20;
  display: flex;
  align-items: stretch;
  justify-content: center;
}
.axis-x {
  top: 0;
  bottom: 0;
  width: 9px;
  cursor: col-resize;
}
.axis-y {
  left: 0;
  right: 0;
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
</style>
