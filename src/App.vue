<script setup lang="ts">
import { ref, onMounted, onUnmounted, nextTick } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { FileText, Search, RefreshCw, Settings, Plus } from 'lucide-vue-next'
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
} from '@phosphor-icons/vue'
import FileList from './components/FileList.vue'
import FilePreview from './components/FilePreview.vue'
import MediaLibraryModal from './components/Media/MediaLibraryModal.vue'
import PublishingJournal from './components/PublishingJournal.vue'
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

// Right panel tab state
const rightTab = ref<'preview' | 'media' | 'journal' | 'gear'>('preview')

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

async function loadFiles() {
  loading.value = true
  try {
    files.value = await invoke('get_recent_files', { limit: 200 })
  } catch (e) {
    console.error('Failed to load files:', e)
  }
  loading.value = false
  // Refresh journal stats (publish may have just happened)
  refreshJournalStats()
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
}

onMounted(async () => {
  loadFiles()
  refreshJournalStats()
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

  // Drag-in: a .md dropped from Finder selects it (or warns if not .md).
  // Tauri's native drag-drop event is fired by WebviewWindow.
  getCurrentWindow().onDragDropEvent((event) => {
    if (event.payload.type !== 'drop') return
    const paths = event.payload.paths || []
    const mdPaths = paths.filter((p) => p.toLowerCase().endsWith('.md'))
    const skipped = paths.length - mdPaths.length

    if (mdPaths.length === 0) {
      toasts.warn(
        'Only .md files supported',
        skipped > 0 ? `Ignored ${skipped} non-markdown file${skipped === 1 ? '' : 's'}` : undefined,
      )
      return
    }

    // Pick the first dropped .md and surface it. If it's already in the
    // file list, just select it; otherwise, open it as-is (the file will
    // appear in the list on next refresh if it's inside the vault).
    const target = mdPaths[0]
    const existing = files.value.find((f) => f.path === target)
    if (existing) {
      selectedFile.value = existing
      toasts.info(`Selected ${existing.filename}`)
    } else {
      toasts.info('Opened dropped file', target)
      // Best-effort: the user may have dropped a file from outside the vault.
      // We don't auto-import; the toast detail makes the path visible.
    }
  })
})

onUnmounted(() => {
  window.removeEventListener('keydown', handleGlobalKey)
  unlistenSchedule?.()
})
</script>

<template>
  <div class="app" :class="{ unfocused: !windowFocused }">
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
        <div class="panel-tabs" data-tauri-drag-region>
          <button :class="{ active: rightTab === 'preview' }" @click="rightTab = 'preview'">Preview</button>
          <button :class="{ active: rightTab === 'media' }" @click="rightTab = 'media'">Media</button>
          <button :class="{ active: rightTab === 'journal' }" @click="rightTab = 'journal'">Journal</button>
          <button :class="{ active: rightTab === 'gear' }" @click="rightTab = 'gear'">Gear</button>
          <div class="panel-tabs-spacer" data-tauri-drag-region></div>
          <div class="titlebar-btns">
            <button @click="openNewPost" class="titlebar-btn" data-tip="New Post">
              <Plus :size="13" />
            </button>
            <button @click="openSearch" class="titlebar-btn" data-tip="Search">
              <Search :size="13" />
            </button>
            <button @click="loadFiles" class="titlebar-btn" :class="{ spinning: loading }" data-tip="Refresh">
              <RefreshCw :size="13" />
            </button>
            <button @click="showSettings = true" class="titlebar-btn" data-tip="Settings">
              <Settings :size="13" />
            </button>
          </div>
        </div>

        <div class="panel-content">
          <FilePreview
            v-if="rightTab === 'preview' && selectedFile"
            ref="filePreviewRef"
            :file="selectedFile"
            @published="loadFiles"
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

          <PublishingJournal v-else-if="rightTab === 'journal'" />

          <GearPanel v-else-if="rightTab === 'gear'" />
        </div>
      </div>
    </main>

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
          :data-tip="gitBranch ? `Branch: ${gitBranch}` : 'Git not connected'"
        >
          <PhGitBranch :size="11" weight="bold" />
          {{ gitBranch || '---' }}
        </span>
        <span
          class="status-item"
          :class="obsidianConnected ? 'connected' : 'muted'"
          :data-tip="obsidianConnected ? 'Obsidian vault connected' : 'Obsidian not detected'"
        >
          <PhCircleWavy :size="10" :weight="obsidianConnected ? 'fill' : 'light'" />
          vault
        </span>
        <span
          class="status-item"
          :class="cloudinaryConnected ? 'connected' : 'muted'"
          :data-tip="cloudinaryConnected ? 'Cloudinary media connected' : 'Cloudinary not configured'"
        >
          <PhImageSquare :size="10" :weight="cloudinaryConnected ? 'fill' : 'light'" />
          media
        </span>
        <span
          class="status-item"
          :class="analyticsConnected ? 'connected' : 'muted'"
          :data-tip="analyticsConnected ? 'Umami analytics connected' : 'Analytics not configured'"
        >
          <PhChartBar :size="10" :weight="analyticsConnected ? 'fill' : 'light'" />
          analytics
        </span>
        <span
          v-if="companionUrl"
          class="status-item connected"
          :data-tip="`Companion connected · PIN: ${companionPin}`"
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
  grid-template-columns: clamp(240px, 38.2%, 400px) 1fr;
  grid-template-rows: 44px 1fr 22px;
  grid-template-areas:
    'sidebar-bar  panel-bar'
    'sidebar      panel'
    'status       status';
  overflow: hidden;
}

.main {
  display: contents;
}

.right-panel {
  display: contents;
}

/* --- Titlebar row (44px) --- */
.panel-tabs {
  grid-area: panel-bar;
  display: flex;
  align-items: flex-end;
  border-bottom: 1px solid var(--border);
  background: var(--bg-secondary);
  -webkit-app-region: drag;
  padding-top: 8px;
  overflow: hidden;
}

.panel-tabs button {
  padding: 6px 12px;
  font-size: 11px;
  font-weight: 500;
  flex-shrink: 0;
  background: transparent;
  border: none;
  color: var(--text-tertiary);
  cursor: pointer;
  border-bottom: 1.5px solid transparent;
  margin-bottom: -1px;
  -webkit-app-region: no-drag;
  align-self: flex-end;
  transition: color 0.15s ease;
}

.panel-tabs button:hover {
  color: var(--text-secondary);
}

.panel-tabs button.active {
  color: var(--text-primary);
  border-bottom-color: var(--selection-bg);
  font-weight: 600;
}

.panel-tabs-spacer {
  flex: 1;
}

.titlebar-btns {
  -webkit-app-region: no-drag;
  display: flex;
  gap: 2px;
  margin-right: 8px;
  align-self: center;
  margin-bottom: 4px;
  flex-shrink: 0;
}

.titlebar-btn {
  background: none;
  border: none;
  color: var(--text-tertiary);
  width: 26px;
  height: 26px;
  border-radius: 5px;
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
  background: var(--accent);
}

.titlebar-btn.spinning :deep(svg) {
  animation: spin 1s linear infinite;
}

.panel-content {
  grid-area: panel;
  overflow: hidden;
  display: flex;
  flex-direction: column;
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
  background: var(--accent);
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
  background: var(--accent);
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
</style>
