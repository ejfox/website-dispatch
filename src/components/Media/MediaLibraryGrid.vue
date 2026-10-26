<script setup lang="ts">
import { Menu, MenuItem, PredefinedMenuItem } from '@tauri-apps/api/menu'
import UiButton from '../ui/UiButton.vue'

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

defineProps<{
  assets: CloudinaryAsset[]
  selectedIndex: number
  loading: boolean
  error: string | null
  nextCursor: string | null
  showUnusedOnly: boolean
  getThumbnailUrl: (asset: CloudinaryAsset) => string
  getUsageCount: (publicId: string) => number
}>()

const emit = defineEmits<{
  showDetail: [asset: CloudinaryAsset]
  hoverIndex: [index: number]
  loadMore: []
}>()

async function showAssetContextMenu(asset: CloudinaryAsset, e: MouseEvent) {
  e.preventDefault()
  const name = asset.public_id.split('/').pop() || asset.public_id
  const mdImage = `![](${asset.secure_url})`
  const mdLink = `[${name}](${asset.secure_url})`
  const cloudinaryConsole = `https://console.cloudinary.com/console/media_library/folders/home?searchQuery=${encodeURIComponent(asset.public_id)}`

  const menu = await Menu.new({
    items: [
      await MenuItem.new({ text: 'Show Details…', action: () => emit('showDetail', asset) }),
      await PredefinedMenuItem.new({ item: 'Separator' }),
      await MenuItem.new({
        text: 'Copy Link',
        action: () => navigator.clipboard.writeText(asset.secure_url),
      }),
      await MenuItem.new({
        text: 'Copy as Markdown Image',
        action: () => navigator.clipboard.writeText(mdImage),
      }),
      await MenuItem.new({
        text: 'Copy as Markdown Link',
        action: () => navigator.clipboard.writeText(mdLink),
      }),
      await MenuItem.new({
        text: 'Copy Public ID',
        action: () => navigator.clipboard.writeText(asset.public_id),
      }),
      await PredefinedMenuItem.new({ item: 'Separator' }),
      await MenuItem.new({
        text: 'Open Image',
        action: () => window.open(asset.secure_url, '_blank'),
      }),
      await MenuItem.new({
        text: 'Show in Cloudinary Console',
        action: () => window.open(cloudinaryConsole, '_blank'),
      }),
    ],
  })
  await menu.popup()
}
</script>

<template>
  <div v-if="error" class="error">{{ error }}</div>

  <div v-if="loading && assets.length === 0" class="loading">Loading...</div>

  <!-- Nothing to show: say why, and how things get here. -->
  <div v-else-if="!error && assets.length === 0" class="grid-empty">
    <div class="grid-empty-title">{{ showUnusedOnly ? 'Every file here is used in a post' : 'No media here yet' }}</div>
    <div v-if="!showUnusedOnly" class="grid-empty-sub">
      Drag images or video onto the window to upload them, or try another folder or search.
    </div>
  </div>

  <div v-else class="assets-grid">
    <div
      v-for="(asset, i) in assets"
      :key="asset.public_id"
      class="asset-card"
      :class="{ selected: i === selectedIndex }"
      @click="emit('showDetail', asset)"
      @mouseenter="emit('hoverIndex', i)"
      @contextmenu="showAssetContextMenu(asset, $event)"
    >
      <div class="asset-thumbnail">
        <img :src="getThumbnailUrl(asset)" :alt="asset.public_id" loading="lazy" />
        <span v-if="asset.resource_type === 'video'" class="badge video">VID</span>
        <span v-if="getUsageCount(asset.public_id) === 0" class="badge unused" data-tip="Not used in any post" aria-label="Not used in any post">•</span>
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
    <UiButton size="small" :disabled="loading" :label="loading ? '…' : 'Load More'" @click="emit('loadMore')" />
  </div>

  <div class="modal-footer">
    <span class="hint">
      <kbd>↑↓←→</kbd>
      navigate
      <kbd>Enter</kbd>
      details
      <kbd>c</kbd>
      copy URL
      <kbd>i</kbd>
      copy markdown
      <kbd>u</kbd>
      toggle unused
    </span>
  </div>
</template>

<style scoped>
.error {
  padding: 16px;
  color: var(--danger);
  text-align: center;
  font-size: 12px;
}

.grid-empty {
  flex: 1;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: var(--space-1);
  padding: var(--space-6) var(--space-4);
  text-align: center;
  color: var(--text-tertiary);
  font-size: 11px;
  line-height: 1.45;
}
.grid-empty-title {
  font-size: 13px;
  font-weight: 500;
  color: var(--text-secondary);
}
.grid-empty-sub {
  max-width: 280px;
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
  grid-template-columns: repeat(auto-fill, minmax(100px, 1fr));
  gap: 6px;
  align-content: start;
}

.asset-card {
  background: var(--fill);
  border: 2px solid transparent;
  border-radius: 6px;
  overflow: hidden;
  cursor: pointer;
  transition: all 0.1s ease;
}

.asset-card:hover {
  border-color: var(--label-3);
}

.asset-card.selected {
  border-color: var(--accent);
}

.asset-thumbnail {
  position: relative;
  height: 80px;
  background: var(--under-page);
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

/* .video / .unused / .used sit ON the thumbnail image, so they keep fixed
   light-on-photo colors in both modes (an image doesn't change with the
   appearance). */
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
  background: var(--status-live);
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
  padding: var(--space-2);
  text-align: center;
}
.modal-footer {
  padding: 8px 12px;
  border-top: 1px solid var(--separator);
  background: var(--fill);
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
  background: var(--fill);
  border: 1px solid var(--separator);
  border-radius: 2px;
  margin: 0 2px;
}
</style>
