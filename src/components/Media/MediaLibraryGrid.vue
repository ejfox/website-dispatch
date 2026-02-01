<script setup lang="ts">
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
</script>

<template>
  <div v-if="error" class="error">{{ error }}</div>

  <div v-if="loading && assets.length === 0" class="loading">Loading...</div>

  <div v-else class="assets-grid">
    <div
      v-for="(asset, i) in assets"
      :key="asset.public_id"
      class="asset-card"
      :class="{ selected: i === selectedIndex }"
      @click="emit('showDetail', asset)"
      @mouseenter="emit('hoverIndex', i)"
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
    <button @click="emit('loadMore')" :disabled="loading">
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
</template>

<style scoped>
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
</style>
