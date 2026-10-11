<script setup lang="ts">
import { ref, computed, nextTick } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import UiButton from './ui/UiButton.vue'
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
        <UiButton variant="plain" :icon="PhX" aria-label="Close" data-tip="Close without applying" @click="$emit('close')" />
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
          <UiButton v-if="acceptedCount < suggestions.length" size="small" class="accept-all-btn" label="Accept all" @click="acceptAll" />
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
              <UiButton
                size="small"
                :icon="PhCheck"
                class="action-btn accept"
                :class="{ active: accepted.has(item.originalIdx) }"
                :data-tip="accepted.has(item.originalIdx) ? 'Undo accept' : 'Accept this description'"
                :aria-label="accepted.has(item.originalIdx) ? 'Undo accept' : 'Accept this description'"
                @click="toggleAccept(item.originalIdx)"
              />
              <UiButton
                size="small"
                :icon="PhPencilSimple"
                aria-label="Edit this description"
                data-tip="Edit this description"
                @click="startEdit(item.originalIdx)"
              />
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
          <UiButton :icon="PhCloudArrowUp" :label="`Upload ${localImageCount} to Cloudinary`" @click="emit('open-local-fixer')" />
          <span class="empty-hint">Then come back and run Describe again.</span>
        </div>
      </div>

      <!-- Footer -->
      <div class="modal-footer">
        <div class="footer-info" v-if="suggestions.length > 0 && !generating">
          {{ acceptedCount }}/{{ suggestions.length }} accepted
        </div>
        <div class="footer-actions">
          <UiButton variant="plain" label="Cancel" @click="$emit('close')" />
          <UiButton
            v-if="suggestions.length > 0"
            variant="primary"
            :disabled="acceptedCount === 0 || applying"
            :label="applying ? 'Applying…' : `Apply ${acceptedCount} to file`"
            @click="applyAccepted"
          />
        </div>
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
  width: 580px;
  max-width: 95vw;
  max-height: 85vh;
  background: var(--modal-bg);
  backdrop-filter: blur(24px) saturate(180%);
  -webkit-backdrop-filter: blur(24px) saturate(180%);
  border: 1px solid var(--border-light);
  border-radius: 12px;
  display: flex;
  flex-direction: column;
  overflow: hidden;
  box-shadow: var(--shadow-lg);
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
/* Error */
.error-banner {
  padding: 8px 16px;
  background: var(--tint-broken);
  border-bottom: 1px solid color-mix(in srgb, var(--status-broken) 25%, transparent);
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
  background: var(--tint-live);
  color: var(--success);
}

.summary-chip.review {
  background: var(--tint-attention);
  color: var(--warning);
}

.summary-chip.skipped {
  background: var(--hover-bg);
  color: var(--text-tertiary);
}

.accept-all-btn {
  margin-left: auto;
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
  border-bottom: 1px solid var(--separator);
  transition: background 0.15s;
}

.review-item:hover {
  background: var(--fill);
}

.review-item.is-accepted {
  background: color-mix(in srgb, var(--status-live) 5%, transparent);
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
  background: var(--fill);
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

/* Tint carries the verdict; the text stays neutral for contrast. */
.conf-badge.high,
.conf-badge.good,
.conf-badge.low,
.conf-badge.poor {
  color: var(--label);
}
.conf-badge.high {
  background: color-mix(in srgb, var(--status-live) 24%, transparent);
}
.conf-badge.good {
  background: var(--tint-live);
}
.conf-badge.low {
  background: var(--tint-attention);
}
.conf-badge.poor {
  background: var(--tint-broken);
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
  border-bottom: 1px dashed color-mix(in srgb, var(--status-attention) 50%, transparent);
}

.alt-textarea {
  font-size: 11px;
  font-family: inherit;
  line-height: 1.4;
  padding: 4px 6px;
  background: var(--control-bg);
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

/* An accepted description keeps its check lit in the live green. */
.action-btn.accept.active {
  background: var(--tint-live);
  border-color: color-mix(in srgb, var(--status-live) 40%, transparent);
  color: var(--success);
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
.empty-hint {
  font-size: 10px;
  color: var(--text-tertiary);
}

/* Footer */
.modal-footer {
  padding: var(--space-3) var(--gutter);
  border-top: 1px solid var(--separator);
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
  gap: var(--space-2);
}

</style>
