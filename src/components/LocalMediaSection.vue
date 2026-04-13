<script setup lang="ts">
import { PhImageSquare, PhFilmSlate, PhWarningCircle } from '@phosphor-icons/vue'
import LocalMediaFixer from './LocalMediaFixer.vue'
import type { LocalMediaRef } from '../types'

defineProps<{
  localMedia: LocalMediaRef[]
  loadingLocalMedia: boolean
  showMediaFixer: boolean
  filePath: string
}>()

const emit = defineEmits<{
  'show-fixer': []
  'close-fixer': []
  'media-fixed': []
}>()
</script>

<template>
  <div v-if="localMedia.length > 0 || loadingLocalMedia" class="local-media-section">
    <div class="local-media-header">
      <span class="label">
        <PhImageSquare :size="10" weight="duotone" />
        Media
      </span>
      <span class="count warning">{{ loadingLocalMedia ? '...' : localMedia.length }}</span>
      <button v-if="localMedia.length > 0" @click="emit('show-fixer')" class="fix-btn">Fix</button>
    </div>
    <div v-if="loadingLocalMedia" class="local-media-loading">Scanning...</div>
    <div v-else class="local-media-list">
      <div v-for="media in localMedia.slice(0, 3)" :key="media.path + media.line_number" class="local-media-item">
        <span class="media-type">
          <PhFilmSlate v-if="media.media_type === 'video'" :size="12" weight="duotone" />
          <PhImageSquare v-else :size="12" weight="duotone" />
        </span>
        <span class="media-path">{{ media.path }}</span>
        <span v-if="!media.resolved_path" class="missing">
          <PhWarningCircle :size="10" weight="fill" />
          not found
        </span>
      </div>
      <div v-if="localMedia.length > 3" class="local-media-more">+{{ localMedia.length - 3 }} more</div>
    </div>
  </div>

  <!-- Local Media Fixer Modal -->
  <LocalMediaFixer
    v-if="showMediaFixer"
    :file-path="filePath"
    :local-media="localMedia"
    @close="emit('close-fixer')"
    @fixed="emit('media-fixed')"
  />
</template>

<style scoped>
/* Shared section label/count styles */
.label {
  font-size: 10px;
  font-weight: 600;
  color: var(--text-tertiary);
  text-transform: uppercase;
  letter-spacing: 0.75px;
}
.count {
  font-size: 10px;
  font-weight: 600;
  color: var(--text-secondary);
}
.count.warning {
  color: var(--warning);
}

/* Local Media */
.local-media-section {
  padding: 8px 16px;
  border-bottom: 1px solid var(--border);
}

.local-media-header {
  display: flex;
  align-items: center;
  gap: 8px;
  margin-bottom: 6px;
}

.fix-btn {
  margin-left: auto;
  padding: 3px 10px;
  font-size: 10px;
  font-weight: 500;
  background: var(--warning);
  color: #000;
  border: none;
  border-radius: 4px;
  cursor: pointer;
}

.fix-btn:hover {
  filter: brightness(1.1);
}
.local-media-loading {
  font-size: 10px;
  color: var(--text-tertiary);
}
.local-media-list {
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.local-media-item {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 4px 6px;
  background: color-mix(in srgb, var(--warning) 15%, transparent);
  border-radius: 4px;
  font-size: 10px;
}

.media-type {
  flex-shrink: 0;
}
.media-path {
  flex: 1;
  font-family: 'SF Mono', monospace;
  color: var(--text-secondary);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}
.local-media-item .missing {
  color: var(--warning);
  font-size: 9px;
}
.missing {
  color: var(--warning);
  display: inline-flex;
  align-items: center;
  gap: 2px;
}
.local-media-more {
  font-size: 10px;
  color: var(--text-tertiary);
  text-align: center;
  padding: 4px;
}
</style>
