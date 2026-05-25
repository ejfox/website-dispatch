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

  /** URL keyed cache so we don't auto-send twice for the same publish.
   *  Republishing clears the cache for that URL (handled by `triggerWebmentions`'s force flag). */
  const autoSentFor = new Set<string>()

  /**
   * Run webmention discovery + send for the post's live URL.
   *
   * @param opts.bridgyFed — also POST to Bridgy Fed (forward to fediverse)
   * @param opts.silent    — used by the auto-send path on publish, suppresses toasts
   * @param opts.force     — bypass the per-URL "already sent" cache (e.g. user clicks Resend)
   */
  async function triggerWebmentions(
    opts: { bridgyFed?: boolean; silent?: boolean; force?: boolean } = {},
  ) {
    const url = options.getLiveUrl()
    if (!url || sendingWebmentions.value) return
    if (!opts.force && autoSentFor.has(url)) return

    sendingWebmentions.value = true
    if (!opts.silent) webmentionReport.value = null
    try {
      const report = await invoke<WebmentionReport>('send_webmentions', {
        postUrl: url,
        bridgyFed: !!opts.bridgyFed,
        targetId: options.getActiveTargetId() || null,
      })
      webmentionReport.value = report
      autoSentFor.add(url)
      if (!opts.silent && report.sent > 0) {
        options.showSuccessToast(`Notified ${report.sent} site${report.sent === 1 ? '' : 's'}`)
      }
    } catch (e) {
      if (!opts.silent) alert(`Webmention error: ${e}`)
      console.warn('webmention send failed', e)
    }
    sendingWebmentions.value = false
  }

  /** Auto-fired from `usePublishing` after a successful publish or republish.
   *  Always silent — the result surfaces inline via the StatusBanner chip. */
  async function autoTriggerOnPublish(bridgyFed: boolean) {
    // Wait a beat for the deploy to settle so the published page has the
    // outbound links rendered — otherwise webmention discovery sees the
    // pre-deploy HTML and reports zero links.
    await new Promise((r) => setTimeout(r, 8000))
    await triggerWebmentions({ bridgyFed, silent: true, force: true })
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
    autoTriggerOnPublish,
    unpublish,
  }
}
