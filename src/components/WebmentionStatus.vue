<script setup lang="ts">
import { ref } from 'vue'
import { PhBroadcast, PhCheck, PhWarningCircle } from '@phosphor-icons/vue'
import type { WebmentionReport } from '../types'

/**
 * Compact status chip + click-to-expand details for the webmentions sent
 * after publish. Replaces the old "Webmention" toolbar button + modal-style
 * WebmentionReport panel.
 *
 * Mental model: "Dispatch told the sites I linked to that I linked to them."
 * The user sees a one-line outcome and can pop a small list if they want
 * to know which sites — or hit Resend if anything failed.
 */
const props = defineProps<{
  /** null = nothing has fired yet (initial state) */
  report: WebmentionReport | null
  sending: boolean
}>()

const emit = defineEmits<{ resend: [] }>()

const expanded = ref(false)
</script>

<template>
  <div class="wm-chip" :class="{ sending, none: report && report.sent === 0 && !report.errors }">
    <button
      class="wm-summary"
      :disabled="sending"
      @click="expanded = !expanded"
      :data-tip="
        sending
          ? 'Notifying sites you linked to in this post…'
          : report
            ? 'Click for details · webmentions tell other sites you linked to them'
            : ''
      "
    >
      <PhBroadcast v-if="sending" :size="11" weight="bold" class="spin" />
      <PhCheck v-else-if="report && report.errors === 0" :size="11" weight="bold" />
      <PhWarningCircle v-else-if="report" :size="11" weight="bold" />
      <PhBroadcast v-else :size="11" weight="bold" />

      <span class="wm-summary-text">
        <template v-if="sending">Notifying linked sites…</template>
        <template v-else-if="report && report.sent > 0">
          Notified {{ report.sent }} site{{ report.sent === 1 ? '' : 's' }}
          <span v-if="report.errors" class="wm-err-inline">· {{ report.errors }} failed</span>
        </template>
        <template v-else-if="report && report.errors > 0">
          {{ report.errors }} site{{ report.errors === 1 ? '' : 's' }} failed
        </template>
        <template v-else-if="report">No outbound links to notify</template>
      </span>
    </button>

    <button
      v-if="report && (report.errors > 0 || report.sent === 0)"
      class="wm-resend"
      @click="$emit('resend')"
      :disabled="sending"
      data-tip="Retry sending webmentions"
    >
      Resend
    </button>

    <!-- Expandable detail row: shows where each notification went -->
    <div v-if="expanded && report" class="wm-detail">
      <div class="wm-detail-header">
        <span>Webmentions tell other sites that you linked to them.</span>
      </div>
      <div v-if="report.results.length === 0" class="wm-empty">
        This post doesn't link to any external sites.
      </div>
      <ul v-else class="wm-list">
        <li v-for="r in report.results" :key="r.target" class="wm-item" :class="r.status">
          <span class="wm-dot"></span>
          <a :href="r.target" target="_blank" class="wm-target">
            {{
              r.target
                .replace(/^https?:\/\//, '')
                .split('/')
                .slice(0, 2)
                .join('/')
            }}
          </a>
          <span v-if="r.message" class="wm-msg">{{ r.message }}</span>
        </li>
      </ul>
    </div>
  </div>
</template>

<style scoped>
.wm-chip {
  display: flex;
  align-items: center;
  flex-wrap: wrap;
  gap: 6px;
  padding: 4px 12px 4px 16px;
  font-size: 10px;
  color: var(--text-tertiary);
  border-bottom: 1px solid var(--border);
  background: var(--bg-tertiary);
}

.wm-chip.sending .wm-summary {
  color: var(--text-secondary);
}

.wm-summary {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  background: none;
  border: none;
  padding: 0;
  color: inherit;
  font: inherit;
  cursor: pointer;
  flex: 1;
  text-align: left;
}
.wm-summary:hover:not(:disabled) {
  color: var(--text-primary);
}
.wm-summary-text {
  font-size: 10px;
}
.wm-err-inline {
  color: var(--danger);
  margin-left: 3px;
}

.spin {
  animation: wm-spin 1.4s linear infinite;
}
@keyframes wm-spin {
  to {
    transform: rotate(360deg);
  }
}

.wm-resend {
  margin-left: auto;
  padding: 2px 8px;
  background: var(--bg-secondary);
  border: 1px solid var(--border);
  color: var(--text-secondary);
  font-size: 10px;
  border-radius: 4px;
  cursor: pointer;
  font-family: inherit;
}
.wm-resend:hover:not(:disabled) {
  background: var(--hover-bg);
  color: var(--text-primary);
}
.wm-resend:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

/* Expanded detail */
.wm-detail {
  flex: 1 1 100%;
  margin-top: 4px;
  padding-top: 6px;
  border-top: 1px solid var(--border);
}
.wm-detail-header {
  color: var(--text-tertiary);
  font-size: 10px;
  margin-bottom: 4px;
  line-height: 1.4;
}
.wm-empty {
  font-size: 10px;
  color: var(--text-tertiary);
  font-style: italic;
}
.wm-list {
  list-style: none;
  padding: 0;
  margin: 0;
  max-height: 160px;
  overflow-y: auto;
  font-family: 'SF Mono', monospace;
  font-size: 10px;
}
.wm-item {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 2px 0;
}
.wm-dot {
  width: 5px;
  height: 5px;
  border-radius: 50%;
  flex-shrink: 0;
}
.wm-item.sent .wm-dot {
  background: var(--success);
}
.wm-item.no_endpoint .wm-dot {
  background: var(--text-tertiary);
}
.wm-item.error .wm-dot {
  background: var(--danger);
}
.wm-target {
  color: var(--text-secondary);
  text-decoration: none;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.wm-target:hover {
  color: var(--text-primary);
}
.wm-msg {
  margin-left: auto;
  color: var(--text-tertiary);
  flex-shrink: 0;
}
</style>
