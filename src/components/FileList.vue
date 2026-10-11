<script setup lang="ts">
import { parseEpochSeconds } from '../utils/formatting'
import { ref, computed, watch } from 'vue'
import { perfTrace } from '../utils/perfTrace'
import { useOverflowMenu } from '../composables/useOverflowMenu'
import { useDeployStatus } from '../composables/useUiState'
import { invoke } from '@tauri-apps/api/core'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { useLocalStorage } from '@vueuse/core'
import { Menu, MenuItem, PredefinedMenuItem } from '@tauri-apps/api/menu'
import {
  PhLockSimple,
  PhEye,
  PhClock,
  PhCheckCircle,
  PhFolderOpen,
  PhPencilSimple,
  PhTextAa,
  PhCalendarBlank,
  PhTextAlignLeft,
  PhBroadcast,
  PhClockCounterClockwise,
} from '@phosphor-icons/vue'
import type { MarkdownFile } from '../types'
import { useAppConfig } from '../composables/useVaultData'
import { CHANGED_LABEL, CHANGED_PILL, CHANGED_HINT, NOT_PUBLISHED_LABEL } from '../utils/vocab'

const props = defineProps<{
  files: MarkdownFile[]
  selected: MarkdownFile | null
  loading: boolean
  compact?: boolean
}>()

const emit = defineEmits<{
  select: [file: MarkdownFile]
  'request-unpublish': [file: MarkdownFile]
  'open-settings': []
}>()

const { appConfig, defaultEditor } = useAppConfig()
const vaultPath = computed(() => appConfig.value?.vault.path || '')
const vaultPathShort = computed(() => vaultPath.value.replace(/^\/Users\/[^/]+/, '~'))
const vaultDirs = computed(() => (appConfig.value?.vault.publishable_dirs || ['blog', 'dispatch']).join('/ and ') + '/')

// Start the click→paint timer at the true origin — the moment the row is
// clicked, before emit/select/watchers/re-render run. FilePreview's
// loadFileContent adds the downstream marks and flushes on paint.
function onRowClick(file: MarkdownFile) {
  perfTrace.begin(file.filename || file.path)
  emit('select', file)
}

// Request a small, cropped variant for list thumbnails. For Cloudinary URLs we
// inject a transform after `/upload/` so we fetch an ~80px square instead of the
// full-resolution image; anything else is returned untouched.
function thumbSrc(url: string): string {
  if (url.includes('res.cloudinary.com') && url.includes('/upload/')) {
    return url.replace('/upload/', '/upload/w_80,h_80,c_fill,q_auto,f_auto/')
  }
  return url
}

const { phaseFor } = useDeployStatus()

const filter = ref<'all' | 'published' | 'drafts' | 'scheduled' | 'photos'>('all')
const sort = ref<'recent' | 'created' | 'title' | 'words'>('recent')
const showWeeknotes = useLocalStorage('dispatch-show-weeknotes', true)

const filteredFiles = computed(() => {
  let result = props.files

  // Filter
  if (filter.value === 'published') {
    result = result.filter((f) => f.published_url)
  } else if (filter.value === 'drafts') {
    result = result.filter((f) => !f.published_url)
  } else if (filter.value === 'scheduled') {
    result = result.filter((f) => f.publish_at && !f.published_url)
  } else if (filter.value === 'photos') {
    result = result.filter((f) => f.content_type === 'photos')
  }

  // Hide weeknotes if toggled off
  if (!showWeeknotes.value) {
    result = result.filter((f) => f.content_type !== 'weeknote')
  }

  // Sort
  result = [...result].sort((a, b) => {
    switch (sort.value) {
      case 'recent':
        // Use published_date if available, otherwise modified
        const aRecent = a.published_date || a.modified
        const bRecent = b.published_date || b.modified
        return bRecent - aRecent
      case 'created':
        // An unparseable date falls back to created; NaN would scramble the sort.
        const aDate = parseEpochSeconds(a.date) ?? a.created
        const bDate = parseEpochSeconds(b.date) ?? b.created
        return bDate - aDate
      case 'title':
        return formatTitle(a).localeCompare(formatTitle(b))
      case 'words':
        return b.word_count - a.word_count
      default:
        return 0
    }
  })

  return result
})

// NSToolbar-style overflow for the .filters row. When the sidebar gets too
// narrow, filter buttons that don't fit are hidden and a `»` chevron pops a
// native Tauri menu containing them.
const filtersRef = ref<HTMLElement | null>(null)
const filterItemRefs = ref<(HTMLElement | null)[]>([])
const { hiddenIndices: hiddenFilterIndices, remeasure: remeasureFilters } =
  useOverflowMenu(filtersRef, filterItemRefs)

// The set of filter items is static today (all/published/drafts) plus an
// optional week-toggle that appears once any week-notes exist — re-measure
// when that toggle's existence flips.
watch(
  () => filterItemRefs.value.filter(Boolean).length,
  () => remeasureFilters(),
)

interface FilterDef {
  key: 'all' | 'published' | 'drafts' | 'week' | 'photos'
  label: string
  active: boolean
  onPick: () => void
}

const filterDefs = computed<FilterDef[]>(() => {
  const defs: FilterDef[] = [
    { key: 'all', label: `All ${counts.value.all}`, active: filter.value === 'all', onPick: () => (filter.value = 'all') },
    { key: 'published', label: `Live ${counts.value.published}`, active: filter.value === 'published', onPick: () => (filter.value = 'published') },
    { key: 'drafts', label: `${NOT_PUBLISHED_LABEL} ${counts.value.drafts}`, active: filter.value === 'drafts', onPick: () => (filter.value = 'drafts') },
  ]
  if (counts.value.photos > 0) {
    defs.push({
      key: 'photos',
      label: `PHOTO ${counts.value.photos}`,
      active: filter.value === 'photos',
      // Toggle: clicking the active PHOTO pill clears back to All.
      onPick: () => (filter.value = filter.value === 'photos' ? 'all' : 'photos'),
    })
  }
  if (counts.value.weeknotes > 0) {
    defs.push({
      key: 'week',
      label: `WEEK ${counts.value.weeknotes}${showWeeknotes.value ? '' : ' (hidden)'}`,
      active: showWeeknotes.value,
      onPick: () => (showWeeknotes.value = !showWeeknotes.value),
    })
  }
  return defs
})

// Pop a native menu listing the items that didn't fit on the bar.
async function showOverflowMenu() {
  const items = await Promise.all(
    hiddenFilterIndices.value.map(async (i) => {
      const def = filterDefs.value[i]
      return await MenuItem.new({
        // Active state shown with a trailing dot; native menus don't expose
        // the "checked" state from JS in Tauri 2, so a textual cue is it.
        text: def.label + (def.active ? '  •' : ''),
        action: def.onPick,
      })
    }),
  )
  if (!items.length) return
  const menu = await Menu.new({ items })
  await menu.popup()
}

const counts = computed(() => {
  // "All / Live / Drafts" counts respect the weeknote toggle, so the numbers
  // match what the user is actually seeing.
  const base = showWeeknotes.value
    ? props.files
    : props.files.filter((f) => f.content_type !== 'weeknote')
  return {
    all: base.length,
    published: base.filter((f) => f.published_url).length,
    drafts: base.filter((f) => !f.published_url).length,
    scheduled: base.filter((f) => f.publish_at && !f.published_url).length,
    weeknotes: props.files.filter((f) => f.content_type === 'weeknote').length,
    photos: props.files.filter((f) => f.content_type === 'photos').length,
  }
})

// Group files by time period (only for 'recent' sort)
const groupedFiles = computed(() => {
  if (sort.value !== 'recent') {
    return [{ label: null, files: filteredFiles.value }]
  }

  const now = Date.now() / 1000
  const dayAgo = now - 86400
  const weekAgo = now - 86400 * 7
  const monthAgo = now - 86400 * 30

  const groups: { label: string | null; files: MarkdownFile[] }[] = []

  const today: MarkdownFile[] = []
  const thisWeek: MarkdownFile[] = []
  const thisMonth: MarkdownFile[] = []
  const earlier: MarkdownFile[] = []

  for (const file of filteredFiles.value) {
    const ts = file.published_date || file.modified
    if (ts > dayAgo) {
      today.push(file)
    } else if (ts > weekAgo) {
      thisWeek.push(file)
    } else if (ts > monthAgo) {
      thisMonth.push(file)
    } else {
      earlier.push(file)
    }
  }

  if (today.length) groups.push({ label: 'Today', files: today })
  if (thisWeek.length) groups.push({ label: 'This Week', files: thisWeek })
  if (thisMonth.length) groups.push({ label: 'This Month', files: thisMonth })
  if (earlier.length) groups.push({ label: 'Earlier', files: earlier })

  return groups
})

/** What the empty list says when a filter hides everything. */
const emptyFilterText = computed(() => {
  switch (filter.value) {
    case 'published':
      return 'Nothing live yet.'
    case 'drafts':
      return 'Everything here is live.'
    case 'scheduled':
      return 'Nothing set to publish later.'
    case 'photos':
      return 'No photo posts.'
    default:
      return showWeeknotes.value ? 'No posts match.' : 'Only week notes here, and they’re hidden.'
  }
})

function formatTitle(file: MarkdownFile): string {
  return file.title || file.filename.replace(/\.md$/, '').replace(/-/g, ' ')
}

/** State for the leading row dot. Drafts return null so the dot doesn't
 *  render (keeps rest-state rows visually quiet — Mail's pattern).
 *  Precedence: what's happening now, then what's broken, then what needs
 *  you, then where it stands. */
type RowState =
  | 'deploying'
  | 'failed'
  | 'privacy'
  | 'modified'
  | 'live'
  | 'unlisted'
  | 'protected'
  | 'lint'
  | 'scheduled'
const MODIFIED = 'Modified since publish'
function lintWarnings(file: MarkdownFile): string[] {
  return (file.warnings ?? []).filter((w) => w !== MODIFIED)
}
function rowState(file: MarkdownFile): RowState | null {
  const deploy = phaseFor(file.path)
  if (deploy === 'deploying') return 'deploying'
  if (deploy === 'failed') return 'failed'
  const lint = lintWarnings(file)
  if (lint.some((w) => w.startsWith('[privacy]'))) return 'privacy'
  if (file.published_url && file.warnings?.includes(MODIFIED)) return 'modified'
  if (file.published_url) {
    if (file.password) return 'protected'
    if (file.unlisted) return 'unlisted'
    return 'live'
  }
  // `draft: true` files are parked on purpose — their lint stays quiet.
  if (lint.length && !file.draft) return 'lint'
  if (file.publish_at) return 'scheduled'
  return null
}
/** Status color for a row state. Visibility (unlisted/password) is not a
 *  status — those rows are live, and say so with a neutral glyph. */
const ROW_STATUS: Record<RowState, string> = {
  deploying: 'deploying',
  failed: 'broken',
  privacy: 'broken',
  modified: 'attention',
  lint: 'attention',
  live: 'live',
  unlisted: 'live',
  protected: 'live',
  scheduled: 'scheduled',
}
function rowStatus(file: MarkdownFile): string | null {
  const s = rowState(file)
  return s ? ROW_STATUS[s] : null
}
function rowStateLabel(file: MarkdownFile): string {
  const s = rowState(file)
  switch (s) {
    case 'deploying':
      return 'Deploying'
    case 'failed':
      return 'Deploy failed'
    case 'privacy':
      return 'Privacy warning'
    case 'lint':
      return `${lintWarnings(file).length} lint warning${lintWarnings(file).length === 1 ? '' : 's'}`
    case 'modified':
      return `${CHANGED_LABEL}: ${CHANGED_HINT.charAt(0).toLowerCase()}${CHANGED_HINT.slice(1)}`
    case 'live':
      return 'Live'
    case 'unlisted':
      return 'Live, unlisted (link only)'
    case 'protected':
      return 'Live, password-protected'
    case 'scheduled':
      return 'Publishes later: Dispatch publishes it at the set time, if running'
    default:
      return ''
  }
}

function formatAge(ts: number): string {
  const seconds = Math.floor(Date.now() / 1000 - ts)
  const minutes = Math.floor(seconds / 60)
  const hours = Math.floor(minutes / 60)
  const days = Math.floor(hours / 24)

  if (seconds < 60) return 'just now'
  if (minutes < 60) return `${minutes}m`
  if (hours < 24) return `${hours}h`
  if (days === 1) return 'yesterday'
  if (days < 7) return `${days}d`
  if (days < 30) return `${Math.floor(days / 7)}w`
  if (days < 365) return `${Math.floor(days / 30)}mo`
  return `${Math.floor(days / 365)}y`
}

function formatWordCount(count: number): string {
  if (count >= 1000) return `${(count / 1000).toFixed(1)}k`
  return `${count}`
}

async function showGroupContextMenu(group: { label: string | null; files: MarkdownFile[] }, e: MouseEvent) {
  e.preventDefault()
  if (!group.files.length) return
  const titles = group.files.map((f) => formatTitle(f)).join('\n')
  const paths = group.files.map((f) => f.path).join('\n')
  const live = group.files.filter((f) => f.published_url)
  const liveUrls = live.map((f) => f.published_url!).join('\n')
  const n = group.files.length
  const items_: (MenuItem | PredefinedMenuItem)[] = [
    await MenuItem.new({
      text: `Copy ${n} Title${n === 1 ? '' : 's'}`,
      action: () => navigator.clipboard.writeText(titles),
    }),
    await MenuItem.new({
      text: `Copy ${n} Path${n === 1 ? '' : 's'}`,
      action: () => navigator.clipboard.writeText(paths),
    }),
  ]
  if (live.length) {
    items_.push(
      await PredefinedMenuItem.new({ item: 'Separator' }),
      await MenuItem.new({
        text: `Copy ${live.length} Live Link${live.length === 1 ? '' : 's'}`,
        action: () => navigator.clipboard.writeText(liveUrls),
      }),
    )
  }
  const menu = await Menu.new({ items: items_ })
  await menu.popup()
}

async function showContextMenu(file: MarkdownFile, e: MouseEvent) {
  e.preventDefault()
  // Select the file first so right-click context matches what the user
  // clicked, even if a different file was previously selected.
  emit('select', file)

  const isLive = !!file.published_url
  const items = [
    await MenuItem.new({ text: 'Open in Obsidian', action: () => invoke('open_in_obsidian', { path: file.path }) }),
    await MenuItem.new({
      text: `Open in ${defaultEditor.value}`,
      action: () => invoke('open_in_app', { path: file.path, app: defaultEditor.value }),
    }),
    await MenuItem.new({
      text: 'Reveal in Finder',
      action: () => invoke('open_in_app', { path: file.path, app: 'Finder' }).catch(() => {}),
    }),
    await PredefinedMenuItem.new({ item: 'Separator' }),
    await MenuItem.new({ text: 'Copy Path', action: () => navigator.clipboard.writeText(file.path) }),
  ]
  if (isLive) {
    items.push(
      await PredefinedMenuItem.new({ item: 'Separator' }),
      await MenuItem.new({ text: 'View on Site', action: () => window.open(file.published_url!, '_blank') }),
      await MenuItem.new({
        text: 'Copy Link',
        action: () => navigator.clipboard.writeText(file.published_url!),
      }),
      await MenuItem.new({
        text: 'Unpublish…',
        action: () => emit('request-unpublish', file),
      }),
    )
  }
  const menu = await Menu.new({ items })
  await menu.popup()
}

/**
 * Manual window-drag handler — required because `titleBarStyle: "Overlay"`
 * breaks the CSS `-webkit-app-region: drag` mechanism on macOS Sonoma+
 * (Tauri issues #9503 / #11605). Calling `startDragging()` from a mousedown
 * listener is the documented workaround. Bail out on non-primary clicks
 * and on bubbled events from buttons/inputs/links so we don't hijack
 * intended clicks on the filter pills, sort row, etc.
 */
async function startWindowDrag(e: MouseEvent) {
  if (e.button !== 0) return
  const target = e.target as HTMLElement
  if (target.closest('button, input, a, select, [data-no-drag]')) return
  try {
    await getCurrentWindow().startDragging()
  } catch (err) {
    console.warn('startDragging failed', err)
  }
}
</script>

<template>
  <aside class="sidebar" :class="{ compact }">
    <!-- `@mousedown="startWindowDrag"` is the real drag handle — see the
         function comment for why CSS-region drag is broken under
         `titleBarStyle: "Overlay"`. -->
    <div class="control-bar" data-tauri-drag-region @mousedown="startWindowDrag">
      <div ref="filtersRef" class="filters">
        <button
          v-for="(def, i) in filterDefs"
          :key="def.key"
          :ref="(el) => (filterItemRefs[i] = el as HTMLElement | null)"
          :class="{
            active: def.active,
            'week-toggle': def.key === 'week',
            off: def.key === 'week' && !showWeeknotes,
            overflowed: hiddenFilterIndices.includes(i),
          }"
          @click="def.onPick"
        >
          {{ def.label }}
        </button>
        <button
          v-if="hiddenFilterIndices.length"
          class="filter-overflow"
          data-tip="Show filters that didn’t fit"
          aria-label="More filters"
          @click="showOverflowMenu"
        >»</button>
      </div>
      <div class="sort-row">
        <button :class="{ active: sort === 'recent' }" @click="sort = 'recent'" data-tip="Sort by most recent" aria-label="Sort by most recent">
          <PhClockCounterClockwise :size="9" weight="bold" />
        </button>
        <button :class="{ active: sort === 'created' }" @click="sort = 'created'" data-tip="Sort by date created" aria-label="Sort by date created">
          <PhCalendarBlank :size="9" weight="bold" />
        </button>
        <button :class="{ active: sort === 'title' }" @click="sort = 'title'" data-tip="Sort by title" aria-label="Sort by title">
          <PhTextAlignLeft :size="9" weight="bold" />
        </button>
      </div>
    </div>

    <div v-if="loading" class="loading">Loading...</div>

    <!-- Nothing scanned at all: no vault, or a vault with nothing to publish. -->
    <div v-else-if="files.length === 0" class="list-empty">
      <template v-if="!vaultPath">
        <div class="list-empty-title">No vault yet</div>
        <div class="list-empty-sub">Point Dispatch at your Obsidian vault folder and your posts show up here.</div>
      </template>
      <template v-else>
        <div class="list-empty-title">No posts found</div>
        <div class="list-empty-sub">
          Dispatch lists notes in {{ vaultDirs }} inside
          <code>{{ vaultPathShort }}</code>. Wrong folder?
        </div>
      </template>
      <button class="list-empty-btn" @click="emit('open-settings')">Choose vault folder…</button>
    </div>

    <!-- A filter hid everything. -->
    <div v-else-if="filteredFiles.length === 0" class="list-empty">
      <div class="list-empty-sub">{{ emptyFilterText }}</div>
      <button class="list-empty-btn" @click="filter = 'all'; showWeeknotes = true">Show all</button>
    </div>

    <div v-else class="list">
      <template v-for="group in groupedFiles" :key="group.label">
        <div
          v-if="group.label"
          class="group-header"
          @contextmenu="showGroupContextMenu(group, $event)"
        >
          {{ group.label }}
          <span class="group-count">{{ group.files.length }}</span>
        </div>
        <button
          v-for="file in group.files"
          :key="file.path"
          v-memo="[
            file.path,
            file.modified,
            file.published_url,
            file.published_word_count,
            file.word_count,
            file.warnings.length,
            file.draft,
            phaseFor(file.path),
            selected?.path === file.path,
          ]"
          class="item"
          :class="{
            selected: selected?.path === file.path,
            published: !!file.published_url,
            weeknote: file.content_type === 'weeknote',
            photos: file.content_type === 'photos',
          }"
          @click="onRowClick(file)"
          @contextmenu="showContextMenu(file, $event)"
        >
          <img
            v-if="file.content_type === 'photos' && file.thumbnail"
            class="row-thumb"
            :src="thumbSrc(file.thumbnail)"
            loading="lazy"
            alt=""
          />
          <div class="content">
            <div class="row">
              <!-- Leading status dot — Mail's "blue unread dot" idiom, but
                   color-coded by visibility/state. Drafts show no dot so
                   they read as quiet. -->
              <span
                v-if="rowState(file)"
                class="status-dot row-dot"
                :data-state="rowState(file)"
                :data-status="rowStatus(file)"
                :data-tip="rowStateLabel(file)"
                aria-hidden="true"
              ></span>
              <span v-if="file.content_type === 'weeknote'" class="weeknote-badge">WEEK</span>
              <span v-if="file.content_type === 'photos'" class="photo-badge">PHOTO</span>
              <span
                v-if="file.content_type === 'dispatch'"
                class="dispatch-badge"
                data-tip="Dispatch piece → ejfox.com/dispatch/"
                >DISPATCH</span
              >
              <span
                v-if="file.draft && !file.published_url"
                class="draft-badge"
                data-tip="draft: true — publishing asks to clear it first"
                >DRAFT</span
              >
              <span
                v-if="file.password && !file.published_url"
                class="protected-badge-draft"
                data-tip="Will be password-protected"
              >
                <PhLockSimple :size="10" weight="fill" />
              </span>
              <span
                v-else-if="file.unlisted && !file.published_url"
                class="unlisted-badge-draft"
                data-tip="Will be unlisted"
              >
                <PhEye :size="10" weight="fill" />
              </span>
              <span class="title" :title="formatTitle(file)">{{ formatTitle(file) }}</span>
              <span
                v-if="file.published_url && file.warnings.includes('Modified since publish')"
                class="modified-badge"
                :data-tip="
                  file.published_word_count != null
                    ? `${CHANGED_HINT} · was ${file.published_word_count.toLocaleString()}w, now ${file.word_count.toLocaleString()}w`
                    : CHANGED_HINT
                "
              >
                {{ CHANGED_PILL }}
                <span
                  v-if="file.published_word_count != null"
                  class="modified-delta"
                  :class="{ pos: file.word_count > file.published_word_count, neg: file.word_count < file.published_word_count }"
                >
                  {{ file.word_count >= file.published_word_count ? '+' : ''
                  }}{{ (file.word_count - file.published_word_count).toLocaleString() }}w
                </span>
              </span>
              <span v-else-if="file.published_url && file.password" class="protected-badge">
                <PhLockSimple :size="9" weight="bold" />
                PROTECTED
              </span>
              <span v-else-if="file.published_url && file.unlisted" class="unlisted-badge">
                <PhEye :size="9" weight="bold" />
                UNLISTED
              </span>
              <span v-else-if="file.published_url" class="live-badge">
                <PhCheckCircle :size="9" weight="fill" />
                LIVE
              </span>
              <span v-else-if="file.publish_at" class="scheduled-badge">
                <PhClock :size="9" weight="bold" />
                PUBLISH LATER
              </span>
            </div>
            <div v-if="file.dek" class="dek">{{ file.dek }}</div>
            <div class="filename">{{ file.source_dir ? file.source_dir + '/' : '' }}{{ file.filename }}</div>
            <div class="meta">
              <span v-if="file.source_dir" class="dir">
                <PhFolderOpen :size="8" weight="duotone" />
                {{ file.source_dir }}/
              </span>
              <template v-if="file.published_date">
                <span class="pub-date">
                  <PhBroadcast :size="8" weight="fill" />
                  {{ formatAge(file.published_date) }}
                </span>
                <span v-if="file.warnings.includes('Modified since publish')" class="edit-date">
                  <PhPencilSimple :size="8" weight="fill" />
                  {{ formatAge(file.modified) }}
                </span>
              </template>
              <template v-else>
                <span class="create-date">
                  <PhCalendarBlank :size="8" weight="duotone" />
                  {{ formatAge(file.created) }}
                </span>
                <span class="edit-date">
                  <PhPencilSimple :size="8" weight="duotone" />
                  {{ formatAge(file.modified) }}
                </span>
              </template>
              <span class="word-count">
                <PhTextAa :size="8" weight="duotone" />
                {{ formatWordCount(file.word_count) }}w
              </span>
            </div>
          </div>
        </button>
      </template>
    </div>
  </aside>
</template>

<style scoped>
.sidebar {
  /* Span both sidebar grid rows (bar + content) */
  grid-area: sidebar-bar / sidebar-bar / sidebar / sidebar;
  border-right: 1px solid var(--border);
  /* Translucent — lets the macOS `sidebar` NSVisualEffectView material
     (declared in tauri.conf.json#windowEffects) bleed through. This is the
     single biggest "native Mac sidebar" cue. The thin overlay tones the
     vibrancy down to dark-mode-appropriate, without painting it out. */
  background: color-mix(in srgb, var(--bg-solid) 32%, transparent);
  display: flex;
  flex-direction: column;
  overflow: hidden;
}

/* Titlebar-aligned control bar — left half of the window-drag region.
   The drag-eligible space is: (a) the strip above the bottom-aligned
   filter buttons, and (b) any horizontal whitespace between the traffic
   lights and the filter buttons. Bumping `padding-top` to 18px gives a
   fatter grab strip that survives any responsive layout. */
.control-bar {
  display: flex;
  align-items: center;
  background: transparent;
  flex-shrink: 0;
  border-bottom: 1px solid var(--border);
  /* Traffic lights now live in the unified titlebar above, so the old
     78px keepout is gone. Standard 10px sidebar padding. */
  padding: 0 10px;
  height: 32px;
  gap: 6px;
}

.filters {
  display: flex;
  flex: 1;
  /* `overflow: hidden` keeps overflowing items from painting past the bar
     while useOverflowMenu re-measures. Items get `flex-shrink: 0` so they
     keep their natural width instead of silently shrinking — the composable
     hides ones that don't fit and surfaces them via the » chevron. */
  overflow: hidden;
  min-width: 0;
}

.filters button {
  flex-shrink: 0;
  padding: 5px 8px;
  font-size: 9px;
  font-weight: 500;
  background: transparent;
  border: none;
  color: var(--text-tertiary);
  cursor: pointer;
  transition: all var(--transition-fast, 0.15s cubic-bezier(0.34, 1.56, 0.64, 1));
  display: inline-flex;
  align-items: center;
  gap: 3px;
  font-variant-numeric: tabular-nums;
  -webkit-app-region: no-drag;
}

.filters button.overflowed {
  display: none;
}

/* NSToolbar-style overflow chevron — pops a native menu of hidden filters. */
.filters .filter-overflow {
  margin-left: auto;
  padding: 5px 6px;
  color: var(--text-tertiary);
  font-size: 11px;
  line-height: 1;
}
.filters .filter-overflow:hover {
  color: var(--text-primary);
}

.filters button:hover {
  color: var(--text-secondary);
  background: color-mix(in srgb, var(--text-primary) 5%, transparent);
}

.filters button.active {
  /* Match the panel-tab pill style — a translucent accent fill instead
     of the old "just bump the text weight" treatment. Reads as a real
     selected state against the vibrant sidebar. */
  color: var(--text-primary);
  background: var(--accent-selected);
  border-radius: 4px;
  font-weight: 600;
}

.filters .week-toggle {
  margin-left: 4px;
  /* Content-type filter, not a status: neutral. */
  background: var(--fill-2);
  color: var(--label-2);
  border-radius: 8px;
  letter-spacing: 0.5px;
  font-weight: 600;
}
.filters .week-toggle:hover {
  background: var(--fill-2);
  color: var(--label);
}
.filters .week-toggle.off {
  background: transparent;
  color: var(--text-tertiary);
  text-decoration: line-through;
  text-decoration-thickness: 1px;
  opacity: 0.6;
}
.filters .week-toggle.off:hover {
  opacity: 1;
}

.sort-row {
  display: flex;
  gap: 0;
}

.sort-row button {
  padding: 5px 6px;
  background: transparent;
  border: none;
  color: var(--text-tertiary);
  cursor: pointer;
  transition: all var(--transition-fast, 0.15s cubic-bezier(0.34, 1.56, 0.64, 1));
  display: inline-flex;
  align-items: center;
  justify-content: center;
  opacity: 0.5;
  -webkit-app-region: no-drag;
}

.sort-row button:hover {
  opacity: 1;
  color: var(--text-secondary);
}

.sort-row button.active {
  opacity: 1;
  color: var(--text-primary);
}

.weeknote-toggle[data-tip]:hover::after {
  transform: translateX(0) translateY(0);
}

.list-empty {
  padding: 40px 20px;
  text-align: center;
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 8px;
}
.list-empty-title {
  font-size: 12px;
  font-weight: 600;
  color: var(--text-secondary);
}
.list-empty-sub {
  font-size: 11px;
  line-height: 1.5;
  color: var(--text-tertiary);
  max-width: 240px;
}
.list-empty-sub code {
  font-family: 'SF Mono', monospace;
  font-size: 10px;
}
.list-empty-btn {
  margin-top: 4px;
  font-size: 11px;
}

.loading {
  padding: 40px 20px;
  text-align: center;
  color: var(--text-tertiary);
  font-size: 11px;
}

.loading::after {
  content: '';
  display: inline-block;
  width: 12px;
  height: 12px;
  margin-left: 8px;
  border: 2px solid var(--border-light);
  border-top-color: var(--text-secondary);
  border-radius: 50%;
  animation: spin 0.8s linear infinite;
}

@keyframes spin {
  to {
    transform: rotate(360deg);
  }
}

.list {
  flex: 1;
  overflow-y: auto;
  scroll-behavior: smooth;
  /* Column flex so rows stretch to full width — the .item buttons size to
     content otherwise, leaving short-title rows (and their selection rect)
     narrower than the sidebar. */
  display: flex;
  flex-direction: column;
  align-items: stretch;
}

.group-header {
  padding: 5px 12px;
  font-size: 9px;
  font-weight: 600;
  text-transform: uppercase;
  letter-spacing: 0.6px;
  color: var(--text-tertiary);
  background: color-mix(in srgb, var(--bg-tertiary) 80%, var(--bg-secondary));
  /* Sticky group label — solid background, no blur needed (and blur on a
     position:sticky element causes layout-shift hitches during scroll). */
  border-bottom: 1px solid var(--border);
  display: flex;
  justify-content: space-between;
  align-items: center;
  position: sticky;
  top: 0;
  z-index: 1;
}

.group-count {
  font-weight: 400;
  opacity: 0.5;
  font-family: 'SF Mono', monospace;
  font-size: 8px;
}

.item {
  width: auto;
  display: flex;
  /* Don't shrink on the column's main axis. .item has overflow:hidden, which
     gives it an automatic min-height:0 — so inside the flex-column .list it
     would otherwise collapse to near-zero height when many rows compete for
     space. flex:none keeps each row at its content height; align-items:stretch
     on .list still gives us the full-width rows this branch was after. */
  flex: none;
  gap: 0;
  padding: 0;
  /* Inset the rounded source-list selection from the sidebar edges. The
     6px margin holds the selection rect away from the sidebar walls —
     same idiom as Mail / Finder. overflow:hidden clips the row
     thumbnail's corners to the row's border-radius. */
  margin: 0 6px;
  border-radius: 6px;
  border: none;
  background: transparent;
  cursor: pointer;
  text-align: left;
  overflow: hidden;
  transition: background 0.12s ease;
}

/* Hairline divider between rows, drawn inside the row's content (so it
   doesn't paint over the rounded selection rect). */
.item + .item {
  box-shadow: inset 0 1px 0 0 color-mix(in srgb, var(--border) 40%, transparent);
}

.item:hover {
  background: color-mix(in srgb, var(--text-primary) 6%, transparent);
}

.item:active {
  background: color-mix(in srgb, var(--text-primary) 8%, transparent);
}

/* Source-list selection — translucent accent fill with rounded corners,
   floating with margin from the sidebar edges. The macOS NSSourceList
   look. Replaces the old solid-color row + left accent stripe. */
.item.selected {
  background: var(--accent-selected);
  box-shadow: none;
}
.item.selected + .item {
  /* No top divider under the selection rect — the rounded fill should
     read as a discrete chip, not a row with a hairline above it. */
  box-shadow: none;
}

/* Old override compensated for the now-gone 2px left accent border — the
   source-list rect doesn't need a content padding adjustment. */

/* The selection fill is a translucent tint, not a solid accent — so the
   title stays --label. (Forcing #fff here was 1.5:1 in light mode.) */
.item.selected .title {
  color: var(--label);
}

.item.selected .meta,
.item.selected .dir {
  color: var(--text-tertiary);
}

.content {
  flex: 1;
  padding: 7px 10px;
  min-width: 0;
}

.row {
  display: flex;
  align-items: center;
  gap: 6px;
}

/* Leading status dot — Mail's unread-dot idiom. Color comes from the
   global .status-dot[data-status] map (live green, attention orange, broken
   red, scheduled purple, deploying = pulsing accent). Drafts get no dot. */
.row-dot {
  margin-left: 1px;
}
/* Hold the dot off the accent selection fill so it never merges into it. */
.item.selected .row-dot:not([data-status='deploying']) {
  box-shadow: 0 0 0 1.5px var(--control-bg);
}

/* Pills: one dialect. Status pills = 15% tint + colored glyph + neutral
   label text. Visibility and content-type tags = neutral fill + --label-2. */
.live-badge {
  font-size: 7.5px;
  font-weight: 600;
  background: var(--tint-live);
  color: var(--label);
  padding: 1px 5px;
  border-radius: 8px;
  flex-shrink: 0;
  display: inline-flex;
  align-items: center;
  gap: 2px;
  letter-spacing: 0.3px;
}

.live-badge svg {
  color: var(--status-live);
}

.modified-badge {
  font-size: 7.5px;
  font-weight: 600;
  background: var(--tint-attention);
  color: var(--label);
  padding: 1px 5px;
  border-radius: 8px;
  flex-shrink: 0;
  letter-spacing: 0.3px;
  display: inline-flex;
  align-items: center;
  gap: 4px;
}
.modified-delta {
  font-weight: 500;
  letter-spacing: 0;
  opacity: 0.85;
}
/* A word-count diff, not a status: neutral (green here read as "live"). */
.modified-delta { color: var(--label-2); }

.unlisted-badge {
  font-size: 7.5px;
  font-weight: 600;
  background: var(--fill-2);
  color: var(--label-2);
  padding: 1px 5px;
  border-radius: 8px;
  flex-shrink: 0;
  display: inline-flex;
  align-items: center;
  gap: 2px;
  letter-spacing: 0.3px;
}

.protected-badge {
  font-size: 7.5px;
  font-weight: 600;
  background: var(--fill-2);
  color: var(--label-2);
  padding: 1px 5px;
  border-radius: 8px;
  flex-shrink: 0;
  display: inline-flex;
  align-items: center;
  gap: 2px;
  letter-spacing: 0.3px;
}

.weeknote-badge {
  font-size: 7.5px;
  font-weight: 600;
  background: var(--fill-2);
  color: var(--label-2);
  padding: 1px 5px;
  border-radius: 8px;
  flex-shrink: 0;
  letter-spacing: 0.5px;
}

.photo-badge {
  font-size: 7.5px;
  font-weight: 600;
  background: var(--fill-2);
  color: var(--label-2);
  padding: 1px 5px;
  border-radius: 8px;
  flex-shrink: 0;
  letter-spacing: 0.5px;
}

.dispatch-badge {
  font-size: 7.5px;
  font-weight: 700;
  background: var(--fill-2);
  color: var(--label-2);
  padding: 1px 5px;
  border-radius: 8px;
  flex-shrink: 0;
  letter-spacing: 0.5px;
}

.draft-badge {
  font-size: 7.5px;
  font-weight: 600;
  border: 1px dashed var(--label-3);
  color: var(--label-2);
  padding: 0 5px;
  border-radius: 8px;
  flex-shrink: 0;
  letter-spacing: 0.5px;
}

/* List thumbnail for photo posts — small cropped square before the text
   content, so photo posts are scannable in the sidebar. */
.row-thumb {
  width: 34px;
  height: 34px;
  flex: none;
  object-fit: cover;
  border-radius: 4px;
  margin: 4px 0 4px 4px;
  background: var(--under-page);
}

.unlisted-badge-draft {
  font-size: 9px;
  flex-shrink: 0;
  padding: 1px 4px;
  border-radius: 2px;
  background: transparent;
  color: var(--label-2);
}

.protected-badge-draft {
  font-size: 9px;
  flex-shrink: 0;
  padding: 1px 4px;
  border-radius: 2px;
  background: transparent;
  color: var(--label-2);
}

.scheduled-badge {
  font-size: 8px;
  font-weight: 500;
  padding: 1px 4px;
  border-radius: 2px;
  background: var(--tint-scheduled);
  color: var(--label);
}
.scheduled-badge svg {
  color: var(--status-scheduled);
}

/* Pills keep their tints when selected — the selection is a tint too. */

.filename {
  font-size: 8.5px;
  font-family: 'SF Mono', monospace;
  color: var(--text-tertiary);
  opacity: 0.45;
  margin-top: 2px;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  letter-spacing: 0.01em;
}

/* .item.selected .filename inherits from the rule above */

.dek {
  font-size: 9px;
  color: var(--text-tertiary);
  margin-top: 2px;
  line-height: 1.35;
  display: -webkit-box;
  -webkit-line-clamp: 2;
  -webkit-box-orient: vertical;
  overflow: hidden;
  font-style: italic;
  opacity: 0.65;
}

.item.selected .dek {
  color: var(--text-tertiary);
}

.title {
  flex: 1;
  font-size: 11px;
  font-weight: 500;
  color: var(--text-primary);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  min-width: 0;
  letter-spacing: -0.01em;
}

.meta {
  margin-top: 2px;
  font-size: 9px;
  font-family: 'SF Mono', monospace;
  color: var(--text-tertiary);
  display: flex;
  gap: 8px;
  align-items: center;
}

.dir {
  color: var(--text-tertiary);
  max-width: 80px;
  overflow: hidden;
  text-overflow: ellipsis;
  display: inline-flex;
  align-items: center;
  gap: 2px;
}

.pub-date {
  color: var(--text-tertiary);
  display: inline-flex;
  align-items: center;
  gap: 2px;
}

.create-date {
  color: var(--text-tertiary);
  display: inline-flex;
  align-items: center;
  gap: 2px;
}

.edit-date {
  color: var(--text-tertiary);
  display: inline-flex;
  align-items: center;
  gap: 2px;
}

.word-count {
  font-variant-numeric: tabular-nums;
  font-feature-settings: 'tnum';
  opacity: 0.7;
  display: inline-flex;
  align-items: center;
  gap: 2px;
}

/* Compact Mode */
.sidebar.compact .item .content {
  padding: 4px 10px;
}

.sidebar.compact .filename {
  display: none;
}

.sidebar.compact .dek {
  display: none;
}

.sidebar.compact .meta {
  display: none;
}

.sidebar.compact .item .row {
  gap: 4px;
}

.sidebar.compact .group-header {
  padding: 4px 10px;
  font-size: 8px;
}
</style>
