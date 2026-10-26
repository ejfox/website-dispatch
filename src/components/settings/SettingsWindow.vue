<script setup lang="ts">
/**
 * Root of the Settings window (a separate Tauri window, label `settings`,
 * loaded as index.html?window=settings — see main.ts and settings.rs).
 *
 * Native Mac Settings shape: the pane name as the window title, a toolbar of
 * pane icons under it, a right-aligned form, no Save/Cancel (each change is
 * saved as it's made), and a fixed width with the height fitted to the pane.
 */
import { computed, nextTick, onMounted, onUnmounted, ref, watch } from 'vue'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { getCurrentWindow, LogicalSize } from '@tauri-apps/api/window'
import { useLocalStorage, useResizeObserver } from '@vueuse/core'
import { PhGearSix, PhPaperPlaneTilt, PhImage, PhShareNetwork, PhSliders } from '@phosphor-icons/vue'
import GeneralPane from './GeneralPane.vue'
import PublishingPane from './PublishingPane.vue'
import MediaPane from './MediaPane.vue'
import ConnectionsPane from './ConnectionsPane.vue'
import AdvancedPane from './AdvancedPane.vue'
import { useSettings } from '../../composables/useSettings'
import type { SettingsPane } from '../../types'
import './settings.css'

const PANES = [
  { id: 'general', label: 'General', icon: PhGearSix, component: GeneralPane },
  { id: 'publishing', label: 'Publishing', icon: PhPaperPlaneTilt, component: PublishingPane },
  { id: 'media', label: 'Media', icon: PhImage, component: MediaPane },
  { id: 'connections', label: 'Connections', icon: PhShareNetwork, component: ConnectionsPane },
  { id: 'advanced', label: 'Advanced', icon: PhSliders, component: AdvancedPane },
] as const

const WIDTH = 640

const { config, loadError, saveError, load } = useSettings()

// The window reopens on the last pane, like Mac Settings windows do, unless
// it was opened for a specific one (?pane=media).
const lastPane = useLocalStorage<SettingsPane>('dispatch-settings-pane', 'general')
const initial = new URLSearchParams(location.search).get('pane') as SettingsPane | null
const pane = ref<SettingsPane>(PANES.some((p) => p.id === initial) ? initial! : lastPane.value)
if (!PANES.some((p) => p.id === pane.value)) pane.value = 'general'
const current = computed(() => PANES.find((p) => p.id === pane.value) ?? PANES[0])

watch(
  pane,
  (p) => {
    lastPane.value = p
    document.title = current.value.label
    getCurrentWindow()
      .setTitle(current.value.label)
      .catch(() => {})
  },
  { immediate: true },
)

// ── Fit the window height to the pane ──────────────────────────────────────
const chrome = ref<HTMLElement | null>(null)
const inner = ref<HTMLElement | null>(null)
let lastHeight = 0
function fit() {
  if (!chrome.value || !inner.value) return
  const max = Math.round((window.screen?.availHeight || 900) * 0.85)
  const want = Math.min(max, Math.max(240, chrome.value.offsetHeight + inner.value.offsetHeight))
  if (Math.abs(want - lastHeight) < 2) return
  lastHeight = want
  getCurrentWindow()
    .setSize(new LogicalSize(WIDTH, want))
    .catch(() => {})
}
useResizeObserver(inner, fit)
watch(pane, () => nextTick(fit))

// ── Drag the window from the toolbar (Overlay titlebar, see App.vue) ────────
async function startDrag(e: MouseEvent) {
  if (e.button !== 0) return
  if ((e.target as HTMLElement).closest('button, input, a, select')) return
  try {
    await getCurrentWindow().startDragging()
  } catch {
    /* not in Tauri */
  }
}

let unlisten: UnlistenFn | null = null
onMounted(async () => {
  document.documentElement.classList.add('is-settings-window')
  await load()
  unlisten = await listen<string>('settings:show-pane', (e) => {
    if (PANES.some((p) => p.id === e.payload)) pane.value = e.payload as SettingsPane
  }).catch(() => null)
})
onUnmounted(() => unlisten?.())

// ⌘1…⌘5 jump to a pane (System Settings-style accelerators).
function onKey(e: KeyboardEvent) {
  if (!e.metaKey || e.altKey || e.ctrlKey) return
  const n = Number(e.key)
  if (n >= 1 && n <= PANES.length) {
    e.preventDefault()
    pane.value = PANES[n - 1].id
  }
}
onMounted(() => window.addEventListener('keydown', onKey))
onUnmounted(() => window.removeEventListener('keydown', onKey))
</script>

<template>
  <div class="st-window">
    <header ref="chrome" class="st-chrome" @mousedown="startDrag">
      <div class="st-title">{{ current.label }}</div>
      <nav class="st-toolbar" role="tablist" aria-label="Settings">
        <button
          v-for="(p, i) in PANES"
          :key="p.id"
          class="st-tab"
          :class="{ 'is-active': pane === p.id }"
          role="tab"
          :aria-selected="pane === p.id"
          :title="`${p.label} (⌘${i + 1})`"
          @click="pane = p.id"
        >
          <component :is="p.icon" :size="22" :weight="pane === p.id ? 'fill' : 'regular'" aria-hidden="true" />
          <span>{{ p.label }}</span>
        </button>
      </nav>
    </header>

    <main class="st-body">
      <div ref="inner" class="st-inner" role="tabpanel" :aria-label="current.label">
        <p v-if="loadError" class="st-note is-error">Couldn’t load settings: {{ loadError }}</p>
        <p v-if="saveError" class="st-note is-error st-save-error">{{ saveError }}</p>
        <component :is="current.component" v-if="config" :key="pane" />
      </div>
    </main>
  </div>
</template>

<style>
html.is-settings-window,
html.is-settings-window body {
  background: var(--window);
  height: 100%;
  overflow: hidden;
}
html.is-settings-window #app {
  height: 100%;
}
</style>

<style scoped>
.st-window {
  height: 100%;
  display: flex;
  flex-direction: column;
  font-size: 13px;
  color: var(--label);
}

.st-chrome {
  flex: 0 0 auto;
  padding-top: 6px; /* the traffic lights sit on this row */
  border-bottom: 1px solid var(--separator);
  background: var(--window);
}

.st-title {
  height: 22px;
  line-height: 22px;
  text-align: center;
  font-size: 13px;
  font-weight: 600;
  color: var(--label);
}

.st-toolbar {
  display: flex;
  justify-content: center;
  gap: 2px;
  padding: 4px var(--space-2) 6px;
}

.st-tab {
  appearance: none;
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 2px;
  min-width: 72px;
  padding: 5px 8px 4px;
  font: inherit;
  font-size: 11px;
  color: var(--label-2);
  background: transparent;
  border: none;
  border-radius: 6px;
  cursor: default;
}
.st-tab:hover {
  color: var(--label);
}
.st-tab.is-active {
  color: var(--accent);
  background: var(--fill);
}
.st-tab.is-active span {
  color: var(--label);
}
.st-tab:focus-visible {
  outline: none;
  box-shadow: 0 0 0 3px var(--accent-focus);
}

.st-body {
  flex: 1 1 auto;
  overflow-y: auto;
}
.st-inner {
  padding: var(--space-5) var(--space-5) var(--space-5);
}
.st-save-error {
  margin-bottom: var(--space-3);
}
</style>
