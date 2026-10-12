import { nextTick, onMounted, onUnmounted, watch, type Ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import type { MarkdownFile } from '../types'
import { useToasts, useInspector } from './useUiState'
import { useAppConfig } from './useVaultData'
import { checkForUpdate } from './useAutoUpdate'

type RightTab = 'desk' | 'preview' | 'media' | 'activity' | 'modified' | 'journal'
type HelpSection = 'overview' | 'shortcuts'

/** The bits of FilePreview a menu item can reach. It only exists while the
 *  Preview tab shows a post, so go through `withPreview()`. */
interface PreviewHandle {
  unpublish: () => unknown
  openSyndication: () => unknown
  openSchedulePicker: () => unknown
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
 *
 * The menu also needs to know what's selected, so items can grey out (no
 * post → no Publish) and labels can flip (Show/Hide Sidebar, Republish…).
 * A watcher below pushes that to `set_menu_context` (menu.rs).
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
  compactMode: Ref<boolean>
  showHelp: Ref<boolean>
  helpSection: Ref<HelpSection>
  /** Open-state of App.vue's own modals; any true blocks post actions. */
  modalFlags: Ref<boolean>[]
}) {
  const toasts = useToasts()
  const inspector = useInspector()
  const { defaultEditor } = useAppConfig()
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

  /** withPreview(), plus the inspector on screen: the schedule picker and
   *  the social-card picker live in it. */
  async function withInspector(): Promise<PreviewHandle | null> {
    const preview = await withPreview()
    if (preview && !inspector.visible.value) {
      inspector.show()
      await nextTick()
    }
    return preview
  }

  /** Open the social-card (OG image) picker and roll a batch. OgImagePicker
   *  listens for this; it isn't mounted for dispatch pieces, which
   *  `handled` reports back. */
  async function generateOgForSelected() {
    if (!(await withInspector())) return
    const detail = { handled: false }
    window.dispatchEvent(new CustomEvent('dispatch:generate-og', { detail }))
    if (!detail.handled) toasts.info('Social cards are for blog posts', 'Dispatch pieces use their chart image.')
  }

  function openHelp(section: HelpSection = 'shortcuts') {
    options.helpSection.value = section
    options.showHelp.value = true
  }

  const actions: Record<string, () => unknown> = {
    // App
    'menu-check-updates': () => checkForUpdate({ manual: true }),
    // File
    'menu-open-in-obsidian': () => {
      const f = needFile()
      if (f) invoke('open_in_obsidian', { path: f.path })
    },
    'menu-reveal-in-finder': () => {
      const f = needFile()
      if (f) invoke('open_in_app', { path: f.path, app: 'Finder' }).catch(() => {})
    },
    'menu-open-in-editor': () => {
      const f = needFile()
      if (f) invoke('open_in_app', { path: f.path, app: defaultEditor.value }).catch(() => {})
    },
    'menu-open-site-preview': () => invoke('open_preview'),
    'menu-copy-path': () => {
      const f = needFile()
      if (!f) return
      navigator.clipboard.writeText(f.path)
      toasts.success('Path copied')
    },
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
    'menu-publish-later': async () => {
      if (modalOpen()) return
      ;(await withInspector())?.openSchedulePicker()
    },
    'menu-unpublish': unpublishSelected,
    'menu-syndicate': async () => {
      if (modalOpen() || !needLive()) return
      ;(await withPreview())?.openSyndication()
    },
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
    // Help
    'menu-show-help': () =>
      options.showHelp.value && options.helpSection.value === 'shortcuts'
        ? (options.showHelp.value = false)
        : openHelp('shortcuts'),
    'menu-show-overview': () => openHelp('overview'),
  }

  let lastContext = ''
  // Keep the menubar's enabled states and labels in step with the window.
  // `immediate` so the first build after launch isn't left all-enabled.
  watch(
    () => {
      const f = selectedFile.value
      const live = !!f?.published_url
      return {
        hasFile: !!f,
        isLive: live,
        canPublishUnlisted: !!f && !live && !f.unlisted && !f.password,
        canSchedule: !!f && !live && f.is_safe && !f.publish_at,
        canGenerateOg: !!f && f.content_type !== 'dispatch',
        sidebarVisible: !options.sidebarCollapsed.value,
        inspectorVisible: inspector.visible.value,
        compact: options.compactMode.value,
        editor: defaultEditor.value || '',
      }
    },
    (ctx) => {
      // The getter re-runs on any change to the selected post; only tell
      // Rust when something the menu shows actually moved.
      const key = JSON.stringify(ctx)
      if (key === lastContext) return
      lastContext = key
      invoke('set_menu_context', { ctx }).catch(() => {})
    },
    { immediate: true, deep: true },
  )

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
