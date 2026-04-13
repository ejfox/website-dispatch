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
  rightTab: Ref<'preview' | 'media' | 'journal'>
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

  function handleGlobalKey(e: KeyboardEvent) {
    // Cmd+K for search
    if ((e.metaKey || e.ctrlKey) && e.key === 'k') {
      e.preventDefault()
      if (options.searchOpen.value) options.closeSearch()
      else options.openSearch()
      return
    }

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

    // , opens settings
    if (e.key === ',' && !e.metaKey && !e.ctrlKey) {
      e.preventDefault()
      options.showSettings.value = true
      return
    }

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

    // n - new post
    if (e.key === 'n' && !e.metaKey && !e.ctrlKey) {
      e.preventDefault()
      options.openNewPost()
      return
    }

    // r - refresh
    if (e.key === 'r' && !e.metaKey && !e.ctrlKey) {
      options.loadFiles()
    }

    // m - toggle media library
    if (e.key === 'm' && !e.metaKey && !e.ctrlKey) {
      e.preventDefault()
      options.rightTab.value = options.rightTab.value === 'media' ? 'preview' : 'media'
    }

    // J - toggle journal
    if (e.key === 'J' && !e.metaKey && !e.ctrlKey) {
      e.preventDefault()
      options.rightTab.value = options.rightTab.value === 'journal' ? 'preview' : 'journal'
    }

    // Cmd+Enter to publish
    if ((e.metaKey || e.ctrlKey) && e.key === 'Enter' && options.selectedFile.value) {
      e.preventDefault()
      const isRepublish = !!options.selectedFile.value.published_url
      options.filePreviewRef.value?.openPublishConfirm(isRepublish)
    }
  }

  return { lastGPress, handleSearchKey, handleGlobalKey }
}
