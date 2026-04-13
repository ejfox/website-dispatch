<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted, nextTick } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { FileText, Search, RefreshCw, Zap, Settings, Plus } from 'lucide-vue-next'
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
  PhFlame,
} from '@phosphor-icons/vue'
import FileList from './components/FileList.vue'
import FilePreview from './components/FilePreview.vue'
import MediaLibraryModal from './components/Media/MediaLibraryModal.vue'
import PublishingJournal from './components/PublishingJournal.vue'
import SettingsModal from './components/SettingsModal.vue'
import SearchModal from './components/SearchModal.vue'
import HelpOverlay from './components/HelpOverlay.vue'
import { useLocalStorage } from '@vueuse/core'
import type { MarkdownFile } from './types'
import { useKeyboardShortcuts } from './composables/useKeyboardShortcuts'
import { useAppConfig } from './composables/useAppConfig'
import { useConnectionStatus } from './composables/useConnectionStatus'

const files = ref<MarkdownFile[]>([])
const selectedFile = ref<MarkdownFile | null>(null)
const loading = ref(true)

// Search state
const searchOpen = ref(false)

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
const rightTab = ref<'preview' | 'media' | 'journal'>('preview')

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

// Stats computations
const stats = computed(() => {
  const now = Date.now() / 1000
  const dayAgo = now - 86400
  const weekAgo = now - 86400 * 7

  const liveFiles = files.value.filter((f) => f.published_url)
  const todayPublished = liveFiles.filter((f) => f.published_date && f.published_date > dayAgo)
  const weekPublished = liveFiles.filter((f) => f.published_date && f.published_date > weekAgo)

  return {
    total: files.value.length,
    live: liveFiles.length,
    drafts: files.value.length - liveFiles.length,
    totalWords: files.value.reduce((sum, f) => sum + f.word_count, 0),
    todayPublished: todayPublished.length,
    weekPublished: weekPublished.length,
    weekWords: weekPublished.reduce((sum, f) => sum + f.word_count, 0),
  }
})

function openSearch() {
  searchOpen.value = true
}

function closeSearch() {
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
})

onUnmounted(() => {
  window.removeEventListener('keydown', handleGlobalKey)
  unlistenSchedule?.()
})
</script>

<template>
  <div class="app" :class="{ unfocused: !windowFocused }">
    <!-- Help Modal -->
    <HelpOverlay :show="showHelp" @close="showHelp = false" />

    <!-- Search Modal -->
    <SearchModal :show="searchOpen" :files="files" @close="closeSearch" @select="handleSearchSelect" />

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
        v-if="rightTab === 'preview'"
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
          <div class="titlebar-stats" v-if="journalStats" data-tauri-drag-region>
            <span v-if="journalStats.current_streak_days >= 2" class="streak-pill">
              <PhFlame :size="9" weight="fill" />
              {{ journalStats.current_streak_days }}d
            </span>
            <Zap :size="10" />
            <span>{{ journalStats.publishes_this_week || stats.weekPublished }} this week</span>
          </div>
          <div class="titlebar-stats" v-else-if="stats.weekPublished > 0" data-tauri-drag-region>
            <Zap :size="10" />
            <span>{{ stats.weekPublished }} this week</span>
          </div>
          <div class="panel-tabs-spacer" data-tauri-drag-region></div>
          <div class="titlebar-btns">
            <button @click="openNewPost" class="titlebar-btn" title="New Post (n)">
              <Plus :size="13" />
            </button>
            <button @click="openSearch" class="titlebar-btn" title="Search (⌘K)">
              <Search :size="13" />
            </button>
            <button @click="loadFiles" class="titlebar-btn" :class="{ spinning: loading }" title="Refresh (r)">
              <RefreshCw :size="13" />
            </button>
            <button @click="showSettings = true" class="titlebar-btn" title="Settings (,)">
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
      </div>
    </div>
  </div>
</template>

<style scoped>
.app {
  height: 100vh;
  display: flex;
  flex-direction: column;
}

.main {
  flex: 1;
  display: flex;
  overflow: hidden;
}

.right-panel {
  flex: 0.618; /* Golden ratio: larger section */
  display: flex;
  flex-direction: column;
  overflow: hidden;
}

.panel-tabs {
  display: flex;
  align-items: flex-end;
  border-bottom: 1px solid var(--border);
  background: var(--bg-secondary);
  flex-shrink: 0;
  -webkit-app-region: drag;
  height: 44px;
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
  margin-top: auto;
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
  flex: 1;
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
  height: 22px;
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
.titlebar-stats {
  -webkit-app-region: no-drag;
  display: flex;
  align-items: center;
  gap: 4px;
  font-size: 10px;
  color: var(--success);
  background: color-mix(in srgb, var(--success) 15%, transparent);
  padding: 2px 8px;
  border-radius: 10px;
  margin-left: 8px;
  flex-shrink: 1;
  overflow: hidden;
  white-space: nowrap;
  min-width: 0;
}

.streak-pill {
  display: inline-flex;
  align-items: center;
  gap: 2px;
  color: #f59e0b;
  font-weight: 600;
  font-variant-numeric: tabular-nums;
  margin-right: 2px;
}

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
