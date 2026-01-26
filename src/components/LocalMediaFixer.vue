<script setup lang="ts">
import { ref, computed } from 'vue'
import { invoke } from '@tauri-apps/api/tauri'

interface LocalMediaRef {
  original_text: string
  path: string
  resolved_path: string | null
  alt_text: string | null
  media_type: string
  line_number: number
}

interface UploadResult {
  success: boolean
  asset: {
    public_id: string
    secure_url: string
    resource_type: string
    format: string
  } | null
  error: string | null
}

interface MediaFixResult {
  original_ref: LocalMediaRef
  upload_result: UploadResult
  replacement_text: string | null
}

interface Props {
  filePath: string
  localMedia: LocalMediaRef[]
}

const props = defineProps<Props>()
const emit = defineEmits<{
  close: []
  fixed: []
}>()

const uploading = ref(false)
const uploadProgress = ref(0)
const currentUpload = ref<string | null>(null)
const results = ref<MediaFixResult[]>([])
const error = ref<string | null>(null)
const showConfirmApply = ref(false)

const successCount = computed(() =>
  results.value.filter(r => r.upload_result.success).length
)

const failCount = computed(() =>
  results.value.filter(r => !r.upload_result.success).length
)

const fixesToApply = computed(() =>
  results.value
    .filter(r => r.upload_result.success && r.replacement_text)
    .map(r => [r.original_ref.original_text, r.replacement_text!] as [string, string])
)

async function uploadAll() {
  uploading.value = true
  uploadProgress.value = 0
  error.value = null
  results.value = []

  try {
    // Upload each file one by one for progress tracking
    for (let i = 0; i < props.localMedia.length; i++) {
      const media = props.localMedia[i]
      currentUpload.value = media.path

      const fixResults: MediaFixResult[] = await invoke('fix_local_media', {
        sourcePath: props.filePath,
        mediaRefs: [media],
        folder: 'blog' // Default folder for blog images
      })

      results.value.push(...fixResults)
      uploadProgress.value = ((i + 1) / props.localMedia.length) * 100
    }
  } catch (e) {
    error.value = String(e)
  }

  uploading.value = false
  currentUpload.value = null
}

async function applyFixes() {
  if (fixesToApply.value.length === 0) return

  try {
    await invoke('apply_media_fixes', {
      filePath: props.filePath,
      fixes: fixesToApply.value
    })
    emit('fixed')
    emit('close')
  } catch (e) {
    error.value = `Failed to apply fixes: ${String(e)}`
  }
}

function copyFixedContent() {
  if (fixesToApply.value.length === 0) return

  // Read original content and apply replacements
  invoke('get_file_content', { path: props.filePath }).then((content: unknown) => {
    let fixed = content as string
    for (const [original, replacement] of fixesToApply.value) {
      fixed = fixed.replace(original, replacement)
    }
    navigator.clipboard.writeText(fixed)
  })
}

function getStatusClass(result: MediaFixResult): string {
  if (result.upload_result.success) return 'success'
  if (result.upload_result.error?.includes('not found')) return 'missing'
  return 'error'
}
</script>

<template>
  <div class="modal-overlay" @click.self="$emit('close')">
    <div class="modal">
      <div class="modal-header">
        <h2>Fix Local Media</h2>
        <span class="count">{{ localMedia.length }} files</span>
        <button class="close-btn" @click="$emit('close')">
          <svg width="14" height="14" viewBox="0 0 16 16" fill="currentColor">
            <path d="M4.646 4.646a.5.5 0 0 1 .708 0L8 7.293l2.646-2.647a.5.5 0 0 1 .708.708L8.707 8l2.647 2.646a.5.5 0 0 1-.708.708L8 8.707l-2.646 2.647a.5.5 0 0 1-.708-.708L7.293 8 4.646 5.354a.5.5 0 0 1 0-.708z"/>
          </svg>
        </button>
      </div>

      <div v-if="error" class="error-banner">
        {{ error }}
      </div>

      <!-- Before upload: show list of media to fix -->
      <div v-if="results.length === 0" class="media-list">
        <div v-for="media in localMedia" :key="media.path + media.line_number" class="media-item">
          <div class="media-icon" :class="media.media_type">
            {{ media.media_type === 'video' ? '🎬' : '🖼' }}
          </div>
          <div class="media-info">
            <div class="media-path">{{ media.path }}</div>
            <div class="media-meta">
              <span>Line {{ media.line_number }}</span>
              <span v-if="media.alt_text" class="alt-text">alt: "{{ media.alt_text }}"</span>
              <span v-if="!media.resolved_path" class="missing">File not found</span>
            </div>
          </div>
        </div>
      </div>

      <!-- During upload: show progress -->
      <div v-if="uploading" class="upload-progress">
        <div class="progress-bar">
          <div class="progress-fill" :style="{ width: uploadProgress + '%' }"></div>
        </div>
        <div class="progress-text">
          Uploading: {{ currentUpload }}
        </div>
      </div>

      <!-- After upload: show results -->
      <div v-if="results.length > 0 && !uploading" class="results">
        <div class="results-summary">
          <span v-if="successCount > 0" class="success-count">{{ successCount }} uploaded</span>
          <span v-if="failCount > 0" class="fail-count">{{ failCount }} failed</span>
        </div>

        <div class="results-list">
          <div
            v-for="result in results"
            :key="result.original_ref.path"
            class="result-item"
            :class="getStatusClass(result)"
          >
            <div class="result-icon">
              {{ result.upload_result.success ? '✓' : '✗' }}
            </div>
            <div class="result-info">
              <div class="result-path">{{ result.original_ref.path }}</div>
              <div v-if="result.upload_result.success" class="result-url">
                → {{ result.upload_result.asset?.secure_url }}
              </div>
              <div v-else class="result-error">
                {{ result.upload_result.error }}
              </div>
            </div>
          </div>
        </div>
      </div>

      <!-- Confirm apply dialog -->
      <div v-if="showConfirmApply" class="confirm-dialog">
        <p>This will modify the source file:</p>
        <code>{{ filePath }}</code>
        <p>{{ fixesToApply.length }} replacement(s) will be made.</p>
        <div class="confirm-actions">
          <button @click="showConfirmApply = false">Cancel</button>
          <button class="danger" @click="applyFixes">Apply Changes</button>
        </div>
      </div>

      <div class="modal-footer">
        <template v-if="results.length === 0">
          <button
            @click="uploadAll"
            :disabled="uploading || localMedia.every(m => !m.resolved_path)"
            class="primary"
          >
            {{ uploading ? 'Uploading...' : 'Upload All to Cloudinary' }}
          </button>
        </template>
        <template v-else-if="successCount > 0">
          <button @click="copyFixedContent">Copy Fixed Content</button>
          <button @click="showConfirmApply = true" class="primary">
            Apply to Source File
          </button>
        </template>
        <template v-else>
          <button @click="$emit('close')">Close</button>
        </template>
      </div>
    </div>
  </div>
</template>

<style scoped>
.modal-overlay {
  position: fixed;
  inset: 0;
  background: rgba(0, 0, 0, 0.6);
  backdrop-filter: blur(4px);
  -webkit-backdrop-filter: blur(4px);
  z-index: 200;
  display: flex;
  align-items: center;
  justify-content: center;
}

.modal {
  width: 550px;
  max-width: 95vw;
  max-height: 80vh;
  background: rgba(30, 30, 34, 0.95);
  backdrop-filter: blur(24px) saturate(180%);
  -webkit-backdrop-filter: blur(24px) saturate(180%);
  border: 1px solid rgba(255, 255, 255, 0.1);
  border-radius: 12px;
  display: flex;
  flex-direction: column;
  overflow: hidden;
  box-shadow: 0 24px 48px rgba(0, 0, 0, 0.5);
}

.modal-header {
  padding: 14px 16px;
  border-bottom: 1px solid rgba(255, 255, 255, 0.08);
  display: flex;
  align-items: center;
  gap: 12px;
}

.modal-header h2 {
  font-size: 14px;
  font-weight: 600;
  margin: 0;
}

.count {
  font-size: 11px;
  color: var(--warning);
  background: rgba(255, 159, 10, 0.2);
  padding: 2px 8px;
  border-radius: 4px;
}

.close-btn {
  margin-left: auto;
  background: none;
  border: none;
  color: var(--text-tertiary);
  cursor: pointer;
  padding: 4px;
  border-radius: 4px;
}

.close-btn:hover {
  background: rgba(255, 255, 255, 0.1);
  color: var(--text-primary);
}

.error-banner {
  padding: 10px 16px;
  background: rgba(255, 107, 107, 0.2);
  color: #ff6b6b;
  font-size: 12px;
}

.media-list {
  flex: 1;
  overflow-y: auto;
  padding: 8px;
}

.media-item {
  display: flex;
  align-items: flex-start;
  gap: 10px;
  padding: 10px;
  border-radius: 6px;
  margin-bottom: 4px;
  background: rgba(0, 0, 0, 0.2);
}

.media-icon {
  font-size: 18px;
  flex-shrink: 0;
}

.media-info {
  flex: 1;
  min-width: 0;
}

.media-path {
  font-size: 12px;
  color: var(--text-primary);
  font-family: 'SF Mono', monospace;
  word-break: break-all;
}

.media-meta {
  font-size: 10px;
  color: var(--text-tertiary);
  display: flex;
  gap: 10px;
  margin-top: 4px;
}

.alt-text {
  color: var(--text-secondary);
  font-style: italic;
}

.missing {
  color: var(--warning);
}

.upload-progress {
  padding: 24px;
}

.progress-bar {
  height: 6px;
  background: rgba(255, 255, 255, 0.1);
  border-radius: 3px;
  overflow: hidden;
}

.progress-fill {
  height: 100%;
  background: var(--accent);
  transition: width 0.3s ease;
}

.progress-text {
  margin-top: 12px;
  font-size: 11px;
  color: var(--text-tertiary);
  text-align: center;
  font-family: 'SF Mono', monospace;
}

.results {
  flex: 1;
  overflow-y: auto;
}

.results-summary {
  padding: 12px 16px;
  display: flex;
  gap: 12px;
  border-bottom: 1px solid rgba(255, 255, 255, 0.06);
}

.success-count {
  color: var(--success);
  font-size: 12px;
  font-weight: 500;
}

.fail-count {
  color: #ff6b6b;
  font-size: 12px;
  font-weight: 500;
}

.results-list {
  padding: 8px;
}

.result-item {
  display: flex;
  align-items: flex-start;
  gap: 10px;
  padding: 10px;
  border-radius: 6px;
  margin-bottom: 4px;
}

.result-item.success {
  background: rgba(48, 209, 88, 0.1);
}

.result-item.error {
  background: rgba(255, 107, 107, 0.1);
}

.result-item.missing {
  background: rgba(255, 159, 10, 0.1);
}

.result-icon {
  font-size: 14px;
  flex-shrink: 0;
}

.result-item.success .result-icon {
  color: var(--success);
}

.result-item.error .result-icon,
.result-item.missing .result-icon {
  color: #ff6b6b;
}

.result-info {
  flex: 1;
  min-width: 0;
}

.result-path {
  font-size: 11px;
  color: var(--text-primary);
  font-family: 'SF Mono', monospace;
}

.result-url {
  font-size: 10px;
  color: var(--success);
  font-family: 'SF Mono', monospace;
  word-break: break-all;
  margin-top: 4px;
}

.result-error {
  font-size: 10px;
  color: #ff6b6b;
  margin-top: 4px;
}

.confirm-dialog {
  padding: 20px;
  text-align: center;
}

.confirm-dialog p {
  font-size: 12px;
  color: var(--text-secondary);
  margin: 8px 0;
}

.confirm-dialog code {
  display: block;
  font-size: 11px;
  font-family: 'SF Mono', monospace;
  color: var(--text-primary);
  background: rgba(0, 0, 0, 0.3);
  padding: 8px 12px;
  border-radius: 4px;
  margin: 8px 0;
}

.confirm-actions {
  display: flex;
  gap: 8px;
  justify-content: center;
  margin-top: 16px;
}

.confirm-actions button {
  padding: 8px 16px;
  font-size: 12px;
  border-radius: 6px;
  cursor: pointer;
}

.confirm-actions button:first-child {
  background: rgba(255, 255, 255, 0.1);
  border: none;
  color: var(--text-secondary);
}

.confirm-actions button.danger {
  background: #ff6b6b;
  border: none;
  color: #fff;
}

.modal-footer {
  padding: 12px 16px;
  border-top: 1px solid rgba(255, 255, 255, 0.06);
  display: flex;
  gap: 8px;
  justify-content: flex-end;
}

.modal-footer button {
  padding: 8px 16px;
  font-size: 12px;
  border-radius: 6px;
  cursor: pointer;
  background: rgba(255, 255, 255, 0.1);
  border: none;
  color: var(--text-secondary);
}

.modal-footer button:hover {
  background: rgba(255, 255, 255, 0.15);
}

.modal-footer button:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

.modal-footer button.primary {
  background: var(--accent);
  color: #fff;
}

.modal-footer button.primary:hover {
  filter: brightness(1.1);
}
</style>
