<script setup lang="ts">
import {
  PhTextAa,
  PhTag,
  PhCalendarBlank,
  PhFolder,
  PhCircleWavy,
  PhGitBranch,
  PhChartBar,
  PhGlobe,
  PhLockSimple,
  PhEye,
} from '@phosphor-icons/vue'
import type { MarkdownFile, GitStatus, PostAnalytics } from '../types'
import { formatDate, formatAgeShort, formatDateCompact } from '../utils/formatting'

defineProps<{
  file: MarkdownFile
  obsidianConnected: boolean
  gitStatus: GitStatus | null
  postStats: PostAnalytics | null
  loadingStats: boolean
  suggestedTags: string[]
  availableTags: Record<string, number>
  addingTag: boolean
  metadataExpanded: boolean
  hasUnpublishedChanges: boolean
  isUnlisted: boolean
  isPasswordProtected: boolean
}>()

defineEmits<{
  'toggle-metadata': []
  'add-tag': [tag: string]
}>()
</script>

<template>
  <div class="info">
    <!-- Summary bar (always visible, clickable) -->
    <div class="info-summary" @click="$emit('toggle-metadata')">
      <span class="info-toggle" :class="{ expanded: metadataExpanded }">&#9654;</span>
      <span class="info-chip">
        <PhTextAa :size="9" weight="duotone" />
        {{ file.word_count }}w
      </span>
      <span class="date-sep">&middot;</span>
      <span class="info-chip">
        <PhTag :size="9" weight="duotone" />
        {{ file.tags.length }} tags
      </span>
      <span class="date-sep">&middot;</span>
      <span class="info-chip">
        <PhCalendarBlank :size="9" weight="duotone" />
        {{ file.date ? formatDate(file.date) : formatDateCompact(file.created) }}
      </span>
    </div>

    <!-- Detail rows (collapsed by default) -->
    <div v-show="metadataExpanded" class="info-detail">
      <div v-if="file.content_type === 'weeknote'" class="row">
        <span class="label">Type</span>
        <span class="weeknote-type">Week Note</span>
      </div>
      <div class="row">
        <span class="label">
          <PhFolder :size="10" weight="duotone" />
          Source
        </span>
        <code>{{ file.source_dir || '.' }}/{{ file.filename }}</code>
      </div>
      <div class="row">
        <span class="label">
          <PhCalendarBlank :size="10" weight="duotone" />
          Dates
        </span>
        <span class="dates-compact">
          <span>c: {{ formatAgeShort(file.created) }}</span>
          <span class="date-sep">&middot;</span>
          <span :class="{ 'modified-highlight': hasUnpublishedChanges }">e: {{ formatAgeShort(file.modified) }}</span>
          <template v-if="file.published_date">
            <span class="date-sep">&middot;</span>
            <span class="published">p: {{ formatAgeShort(file.published_date) }}</span>
          </template>
        </span>
      </div>
      <div class="row">
        <span class="label">
          <PhCircleWavy :size="10" weight="duotone" />
          Obsidian
        </span>
        <span :class="obsidianConnected ? 'connected' : 'disconnected'">
          {{ obsidianConnected ? 'connected' : 'not connected' }}
        </span>
      </div>
      <div class="row">
        <span class="label">
          <PhGitBranch :size="10" weight="duotone" />
          Git
        </span>
        <span v-if="!gitStatus">checking...</span>
        <span v-else-if="gitStatus.ok" class="connected">
          {{ gitStatus.branch }}
        </span>
        <span v-else class="git-warning" :title="gitStatus.error || ''">
          {{ gitStatus.error }}
        </span>
      </div>
      <div v-if="postStats || loadingStats" class="row">
        <span class="label">
          <PhChartBar :size="10" weight="duotone" />
          Analytics
        </span>
        <span v-if="loadingStats" class="muted">loading...</span>
        <span v-else-if="postStats" class="analytics-inline">
          {{ postStats.pageviews }} views &middot; {{ postStats.visitors }} visitors
          <template v-if="postStats.totaltime > 0">
            &middot; {{ Math.round(postStats.totaltime / Math.max(postStats.visits, 1)) }}s avg
          </template>
        </span>
      </div>
      <div class="row">
        <span class="label">
          <PhGlobe :size="10" weight="duotone" />
          Visibility
        </span>
        <span v-if="isPasswordProtected" class="protected-text" :title="`Password: ${file.password}`">
          <PhLockSimple :size="11" weight="bold" />
          Protected
        </span>
        <span v-else-if="isUnlisted" class="unlisted-text">
          <PhEye :size="11" weight="bold" />
          Unlisted
        </span>
        <span v-else class="public-text">Public</span>
      </div>
    </div>

    <!-- Always visible: actionable metadata -->
    <div v-if="file.tags.length" class="row">
      <span class="label">
        <PhTag :size="10" weight="duotone" />
        Tags
      </span>
      <span class="tags-list">{{ file.tags.join(', ') }}</span>
    </div>
    <div v-if="suggestedTags.length > 0" class="row suggested-tags-row">
      <span class="label">
        <PhTag :size="10" weight="duotone" />
        Suggest
      </span>
      <div class="suggested-tags">
        <button
          v-for="tag in suggestedTags"
          :key="tag"
          class="tag-chip"
          @click="$emit('add-tag', tag)"
          :disabled="addingTag"
          :title="`Click to add '${tag}' to frontmatter`"
        >
          <span class="tag-plus">+</span>
          {{ tag }}
          <span class="tag-count">{{ availableTags[tag] }}</span>
        </button>
      </div>
    </div>
  </div>
</template>

<style scoped>
.info {
  padding: 0 16px 12px;
  display: flex;
  flex-direction: column;
  gap: 4px;
  border-bottom: 1px solid var(--border);
}

.info-summary {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 4px 0;
  font-size: 10px;
  color: var(--text-secondary);
  cursor: pointer;
  user-select: none;
}

.info-summary:hover {
  color: var(--text-primary);
}

.info-chip {
  display: inline-flex;
  align-items: center;
  gap: 3px;
}

.info-toggle {
  font-size: 8px;
  color: var(--text-tertiary);
  transition: transform 0.15s ease;
  display: inline-block;
}

.info-toggle.expanded {
  transform: rotate(90deg);
}

.info-detail {
  display: flex;
  flex-direction: column;
  gap: 4px;
  padding: 4px 0 4px 4px;
  margin-bottom: 4px;
  border-left: 2px solid var(--border-light);
}

.row {
  display: flex;
  font-size: 11px;
  gap: 12px;
}

.label {
  width: 68px;
  color: var(--text-tertiary);
  flex-shrink: 0;
  display: flex;
  align-items: center;
  gap: 3px;
}

.weeknote-type {
  color: #f59e0b;
  font-weight: 600;
  font-size: 10px;
}

.row code {
  font-family: 'SF Mono', monospace;
  font-size: 10px;
  color: var(--text-secondary);
}

.dates-compact {
  display: flex;
  align-items: center;
  gap: 4px;
  font-size: 10px;
  color: var(--text-secondary);
  flex-wrap: wrap;
}

.date-sep {
  color: var(--text-tertiary);
  opacity: 0.5;
}

.connected,
.published {
  color: var(--success);
}

.modified-highlight {
  color: var(--warning);
  font-weight: 500;
}

.disconnected {
  color: var(--warning);
  opacity: 0.7;
}

.git-warning {
  color: var(--warning);
  font-size: 10px;
}

.analytics-inline {
  font-variant-numeric: tabular-nums;
  font-feature-settings: 'tnum';
  color: var(--text-secondary);
}

.unlisted-text {
  color: #6366f1;
}

.protected-text {
  color: #8b5cf6;
}

.public-text {
  color: var(--success);
}

.tags-list {
  color: var(--text-secondary);
}

/* Suggested tags */
.suggested-tags-row {
  align-items: flex-start;
}

.suggested-tags {
  display: flex;
  flex-wrap: wrap;
  gap: 4px;
}

.tag-chip {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  padding: 2px 8px;
  font-size: 10px;
  font-weight: 500;
  background: var(--accent);
  border: 1px solid var(--border);
  border-radius: 12px;
  color: var(--text-secondary);
  cursor: pointer;
  transition: all 0.15s cubic-bezier(0.34, 1.56, 0.64, 1);
}

.tag-chip:hover {
  background: var(--bg-tertiary);
  border-color: var(--border-light);
  color: var(--text-primary);
  transform: scale(1.05);
}

.tag-chip:active {
  transform: scale(0.95);
}

.tag-chip:disabled {
  opacity: 0.5;
  cursor: wait;
}

.tag-plus {
  font-weight: 600;
  color: var(--success);
  margin-right: 2px;
}

.tag-count {
  font-size: 8px;
  opacity: 0.5;
  font-weight: 400;
}
</style>
