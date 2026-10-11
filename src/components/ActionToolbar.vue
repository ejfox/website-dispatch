<script setup lang="ts">
/**
 * The inspector's action block, top to bottom:
 *
 *   [ Update live post… ]            the ONE accent button, full width
 *   Open in   [ ✎ | ✎ | ▶ ]           icon-only, labels in tooltips
 *   Share     [ ↗ | 🔗▾ | ⇪ ]          live posts only
 *   [ ⋯ More ]                        native menu: Convert to Vue Page… ·
 *                                     Unpublish… (live) · Publish later… ·
 *                                     Unlisted… (not live)
 *
 * For a post that isn't live the primary is Publish… / See what to fix….
 */
import { computed } from 'vue'
import { Menu, MenuItem, PredefinedMenuItem } from '@tauri-apps/api/menu'
import { LogicalPosition } from '@tauri-apps/api/dpi'
import {
  PhNotePencil,
  PhPlay,
  PhArrowSquareOut,
  PhLink,
  PhArrowSquareUpRight,
  PhDotsThree,
} from '@phosphor-icons/vue'
import type { EditorConfig, PublishTarget } from '../types'
import { SITE_PREVIEW_HINT } from '../utils/vocab'
import UiButton from './ui/UiButton.vue'
import UiButtonGroup from './ui/UiButtonGroup.vue'

const props = defineProps<{
  enabledEditors: EditorConfig[]
  publishTargets: PublishTarget[]
  hasMultipleTargets: boolean
  selectedTargetId: string | null
  isLive: boolean
  liveUrl: string | null
  isVuePage: boolean
  converting: boolean
  unpublishing: boolean
  publishing: boolean
  isSafe: boolean
  isScheduled: boolean
  isUnlisted: boolean
}>()

const emit = defineEmits<{
  'open-obsidian': []
  'open-editor': [appName: string]
  'open-preview': []
  'select-target': [id: string]
  'show-syndication': []
  'copy-share-link': [source: string, medium: string]
  'convert-to-vue-page': []
  unpublish: []
  'open-publish-confirm': [isRepublish: boolean]
  'publish-unlisted': []
  'toggle-schedule': []
}>()

/** Pop a native menu just under the button that was clicked. */
async function popupBelow(e: MouseEvent, menu: Menu) {
  const el = (e.currentTarget as HTMLElement | null) ?? null
  if (!el) return menu.popup()
  const r = el.getBoundingClientRect()
  return menu.popup(new LogicalPosition(r.left, r.bottom + 4))
}

async function showCopyLinkMenu(e: MouseEvent) {
  const menu = await Menu.new({
    items: [
      await MenuItem.new({ text: 'Copy link for X', action: () => emit('copy-share-link', 'x', 'social') }),
      await MenuItem.new({
        text: 'Copy link for newsletter',
        action: () => emit('copy-share-link', 'newsletter', 'email'),
      }),
    ],
  })
  await popupBelow(e, menu)
}

// Rare and destructive actions live here, off the main rows. For a post
// that isn't live, the publish alternates (later / unlisted) live here too.
const canPublishLater = computed(() => !props.isLive && props.isSafe && !props.isScheduled)
const canPublishUnlisted = computed(() => !props.isLive && props.isSafe && !props.isUnlisted && !props.publishing)
const hasMore = computed(
  () => !props.isVuePage || props.isLive || canPublishLater.value || canPublishUnlisted.value,
)

async function showMoreMenu(e: MouseEvent) {
  const items: (MenuItem | PredefinedMenuItem)[] = []
  if (canPublishLater.value) {
    items.push(await MenuItem.new({ text: 'Publish later…', action: () => emit('toggle-schedule') }))
  }
  if (canPublishUnlisted.value) {
    items.push(await MenuItem.new({ text: 'Publish as Unlisted…', action: () => emit('publish-unlisted') }))
  }
  if (!props.isVuePage) {
    if (items.length) items.push(await PredefinedMenuItem.new({ item: 'Separator' }))
    items.push(
      await MenuItem.new({
        text: props.converting ? 'Converting…' : 'Convert to Vue Page…',
        enabled: !props.converting,
        action: () => emit('convert-to-vue-page'),
      }),
    )
  }
  if (props.isLive) {
    if (items.length) items.push(await PredefinedMenuItem.new({ item: 'Separator' }))
    items.push(
      await MenuItem.new({
        text: props.unpublishing ? 'Unpublishing…' : 'Unpublish…',
        enabled: !props.unpublishing,
        action: () => emit('unpublish'),
      }),
    )
  }
  const menu = await Menu.new({ items })
  await popupBelow(e, menu)
}
</script>

<template>
  <div class="actions">
    <select
      v-if="hasMultipleTargets"
      class="target-select"
      aria-label="Publish target"
      :value="selectedTargetId || publishTargets.find((t) => t.is_default)?.id"
      @change="emit('select-target', ($event.target as HTMLSelectElement).value)"
    >
      <option v-for="t in publishTargets" :key="t.id" :value="t.id">
        {{ t.name }}
      </option>
    </select>

    <!-- Primary: the one accent button on the screen. -->
    <UiButton
      v-if="isLive"
      variant="primary"
      class="full"
      :disabled="publishing"
      :label="publishing ? 'Publishing…' : 'Update live post…'"
      @click="emit('open-publish-confirm', true)"
    />
    <UiButton v-else variant="primary" class="full" :disabled="publishing" @click="emit('open-publish-confirm', false)">
      {{ publishing ? 'Publishing…' : isSafe ? 'Publish…' : 'See what to fix…' }}
      <kbd v-if="isSafe && !publishing">⌘&crarr;</kbd>
    </UiButton>

    <!-- Open in -->
    <div class="insp-row">
      <span class="insp-label">Open in</span>
      <UiButtonGroup label="Open in">
        <UiButton
          v-for="editor in enabledEditors"
          :key="editor.app_name"
          size="small"
          :icon="PhNotePencil"
          :aria-label="`Open in ${editor.name}`"
          @click="editor.app_name === 'Obsidian' ? emit('open-obsidian') : emit('open-editor', editor.app_name)"
        />
        <UiButton
          size="small"
          :icon="PhPlay"
          aria-label="Open site preview"
          :title="SITE_PREVIEW_HINT"
          @click="emit('open-preview')"
        />
      </UiButtonGroup>
    </div>

    <!-- Share -->
    <div v-if="isLive && liveUrl" class="insp-row">
      <span class="insp-label">Share</span>
      <UiButtonGroup label="Share">
        <UiButton size="small" :href="liveUrl" :icon="PhArrowSquareOut" aria-label="View on site" />
        <UiButton
          size="small"
          :icon="PhLink"
          menu
          aria-label="Copy link"
          title="Tagged link for X or the newsletter"
          @click="showCopyLinkMenu"
        />
        <UiButton
          size="small"
          :icon="PhArrowSquareUpRight"
          aria-label="Syndicate…"
          title="Share to social platforms"
          @click="emit('show-syndication')"
        />
      </UiButtonGroup>
    </div>

    <!-- More -->
    <UiButton
      v-if="hasMore"
      variant="plain"
      size="small"
      class="full more"
      :icon="PhDotsThree"
      label="More"
      menu
      @click="showMoreMenu"
    />
  </div>
</template>

<style scoped>
.actions {
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
  min-width: 0;
}

/* Full-width rows. (Not UiButton's `block`: that is flex:1, which in a
   column would grow the button vertically.) */
.actions > .full {
  width: 100%;
}
.actions > .more {
  justify-content: flex-start;
  /* Flush with the section's left edge like the row labels. */
  margin-left: calc(-1 * var(--space-2));
  width: calc(100% + var(--space-2));
}

/* Icon groups keep their natural width and sit at the right edge. */
.insp-row > .ui-btn-group {
  flex-shrink: 0;
}

.target-select {
  height: 28px;
  width: 100%;
  padding: 0 var(--space-2);
  font-size: 11px;
  color: var(--label);
  background: transparent;
  border: 1px solid var(--separator);
  border-radius: 6px;
}
</style>
