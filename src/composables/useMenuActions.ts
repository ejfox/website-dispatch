import { nextTick, onMounted, onUnmounted, type Ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import type { MarkdownFile } from '../types'
import { useToasts, useInspector } from './useUiState'

type RightTab = 'desk' | 'preview' | 'media' | 'activity' | 'modified' | 'journal'
type HelpSection = 'overview' | 'shortcuts'

/** The bits of FilePreview a menu item can reach. It only exists while the
 *  Preview tab shows a post, so go through `withPreview()`. */
interface PreviewHandle {
  unpublish: () => unknown
}

/**
 * Native menu bar actions (src-tauri/src/menu.rs → `menu-<id>` events).
 *
 * On macOS a menu item's key equivalent fires before the webview sees the
 * key, so every ⌘-shortcut with a menu item lives here, not in
 * useKeyboardShortcuts. v0.8.0 declared eleven of these with no listener,
 * Publish ⌘↩ among them. A cargo test (menu.rs) checks each menu id is
 * named in this file or in App.vue.
 *
 * Post actions call what the matching toolbar button or key calls.
 */

/** Listened to in App.vue itself: the five originals (new post, refresh,
 *  compact, search, settings), plus publish and publish-unlisted, which
 *  open the publish review sheet (feat/publish-review owns those). */
export const MENU_EVENTS_IN_APP_VUE = [
  'menu-new-post',
  'menu-refresh',
  'menu-toggle-compact',
  'menu-search',
  'menu-settings',
  'menu-publish',
  'menu-publish-unlisted',
] as const

export function useMenuActions(options: {
  files: Ref<MarkdownFile[]>
  selectedFile: Ref<MarkdownFile | null>
  rightTab: Ref<RightTab>
  filePreviewRef: Ref<PreviewHandle | null>
  sidebarCollapsed: Ref<boolean>
  showHelp: Ref<boolean>
  helpSection: Ref<HelpSection>
  /** Open-state of App.vue's own modals; any true blocks post actions. */
  modalFlags: Ref<boolean>[]
}) {
  const toasts = useToasts()
  const inspector = useInspector()
  const { selectedFile, rightTab } = options

  /** A sheet or modal is up, so a menu shortcut mustn't act on the post
   *  hidden underneath it. */
  function modalOpen(): boolean {
    return options.modalFlags.some((f) => f.value) || !!document.querySelector('[class$="-overlay"]')
  }

  function needFile(): MarkdownFile | null {
    if (!selectedFile.value) {
      toasts.info('Select a post first')
      return null
    }
    return selectedFile.value
  }

  function needLive(): MarkdownFile | null {
    const f = needFile()
    if (f && !f.published_url) {
      toasts.info('This post isn’t live yet')
      return null
    }
    return f
  }

  /** Show the selected post's Preview tab and hand back its FilePreview. */
  async function withPreview(): Promise<PreviewHandle | null> {
    if (!needFile()) return null
    if (rightTab.value !== 'preview') {
      rightTab.value = 'preview'
      await nextTick()
    }
    return options.filePreviewRef.value
  }

  /** Unpublish: the toolbar's Unpublish button (it asks first, natively). */
  async function unpublishSelected() {
    if (modalOpen()) return
    if (!needLive()) return
    ;(await withPreview())?.unpublish()
  }

  /** Open the social-card (OG image) picker and roll a batch. OgImagePicker
   *  listens for this; it isn't mounted for dispatch pieces, which
   *  `handled` reports back. The picker lives in the inspector, so make
   *  sure that is on screen first. */
  async function generateOgForSelected() {
    if (!(await withPreview())) return
    if (!inspector.visible.value) {
      inspector.show()
      await nextTick()
    }
    const detail = { handled: false }
    window.dispatchEvent(new CustomEvent('dispatch:generate-og', { detail }))
    if (!detail.handled) toasts.info('Social cards are for blog posts', 'Dispatch pieces use their chart image.')
  }

  function openHelp(section: HelpSection = 'shortcuts') {
    options.helpSection.value = section
    options.showHelp.value = true
  }

  const actions: Record<string, () => unknown> = {
    // File
    'menu-open-in-obsidian': () => {
      const f = needFile()
      if (f) invoke('open_in_obsidian', { path: f.path })
    },
    'menu-reveal-in-finder': () => {
      const f = needFile()
      if (f) invoke('open_in_app', { path: f.path, app: 'Finder' }).catch(() => {})
    },
    'menu-open-site-preview': () => invoke('open_preview'),
    // View
    'menu-toggle-sidebar': () => (options.sidebarCollapsed.value = !options.sidebarCollapsed.value),
    'menu-toggle-inspector': () => inspector.toggle(),
    'menu-panel-desk': () => (rightTab.value = 'desk'),
    'menu-panel-preview': () => (rightTab.value = 'preview'),
    'menu-panel-media': () => (rightTab.value = 'media'),
    'menu-panel-journal': () => (rightTab.value = 'journal'),
    'menu-panel-changed': () => (rightTab.value = 'modified'),
    'menu-panel-activity': () => (rightTab.value = 'activity'),
    // Publish (Publish ⌘↩ and Publish as Unlisted ⌘⇧↩ are in App.vue)
    'menu-unpublish': unpublishSelected,
    'menu-view-on-site': () => {
      const f = needLive()
      if (f?.published_url) window.open(f.published_url, '_blank')
    },
    'menu-copy-public-url': () => {
      const f = needLive()
      if (!f?.published_url) return
      navigator.clipboard.writeText(f.published_url)
      toasts.success('Link copied')
    },
    'menu-generate-og': generateOgForSelected,
    'menu-show-journal': () => (rightTab.value = 'journal'),
    // Help
    'menu-show-help': () =>
      options.showHelp.value && options.helpSection.value === 'shortcuts'
        ? (options.showHelp.value = false)
        : openHelp('shortcuts'),
    'menu-show-overview': () => openHelp('overview'),
  }

  const disposers: Array<() => void> = []
  onMounted(() => {
    for (const [event, run] of Object.entries(actions)) {
      listen(event, () => run())
        .then((u) => disposers.push(u))
        .catch(() => {})
    }
  })
  onUnmounted(() => {
    disposers.forEach((u) => u())
    disposers.length = 0
  })

  return { openHelp, unpublishSelected }
}
