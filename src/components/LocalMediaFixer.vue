<script setup lang="ts">
import { ref, computed } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { PhX } from '@phosphor-icons/vue'
import UiButton from './ui/UiButton.vue'

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
  /** Upload folder (Cloudinary path / R2 prefix). Should mirror the
   *  drag-drop handler's derivation so screenshots fixed via this modal
   *  land next to those uploaded by drag. Defaults to "blog" if omitted
   *  so older callers keep working. */
  folder?: string
}

const props = defineProps<Props>()
const emit = defineEmits<{
  close: []
  fixed: []
  /** User chose to chain into AltTextReviewer after a successful apply.
   *  Parent should close this modal and open the Describe modal. */
  'open-alt-text': []
}>()

const uploading = ref(false)
const uploadProgress = ref(0)
const currentUpload = ref<string | null>(null)
const results = ref<MediaFixResult[]>([])
const error = ref<string | null>(null)
const showConfirmApply = ref(false)
/** In-flight guard for applyFixes — a double-click would rewrite the source
 *  file twice (the second pass's originals are already gone), a wasted commit
 *  at best and content corruption at worst. */
const applying = ref(false)
/** Set true after a successful `applyFixes()` so the modal can pivot to
 *  the "now describe images" handoff instead of just closing. */
const justApplied = ref(false)

const successCount = computed(() => results.value.filter((r) => r.upload_result.success).length)

const failCount = computed(() => results.value.filter((r) => !r.upload_result.success).length)

const fixesToApply = computed(() =>
  results.value
    .filter((r) => r.upload_result.success && r.replacement_text)
    .map((r) => [r.original_ref.original_text, r.replacement_text!] as [string, string]),
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
        folder: props.folder || 'blog',
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
  if (fixesToApply.value.length === 0 || applying.value) return

  applying.value = true
  try {
    await invoke('apply_media_fixes', {
      filePath: props.filePath,
      fixes: fixesToApply.value,
    })
    emit('fixed')
    // Don't auto-close: pivot the modal into a "done — what's next?" state
    // so the user can chain into Alt Text Review without going back to the
    // sidebar to find the button.
    showConfirmApply.value = false
    justApplied.value = true
  } catch (e) {
    error.value = `Failed to apply fixes: ${String(e)}`
  } finally {
    applying.value = false
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
        <h2>
          <template v-if="justApplied">Done</template>
          <template v-else-if="results.length === 0">Upload local images</template>
          <template v-else>Upload results</template>
        </h2>
        <span v-if="!justApplied" class="count">{{ localMedia.length }} files</span>
        <UiButton variant="plain" :icon="PhX" class="close-btn" aria-label="Close" data-tip="Close" @click="$emit('close')" />
      </div>

      <div v-if="!uploading && !justApplied && results.length === 0" class="intro-blurb">
        Each file gets uploaded to Cloudinary, and the markdown reference is
        rewritten to point at the public URL. After that you can generate alt
        text for them.
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
        <div class="progress-text">Uploading: {{ currentUpload }}</div>
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
        <p>This will rewrite the markdown file:</p>
        <code>{{ filePath }}</code>
        <p>{{ fixesToApply.length }} reference{{ fixesToApply.length === 1 ? '' : 's' }} will be replaced with Cloudinary URLs.</p>
        <div class="confirm-actions">
          <UiButton variant="plain" :disabled="applying" label="Cancel" @click="showConfirmApply = false" />
          <UiButton variant="primary" :disabled="applying" :label="applying ? 'Applying…' : 'Apply changes'" @click="applyFixes" />
        </div>
      </div>

      <!-- "Done — what's next?" state, shown after applyFixes succeeds. -->
      <div v-if="justApplied" class="done-state">
        <div class="done-icon">✓</div>
        <h3>{{ successCount }} image{{ successCount === 1 ? '' : 's' }} uploaded</h3>
        <p>Your markdown now points at public URLs. Next up: generate alt text descriptions for each one.</p>
      </div>

      <div class="modal-footer">
        <template v-if="justApplied">
          <UiButton variant="plain" label="Close" @click="$emit('close')" />
          <UiButton variant="primary" label="Describe images →" @click="$emit('open-alt-text')" />
        </template>
        <template v-else-if="results.length === 0">
          <UiButton variant="plain" label="Cancel" @click="$emit('close')" />
          <UiButton
            variant="primary"
            :disabled="uploading || localMedia.every((m) => !m.resolved_path)"
            :label="uploading ? 'Uploading…' : `Upload ${localMedia.filter((m) => m.resolved_path).length} to Cloudinary`"
            @click="uploadAll"
          />
        </template>
        <template v-else-if="successCount > 0">
          <UiButton label="Copy to clipboard" @click="copyFixedContent" />
          <UiButton variant="primary" label="Apply to source file" @click="showConfirmApply = true" />
        </template>
        <template v-else>
          <UiButton variant="plain" label="Close" @click="$emit('close')" />
        </template>
      </div>
    </div>
  </div>
</template>

<style scoped>
.modal-overlay {
  position: fixed;
  inset: 0;
  background: var(--scrim);
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
  background: var(--modal-bg);
  backdrop-filter: blur(24px) saturate(180%);
  -webkit-backdrop-filter: blur(24px) saturate(180%);
  border: 1px solid var(--separator);
  border-radius: 12px;
  display: flex;
  flex-direction: column;
  overflow: hidden;
  box-shadow: var(--shadow-lg);
}

.modal-header {
  padding: 14px 16px;
  border-bottom: 1px solid var(--separator);
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
  background: color-mix(in srgb, var(--warning) 20%, transparent);
  padding: 2px 8px;
  border-radius: 4px;
}

.intro-blurb {
  padding: 12px 16px;
  font-size: 11px;
  line-height: 1.5;
  color: var(--text-secondary);
  background: var(--bg-tertiary);
  border-bottom: 1px solid var(--border);
}

.close-btn {
  margin-left: auto;
}

.error-banner {
  padding: 10px 16px;
  background: color-mix(in srgb, var(--danger) 18%, transparent);
  color: var(--danger);
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
  background: var(--control-bg);
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
  background: var(--fill-2);
  border-radius: 3px;
  overflow: hidden;
}

.progress-fill {
  height: 100%;
  background: var(--hover-bg);
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
  border-bottom: 1px solid var(--separator);
}

.success-count {
  color: var(--success);
  font-size: 12px;
  font-weight: 500;
}

.fail-count {
  color: var(--danger);
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
  background: var(--tint-live);
}

.result-item.error {
  background: var(--tint-broken);
}

.result-item.missing {
  background: var(--tint-attention);
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
  color: var(--danger);
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
  color: var(--danger);
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
  background: var(--control-bg);
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

/* "Done — what's next?" state shown after Apply succeeds. */
.done-state {
  padding: 28px 24px 12px;
  text-align: center;
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 8px;
}
.done-icon {
  width: 36px;
  height: 36px;
  border-radius: 50%;
  background: color-mix(in srgb, var(--success) 22%, transparent);
  color: var(--success);
  font-size: 18px;
  font-weight: 700;
  display: flex;
  align-items: center;
  justify-content: center;
}
.done-state h3 {
  font-size: 14px;
  font-weight: 600;
  margin: 0;
  color: var(--text-primary);
}
.done-state p {
  font-size: 12px;
  color: var(--text-secondary);
  max-width: 360px;
  line-height: 1.5;
  margin: 0;
}

.modal-footer {
  padding: var(--space-3) var(--gutter);
  border-top: 1px solid var(--border);
  display: flex;
  gap: var(--space-2);
  justify-content: flex-end;
}

</style>
