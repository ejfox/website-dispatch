<script setup lang="ts">
/**
 * A first-use tip, TipKit style. Quiet card, one sentence, a close button,
 * at most one link. Pair it with `useTip()` (composables/useTips.ts), which
 * decides whether it's this tip's turn; this component only draws it.
 *
 *   inline (default)   sits in the layout, like TipKit's TipView
 *   anchor="below-end" floats under its positioned parent with an arrow at
 *                      the right, like a popoverTip on a toolbar button
 */
import { PhLightbulb, PhX } from '@phosphor-icons/vue'
import UiButton from './UiButton.vue'

withDefaults(
  defineProps<{
    title?: string
    /** Optional link-style action, e.g. "How publishing works". */
    action?: string
    anchor?: 'none' | 'below-end' | 'below-start'
  }>(),
  { anchor: 'none' },
)
const emit = defineEmits<{ close: []; action: [] }>()
</script>

<template>
  <div
    class="tip"
    :class="anchor !== 'none' ? ['is-popover', `is-${anchor}`] : null"
    role="note"
    aria-label="Tip"
    data-no-drag
  >
    <PhLightbulb class="tip-glyph" :size="14" weight="duotone" aria-hidden="true" />
    <div class="tip-body">
      <div v-if="title" class="tip-title">{{ title }}</div>
      <div class="tip-text"><slot /></div>
      <button v-if="action" type="button" class="tip-action" @click="emit('action')">{{ action }}</button>
    </div>
    <UiButton
      variant="plain"
      size="small"
      :icon="PhX"
      class="tip-close"
      aria-label="Close tip"
      @click="emit('close')"
    />
  </div>
</template>

<style scoped>
.tip {
  display: flex;
  align-items: flex-start;
  gap: var(--space-2);
  padding: var(--space-2) var(--space-1) var(--space-2) var(--space-3);
  background: var(--under-page);
  border: 0.5px solid var(--separator);
  border-radius: 8px;
  font-size: 11px;
  line-height: 1.45;
  color: var(--label-2);
  text-align: left;
  user-select: text;
  -webkit-app-region: no-drag;
  animation: tip-in 0.22s var(--ease-out-expo);
}

.tip-glyph {
  flex-shrink: 0;
  margin-top: 1px;
  color: var(--label-2);
}

.tip-body {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 2px;
  padding-top: 1px;
}

.tip-title {
  font-weight: 600;
  color: var(--label);
}

.tip-text :deep(kbd) {
  padding: 0 4px;
  font-size: 10px;
}

.tip-action {
  align-self: flex-start;
  margin-top: 2px;
  padding: 0;
  border: none;
  background: none;
  font: inherit;
  font-weight: 500;
  color: var(--accent);
  cursor: pointer;
}
.tip-action:hover {
  text-decoration: underline;
}

.tip-close {
  flex-shrink: 0;
  margin-top: -3px;
  color: var(--label-2);
}

/* Popover: floats under its positioned parent, arrow toward the anchor. */
.tip.is-popover {
  position: absolute;
  top: calc(100% + 8px);
  z-index: 60;
  width: 248px;
  white-space: normal;
  box-shadow: var(--shadow-md), 0 0 0 0.5px rgba(0, 0, 0, 0.15);
}
.tip.is-below-end {
  right: 0;
}
.tip.is-below-start {
  left: 0;
}
.tip.is-popover::before {
  content: '';
  position: absolute;
  top: -5px;
  width: 9px;
  height: 9px;
  background: var(--under-page);
  border-left: 0.5px solid var(--separator);
  border-top: 0.5px solid var(--separator);
  transform: rotate(45deg);
}
.tip.is-below-end::before {
  right: 10px;
}
.tip.is-below-start::before {
  left: 10px;
}

@keyframes tip-in {
  from {
    opacity: 0;
    transform: translateY(-3px);
  }
  to {
    opacity: 1;
    transform: translateY(0);
  }
}

@media (prefers-reduced-motion: reduce) {
  .tip {
    animation: none;
  }
}
</style>
