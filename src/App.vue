<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted, nextTick, watch } from 'vue'
import { Menu, MenuItem, PredefinedMenuItem } from '@tauri-apps/api/menu'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { getCurrentWindow } from '@tauri-apps/api/window'
import {
  FileText,
  Search,
  RefreshCw,
  Settings,
  Plus,
  Eye,
  Image as LucideImage,
  Activity,
  Pencil,
  BookOpen,
  Backpack,
} from 'lucide-vue-next'
import {
  PhRows,
  PhSquaresFour,
  PhCircleWavy,
  PhChartBar,
  PhDeviceMobile,
  PhFolder,
  PhTextAa,
  PhGitBranch,
  PhImageSquare,
  PhArrowSquareDown,
  PhFileText,
  PhWarningCircle,
} from '@phosphor-icons/vue'
import FileList from './components/FileList.vue'
import ResizeHandle from './components/ResizeHandle.vue'
import { useResizable } from './composables/useResizable'
import FilePreview from './components/FilePreview.vue'
import MediaLibraryModal from './components/Media/MediaLibraryModal.vue'
import PublishingJournal from './components/PublishingJournal.vue'
import ActivityFeed from './components/ActivityFeed.vue'
import ModifiedPostsView from './components/ModifiedPostsView.vue'
import GearPanel from './components/GearPanel.vue'
import SettingsModal from './components/SettingsModal.vue'
import SearchModal from './components/SearchModal.vue'
import CommandPalette from './components/CommandPalette.vue'
import HelpOverlay from './components/HelpOverlay.vue'
import ToastStack from './components/ToastStack.vue'
import { useLocalStorage } from '@vueuse/core'
import type { MarkdownFile } from './types'
import { useKeyboardShortcuts } from './composables/useKeyboardShortcuts'
import { useAppConfig } from './composables/useAppConfig'
import { useConnectionStatus } from './composables/useConnectionStatus'
import { useToasts } from './composables/useToasts'
import { checkForUpdate } from './composables/useAutoUpdate'

const toasts = useToasts()

const files = ref<MarkdownFile[]>([])
const selectedFile = ref<MarkdownFile | null>(null)
const loading = ref(true)

const appVersion = __APP_VERSION__

// Search state
const searchOpen = ref(false)

// Command palette (⌘K) — supersedes the older searchOpen flow but
// SearchModal stays around as a pure file-search fallback if needed.
const paletteOpen = ref(false)

// New Post state
const newPostOpen = ref(false)
const newPostTitle = ref('')
const newPostInput = ref<HTMLInputElement | null>(null)
const newPostCreating = ref(false)

// Help state
const showHelp = ref(false)

// Window focus state (for native dimming when unfocused)
const windowFocused = ref(true)

/**
 * Manual window-drag handler. With `titleBarStyle: "Overlay"` in
 * tauri.conf.json, the CSS `-webkit-app-region: drag` mechanism is broken
 * on macOS Sonoma+ — the window won't move on click+drag (Tauri issues
 * #9503, #11605). The documented workaround is to call
 * `getCurrentWindow().startDragging()` from a mousedown listener on the
 * elements we want to act as drag handles.
 *
 * Bail out for non-primary clicks and for clicks that bubble up from
 * interactive descendants (buttons, links, inputs) so we don't hijack
 * intended button presses.
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

// Drag-drop state — drives the full-window overlay so the user gets visual
// confirmation that Dispatch is going to handle the dropped file(s).
const dragActive = ref(false)
const dragCount = ref(0)
const dragKind = ref<'media' | 'markdown' | 'mixed' | 'unsupported'>('media')

// Cached media-status snapshot — used by the drag overlay to label the
// destination. Refreshed on mount and after Settings save; cheap enough
// to also refresh on each drag-enter without it being noticeable.
const mediaStatusCache = ref<{ primary: string; cloudinary: boolean; r2: boolean } | null>(null)
async function refreshMediaStatus() {
  try {
    mediaStatusCache.value = (await invoke('check_media_status')) as any
  } catch {
    mediaStatusCache.value = null
  }
}

// Folder hint mirroring the drop-handler's logic, used only for display in
// the drag overlay so the user knows where files will land. Keeping the
// two in sync by extracting if this grows.
const folderHintForSelected = computed(() => {
  const sd = selectedFile.value?.source_dir
  if (!sd) return 'root'
  const m = sd.match(/(\d{4})/)
  return m ? `blog/${m[1]}` : sd
})

// Capitalize the destination key from `check_media_status` for display.
// "cloudinary" → "Cloudinary", "r2" → "R2".
const destinationLabel = computed(() => {
  const p = mediaStatusCache.value?.primary
  if (!p) return 'Cloudinary'
  if (p === 'r2') return 'R2'
  return p.charAt(0).toUpperCase() + p.slice(1)
})

// Count of MODIFIED posts (source diverged from live). Drives the small
// badge on the Modified tab so the user sees "I have stuff to triage"
// without having to click in.
const modifiedCount = computed(
  () => files.value.filter((f) => f.warnings.includes('Modified since publish')).length,
)

// Reflect the modified backlog on the Dock icon. macOS shows the number in a
// red bubble on the app icon — glanceable "you have N posts to triage" without
// the window focused. `setBadgeCount(undefined)` clears it. Tauri 2.2+ API.
watch(
  modifiedCount,
  (n) => {
    getCurrentWindow().setBadgeCount(n > 0 ? n : undefined).catch(() => {})
  },
  { immediate: true },
)

// File → Open Recent. Push the selected file's path to Rust; the menubar
// gets rebuilt there. Rust dedupes/reorders, so we don't have to.
watch(
  () => selectedFile.value?.path,
  (path) => {
    if (path) invoke('record_recent_file', { path }).catch(() => {})
  },
)

// Right panel tab state. Persists the user's last/preferred home tab so
// Dispatch opens where they want to be — Preview by default for most folks,
// but switchable to Journal or Gear for routines that don't start with the post list.
const defaultHomeTab = useLocalStorage<'preview' | 'media' | 'activity' | 'modified' | 'journal' | 'gear'>(
  'dispatch-home-tab',
  'preview',
)
const rightTab = ref<'preview' | 'media' | 'activity' | 'modified' | 'journal' | 'gear'>(defaultHomeTab.value)

// Connection status (auto-checks on creation)
const { cloudinaryConnected, obsidianConnected, analyticsConnected, companionUrl, companionPin, gitBranch } =
  useConnectionStatus()

// Journal stats (for status bar / titlebar)
const journalStats = ref<any>(null)

async function refreshJournalStats() {
  try {
    journalStats.value = await invoke('get_journal_stats')
  } catch (_) {
    /* journal may not be ready yet */
  }
}

// Settings modal
const showSettings = ref(false)

// FilePreview ref for Cmd+Enter publish
const filePreviewRef = ref<InstanceType<typeof FilePreview> | null>(null)

// App config (shared singleton)
const { appConfig, fetchConfig: loadConfig } = useAppConfig()

// Compact mode preference (auto-syncs with localStorage)
const compactMode = useLocalStorage('dispatch-compact', false)

// Sidebar collapse — ⌘0 toggles. When collapsed the file list hides and
// the right panel + preview take the full window width. Useful for focused
// reading / writing modes.
const sidebarCollapsed = useLocalStorage('dispatch-sidebar-collapsed', false)

// Drag-to-resize sidebar width. Bounds mirror the old clamp(240..400) but the
// max grows a little on wide windows so it doesn't feel pinned. Double-click
// the divider to snap back to 300px.
const {
  size: sidebarWidth,
  dragging: sidebarDragging,
  start: startSidebarResize,
  reset: resetSidebarWidth,
} = useResizable('dispatch-sidebar-width', {
  default: 300,
  min: 200,
  max: () => Math.min(560, window.innerWidth * 0.5),
  axis: 'x',
})

// ⌘K toggles the command palette (CommandPalette.vue). The legacy
// SearchModal stays mounted but is not bound to anything — kept around
// in case we want to expose a pure file-search shortcut later.
// `openSearch` is what useKeyboardShortcuts calls; we toggle here so
// double-⌘K closes (composable's "is searchOpen" check would lie since
// it tracks the deprecated SearchModal flag).
function openSearch() {
  paletteOpen.value = !paletteOpen.value
}

function closeSearch() {
  paletteOpen.value = false
  searchOpen.value = false
}

function openNewPost() {
  newPostOpen.value = true
  newPostTitle.value = ''
  setTimeout(() => newPostInput.value?.focus(), 10)
}

function closeNewPost() {
  newPostOpen.value = false
  newPostTitle.value = ''
}

async function createNewPost() {
  if (!newPostTitle.value.trim() || newPostCreating.value) return
  newPostCreating.value = true
  try {
    const path: string = await invoke('create_new_post', { title: newPostTitle.value.trim() })
    closeNewPost()
    await loadFiles()
    // Select the new file
    const newFile = files.value.find((f) => f.path === path)
    if (newFile) selectedFile.value = newFile
    // Open in default editor
    const editor = appConfig.value?.default_editor || 'iA Writer'
    invoke('open_in_app', { path, app: editor })
  } catch (e) {
    console.error('Failed to create post:', e)
  }
  newPostCreating.value = false
}

function copyToClipboard(text: string) {
  navigator.clipboard.writeText(text)
}

async function handleInsertMedia(markdown: string) {
  if (!selectedFile.value) {
    // No file selected - just copy to clipboard
    navigator.clipboard.writeText(markdown)
    return
  }

  try {
    await invoke('append_to_file', {
      path: selectedFile.value.path,
      content: markdown,
    })
    rightTab.value = 'preview'
    // Refresh the file content by re-selecting
    const file = selectedFile.value
    selectedFile.value = null
    await nextTick()
    selectedFile.value = file
  } catch (e) {
    console.error('Failed to insert media:', e)
    // Fallback to clipboard
    navigator.clipboard.writeText(markdown)
  }
}

function handleSearchSelect(file: MarkdownFile) {
  selectedFile.value = file
  closeSearch()
}

// Persist the last-opened file path so Dispatch re-opens to where you were.
const lastFilePath = useLocalStorage<string>('dispatch-last-file', '')

async function loadFiles() {
  loading.value = true
  try {
    files.value = await invoke('get_recent_files', { limit: 200 })
  } catch (e) {
    console.error('Failed to load files:', e)
  }
  loading.value = false
  // Restore prior selection on first load if it still exists in the list.
  if (!selectedFile.value && lastFilePath.value) {
    const match = files.value.find((f) => f.path === lastFilePath.value)
    if (match) selectedFile.value = match
  }
  // Refresh journal stats (publish may have just happened)
  refreshJournalStats()
}

watch(selectedFile, (f) => {
  lastFilePath.value = f?.path || ''
})

// Status-bar chip context menus (native-feel right-click).
async function showVaultMenu(e: MouseEvent) {
  e.preventDefault()
  const vaultPath = appConfig.value?.vault.path || ''
  const vaultName = appConfig.value?.vault.name || ''
  const items_: any[] = []
  if (vaultPath) {
    items_.push(
      await MenuItem.new({
        text: 'Reveal Vault in Finder',
        action: () => invoke('open_in_app', { path: vaultPath, app: 'Finder' }).catch(() => {}),
      }),
    )
  }
  if (vaultName) {
    items_.push(
      await MenuItem.new({
        text: 'Open Vault in Obsidian',
        action: () => window.open(`obsidian://open?vault=${encodeURIComponent(vaultName)}`, '_blank'),
      }),
    )
  }
  if (vaultPath) {
    items_.push(
      await PredefinedMenuItem.new({ item: 'Separator' }),
      await MenuItem.new({ text: 'Copy Vault Path', action: () => navigator.clipboard.writeText(vaultPath) }),
    )
  }
  items_.push(
    await PredefinedMenuItem.new({ item: 'Separator' }),
    await MenuItem.new({ text: 'Vault Settings…', action: () => (showSettings.value = true) }),
  )
  const menu = await Menu.new({ items: items_ })
  await menu.popup()
}

async function showMediaMenu(e: MouseEvent) {
  e.preventDefault()
  // Prefer the new nested location (media.cloudinary.cloud_name); fall back to
  // the deprecated top-level field for any user whose config hasn't yet been
  // round-tripped through the migration in `config::load_or_create`.
  const cloud =
    appConfig.value?.media?.cloudinary?.cloud_name || appConfig.value?.cloudinary_cloud_name
  const items_: any[] = [
    await MenuItem.new({
      text: 'Show Media Library',
      action: () => (rightTab.value = 'media'),
    }),
  ]
  if (cloud) {
    items_.push(
      await MenuItem.new({
        text: 'Open Cloudinary Console',
        action: () => window.open(`https://console.cloudinary.com/console/c-${cloud}/media_library/folders/home`, '_blank'),
      }),
    )
  }
  items_.push(
    await PredefinedMenuItem.new({ item: 'Separator' }),
    await MenuItem.new({ text: 'Media Settings…', action: () => (showSettings.value = true) }),
  )
  const menu = await Menu.new({ items: items_ })
  await menu.popup()
}

async function showAnalyticsMenu(e: MouseEvent) {
  e.preventDefault()
  const url = appConfig.value?.analytics_url
  const items_: any[] = []
  if (url) {
    items_.push(
      await MenuItem.new({ text: 'Open Analytics Dashboard', action: () => window.open(url, '_blank') }),
      await MenuItem.new({ text: 'Copy Analytics URL', action: () => navigator.clipboard.writeText(url) }),
      await PredefinedMenuItem.new({ item: 'Separator' }),
    )
  }
  items_.push(
    await MenuItem.new({ text: 'Analytics Settings…', action: () => (showSettings.value = true) }),
  )
  const menu = await Menu.new({ items: items_ })
  await menu.popup()
}

async function showCompanionMenu(e: MouseEvent) {
  e.preventDefault()
  if (!companionUrl.value) return
  const url = companionUrl.value
  const pin = companionPin.value || ''
  const menu = await Menu.new({
    items: [
      await MenuItem.new({ text: 'Open Companion in Browser', action: () => window.open(url, '_blank') }),
      await PredefinedMenuItem.new({ item: 'Separator' }),
      await MenuItem.new({ text: 'Copy Companion URL', action: () => navigator.clipboard.writeText(url) }),
      await MenuItem.new({ text: `Copy PIN (${pin})`, action: () => navigator.clipboard.writeText(pin) }),
      await MenuItem.new({
        text: 'Copy URL + PIN',
        action: () => navigator.clipboard.writeText(`${url} (PIN: ${pin})`),
      }),
    ],
  })
  await menu.popup()
}

async function showBranchMenu(e: MouseEvent) {
  e.preventDefault()
  const branch = gitBranch.value
  if (!branch) return
  const menu = await Menu.new({
    items: [
      await MenuItem.new({ text: `On branch "${branch}"`, enabled: false, action: () => {} }),
      await PredefinedMenuItem.new({ item: 'Separator' }),
      await MenuItem.new({ text: 'Copy Branch Name', action: () => navigator.clipboard.writeText(branch) }),
    ],
  })
  await menu.popup()
}

// Journal → Recent Activity click: find the matching file in the list and
// surface it in the Preview tab.
// Backlinks-graph node click: find the file by its full path.
function jumpToPath(path: string) {
  const match = files.value.find((f) => f.path === path)
  if (match) selectedFile.value = match
}

function jumpToSlug(slug: string) {
  // Slug shape from journal: "2026/dubai-chocolate-term-trace" (year/slug)
  // or sometimes bare "vulpes-browser". Match by filename or path tail.
  const bare = slug.split('/').pop() || slug
  const match = files.value.find((f) => {
    const stem = f.filename.replace(/\.md$/, '')
    return stem === bare || f.path.endsWith(`${slug}.md`)
  })
  if (match) {
    selectedFile.value = match
    rightTab.value = 'preview'
  }
}

// Keyboard shortcuts composable
const { handleGlobalKey } = useKeyboardShortcuts({
  files,
  selectedFile,
  searchOpen,
  showSettings,
  newPostOpen,
  showHelp,
  rightTab,
  sidebarCollapsed,
  filePreviewRef,
  openSearch,
  closeSearch,
  openNewPost,
  closeNewPost,
  loadFiles,
})

let unlistenSchedule: (() => void) | null = null

function onSettingsSaved() {
  loadConfig()
  loadFiles()
  refreshMediaStatus()
}

onMounted(async () => {
  loadFiles()
  refreshJournalStats()
  refreshMediaStatus()
  window.addEventListener('keydown', handleGlobalKey)

  // Quietly check for updates on startup (after the UI has had a chance
  // to settle). Surfaces a sticky toast if a newer release is available.
  setTimeout(() => {
    checkForUpdate()
  }, 3000)

  // Listen for scheduled publish events from backend
  const unlisten = await listen('scheduled-publish', (_event) => {
    loadFiles()
  })
  unlistenSchedule = unlisten

  // Track window focus for native dimming behavior
  getCurrentWindow().onFocusChanged(({ payload: focused }) => {
    windowFocused.value = focused
  })

  // Handle menu bar events from Rust
  listen('menu-new-post', () => openNewPost())
  listen('menu-refresh', () => loadFiles())
  listen('menu-toggle-compact', () => {
    compactMode.value = !compactMode.value
  })
  listen('menu-search', () => openSearch())

  // Open Recent: payload is the absolute file path. If the file is in the
  // current scan, select it; otherwise the path is stale (deleted/moved) —
  // surface a toast so the user knows why nothing happened.
  listen<string>('menu-open-recent', (event) => {
    const path = event.payload
    const match = files.value.find((f) => f.path === path)
    if (match) {
      selectedFile.value = match
      rightTab.value = 'preview'
    } else {
      toasts.info('That file is no longer in the vault.')
    }
  })

  // Auto-refresh on vault file changes (fs::notify watcher in Rust).
  // Debounced 500ms server-side; we still throttle the toast so a flurry
  // of saves doesn't spam the UI.
  let lastVaultToastAt = 0
  listen('vault-changed', () => {
    loadFiles()
    refreshJournalStats()
    const now = Date.now()
    if (now - lastVaultToastAt > 8000) {
      lastVaultToastAt = now
      toasts.info('Vault updated')
    }
  })

  // Drag-in handler: routes by file type.
  //   .md  → select the post in the list (existing behavior)
  //   image/video → upload to Cloudinary, copy ready-to-paste markdown
  //                 references to the clipboard. One motion: drag → ⌘V in
  //                 your editor at the cursor where you want it.
  // Drag-drop: window-level handler. Tauri 2 emits four types — `enter` /
  // `over` / `leave` / `drop` — we use the first three to drive the visual
  // overlay and `drop` to actually do the work.
  const IMAGE_EXTS = ['png', 'jpg', 'jpeg', 'gif', 'webp', 'svg', 'heic', 'heif', 'avif', 'bmp', 'tiff']
  const VIDEO_EXTS = ['mp4', 'mov', 'webm', 'm4v', 'mkv']
  const ext = (p: string) => p.split('.').pop()?.toLowerCase() || ''
  const classify = (paths: string[]): typeof dragKind.value => {
    let md = 0
    let media = 0
    let other = 0
    for (const p of paths) {
      const e = ext(p)
      if (e === 'md') md++
      else if (IMAGE_EXTS.includes(e) || VIDEO_EXTS.includes(e)) media++
      else other++
    }
    if (md > 0 && media === 0 && other === 0) return 'markdown'
    if (media > 0 && md === 0 && other === 0) return 'media'
    if (other > 0 && md === 0 && media === 0) return 'unsupported'
    return 'mixed'
  }

  getCurrentWindow().onDragDropEvent(async (event) => {
    const t = event.payload.type
    if (t === 'enter') {
      // Only classify once per drag — `over` fires continuously and the
      // payload paths don't change while a drag is in progress.
      const paths: string[] = (event.payload as any).paths || []
      dragCount.value = paths.length
      dragKind.value = classify(paths)
      dragActive.value = true
      // Refresh the destination status snapshot so the overlay's
      // "going to Cloudinary" label is accurate if the user just
      // changed settings.
      void refreshMediaStatus()
      return
    }
    if (t === 'over') {
      // Cheap: just keep the overlay alive. No reclassification needed.
      if (!dragActive.value) dragActive.value = true
      return
    }
    if (t === 'leave') {
      dragActive.value = false
      return
    }
    if (t !== 'drop') return

    dragActive.value = false
    const paths: string[] = event.payload.paths || []
    if (!paths.length) return

    const mdPaths = paths.filter((p) => ext(p) === 'md')
    const mediaPaths = paths.filter(
      (p) => IMAGE_EXTS.includes(ext(p)) || VIDEO_EXTS.includes(ext(p)),
    )

    // Markdown: existing select-or-open behavior.
    if (mdPaths.length) {
      const target = mdPaths[0]
      const existing = files.value.find((f) => f.path === target)
      if (existing) {
        selectedFile.value = existing
        toasts.info(`Selected ${existing.filename}`)
      } else {
        toasts.info('Opened dropped file', target)
      }
    }

    // Media: upload to the user's primary destination (plus any mirrors)
    // and copy markdown refs to the clipboard. The Rust `media` orchestrator
    // handles which destination's URL goes in the ref.
    if (mediaPaths.length) {
      // Folder derivation:
      //   1. If a post is selected, try to extract a YYYY from its source_dir
      //      ("blog/2026" → "blog/2026"). This is the common case for blog
      //      posts living in year folders.
      //   2. Else, if the selected post has a non-empty source_dir without a
      //      year (e.g. "blog/week-notes"), use that verbatim so screenshots
      //      land near the post they're for.
      //   3. Else, no post selected — surface a warning so the user knows
      //      assets are going to the Cloudinary root, then proceed.
      const folder = (() => {
        const sd = selectedFile.value?.source_dir
        if (!sd) return undefined
        const m = sd.match(/(\d{4})/)
        if (m) return `blog/${m[1]}`
        return sd // "blog/week-notes", "drafts", etc — preserve hierarchy
      })()
      if (!selectedFile.value) {
        toasts.warn(
          'No post selected',
          'Files will upload to the Cloudinary root. Click a post first to organize by folder.',
        )
      }

      const status: any = await invoke('check_media_status').catch(() => null)
      const primaryLabel = (status?.primary ?? 'cloudinary').toString()
      const mirrorCount = Array.isArray(status?.mirrors) ? status.mirrors.length : 0
      const destDesc =
        mirrorCount > 0 ? `${primaryLabel} (+ ${mirrorCount} mirror)` : primaryLabel
      // Surface the configured-creds problem early — otherwise the user
      // just sees N "Upload failed" toasts with no actionable hint.
      if (status && status[primaryLabel] === false) {
        toasts.error(
          `${primaryLabel} not configured`,
          `Open Settings → Media to paste your ${primaryLabel} credentials.`,
        )
        return
      }
      toasts.info(
        `Uploading ${mediaPaths.length} ${mediaPaths.length === 1 ? 'file' : 'files'} to ${destDesc}…`,
      )

      // Upload in parallel batches. Cloudinary's free tier is fine with
      // 3 concurrent uploads — more risks getting rate-limited. Sequential
      // (the old loop) made 6 screenshots take 6× longer than necessary.
      const CONCURRENCY = 3
      const refs: { idx: number; ref: string }[] = []
      let failures = 0
      let lastError: string | undefined
      const mirrorFailures: string[] = []

      async function uploadOne(p: string, idx: number) {
        try {
          const result: any = await invoke('media_upload', { filePath: p, folder })
          if (result?.success && result?.asset?.url) {
            const url = result.asset.url
            const isVideo = VIDEO_EXTS.includes(ext(p))
            refs.push({
              idx,
              ref: isVideo ? `<video src="${url}" controls></video>` : `![](${url})`,
            })
            for (const m of result.mirrors ?? []) {
              if (!m.success) {
                mirrorFailures.push(`${m.destination}: ${m.error ?? 'unknown'}`)
                console.warn('Mirror upload failed', m.destination, m.error)
              }
            }
          } else {
            failures++
            lastError = result?.error ?? 'unknown error'
            console.warn('media_upload returned failure', p, result?.error)
          }
        } catch (e: any) {
          failures++
          lastError = typeof e === 'string' ? e : (e?.message ?? String(e))
          console.warn('media_upload threw', p, e)
        }
      }

      // Worker-pool pattern: keep CONCURRENCY in flight at all times until
      // every path has been processed.
      let cursor = 0
      async function worker() {
        while (cursor < mediaPaths.length) {
          const i = cursor++
          await uploadOne(mediaPaths[i], i)
        }
      }
      await Promise.all(
        Array.from({ length: Math.min(CONCURRENCY, mediaPaths.length) }, () => worker()),
      )

      // Preserve original drop order in the clipboard payload.
      refs.sort((a, b) => a.idx - b.idx)

      if (refs.length) {
        await navigator.clipboard.writeText(refs.map((r) => r.ref).join('\n\n'))
        const subtitle: string[] = []
        if (failures) subtitle.push(`${failures} failed (${lastError ?? 'see console'})`)
        if (mirrorFailures.length) subtitle.push(`mirror: ${mirrorFailures.join(', ')}`)
        toasts.success(
          `${refs.length} uploaded — ⌘V in your editor`,
          subtitle.length ? subtitle.join(' · ') : undefined,
        )
      } else {
        toasts.error(
          'Upload failed',
          lastError ?? `${failures} file${failures === 1 ? '' : 's'} didn't upload`,
        )
      }
    }

    if (!mdPaths.length && !mediaPaths.length) {
      toasts.warn(
        'Unsupported file type',
        `Dispatch accepts .md, images, and video. Got: ${paths.map((p) => '.' + ext(p)).join(', ')}`,
      )
    }
  })
})

onUnmounted(() => {
  window.removeEventListener('keydown', handleGlobalKey)
  unlistenSchedule?.()
})
</script>

<template>
  <div
    class="app"
    :class="{ unfocused: !windowFocused, 'sidebar-collapsed': sidebarCollapsed, resizing: sidebarDragging }"
    :style="{ '--sidebar-width': sidebarWidth + 'px' }"
  >
    <!-- Drag divider between the file list and the right panel. Hidden when
         the sidebar is collapsed (⌘0) since there's no seam to grab. -->
    <ResizeHandle
      v-if="!sidebarCollapsed"
      axis="x"
      :active="sidebarDragging"
      class="sidebar-resize"
      data-tip="drag to resize · double-click to reset"
      @down="startSidebarResize"
      @reset="resetSidebarWidth"
    />
    <!-- Toast notifications (transient feedback for command outcomes) -->
    <ToastStack />

    <!-- Help Modal -->
    <HelpOverlay :show="showHelp" @close="showHelp = false" />

    <!-- Search Modal -->
    <SearchModal :show="searchOpen" :files="files" @close="closeSearch" @select="handleSearchSelect" />

    <!-- Command palette (⌘K). Files + actions in one fuzzy list. -->
    <CommandPalette
      :show="paletteOpen"
      :files="files"
      :selected-file="selectedFile"
      @close="paletteOpen = false"
      @select-file="
        (f) => {
          handleSearchSelect(f)
          paletteOpen = false
        }
      "
      @new-post="
        () => {
          paletteOpen = false
          openNewPost()
        }
      "
      @refresh="loadFiles"
      @show-help="showHelp = true"
      @toggle-panel="(p) => (rightTab = p)"
      @publish="
        () => {
          paletteOpen = false
          filePreviewRef?.openPublishConfirm?.(false)
        }
      "
    />

    <!-- New Post Modal -->
    <Transition name="modal">
      <div v-if="newPostOpen" class="search-overlay" @click.self="closeNewPost">
        <div class="search-modal new-post-modal">
          <div class="new-post-header">New Post</div>
          <input
            ref="newPostInput"
            v-model="newPostTitle"
            type="text"
            placeholder="Post title..."
            class="search-input"
            @keydown.enter="createNewPost"
            @keydown.escape="closeNewPost"
          />
          <div class="new-post-footer">
            <span class="new-post-slug" v-if="newPostTitle.trim()">
              blog/{{ new Date().getFullYear() }}/{{
                newPostTitle
                  .trim()
                  .toLowerCase()
                  .replace(/[^a-z0-9\s]/g, '')
                  .replace(/\s+/g, '-')
              }}.md
            </span>
            <div class="new-post-actions">
              <button @click="closeNewPost" class="np-btn">Cancel</button>
              <button @click="createNewPost" class="np-btn accent" :disabled="!newPostTitle.trim() || newPostCreating">
                {{ newPostCreating ? 'Creating...' : 'Create & Open' }}
              </button>
            </div>
          </div>
        </div>
      </div>
    </Transition>

    <main class="main">
      <FileList
        :files="files"
        :selected="selectedFile"
        :loading="loading"
        :compact="compactMode"
        @select="
          (f) => {
            selectedFile = f
            rightTab = 'preview'
          }
        "
      />

      <div class="right-panel">
        <!-- `@mousedown="startWindowDrag"` is required because `titleBarStyle:
             "Overlay"` breaks the CSS `-webkit-app-region: drag` path on
             macOS Sonoma+ (Tauri issues #9503, #11605). The data-* attribute
             stays as a hint, but the mousedown handler is what actually
             moves the window. -->
        <div class="panel-tabs" data-tauri-drag-region @mousedown="startWindowDrag">
          <!-- Leading icons on each tab match Apple Mail's Primary/Transactions
               header. Icon + label reads as a real macOS toolbar segment. -->
          <button :class="{ active: rightTab === 'preview' }" @click="rightTab = 'preview'">
            <Eye :size="13" />
            <span>Preview</span>
          </button>
          <button :class="{ active: rightTab === 'media' }" @click="rightTab = 'media'">
            <LucideImage :size="13" />
            <span>Media</span>
          </button>
          <button :class="{ active: rightTab === 'activity' }" @click="rightTab = 'activity'">
            <Activity :size="13" />
            <span>Activity</span>
          </button>
          <button :class="{ active: rightTab === 'modified' }" @click="rightTab = 'modified'">
            <Pencil :size="13" />
            <span>Modified</span>
            <span v-if="modifiedCount > 0" class="tab-badge">{{ modifiedCount }}</span>
          </button>
          <button :class="{ active: rightTab === 'journal' }" @click="rightTab = 'journal'">
            <BookOpen :size="13" />
            <span>Journal</span>
          </button>
          <button :class="{ active: rightTab === 'gear' }" @click="rightTab = 'gear'">
            <Backpack :size="13" />
            <span>Gear</span>
          </button>
          <div class="panel-tabs-spacer" data-tauri-drag-region @mousedown="startWindowDrag"></div>
          <div class="titlebar-btns">
            <button @click="openNewPost" class="titlebar-btn" data-tip="New Post">
              <Plus :size="16" />
            </button>
            <button @click="openSearch" class="titlebar-btn" data-tip="Search">
              <Search :size="16" />
            </button>
            <button @click="loadFiles" class="titlebar-btn" :class="{ spinning: loading }" data-tip="Refresh">
              <RefreshCw :size="16" />
            </button>
            <button @click="showSettings = true" class="titlebar-btn" data-tip="Settings">
              <Settings :size="16" />
            </button>
          </div>
        </div>

        <div class="panel-content">
          <FilePreview
            v-if="rightTab === 'preview' && selectedFile"
            ref="filePreviewRef"
            :file="selectedFile"
            @published="loadFiles"
            @jump-to-path="jumpToPath"
          />

          <div v-else-if="rightTab === 'preview'" class="empty">
            <div class="empty-icon">
              <FileText :size="48" :stroke-width="1" />
            </div>
            <div class="empty-title">Select a post</div>
            <div class="empty-shortcuts">
              <div class="shortcut-row">
                <kbd>j</kbd>
                <kbd>k</kbd>
                <span>navigate</span>
              </div>
              <div class="shortcut-row">
                <kbd>/</kbd>
                <span>search</span>
              </div>
              <div class="shortcut-row">
                <kbd>?</kbd>
                <span>all shortcuts</span>
              </div>
            </div>
          </div>

          <MediaLibraryModal
            v-else-if="rightTab === 'media'"
            :selected-file="selectedFile"
            :inline="true"
            @close="rightTab = 'preview'"
            @select="(asset) => copyToClipboard(asset.secure_url)"
            @insert="handleInsertMedia"
          />

          <ActivityFeed
            v-else-if="rightTab === 'activity'"
            :files="files"
            @select-file="(file) => (selectedFile = file)"
            @jump-to-slug="jumpToSlug"
          />

          <ModifiedPostsView
            v-else-if="rightTab === 'modified'"
            :files="files"
            @select-file="(file) => { selectedFile = file; rightTab = 'preview' }"
            @published="loadFiles"
          />

          <PublishingJournal v-else-if="rightTab === 'journal'" @jump-to-slug="jumpToSlug" />

          <GearPanel v-else-if="rightTab === 'gear'" />
        </div>
      </div>
    </main>

    <!-- Drag-drop overlay — appears full-window while files are being dragged
         over Dispatch. `pointer-events: none` so it never intercepts the
         mousedown→startDragging path. -->
    <Transition name="drag-overlay">
      <div v-if="dragActive" class="drag-overlay" :class="dragKind">
        <div class="drag-overlay-card">
          <div class="drag-overlay-icon">
            <PhArrowSquareDown v-if="dragKind === 'media'" :size="48" weight="duotone" />
            <PhFileText v-else-if="dragKind === 'markdown'" :size="48" weight="duotone" />
            <PhWarningCircle v-else :size="48" weight="duotone" />
          </div>
          <div class="drag-overlay-title">
            <template v-if="dragKind === 'media'">
              Drop to upload to {{ destinationLabel }}
            </template>
            <template v-else-if="dragKind === 'markdown'">
              Drop to open in Dispatch
            </template>
            <template v-else-if="dragKind === 'mixed'">
              Mixed files — markdown opens, media uploads
            </template>
            <template v-else>Dispatch can't handle this file type</template>
          </div>
          <div class="drag-overlay-sub">
            <template v-if="dragKind === 'media'">
              {{ dragCount }} file{{ dragCount === 1 ? '' : 's' }} ·
              <template v-if="selectedFile">going into <code>{{ folderHintForSelected }}</code></template>
              <template v-else>no post selected · will land at root</template>
            </template>
            <template v-else-if="dragKind === 'unsupported'">
              Dispatch accepts <code>.md</code>, images, and video.
            </template>
            <template v-else>
              {{ dragCount }} item{{ dragCount === 1 ? '' : 's' }}
            </template>
          </div>
        </div>
      </div>
    </Transition>

    <!-- Settings Modal -->
    <SettingsModal v-if="showSettings" @close="showSettings = false" @saved="onSettingsSaved" />

    <!-- Status Bar -->
    <div class="statusbar">
      <div class="status-left">
        <span
          v-if="selectedFile"
          class="status-item file-path"
          :data-tip="selectedFile.source_dir + '/' + selectedFile.filename"
        >
          <PhFolder :size="10" weight="duotone" />
          {{ selectedFile.source_dir }}/{{ selectedFile.filename }}
        </span>
        <span v-else class="status-item muted">No file selected</span>
        <span
          v-if="selectedFile"
          class="status-item muted tabular"
          :data-tip="selectedFile.word_count.toLocaleString() + ' words'"
        >
          <PhTextAa :size="10" weight="duotone" />
          {{ selectedFile.word_count.toLocaleString() }}
        </span>
        <span
          v-if="journalStats && journalStats.words_this_month > 0"
          class="status-item muted tabular"
          :data-tip="`${journalStats.words_this_month.toLocaleString()} words published this month`"
        >
          {{ (journalStats.words_this_month / 1000).toFixed(1) }}k/mo
        </span>
      </div>
      <div class="status-right">
        <button
          @click="compactMode = !compactMode"
          class="status-btn"
          :class="{ active: compactMode }"
          data-tip="Toggle compact view"
        >
          <PhRows v-if="compactMode" :size="11" weight="bold" />
          <PhSquaresFour v-else :size="11" weight="bold" />
        </button>
        <span class="status-divider"></span>
        <span
          class="status-item"
          :class="gitBranch ? 'connected' : 'muted'"
          :data-tip="gitBranch ? `Branch: ${gitBranch} · right-click for options` : 'Git not connected'"
          @contextmenu="showBranchMenu"
        >
          <PhGitBranch :size="11" weight="bold" />
          {{ gitBranch || '---' }}
        </span>
        <span
          class="status-item"
          :class="obsidianConnected ? 'connected' : 'muted'"
          :data-tip="obsidianConnected ? 'Obsidian vault connected · right-click for options' : 'Obsidian not detected'"
          @contextmenu="showVaultMenu"
        >
          <PhCircleWavy :size="10" :weight="obsidianConnected ? 'fill' : 'light'" />
          vault
        </span>
        <span
          class="status-item"
          :class="cloudinaryConnected ? 'connected' : 'muted'"
          :data-tip="cloudinaryConnected ? 'Cloudinary media connected · right-click for options' : 'Cloudinary not configured'"
          @contextmenu="showMediaMenu"
        >
          <PhImageSquare :size="10" :weight="cloudinaryConnected ? 'fill' : 'light'" />
          media
        </span>
        <span
          class="status-item"
          :class="analyticsConnected ? 'connected' : 'muted'"
          :data-tip="analyticsConnected ? 'Umami analytics connected · right-click for options' : 'Analytics not configured'"
          @contextmenu="showAnalyticsMenu"
        >
          <PhChartBar :size="10" :weight="analyticsConnected ? 'fill' : 'light'" />
          analytics
        </span>
        <span
          v-if="companionUrl"
          class="status-item connected"
          :data-tip="`Companion connected · PIN: ${companionPin} · right-click for options`"
          @contextmenu="showCompanionMenu"
        >
          <PhDeviceMobile :size="10" weight="fill" />
          {{ companionUrl.replace('http://', '') }}
        </span>
        <span class="status-divider"></span>
        <span class="status-item muted">v{{ appVersion }}</span>
      </div>
    </div>
  </div>
</template>

<style scoped>
/* === LAYOUT SYSTEM ===
   The entire app is a CSS Grid with fixed regions.
   Nothing inside needs to know about traffic lights or titlebar height.
   The grid handles it all.

   ┌──────────────┬──────────────────────────┐
   │  sidebar-bar │  panel-tabs              │  ← 44px titlebar row
   ├──────────────┼──────────────────────────┤
   │  sidebar     │  panel-content           │  ← fills remaining space
   ├──────────────┴──────────────────────────┤
   │  statusbar                              │  ← 22px status row
   └─────────────────────────────────────────┘
*/

.app {
  height: 100vh;
  display: grid;
  /* Width is user-draggable (see useResizable / ResizeHandle). Falls back to
     the old responsive clamp if the var isn't set yet. */
  grid-template-columns: var(--sidebar-width, clamp(240px, 38.2%, 400px)) 1fr;
  grid-template-rows: 44px 1fr 22px;
  grid-template-areas:
    'sidebar-bar  panel-bar'
    'sidebar      panel'
    'status       status';
  overflow: hidden;
  position: relative;
  transition: grid-template-columns 0.18s ease;
}

/* While actively dragging the divider, kill the column transition so the
   panel tracks the cursor 1:1 instead of easing behind it. The transition
   still applies to the ⌘0 collapse animation. */
.app.resizing {
  transition: none;
}

/* Park the divider on the sidebar/panel seam. The handle component is
   position-agnostic; here we anchor it absolutely on the grid seam. */
.sidebar-resize {
  position: absolute;
  top: 0;
  bottom: 0;
  left: var(--sidebar-width, 300px);
  transform: translateX(-50%);
}

/* ⌘0 collapses the sidebar. We zero out the first column AND hide the
   sidebar grid area entirely so the right panel takes the full window. */
.app.sidebar-collapsed {
  grid-template-columns: 0 1fr;
}
.app.sidebar-collapsed .sidebar,
.app.sidebar-collapsed .control-bar {
  visibility: hidden;
}

.main {
  display: contents;
}

.right-panel {
  display: contents;
}

/* --- Titlebar row (44px) ---
   IMPORTANT: this is the right-side window-drag region. The only drag-eligible
   space in this bar is (a) the 12px strip above the bottom-aligned buttons and
   (b) `.panel-tabs-spacer`. If you add another tab here, double-check the
   spacer still has real width on the narrowest window you support — otherwise
   the window becomes effectively non-draggable. */
.panel-tabs {
  grid-area: panel-bar;
  display: flex;
  align-items: flex-end;
  /* No bottom border — pill tabs float on the panel background; a divider
     under them reads as 90s tabbed-interface, not modern macOS. */
  background: var(--bg-secondary);
  -webkit-app-region: drag;
  /* Bigger top padding = fatter drag-stripe above the tab buttons.
     Buttons sit at the bottom (align-items: flex-end), so this entire
     padding-top region is window-drag. 18px gives you a comfortable
     grab even on a very narrow window. */
  padding-top: 18px;
  overflow: hidden;
}

.panel-tabs button {
  /* Pill-shaped tabs — translucent accent fill on the active one, hover
     state with a subtle wash. Same idiom Mail uses for its
     Primary/Transactions/Updates/Promotions header. */
  padding: 4px 9px;
  margin-bottom: 6px;
  font-size: 11px;
  font-weight: 500;
  flex-shrink: 0;
  background: transparent;
  border: none;
  border-radius: 6px;
  color: var(--text-tertiary);
  cursor: pointer;
  -webkit-app-region: no-drag;
  align-self: flex-end;
  transition: background 0.12s ease, color 0.12s ease;
  /* Icon + label sit on one baseline. */
  display: inline-flex;
  align-items: center;
  gap: 5px;
  line-height: 1;
}
.panel-tabs button > svg {
  flex-shrink: 0;
  opacity: 0.7;
  transition: opacity 0.12s ease;
}
.panel-tabs button.active > svg {
  opacity: 1;
}

.panel-tabs button:hover {
  color: var(--text-secondary);
  background: color-mix(in srgb, var(--text-primary) 5%, transparent);
}

.panel-tabs button.active {
  color: var(--text-primary);
  background: color-mix(in srgb, var(--accent) 26%, transparent);
  font-weight: 600;
}
.panel-tabs button.active:hover {
  background: color-mix(in srgb, var(--accent) 32%, transparent);
}

/* Numeric badge nested inside a panel-tab button (e.g. "Modified · 12"). */
.panel-tabs button .tab-badge {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  min-width: 16px;
  height: 14px;
  padding: 0 4px;
  margin-left: 4px;
  font-size: 9px;
  font-weight: 600;
  border-radius: 999px;
  background: color-mix(in srgb, var(--warning) 22%, transparent);
  color: var(--warning);
  font-variant-numeric: tabular-nums;
  line-height: 1;
}

/* The middle of the title-bar is the primary window-drag handle. Reserve a
   firm minimum so it survives every responsive width and doesn't get
   squeezed to zero by tabs/buttons. The explicit `-webkit-app-region: drag`
   is load-bearing — Tauri 2's `data-tauri-drag-region` attribute alone is
   unreliable in dev mode; the CSS rule is what actually activates the
   OS-level drag handle. */
.panel-tabs-spacer {
  flex: 1;
  min-width: 120px;
  align-self: stretch;
  -webkit-app-region: drag;
}

.titlebar-btns {
  -webkit-app-region: no-drag;
  display: flex;
  /* Slightly more breathing room between buttons — closer to NSToolbar's
     ~4px spacing than the previous 2px shoulder-to-shoulder pack. */
  gap: 4px;
  /* Mirror the 12px trafficLightPosition inset on the left so the trailing
     buttons clear the window's rounded corner with the same visual margin.
     8px wasn't enough once the buttons grew to 30px. */
  margin-right: 12px;
  align-self: center;
  margin-bottom: 4px;
  flex-shrink: 0;
}

.titlebar-btn {
  background: none;
  border: none;
  color: var(--text-tertiary);
  /* NSToolbar-ish sizing — 30px hit target, 16px glyph (was a too-cute
     26/13). Matches Finder/Mail's titlebar buttons. */
  width: 30px;
  height: 30px;
  border-radius: 6px;
  cursor: pointer;
  display: flex;
  align-items: center;
  justify-content: center;
  transition: all var(--transition-fast);
  -webkit-app-region: no-drag;
}

.titlebar-btn:hover {
  background: var(--bg-tertiary);
  color: var(--text-primary);
}

.titlebar-btn:active {
  transform: scale(0.92);
}

.titlebar-btn.active {
  color: var(--text-primary);
  background: var(--hover-bg);
}

.titlebar-btn.spinning :deep(svg) {
  animation: spin 1s linear infinite;
}

.panel-content {
  grid-area: panel;
  overflow: hidden;
  display: flex;
  flex-direction: column;
  /* Opaque so the right pane reads as a solid reading surface — only the
     left sidebar shows the window's vibrancy material. Same model as
     Apple Mail: vibrant source list + opaque content panes. */
  background: var(--bg-solid);
}

.empty {
  flex: 1;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 16px;
  color: var(--text-tertiary);
  user-select: none;
}

.empty-icon {
  opacity: 0.15;
  margin-bottom: 4px;
}

.empty-title {
  font-size: 14px;
  font-weight: 500;
  color: var(--text-secondary);
}

.empty-shortcuts {
  display: flex;
  flex-direction: column;
  gap: 6px;
  margin-top: 8px;
}

.shortcut-row {
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: 11px;
  color: var(--text-tertiary);
}

.shortcut-row span {
  margin-left: 4px;
  opacity: 0.7;
}

/* Status Bar */
.statusbar {
  grid-area: status;
  background: var(--bg-secondary);
  border-top: 1px solid var(--border);
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 0 10px;
  font-size: 10px;
  font-family: 'SF Mono', monospace;
  color: var(--text-tertiary);
  flex-shrink: 0;
}

.status-left,
.status-right {
  display: flex;
  align-items: center;
  gap: 8px;
}

.status-item {
  display: flex;
  align-items: center;
  gap: 3px;
}

.status-item.connected {
  color: var(--success);
}

.status-item.muted {
  opacity: 0.5;
}

.status-item.file-path {
  color: var(--text-secondary);
  max-width: 300px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.status-item.tabular {
  font-variant-numeric: tabular-nums;
  font-feature-settings: 'tnum';
}

.status-divider {
  width: 1px;
  height: 10px;
  background: var(--border-light);
}

.status-item.connected :deep(svg) {
  color: var(--success);
}

.status-item.muted :deep(svg) {
  opacity: 0.4;
}

.status-btn {
  background: none;
  border: none;
  color: var(--text-tertiary);
  cursor: pointer;
  padding: 0 2px;
  display: flex;
  align-items: center;
  transition: all var(--transition-fast);
  border-radius: 3px;
}

.status-btn:hover {
  background: var(--hover-bg);
  color: var(--text-secondary);
}

.status-btn.active {
  color: var(--text-primary);
}

/* Keep tooltips from overflowing left edge */
.status-left :deep([data-tip])::after {
  left: 0;
  transform: translateX(0) translateY(-2px);
}

.status-left :deep([data-tip]):hover::after {
  transform: translateX(0) translateY(0);
}

/* Keep tooltips from overflowing right edge */
.status-right :deep([data-tip])::after {
  left: auto;
  right: 0;
  transform: translateX(0) translateY(-2px);
}

.status-right :deep([data-tip]):hover::after {
  transform: translateX(0) translateY(0);
}

/* Titlebar Stats */
/* New Post Modal */
.new-post-header {
  padding: 12px 16px 0;
  font-size: 12px;
  font-weight: 600;
  color: var(--text-secondary);
  text-transform: uppercase;
  letter-spacing: 0.5px;
}

.new-post-footer {
  padding: 10px 16px;
  border-top: 1px solid var(--border);
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
}

.new-post-slug {
  font-size: 10px;
  font-family: 'SF Mono', monospace;
  color: var(--text-tertiary);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  max-width: 250px;
}

.new-post-actions {
  display: flex;
  gap: 6px;
  flex-shrink: 0;
}

.np-btn {
  padding: 5px 12px;
  font-size: 11px;
  font-weight: 500;
  border: 1px solid var(--border-light);
  border-radius: 6px;
  background: transparent;
  color: var(--text-secondary);
  cursor: pointer;
  transition: all var(--transition-fast);
}

.np-btn:hover {
  background: var(--hover-bg);
  color: var(--text-primary);
}

.np-btn.accent {
  background: var(--selection-bg);
  color: #fff;
  border-color: transparent;
}

.np-btn.accent:hover {
  filter: brightness(1.1);
}

.np-btn:disabled {
  opacity: 0.4;
  cursor: default;
}

/* Native macOS window unfocused dimming */
.app.unfocused .sidebar,
.app.unfocused .panel-tabs,
.app.unfocused .statusbar {
  opacity: 0.55;
  transition: opacity 0.2s ease;
}

.sidebar,
.panel-tabs,
.statusbar {
  transition: opacity 0.2s ease;
}

/* --- Drag-drop overlay -------------------------------------------------
   Full-window scrim + centered card that tells the user what's about to
   happen when they drop. Mounted only while a drag is over the window.
   Critically, the overlay must NOT capture pointer/drag events itself —
   otherwise Tauri's drop event wouldn't fire on the actual drop. */
.drag-overlay {
  position: fixed;
  inset: 0;
  z-index: 500;
  display: flex;
  align-items: center;
  justify-content: center;
  background: color-mix(in srgb, var(--accent) 18%, rgba(0, 0, 0, 0.55));
  pointer-events: none;
}

.drag-overlay.media {
  background: color-mix(in srgb, var(--accent) 22%, rgba(0, 0, 0, 0.6));
}
.drag-overlay.markdown {
  background: color-mix(in srgb, var(--success) 18%, rgba(0, 0, 0, 0.55));
}
.drag-overlay.unsupported {
  background: color-mix(in srgb, var(--danger) 22%, rgba(0, 0, 0, 0.55));
}

.drag-overlay-card {
  padding: 32px 40px;
  background: var(--bg-secondary);
  border: 2px dashed var(--accent);
  border-radius: 12px;
  text-align: center;
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 10px;
  max-width: 480px;
  box-shadow: var(--shadow-lg, 0 24px 48px rgba(0, 0, 0, 0.4));
}
.drag-overlay.markdown .drag-overlay-card {
  border-color: var(--success);
}
.drag-overlay.unsupported .drag-overlay-card {
  border-color: var(--danger);
}

.drag-overlay-icon {
  color: var(--accent);
  animation: drag-overlay-pulse 1.6s ease-in-out infinite;
}
.drag-overlay.markdown .drag-overlay-icon {
  color: var(--success);
}
.drag-overlay.unsupported .drag-overlay-icon {
  color: var(--danger);
  animation: none;
}

@keyframes drag-overlay-pulse {
  0%,
  100% {
    transform: translateY(0);
    opacity: 1;
  }
  50% {
    transform: translateY(-3px);
    opacity: 0.75;
  }
}

.drag-overlay-title {
  font-size: 15px;
  font-weight: 600;
  color: var(--text-primary);
  line-height: 1.3;
}

.drag-overlay-sub {
  font-size: 12px;
  color: var(--text-secondary);
  line-height: 1.4;
}
.drag-overlay-sub code {
  background: var(--bg-tertiary);
  padding: 1px 6px;
  border-radius: 3px;
  font-size: 11px;
  font-family: 'SF Mono', monospace;
  color: var(--text-primary);
}

.drag-overlay-enter-active,
.drag-overlay-leave-active {
  transition: opacity 0.12s ease, transform 0.12s ease;
}
.drag-overlay-enter-active .drag-overlay-card,
.drag-overlay-leave-active .drag-overlay-card {
  transition: transform 0.18s var(--ease-spring, cubic-bezier(0.34, 1.56, 0.64, 1));
}
.drag-overlay-enter-from,
.drag-overlay-leave-to {
  opacity: 0;
}
.drag-overlay-enter-from .drag-overlay-card {
  transform: scale(0.95) translateY(8px);
}
.drag-overlay-leave-to .drag-overlay-card {
  transform: scale(0.98);
}
</style>
