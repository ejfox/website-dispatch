<script setup lang="ts">
import { PhShieldWarning } from '@phosphor-icons/vue'

defineProps<{ warnings: string[] }>()
</script>

<template>
  <div v-if="warnings.length" class="lint-receipt">
    <div class="lint-receipt-header">
      <span>Lint Receipt</span>
      <span class="lint-receipt-count">{{ warnings.length }}</span>
    </div>
    <div class="lint-receipt-divider"></div>
    <div class="lint-receipt-list">
      <div
        v-for="warning in warnings"
        :key="warning"
        class="lint-receipt-item"
        :class="{ privacy: warning.startsWith('[privacy]') }"
      >
        <span class="lint-receipt-bullet" :class="{ privacy: warning.startsWith('[privacy]') }">
          <PhShieldWarning v-if="warning.startsWith('[privacy]')" :size="12" weight="fill" />
          <template v-else>&bull;</template>
        </span>
        <span class="lint-receipt-text">{{ warning.startsWith('[privacy]') ? warning.slice(9) : warning }}</span>
      </div>
    </div>
    <div class="lint-receipt-footer">Dispatch</div>
  </div>
</template>

<style scoped>
.lint-receipt {
  margin: 10px 16px 12px;
  padding: 10px 12px;
  background: var(--bg-tertiary);
  border: 1px dashed var(--border-light);
  border-radius: 4px;
  font-family: 'SF Mono', 'Menlo', monospace;
  font-size: 10px;
  color: var(--text-secondary);
}

.lint-receipt-header {
  display: flex;
  justify-content: space-between;
  text-transform: uppercase;
  letter-spacing: 0.12em;
  color: var(--text-tertiary);
  font-size: 9px;
}

.lint-receipt-count {
  font-variant-numeric: tabular-nums;
}
.lint-receipt-divider {
  height: 1px;
  margin: 6px 0 8px;
  background: repeating-linear-gradient(90deg, var(--border-light) 0 4px, transparent 4px 8px);
}
.lint-receipt-list {
  display: flex;
  flex-direction: column;
  gap: 4px;
}
.lint-receipt-item {
  display: flex;
  gap: 6px;
  align-items: flex-start;
}
.lint-receipt-bullet {
  color: var(--warning);
  line-height: 1;
}
.lint-receipt-text {
  color: var(--text-primary);
}

.lint-receipt-item.privacy {
  background: rgba(239, 68, 68, 0.1);
  padding: 2px 6px;
  border-radius: 3px;
  border-left: 2px solid #ef4444;
}

.lint-receipt-bullet.privacy {
  color: #ef4444;
  font-size: 10px;
}
.lint-receipt-item.privacy .lint-receipt-text {
  color: #ef4444;
  font-weight: 500;
}
.lint-receipt-footer {
  margin-top: 8px;
  text-align: right;
  text-transform: uppercase;
  letter-spacing: 0.2em;
  color: var(--text-tertiary);
  font-size: 8px;
}
</style>
