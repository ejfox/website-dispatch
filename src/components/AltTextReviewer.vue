<script setup lang="ts">
import { ref, computed, nextTick } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { PhCheck, PhX, PhPencilSimple, PhSparkle, PhCloudArrowUp, PhFilmSlate } from '@phosphor-icons/vue'

interface AltTextSuggestion {
  image_url: string
  alt_text: string
  confidence: number
  line_number: number
}

interface AltTextResult {
  file_path: string
  suggestions: AltTextSuggestion[]
  skipped: number
}

const props = defineProps<{
  filePath: string
  count: number
  /** How many local-vault image refs the parent found in this file.
   *  When > 0 and we're in the empty state, we offer a one-click handoff
   *  to LocalMediaFixer instead of leaving the user at a dead end. */
  localImageCount?: number
  localVideoCount?: number
}>()
const emit = defineEmits<{ close: []; applied: []; 'open-local-fixer': [] }>()

// State
const generating = ref(false)
const progress = ref(0)
const progressLabel = ref('')
const suggestions = ref<AltTextSuggestion[]>([])
const accepted = ref<Set<number>>(new Set())
const editingIdx = ref<number | null>(null)
const error = ref<string | null>(null)
const applying = ref(false)
const skippedCount = ref(0)

// Sorted: needs-review first, then by line number within each group
const sorted = computed(() => {
  const items = suggestions.value.map((s, i) => ({ ...s, originalIdx: i }))
  return items.sort((a, b) => {
    const aLow = a.confidence < 0.8 ? 0 : 1
    const bLow = b.confidence < 0.8 ? 0 : 1
    if (aLow !== bLow) return aLow - bLow
    return a.line_number - b.line_number
  })
})

const confidentCount = computed(() => suggestions.value.filter((s) => s.confidence >= 0.8).length)
const reviewCount = computed(() => suggestions.value.filter((s) => s.confidence < 0.8).length)
const acceptedCount = computed(() => accepted.value.size)

function thumbnailUrl(url: string): string {
  const u = url.replace(/^http:\/\//i, 'https://')
  const parts = u.split('/upload/')
  if (parts.length !== 2) return url
  return `${parts[0]}/upload/c_fill,w_80,h_56,f_auto,q_auto/${parts[1]}`
}

function confLabel(c: number): string {
  if (c >= 0.9) return 'high'
  if (c >= 0.8) return 'good'
  if (c >= 0.6) return 'low'
  return 'poor'
}

// Generate
async function generate() {
  generating.value = true
  progress.value = 0
  progressLabel.value = 'Starting...'
  error.value = null
  suggestions.value = []
  accepted.value = new Set()

  try {
    const result = await invoke<AltTextResult>('generate_alt_text', { filePath: props.filePath })
    suggestions.value = result.suggestions
    skippedCount.value = result.skipped

    // Auto-accept high confidence
    result.suggestions.forEach((s, i) => {
      if (s.confidence >= 0.8) accepted.value.add(i)
    })
  } catch (e) {
    error.value = `${e}`
  }

  generating.value = false
}

// Toggle accept
function toggleAccept(originalIdx: number) {
  if (accepted.value.has(originalIdx)) {
    accepted.value.delete(originalIdx)
  } else {
    accepted.value.add(originalIdx)
  }
  // Force reactivity
  accepted.value = new Set(accepted.value)
}

// Accept all
function acceptAll() {
  suggestions.value.forEach((_, i) => accepted.value.add(i))
  accepted.value = new Set(accepted.value)
}

// Edit
function startEdit(idx: number) {
  editingIdx.value = idx
  nextTick(() => {
    const el = document.querySelector(`.alt-edit-${idx}`) as HTMLTextAreaElement
    if (el) {
      el.focus()
      el.select()
    }
  })
}

function finishEdit() {
  editingIdx.value = null
}

// Apply accepted suggestions
async function applyAccepted() {
  const toApply = suggestions.value.filter((_, i) => accepted.value.has(i))
  if (toApply.length === 0) return

  applying.value = true
  try {
    await invoke<number>('apply_alt_text', {
      filePath: props.filePath,
      suggestions: toApply,
    })
    emit('applied')
    emit('close')
  } catch (e) {
    error.value = `Failed to apply: ${e}`
  }
  applying.value = false
}

// Start generating on mount
generate()
</script>

<template>
  <div class="modal-overlay" @click.self="$emit('close')">
    <div class="modal">
      <!-- Header -->
      <div class="modal-header">
        <PhSparkle :size="16" weight="fill" class="header-icon" />
        <h2>Alt Text Review</h2>
        <span class="header-count">{{ props.count }} images</span>
        <button class="close-btn" @click="$emit('close')">
          <PhX :size="14" weight="bold" />
        </button>
      </div>

      <!-- Error -->
      <div v-if="error" class="error-banner">{{ error }}</div>

      <!-- Generating -->
      <div v-if="generating" class="generating">
        <div class="progress-bar">
          <div class="progress-fill indeterminate"></div>
        </div>
        <div class="progress-text">
          <PhSparkle :size="12" weight="fill" class="spin" />
          Analyzing images with AI...
        </div>
      </div>

      <!-- Results -->
      <div v-else-if="suggestions.length > 0" class="results">
        <!-- Summary bar -->
        <div class="summary-bar">
          <span v-if="confidentCount > 0" class="summary-chip confident">
            <PhCheck :size="10" weight="bold" />
            {{ confidentCount }} confident
          </span>
          <span v-if="reviewCount > 0" class="summary-chip review">
            <PhPencilSimple :size="10" weight="bold" />
            {{ reviewCount }} needs review
          </span>
          <span v-if="skippedCount > 0" class="summary-chip skipped">{{ skippedCount }} skipped</span>
          <button v-if="acceptedCount < suggestions.length" class="accept-all-btn" @click="acceptAll">
            Accept all
          </button>
        </div>

        <!-- Review list -->
        <div class="review-list">
          <div
            v-for="item in sorted"
            :key="item.image_url"
            class="review-item"
            :class="{
              'is-accepted': accepted.has(item.originalIdx),
              'needs-review': item.confidence < 0.8,
            }"
          >
            <!-- Thumbnail -->
            <div class="review-thumb">
              <img
                :src="thumbnailUrl(item.image_url)"
                :alt="item.alt_text"
                loading="lazy"
                @error="($event.target as HTMLImageElement).style.display = 'none'"
              />
            </div>

            <!-- Content -->
            <div class="review-content">
              <!-- Confidence + line -->
              <div class="review-meta">
                <span class="conf-badge" :class="confLabel(item.confidence)">
                  {{ (item.confidence * 100).toFixed(0) }}%
                </span>
                <span class="line-num">L{{ item.line_number }}</span>
              </div>

              <!-- Alt text (view or edit) -->
              <textarea
                v-if="editingIdx === item.originalIdx"
                v-model="suggestions[item.originalIdx].alt_text"
                :class="'alt-textarea alt-edit-' + item.originalIdx"
                rows="2"
                @keydown.enter.prevent="finishEdit"
                @keydown.escape="finishEdit"
                @blur="finishEdit"
              />
              <div v-else class="alt-text" @click="startEdit(item.originalIdx)">
                {{ item.alt_text }}
              </div>
            </div>

            <!-- Actions -->
            <div class="review-actions">
              <button
                class="action-btn accept"
                :class="{ active: accepted.has(item.originalIdx) }"
                @click="toggleAccept(item.originalIdx)"
                :title="accepted.has(item.originalIdx) ? 'Undo accept' : 'Accept'"
              >
                <PhCheck :size="14" weight="bold" />
              </button>
              <button class="action-btn edit" @click="startEdit(item.originalIdx)" title="Edit">
                <PhPencilSimple :size="12" weight="bold" />
              </button>
            </div>
          </div>
        </div>
      </div>

      <!-- Empty state (all skipped) -->
      <div v-else-if="!generating" class="empty-state">
        <PhCloudArrowUp v-if="(localImageCount ?? 0) > 0" :size="32" weight="duotone" class="empty-icon" />
        <PhFilmSlate v-else :size="32" weight="duotone" class="empty-icon muted-icon" />

        <h3 class="empty-title">Nothing to describe yet</h3>

        <p class="empty-explain">
          Alt text generation needs a public image URL to send to the vision model.
          The {{ skippedCount }} image{{ skippedCount === 1 ? '' : 's' }} in this post
          {{ skippedCount === 1 ? 'is' : 'are' }} stored locally or are videos, so there's
          no URL to look at.
        </p>

        <ul class="empty-breakdown" v-if="(localImageCount ?? 0) > 0 || (localVideoCount ?? 0) > 0">
          <li v-if="(localImageCount ?? 0) > 0">
            <strong>{{ localImageCount }}</strong> local image{{ (localImageCount ?? 0) === 1 ? '' : 's' }}
            <span class="muted">— upload to Cloudinary to enable alt text</span>
          </li>
          <li v-if="(localVideoCount ?? 0) > 0">
            <strong>{{ localVideoCount }}</strong> video{{ (localVideoCount ?? 0) === 1 ? '' : 's' }}
            <span class="muted">— videos don't get alt text</span>
          </li>
        </ul>

        <div class="empty-actions" v-if="(localImageCount ?? 0) > 0">
          <button class="btn primary" @click="emit('open-local-fixer')">
            <PhCloudArrowUp :size="14" weight="bold" />
            Upload {{ localImageCount }} to Cloudinary
          </button>
          <span class="empty-hint">Then come back and run Describe again.</span>
        </div>
      </div>

      <!-- Footer -->
      <div class="modal-footer">
        <div class="footer-info" v-if="suggestions.length > 0 && !generating">
          {{ acceptedCount }}/{{ suggestions.length }} accepted
        </div>
        <div class="footer-actions">
          <button class="btn secondary" @click="$emit('close')">Cancel</button>
          <button
            v-if="suggestions.length > 0"
            class="btn primary"
            :disabled="acceptedCount === 0 || applying"
            @click="applyAccepted"
          >
            {{ applying ? 'Applying...' : `Apply ${acceptedCount} to file` }}
          </button>
        </div>
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
  width: 580px;
  max-width: 95vw;
  max-height: 85vh;
  background: rgba(30, 30, 34, 0.95);
  backdrop-filter: blur(24px) saturate(180%);
  -webkit-backdrop-filter: blur(24px) saturate(180%);
  border: 1px solid var(--border-light);
  border-radius: 12px;
  display: flex;
  flex-direction: column;
  overflow: hidden;
  box-shadow: 0 24px 48px rgba(0, 0, 0, 0.5);
}

/* Header */
.modal-header {
  padding: 14px 16px;
  border-bottom: 1px solid var(--border);
  display: flex;
  align-items: center;
  gap: 10px;
}

.header-icon {
  color: var(--accent);
}

.modal-header h2 {
  font-size: 14px;
  font-weight: 600;
  margin: 0;
  flex: 1;
}

.header-count {
  font-size: 11px;
  color: var(--text-tertiary);
}

.close-btn {
  background: none;
  border: none;
  color: var(--text-tertiary);
  cursor: pointer;
  padding: 4px;
  border-radius: 4px;
  display: flex;
}
.close-btn:hover {
  color: var(--text-primary);
  background: var(--hover-bg);
}

/* Error */
.error-banner {
  padding: 8px 16px;
  background: rgba(239, 68, 68, 0.15);
  border-bottom: 1px solid rgba(239, 68, 68, 0.2);
  color: var(--danger);
  font-size: 11px;
}

/* Generating */
.generating {
  padding: 32px 16px;
  text-align: center;
}

.progress-bar {
  height: 3px;
  background: var(--hover-bg);
  border-radius: 2px;
  overflow: hidden;
  margin-bottom: 12px;
}

.progress-fill.indeterminate {
  height: 100%;
  width: 40%;
  background: linear-gradient(90deg, var(--accent), var(--accent));
  border-radius: 2px;
  animation: indeterminate 1.4s ease-in-out infinite;
}

@keyframes indeterminate {
  0% {
    transform: translateX(-100%);
  }
  100% {
    transform: translateX(350%);
  }
}

.progress-text {
  font-size: 11px;
  color: var(--text-tertiary);
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 6px;
}

.spin {
  color: var(--accent);
  animation: spin 2s linear infinite;
}

@keyframes spin {
  from {
    transform: rotate(0deg);
  }
  to {
    transform: rotate(360deg);
  }
}

/* Summary bar */
.summary-bar {
  padding: 8px 16px;
  border-bottom: 1px solid var(--border);
  display: flex;
  align-items: center;
  gap: 8px;
  flex-wrap: wrap;
}

.summary-chip {
  font-size: 10px;
  font-weight: 500;
  padding: 2px 8px;
  border-radius: 10px;
  display: flex;
  align-items: center;
  gap: 4px;
}

.summary-chip.confident {
  background: rgba(34, 197, 94, 0.15);
  color: var(--success);
}

.summary-chip.review {
  background: rgba(251, 191, 36, 0.15);
  color: var(--warning);
}

.summary-chip.skipped {
  background: var(--hover-bg);
  color: var(--text-tertiary);
}

.accept-all-btn {
  margin-left: auto;
  font-size: 10px;
  padding: 2px 10px;
  background: none;
  border: 1px solid var(--border-light);
  color: var(--text-secondary);
  border-radius: 10px;
  cursor: pointer;
}
.accept-all-btn:hover {
  border-color: rgba(255, 255, 255, 0.3);
  color: var(--text-primary);
}

/* Review list */
.review-list {
  overflow-y: auto;
  flex: 1;
  padding: 4px 0;
}

.review-item {
  display: flex;
  gap: 10px;
  padding: 8px 16px;
  border-bottom: 1px solid rgba(255, 255, 255, 0.04);
  transition: background 0.15s;
}

.review-item:hover {
  background: rgba(255, 255, 255, 0.02);
}

.review-item.is-accepted {
  background: rgba(34, 197, 94, 0.04);
}

.review-item.needs-review:not(.is-accepted) {
  border-left: 2px solid var(--warning);
}

/* Thumbnail */
.review-thumb {
  flex-shrink: 0;
  width: 80px;
  height: 56px;
  border-radius: 4px;
  overflow: hidden;
  background: rgba(255, 255, 255, 0.04);
}

.review-thumb img {
  width: 100%;
  height: 100%;
  object-fit: cover;
  display: block;
}

/* Content */
.review-content {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.review-meta {
  display: flex;
  align-items: center;
  gap: 6px;
}

.conf-badge {
  font-size: 9px;
  font-weight: 600;
  font-family: monospace;
  padding: 1px 5px;
  border-radius: 3px;
}

.conf-badge.high {
  background: rgba(34, 197, 94, 0.2);
  color: var(--success);
}
.conf-badge.good {
  background: rgba(34, 197, 94, 0.12);
  color: var(--success);
}
.conf-badge.low {
  background: rgba(251, 191, 36, 0.15);
  color: var(--warning);
}
.conf-badge.poor {
  background: rgba(239, 68, 68, 0.15);
  color: var(--danger);
}

.line-num {
  font-size: 9px;
  color: var(--text-tertiary);
  font-family: monospace;
}

.alt-text {
  font-size: 11px;
  line-height: 1.4;
  color: var(--text-secondary);
  cursor: pointer;
  padding: 2px 0;
}

.alt-text:hover {
  color: var(--text-primary);
}

.review-item.needs-review:not(.is-accepted) .alt-text {
  border-bottom: 1px dashed rgba(251, 191, 36, 0.4);
}

.alt-textarea {
  font-size: 11px;
  font-family: inherit;
  line-height: 1.4;
  padding: 4px 6px;
  background: rgba(0, 0, 0, 0.3);
  color: var(--text-primary);
  border: 1px solid var(--accent);
  border-radius: 4px;
  resize: vertical;
  outline: none;
  width: 100%;
}

.alt-textarea:focus {
  border-color: var(--accent);
  box-shadow: 0 0 0 1px color-mix(in srgb, var(--accent) 35%, transparent);
}

/* Actions column */
.review-actions {
  flex-shrink: 0;
  display: flex;
  flex-direction: column;
  gap: 4px;
  align-self: center;
}

.action-btn {
  width: 26px;
  height: 26px;
  border: 1px solid var(--border-light);
  border-radius: 6px;
  background: none;
  color: var(--text-tertiary);
  cursor: pointer;
  display: flex;
  align-items: center;
  justify-content: center;
  transition: all 0.15s;
}

.action-btn:hover {
  border-color: rgba(255, 255, 255, 0.2);
  color: var(--text-secondary);
}

.action-btn.accept.active {
  background: rgba(34, 197, 94, 0.2);
  border-color: rgba(34, 197, 94, 0.4);
  color: var(--success);
}

.action-btn.edit:hover {
  border-color: color-mix(in srgb, var(--accent) 35%, transparent);
  color: var(--accent);
}

/* Empty state */
.empty-state {
  padding: 32px 24px;
  text-align: center;
  font-size: 12px;
  color: var(--text-secondary);
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 12px;
}
.empty-state .muted {
  color: var(--text-tertiary);
  font-size: 11px;
}
.empty-icon {
  color: var(--accent);
}
.empty-icon.muted-icon {
  color: var(--text-tertiary);
}
.empty-title {
  margin: 0;
  font-size: 14px;
  font-weight: 600;
  color: var(--text-primary);
}
.empty-explain {
  margin: 0;
  max-width: 380px;
  line-height: 1.5;
  color: var(--text-secondary);
}
.empty-breakdown {
  list-style: none;
  padding: 8px 12px;
  margin: 0;
  background: var(--bg-tertiary);
  border-radius: 6px;
  border: 1px solid var(--border);
  font-size: 11px;
  text-align: left;
  min-width: 280px;
}
.empty-breakdown li {
  padding: 3px 0;
  color: var(--text-secondary);
}
.empty-breakdown li strong {
  color: var(--text-primary);
  font-variant-numeric: tabular-nums;
  margin-right: 4px;
}
.empty-actions {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 6px;
  margin-top: 4px;
}
.empty-actions .btn.primary {
  display: inline-flex;
  align-items: center;
  gap: 6px;
}
.empty-hint {
  font-size: 10px;
  color: var(--text-tertiary);
}

/* Footer */
.modal-footer {
  padding: 12px 16px;
  border-top: 1px solid rgba(255, 255, 255, 0.08);
  display: flex;
  align-items: center;
  justify-content: space-between;
}

.footer-info {
  font-size: 11px;
  color: var(--text-tertiary);
}

.footer-actions {
  display: flex;
  gap: 8px;
}

/* Shared button system — primary uses the macOS accent, secondary is the
   subtle Cancel/Close style. Matches the one in LocalMediaFixer so the two
   modals feel like the same product surface. */
.btn {
  padding: 6px 14px;
  font-size: 12px;
  font-weight: 500;
  border-radius: 6px;
  cursor: pointer;
  border: none;
  transition: background 0.15s;
  font-family: inherit;
}
.btn.secondary {
  background: var(--bg-tertiary);
  color: var(--text-secondary);
}
.btn.secondary:hover {
  background: var(--hover-bg);
  color: var(--text-primary);
}
.btn.primary {
  background: var(--accent);
  color: var(--accent-contrast);
}
.btn.primary:hover {
  background: var(--accent-strong);
}
.btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}
.btn:disabled:hover {
  background: var(--accent);
}
.btn:focus-visible {
  outline: 2px solid var(--accent);
  outline-offset: 2px;
}
</style>
