<script setup lang="ts">
import { Command } from 'lucide-vue-next'
import {
  PhNotePencil,
  PhPlay,
  PhArrowSquareOut,
  PhArrowsClockwise,
  PhGlobe,
  PhArrowSquareUpRight,
  PhTrash,
  PhTrophy,
  PhClock,
  PhEyeSlash,
} from '@phosphor-icons/vue'
import type { EditorConfig, PublishTarget } from '../types'

defineProps<{
  enabledEditors: EditorConfig[]
  publishTargets: PublishTarget[]
  hasMultipleTargets: boolean
  selectedTargetId: string | null
  isLive: boolean
  liveUrl: string | null
  sendingWebmentions: boolean
  isCrowned: boolean
  crowning: boolean
  unpublishing: boolean
  publishing: boolean
  isSafe: boolean
  isScheduled: boolean
  isUnlisted: boolean
}>()

defineEmits<{
  'open-obsidian': []
  'open-editor': [appName: string]
  'open-preview': []
  'select-target': [id: string]
  'view-live': []
  'trigger-webmentions': []
  'show-syndication': []
  'crown-post': []
  unpublish: []
  'open-publish-confirm': [isRepublish: boolean]
  'publish-unlisted': []
  'toggle-schedule': []
}>()
</script>

<template>
  <div class="toolbar">
    <div class="toolbar-open">
      <button
        v-for="editor in enabledEditors"
        :key="editor.app_name"
        @click="editor.app_name === 'Obsidian' ? $emit('open-obsidian') : $emit('open-editor', editor.app_name)"
        class="tool-btn"
        :data-tip="`Open in ${editor.name}`"
      >
        <PhNotePencil :size="12" weight="duotone" />
        {{ editor.name }}
      </button>
      <button @click="$emit('open-preview')" class="tool-btn" data-tip="Open local preview server">
        <PhPlay :size="12" weight="fill" />
        Preview
      </button>
    </div>
    <div class="toolbar-actions">
      <select
        v-if="hasMultipleTargets"
        class="target-select"
        :value="selectedTargetId || publishTargets.find((t) => t.is_default)?.id"
        @change="$emit('select-target', ($event.target as HTMLSelectElement).value)"
      >
        <option v-for="t in publishTargets" :key="t.id" :value="t.id">
          {{ t.name }}
        </option>
      </select>
      <template v-if="isLive">
        <a :href="liveUrl!" target="_blank" class="btn">
          <PhArrowSquareOut :size="12" weight="bold" />
          View
        </a>
        <button
          @click="$emit('trigger-webmentions')"
          :disabled="sendingWebmentions"
          class="btn webmention-btn"
          data-tip="Send webmentions to linked sites"
        >
          <PhGlobe :size="12" weight="bold" />
          {{ sendingWebmentions ? 'Sending...' : 'Webmention' }}
        </button>
        <button @click="$emit('show-syndication')" class="btn syndicate-btn" data-tip="Share to social platforms">
          <PhArrowSquareUpRight :size="12" weight="bold" />
          Syndicate
        </button>
        <button
          v-if="!isCrowned"
          @click="$emit('crown-post')"
          :disabled="crowning"
          class="btn crown-btn"
          data-tip="Create interactive Vue page takeover"
        >
          <PhTrophy :size="12" weight="bold" />
          {{ crowning ? 'Crowning...' : 'Crown' }}
        </button>
        <span v-else class="crowned-badge" data-tip="This post has a Vue page takeover">
          <PhTrophy :size="10" weight="fill" />
          Crowned
        </span>
        <button @click="$emit('unpublish')" :disabled="unpublishing" class="btn">
          <PhTrash :size="12" weight="bold" />
          {{ unpublishing ? '...' : 'Unpublish' }}
        </button>
        <button @click="$emit('open-publish-confirm', true)" :disabled="publishing" class="btn accent">
          <PhArrowsClockwise :size="12" weight="bold" />
          {{ publishing ? '...' : 'Republish' }}
        </button>
      </template>
      <template v-else>
        <button v-if="isSafe && !isScheduled" @click="$emit('toggle-schedule')" class="btn">
          <PhClock :size="12" weight="bold" />
          Schedule
        </button>
        <button
          v-if="isSafe && !isUnlisted && !publishing"
          @click="$emit('publish-unlisted')"
          class="btn publish-unlisted-btn"
        >
          <PhEyeSlash :size="12" weight="bold" />
          Unlisted
        </button>
        <button
          @click="$emit('open-publish-confirm', false)"
          :disabled="!isSafe || publishing"
          class="btn accent publish-btn"
          :class="{ disabled: !isSafe, full: !isSafe || isScheduled }"
        >
          <span>{{ publishing ? 'Publishing...' : isSafe ? 'Publish' : 'Fix issues to publish' }}</span>
          <kbd v-if="isSafe && !publishing" class="shortcut-hint">
            <Command :size="10" />
            &crarr;
          </kbd>
        </button>
      </template>
    </div>
  </div>
</template>

<style scoped>
.toolbar {
  padding: 8px 16px;
  display: flex;
  flex-direction: column;
  gap: 6px;
  border-bottom: 1px solid var(--border);
}

.toolbar-open {
  display: flex;
  gap: 4px;
}

.tool-btn {
  flex: 1;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  gap: 5px;
  padding: 4px 6px;
  height: 26px;
  font-size: 10px;
  font-weight: 500;
  color: var(--text-secondary);
  background: var(--bg-secondary);
  border: 1px solid var(--border);
  border-radius: 4px;
  cursor: pointer;
  transition: all 0.15s cubic-bezier(0.34, 1.56, 0.64, 1);
}

.tool-btn:hover {
  background: var(--bg-tertiary);
  border-color: var(--border-light);
  color: var(--text-primary);
}

.tool-btn:active {
  transform: scale(0.97);
}

.tool-btn svg {
  flex-shrink: 0;
}

.toolbar-actions {
  display: flex;
  gap: 8px;
  align-items: center;
}

.target-select {
  padding: 4px 8px;
  font-size: 10px;
  background: var(--bg-tertiary);
  border: 1px solid var(--border);
  border-radius: 4px;
  color: var(--text-secondary);
  cursor: pointer;
  font-family: 'SF Mono', monospace;
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

.btn:hover {
  filter: brightness(1.1);
  transform: translateY(-1px);
}

.btn:active {
  transform: translateY(0);
}

.btn.accent {
  background: color-mix(in srgb, var(--success) 20%, var(--bg-tertiary));
  color: var(--success);
  font-weight: 600;
}

.btn.full {
  flex: 1;
}

.btn.disabled,
.btn:disabled {
  background: var(--bg-tertiary);
  color: var(--text-tertiary);
  cursor: not-allowed;
  filter: none;
}

.publish-btn {
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 8px;
  background: var(--success) !important;
  color: #000 !important;
}

.publish-btn:hover:not(:disabled) {
  background: color-mix(in srgb, var(--success) 90%, #fff) !important;
}

.publish-btn:disabled {
  background: var(--bg-tertiary) !important;
  color: var(--text-tertiary) !important;
}

.publish-unlisted-btn {
  background: rgba(99, 102, 241, 0.15) !important;
  color: #818cf8 !important;
  border: 1px solid rgba(99, 102, 241, 0.3) !important;
}

.publish-unlisted-btn:hover {
  background: rgba(99, 102, 241, 0.25) !important;
}

.shortcut-hint {
  display: inline-flex;
  align-items: center;
  gap: 2px;
  padding: 2px 5px;
  background: rgba(0, 0, 0, 0.15);
  border: none;
  border-radius: 3px;
  font-size: 9px;
  font-family: 'SF Mono', monospace;
  color: inherit;
  opacity: 0.8;
}

/* Crown button */
.crown-btn {
  background: rgba(245, 158, 11, 0.15) !important;
  color: #f59e0b !important;
  border: 1px solid rgba(245, 158, 11, 0.2) !important;
}
.crown-btn:hover:not(:disabled) {
  background: rgba(245, 158, 11, 0.25) !important;
}

.crowned-badge {
  display: inline-flex;
  align-items: center;
  gap: 3px;
  font-size: 9px;
  font-weight: 600;
  color: #f59e0b;
  padding: 3px 8px;
  border-radius: 6px;
  background: rgba(245, 158, 11, 0.1);
  letter-spacing: 0.03em;
}

/* Webmention styles */
.webmention-btn {
  background: rgba(99, 102, 241, 0.15) !important;
  color: #818cf8 !important;
  border: 1px solid rgba(99, 102, 241, 0.2) !important;
}

.webmention-btn:hover:not(:disabled) {
  background: rgba(99, 102, 241, 0.25) !important;
}

.syndicate-btn {
  color: #a78bfa !important;
}
</style>
