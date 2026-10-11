<script setup lang="ts">
// One review sheet for "Republish all" in the Modified tab: one sentence per
// post from `publish_preview`, one button. Posts that can't be updated are
// listed with the reason and skipped.
import { computed, onUnmounted, ref, watch } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import type { MarkdownFile, PublishPreview } from '../types'
import { listJoin, networkName, plainUrl } from '../utils/publishReview'

const props = defineProps<{ show: boolean; files: MarkdownFile[] }>()
const emit = defineEmits<{ close: []; confirm: [paths: string[]] }>()

interface Row {
  file: MarkdownFile
  preview: PublishPreview | null
  error: string | null
}

const loading = ref(false)
const rows = ref<Row[]>([])
const submitting = ref(false)

async function load() {
  loading.value = true
  rows.value = []
  // One at a time: each runs git in the same repo.
  const out: Row[] = []
  for (const file of props.files) {
    try {
      const preview = await invoke<PublishPreview>('publish_preview', { sourcePath: file.path, targetId: null })
      out.push({ file, preview, error: null })
    } catch (e) {
      out.push({ file, preview: null, error: String(e) })
    }
  }
  rows.value = out
  loading.value = false
}

const ready = computed(() => rows.value.filter((r) => r.preview && !r.preview.blocked))
const skipped = computed(() => rows.value.filter((r) => !r.preview || r.preview.blocked))
/** When every post is blocked for the same reason (say, the wrong branch),
 *  say it once instead of N times. */
const sharedBlocker = computed(() => {
  if (!rows.value.length || ready.value.length) return null
  const msgs = new Set(rows.value.map((r) => r.preview?.blocked?.message ?? r.error))
  return msgs.size === 1 ? [...msgs][0] : null
})

function lineFor(r: Row): string {
  const p = r.preview!
  const title = p.title || p.slug
  return p.is_update
    ? `“${title}” replaces the live version at ${plainUrl(p.url)}.`
    : `“${title}” will be published at ${plainUrl(p.url)} (it isn’t live yet).`
}

const summary = computed(() => {
  const first = ready.value[0]?.preview
  if (!first) return []
  const n = ready.value.length
  const mins = Math.max(1, Math.round(first.deploy_estimate_secs / 60))
  const lines = [
    n === 1
      ? `It’ll be committed to ${first.repo_name} on ${first.branch} and deployed, about ${mins} minutes.`
      : `Each is its own commit to ${first.repo_name} on ${first.branch} and its own deploy. Deploys run one after another, so the last one is live in about ${n * mins} minutes.`,
    'Dispatch won’t watch these deploys or send webmentions for them; open a post and update it on its own if you want that.',
  ]
  const queued = ready.value.flatMap((r) => r.preview!.syndication.filter((s) => s.mode === 'queued'))
  lines.push(
    queued.length
      ? `Already queued and sending on its own: ${listJoin([...new Set(queued.map((s) => networkName(s.network)))])}. That can’t be undone.`
      : 'None of them will be posted to Bluesky or Mastodon.',
  )
  lines.push(`You can unpublish any of them later; that deploys again (~${mins} min).`)
  return lines
})

function submit() {
  if (submitting.value || loading.value || !ready.value.length) return
  submitting.value = true
  emit('confirm', ready.value.map((r) => r.file.path))
  emit('close')
}

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

watch(
  () => props.show,
  (open) => {
    if (open) {
      submitting.value = false
      window.addEventListener('keydown', onKey, true)
      load()
    } else {
      window.removeEventListener('keydown', onKey, true)
    }
  },
  { immediate: true },
)
onUnmounted(() => window.removeEventListener('keydown', onKey, true))
</script>

<template>
  <Transition name="pub-modal">
    <div v-if="show" class="pub-overlay" @click.self="$emit('close')">
      <div class="pub-modal" role="dialog" aria-modal="true">
        <div class="pub-modal-body">
          <template v-if="loading">
            <h2 class="pub-modal-title">Checking {{ files.length }} posts…</h2>
            <p class="pub-line pub-muted">Looking at the website repo. Nothing is written yet.</p>
          </template>

          <template v-else-if="sharedBlocker">
            <h2 class="pub-modal-title">Can’t update yet</h2>
            <p class="pub-line">{{ sharedBlocker }}</p>
            <p class="pub-line pub-muted">Nothing was written.</p>
          </template>

          <template v-else>
            <h2 class="pub-modal-title">
              Update {{ ready.length }} live post{{ ready.length === 1 ? '' : 's' }}
            </h2>
            <ul class="pub-list">
              <li v-for="r in ready" :key="r.file.path">{{ lineFor(r) }}</li>
            </ul>
            <p v-for="line in summary" :key="line" class="pub-line">{{ line }}</p>

            <template v-if="skipped.length">
              <p class="pub-line pub-muted">
                {{ skipped.length }} won’t be touched:
              </p>
              <ul class="pub-list pub-muted">
                <li v-for="r in skipped" :key="r.file.path">
                  “{{ r.preview?.title || r.file.title || r.file.filename }}”:
                  {{ r.preview?.blocked?.message ?? r.error }}
                </li>
              </ul>
            </template>
          </template>
        </div>

        <div class="pub-modal-footer">
          <button class="pub-btn-cancel" @click="$emit('close')">
            {{ ready.length ? 'Cancel' : 'Close' }}
            <kbd>esc</kbd>
          </button>
          <button v-if="!loading && ready.length" class="pub-btn-go" :disabled="submitting" @click="submit">
            Update {{ ready.length }} post{{ ready.length === 1 ? '' : 's' }}
            <kbd>⌘↩</kbd>
          </button>
        </div>
      </div>
    </div>
  </Transition>
</template>

<style scoped>
/* Same sheet as PublishConfirmModal. */
.pub-overlay {
  position: fixed;
  inset: 0;
  background: rgba(0, 0, 0, 0.42);
  backdrop-filter: blur(8px);
  -webkit-backdrop-filter: blur(8px);
  z-index: 200;
  display: flex;
  align-items: flex-start;
  justify-content: center;
  padding-top: 48px;
}

.pub-modal {
  width: 500px;
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
    0 0 0 1px rgba(255, 255, 255, 0.05);
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

.pub-list {
  margin: 0 0 10px;
  padding-left: 18px;
  font-size: 13px;
  line-height: 1.45;
  color: var(--text-primary);
}

.pub-list li {
  margin-bottom: 3px;
}

.pub-muted,
.pub-list.pub-muted {
  color: var(--text-secondary);
}

.pub-modal-footer {
  padding: 12px 20px;
  border-top: 1px solid var(--border);
  display: flex;
  justify-content: space-between;
  align-items: center;
}

.pub-btn-cancel {
  padding: 8px 14px;
  font-size: 12px;
  font-weight: 500;
  background: transparent;
  border: none;
  color: var(--text-tertiary);
  cursor: pointer;
  display: flex;
  align-items: center;
  gap: 6px;
  border-radius: 8px;
}

.pub-btn-cancel:hover {
  background: var(--bg-tertiary);
  color: var(--text-secondary);
}

kbd {
  font-size: 9px;
  padding: 1px 4px;
  background: var(--bg-tertiary);
  border: 1px solid var(--border);
  border-radius: 3px;
  font-family: 'SF Mono', monospace;
  color: var(--text-tertiary);
}

.pub-btn-go {
  padding: 8px 18px;
  font-size: 12px;
  font-weight: 600;
  background: var(--success);
  color: #000;
  border: none;
  border-radius: 8px;
  cursor: pointer;
  display: flex;
  align-items: center;
  gap: 8px;
}

.pub-btn-go kbd {
  background: rgba(0, 0, 0, 0.12);
  border-color: rgba(0, 0, 0, 0.18);
  color: rgba(0, 0, 0, 0.7);
}

.pub-btn-go:hover:not(:disabled) {
  background: color-mix(in srgb, var(--success) 88%, white);
}

.pub-btn-go:disabled {
  opacity: 0.3;
  cursor: not-allowed;
}

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
