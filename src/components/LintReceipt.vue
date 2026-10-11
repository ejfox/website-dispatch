<script setup lang="ts">
/**
 * "What to fix": the publish lint, as short sentences in the inspector.
 * Renders nothing when there is nothing to fix. The section shell (label,
 * padding, hairline) comes from the inspector's `.insp-*` primitives.
 */
import { PhShieldWarning } from '@phosphor-icons/vue'

defineProps<{ warnings: string[] }>()
</script>

<template>
  <div v-if="warnings.length" class="lint insp-section">
    <div class="insp-row">
      <span class="insp-label">What to fix</span>
      <span class="lint-count">{{ warnings.length }}</span>
    </div>
    <ul class="lint-list">
      <li
        v-for="warning in warnings"
        :key="warning"
        class="lint-item"
        :class="{ privacy: warning.startsWith('[privacy]') }"
      >
        <span class="lint-bullet" aria-hidden="true">
          <PhShieldWarning v-if="warning.startsWith('[privacy]')" :size="11" weight="fill" />
          <template v-else>&bull;</template>
        </span>
        <span class="lint-text">{{ warning.startsWith('[privacy]') ? warning.slice(9) : warning }}</span>
      </li>
    </ul>
  </div>
</template>

<style scoped>
.lint-count {
  font-size: 10px;
  font-weight: 600;
  font-variant-numeric: tabular-nums;
  color: var(--ink-attention);
}
.lint-list {
  list-style: none;
  margin: 0;
  padding: 0;
  display: flex;
  flex-direction: column;
  gap: var(--space-1);
}
.lint-item {
  display: flex;
  gap: 6px;
  align-items: flex-start;
  font-size: 11px;
  line-height: 16px;
  color: var(--label);
  min-width: 0;
}
.lint-text {
  min-width: 0;
  overflow-wrap: anywhere;
}
.lint-bullet {
  flex-shrink: 0;
  width: 11px;
  text-align: center;
  color: var(--status-attention);
  line-height: 16px;
}
/* A privacy hit is broken, not "needs attention". */
.lint-item.privacy .lint-bullet {
  color: var(--status-broken);
  display: inline-flex;
  align-items: center;
  height: 16px;
}
.lint-item.privacy .lint-text {
  font-weight: 500;
}
</style>
