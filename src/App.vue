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
  if ((e.metaKey || e.ctrlKey) && e.key === 'k') {
    e.preventDefault()
    if (searchOpen.value) closeSearch()
    else openSearch()
  }
  if (e.key === 'Escape' && searchOpen.value) {
    closeSearch()
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
      <span class="titlebar-title">Dispatch</span>
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
        Select a file
      </div>
    </main>
  </div>
</template>

<style>
* { margin: 0; padding: 0; box-sizing: border-box; }

:root {
  --bg-primary: #1a1a1a;
  --bg-secondary: #242424;
  --bg-tertiary: #2e2e2e;
  --border: #333;
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
  background: var(--bg-primary);
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
  border-bottom: 1px solid var(--border);
  display: flex;
  align-items: center;
  justify-content: center;
  -webkit-app-region: drag;
  position: relative;
}

.titlebar-spacer { width: 70px; }

.titlebar-title {
  font-size: 12px;
  font-weight: 500;
  color: var(--text-secondary);
}

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
  align-items: center;
  justify-content: center;
  color: var(--text-tertiary);
}

/* Search Modal */
.search-overlay {
  position: fixed;
  inset: 0;
  background: rgba(0, 0, 0, 0.6);
  z-index: 100;
  display: flex;
  align-items: flex-start;
  justify-content: center;
  padding-top: 80px;
}

.search-modal {
  width: 500px;
  max-width: 90vw;
  background: var(--bg-secondary);
  border: 1px solid var(--border);
  border-radius: 12px;
  overflow: hidden;
  box-shadow: 0 20px 40px rgba(0, 0, 0, 0.4);
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
  background: var(--bg-tertiary);
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
</style>
