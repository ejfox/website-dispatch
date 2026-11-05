// Getting an image into a post. iA Writer owns the text, so Dispatch can't
// insert at your cursor: it uploads, puts the markdown on the clipboard, and
// brings the editor forward, leaving one keystroke (⌘V). Drag, the inspector's
// Choose… button and the palette all land here, and the state is shared so
// the inspector's image well can show the result after a toast would be gone.

import { ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { open } from '@tauri-apps/plugin-dialog'
import { writeText } from '@tauri-apps/plugin-clipboard-manager'
import { useToasts } from './useUiState'
import { useAppConfig } from './useVaultData'
import type { MarkdownFile } from '../types'

export const IMAGE_EXTS = ['png', 'jpg', 'jpeg', 'gif', 'webp', 'svg', 'heic', 'heif', 'avif', 'bmp', 'tiff']
export const VIDEO_EXTS = ['mp4', 'mov', 'webm', 'm4v', 'mkv']

export const fileExt = (p: string) => p.split('.').pop()?.toLowerCase() || ''

export type ImageUploadState =
  | { phase: 'idle' }
  | { phase: 'uploading'; count: number }
  | { phase: 'ready'; count: number; markdown: string; needsAlt: boolean }
  | { phase: 'failed'; message: string }

const state = ref<ImageUploadState>({ phase: 'idle' })

/** Where a post's media goes: "blog/2026" for year folders, otherwise the
 *  post's own folder, otherwise the root. */
function folderFor(file: MarkdownFile | null): string | undefined {
  const dir = file?.source_dir
  if (!dir) return undefined
  const year = dir.match(/(\d{4})/)
  return year ? `blog/${year[1]}` : dir
}

export function useImageUpload() {
  const toasts = useToasts()
  const { defaultEditor } = useAppConfig()

  async function uploadMedia(paths: string[], file: MarkdownFile | null) {
    if (!paths.length || state.value.phase === 'uploading') return

    const status: any = await invoke('check_media_status').catch(() => null)
    const primary = (status?.primary ?? 'cloudinary').toString()
    // Catch missing credentials early: otherwise it's N "Upload failed"s.
    if (status && status[primary] === false) {
      const message = `${primary} isn’t set up. Add it in Settings › Media.`
      state.value = { phase: 'failed', message }
      toasts.error(`${primary} not configured`, message)
      return
    }
    if (!file) {
      toasts.warn('No post selected', 'Files will upload to the media root. Click a post first to organize by folder.')
    }

    state.value = { phase: 'uploading', count: paths.length }
    const folder = folderFor(file)
    const refs: { idx: number; ref: string }[] = []
    const mirrorFailures: string[] = []
    let failures = 0
    let lastError: string | undefined

    async function uploadOne(path: string, idx: number) {
      try {
        const result: any = await invoke('media_upload', { filePath: path, folder })
        if (result?.success && result?.asset?.url) {
          const url = result.asset.url
          const video = VIDEO_EXTS.includes(fileExt(path))
          // Describe it now so nothing's left to clean up later. No vision key
          // or a failed call just means an empty alt: the upload still counts.
          const alt = video ? '' : await invoke<string>('alt_for_url', { url }).catch(() => '')
          const cleanAlt = alt.replace(/[[\]\n]/g, ' ').trim()
          refs.push({ idx, ref: video ? `<video src="${url}" controls></video>` : `![${cleanAlt}](${url})` })
          for (const m of result.mirrors ?? []) {
            if (!m.success) mirrorFailures.push(`${m.destination}: ${m.error ?? 'unknown'}`)
          }
        } else {
          failures++
          lastError = result?.error ?? 'unknown error'
        }
      } catch (e: any) {
        failures++
        lastError = typeof e === 'string' ? e : (e?.message ?? String(e))
      }
    }

    // Three at a time: fine for Cloudinary's free tier, and six screenshots
    // don't take six times as long.
    let cursor = 0
    async function worker() {
      while (cursor < paths.length) {
        const i = cursor++
        await uploadOne(paths[i], i)
      }
    }
    await Promise.all(Array.from({ length: Math.min(3, paths.length) }, () => worker()))

    if (!refs.length) {
      const message = lastError ?? `${failures} file${failures === 1 ? '' : 's'} didn’t upload`
      state.value = { phase: 'failed', message }
      toasts.error('Upload failed', message)
      return
    }

    // Drop order, not finish order.
    refs.sort((a, b) => a.idx - b.idx)
    const markdown = refs.map((r) => r.ref).join('\n\n')
    const needsAlt = refs.some((r) => r.ref.startsWith('![]'))
    try {
      await writeText(markdown)
    } catch (e) {
      const message = `Uploaded, but the clipboard refused it: ${String(e)}`
      state.value = { phase: 'failed', message }
      toasts.error('Couldn’t copy the markdown', markdown)
      return
    }
    state.value = { phase: 'ready', count: refs.length, markdown, needsAlt }

    const notes: string[] = []
    if (failures) notes.push(`${failures} failed (${lastError ?? 'see the log'})`)
    if (mirrorFailures.length) notes.push(`mirror: ${mirrorFailures.join(', ')}`)
    toasts.push({
      kind: 'success',
      message: `Copied. ⌘V in ${defaultEditor.value}`,
      detail: notes.join(' · ') || undefined,
      ttl: 6000,
    })

    // Activate, don't open: opening the selected post could swap the document
    // you're writing in if Dispatch has a different post selected.
    invoke('activate_app', { app: defaultEditor.value }).catch(() => {})
  }

  async function chooseAndUpload(file: MarkdownFile | null) {
    const picked = await open({
      multiple: true,
      title: 'Choose images or video',
      filters: [{ name: 'Images and video', extensions: [...IMAGE_EXTS, ...VIDEO_EXTS] }],
    })
    if (picked) await uploadMedia(Array.isArray(picked) ? picked : [picked], file)
  }

  async function copyAgain() {
    if (state.value.phase !== 'ready') return
    await writeText(state.value.markdown)
    invoke('activate_app', { app: defaultEditor.value }).catch(() => {})
  }

  /** Back to the invitation, unless an upload is still running. */
  function reset() {
    if (state.value.phase !== 'uploading') state.value = { phase: 'idle' }
  }

  return { state, uploadMedia, chooseAndUpload, copyAgain, reset }
}
