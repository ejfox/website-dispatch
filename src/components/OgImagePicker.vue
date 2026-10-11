<script setup lang="ts">
import { ref, computed, watch, onMounted, onUnmounted } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { useToasts } from '../composables/useUiState'

const toasts = useToasts()
import { convertFileSrc } from '@tauri-apps/api/core'
import { Menu, MenuItem, PredefinedMenuItem } from '@tauri-apps/api/menu'
import { PhArrowsClockwise, PhCheck, PhUpload, PhSparkle, PhCaretDown } from '@phosphor-icons/vue'
import { useLocalStorage } from '@vueuse/core'

const collapsed = useLocalStorage('dispatch-og-collapsed', true)

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
// Every render overwrites the same variant-N.png, so stamp the URL or the
// webview serves the previous batch from cache (the asset protocol reads only
// the path and ignores the query).
const renderStamp = ref(0)
const variantUrls = computed(() =>
  (variants.value?.paths || []).map((p) => `${convertFileSrc(p)}?r=${renderStamp.value}`),
)

// The Rust side kills a wedged render at 90s; this is the backstop in case
// the IPC reply itself never arrives. Either way the picker ends in an error
// with a Retry, never an endless spinner.
const GENERATE_TIMEOUT_MS = 100_000

// Each request gets a ticket; a reply for a superseded request (slug changed,
// or a newer reroll) is dropped instead of clobbering the current state.
let ticket = 0

async function generate() {
  const mine = ++ticket
  const slug = props.slug
  generating.value = true
  error.value = null
  selectedIdx.value = null
  uploadedUrl.value = null
  let timer: ReturnType<typeof setTimeout> | undefined
  try {
    const timeout = new Promise<never>((_, reject) => {
      timer = setTimeout(
        () => reject(new Error(`Rendering took longer than ${GENERATE_TIMEOUT_MS / 1000}s and was abandoned.`)),
        GENERATE_TIMEOUT_MS,
      )
    })
    const result = await Promise.race([
      invoke<OgImageVariants>('generate_og_variants', { slug, batch: batch.value }),
      timeout,
    ])
    if (mine !== ticket) return
    brokenImages.value = 0
    renderStamp.value = Date.now()
    variants.value = result
  } catch (e) {
    if (mine !== ticket) return
    const msg = e instanceof Error ? e.message : String(e)
    error.value = msg
    toasts.error('Social card generation failed', msg)
  } finally {
    clearTimeout(timer)
    if (mine === ticket) generating.value = false
  }
}

// A rendered PNG the webview can't load (asset-protocol scope, file gone)
// would otherwise show as an empty box forever. Count them; all four broken
// is an error with a Retry.
const brokenImages = ref(0)
function onImageError() {
  brokenImages.value++
  if (variants.value && brokenImages.value >= variants.value.paths.length) {
    error.value = 'The social cards rendered but could not be displayed.'
  }
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
    toasts.error('OG image upload failed', String(e))
  }
  uploading.value = false
}

async function showVariantMenu(i: number, e: MouseEvent) {
  e.preventDefault()
  if (!variants.value) return
  const path = variants.value.paths[i]
  const previewHtml = variants.value.preview_html
  const menu = await Menu.new({
    items: [
      await MenuItem.new({
        text: 'Open at Full Size',
        action: () => invoke('open_in_app', { path, app: 'Preview' }).catch(() => {}),
      }),
      await MenuItem.new({
        text: 'Open Composer (all 4)',
        action: () => invoke('open_in_app', { path: previewHtml, app: 'Safari' }).catch(() => {}),
      }),
      await MenuItem.new({
        text: 'Reveal in Finder',
        action: () => invoke('open_in_app', { path, app: 'Finder' }).catch(() => {}),
      }),
      await PredefinedMenuItem.new({ item: 'Separator' }),
      await MenuItem.new({
        text: 'Use This One',
        enabled: !uploading.value,
        action: () => {
          selectedIdx.value = i
          uploadSelected()
        },
      }),
      await MenuItem.new({ text: 'Reroll All', action: () => reroll() }),
      await PredefinedMenuItem.new({ item: 'Separator' }),
      await MenuItem.new({ text: 'Copy File Path', action: () => navigator.clipboard.writeText(path) }),
    ],
  })
  await menu.popup()
}

// Reset when the slug changes — Vue keeps the picker instance alive across post
// selections so we can't rely on mount-time effects. `immediate: true` covers
// the first render too.
//
// LAZY generation: the picker is collapsed by default, and `generate()` fires a
// heavy Rust image-render + IPC round-trip. Generating on every file click for
// a panel nobody opened was the single biggest remaining source of click lag —
// so only (re)generate when the picker is actually expanded.
watch(
  () => props.slug,
  () => {
    variants.value = null
    selectedIdx.value = null
    uploadedUrl.value = null
    error.value = null
    batch.value = 0
    brokenImages.value = 0
    ticket++ // drop any in-flight reply for the previous post
    generating.value = false
    if (!collapsed.value) generate()
  },
  { immediate: true },
)

// First expand for the current slug kicks off generation.
watch(collapsed, (isCollapsed) => {
  if (!isCollapsed && !variants.value && !generating.value) generate()
})

// Publish › Generate Social Card (App.vue) asks for a batch: open the picker,
// bring it into view, and roll one (the expand watcher above generates the
// first batch; an already-open picker rerolls).
const root = ref<HTMLElement | null>(null)
function onGenerateRequest(e: Event) {
  const detail = (e as CustomEvent<{ handled: boolean }>).detail
  if (detail) detail.handled = true
  if (collapsed.value) collapsed.value = false
  else if (!generating.value) reroll()
  root.value?.scrollIntoView({ behavior: 'smooth', block: 'nearest' })
}
onMounted(() => window.addEventListener('dispatch:generate-og', onGenerateRequest))
onUnmounted(() => window.removeEventListener('dispatch:generate-og', onGenerateRequest))
</script>

<template>
  <div ref="root" class="og-picker">
    <!-- Header -->
    <div class="picker-header">
      <button class="header-toggle" @click="collapsed = !collapsed" :aria-expanded="!collapsed">
        <PhCaretDown :size="9" weight="bold" class="caret" :class="{ collapsed }" />
        <PhSparkle :size="14" weight="fill" class="header-icon" />
        <span class="header-title">Social card (OG image)</span>
        <span v-if="variants && batch > 0" class="header-batch" title="How many times you've rerolled this post's social cards">reroll {{ batch }}</span>
      </button>
      <button v-if="!collapsed" class="reroll-btn" @click.stop="reroll" :disabled="generating" title="Generate 4 new variants" aria-label="Generate 4 new social card variants">
        <PhArrowsClockwise :size="12" weight="bold" :class="{ spinning: generating }" />
        {{ generating ? '' : 'Reroll' }}
      </button>
    </div>

    <template v-if="!collapsed">
    <!-- Error -->
    <div v-if="error" class="picker-error" role="alert">
      <span>{{ error }}</span>
      <button class="retry-btn" :disabled="generating" @click="generate">Retry</button>
    </div>

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
        title="Click to select · double-click to open at full size · right-click for more"
        @click="select(i)"
        @dblclick="invoke('open_in_app', { path: variants!.paths[i], app: 'Preview' }).catch(() => {})"
        @contextmenu="showVariantMenu(i, $event)"
      >
        <img :src="url" :alt="`OG variant ${i}`" loading="eager" @error="onImageError" />
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
    </template>
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

.header-toggle {
  display: flex;
  align-items: center;
  gap: 8px;
  flex: 1;
  background: none;
  border: none;
  padding: 0;
  cursor: pointer;
  color: inherit;
  font: inherit;
  text-align: left;
}

.caret {
  transition: transform 0.15s;
  color: var(--text-tertiary);
}
.caret.collapsed {
  transform: rotate(-90deg);
}

.header-icon {
  color: var(--accent);
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
  background: var(--fill);
  color: var(--text-secondary);
  border: 1px solid var(--separator);
  border-radius: 4px;
  cursor: pointer;
}

.reroll-btn:hover {
  border-color: var(--label-3);
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
  color: var(--accent);
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
  display: flex;
  align-items: flex-start;
  gap: 8px;
  font-size: 10px;
  color: var(--warning);
  margin-bottom: 6px;
  overflow-wrap: anywhere;
}
.retry-btn {
  flex-shrink: 0;
  padding: 2px 8px;
  font-size: 10px;
  font-family: inherit;
  background: var(--fill);
  color: var(--text-secondary);
  border: 1px solid var(--separator);
  border-radius: 4px;
  cursor: pointer;
}
.retry-btn:disabled {
  opacity: 0.4;
  cursor: wait;
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
  border-color: var(--label-3);
}
/* Selection is accent (was red, which reads as an error). */
.variant-card.selected {
  border-color: var(--accent);
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
  background: var(--under-page);
}

.variant-label {
  position: absolute;
  top: 3px;
  left: 3px;
  font-size: 8px;
  font-family: monospace;
  /* Sits on the OG image: fixed light-on-dark in both appearances. */
  background: rgba(0, 0, 0, 0.6);
  color: rgba(255, 255, 255, 0.85);
  padding: 1px 4px;
  border-radius: 2px;
}

.variant-check {
  position: absolute;
  top: 50%;
  left: 50%;
  transform: translate(-50%, -50%);
  color: var(--on-accent);
  background: var(--accent);
  border-radius: 50%;
  width: 28px;
  height: 28px;
  display: flex;
  align-items: center;
  justify-content: center;
}

.picker-success {
  font-size: 10px;
  color: var(--label-2);
  display: flex;
  align-items: center;
  gap: 4px;
  padding: 4px 0;
}

.picker-success svg {
  color: var(--status-live);
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
  background: var(--accent);
  color: var(--on-accent);
  border: none;
  border-radius: 4px;
  cursor: pointer;
}

.upload-btn:hover {
  background: var(--accent-strong);
}
.upload-btn:disabled {
  opacity: 0.4;
  cursor: not-allowed;
}
</style>
