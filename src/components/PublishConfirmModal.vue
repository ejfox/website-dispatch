<script setup lang="ts">
import { ref, watch } from 'vue'
import { PhArrowsClockwise, PhArrowSquareUpRight, PhKeyboard, PhCheck } from '@phosphor-icons/vue'
import type { MarkdownFile } from '../types'

const props = defineProps<{
  show: boolean
  file: MarkdownFile
  slug: string
  targetUrl: string
  publishContext: string | null
  isRepublish: boolean
}>()

const emit = defineEmits<{
  close: []
  confirm: [isRepublish: boolean]
}>()

const publishConfirmStep = ref(1)
const publishConfirmChecked = ref(false)
const publishConfirmText = ref('')

// Reset internal state when modal opens
watch(
  () => props.show,
  (val) => {
    if (val) {
      publishConfirmStep.value = 1
      publishConfirmChecked.value = false
      publishConfirmText.value = ''
    }
  },
)

function handleConfirm() {
  emit('confirm', props.isRepublish)
  emit('close')
}
</script>

<template>
  <Transition name="pub-modal">
    <div v-if="show" class="pub-overlay" @click.self="$emit('close')" @keydown.escape="$emit('close')" tabindex="-1">
      <div class="pub-modal">
        <div class="pub-modal-step-indicator">
          <div class="pub-step" :class="{ active: publishConfirmStep === 1, done: publishConfirmStep === 2 }">
            <span class="pub-step-num">1</span>
            <span class="pub-step-label">Review</span>
          </div>
          <div class="pub-step-line" :class="{ active: publishConfirmStep === 2 }"></div>
          <div class="pub-step" :class="{ active: publishConfirmStep === 2 }">
            <span class="pub-step-num">2</span>
            <span class="pub-step-label">Confirm</span>
          </div>
        </div>

        <div v-if="publishConfirmStep === 1" class="pub-modal-body">
          <div class="pub-modal-icon">
            <PhArrowsClockwise v-if="isRepublish" :size="28" weight="light" />
            <PhArrowSquareUpRight v-else :size="28" weight="light" />
          </div>
          <h2 class="pub-modal-title">{{ isRepublish ? 'Republish' : 'Ready to publish?' }}</h2>
          <p class="pub-modal-subtitle">This will push to your live website.</p>
          <p v-if="publishContext" class="pub-modal-context">{{ publishContext }}</p>

          <div class="pub-detail-card">
            <div class="pub-detail-row">
              <span class="pub-detail-label">File</span>
              <code class="pub-detail-value">{{ file.filename }}</code>
            </div>
            <div class="pub-detail-divider"></div>
            <div class="pub-detail-row">
              <span class="pub-detail-label">URL</span>
              <code class="pub-detail-value pub-detail-url">{{ targetUrl }}</code>
            </div>
            <div class="pub-detail-divider"></div>
            <div class="pub-detail-row">
              <span class="pub-detail-label">Words</span>
              <code class="pub-detail-value">{{ file.word_count.toLocaleString() }}</code>
            </div>
            <div v-if="file.content_type === 'weeknote'" class="pub-detail-divider"></div>
            <div v-if="file.content_type === 'weeknote'" class="pub-detail-row">
              <span class="pub-detail-label">Type</span>
              <span class="pub-detail-value pub-weeknote-badge">Week Note</span>
            </div>
          </div>

          <label class="pub-checkbox" :class="{ checked: publishConfirmChecked }">
            <input type="checkbox" v-model="publishConfirmChecked" />
            <span class="pub-checkbox-box">
              <PhCheck v-if="publishConfirmChecked" :size="13" weight="bold" />
            </span>
            <span>I reviewed links, media, and metadata.</span>
          </label>
        </div>

        <div v-else class="pub-modal-body">
          <div class="pub-modal-icon">
            <PhKeyboard :size="28" weight="light" />
          </div>
          <h2 class="pub-modal-title">Type slug to confirm</h2>
          <p class="pub-modal-subtitle">
            Type
            <code class="pub-slug-hint">{{ slug }}</code>
            to publish.
          </p>
          <input
            class="pub-slug-input"
            v-model="publishConfirmText"
            :placeholder="slug"
            autofocus
            @keydown.enter="publishConfirmText.trim() === slug && handleConfirm()"
          />
          <div class="pub-slug-match" v-if="publishConfirmText.length > 0">
            <span v-if="publishConfirmText.trim() === slug" class="pub-match-yes">Match!</span>
            <span v-else class="pub-match-no">{{ publishConfirmText.length }}/{{ slug.length }}</span>
          </div>
        </div>

        <div class="pub-modal-footer">
          <button class="pub-btn-cancel" @click="$emit('close')">
            Cancel
            <kbd>esc</kbd>
          </button>
          <button
            v-if="publishConfirmStep === 1"
            class="pub-btn-go"
            :disabled="!publishConfirmChecked"
            @click="publishConfirmStep = 2"
          >
            Continue
          </button>
          <button
            v-else
            class="pub-btn-go pub-btn-publish"
            :disabled="publishConfirmText.trim() !== slug"
            @click="handleConfirm()"
          >
            {{ isRepublish ? 'Republish' : 'Publish' }}
          </button>
        </div>
      </div>
    </div>
  </Transition>
</template>

<style scoped>
.pub-overlay {
  position: fixed;
  inset: 0;
  background: rgba(0, 0, 0, 0.5);
  backdrop-filter: blur(8px);
  -webkit-backdrop-filter: blur(8px);
  z-index: 200;
  display: flex;
  align-items: center;
  justify-content: center;
}

.pub-modal {
  width: 420px;
  max-width: 90vw;
  background: var(--modal-bg);
  backdrop-filter: blur(24px) saturate(180%);
  -webkit-backdrop-filter: blur(24px) saturate(180%);
  border: 1px solid var(--border-light);
  border-radius: 16px;
  box-shadow:
    var(--shadow-lg),
    0 0 0 1px rgba(255, 255, 255, 0.05);
  overflow: hidden;
}

/* Step indicator */
.pub-modal-step-indicator {
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 0;
  padding: 16px 24px 0;
}

.pub-step {
  display: flex;
  align-items: center;
  gap: 6px;
  opacity: 0.35;
  transition: all 0.3s ease;
}

.pub-step.active,
.pub-step.done {
  opacity: 1;
}

.pub-step-num {
  width: 20px;
  height: 20px;
  border-radius: 50%;
  display: flex;
  align-items: center;
  justify-content: center;
  font-size: 10px;
  font-weight: 700;
  background: var(--bg-tertiary);
  color: var(--text-tertiary);
  transition: all 0.3s ease;
}

.pub-step.active .pub-step-num {
  background: var(--success);
  color: #000;
}

.pub-step.done .pub-step-num {
  background: var(--success);
  color: #000;
}

.pub-step-label {
  font-size: 10px;
  font-weight: 500;
  text-transform: uppercase;
  letter-spacing: 0.5px;
  color: var(--text-tertiary);
}

.pub-step.active .pub-step-label {
  color: var(--text-primary);
}

.pub-step-line {
  width: 40px;
  height: 2px;
  background: var(--border);
  margin: 0 10px;
  border-radius: 1px;
  transition: background 0.3s ease;
}

.pub-step-line.active {
  background: var(--success);
}

/* Modal body */
.pub-modal-body {
  padding: 20px 24px;
  display: flex;
  flex-direction: column;
  align-items: center;
  text-align: center;
}

.pub-modal-icon {
  margin-bottom: 8px;
  color: var(--success);
  width: 44px;
  height: 44px;
  border-radius: 12px;
  background: color-mix(in srgb, var(--success) 12%, transparent);
  display: flex;
  align-items: center;
  justify-content: center;
}

.pub-modal-title {
  margin: 0;
  font-size: 16px;
  font-weight: 600;
  color: var(--text-primary);
}

.pub-modal-subtitle {
  margin: 4px 0 16px;
  font-size: 12px;
  color: var(--text-tertiary);
}

.pub-modal-context {
  margin: -10px 0 14px;
  font-size: 10px;
  font-family: 'SF Mono', monospace;
  color: var(--success);
  opacity: 0.8;
  letter-spacing: 0.01em;
}

/* Detail card */
.pub-detail-card {
  width: 100%;
  background: var(--bg-tertiary);
  border: 1px solid var(--border);
  border-radius: 10px;
  padding: 2px 0;
  margin-bottom: 16px;
}

.pub-detail-row {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: 8px 14px;
}

.pub-detail-label {
  font-size: 10px;
  font-weight: 600;
  text-transform: uppercase;
  letter-spacing: 0.5px;
  color: var(--text-tertiary);
  flex-shrink: 0;
}

.pub-detail-value {
  font-size: 11px;
  font-family: 'SF Mono', monospace;
  color: var(--text-primary);
  text-align: right;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  max-width: 260px;
}

.pub-detail-url {
  color: var(--success);
}

.pub-detail-divider {
  height: 1px;
  background: var(--border);
  margin: 0 14px;
}

.pub-weeknote-badge {
  font-family: -apple-system, BlinkMacSystemFont, sans-serif;
  font-size: 10px;
  font-weight: 600;
  color: #f59e0b;
  background: rgba(245, 158, 11, 0.15);
  padding: 2px 8px;
  border-radius: 4px;
}

/* Custom checkbox */
.pub-checkbox {
  display: flex;
  align-items: center;
  gap: 10px;
  font-size: 12px;
  color: var(--text-secondary);
  cursor: pointer;
  padding: 10px 14px;
  border-radius: 8px;
  background: transparent;
  border: 1px solid var(--border);
  width: 100%;
  transition: all 0.2s ease;
}

.pub-checkbox:hover {
  background: var(--bg-tertiary);
}

.pub-checkbox.checked {
  border-color: var(--success);
  background: color-mix(in srgb, var(--success) 8%, transparent);
}

.pub-checkbox input {
  display: none;
}

.pub-checkbox-box {
  width: 18px;
  height: 18px;
  border-radius: 4px;
  border: 2px solid var(--border-light);
  display: flex;
  align-items: center;
  justify-content: center;
  flex-shrink: 0;
  transition: all 0.2s ease;
  color: #fff;
}

.pub-checkbox.checked .pub-checkbox-box {
  background: var(--success);
  border-color: var(--success);
}

/* Slug confirmation input */
.pub-slug-hint {
  font-family: 'SF Mono', monospace;
  font-size: 11px;
  background: var(--bg-tertiary);
  padding: 2px 6px;
  border-radius: 4px;
  color: var(--success);
}

.pub-slug-input {
  width: 100%;
  padding: 12px 14px;
  font-size: 14px;
  font-family: 'SF Mono', monospace;
  background: var(--bg-tertiary);
  border: 2px solid var(--border);
  border-radius: 10px;
  color: var(--text-primary);
  outline: none;
  text-align: center;
  transition: border-color 0.2s ease;
}

.pub-slug-input:focus {
  border-color: var(--success);
}

.pub-slug-match {
  margin-top: 6px;
  font-size: 11px;
  font-weight: 500;
}

.pub-match-yes {
  color: var(--success);
}

.pub-match-no {
  color: var(--text-tertiary);
  font-family: 'SF Mono', monospace;
}

/* Footer */
.pub-modal-footer {
  padding: 12px 20px;
  border-top: 1px solid var(--border);
  display: flex;
  justify-content: space-between;
  align-items: center;
}

.pub-btn-cancel {
  padding: 8px 14px;
  font-size: 11px;
  font-weight: 500;
  background: transparent;
  border: none;
  color: var(--text-tertiary);
  cursor: pointer;
  display: flex;
  align-items: center;
  gap: 6px;
  border-radius: 8px;
  transition: all 0.15s ease;
}

.pub-btn-cancel:hover {
  background: var(--bg-tertiary);
  color: var(--text-secondary);
}

.pub-btn-cancel kbd {
  font-size: 9px;
  padding: 1px 4px;
  background: var(--bg-tertiary);
  border: 1px solid var(--border);
  border-radius: 3px;
  font-family: 'SF Mono', monospace;
  color: var(--text-tertiary);
}

.pub-btn-go {
  padding: 8px 20px;
  font-size: 12px;
  font-weight: 600;
  background: var(--success);
  color: #000;
  border: none;
  border-radius: 8px;
  cursor: pointer;
  transition: all 0.2s cubic-bezier(0.34, 1.56, 0.64, 1);
}

.pub-btn-go:hover:not(:disabled) {
  transform: translateY(-1px);
  box-shadow: 0 4px 12px color-mix(in srgb, var(--success) 40%, transparent);
}

.pub-btn-go:active:not(:disabled) {
  transform: translateY(0);
}

.pub-btn-go:disabled {
  opacity: 0.3;
  cursor: not-allowed;
}

.pub-btn-publish {
  padding: 8px 24px;
}

/* Modal transition */
.pub-modal-enter-active,
.pub-modal-leave-active {
  transition: all 0.25s cubic-bezier(0.16, 1, 0.3, 1);
}

.pub-modal-enter-active .pub-modal,
.pub-modal-leave-active .pub-modal {
  transition: all 0.25s cubic-bezier(0.16, 1, 0.3, 1);
}

.pub-modal-leave-active {
  transition: all 0.15s ease;
}

.pub-modal-leave-active .pub-modal {
  transition: all 0.15s ease;
}

.pub-modal-enter-from,
.pub-modal-leave-to {
  opacity: 0;
}

.pub-modal-enter-from .pub-modal {
  transform: scale(0.95) translateY(8px);
  opacity: 0;
}

.pub-modal-leave-to .pub-modal {
  transform: scale(0.98) translateY(4px);
  opacity: 0;
}
</style>
