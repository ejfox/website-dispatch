import { ref, watch, type Ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import type { WebmentionReport } from '../types'

export function usePostActions(options: {
  slug: Ref<string>
  getLiveUrl: () => string | null
  isLive: () => boolean
  getActiveTargetId: () => string | undefined
  showSuccessToast: (msg: string, duration?: number) => void
  onRefresh: () => void
}) {
  const sendingWebmentions = ref(false)
  const webmentionReport = ref<WebmentionReport | null>(null)
  const isCrowned = ref(false)
  const crowning = ref(false)
  const crownHue = ref(220)
  const unpublishing = ref(false)

  // Check crown status when slug changes
  watch(
    options.slug,
    async (s) => {
      if (!s) {
        isCrowned.value = false
        return
      }
      try {
        isCrowned.value = await invoke<boolean>('is_post_crowned', { slug: s })
      } catch {
        isCrowned.value = false
      }
    },
    { immediate: true },
  )

  async function crownPost() {
    if (!options.slug.value || crowning.value) return
    crowning.value = true
    try {
      const path = await invoke<string>('crown_post', { slug: options.slug.value, hue: crownHue.value })
      isCrowned.value = true
      options.showSuccessToast(`Crowned! Edit ${path.split('/').slice(-3).join('/')}`, 5000)
    } catch (e) {
      alert(`Crown failed: ${e}`)
    }
    crowning.value = false
  }

  async function triggerWebmentions(bridgyFed = false) {
    const url = options.getLiveUrl()
    if (!url || sendingWebmentions.value) return
    sendingWebmentions.value = true
    webmentionReport.value = null
    try {
      const report = await invoke<WebmentionReport>('send_webmentions', {
        postUrl: url,
        bridgyFed,
        targetId: options.getActiveTargetId() || null,
      })
      webmentionReport.value = report
      if (report.sent > 0) {
        options.showSuccessToast(`Sent ${report.sent} webmention${report.sent > 1 ? 's' : ''}!`)
      }
    } catch (e) {
      alert(`Webmention error: ${e}`)
    }
    sendingWebmentions.value = false
  }

  async function unpublish() {
    if (!options.isLive() || !options.slug.value || unpublishing.value) return
    const confirmText = `Unpublish "${options.slug.value}" and move it back to drafts?`
    if (!confirm(confirmText)) return
    unpublishing.value = true
    try {
      await invoke('unpublish_file', { slug: options.slug.value, targetId: options.getActiveTargetId() || null })
      options.showSuccessToast('Unpublished — moved to drafts')
      setTimeout(() => options.onRefresh(), 500)
    } catch (e) {
      alert(`Failed: ${e}`)
    }
    unpublishing.value = false
  }

  return {
    sendingWebmentions,
    webmentionReport,
    isCrowned,
    crowning,
    crownHue,
    unpublishing,
    crownPost,
    triggerWebmentions,
    unpublish,
  }
}
