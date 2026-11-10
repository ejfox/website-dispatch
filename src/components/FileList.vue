<script setup lang="ts">
import { parseEpochSeconds } from '../utils/formatting'
import { ref, computed, watch, nextTick } from 'vue'
import { perfTrace } from '../utils/perfTrace'
import { useDeployStatus } from '../composables/useUiState'
import { invoke } from '@tauri-apps/api/core'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { useLocalStorage } from '@vueuse/core'
import { Menu, MenuItem, CheckMenuItem, PredefinedMenuItem } from '@tauri-apps/api/menu'
import { LogicalPosition } from '@tauri-apps/api/dpi'
import {
  PhLockSimple,
  PhEyeSlash,
  PhClock,
  PhCaretRight,
  PhCaretDown,
  PhArrowsDownUp,
} from '@phosphor-icons/vue'
import UiButton from './ui/UiButton.vue'
import type { MarkdownFile } from '../types'
import { useAppConfig } from '../composables/useVaultData'
import { CHANGED_LABEL, CHANGED_HINT, NOT_PUBLISHED_LABEL } from '../utils/vocab'

const props = defineProps<{
  files: MarkdownFile[]
  selected: MarkdownFile | null
  loading: boolean
  compact?: boolean
}>()

const emit = defineEmits<{
  select: [file: MarkdownFile]
  /** Open the publish review sheet for this file (App.vue › openReviewSheet). */
  'request-publish': [file: MarkdownFile, mode: 'publish' | 'unlisted']
  'request-unpublish': [file: MarkdownFile]
  'open-settings': []
  /** Rows in on-screen order, collapsed sections skipped (keyboard nav). */
  'update:visible': [files: MarkdownFile[]]
}>()

// Keep the selected post in view: it's restored at launch and can be chosen
// from the palette, often far down a 150-post list.
watch(
  () => props.selected?.path,
  async () => {
    await nextTick()
    document.querySelector('.sidebar .item.selected')?.scrollIntoView({ block: 'nearest' })
  },
)

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

const { phaseFor, deployStatus } = useDeployStatus()

// ── Filter + sort ───────────────────────────────────────────────────────
// One status filter for the whole list, picked from a native menu (Finder's
// pull-down idiom). Content type is not a filter any more: each type is its
// own collapsible section below, so "show only projects" is "collapse the
// rest".
type StatusFilter = 'all' | 'published' | 'drafts' | 'scheduled'
type SortKey = 'recent' | 'created' | 'title' | 'words'
const filter = ref<StatusFilter>('all')
const sort = useLocalStorage<SortKey>('dispatch-sidebar-sort', 'recent')

const FILTER_LABELS: Record<StatusFilter, string> = {
  all: 'All',
  published: 'Live',
  drafts: NOT_PUBLISHED_LABEL,
  scheduled: 'Publish Later',
}
const SORT_LABELS: Record<SortKey, string> = {
  recent: 'Most Recent',
  created: 'Date Created',
  title: 'Title',
  words: 'Word Count',
}

function matchesFilter(f: MarkdownFile, which: StatusFilter): boolean {
  switch (which) {
    case 'published':
      return !!f.published_url
    case 'drafts':
      return !f.published_url
    case 'scheduled':
      return !!f.publish_at && !f.published_url
    default:
      return true
  }
}

function compare(a: MarkdownFile, b: MarkdownFile): number {
  switch (sort.value) {
    case 'recent':
      return (b.published_date || b.modified) - (a.published_date || a.modified)
    case 'created':
      // An unparseable date falls back to created; NaN would scramble the sort.
      return (parseEpochSeconds(b.date) ?? b.created) - (parseEpochSeconds(a.date) ?? a.created)
    case 'title':
      return formatTitle(a).localeCompare(formatTitle(b))
    case 'words':
      return b.word_count - a.word_count
  }
}

const filteredFiles = computed(() =>
  props.files.filter((f) => matchesFilter(f, filter.value)).sort(compare),
)

const filterCounts = computed(() => {
  const c = {} as Record<StatusFilter, number>
  for (const k of Object.keys(FILTER_LABELS) as StatusFilter[]) {
    c[k] = props.files.filter((f) => matchesFilter(f, k)).length
  }
  return c
})

// Native pull-down menus. CheckMenuItem gives the real macOS checkmark.
async function showFilterMenu(e: MouseEvent) {
  const items = await Promise.all(
    (Object.keys(FILTER_LABELS) as StatusFilter[]).map((k) =>
      CheckMenuItem.new({
        text: `${FILTER_LABELS[k]}  (${filterCounts.value[k]})`,
        checked: filter.value === k,
        enabled: k === 'all' || filterCounts.value[k] > 0,
        action: () => (filter.value = k),
      }),
    ),
  )
  await popupBelow(await Menu.new({ items }), e)
}

async function showSortMenu(e: MouseEvent) {
  const items = await Promise.all(
    (Object.keys(SORT_LABELS) as SortKey[]).map((k) =>
      CheckMenuItem.new({ text: SORT_LABELS[k], checked: sort.value === k, action: () => (sort.value = k) }),
    ),
  )
  await popupBelow(await Menu.new({ items }), e)
}

/** Drop the menu from the button's bottom-left corner, like an NSPopUpButton. */
async function popupBelow(menu: Menu, e: MouseEvent) {
  const r = (e.currentTarget as HTMLElement).getBoundingClientRect()
  await menu.popup(new LogicalPosition(r.left, r.bottom + 4))
}

// ── Sections ────────────────────────────────────────────────────────────
// Finder/Mail source list: one collapsible section per content type, posts
// first. Projects (91, mostly drafts) and week notes start collapsed so they
// can't bury the posts; the open/closed state persists.
type SectionKey = 'posts' | 'dispatch' | 'projects' | 'weeknotes'
interface Group {
  label: string | null
  files: MarkdownFile[]
}
interface Section {
  key: SectionKey
  label: string
  /** Quiet text before the count, e.g. "6 live" for projects. */
  meta: string | null
  files: MarkdownFile[]
  groups: Group[]
}

const SECTION_LABELS: Record<SectionKey, string> = {
  posts: 'Posts',
  dispatch: 'Dispatch',
  projects: 'Projects',
  weeknotes: 'Week Notes',
}
function sectionOf(f: MarkdownFile): SectionKey {
  if (f.content_type === 'project') return 'projects'
  if (f.content_type === 'dispatch') return 'dispatch'
  if (f.content_type === 'weeknote') return 'weeknotes'
  return 'posts' // blog posts and photo posts
}

const collapsed = useLocalStorage<Record<SectionKey, boolean>>(
  'dispatch-sidebar-collapsed-sections',
  { posts: false, dispatch: false, projects: true, weeknotes: true },
  { mergeDefaults: true },
)
function toggleSection(key: SectionKey) {
  collapsed.value = { ...collapsed.value, [key]: !collapsed.value[key] }
}

// Notes.app date buckets: Today, Yesterday, Previous 7 Days, Previous 30
// Days, then this year's months by name, then whole years.
function dateGroups(list: MarkdownFile[]): Group[] {
  const today = new Date()
  today.setHours(0, 0, 0, 0)
  const t0 = today.getTime() / 1000
  const groups: Group[] = []
  for (const f of list) {
    const ts = f.published_date || f.modified
    const d = new Date(ts * 1000)
    let label: string
    if (ts >= t0) label = 'Today'
    else if (ts >= t0 - 86400) label = 'Yesterday'
    else if (ts >= t0 - 86400 * 7) label = 'Previous 7 Days'
    else if (ts >= t0 - 86400 * 30) label = 'Previous 30 Days'
    else if (d.getFullYear() === today.getFullYear()) label = d.toLocaleString('en-US', { month: 'long' })
    else label = String(d.getFullYear())
    const last = groups[groups.length - 1]
    if (last?.label === label) last.files.push(f)
    else groups.push({ label, files: [f] })
  }
  return groups
}

const sections = computed<Section[]>(() => {
  const by: Record<SectionKey, MarkdownFile[]> = { posts: [], dispatch: [], projects: [], weeknotes: [] }
  for (const f of filteredFiles.value) by[sectionOf(f)].push(f)
  return (Object.keys(SECTION_LABELS) as SectionKey[])
    .filter((key) => by[key].length)
    .map((key) => {
      const list = by[key]
      let groups: Group[]
      let meta: string | null = null
      if (key === 'projects') {
        // Live first: the six on the site matter more than 85 parked drafts.
        const live = list.filter((f) => f.published_url)
        const rest = list.filter((f) => !f.published_url)
        groups =
          live.length && rest.length
            ? [
                { label: 'Live', files: live },
                { label: NOT_PUBLISHED_LABEL, files: rest },
              ]
            : [{ label: null, files: list }]
        if (live.length && rest.length) meta = `${live.length} live`
      } else if (key === 'posts' && sort.value === 'recent') {
        groups = dateGroups(list)
      } else {
        groups = [{ label: null, files: list }]
      }
      return { key, label: SECTION_LABELS[key], meta, files: list, groups }
    })
})

// The selected post can arrive from launch restore or the command palette:
// open its section so it's never selected-but-invisible.
watch(
  () => props.selected?.path,
  (path) => {
    if (!path || !props.selected) return
    const key = sectionOf(props.selected)
    if (collapsed.value[key]) collapsed.value = { ...collapsed.value, [key]: false }
  },
  { immediate: true },
)

// Rows in on-screen order, collapsed sections skipped. App.vue walks this
// for ↑↓ / j k / gg / G, so the keyboard follows what you see.
const visibleFiles = computed(() =>
  sections.value.filter((s) => !collapsed.value[s.key]).flatMap((s) => s.files),
)
watch(visibleFiles, (list) => emit('update:visible', list), { immediate: true })

/** What the empty list says when the filter hides everything. */
const emptyFilterText = computed(() => {
  switch (filter.value) {
    case 'published':
      return 'Nothing live yet.'
    case 'drafts':
      return 'Everything here is live.'
    case 'scheduled':
      return 'Nothing set to publish later.'
    default:
      return 'No posts match.'
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
  | 'stalled'
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
  if (deploy === 'failed') {
    // Timed out / untracked: may still land, so amber, not red.
    const f = deployStatus.value[file.path]?.failure
    return f === 'timeout' || f === 'untracked' ? 'stalled' : 'failed'
  }
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
  stalled: 'attention',
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
    case 'stalled':
      return 'Not live yet'
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

  // Same verbs, labels and (display-only) shortcuts as the menu bar
  // (src-tauri/src/menu.rs), so the two teach each other. Publishing always
  // goes through the review sheet, hence the ellipses.
  const isLive = !!file.published_url
  const sep = () => PredefinedMenuItem.new({ item: 'Separator' })
  const items: (MenuItem | PredefinedMenuItem)[] = [
    await MenuItem.new({
      text: 'Open in Obsidian',
      accelerator: 'CmdOrCtrl+Shift+O',
      action: () => invoke('open_in_obsidian', { path: file.path }),
    }),
    await MenuItem.new({
      text: `Open in ${defaultEditor.value}`,
      action: () => invoke('open_in_app', { path: file.path, app: defaultEditor.value }),
    }),
    await MenuItem.new({
      text: 'Reveal in Finder',
      accelerator: 'CmdOrCtrl+Shift+R',
      action: () => invoke('open_in_app', { path: file.path, app: 'Finder' }).catch(() => {}),
    }),
    await sep(),
    await MenuItem.new({
      text: isLive ? 'Republish…' : 'Publish…',
      accelerator: 'CmdOrCtrl+Return',
      action: () => emit('request-publish', file, 'publish'),
    }),
  ]
  if (!isLive && !file.unlisted && !file.password) {
    items.push(
      await MenuItem.new({
        text: 'Publish as Unlisted…',
        accelerator: 'CmdOrCtrl+Shift+Return',
        action: () => emit('request-publish', file, 'unlisted'),
      }),
    )
  }
  if (isLive) {
    items.push(
      await sep(),
      await MenuItem.new({ text: 'View on Site', action: () => window.open(file.published_url!, '_blank') }),
      await MenuItem.new({
        text: 'Copy Public URL',
        accelerator: 'CmdOrCtrl+Shift+L',
        action: () => navigator.clipboard.writeText(file.published_url!),
      }),
    )
  }
  items.push(
    await sep(),
    await MenuItem.new({
      text: 'Copy File Path',
      accelerator: 'CmdOrCtrl+Alt+C',
      action: () => navigator.clipboard.writeText(file.path),
    }),
  )
  if (isLive) {
    items.push(
      await sep(),
      await MenuItem.new({ text: 'Unpublish…', action: () => emit('request-unpublish', file) }),
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
      <button
        class="filter-popup"
        :class="{ active: filter !== 'all' }"
        :aria-label="`Filter: ${FILTER_LABELS[filter]}`"
        data-tip="Show all, live, or unpublished"
        @click="showFilterMenu"
      >
        {{ FILTER_LABELS[filter] }}
        <span class="filter-popup-count">{{ filteredFiles.length }}</span>
        <PhCaretDown :size="9" weight="bold" class="filter-popup-caret" />
      </button>
      <button
        class="sort-button"
        :aria-label="`Sort: ${SORT_LABELS[sort]}`"
        :data-tip="`Sorted by ${SORT_LABELS[sort].toLowerCase()}`"
        @click="showSortMenu"
      >
        <PhArrowsDownUp :size="13" />
      </button>
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
      <UiButton size="small" class="list-empty-btn" label="Choose vault folder…" @click="emit('open-settings')" />
    </div>

    <!-- A filter hid everything. -->
    <div v-else-if="filteredFiles.length === 0" class="list-empty">
      <div class="list-empty-sub">{{ emptyFilterText }}</div>
      <UiButton size="small" class="list-empty-btn" label="Show all" @click="filter = 'all'" />
    </div>

    <div v-else class="list">
      <section v-for="section in sections" :key="section.key" class="section" :class="`section-${section.key}`">
        <button
          class="section-header"
          :aria-expanded="!collapsed[section.key]"
          @click="toggleSection(section.key)"
          @contextmenu="showGroupContextMenu(section, $event)"
        >
          <PhCaretRight :size="10" weight="bold" class="disclosure" :class="{ open: !collapsed[section.key] }" />
          <span class="section-label">{{ section.label }}</span>
          <span v-if="section.meta" class="section-meta">{{ section.meta }}</span>
          <span class="section-count">{{ section.files.length }}</span>
        </button>
        <template v-if="!collapsed[section.key]">
          <template v-for="group in section.groups" :key="`${section.key}-${group.label}`">
            <div v-if="group.label" class="group-header" @contextmenu="showGroupContextMenu(group, $event)">
              {{ group.label }}
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
                parked: file.draft && !file.published_url,
              }"
              @click="onRowClick(file)"
              @contextmenu="showContextMenu(file, $event)"
            >
              <!-- Fixed-width dot slot so titles line up whether or not a row
                   has a state. Drafts show no dot: rest state stays quiet. -->
              <span class="dot-slot">
                <span
                  v-if="rowState(file)"
                  class="status-dot row-dot"
                  :data-state="rowState(file)"
                  :data-status="rowStatus(file)"
                  :data-tip="rowStateLabel(file)"
                  aria-hidden="true"
                ></span>
              </span>
              <img
                v-if="file.content_type === 'photos' && file.thumbnail"
                class="row-thumb"
                :src="thumbSrc(file.thumbnail)"
                loading="lazy"
                alt=""
              />
              <div class="content">
                <div class="row">
                  <span class="title" :title="formatTitle(file)">{{ formatTitle(file) }}</span>
                  <!-- Trailing accessories, Mail-style: one quiet glyph or
                       number, never a pill. The dot already says live. -->
                  <span
                    v-if="file.published_url && file.warnings.includes('Modified since publish') && file.published_word_count != null"
                    class="accessory delta"
                    :class="{ pos: file.word_count > file.published_word_count, neg: file.word_count < file.published_word_count }"
                    :data-tip="`${CHANGED_HINT} · was ${file.published_word_count.toLocaleString()}w, now ${file.word_count.toLocaleString()}w`"
                  >
                    {{ file.word_count >= file.published_word_count ? '+' : ''
                    }}{{ (file.word_count - file.published_word_count).toLocaleString() }}w
                  </span>
                  <span
                    v-if="file.password"
                    class="accessory"
                    :data-tip="file.published_url ? 'Live, behind a password' : 'Will be password-protected'"
                  >
                    <PhLockSimple :size="11" weight="fill" />
                  </span>
                  <span
                    v-else-if="file.unlisted"
                    class="accessory"
                    :data-tip="file.published_url ? 'Live, left off listings and feeds' : 'Will be unlisted'"
                  >
                    <PhEyeSlash :size="11" />
                  </span>
                  <span
                    v-if="file.publish_at && !file.published_url"
                    class="accessory"
                    data-tip="Dispatch publishes it at the set time, while it’s open"
                  >
                    <PhClock :size="11" />
                  </span>
                </div>
                <div v-if="file.dek" class="dek">{{ file.dek }}</div>
                <div class="meta">
                  <span v-if="file.draft && !file.published_url">Draft</span>
                  <span>{{ formatAge(file.published_date || file.modified) }}</span>
                  <span>{{ formatWordCount(file.word_count) }} words</span>
                </div>
              </div>
            </button>
          </template>
        </template>
      </section>
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
/* ── Control bar: filter pull-down + sort ─────────────────────────── */
.control-bar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  background: transparent;
  flex-shrink: 0;
  /* No rule under it: the column reads as one translucent surface. */
  padding: 0 8px 0 10px;
  height: 32px;
  gap: 6px;
}

/* NSPopUpButton, pull-down style: borderless until hovered. */
.filter-popup {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  height: 22px;
  padding: 0 6px;
  margin-left: -6px;
  border: none;
  border-radius: 5px;
  background: transparent;
  color: var(--label);
  font: 600 13px/1 -apple-system, BlinkMacSystemFont, sans-serif;
  cursor: default;
}
.filter-popup:hover {
  background: var(--fill);
}
.filter-popup:active {
  background: var(--fill-2);
}
/* A narrowed list says so in the accent, Finder-style. */
.filter-popup.active {
  color: var(--accent);
}
.filter-popup-count {
  font-weight: 400;
  color: var(--label-2);
  font-variant-numeric: tabular-nums;
}
.filter-popup-caret {
  color: var(--label-2);
}

.sort-button {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 24px;
  height: 22px;
  border: none;
  border-radius: 5px;
  background: transparent;
  color: var(--label-2);
  cursor: default;
}
.sort-button:hover {
  background: var(--fill);
  color: var(--label);
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
  margin-top: var(--space-1);
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

/* ── Source list ───────────────────────────────────────────────────── */
.list {
  flex: 1;
  overflow-y: auto;
  padding: 2px 0 12px;
  display: flex;
  flex-direction: column;
  align-items: stretch;
}

.section {
  display: flex;
  flex-direction: column;
  align-items: stretch;
}

/* Section header: Finder/Mail sidebar heading. Title Case, small bold,
   secondary label, disclosure chevron, trailing count. Not sticky: on a
   vibrant surface a sticky header would need an opaque fill. */
.section-header {
  display: flex;
  align-items: center;
  gap: 4px;
  margin: 10px 6px 2px;
  padding: 3px 8px 3px 4px;
  border: none;
  border-radius: 5px;
  background: transparent;
  color: var(--label-2);
  font: 700 11px/14px -apple-system, BlinkMacSystemFont, sans-serif;
  text-align: left;
  cursor: default;
}
.section:first-child .section-header {
  margin-top: 2px;
}
.section-header:hover {
  background: var(--fill);
}
.disclosure {
  flex-shrink: 0;
  color: var(--label-3);
  transition: transform 0.15s ease;
}
.disclosure.open {
  transform: rotate(90deg);
}
.section-header:hover .disclosure {
  color: var(--label-2);
}
.section-label {
  flex: 1;
}
.section-meta,
.section-count {
  font-weight: 400;
  font-variant-numeric: tabular-nums;
}
.section-meta {
  color: var(--label-2);
}
.section-meta::after {
  content: ' ·';
}
.section-count {
  color: var(--label-3);
}

/* Date / Live sub-groups inside a section: Notes.app-style small heading. */
.group-header {
  padding: 8px 14px 2px 26px;
  font: 600 11px/14px -apple-system, BlinkMacSystemFont, sans-serif;
  color: var(--label-3);
}

/* Row: inset rounded selection, no dividers (source lists have none). */
.item {
  display: flex;
  align-items: flex-start;
  flex: none;
  gap: 0;
  margin: 0 6px;
  padding: 0;
  min-height: 28px;
  border: none;
  border-radius: 6px;
  background: transparent;
  cursor: default;
  text-align: left;
  overflow: hidden;
}
.item:hover {
  background: var(--fill);
}
.item.selected {
  background: var(--accent-selected);
}

.dot-slot {
  flex: none;
  width: 20px;
  height: 28px;
  display: flex;
  align-items: center;
  justify-content: center;
}
/* Hold the dot off the accent selection fill so it never merges into it. */
.item.selected .row-dot:not([data-status='deploying']) {
  box-shadow: 0 0 0 1.5px var(--control-bg);
}

.row-thumb {
  flex: none;
  width: 28px;
  height: 28px;
  margin: 6px 6px 6px 0;
  border-radius: 4px;
  object-fit: cover;
}

.content {
  flex: 1;
  min-width: 0;
  padding: 6px 10px 6px 0;
}

.row {
  display: flex;
  align-items: center;
  gap: 6px;
  min-height: 16px;
}

.title {
  flex: 1;
  min-width: 0;
  font: 400 13px/16px -apple-system, BlinkMacSystemFont, sans-serif;
  color: var(--label);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}
/* `draft: true` rows are parked on purpose: secondary label, like a
   disabled-but-present item. */
.item.parked .title {
  color: var(--label-2);
}

/* Trailing accessories: one quiet glyph or number per fact. */
.accessory {
  flex: none;
  display: inline-flex;
  align-items: center;
  color: var(--label-2);
  font: 400 11px/1 -apple-system, BlinkMacSystemFont, sans-serif;
  font-variant-numeric: tabular-nums;
}

.dek {
  margin-top: 2px;
  font: 400 12px/15px -apple-system, BlinkMacSystemFont, sans-serif;
  color: var(--label-2);
  display: -webkit-box;
  -webkit-line-clamp: 2;
  -webkit-box-orient: vertical;
  overflow: hidden;
}

.meta {
  display: flex;
  gap: 8px;
  margin-top: 2px;
  font: 400 11px/14px -apple-system, BlinkMacSystemFont, sans-serif;
  color: var(--label-2);
  font-variant-numeric: tabular-nums;
}

/* Compact (View › Compact List): one 28px line per row, Finder density. */
.sidebar.compact .item .content {
  padding: 6px 10px 6px 0;
}
.sidebar.compact .dek,
.sidebar.compact .meta {
  display: none;
}
.sidebar.compact .row-thumb {
  width: 20px;
  height: 20px;
  margin: 4px 6px 4px 0;
}
</style>
