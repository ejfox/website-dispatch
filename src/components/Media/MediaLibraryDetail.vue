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

interface AssetUsage {
  post_path: string
  post_title: string | null
  line_number: number
  context: string
}

defineProps<{
  show: boolean
  asset: CloudinaryAsset | null
  usage: AssetUsage[]
  formatBytes: (bytes: number) => string
  getPreviewUrl: (asset: CloudinaryAsset) => string
}>()

const emit = defineEmits<{
  close: []
  copyMarkdown: [asset: CloudinaryAsset]
  copyUrl: [asset: CloudinaryAsset]
}>()
</script>

<template>
  <Transition name="slide">
    <div v-if="show && asset" class="detail-panel">
      <div class="detail-header">
        <h3>{{ asset.public_id.split('/').pop() }}</h3>
        <button @click="emit('close')" class="close-btn">✕</button>
      </div>

      <div class="detail-preview">
        <img :src="getPreviewUrl(asset)" :alt="asset.public_id" />
      </div>

      <div class="detail-meta">
        <div class="meta-row">
          <span class="label">Size</span>
          <span>{{ formatBytes(asset.bytes) }}</span>
        </div>
        <div v-if="asset.width && asset.height" class="meta-row">
          <span class="label">Dimensions</span>
          <span>{{ asset.width }} × {{ asset.height }}</span>
        </div>
        <div class="meta-row">
          <span class="label">Format</span>
          <span>{{ asset.format.toUpperCase() }}</span>
        </div>
        <div class="meta-row">
          <span class="label">Type</span>
          <span>{{ asset.resource_type }}</span>
        </div>
        <div class="meta-row">
          <span class="label">Folder</span>
          <span>{{ asset.public_id.split('/').slice(0, -1).join('/') || '(root)' }}</span>
        </div>
      </div>

      <div class="detail-actions">
        <button @click="emit('copyMarkdown', asset)" class="action-btn primary">
          Copy Markdown
        </button>
        <button @click="emit('copyUrl', asset)" class="action-btn">
          Copy URL
        </button>
      </div>

      <div class="detail-usage">
        <div class="usage-header">
          <span>Used in {{ usage.length }} post{{ usage.length === 1 ? '' : 's' }}</span>
        </div>
        <div v-if="usage.length === 0" class="usage-empty">
          Not used in any posts
        </div>
        <div v-else class="usage-list">
          <div v-for="usageItem in usage" :key="usageItem.post_path + usageItem.line_number" class="usage-item">
            <span class="usage-title">{{ usageItem.post_title || usageItem.post_path.split('/').pop() }}</span>
            <span class="usage-line">line {{ usageItem.line_number }}</span>
          </div>
        </div>
      </div>

      <div class="detail-url">
        <input type="text" :value="asset.secure_url" readonly @click="($event.target as HTMLInputElement).select()" />
      </div>
    </div>
  </Transition>
</template>

<style scoped>
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
