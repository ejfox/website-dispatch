<script setup lang="ts">
/**
 * The post's own facts, as key/value rows in the inspector's Details
 * section: words, date, folder, visibility, tags (right-click for options)
 * and suggested tags, then whatever rows the parent slots in, then a small
 * disclosure for the rarely-needed rows (created/edited/published,
 * Obsidian, git).
 */
import { PhCaretDown } from '@phosphor-icons/vue'
import { Menu, MenuItem, PredefinedMenuItem } from '@tauri-apps/api/menu'
import type { MarkdownFile, GitStatus } from '../types'
import { formatDate, formatAgeShort, formatDateCompact } from '../utils/formatting'
import { useAppConfig } from '../composables/useVaultData'

const { publishTargets } = useAppConfig()
function siteDomain() {
  const t = publishTargets.value.find((x) => x.is_default) || publishTargets.value[0]
  return (t?.domain || 'https://ejfox.com').replace(/\/$/, '')
}

async function showTagMenu(tag: string, e: MouseEvent) {
  e.preventDefault()
  const url = `${siteDomain()}/tag/${tag.toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/^-|-$/g, '')}`
  const menu = await Menu.new({
    items: [
      await MenuItem.new({
        text: 'Open Tag Page',
        action: () => window.open(url, '_blank'),
      }),
      await PredefinedMenuItem.new({ item: 'Separator' }),
      await MenuItem.new({ text: 'Copy Tag', action: () => navigator.clipboard.writeText(tag) }),
      await MenuItem.new({
        text: 'Copy Tag URL',
        action: () => navigator.clipboard.writeText(url),
      }),
      await MenuItem.new({
        text: 'Copy as #Hashtag',
        action: () => navigator.clipboard.writeText(`#${tag}`),
      }),
    ],
  })
  await menu.popup()
}

defineProps<{
  file: MarkdownFile
  obsidianConnected: boolean
  gitStatus: GitStatus | null
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
    <div class="insp-kv">
      <span class="insp-key">Words</span>
      <span class="insp-val">{{ file.word_count.toLocaleString() }}</span>
    </div>
    <div class="insp-kv">
      <span class="insp-key">Date</span>
      <span class="insp-val">{{ formatDate(file.date) || formatDateCompact(file.created) }}</span>
    </div>
    <div v-if="file.content_type === 'weeknote'" class="insp-kv">
      <span class="insp-key">Type</span>
      <span class="insp-val">Week Note</span>
    </div>
    <div class="insp-kv">
      <span class="insp-key">Folder</span>
      <span class="insp-val is-mono is-muted is-oneline" :title="`${file.source_dir || '.'}/${file.filename}`">
        {{ file.source_dir || '.' }}/{{ file.filename }}
      </span>
    </div>
    <div class="insp-kv">
      <span class="insp-key">Visibility</span>
      <!-- Visibility is not a status: neutral ink. -->
      <span v-if="isPasswordProtected" class="insp-val" :title="`Password: ${file.password}`">Password</span>
      <span v-else-if="isUnlisted" class="insp-val">Unlisted</span>
      <span v-else class="insp-val">Public</span>
    </div>

    <div v-if="file.tags.length" class="insp-kv">
      <span class="insp-key">Tags</span>
      <span class="insp-val tags-list">
        <span
          v-for="(tag, idx) in file.tags"
          :key="tag"
          class="tag-pill"
          title="Right-click for options"
          @contextmenu="showTagMenu(tag, $event)"
        >
          {{ tag }}<span v-if="idx < file.tags.length - 1" class="tag-sep">,&nbsp;</span>
        </span>
      </span>
    </div>
    <div v-if="suggestedTags.length > 0" class="insp-kv suggested-tags-row">
      <span class="insp-key">Suggest</span>
      <div class="insp-val suggested-tags">
        <button
          v-for="tag in suggestedTags"
          :key="tag"
          class="tag-chip"
          @click="$emit('add-tag', tag)"
          @contextmenu="showTagMenu(tag, $event)"
          :disabled="addingTag"
          :title="`Click to add '${tag}' to frontmatter — right-click for more`"
        >
          <span class="tag-plus">+</span>
          {{ tag }}
          <span class="tag-count">{{ availableTags[tag] }}</span>
        </button>
      </div>
    </div>

    <!-- The parent's rows (media, analytics, backlinks) go here so the
         disclosure below stays the last thing in Details. -->
    <slot />

    <!-- The rarely-needed rows, behind a disclosure. -->
    <button class="info-more" :aria-expanded="metadataExpanded" @click="$emit('toggle-metadata')">
      <PhCaretDown :size="9" weight="bold" class="caret" :class="{ collapsed: !metadataExpanded }" />
      {{ metadataExpanded ? 'Fewer details' : 'More details' }}
    </button>
    <template v-if="metadataExpanded">
      <div class="insp-kv">
        <span class="insp-key">Created</span>
        <span class="insp-val is-muted">{{ formatAgeShort(file.created) }}</span>
      </div>
      <div class="insp-kv">
        <span class="insp-key">Edited</span>
        <span class="insp-val" :class="hasUnpublishedChanges ? 'is-attention' : 'is-muted'">
          {{ formatAgeShort(file.modified) }}
        </span>
      </div>
      <div v-if="file.published_date" class="insp-kv">
        <span class="insp-key">Published</span>
        <span class="insp-val is-muted">{{ formatAgeShort(file.published_date) }}</span>
      </div>
      <div class="insp-kv">
        <span class="insp-key">Obsidian</span>
        <span class="insp-val" :class="obsidianConnected ? 'is-muted' : 'is-attention'">
          {{ obsidianConnected ? 'connected' : 'not connected' }}
        </span>
      </div>
      <div class="insp-kv">
        <span class="insp-key">Git</span>
        <span v-if="!gitStatus" class="insp-val is-muted">checking…</span>
        <span v-else-if="gitStatus.ok" class="insp-val is-mono is-muted">{{ gitStatus.branch }}</span>
        <span v-else class="insp-val is-attention" :title="gitStatus.error || ''">{{ gitStatus.error }}</span>
      </div>
    </template>
  </div>
</template>

<style scoped>
.info {
  display: flex;
  flex-direction: column;
  gap: var(--space-1);
  min-width: 0;
}

.info-more {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  align-self: flex-start;
  padding: 0;
  margin: 2px 0 0;
  background: none;
  border: none;
  font: inherit;
  font-size: 10px;
  line-height: 16px;
  color: var(--text-tertiary);
  cursor: pointer;
}
.info-more:hover {
  color: var(--label-2);
}
.caret {
  transition: transform 0.15s;
}
.caret.collapsed {
  transform: rotate(-90deg);
}

.tags-list {
  color: var(--label-2);
}
.tag-pill {
  cursor: context-menu;
  border-radius: 3px;
  padding: 0 2px;
  margin: 0 -2px;
  transition: background 0.1s;
}
.tag-pill:hover {
  background: var(--fill);
}
.tag-sep {
  cursor: default;
}
.tag-sep:hover {
  background: none;
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
  padding: 1px 7px;
  font-size: 10px;
  font-weight: 500;
  background: var(--fill);
  border: 1px solid var(--separator);
  border-radius: 10px;
  color: var(--label-2);
  cursor: pointer;
  transition: background 0.12s ease, color 0.12s ease, border-color 0.12s ease;
}
.tag-chip:hover {
  /* No scale bump on hover — macOS chips darken/lighten, not grow. */
  background: var(--fill-2);
  border-color: var(--border-light);
  color: var(--label);
}
.tag-chip:active {
  background: var(--fill-2);
}
.tag-chip:disabled {
  opacity: 0.5;
  cursor: wait;
}
.tag-plus {
  font-weight: 600;
  color: var(--ink-live);
}
.tag-count {
  font-size: 8px;
  opacity: 0.5;
  font-weight: 400;
}
</style>
