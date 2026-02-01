<script setup lang="ts">
interface UsageScanResult {
  usage_map: {
    by_asset: Record<string, unknown[]>
    total_posts: number
  }
}

const props = defineProps<{
  folders: string[]
  selectedFolder: string | null
  showUnusedOnly: boolean
  usageData: UsageScanResult | null
  loadingFolders: boolean
  loadingUsage: boolean
}>()

const emit = defineEmits<{
  selectFolder: [folder: string | null]
  toggleUnused: []
  rescan: []
}>()

const unusedCountLabel = () => {
  if (!props.usageData) return '…'
  return Object.keys(props.usageData.usage_map.by_asset).length > 0 ? '?' : '0'
}
</script>

<template>
  <div class="sidebar">
    <div class="sidebar-header">Folders</div>
    <div class="folder-list">
      <button
        class="folder-item"
        :class="{ active: selectedFolder === null && !showUnusedOnly }"
        @click="emit('selectFolder', null)"
      >
        <span class="folder-icon">📁</span>
        <span>All Assets</span>
      </button>
      <button
        class="folder-item"
        :class="{ active: showUnusedOnly }"
        @click="emit('toggleUnused')"
      >
        <span class="folder-icon">👻</span>
        <span>Unused</span>
        <span v-if="usageData" class="folder-count">{{ unusedCountLabel() }}</span>
      </button>
      <div class="folder-divider"></div>
      <button
        v-for="folder in folders"
        :key="folder"
        class="folder-item"
        :class="{ active: selectedFolder === folder }"
        @click="emit('selectFolder', folder)"
      >
        <span class="folder-icon">📂</span>
        <span>{{ folder }}</span>
      </button>
      <div v-if="loadingFolders" class="folder-loading">Loading...</div>
    </div>

    <div class="sidebar-footer">
      <div v-if="usageData" class="usage-stats">
        <span>{{ usageData.usage_map.total_posts }} posts scanned</span>
        <button @click="emit('rescan')" :disabled="loadingUsage" class="rescan-btn">
          {{ loadingUsage ? '...' : '↻' }}
        </button>
      </div>
    </div>
  </div>
</template>

<style scoped>
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
</style>
