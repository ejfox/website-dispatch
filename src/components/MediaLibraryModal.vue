<script setup lang="ts">
import { ref, computed, watch, onMounted } from 'vue'
import { invoke } from '@tauri-apps/api/tauri'

interface CloudinaryAsset {
  public_id: string
  secure_url: string
  resource_type: string
  format: string
  width: number | null
  height: number | null
  bytes: number
  created_at: string | null
}

interface MediaLibraryPage {
  assets: CloudinaryAsset[]
  next_cursor: string | null
  total_count: number | null
}

interface AssetUsage {
  post_path: string
  post_title: string | null
  line_number: number
  context: string
}

interface UsageScanResult {
  usage_map: {
    by_asset: Record<string, AssetUsage[]>
    by_post: Record<string, string[]>
    total_assets: number
    total_posts: number
  }
  cloudinary_urls: string[]
  scan_duration_ms: number
}

interface SelectedFile {
  path: string
  filename: string
  title?: string | null
}

const props = defineProps<{
  selectedFile?: SelectedFile | null
  inline?: boolean
}>()

const emit = defineEmits<{
  close: []
  select: [asset: CloudinaryAsset]
  insert: [markdown: string]
}>()

// Computed
const targetPostName = computed(() => {
  if (!props.selectedFile) return null
  return props.selectedFile.title || props.selectedFile.filename.replace('.md', '')
})

// State
const assets = ref<CloudinaryAsset[]>([])
const loading = ref(false)
const error = ref<string | null>(null)
const nextCursor = ref<string | null>(null)
const searchQuery = ref('')
const resourceType = ref<'all' | 'image' | 'video'>('all')
const searchInput = ref<HTMLInputElement | null>(null)
const selectedIndex = ref(0)
const copiedText = ref<string | null>(null)

// Folder state
const folders = ref<string[]>([])
const selectedFolder = ref<string | null>('blog')  // Default to blog folder
const loadingFolders = ref(false)

// Usage state
const usageData = ref<UsageScanResult | null>(null)
const loadingUsage = ref(false)
const showUnusedOnly = ref(false)

// Detail panel
const showDetail = ref(false)
const detailAsset = ref<CloudinaryAsset | null>(null)
const detailUsage = ref<AssetUsage[]>([])

// Computed
const selectedAsset = computed(() => assets.value[selectedIndex.value])

const filteredAssets = computed(() => {
  if (!showUnusedOnly.value || !usageData.value) return assets.value

  return assets.value.filter(asset => {
    const usage = usageData.value?.usage_map.by_asset[asset.public_id]
    return !usage || usage.length === 0
  })
})

const getUsageCount = (publicId: string): number => {
  if (!usageData.value) return -1
  const usage = usageData.value.usage_map.by_asset[publicId]
  return usage ? usage.length : 0
}

// Load folders
async function loadFolders() {
  loadingFolders.value = true
  try {
    folders.value = await invoke('cloudinary_list_folders')
  } catch (e) {
    console.error('Failed to load folders:', e)
    folders.value = []
  }
  loadingFolders.value = false
}

// Scan for usage
async function scanUsage() {
  loadingUsage.value = true
  try {
    usageData.value = await invoke('scan_asset_usage')
  } catch (e) {
    console.error('Failed to scan usage:', e)
  }
  loadingUsage.value = false
}

// Load assets
async function loadAssets(append = false) {
  loading.value = true
  error.value = null

  try {
    let result: MediaLibraryPage

    if (searchQuery.value.trim()) {
      // Build search expression
      let expr = searchQuery.value
      if (selectedFolder.value) {
        expr = `folder:${selectedFolder.value}/* AND ${expr}`
      }
      result = await invoke('cloudinary_search', {
        query: expr,
        maxResults: 50
      })
    } else if (selectedFolder.value) {
      // Search by folder
      result = await invoke('cloudinary_search', {
        query: `folder:${selectedFolder.value}/*`,
        maxResults: 50
      })
    } else {
      const resType = resourceType.value === 'all' ? 'image' : resourceType.value
      result = await invoke('cloudinary_list_assets', {
        resourceType: resType,
        maxResults: 50,
        cursor: append ? nextCursor.value : null
      })
    }

    if (append) {
      assets.value = [...assets.value, ...result.assets]
    } else {
      assets.value = result.assets
      selectedIndex.value = 0
    }
    nextCursor.value = result.next_cursor
  } catch (e) {
    error.value = String(e)
  }

  loading.value = false
}

// Show detail panel
function showAssetDetail(asset: CloudinaryAsset) {
  detailAsset.value = asset
  detailUsage.value = usageData.value?.usage_map.by_asset[asset.public_id] || []
  showDetail.value = true
}

// Keyboard handling
function handleKeydown(e: KeyboardEvent) {
  if (showDetail.value) {
    if (e.key === 'Escape') {
      showDetail.value = false
      return
    }
    return
  }

  if (e.key === 'Escape') {
    emit('close')
    return
  }

  // If search input is focused
  if (document.activeElement === searchInput.value) {
    if (e.key === 'Enter') {
      e.preventDefault()
      loadAssets()
      searchInput.value?.blur()
    }
    if (e.key === 'Escape') {
      searchInput.value?.blur()
    }
    return
  }

  const cols = 5
  const len = filteredAssets.value.length

  if (e.key === 'ArrowRight' || e.key === 'l') {
    e.preventDefault()
    selectedIndex.value = Math.min(selectedIndex.value + 1, len - 1)
  }
  if (e.key === 'ArrowLeft' || e.key === 'h') {
    e.preventDefault()
    selectedIndex.value = Math.max(selectedIndex.value - 1, 0)
  }
  if (e.key === 'ArrowDown' || e.key === 'j') {
    e.preventDefault()
    selectedIndex.value = Math.min(selectedIndex.value + cols, len - 1)
  }
  if (e.key === 'ArrowUp' || e.key === 'k') {
    e.preventDefault()
    selectedIndex.value = Math.max(selectedIndex.value - cols, 0)
  }
  if (e.key === 'Enter' || e.key === ' ') {
    e.preventDefault()
    const asset = filteredAssets.value[selectedIndex.value]
    if (asset) showAssetDetail(asset)
  }
  if (e.key === 'c' && !e.metaKey) {
    e.preventDefault()
    const asset = filteredAssets.value[selectedIndex.value]
    if (asset) copyUrl(asset)
  }
  if (e.key === 'i' && !e.metaKey) {
    e.preventDefault()
    const asset = filteredAssets.value[selectedIndex.value]
    if (asset) copyMarkdown(asset)
  }
  if (e.key === '/') {
    e.preventDefault()
    searchInput.value?.focus()
  }
  if (e.key === 'u') {
    e.preventDefault()
    showUnusedOnly.value = !showUnusedOnly.value
  }
}

function copyUrl(asset: CloudinaryAsset) {
  navigator.clipboard.writeText(asset.secure_url)
  copiedText.value = 'URL'
  setTimeout(() => { copiedText.value = null }, 1500)
}

function copyMarkdown(asset: CloudinaryAsset, alt = '') {
  const md = `![${alt}](${asset.secure_url})`
  navigator.clipboard.writeText(md)
  copiedText.value = 'Markdown'
  setTimeout(() => { copiedText.value = null }, 1500)
}

function insertMarkdown(asset: CloudinaryAsset, alt = '') {
  const md = `![${alt}](${asset.secure_url})`
  emit('insert', md)
  emit('close')
}

function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes}B`
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(0)}KB`
  return `${(bytes / (1024 * 1024)).toFixed(1)}MB`
}

function getThumbnailUrl(asset: CloudinaryAsset): string {
  if (asset.resource_type === 'video') {
    return asset.secure_url.replace('/upload/', '/upload/c_fill,w_160,h_100,g_auto/')
      .replace(/\.[^.]+$/, '.jpg')
  }
  return asset.secure_url.replace('/upload/', '/upload/c_fill,w_160,h_100,g_auto/')
}

function getPreviewUrl(asset: CloudinaryAsset): string {
  if (asset.resource_type === 'video') {
    return asset.secure_url.replace('/upload/', '/upload/c_limit,w_600/')
      .replace(/\.[^.]+$/, '.jpg')
  }
  return asset.secure_url.replace('/upload/', '/upload/c_limit,w_600/')
}

function selectFolder(folder: string | null) {
  selectedFolder.value = folder
  loadAssets()
}

watch([resourceType], () => {
  searchQuery.value = ''
  loadAssets()
})

onMounted(() => {
  loadAssets()
  loadFolders()
  scanUsage()
})
</script>

<template>
  <div
    :class="inline ? 'inline-container' : 'modal-overlay'"
    @click.self="!inline && $emit('close')"
    @keydown="handleKeydown"
    tabindex="0"
  >
    <div :class="[inline ? 'inline-panel' : 'modal', { 'with-detail': showDetail }]">
      <!-- Sidebar -->
      <div class="sidebar">
        <div class="sidebar-header">Folders</div>
        <div class="folder-list">
          <button
            class="folder-item"
            :class="{ active: selectedFolder === null && !showUnusedOnly }"
            @click="selectFolder(null); showUnusedOnly = false"
          >
            <span class="folder-icon">📁</span>
            <span>All Assets</span>
          </button>
          <button
            class="folder-item"
            :class="{ active: showUnusedOnly }"
            @click="showUnusedOnly = !showUnusedOnly"
          >
            <span class="folder-icon">👻</span>
            <span>Unused</span>
            <span v-if="usageData" class="folder-count">
              {{ Object.keys(usageData.usage_map.by_asset).length > 0 ? '?' : '0' }}
            </span>
          </button>
          <div class="folder-divider"></div>
          <button
            v-for="folder in folders"
            :key="folder"
            class="folder-item"
            :class="{ active: selectedFolder === folder }"
            @click="selectFolder(folder)"
          >
            <span class="folder-icon">📂</span>
            <span>{{ folder }}</span>
          </button>
          <div v-if="loadingFolders" class="folder-loading">Loading...</div>
        </div>

        <div class="sidebar-footer">
          <div v-if="usageData" class="usage-stats">
            <span>{{ usageData.usage_map.total_posts }} posts scanned</span>
            <button @click="scanUsage" :disabled="loadingUsage" class="rescan-btn">
              {{ loadingUsage ? '...' : '↻' }}
            </button>
          </div>
        </div>
      </div>

      <!-- Main content -->
      <div class="main-content">
        <div class="modal-header">
          <h2>Media Library</h2>
          <div class="filters">
            <button :class="{ active: resourceType === 'all' }" @click="resourceType = 'all'">All</button>
            <button :class="{ active: resourceType === 'image' }" @click="resourceType = 'image'">Images</button>
            <button :class="{ active: resourceType === 'video' }" @click="resourceType = 'video'">Videos</button>
          </div>
          <button v-if="!inline" class="close-btn" @click="$emit('close')">✕</button>
        </div>

        <div class="search-bar">
          <input
            ref="searchInput"
            v-model="searchQuery"
            type="text"
            placeholder="Search... (/ to focus)"
            @keydown.enter="loadAssets()"
          />
        </div>

        <!-- Copied toast -->
        <Transition name="toast">
          <div v-if="copiedText" class="copied-toast">{{ copiedText }} copied!</div>
        </Transition>

        <div v-if="error" class="error">{{ error }}</div>

        <div v-if="loading && assets.length === 0" class="loading">Loading...</div>

        <div v-else class="assets-grid">
          <div
            v-for="(asset, i) in filteredAssets"
            :key="asset.public_id"
            class="asset-card"
            :class="{ selected: i === selectedIndex }"
            @click="showAssetDetail(asset)"
            @mouseenter="selectedIndex = i"
          >
            <div class="asset-thumbnail">
              <img :src="getThumbnailUrl(asset)" :alt="asset.public_id" loading="lazy" />
              <span v-if="asset.resource_type === 'video'" class="badge video">VID</span>
              <span v-if="getUsageCount(asset.public_id) === 0" class="badge unused" title="Not used in any posts">•</span>
              <span v-else-if="getUsageCount(asset.public_id) > 0" class="badge used">
                {{ getUsageCount(asset.public_id) }}
              </span>
            </div>
            <div class="asset-info">
              <span class="asset-name">{{ asset.public_id.split('/').pop() }}</span>
            </div>
          </div>
        </div>

        <div v-if="nextCursor && !showUnusedOnly" class="load-more">
          <button @click="loadAssets(true)" :disabled="loading">
            {{ loading ? '...' : 'Load More' }}
          </button>
        </div>

        <div class="modal-footer">
          <span class="hint">
            <kbd>↑↓←→</kbd> navigate
            <kbd>Enter</kbd> details
            <kbd>c</kbd> copy URL
            <kbd>i</kbd> copy markdown
            <kbd>u</kbd> toggle unused
          </span>
        </div>
      </div>

      <!-- Detail Panel -->
      <Transition name="slide">
        <div v-if="showDetail && detailAsset" class="detail-panel">
          <div class="detail-header">
            <h3>{{ detailAsset.public_id.split('/').pop() }}</h3>
            <button @click="showDetail = false" class="close-btn">✕</button>
          </div>

          <div class="detail-preview">
            <img :src="getPreviewUrl(detailAsset)" :alt="detailAsset.public_id" />
          </div>

          <div class="detail-meta">
            <div class="meta-row">
              <span class="label">Size</span>
              <span>{{ formatBytes(detailAsset.bytes) }}</span>
            </div>
            <div v-if="detailAsset.width && detailAsset.height" class="meta-row">
              <span class="label">Dimensions</span>
              <span>{{ detailAsset.width }} × {{ detailAsset.height }}</span>
            </div>
            <div class="meta-row">
              <span class="label">Format</span>
              <span>{{ detailAsset.format.toUpperCase() }}</span>
            </div>
            <div class="meta-row">
              <span class="label">Type</span>
              <span>{{ detailAsset.resource_type }}</span>
            </div>
            <div class="meta-row">
              <span class="label">Folder</span>
              <span>{{ detailAsset.public_id.split('/').slice(0, -1).join('/') || '(root)' }}</span>
            </div>
          </div>

          <div class="detail-actions">
            <button @click="copyMarkdown(detailAsset)" class="action-btn primary">
              Copy Markdown
            </button>
            <button @click="copyUrl(detailAsset)" class="action-btn">
              Copy URL
            </button>
          </div>

          <div class="detail-usage">
            <div class="usage-header">
              <span>Used in {{ detailUsage.length }} post{{ detailUsage.length === 1 ? '' : 's' }}</span>
            </div>
            <div v-if="detailUsage.length === 0" class="usage-empty">
              Not used in any posts
            </div>
            <div v-else class="usage-list">
              <div v-for="usage in detailUsage" :key="usage.post_path + usage.line_number" class="usage-item">
                <span class="usage-title">{{ usage.post_title || usage.post_path.split('/').pop() }}</span>
                <span class="usage-line">line {{ usage.line_number }}</span>
              </div>
            </div>
          </div>

          <div class="detail-url">
            <input type="text" :value="detailAsset.secure_url" readonly @click="($event.target as HTMLInputElement).select()" />
          </div>
        </div>
      </Transition>
    </div>
  </div>
</template>

<style scoped>
.modal-overlay {
  position: fixed;
  inset: 0;
  background: rgba(0, 0, 0, 0.7);
  backdrop-filter: blur(8px);
  z-index: 200;
  display: flex;
  align-items: center;
  justify-content: center;
  outline: none;
}

.modal {
  width: 1000px;
  max-width: 95vw;
  height: 80vh;
  max-height: 800px;
  background: #1a1a1e;
  border: 1px solid rgba(255, 255, 255, 0.1);
  border-radius: 12px;
  display: flex;
  overflow: hidden;
  box-shadow: 0 24px 48px rgba(0, 0, 0, 0.5);
}

.modal.with-detail {
  width: 1300px;
}

/* Inline mode (tab panel) */
.inline-container {
  flex: 1;
  display: flex;
  overflow: hidden;
  outline: none;
}

.inline-panel {
  flex: 1;
  display: flex;
  overflow: hidden;
  background: var(--bg-solid);
}

.inline-panel .sidebar {
  width: 150px;
  min-width: 150px;
}

.inline-panel .modal-header h2 {
  display: none;
}

/* Sidebar */
.sidebar {
  width: 180px;
  background: #141416;
  border-right: 1px solid rgba(255, 255, 255, 0.08);
  display: flex;
  flex-direction: column;
  flex-shrink: 0;
}

.sidebar-header {
  padding: 12px 14px;
  font-size: 10px;
  font-weight: 600;
  text-transform: uppercase;
  letter-spacing: 0.5px;
  color: var(--text-tertiary);
  border-bottom: 1px solid rgba(255, 255, 255, 0.06);
}

.folder-list {
  flex: 1;
  overflow-y: auto;
  padding: 6px;
}

.folder-item {
  width: 100%;
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 8px 10px;
  font-size: 11px;
  color: var(--text-secondary);
  background: transparent;
  border: none;
  border-radius: 5px;
  cursor: pointer;
  text-align: left;
}

.folder-item:hover {
  background: rgba(255, 255, 255, 0.06);
}

.folder-item.active {
  background: rgba(255, 255, 255, 0.12);
  color: var(--text-primary);
}

.folder-icon {
  font-size: 12px;
}

.folder-count {
  margin-left: auto;
  font-size: 9px;
  opacity: 0.6;
}

.folder-divider {
  height: 1px;
  background: rgba(255, 255, 255, 0.06);
  margin: 6px 0;
}

.folder-loading {
  padding: 8px 10px;
  font-size: 10px;
  color: var(--text-tertiary);
}

.sidebar-footer {
  padding: 10px;
  border-top: 1px solid rgba(255, 255, 255, 0.06);
}

.usage-stats {
  display: flex;
  align-items: center;
  justify-content: space-between;
  font-size: 9px;
  color: var(--text-tertiary);
}

.rescan-btn {
  background: none;
  border: none;
  color: var(--text-tertiary);
  cursor: pointer;
  font-size: 12px;
}

.rescan-btn:hover {
  color: var(--text-primary);
}

/* Main content */
.main-content {
  flex: 1;
  display: flex;
  flex-direction: column;
  min-width: 0;
}

.modal-header {
  padding: 10px 12px;
  border-bottom: 1px solid rgba(255, 255, 255, 0.08);
  display: flex;
  align-items: center;
  gap: 12px;
  background: rgba(0, 0, 0, 0.2);
}

.modal-header h2 {
  font-size: 13px;
  font-weight: 600;
  margin: 0;
}

.filters {
  display: flex;
  gap: 2px;
  margin-left: auto;
  background: rgba(0, 0, 0, 0.3);
  padding: 2px;
  border-radius: 4px;
}

.filters button {
  padding: 4px 10px;
  font-size: 10px;
  background: transparent;
  border: none;
  border-radius: 3px;
  color: var(--text-tertiary);
  cursor: pointer;
}

.filters button:hover {
  color: var(--text-primary);
}

.filters button.active {
  background: rgba(255, 255, 255, 0.2);
  color: var(--text-primary);
}

.close-btn {
  background: none;
  border: none;
  color: var(--text-tertiary);
  cursor: pointer;
  font-size: 14px;
  padding: 4px 8px;
}

.close-btn:hover {
  color: var(--text-primary);
}

.search-bar {
  padding: 8px 12px;
  border-bottom: 1px solid rgba(255, 255, 255, 0.06);
}

.search-bar input {
  width: 100%;
  padding: 8px 12px;
  font-size: 12px;
  background: rgba(0, 0, 0, 0.4);
  border: 1px solid rgba(255, 255, 255, 0.08);
  border-radius: 6px;
  color: var(--text-primary);
  outline: none;
}

.search-bar input:focus {
  border-color: rgba(255, 255, 255, 0.3);
}

.search-bar input::placeholder {
  color: var(--text-tertiary);
}

.copied-toast {
  position: absolute;
  top: 80px;
  left: 50%;
  transform: translateX(-50%);
  padding: 8px 16px;
  background: rgba(255, 255, 255, 0.9);
  color: #000;
  font-size: 12px;
  font-weight: 600;
  border-radius: 6px;
  z-index: 10;
}

.toast-enter-active, .toast-leave-active {
  transition: all 0.2s ease;
}
.toast-enter-from, .toast-leave-to {
  opacity: 0;
  transform: translateX(-50%) translateY(-10px);
}

.error {
  padding: 16px;
  color: #ff6b6b;
  text-align: center;
  font-size: 12px;
}

.loading {
  padding: 40px;
  text-align: center;
  color: var(--text-tertiary);
}

.assets-grid {
  flex: 1;
  overflow-y: auto;
  padding: 8px;
  display: grid;
  grid-template-columns: repeat(5, 1fr);
  gap: 6px;
  align-content: start;
}

.asset-card {
  background: rgba(0, 0, 0, 0.3);
  border: 2px solid transparent;
  border-radius: 6px;
  overflow: hidden;
  cursor: pointer;
  transition: all 0.1s ease;
}

.asset-card:hover {
  border-color: rgba(255, 255, 255, 0.2);
}

.asset-card.selected {
  border-color: rgba(255, 255, 255, 0.5);
}

.asset-thumbnail {
  position: relative;
  height: 80px;
  background: #111;
  overflow: hidden;
}

.asset-thumbnail img {
  width: 100%;
  height: 100%;
  object-fit: cover;
}

.badge {
  position: absolute;
  padding: 2px 5px;
  font-size: 8px;
  font-weight: 700;
  border-radius: 2px;
  letter-spacing: 0.3px;
}

.badge.video {
  top: 4px;
  left: 4px;
  background: rgba(255, 255, 255, 0.8);
  color: #000;
}

.badge.unused {
  top: 4px;
  right: 4px;
  background: rgba(255, 255, 255, 0.3);
  color: transparent;
  width: 6px;
  height: 6px;
  padding: 0;
  border-radius: 50%;
  font-size: 0;
}

.badge.used {
  top: 4px;
  right: 4px;
  background: var(--success);
  color: #000;
}

.asset-info {
  padding: 5px 6px;
}

.asset-name {
  display: block;
  font-size: 9px;
  color: var(--text-secondary);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.load-more {
  padding: 8px;
  text-align: center;
}

.load-more button {
  padding: 6px 20px;
  font-size: 11px;
  background: rgba(255, 255, 255, 0.1);
  border: 1px solid rgba(255, 255, 255, 0.15);
  border-radius: 4px;
  color: var(--text-secondary);
  cursor: pointer;
}

.load-more button:hover {
  background: rgba(255, 255, 255, 0.15);
}

.modal-footer {
  padding: 8px 12px;
  border-top: 1px solid rgba(255, 255, 255, 0.06);
  background: rgba(0, 0, 0, 0.2);
}

.hint {
  font-size: 9px;
  color: var(--text-tertiary);
}

.hint kbd {
  display: inline-block;
  padding: 1px 4px;
  font-family: 'SF Mono', monospace;
  font-size: 8px;
  background: rgba(255, 255, 255, 0.08);
  border: 1px solid rgba(255, 255, 255, 0.1);
  border-radius: 2px;
  margin: 0 2px;
}

/* Detail Panel */
.detail-panel {
  width: 300px;
  background: #141416;
  border-left: 1px solid rgba(255, 255, 255, 0.08);
  display: flex;
  flex-direction: column;
  flex-shrink: 0;
}

.slide-enter-active, .slide-leave-active {
  transition: all 0.2s ease;
}
.slide-enter-from, .slide-leave-to {
  transform: translateX(100%);
  opacity: 0;
}

.detail-header {
  padding: 12px;
  border-bottom: 1px solid rgba(255, 255, 255, 0.06);
  display: flex;
  align-items: center;
  justify-content: space-between;
}

.detail-header h3 {
  font-size: 12px;
  font-weight: 600;
  margin: 0;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.detail-preview {
  background: #0a0a0a;
  padding: 12px;
  display: flex;
  align-items: center;
  justify-content: center;
}

.detail-preview img {
  max-width: 100%;
  max-height: 200px;
  border-radius: 4px;
}

.detail-meta {
  padding: 12px;
  border-bottom: 1px solid rgba(255, 255, 255, 0.06);
}

.meta-row {
  display: flex;
  justify-content: space-between;
  font-size: 11px;
  padding: 4px 0;
}

.meta-row .label {
  color: var(--text-tertiary);
}

.detail-actions {
  padding: 12px;
  display: flex;
  flex-direction: column;
  gap: 6px;
  border-bottom: 1px solid rgba(255, 255, 255, 0.06);
}

.action-btn {
  width: 100%;
  padding: 8px;
  font-size: 11px;
  background: rgba(255, 255, 255, 0.08);
  border: none;
  border-radius: 5px;
  color: var(--text-primary);
  cursor: pointer;
}

.action-btn:hover {
  background: rgba(255, 255, 255, 0.12);
}

.action-btn.primary {
  background: rgba(255, 255, 255, 0.15);
  color: var(--text-primary);
  font-weight: 500;
}

.action-btn.primary:hover {
  filter: brightness(1.1);
}

.detail-usage {
  flex: 1;
  overflow-y: auto;
  padding: 12px;
}

.usage-header {
  font-size: 10px;
  font-weight: 600;
  color: var(--text-tertiary);
  text-transform: uppercase;
  letter-spacing: 0.5px;
  margin-bottom: 8px;
}

.usage-empty {
  font-size: 11px;
  color: var(--text-tertiary);
  font-style: italic;
}

.usage-list {
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.usage-item {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: 6px 8px;
  background: rgba(255, 255, 255, 0.04);
  border-radius: 4px;
  font-size: 10px;
}

.usage-title {
  color: var(--text-primary);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.usage-line {
  color: var(--text-tertiary);
  flex-shrink: 0;
  margin-left: 8px;
}

.detail-url {
  padding: 12px;
  border-top: 1px solid rgba(255, 255, 255, 0.06);
}

.detail-url input {
  width: 100%;
  padding: 8px;
  font-size: 9px;
  font-family: 'SF Mono', monospace;
  background: rgba(0, 0, 0, 0.4);
  border: 1px solid rgba(255, 255, 255, 0.08);
  border-radius: 4px;
  color: var(--text-secondary);
  cursor: text;
}
</style>
