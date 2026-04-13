<script setup lang="ts">
import { PhGlobe } from '@phosphor-icons/vue'
import type { WebmentionReport } from '../types'

defineProps<{
  report: WebmentionReport
  sendingWebmentions: boolean
}>()

defineEmits<{
  close: []
  'resend-bridgy': []
}>()
</script>

<template>
  <div class="webmention-report">
    <div class="webmention-header">
      <span class="webmention-title">
        <PhGlobe :size="12" weight="bold" />
        Webmentions
      </span>
      <span class="webmention-stats">
        <span v-if="report.sent" class="wm-sent">{{ report.sent }} sent</span>
        <span v-if="report.no_endpoint" class="wm-none">{{ report.no_endpoint }} no endpoint</span>
        <span v-if="report.errors" class="wm-err">{{ report.errors }} failed</span>
      </span>
      <button class="wm-close" @click="$emit('close')">&times;</button>
    </div>
    <div class="webmention-list">
      <div v-for="r in report.results" :key="r.target" class="wm-item" :class="r.status">
        <span class="wm-status-dot"></span>
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
      </div>
    </div>
    <button
      v-if="!sendingWebmentions"
      class="btn webmention-btn wm-bridgy"
      @click="$emit('resend-bridgy')"
      data-tip="Also send to Bridgy Fed for fediverse"
    >
      <PhGlobe :size="11" weight="bold" />
      Resend + Bridgy Fed
    </button>
  </div>
</template>

<style scoped>
.webmention-report {
  margin: 0 16px 8px;
  background: var(--bg-tertiary);
  border: 1px solid var(--border);
  border-radius: 6px;
  overflow: hidden;
}

.webmention-header {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 6px 10px;
  border-bottom: 1px solid var(--border);
  font-size: 10px;
}

.webmention-title {
  font-weight: 600;
  color: #818cf8;
  display: flex;
  align-items: center;
  gap: 4px;
}

.webmention-stats {
  flex: 1;
  display: flex;
  gap: 8px;
  font-family: 'SF Mono', monospace;
  font-size: 9px;
}

.wm-sent {
  color: var(--success);
}
.wm-none {
  color: var(--text-tertiary);
}
.wm-err {
  color: var(--error, #ef4444);
}

.wm-close {
  background: none;
  border: none;
  color: var(--text-tertiary);
  cursor: pointer;
  font-size: 14px;
  padding: 0 2px;
  line-height: 1;
}

.webmention-list {
  max-height: 120px;
  overflow-y: auto;
}

.wm-item {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 3px 10px;
  font-size: 9px;
  font-family: 'SF Mono', monospace;
  border-bottom: 1px solid color-mix(in srgb, var(--border) 50%, transparent);
}

.wm-item:last-child {
  border-bottom: none;
}

.wm-status-dot {
  width: 5px;
  height: 5px;
  border-radius: 50%;
  flex-shrink: 0;
}

.wm-item.sent .wm-status-dot {
  background: var(--success);
}
.wm-item.no_endpoint .wm-status-dot {
  background: var(--text-tertiary);
}
.wm-item.error .wm-status-dot {
  background: var(--error, #ef4444);
}

.wm-target {
  color: var(--text-secondary);
  text-decoration: none;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  flex: 1;
}

.wm-target:hover {
  color: var(--text-primary);
}

.wm-msg {
  color: var(--text-tertiary);
  flex-shrink: 0;
}

.btn {
  padding: 8px 16px;
  border: none;
  border-radius: 6px;
  font-size: 11px;
  font-weight: 500;
  cursor: pointer;
  text-decoration: none;
  background: var(--bg-tertiary);
  color: var(--text-primary);
  transition: all 0.15s cubic-bezier(0.34, 1.56, 0.64, 1);
  min-height: 28px;
  display: inline-flex;
  align-items: center;
  gap: 4px;
}

.webmention-btn {
  background: rgba(99, 102, 241, 0.15) !important;
  color: #818cf8 !important;
  border: 1px solid rgba(99, 102, 241, 0.2) !important;
}

.webmention-btn:hover:not(:disabled) {
  background: rgba(99, 102, 241, 0.25) !important;
}

.wm-bridgy {
  margin: 6px 10px;
  font-size: 9px;
}
</style>
