<script setup lang="ts">
import { ref, computed, watch, onMounted } from 'vue'
import { invoke } from '@tauri-apps/api/tauri'
import MediaLibrarySidebar from './MediaLibrarySidebar.vue'
import MediaLibraryGrid from './MediaLibraryGrid.vue'
import MediaLibraryDetail from './MediaLibraryDetail.vue'

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

defineProps<{
  selectedFile?: SelectedFile | null
  inline?: boolean
}>()

const emit = defineEmits<{
  close: []
  select: [asset: CloudinaryAsset]
  insert: [markdown: string]
}>()

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

function handleSelectFolder(folder: string | null) {
  selectedFolder.value = folder
  showUnusedOnly.value = false
  loadAssets()
}

function handleToggleUnused() {
  showUnusedOnly.value = !showUnusedOnly.value
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
      <MediaLibrarySidebar
        :folders="folders"
        :selected-folder="selectedFolder"
        :show-unused-only="showUnusedOnly"
        :usage-data="usageData"
        :loading-folders="loadingFolders"
        :loading-usage="loadingUsage"
        @select-folder="handleSelectFolder"
        @toggle-unused="handleToggleUnused"
        @rescan="scanUsage"
      />

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

        <Transition name="toast">
          <div v-if="copiedText" class="copied-toast">{{ copiedText }} copied!</div>
        </Transition>

        <MediaLibraryGrid
          :assets="filteredAssets"
          :selected-index="selectedIndex"
          :loading="loading"
          :error="error"
          :next-cursor="nextCursor"
          :show-unused-only="showUnusedOnly"
          :get-thumbnail-url="getThumbnailUrl"
          :get-usage-count="getUsageCount"
          @show-detail="showAssetDetail"
          @hover-index="selectedIndex = $event"
          @load-more="loadAssets(true)"
        />
      </div>

      <MediaLibraryDetail
        :show="showDetail"
        :asset="detailAsset"
        :usage="detailUsage"
        :format-bytes="formatBytes"
        :get-preview-url="getPreviewUrl"
        @close="showDetail = false"
        @copy-markdown="copyMarkdown"
        @copy-url="copyUrl"
      />
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

:deep(.inline-panel .sidebar) {
  width: 150px;
  min-width: 150px;
}

.inline-panel .modal-header h2 {
  display: none;
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
</style>
