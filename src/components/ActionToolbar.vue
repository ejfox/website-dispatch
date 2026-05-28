<script setup lang="ts">
import { Command } from 'lucide-vue-next'
import {
  PhNotePencil,
  PhPlay,
  PhArrowSquareOut,
  PhArrowsClockwise,
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
  border-radius: 5px;
  cursor: pointer;
  transition: background 0.12s ease, color 0.12s ease, border-color 0.12s ease;
}

.tool-btn:hover {
  background: var(--bg-tertiary);
  border-color: var(--border-light);
  color: var(--text-primary);
}

.tool-btn:active {
  /* No scale(0.97) — macOS push-buttons darken on press, not shrink. */
  background: color-mix(in srgb, var(--bg-tertiary) 90%, black);
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
  /* macOS push-button proportions — tighter than the old 8/16 and
     without the lift-on-hover / brightness(1.1) animation. Mac buttons
     darken on hover; they don't bounce. */
  padding: 5px 12px;
  border: none;
  border-radius: 5px;
  font-size: 11px;
  font-weight: 500;
  cursor: pointer;
  text-decoration: none;
  background: var(--bg-tertiary);
  color: var(--text-primary);
  transition: background 0.12s ease, color 0.12s ease;
  min-height: 24px;
  display: inline-flex;
  align-items: center;
  gap: 4px;
}

.btn:hover {
  background: color-mix(in srgb, var(--bg-tertiary) 70%, white);
}

.btn:active {
  /* No transform — macOS native uses a darker fill on press, not movement. */
  background: color-mix(in srgb, var(--bg-tertiary) 90%, black);
}

.btn.accent {
  background: var(--accent-soft);
  color: var(--accent);
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
  background: var(--accent) !important;
  color: var(--accent-contrast) !important;
}

.publish-btn:hover:not(:disabled) {
  background: var(--accent-strong) !important;
}

.publish-btn:disabled {
  background: var(--bg-tertiary) !important;
  color: var(--text-tertiary) !important;
}

.publish-unlisted-btn {
  background: var(--accent-soft) !important;
  color: var(--accent) !important;
  border: 1px solid color-mix(in srgb, var(--accent) 30%, transparent) !important;
}

.publish-unlisted-btn:hover {
  background: color-mix(in srgb, var(--accent) 22%, transparent) !important;
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
  color: var(--warning) !important;
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
  color: var(--warning);
  padding: 3px 8px;
  border-radius: 6px;
  background: rgba(245, 158, 11, 0.1);
  letter-spacing: 0.03em;
}

.syndicate-btn {
  color: var(--accent) !important;
}
</style>
