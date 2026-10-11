<script setup lang="ts">
/**
 * The Preview tab's action area, grouped by job:
 *
 *   row 1  Open in     [ Obsidian | iA Writer | Site preview ]      joined, small
 *   row 2  Share       [ View on site | Copy link ▾ | Syndicate… ]  joined
 *          More        [ ⋯ ]  native menu: Convert to Vue Page… · Unpublish…
 *          Primary     [ Update live post… ]   the ONE accent button
 *
 * For a post that isn't live yet, row 2 is the publish alternates
 * ([Publish later…] [Unlisted…]) and the primary is Publish… / See what to fix….
 * Row 2 wraps at narrow widths; the primary keeps the right edge.
 */
import { Command } from 'lucide-vue-next'
import { Menu, MenuItem, PredefinedMenuItem } from '@tauri-apps/api/menu'
import { LogicalPosition } from '@tauri-apps/api/dpi'
import {
  PhNotePencil,
  PhPlay,
  PhArrowSquareOut,
  PhLink,
  PhArrowSquareUpRight,
  PhDotsThree,
  PhClock,
  PhEyeSlash,
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

// Rare and destructive actions live here, off the main row.
async function showMoreMenu(e: MouseEvent) {
  const items: (MenuItem | PredefinedMenuItem)[] = []
  if (!props.isVuePage) {
    items.push(
      await MenuItem.new({
        text: props.converting ? 'Converting…' : 'Convert to Vue Page…',
        enabled: !props.converting,
        action: () => emit('convert-to-vue-page'),
      }),
    )
  } else {
    items.push(await MenuItem.new({ text: 'Custom Vue page', enabled: false, action: () => {} }))
  }
  items.push(
    await PredefinedMenuItem.new({ item: 'Separator' }),
    await MenuItem.new({
      text: props.unpublishing ? 'Unpublishing…' : 'Unpublish…',
      enabled: !props.unpublishing,
      action: () => emit('unpublish'),
    }),
  )
  const menu = await Menu.new({ items })
  await popupBelow(e, menu)
}
</script>

<template>
  <div class="toolbar">
    <!-- Open in -->
    <UiButtonGroup label="Open in" block>
      <UiButton
        v-for="editor in enabledEditors"
        :key="editor.app_name"
        size="small"
        :icon="PhNotePencil"
        :label="editor.name"
        :data-tip="`Open in ${editor.name}`"
        @click="editor.app_name === 'Obsidian' ? emit('open-obsidian') : emit('open-editor', editor.app_name)"
      />
      <UiButton size="small" :icon="PhPlay" label="Site preview" :data-tip="SITE_PREVIEW_HINT" @click="emit('open-preview')" />
    </UiButtonGroup>

    <div class="toolbar-row">
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

      <template v-if="isLive">
        <!-- Share -->
        <UiButtonGroup label="Share">
          <UiButton :href="liveUrl!" :icon="PhArrowSquareOut" label="View on site" />
          <UiButton :icon="PhLink" label="Copy link" menu data-tip="Tagged link for X or the newsletter" @click="showCopyLinkMenu" />
          <UiButton :icon="PhArrowSquareUpRight" label="Syndicate…" data-tip="Share to social platforms" @click="emit('show-syndication')" />
        </UiButtonGroup>
        <!-- More -->
        <UiButton :icon="PhDotsThree" aria-label="More actions" menu data-tip="Convert to Vue Page · Unpublish" @click="showMoreMenu" />
        <!-- Primary -->
        <UiButton
          variant="primary"
          class="primary-action"
          :disabled="publishing"
          :label="publishing ? 'Publishing…' : 'Update live post…'"
          @click="emit('open-publish-confirm', true)"
        />
      </template>

      <template v-else>
        <UiButton
          v-if="isSafe && !isScheduled"
          :icon="PhClock"
          label="Publish later…"
          data-tip="Dispatch publishes it at the time you pick (keep Dispatch running)"
          @click="emit('toggle-schedule')"
        />
        <UiButton
          v-if="isSafe && !isUnlisted && !publishing"
          :icon="PhEyeSlash"
          label="Unlisted…"
          data-tip="Publish without listing it anywhere"
          @click="emit('publish-unlisted')"
        />
        <UiButton
          variant="primary"
          class="primary-action"
          :disabled="publishing"
          @click="emit('open-publish-confirm', false)"
        >
          {{ publishing ? 'Publishing…' : isSafe ? 'Publish…' : 'See what to fix…' }}
          <kbd v-if="isSafe && !publishing"><Command :size="10" />&crarr;</kbd>
        </UiButton>
      </template>
    </div>
  </div>
</template>

<style scoped>
.toolbar {
  display: flex;
  flex-direction: column;
  gap: var(--space-2);
  padding: var(--space-2) var(--gutter);
  border-bottom: 1px solid var(--separator);
}

.toolbar-row {
  display: flex;
  align-items: center;
  flex-wrap: wrap;
  gap: var(--space-2);
}

/* The primary holds the right edge; when the row wraps it moves to the
   next line and still sits right. */
.primary-action {
  margin-left: auto;
}

.target-select {
  height: 28px;
  padding: 0 var(--space-2);
  font-size: 11px;
  color: var(--label);
  background: transparent;
  border: 1px solid var(--separator);
  border-radius: 6px;
}
</style>
