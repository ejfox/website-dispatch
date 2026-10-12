<script setup lang="ts">
// The publish review sheet. It states what publishing will actually do,
// composed from `publish_preview` (the same checks publish runs, nothing
// written), and offers one verb button. Publishing is reversible, so the
// sentence plus the button is the confirmation; the irreversible extra
// (webmentions) gets its own visible toggle.
import { computed, nextTick, onMounted, onUnmounted, ref, watch } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import type { MarkdownFile, PublishPreview } from '../types'
import { composeReview, type ReviewMode } from '../utils/publishReview'
import UiButton from './ui/UiButton.vue'

const props = withDefaults(
  defineProps<{
    show: boolean
    file: MarkdownFile
    targetId?: string | null
    mode?: ReviewMode
  }>(),
  { targetId: null, mode: 'publish' },
)

const emit = defineEmits<{
  close: []
  confirm: [opts: { isUpdate: boolean; webmentions: boolean }]
}>()

const loading = ref(false)
const loadError = ref<string | null>(null)
const preview = ref<PublishPreview | null>(null)
const sendWebmentions = ref(true)
const submitting = ref(false)
const primaryBtn = ref<InstanceType<typeof UiButton> | null>(null)

const review = computed(() =>
  preview.value
    ? composeReview(preview.value, {
        mode: props.mode,
        webmentions: sendWebmentions.value,
        warnings: props.file.warnings,
      })
    : null,
)
const canSubmit = computed(() => !!review.value && !review.value.blocker && !submitting.value)

// Re-read on every open: the branch or the repo copy may have changed.
async function load() {
  loading.value = true
  loadError.value = null
  preview.value = null
  try {
    preview.value = await invoke<PublishPreview>('publish_preview', {
      sourcePath: props.file.path,
      targetId: props.targetId || null,
    })
  } catch (e) {
    loadError.value = String(e)
  }
  loading.value = false
  await nextTick()
  primaryBtn.value?.focus()
}

function submit() {
  if (!props.show || !canSubmit.value || !preview.value) return
  submitting.value = true
  emit('confirm', { isUpdate: preview.value.is_update, webmentions: sendWebmentions.value })
  emit('close')
}

// Keys: Esc cancels, ⌘↩ runs the primary action. Capture phase so the
// global ⌘↩ (open this sheet) and Esc (deselect) handlers never also run.
function onKey(e: KeyboardEvent) {
  if (e.key === 'Escape') {
    e.preventDefault()
    e.stopPropagation()
    emit('close')
  } else if (e.key === 'Enter' && (e.metaKey || e.ctrlKey)) {
    e.preventDefault()
    e.stopPropagation()
    submit()
  }
}

// The native Publish menu owns ⌘↩ as its key equivalent, so the keystroke
// may arrive as a menu event instead of a keydown. One press can produce
// both (the keydown opened this sheet, then the menu event lands), so a
// menu event right after opening is ignored: opening never also publishes.
let unlistenMenu: UnlistenFn | null = null
let openedAt = 0
function onMenuPublish() {
  if (!props.show || Date.now() - openedAt < 700) return
  submit()
}
// Registered once for the component's life (not per open), so a quick
// open/close can't leave a stray listener behind.
onMounted(async () => {
  unlistenMenu = await listen('menu-publish', onMenuPublish).catch(() => null)
})

watch(
  () => props.show,
  (open) => {
    if (open) {
      openedAt = Date.now()
      submitting.value = false
      sendWebmentions.value = true
      window.addEventListener('keydown', onKey, true)
      load()
    } else {
      window.removeEventListener('keydown', onKey, true)
    }
  },
  { immediate: true },
)
onUnmounted(() => {
  window.removeEventListener('keydown', onKey, true)
  unlistenMenu?.()
})
</script>

<template>
  <Transition name="pub-modal">
    <div v-if="show" class="pub-overlay" @click.self="$emit('close')">
      <div class="pub-modal" role="dialog" aria-modal="true" :aria-label="review?.headline ?? 'Publish'">
        <div class="pub-modal-body">
          <template v-if="loading">
            <h2 class="pub-modal-title">Checking {{ file.title || file.filename }}…</h2>
            <p class="pub-line pub-muted">Looking at the website repo. Nothing is written yet.</p>
          </template>

          <template v-else-if="loadError">
            <h2 class="pub-modal-title">Can’t check this post</h2>
            <p class="pub-line">{{ loadError }}</p>
          </template>

          <template v-else-if="review && preview">
            <h2 class="pub-modal-title">{{ review.headline }}</h2>

            <template v-if="review.blocker">
              <p class="pub-line pub-blocker">{{ review.blocker }}</p>
              <ul v-if="review.blockerList.length" class="pub-issues">
                <li v-for="issue in review.blockerList" :key="issue">{{ issue }}</li>
              </ul>
              <p class="pub-line pub-muted">Nothing was written.</p>
              <p class="pub-git">{{ review.git }}</p>
            </template>

            <template v-else>
              <p v-for="line in review.summary" :key="line" class="pub-line">{{ line }}</p>
              <p v-if="review.visibility" class="pub-line">{{ review.visibility }}</p>
              <p class="pub-line">{{ review.syndication }}</p>

              <p v-if="!review.webmentionsApply" class="pub-line">{{ review.webmentions }}</p>
              <label v-else class="pub-toggle" :class="{ on: sendWebmentions }">
                <input type="checkbox" v-model="sendWebmentions" />
                <span class="pub-toggle-text">
                  <span class="pub-toggle-label">Send webmentions once it’s live</span>
                  <span class="pub-toggle-detail">{{ review.webmentions }}</span>
                </span>
              </label>

              <div v-if="review.notes.length" class="pub-notes">
                <p class="pub-line pub-muted">Still in the text (updates go through anyway):</p>
                <ul class="pub-issues">
                  <li v-for="note in review.notes" :key="note">{{ note }}</li>
                </ul>
              </div>

              <p class="pub-line pub-muted">{{ review.undo }}</p>
              <p class="pub-git">{{ review.git }}</p>
            </template>
          </template>
        </div>

        <div class="pub-modal-footer">
          <UiButton variant="plain" @click="$emit('close')">
            {{ review?.blocker || loadError ? 'Close' : 'Cancel' }}
            <kbd>esc</kbd>
          </UiButton>
          <UiButton
            v-if="review && !review.blocker"
            ref="primaryBtn"
            variant="primary"
            :disabled="!canSubmit"
            @click="submit"
          >
            {{ review.primaryLabel }}
            <kbd>⌘↩</kbd>
          </UiButton>
        </div>
      </div>
    </div>
  </Transition>
</template>

<style scoped>
/* Top-anchored sheet, close to the titlebar like a real Mac sheet. */
.pub-overlay {
  position: fixed;
  inset: 0;
  background: var(--scrim);
  backdrop-filter: blur(8px);
  -webkit-backdrop-filter: blur(8px);
  z-index: 200;
  display: flex;
  align-items: flex-start;
  justify-content: center;
  padding-top: 48px;
}

.pub-modal {
  width: 460px;
  max-width: 90vw;
  max-height: calc(100vh - 96px);
  display: flex;
  flex-direction: column;
  background: var(--modal-bg);
  backdrop-filter: blur(24px) saturate(180%);
  -webkit-backdrop-filter: blur(24px) saturate(180%);
  border: 1px solid var(--border-light);
  border-radius: 16px;
  box-shadow:
    var(--shadow-lg),
    0 0 0 1px var(--separator);
  overflow: hidden;
  font-family: -apple-system, BlinkMacSystemFont, sans-serif;
}

.pub-modal-body {
  padding: 20px 24px 16px;
  overflow-y: auto;
}

.pub-modal-title {
  margin: 0 0 10px;
  font-size: 15px;
  font-weight: 600;
  color: var(--text-primary);
}

.pub-line {
  margin: 0 0 8px;
  font-size: 13px;
  line-height: 1.45;
  color: var(--text-primary);
}

.pub-muted {
  color: var(--text-secondary);
}

.pub-blocker {
  color: var(--text-primary);
  font-weight: 500;
}

.pub-issues {
  margin: 0 0 10px;
  padding-left: 18px;
  font-size: 13px;
  line-height: 1.45;
  color: var(--text-primary);
}

.pub-issues li {
  margin-bottom: 2px;
}

.pub-notes {
  margin-top: 4px;
}

/* Webmentions toggle: a native-looking checkbox with its consequence. */
.pub-toggle {
  display: flex;
  align-items: flex-start;
  gap: 8px;
  margin: 4px 0 12px;
  padding: 8px 10px;
  border: 1px solid var(--border);
  border-radius: 8px;
  cursor: pointer;
}

.pub-toggle input {
  margin: 2px 0 0;
  accent-color: var(--accent);
  flex-shrink: 0;
}

.pub-toggle-text {
  display: flex;
  flex-direction: column;
  gap: 2px;
}

.pub-toggle-label {
  font-size: 13px;
  font-weight: 500;
  color: var(--text-primary);
}

.pub-toggle-detail {
  font-size: 12px;
  line-height: 1.4;
  color: var(--text-secondary);
}

.pub-git {
  margin: 10px 0 0;
  font-size: 11px;
  font-family: 'SF Mono', monospace;
  color: var(--text-tertiary);
  word-break: break-all;
}

/* Footer */
.pub-modal-footer {
  padding: var(--space-3) 20px;
  border-top: 1px solid var(--border);
  display: flex;
  justify-content: space-between;
  align-items: center;
}

/* Sheet-style transition: scrim fades, the sheet slides down from above the
 * titlebar with a soft settle. */
.pub-modal-enter-active,
.pub-modal-leave-active {
  transition: opacity 0.18s ease-out;
}
.pub-modal-enter-from,
.pub-modal-leave-to {
  opacity: 0;
}
.pub-modal-enter-active .pub-modal,
.pub-modal-leave-active .pub-modal {
  transition:
    transform 0.32s cubic-bezier(0.22, 1, 0.36, 1),
    opacity 0.18s ease-out;
}
.pub-modal-enter-from .pub-modal,
.pub-modal-leave-to .pub-modal {
  transform: translateY(calc(-100% - 64px));
  opacity: 0;
}
</style>
