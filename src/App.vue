<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted, nextTick } from 'vue'
import { invoke } from '@tauri-apps/api/tauri'
import { FileText, Search, RefreshCw, Image, GitBranch, Circle, Zap } from 'lucide-vue-next'
import FileList from './components/FileList.vue'
import FilePreview from './components/FilePreview.vue'
import MediaLibraryModal from './components/Media/MediaLibraryModal.vue'

interface MarkdownFile {
  path: string
  filename: string
  title: string | null
  dek: string | null
  date: string | null
  tags: string[]
  created: number
  modified: number
  word_count: number
  is_safe: boolean
  warnings: string[]
  published_url: string | null
  published_date: number | null
  source_dir: string
  unlisted: boolean
  password: string | null
}

const files = ref<MarkdownFile[]>([])
const selectedFile = ref<MarkdownFile | null>(null)
const loading = ref(true)

// Search state
const searchOpen = ref(false)
const searchQuery = ref('')
const searchInput = ref<HTMLInputElement | null>(null)
const selectedIndex = ref(0)

// Help & keyboard state
const showHelp = ref(false)
const lastGPress = ref(0)

// Right panel tab state
const rightTab = ref<'preview' | 'media'>('preview')
const cloudinaryConnected = ref(false)
const obsidianConnected = ref(false)
const gitBranch = ref<string | null>(null)

// Compact mode preference
const compactMode = ref(localStorage.getItem('dispatch-compact') === 'true')
function toggleCompactMode() {
  compactMode.value = !compactMode.value
  localStorage.setItem('dispatch-compact', compactMode.value.toString())
}


const searchResults = computed(() => {
  if (!searchQuery.value.trim()) return files.value.slice(0, 20)
  const q = searchQuery.value.toLowerCase()
  return files.value
    .filter(f => {
      const title = (f.title || f.filename).toLowerCase()
      const tags = f.tags.join(' ').toLowerCase()
      return title.includes(q) || tags.includes(q) || f.filename.toLowerCase().includes(q)
    })
    .slice(0, 20)
})

// Stats computations
const stats = computed(() => {
  const now = Date.now() / 1000
  const dayAgo = now - 86400
  const weekAgo = now - 86400 * 7

  const liveFiles = files.value.filter(f => f.published_url)
  const todayPublished = liveFiles.filter(f => f.published_date && f.published_date > dayAgo)
  const weekPublished = liveFiles.filter(f => f.published_date && f.published_date > weekAgo)

  return {
    total: files.value.length,
    live: liveFiles.length,
    drafts: files.value.length - liveFiles.length,
    totalWords: files.value.reduce((sum, f) => sum + f.word_count, 0),
    todayPublished: todayPublished.length,
    weekPublished: weekPublished.length,
    weekWords: weekPublished.reduce((sum, f) => sum + f.word_count, 0)
  }
})

function openSearch() {
  searchOpen.value = true
  searchQuery.value = ''
  selectedIndex.value = 0
  setTimeout(() => searchInput.value?.focus(), 10)
}

function closeSearch() {
  searchOpen.value = false
  searchQuery.value = ''
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
      content: markdown
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

function selectResult(file: MarkdownFile) {
  selectedFile.value = file
  closeSearch()
}

function handleSearchKey(e: KeyboardEvent) {
  if (e.key === 'Escape') {
    closeSearch()
  } else if (e.key === 'ArrowDown') {
    e.preventDefault()
    selectedIndex.value = Math.min(selectedIndex.value + 1, searchResults.value.length - 1)
  } else if (e.key === 'ArrowUp') {
    e.preventDefault()
    selectedIndex.value = Math.max(selectedIndex.value - 1, 0)
  } else if (e.key === 'Enter') {
    e.preventDefault()
    const result = searchResults.value[selectedIndex.value]
    if (result) selectResult(result)
  }
}

function handleGlobalKey(e: KeyboardEvent) {
  // ⌘K or / for search
  if ((e.metaKey || e.ctrlKey) && e.key === 'k') {
    e.preventDefault()
    if (searchOpen.value) closeSearch()
    else openSearch()
    return
  }

  // Don't handle navigation when search is open
  if (searchOpen.value) {
    if (e.key === 'Escape') closeSearch()
    return
  }

  // / also opens search (vim style)
  if (e.key === '/' && !e.metaKey && !e.ctrlKey) {
    e.preventDefault()
    openSearch()
    return
  }

  // ? shows help
  if (e.key === '?' && e.shiftKey) {
    e.preventDefault()
    showHelp.value = !showHelp.value
    return
  }

  const currentIndex = selectedFile.value
    ? files.value.findIndex(f => f.path === selectedFile.value?.path)
    : -1

  // Arrow navigation through file list
  if (e.key === 'ArrowDown' || e.key === 'j') {
    e.preventDefault()
    const nextIndex = Math.min(currentIndex + 1, files.value.length - 1)
    selectedFile.value = files.value[nextIndex]
  }

  if (e.key === 'ArrowUp' || e.key === 'k') {
    e.preventDefault()
    const prevIndex = Math.max(currentIndex - 1, 0)
    selectedFile.value = files.value[prevIndex]
  }

  // g g - go to top (vim style, track double-tap)
  if (e.key === 'g' && !e.metaKey && !e.ctrlKey) {
    const now = Date.now()
    if (lastGPress.value && now - lastGPress.value < 300) {
      selectedFile.value = files.value[0]
      lastGPress.value = 0
    } else {
      lastGPress.value = now
    }
  }

  // G - go to bottom
  if (e.key === 'G' && e.shiftKey) {
    e.preventDefault()
    selectedFile.value = files.value[files.value.length - 1]
  }

  // [ and ] - prev/next with wrap
  if (e.key === '[') {
    const prevIndex = currentIndex <= 0 ? files.value.length - 1 : currentIndex - 1
    selectedFile.value = files.value[prevIndex]
  }
  if (e.key === ']') {
    const nextIndex = currentIndex >= files.value.length - 1 ? 0 : currentIndex + 1
    selectedFile.value = files.value[nextIndex]
  }

  // 1-9 jump to nth file
  if (e.key >= '1' && e.key <= '9' && !e.metaKey && !e.ctrlKey) {
    const idx = parseInt(e.key) - 1
    if (idx < files.value.length) {
      selectedFile.value = files.value[idx]
    }
  }

  // Escape to deselect
  if (e.key === 'Escape') {
    selectedFile.value = null
    showHelp.value = false
  }

  // o - open in Obsidian
  if (e.key === 'o' && selectedFile.value && !e.metaKey) {
    invoke('open_in_obsidian', { path: selectedFile.value.path })
  }

  // i - open in iA Writer
  if (e.key === 'i' && selectedFile.value) {
    invoke('open_in_app', { path: selectedFile.value.path, app: 'iA Writer' })
  }

  // p - open preview
  if (e.key === 'p' && selectedFile.value && !e.metaKey) {
    invoke('open_preview')
  }

  // v - view on site (if published)
  if (e.key === 'v' && selectedFile.value?.published_url) {
    window.open(selectedFile.value.published_url, '_blank')
  }

  // c - copy URL (if published)
  if (e.key === 'c' && selectedFile.value?.published_url && !e.metaKey) {
    navigator.clipboard.writeText(selectedFile.value.published_url)
  }

  // r - refresh
  if (e.key === 'r' && !e.metaKey && !e.ctrlKey) {
    loadFiles()
  }

  // m - toggle media library tab
  if (e.key === 'm' && !e.metaKey && !e.ctrlKey) {
    e.preventDefault()
    rightTab.value = rightTab.value === 'media' ? 'preview' : 'media'
  }

  // ⌘Enter to publish
  if ((e.metaKey || e.ctrlKey) && e.key === 'Enter' && selectedFile.value?.is_safe) {
    e.preventDefault()
    invoke('publish_file', {
      sourcePath: selectedFile.value.path,
      slug: selectedFile.value.filename.replace('.md', '')
    }).then(() => loadFiles())
  }
}

async function loadFiles() {
  loading.value = true
  try {
    files.value = await invoke('get_recent_files', { limit: 200 })
  } catch (e) {
    console.error('Failed to load files:', e)
  }
  loading.value = false
}

onMounted(() => {
  loadFiles()
  window.addEventListener('keydown', handleGlobalKey)

  // Check connection statuses
  invoke('check_cloudinary_status').then((connected: unknown) => {
    cloudinaryConnected.value = connected as boolean
  }).catch(() => {
    cloudinaryConnected.value = false
  })

  invoke('check_obsidian_api').then((connected: unknown) => {
    obsidianConnected.value = connected as boolean
  }).catch(() => {
    obsidianConnected.value = false
  })

  invoke('get_git_status').then((status: any) => {
    if (status?.ok) {
      gitBranch.value = status.branch
    }
  }).catch(() => {})
})

onUnmounted(() => {
  window.removeEventListener('keydown', handleGlobalKey)
})
</script>

<template>
  <div class="app">
    <div class="titlebar" data-tauri-drag-region>
      <div class="titlebar-spacer"></div>
      <div class="titlebar-stats" v-if="stats.weekPublished > 0">
        <Zap :size="10" />
        <span>{{ stats.weekPublished }} this week</span>
      </div>
      <div class="titlebar-btns">
        <button @click="rightTab = rightTab === 'media' ? 'preview' : 'media'" class="titlebar-btn" :class="{ active: rightTab === 'media' }" title="Media Library (m)">
          <Image :size="14" />
        </button>
        <button @click="openSearch" class="titlebar-btn" title="Search (⌘K)">
          <Search :size="14" />
        </button>
        <button @click="loadFiles" class="titlebar-btn" :class="{ spinning: loading }" title="Refresh (r)">
          <RefreshCw :size="14" />
        </button>
      </div>
    </div>

    <!-- Help Modal -->
    <div v-if="showHelp" class="search-overlay" @click.self="showHelp = false">
      <div class="help-modal">
        <div class="help-title">Keyboard Shortcuts</div>
        <div class="help-grid">
          <div class="help-section">
            <div class="help-section-title">Navigation</div>
            <div class="help-row"><kbd>↑</kbd> <kbd>↓</kbd> or <kbd>j</kbd> <kbd>k</kbd> <span>move up/down</span></div>
            <div class="help-row"><kbd>g</kbd><kbd>g</kbd> <span>go to top</span></div>
            <div class="help-row"><kbd>G</kbd> <span>go to bottom</span></div>
            <div class="help-row"><kbd>[</kbd> <kbd>]</kbd> <span>prev/next (wrap)</span></div>
            <div class="help-row"><kbd>1</kbd>-<kbd>9</kbd> <span>jump to nth file</span></div>
            <div class="help-row"><kbd>esc</kbd> <span>deselect</span></div>
          </div>
          <div class="help-section">
            <div class="help-section-title">Actions</div>
            <div class="help-row"><kbd>⌘</kbd><kbd>K</kbd> or <kbd>/</kbd> <span>search</span></div>
            <div class="help-row"><kbd>o</kbd> <span>open in Obsidian</span></div>
            <div class="help-row"><kbd>i</kbd> <span>open in iA Writer</span></div>
            <div class="help-row"><kbd>p</kbd> <span>preview</span></div>
            <div class="help-row"><kbd>v</kbd> <span>view on site</span></div>
            <div class="help-row"><kbd>c</kbd> <span>copy URL</span></div>
            <div class="help-row"><kbd>r</kbd> <span>refresh</span></div>
            <div class="help-row"><kbd>m</kbd> <span>media library</span></div>
            <div class="help-row"><kbd>⌘</kbd><kbd>↵</kbd> <span>publish</span></div>
          </div>
        </div>
        <div class="help-divider"></div>
        <div class="help-section-title">Visibility Spectrum</div>
        <div class="visibility-spectrum">
          <div class="vis-row"><span class="vis-badge vis-public">✓ PUBLIC</span> <span class="vis-desc">Appears in listings, feeds, search</span></div>
          <div class="vis-row"><span class="vis-badge vis-unlisted">👁 UNLISTED</span> <span class="vis-desc">Link only — add <code>unlisted: true</code></span></div>
          <div class="vis-row"><span class="vis-badge vis-protected">🔒 PROTECTED</span> <span class="vis-desc">Link + password — add <code>password: xyz</code></span></div>
        </div>
        <div class="help-hint">Press <kbd>?</kbd> or <kbd>esc</kbd> to close</div>
      </div>
    </div>


    <!-- Search Modal -->
    <Transition name="modal">
      <div v-if="searchOpen" class="search-overlay" @click.self="closeSearch">
        <div class="search-modal">
        <input
          ref="searchInput"
          v-model="searchQuery"
          type="text"
          placeholder="Search posts..."
          class="search-input"
          @keydown="handleSearchKey"
        />
        <div class="search-results">
          <button
            v-for="(file, i) in searchResults"
            :key="file.path"
            class="search-result"
            :class="{ selected: i === selectedIndex }"
            @click="selectResult(file)"
            @mouseenter="selectedIndex = i"
          >
            <span v-if="file.password" class="result-badge protected">🔒</span>
            <span v-else-if="file.unlisted" class="result-badge unlisted">👁</span>
            <span v-else-if="file.published_url" class="result-badge live">✓</span>
            <span class="result-title">{{ file.title || file.filename.replace('.md', '') }}</span>
            <span class="result-words">{{ file.word_count }}w</span>
            <span class="result-dir">{{ file.source_dir }}</span>
          </button>
          <div v-if="searchResults.length === 0" class="no-results">No results</div>
        </div>
        <div class="search-hint">
          <span>↑↓ navigate</span>
          <span>↵ select</span>
          <span>esc close</span>
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
        @select="(f) => { selectedFile = f; rightTab = 'preview' }"
      />

      <div class="right-panel">
        <div class="panel-tabs">
          <button
            :class="{ active: rightTab === 'preview' }"
            @click="rightTab = 'preview'"
          >
            Preview
          </button>
          <button
            :class="{ active: rightTab === 'media' }"
            @click="rightTab = 'media'"
          >
            Media
          </button>
        </div>

        <div class="panel-content">
          <FilePreview
            v-if="rightTab === 'preview' && selectedFile"
            :file="selectedFile"
            @published="loadFiles"
          />

          <div v-else-if="rightTab === 'preview'" class="empty">
            <div class="empty-icon">
              <FileText :size="48" :stroke-width="1" />
            </div>
            <div class="empty-title">Select a post</div>
            <div class="empty-shortcuts">
              <div class="shortcut-row"><kbd>j</kbd><kbd>k</kbd> <span>navigate</span></div>
              <div class="shortcut-row"><kbd>/</kbd> <span>search</span></div>
              <div class="shortcut-row"><kbd>?</kbd> <span>all shortcuts</span></div>
            </div>
          </div>

          <MediaLibraryModal
            v-else-if="rightTab === 'media'"
            :selected-file="selectedFile"
            :inline="true"
            @close="rightTab = 'preview'"
            @select="(asset) => navigator.clipboard.writeText(asset.secure_url)"
            @insert="handleInsertMedia"
          />
        </div>
      </div>
    </main>

    <!-- Status Bar -->
    <div class="statusbar">
      <div class="status-left">
        <span v-if="selectedFile" class="status-item file-path">
          {{ selectedFile.source_dir }}/{{ selectedFile.filename }}
        </span>
        <span v-else class="status-item muted">No file selected</span>
        <span v-if="selectedFile" class="status-item muted tabular">
          {{ selectedFile.word_count.toLocaleString() }} words
        </span>
      </div>
      <div class="status-right">
        <button @click="toggleCompactMode" class="status-btn" :class="{ active: compactMode }" title="Toggle compact view">
          {{ compactMode ? '⊟' : '⊞' }}
        </button>
        <span class="status-item" :class="gitBranch ? 'connected' : 'muted'" :title="gitBranch ? `Branch: ${gitBranch}` : 'Git not connected'">
          <GitBranch :size="10" />
          {{ gitBranch || 'git' }}
        </span>
        <span class="status-item" :class="obsidianConnected ? 'connected' : 'muted'" title="Obsidian API">
          <Circle :size="6" :fill="obsidianConnected ? 'currentColor' : 'none'" />
          obsidian
        </span>
        <span class="status-item" :class="cloudinaryConnected ? 'connected' : 'muted'" title="Cloudinary">
          <Circle :size="6" :fill="cloudinaryConnected ? 'currentColor' : 'none'" />
          media
        </span>
      </div>
    </div>
  </div>
</template>

<style>
* { margin: 0; padding: 0; box-sizing: border-box; }

/* Base/fallback (dark default) - must come FIRST */
:root {
  /* Colors */
  --bg-primary: rgba(20, 20, 22, 0.85);
  --bg-secondary: rgba(30, 30, 34, 0.8);
  --bg-tertiary: rgba(45, 45, 50, 0.6);
  --bg-solid: #141416;
  --border: rgba(255, 255, 255, 0.08);
  --border-light: rgba(255, 255, 255, 0.12);
  --text-primary: #e5e5e5;
  --text-secondary: #999;
  --text-tertiary: #666;
  --accent: rgba(255, 255, 255, 0.15);
  --selection-bg: rgba(10, 132, 255, 0.85);
  --selection-text: #fff;
  --success: #30d158;
  --warning: #ff9f0a;
  --danger: #ff453a;
  --modal-bg: rgba(35, 35, 40, 0.9);
  --kbd-bg: rgba(255, 255, 255, 0.1);
  --kbd-border: rgba(255, 255, 255, 0.15);

  /* Spacing scale */
  --space-1: 4px;
  --space-2: 8px;
  --space-3: 12px;
  --space-4: 16px;
  --space-5: 24px;
  --space-6: 32px;

  /* Animation timing */
  --ease-out-expo: cubic-bezier(0.16, 1, 0.3, 1);
  --ease-spring: cubic-bezier(0.34, 1.56, 0.64, 1);
  --transition-fast: 0.15s var(--ease-spring);
  --transition-normal: 0.25s var(--ease-out-expo);

  /* Refined shadows */
  --shadow-sm: 0 1px 2px rgba(0, 0, 0, 0.05);
  --shadow-md: 0 4px 12px rgba(0, 0, 0, 0.1);
  --shadow-lg: 0 24px 48px rgba(0, 0, 0, 0.2);
  --shadow-glow: 0 0 0 1px var(--selection-bg), 0 2px 8px color-mix(in srgb, var(--selection-bg) 30%, transparent);
}

/* Light mode - overrides base when system is light */
@media (prefers-color-scheme: light) {
  :root {
    --bg-primary: rgba(255, 255, 255, 0.9);
    --bg-secondary: rgba(248, 248, 248, 0.95);
    --bg-tertiary: rgba(235, 235, 235, 0.8);
    --bg-solid: #ffffff;
    --border: rgba(0, 0, 0, 0.08);
    --border-light: rgba(0, 0, 0, 0.12);
    --text-primary: #1a1a1a;
    --text-secondary: #555;
    --text-tertiary: #888;
    --accent: rgba(0, 0, 0, 0.08);
    --selection-bg: rgba(10, 132, 255, 0.2);
    --selection-text: #1a1a1a;
    --success: #28a745;
    --warning: #e67700;
    --danger: #dc3545;
    --modal-bg: rgba(255, 255, 255, 0.92);
    --kbd-bg: rgba(0, 0, 0, 0.06);
    --kbd-border: rgba(0, 0, 0, 0.12);
    --shadow-sm: 0 1px 2px rgba(0, 0, 0, 0.03);
    --shadow-md: 0 4px 12px rgba(0, 0, 0, 0.06);
    --shadow-lg: 0 24px 48px rgba(0, 0, 0, 0.1);
    --shadow-glow: 0 0 0 1px var(--selection-bg), 0 2px 8px color-mix(in srgb, var(--selection-bg) 20%, transparent);
  }
}

body {
  font-family: -apple-system, BlinkMacSystemFont, 'SF Pro Text', sans-serif;
  font-size: 12px;
  color: var(--text-primary);
  background: var(--bg-solid);
  -webkit-font-smoothing: antialiased;
}

/* Focus ring for accessibility */
:focus-visible {
  outline: 2px solid var(--selection-bg);
  outline-offset: 2px;
}

button:focus:not(:focus-visible) {
  outline: none;
}

/* Smooth scroll */
* {
  scroll-behavior: smooth;
}

/* Selection color */
::selection {
  background: var(--selection-bg);
  color: var(--selection-text);
}

.app {
  height: 100vh;
  display: flex;
  flex-direction: column;
}

.titlebar {
  height: 36px;
  background: var(--bg-secondary);
  backdrop-filter: blur(20px) saturate(180%);
  -webkit-backdrop-filter: blur(20px) saturate(180%);
  border-bottom: 1px solid var(--border-light);
  display: flex;
  align-items: center;
  justify-content: center;
  -webkit-app-region: drag;
  position: relative;
}

.titlebar-spacer { width: 70px; }

.titlebar-btns {
  -webkit-app-region: no-drag;
  position: absolute;
  right: 10px;
  display: flex;
  gap: 4px;
}

.titlebar-btn {
  background: none;
  border: none;
  color: var(--text-tertiary);
  width: 28px;
  height: 28px;
  border-radius: 6px;
  cursor: pointer;
  display: flex;
  align-items: center;
  justify-content: center;
  transition: all var(--transition-fast);
}

.titlebar-btn:hover {
  background: var(--bg-tertiary);
  color: var(--text-primary);
  transform: scale(1.1);
}

.titlebar-btn:active {
  transform: scale(0.95);
}

.titlebar-btn.active {
  color: var(--text-primary);
  background: var(--accent);
}

.titlebar-btn.connected {
  color: var(--success);
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
  border-bottom: 1px solid var(--border);
  background: var(--bg-secondary);
  flex-shrink: 0;
}

.panel-tabs button {
  padding: 8px 16px;
  font-size: 11px;
  font-weight: 500;
  background: transparent;
  border: none;
  color: var(--text-tertiary);
  cursor: pointer;
  border-bottom: 2px solid transparent;
  margin-bottom: -1px;
}

.panel-tabs button:hover {
  color: var(--text-secondary);
}

.panel-tabs button.active {
  color: var(--text-primary);
  border-bottom-color: var(--text-primary);
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

.titlebar-btn.spinning svg {
  animation: spin 1s linear infinite;
}

/* Spring animations */
@keyframes slideUp {
  from { opacity: 0; transform: translateY(8px); }
  to { opacity: 1; transform: translateY(0); }
}

@keyframes scaleIn {
  from { opacity: 0; transform: scale(0.95); }
  to { opacity: 1; transform: scale(1); }
}

@keyframes celebrate {
  0% { transform: translate(-50%, -50%) scale(1); }
  50% { transform: translate(-50%, -50%) scale(1.08); }
  100% { transform: translate(-50%, -50%) scale(1); }
}

@keyframes successGlow {
  0% { box-shadow: 0 0 60px color-mix(in srgb, var(--success) 50%, transparent); }
  100% { box-shadow: none; }
}

@keyframes confettiBurst {
  0% { opacity: 1; transform: translateY(0) scale(1); }
  100% { opacity: 0; transform: translateY(-20px) scale(0); }
}

/* Search Modal */
.search-overlay {
  position: fixed;
  inset: 0;
  background: rgba(0, 0, 0, 0.4);
  backdrop-filter: blur(4px);
  -webkit-backdrop-filter: blur(4px);
  z-index: 100;
  display: flex;
  align-items: flex-start;
  justify-content: center;
  padding-top: 80px;
}

.search-modal {
  width: 500px;
  max-width: 90vw;
  background: var(--modal-bg);
  backdrop-filter: blur(24px) saturate(180%);
  -webkit-backdrop-filter: blur(24px) saturate(180%);
  border: 1px solid var(--border-light);
  border-radius: 12px;
  overflow: hidden;
  box-shadow: var(--shadow-lg);
}

.search-input {
  width: 100%;
  padding: 14px 16px;
  font-size: 15px;
  background: transparent;
  border: none;
  border-bottom: 1px solid var(--border);
  color: var(--text-primary);
  outline: none;
}

.search-input::placeholder {
  color: var(--text-tertiary);
}

.search-results {
  max-height: 350px;
  overflow-y: auto;
}

.search-result {
  width: 100%;
  padding: 10px 16px;
  display: flex;
  align-items: center;
  gap: 8px;
  background: transparent;
  border: none;
  cursor: pointer;
  text-align: left;
  transition: background 0.1s ease;
}

.search-result:hover {
  background: var(--accent);
}

.search-result.selected {
  background: var(--selection-bg);
  color: var(--selection-text);
}

.search-result.selected .result-title {
  color: var(--selection-text);
}

.search-result.selected .result-dir {
  color: var(--selection-text);
  opacity: 0.7;
}

.result-badge {
  font-size: 9px;
  padding: 2px 5px;
  border-radius: 3px;
  flex-shrink: 0;
}

.result-badge.live {
  background: var(--success);
  color: #000;
}

.result-badge.unlisted {
  background: #6366f1;
  color: #fff;
}

.result-badge.protected {
  background: #8b5cf6;
  color: #fff;
}

.result-words {
  font-size: 9px;
  font-family: 'SF Mono', monospace;
  font-variant-numeric: tabular-nums;
  font-feature-settings: 'tnum';
  color: var(--text-tertiary);
  flex-shrink: 0;
}

.result-title {
  flex: 1;
  font-size: 13px;
  color: var(--text-primary);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.result-dir {
  font-size: 10px;
  color: var(--text-tertiary);
  font-family: 'SF Mono', monospace;
}

.no-results {
  padding: 20px;
  text-align: center;
  color: var(--text-tertiary);
}

.search-hint {
  padding: 8px 16px;
  border-top: 1px solid var(--border);
  display: flex;
  gap: 16px;
  font-size: 10px;
  color: var(--text-tertiary);
}

/* Modal animations - using spring timing */
.modal-enter-active,
.modal-leave-active {
  transition: all var(--transition-normal);
}

.modal-enter-active .search-modal,
.modal-leave-active .search-modal {
  transition: all var(--transition-normal);
}

.modal-leave-active {
  transition: all 0.15s ease;
}

.modal-leave-active .search-modal {
  transition: all 0.15s ease;
}

.modal-enter-from,
.modal-leave-to {
  opacity: 0;
}

.modal-enter-from .search-modal {
  transform: scale(0.96) translateY(-10px);
  opacity: 0;
}

.modal-leave-to .search-modal {
  transform: scale(0.98) translateY(-4px);
  opacity: 0;
}

/* Help Modal */
.help-modal {
  width: 480px;
  max-width: 90vw;
  background: var(--modal-bg);
  backdrop-filter: blur(24px) saturate(180%);
  -webkit-backdrop-filter: blur(24px) saturate(180%);
  border: 1px solid var(--border-light);
  border-radius: 12px;
  padding: var(--space-5);
  box-shadow: var(--shadow-lg);
  animation: scaleIn 0.25s var(--ease-out-expo);
}

.help-title {
  font-size: 14px;
  font-weight: 600;
  color: var(--text-primary);
  margin-bottom: 16px;
  text-align: center;
}

.help-grid {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 20px;
}

.help-section-title {
  font-size: 10px;
  font-weight: 600;
  color: var(--text-tertiary);
  text-transform: uppercase;
  letter-spacing: 0.75px;
  margin-bottom: 8px;
}

.help-row {
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: 11px;
  color: var(--text-secondary);
  margin-bottom: 6px;
}

.help-row span {
  margin-left: auto;
  color: var(--text-tertiary);
}

kbd {
  display: inline-block;
  padding: 2px 6px;
  font-family: 'SF Mono', monospace;
  font-size: 10px;
  background: var(--kbd-bg);
  border: 1px solid var(--kbd-border);
  border-radius: 4px;
  color: var(--text-primary);
}

.help-hint {
  margin-top: 16px;
  padding-top: 12px;
  border-top: 1px solid var(--border);
  text-align: center;
  font-size: 10px;
  color: var(--text-tertiary);
}

.help-hint kbd {
  font-size: 9px;
  padding: 1px 4px;
}

.help-divider {
  margin: 12px 0;
  border-top: 1px solid var(--border);
}

.visibility-spectrum {
  display: flex;
  flex-direction: column;
  gap: 6px;
  margin-top: 8px;
}

.vis-row {
  display: flex;
  align-items: center;
  gap: 12px;
  font-size: 10px;
}

.vis-badge {
  font-size: 8px;
  font-weight: 700;
  padding: 2px 6px;
  border-radius: 3px;
  min-width: 80px;
  text-align: center;
}

.vis-public {
  background: var(--success);
  color: #000;
}

.vis-unlisted {
  background: #6366f1;
  color: #fff;
}

.vis-protected {
  background: #8b5cf6;
  color: #fff;
}

.vis-desc {
  color: var(--text-tertiary);
}

.vis-desc code {
  font-family: 'SF Mono', monospace;
  font-size: 9px;
  background: var(--kbd-bg);
  padding: 1px 4px;
  border-radius: 2px;
}

/* Status Bar */
.statusbar {
  height: 22px;
  background: var(--bg-secondary);
  border-top: 1px solid var(--border);
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 0 12px;
  font-size: 10px;
  font-family: 'SF Mono', monospace;
  color: var(--text-tertiary);
  flex-shrink: 0;
}

.status-left,
.status-right {
  display: flex;
  align-items: center;
  gap: 12px;
}

.status-item {
  display: flex;
  align-items: center;
  gap: 4px;
}

.status-item.connected {
  color: var(--success);
}

.status-item.connected::before {
  content: '';
  width: 5px;
  height: 5px;
  background: var(--success);
  border-radius: 50%;
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

.status-btn {
  background: none;
  border: none;
  color: var(--text-tertiary);
  cursor: pointer;
  padding: 0 4px;
  font-size: 12px;
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
  position: absolute;
  left: 50%;
  transform: translateX(-50%);
}

</style>
