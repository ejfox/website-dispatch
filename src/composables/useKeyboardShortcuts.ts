import { ref, type Ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import type { MarkdownFile } from '../types'
import { useAppConfig } from './useAppConfig'

export function useKeyboardShortcuts(options: {
  files: Ref<MarkdownFile[]>
  selectedFile: Ref<MarkdownFile | null>
  searchOpen: Ref<boolean>
  showSettings: Ref<boolean>
  newPostOpen: Ref<boolean>
  showHelp: Ref<boolean>
  rightTab: Ref<'preview' | 'media' | 'activity' | 'modified' | 'journal' | 'gear'>
  sidebarCollapsed: Ref<boolean>
  filePreviewRef: Ref<{ openPublishConfirm: (isRepublish: boolean) => void } | null>
  openSearch: () => void
  closeSearch: () => void
  openNewPost: () => void
  closeNewPost: () => void
  loadFiles: () => void
}) {
  const { defaultEditor } = useAppConfig()
  const lastGPress = ref(0)

  function handleSearchKey(e: KeyboardEvent) {
    if (e.key === 'Escape') {
      options.closeSearch()
    } else if (e.key === 'ArrowDown') {
      e.preventDefault()
      // Search navigation handled by SearchModal component or parent
    } else if (e.key === 'ArrowUp') {
      e.preventDefault()
    } else if (e.key === 'Enter') {
      e.preventDefault()
    }
  }

  /**
   * Returns true if the focused element is a text input, textarea, or
   * contenteditable — in which case unmodified single-key shortcuts (j, k,
   * n, i, r, m, /, etc.) must NOT fire, otherwise typing in the alt-text
   * editor or syndication wizard accidentally triggers nav actions.
   *
   * Modified shortcuts (⌘K, ⌘,, ⌘Enter, Escape) still go through.
   */
  function isTypingTarget(e: KeyboardEvent): boolean {
    const t = e.target as HTMLElement | null
    if (!t) return false
    const tag = t.tagName
    if (tag === 'INPUT' || tag === 'TEXTAREA' || tag === 'SELECT') return true
    if (t.isContentEditable) return true
    return false
  }

  function handleGlobalKey(e: KeyboardEvent) {
    // --- COMMAND-MODIFIED SHORTCUTS — fire even while typing ---

    // ⌘K toggles the command palette.
    if ((e.metaKey || e.ctrlKey) && e.key === 'k') {
      e.preventDefault()
      if (options.searchOpen.value) options.closeSearch()
      else options.openSearch()
      return
    }

    // ⌘, opens Settings (macOS convention).
    if ((e.metaKey || e.ctrlKey) && e.key === ',') {
      e.preventDefault()
      options.showSettings.value = !options.showSettings.value
      return
    }

    // ⌘0 toggles the sidebar.
    if ((e.metaKey || e.ctrlKey) && e.key === '0') {
      e.preventDefault()
      options.sidebarCollapsed.value = !options.sidebarCollapsed.value
      return
    }

    // ⌘1..6 jump to right-panel tab.
    if ((e.metaKey || e.ctrlKey) && ['1', '2', '3', '4', '5', '6'].includes(e.key)) {
      e.preventDefault()
      const tabs = ['preview', 'media', 'activity', 'modified', 'journal', 'gear'] as const
      options.rightTab.value = tabs[parseInt(e.key) - 1]
      return
    }

    // ⌘Enter publishes the selected file (republishes if live).
    if ((e.metaKey || e.ctrlKey) && e.key === 'Enter' && options.selectedFile.value) {
      e.preventDefault()
      const isRepublish = !!options.selectedFile.value.published_url
      options.filePreviewRef.value?.openPublishConfirm(isRepublish)
      return
    }

    // --- MODAL-AWARE ESCAPE HANDLING ---
    // Each modal-open guard lets only Escape through and short-circuits the
    // rest of the handler, so opening a modal doesn't accidentally also
    // navigate or trigger a typing shortcut.

    if (options.searchOpen.value) {
      if (e.key === 'Escape') options.closeSearch()
      return
    }
    if (options.showSettings.value) {
      if (e.key === 'Escape') options.showSettings.value = false
      return
    }
    if (options.newPostOpen.value) {
      if (e.key === 'Escape') options.closeNewPost()
      return
    }

    // --- TYPING GUARD ---
    // Unmodified single-key shortcuts below would hijack typing in inputs,
    // textareas, alt-text edit fields, syndication wizard, etc. Bail out
    // if the user is in any editable surface.
    if (isTypingTarget(e)) return

    // ── UNIVERSAL APP SHORTCUTS — always fire, any tab ────────────────
    // / opens search (vim style)
    if (e.key === '/' && !e.metaKey && !e.ctrlKey) {
      e.preventDefault()
      options.openSearch()
      return
    }

    // ? shows help
    if (e.key === '?' && e.shiftKey) {
      e.preventDefault()
      options.showHelp.value = !options.showHelp.value
      return
    }

    // n - new post (universal app action)
    if (e.key === 'n' && !e.metaKey && !e.ctrlKey) {
      e.preventDefault()
      options.openNewPost()
      return
    }

    // r - refresh files (universal app action)
    if (e.key === 'r' && !e.metaKey && !e.ctrlKey) {
      options.loadFiles()
    }

    // m - toggle media tab (universal navigation)
    if (e.key === 'm' && !e.metaKey && !e.ctrlKey) {
      e.preventDefault()
      options.rightTab.value = options.rightTab.value === 'media' ? 'preview' : 'media'
      return
    }

    // J - toggle journal tab (universal navigation)
    if (e.key === 'J' && !e.metaKey && !e.ctrlKey) {
      e.preventDefault()
      options.rightTab.value = options.rightTab.value === 'journal' ? 'preview' : 'journal'
      return
    }

    // ── PANE OWNERSHIP ────────────────────────────────────────────────
    // Everything below is Preview-tab-specific (file-list nav + file
    // actions). On other tabs the pane owns its own single-key bindings;
    // running these in the background would step on them silently.
    // Bugs this prevents:
    //   - on Gear, arrows walked the sidebar selection in the background
    //   - `c` copied the bg-selected file's URL AND committed gear changes
    //   - `o`/`i`/`p`/`v` operated on the bg-selected file from any tab
    // The universal shortcuts above this point still fire everywhere.
    // Panes that share a key with a universal (e.g. Gear's `m` for Move)
    // register with `capture: true` and call stopImmediatePropagation so
    // they run first and block the universal handler.
    if (options.rightTab.value !== 'preview') return

    const currentIndex = options.selectedFile.value
      ? options.files.value.findIndex((f) => f.path === options.selectedFile.value?.path)
      : -1

    // Arrow / j/k navigation
    if (e.key === 'ArrowDown' || e.key === 'j') {
      e.preventDefault()
      const nextIndex = Math.min(currentIndex + 1, options.files.value.length - 1)
      options.selectedFile.value = options.files.value[nextIndex]
    }

    if (e.key === 'ArrowUp' || e.key === 'k') {
      e.preventDefault()
      const prevIndex = Math.max(currentIndex - 1, 0)
      options.selectedFile.value = options.files.value[prevIndex]
    }

    // gg - go to top
    if (e.key === 'g' && !e.metaKey && !e.ctrlKey) {
      const now = Date.now()
      if (lastGPress.value && now - lastGPress.value < 300) {
        options.selectedFile.value = options.files.value[0]
        lastGPress.value = 0
      } else {
        lastGPress.value = now
      }
    }

    // G - go to bottom
    if (e.key === 'G' && e.shiftKey) {
      e.preventDefault()
      options.selectedFile.value = options.files.value[options.files.value.length - 1]
    }

    // [ ] wrap navigation
    if (e.key === '[') {
      const prevIndex = currentIndex <= 0 ? options.files.value.length - 1 : currentIndex - 1
      options.selectedFile.value = options.files.value[prevIndex]
    }
    if (e.key === ']') {
      const nextIndex = currentIndex >= options.files.value.length - 1 ? 0 : currentIndex + 1
      options.selectedFile.value = options.files.value[nextIndex]
    }

    // 1-9 jump
    if (e.key >= '1' && e.key <= '9' && !e.metaKey && !e.ctrlKey) {
      const idx = parseInt(e.key) - 1
      if (idx < options.files.value.length) {
        options.selectedFile.value = options.files.value[idx]
      }
    }

    // Escape to deselect
    if (e.key === 'Escape') {
      options.selectedFile.value = null
      options.showHelp.value = false
    }

    // o - open in Obsidian
    if (e.key === 'o' && options.selectedFile.value && !e.metaKey) {
      invoke('open_in_obsidian', { path: options.selectedFile.value.path })
    }

    // i - open in default editor
    if (e.key === 'i' && options.selectedFile.value) {
      const editor = defaultEditor.value
      invoke('open_in_app', { path: options.selectedFile.value.path, app: editor })
    }

    // p - open preview
    if (e.key === 'p' && options.selectedFile.value && !e.metaKey) {
      invoke('open_preview')
    }

    // v - view on site
    if (e.key === 'v' && options.selectedFile.value?.published_url) {
      window.open(options.selectedFile.value.published_url, '_blank')
    }

    // c - copy URL
    if (e.key === 'c' && options.selectedFile.value?.published_url && !e.metaKey) {
      navigator.clipboard.writeText(options.selectedFile.value.published_url)
    }

  }

  return { lastGPress, handleSearchKey, handleGlobalKey }
}
