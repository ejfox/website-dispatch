<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted } from 'vue'
import { invoke } from '@tauri-apps/api/tauri'
import FileList from './components/FileList.vue'
import FilePreview from './components/FilePreview.vue'

interface MarkdownFile {
  path: string
  filename: string
  title: string | null
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
})

onUnmounted(() => {
  window.removeEventListener('keydown', handleGlobalKey)
})
</script>

<template>
  <div class="app">
    <div class="titlebar" data-tauri-drag-region>
      <div class="titlebar-spacer"></div>
      <div class="titlebar-btns">
        <button @click="openSearch" class="titlebar-btn" title="Search (⌘K)">
          <svg width="14" height="14" viewBox="0 0 16 16" fill="currentColor">
            <path d="M11.742 10.344a6.5 6.5 0 1 0-1.397 1.398h-.001c.03.04.062.078.098.115l3.85 3.85a1 1 0 0 0 1.415-1.414l-3.85-3.85a1.007 1.007 0 0 0-.115-.1zM12 6.5a5.5 5.5 0 1 1-11 0 5.5 5.5 0 0 1 11 0z"/>
          </svg>
        </button>
        <button @click="loadFiles" class="titlebar-btn" title="Refresh">
          <svg width="14" height="14" viewBox="0 0 16 16" fill="currentColor">
            <path d="M8 3a5 5 0 1 0 4.546 2.914.5.5 0 0 1 .908-.417A6 6 0 1 1 8 2v1z"/>
            <path d="M8 4.466V.534a.25.25 0 0 1 .41-.192l2.36 1.966c.12.1.12.284 0 .384L8.41 4.658A.25.25 0 0 1 8 4.466z"/>
          </svg>
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
            <div class="help-row"><kbd>⌘</kbd><kbd>↵</kbd> <span>publish</span></div>
          </div>
        </div>
        <div class="help-hint">Press <kbd>?</kbd> or <kbd>esc</kbd> to close</div>
      </div>
    </div>

    <!-- Search Modal -->
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
            <span v-if="file.published_url" class="result-live">LIVE</span>
            <span class="result-title">{{ file.title || file.filename.replace('.md', '') }}</span>
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

    <main class="main">
      <FileList
        :files="files"
        :selected="selectedFile"
        :loading="loading"
        @select="(f) => selectedFile = f"
      />

      <FilePreview
        v-if="selectedFile"
        :file="selectedFile"
        @published="loadFiles"
      />

      <div v-else class="empty">
        <div>Select a file</div>
        <div class="empty-hint">Press <kbd>?</kbd> for shortcuts</div>
      </div>
    </main>
  </div>
</template>

<style>
* { margin: 0; padding: 0; box-sizing: border-box; }

:root {
  --bg-primary: rgba(20, 20, 22, 0.85);
  --bg-secondary: rgba(30, 30, 34, 0.8);
  --bg-tertiary: rgba(45, 45, 50, 0.6);
  --bg-solid: #141416;
  --border: rgba(255, 255, 255, 0.08);
  --border-light: rgba(255, 255, 255, 0.12);
  --text-primary: #e5e5e5;
  --text-secondary: #999;
  --text-tertiary: #666;
  --accent: #0a84ff;
  --warning: #ff9f0a;
  --success: #30d158;
}

body {
  font-family: -apple-system, BlinkMacSystemFont, 'SF Pro Text', sans-serif;
  font-size: 12px;
  color: var(--text-primary);
  background: var(--bg-solid);
  -webkit-font-smoothing: antialiased;
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
  width: 24px;
  height: 24px;
  border-radius: 4px;
  cursor: pointer;
  display: flex;
  align-items: center;
  justify-content: center;
}

.titlebar-btn:hover {
  background: var(--bg-tertiary);
  color: var(--text-primary);
}

.main {
  flex: 1;
  display: flex;
  overflow: hidden;
}

.empty {
  flex: 1;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 8px;
  color: var(--text-tertiary);
}

.empty-hint {
  font-size: 10px;
  color: var(--text-tertiary);
  opacity: 0.6;
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
  background: rgba(35, 35, 40, 0.85);
  backdrop-filter: blur(24px) saturate(180%);
  -webkit-backdrop-filter: blur(24px) saturate(180%);
  border: 1px solid var(--border-light);
  border-radius: 12px;
  overflow: hidden;
  box-shadow: 0 24px 48px rgba(0, 0, 0, 0.5), 0 0 0 1px rgba(255,255,255,0.05) inset;
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
}

.search-result:hover,
.search-result.selected {
  background: rgba(255, 255, 255, 0.08);
}

.result-live {
  font-size: 8px;
  font-weight: 700;
  background: var(--success);
  color: #000;
  padding: 2px 5px;
  border-radius: 3px;
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

/* Help Modal */
.help-modal {
  width: 480px;
  max-width: 90vw;
  background: rgba(35, 35, 40, 0.9);
  backdrop-filter: blur(24px) saturate(180%);
  -webkit-backdrop-filter: blur(24px) saturate(180%);
  border: 1px solid var(--border-light);
  border-radius: 12px;
  padding: 20px;
  box-shadow: 0 24px 48px rgba(0, 0, 0, 0.5);
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
  letter-spacing: 0.5px;
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
  background: rgba(255, 255, 255, 0.1);
  border: 1px solid rgba(255, 255, 255, 0.15);
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
</style>
