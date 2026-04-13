<script setup lang="ts">
import { ref, computed } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { convertFileSrc } from '@tauri-apps/api/core'
import { PhArrowsClockwise, PhCheck, PhUpload, PhSparkle } from '@phosphor-icons/vue'

interface OgImageVariants {
  slug: string
  paths: string[]
  preview_html: string
}

const props = defineProps<{ slug: string }>()
const emit = defineEmits<{ picked: [url: string]; close: [] }>()

const generating = ref(false)
const uploading = ref(false)
const variants = ref<OgImageVariants | null>(null)
const selectedIdx = ref<number | null>(null)
const uploadedUrl = ref<string | null>(null)
const error = ref<string | null>(null)
const batch = ref(0)

// Convert file paths to displayable URLs (Tauri asset protocol)
const variantUrls = computed(() => (variants.value?.paths || []).map((p) => convertFileSrc(p)))

async function generate() {
  generating.value = true
  error.value = null
  selectedIdx.value = null
  uploadedUrl.value = null
  try {
    const result = await invoke<OgImageVariants>('generate_og_variants', {
      slug: props.slug,
      batch: batch.value,
    })
    variants.value = result
  } catch (e) {
    error.value = `${e}`
  }
  generating.value = false
}

async function reroll() {
  batch.value++
  await generate()
}

function select(idx: number) {
  selectedIdx.value = idx
}

async function uploadSelected() {
  if (selectedIdx.value === null || !variants.value) return
  uploading.value = true
  error.value = null
  try {
    const filePath = variants.value.paths[selectedIdx.value]
    const url = await invoke<string>('upload_og_image', {
      filePath,
      slug: props.slug,
    })
    uploadedUrl.value = url
    emit('picked', url)
  } catch (e) {
    error.value = `Upload failed: ${e}`
  }
  uploading.value = false
}

// Auto-generate on mount
generate()
</script>

<template>
  <div class="og-picker">
    <!-- Header -->
    <div class="picker-header">
      <PhSparkle :size="14" weight="fill" class="header-icon" />
      <span class="header-title">OG Image</span>
      <span v-if="variants" class="header-batch">batch {{ batch }}</span>
      <button class="reroll-btn" @click="reroll" :disabled="generating" title="Generate 4 new variants">
        <PhArrowsClockwise :size="12" weight="bold" :class="{ spinning: generating }" />
        {{ generating ? '' : 'Reroll' }}
      </button>
    </div>

    <!-- Error -->
    <div v-if="error" class="picker-error">{{ error }}</div>

    <!-- Generating -->
    <div v-if="generating && !variants" class="picker-loading">
      <PhSparkle :size="14" weight="fill" class="spin" />
      Rendering 4 variants...
    </div>

    <!-- Gallery -->
    <div v-if="variants && variants.paths.length > 0" class="picker-gallery">
      <div
        v-for="(url, i) in variantUrls"
        :key="i"
        class="variant-card"
        :class="{ selected: selectedIdx === i, generating }"
        @click="select(i)"
      >
        <img :src="url" :alt="`OG variant ${i}`" loading="eager" />
        <div class="variant-label">v{{ i }}</div>
        <div v-if="selectedIdx === i" class="variant-check">
          <PhCheck :size="16" weight="bold" />
        </div>
      </div>
    </div>

    <!-- Uploaded success -->
    <div v-if="uploadedUrl" class="picker-success">
      <PhCheck :size="12" weight="bold" />
      Uploaded
    </div>

    <!-- Actions -->
    <div v-if="variants && !uploadedUrl" class="picker-actions">
      <button class="upload-btn" :disabled="selectedIdx === null || uploading" @click="uploadSelected">
        <PhUpload :size="12" weight="bold" />
        {{ uploading ? 'Uploading...' : 'Use this one' }}
      </button>
    </div>
  </div>
</template>

<style scoped>
.og-picker {
  border-bottom: 1px solid var(--border);
  padding: 8px 16px 12px;
}

.picker-header {
  display: flex;
  align-items: center;
  gap: 8px;
  margin-bottom: 8px;
}

.header-icon {
  color: #a78bfa;
}

.header-title {
  font-size: 10px;
  font-weight: 600;
  color: var(--text-tertiary);
  text-transform: uppercase;
  letter-spacing: 0.75px;
}

.header-batch {
  font-size: 9px;
  color: var(--text-tertiary);
  font-family: monospace;
}

.reroll-btn {
  margin-left: auto;
  display: flex;
  align-items: center;
  gap: 4px;
  padding: 3px 8px;
  font-size: 10px;
  font-family: inherit;
  background: rgba(255, 255, 255, 0.05);
  color: var(--text-secondary);
  border: 1px solid rgba(255, 255, 255, 0.1);
  border-radius: 4px;
  cursor: pointer;
}

.reroll-btn:hover {
  border-color: rgba(255, 255, 255, 0.2);
}
.reroll-btn:disabled {
  opacity: 0.4;
  cursor: wait;
}

.spinning {
  animation: spin 0.8s linear infinite;
}
.spin {
  animation: spin 1.5s linear infinite;
  color: #a78bfa;
}
@keyframes spin {
  from {
    transform: rotate(0deg);
  }
  to {
    transform: rotate(360deg);
  }
}

.picker-error {
  font-size: 10px;
  color: var(--warning);
  margin-bottom: 6px;
}

.picker-loading {
  font-size: 10px;
  color: var(--text-tertiary);
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 12px 0;
}

/* Gallery: 2x2 grid */
.picker-gallery {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 6px;
  margin-bottom: 8px;
}

.variant-card {
  position: relative;
  border-radius: 4px;
  overflow: hidden;
  cursor: pointer;
  border: 2px solid transparent;
  transition:
    border-color 0.15s,
    opacity 0.15s;
}

.variant-card:hover {
  border-color: rgba(255, 255, 255, 0.15);
}
.variant-card.selected {
  border-color: #ef4444;
}
.variant-card.generating {
  opacity: 0.4;
  pointer-events: none;
}

.variant-card img {
  width: 100%;
  display: block;
  aspect-ratio: 1200 / 630;
  object-fit: cover;
  background: #18181b;
}

.variant-label {
  position: absolute;
  top: 3px;
  left: 3px;
  font-size: 8px;
  font-family: monospace;
  background: rgba(0, 0, 0, 0.6);
  color: var(--text-tertiary);
  padding: 1px 4px;
  border-radius: 2px;
}

.variant-check {
  position: absolute;
  top: 50%;
  left: 50%;
  transform: translate(-50%, -50%);
  color: #ef4444;
  background: rgba(0, 0, 0, 0.5);
  border-radius: 50%;
  width: 28px;
  height: 28px;
  display: flex;
  align-items: center;
  justify-content: center;
}

.picker-success {
  font-size: 10px;
  color: #4ade80;
  display: flex;
  align-items: center;
  gap: 4px;
  padding: 4px 0;
}

.picker-actions {
  display: flex;
  justify-content: flex-end;
}

.upload-btn {
  display: flex;
  align-items: center;
  gap: 4px;
  padding: 4px 12px;
  font-size: 10px;
  font-family: inherit;
  background: #7c3aed;
  color: #fff;
  border: none;
  border-radius: 4px;
  cursor: pointer;
}

.upload-btn:hover {
  background: #6d28d9;
}
.upload-btn:disabled {
  opacity: 0.4;
  cursor: not-allowed;
}
</style>
