// Grouped publish-flow composables: the publish/confirm/success lifecycle
// and post-level actions (webmentions, unpublish, etc).

import { ref, computed, watch, type Ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { useToasts } from './useUiState'
import type { WebmentionReport } from '../types'

// ── Publishing lifecycle ────────────────────────────────────────────────────

// Fire-and-forget native macOS system sound. Names map to files in
// /System/Library/Sounds (Glass, Hero, Pop, Tink, Sosumi, Submarine, etc).
function playSound(name: string) {
  invoke('play_system_sound', { name }).catch(() => {})
}

export function usePublishing(options: {
  getSlug: () => string
  getFilePath: () => string
  getFileIsSafe: () => boolean
  getActiveTargetId: () => string | undefined
  isPasswordProtected: () => boolean
  isUnlisted: () => boolean
  onPublished: () => void
  /** Called after a successful publish/republish with the live URL.
   *  Used by FilePreview to silently auto-fire webmentions. Optional —
   *  if omitted, no auto-fire happens. */
  onPublishSuccess?: (url: string) => void
}) {
  const publishing = ref(false)
  const justPublished = ref<string | null>(null)
  const justPublishedGlow = ref(false)
  const showSuccess = ref(false)
  const successMessage = ref('')
  const isMilestoneToast = ref(false)
  const showPublishConfirm = ref(false)
  const publishConfirmStep = ref<1 | 2>(1)
  const publishConfirmChecked = ref(false)
  const publishConfirmText = ref('')
  const publishConfirmRepublish = ref(false)
  const showSyndicationWizard = ref(false)
  const showAltTextReviewer = ref(false)

  const toasts = useToasts()
  const journalStatsCache = ref<any>(null)

  const publishContext = computed(() => {
    const s = journalStatsCache.value
    if (!s) return null
    const parts: string[] = []
    const nth = s.total_publishes + 1
    if (nth > 1) parts.push(`This will be publish #${nth}`)
    if (s.current_streak_days >= 2) parts.push(`${s.current_streak_days}-day streak`)
    if (s.words_this_month > 1000) parts.push(`${(s.words_this_month / 1000).toFixed(1)}k words this month`)
    return parts.length > 0 ? parts.join(' · ') : null
  })

  async function openPublishConfirm(isRepublish = false) {
    publishConfirmRepublish.value = isRepublish
    publishConfirmStep.value = 1
    publishConfirmChecked.value = false
    publishConfirmText.value = ''
    showPublishConfirm.value = true
    try {
      journalStatsCache.value = await invoke('get_journal_stats')
    } catch (_) {
      /* ok */
    }
  }

  function closePublishConfirm() {
    showPublishConfirm.value = false
  }

  async function publish(isRepublish = false) {
    // In-flight guard: publishing commits + pushes, so a second call while the
    // first is still running would publish (and git-commit) the post twice.
    // The confirm button/Enter can both fire in the same burst before the modal
    // unmounts, so guard here — the load-bearing spot — not just in the UI.
    if (publishing.value) return
    if (!isRepublish && !options.getFileIsSafe()) return
    publishing.value = true
    try {
      let milestonesBefore: string[] = []
      try {
        const statsBefore = await invoke<any>('get_journal_stats')
        milestonesBefore = (statsBefore.milestones || []).filter((m: any) => m.achieved_at).map((m: any) => m.id)
      } catch (_) {
        /* journal may not be ready */
      }

      const url = await invoke<string>('publish_file', {
        sourcePath: options.getFilePath(),
        slug: options.getSlug(),
        targetId: options.getActiveTargetId() || null,
      })
      justPublished.value = url

      let newMilestone: { label: string; description: string } | null = null
      try {
        const statsAfter = await invoke<any>('get_journal_stats')
        const earned = (statsAfter.milestones || []).filter((m: any) => m.achieved_at)
        const fresh = earned.find((m: any) => !milestonesBefore.includes(m.id))
        if (fresh) newMilestone = { label: fresh.label, description: fresh.description }
      } catch (_) {
        /* ok */
      }

      isMilestoneToast.value = !!newMilestone
      if (newMilestone) {
        successMessage.value = `${newMilestone.label}! ${newMilestone.description}`
        playSound('Hero') // Triumphant fanfare for milestones
      } else {
        playSound('Glass') // Default success chime
        const visibilityContext = options.isPasswordProtected()
          ? ' (protected)'
          : options.isUnlisted()
            ? ' (unlisted)'
            : ''
        successMessage.value = isRepublish ? 'Republished!' : `Published${visibilityContext}!`
      }
      showSuccess.value = true
      justPublishedGlow.value = true
      setTimeout(
        () => {
          showSuccess.value = false
        },
        newMilestone ? 5000 : 3000,
      )
      setTimeout(() => {
        justPublishedGlow.value = false
      }, 1500)

      setTimeout(() => options.onPublished(), 500)
      // Fire-and-forget post-publish hook (auto webmentions, etc).
      // Owner of the callback handles its own delays and silence.
      if (options.onPublishSuccess) {
        try {
          options.onPublishSuccess(url)
        } catch (e) {
          console.warn('onPublishSuccess threw', e)
        }
      }
    } catch (e) {
      toasts.error('Publish failed', String(e))
    }
    publishing.value = false
  }

  async function publishUnlisted() {
    if (!options.getFileIsSafe()) return
    try {
      await invoke('set_frontmatter', { path: options.getFilePath(), key: 'unlisted', value: 'true' })
      await publish(false)
    } catch (e) {
      toasts.error('Failed to set unlisted', String(e))
    }
  }

  function showSuccessToast(msg: string, duration = 3000) {
    successMessage.value = msg
    showSuccess.value = true
    setTimeout(() => {
      showSuccess.value = false
    }, duration)
  }

  function onSyndicationQueued() {
    showSyndicationWizard.value = false
    showSuccessToast('Posts queued for syndication!')
  }

  function onAltTextApplied() {
    showAltTextReviewer.value = false
    showSuccessToast('Alt text applied!')
    setTimeout(() => options.onPublished(), 500)
  }

  return {
    publishing,
    justPublished,
    justPublishedGlow,
    showSuccess,
    successMessage,
    isMilestoneToast,
    showPublishConfirm,
    publishConfirmStep,
    publishConfirmChecked,
    publishConfirmText,
    publishConfirmRepublish,
    showSyndicationWizard,
    showAltTextReviewer,
    journalStatsCache,
    publishContext,
    openPublishConfirm,
    closePublishConfirm,
    publish,
    publishUnlisted,
    showSuccessToast,
    onSyndicationQueued,
    onAltTextApplied,
  }
}

// ── Post actions ────────────────────────────────────────────────────────────

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
  const isVuePage = ref(false)
  const converting = ref(false)
  const vuePageHue = ref(220)
  const unpublishing = ref(false)

  // Check Vue-page status when slug changes
  watch(
    options.slug,
    async (s) => {
      if (!s) {
        isVuePage.value = false
        return
      }
      try {
        isVuePage.value = await invoke<boolean>('is_vue_page', { slug: s })
      } catch {
        isVuePage.value = false
      }
    },
    { immediate: true },
  )

  async function convertToVuePage() {
    if (!options.slug.value || converting.value) return
    converting.value = true
    try {
      const path = await invoke<string>('convert_to_vue_page', {
        slug: options.slug.value,
        hue: vuePageHue.value,
      })
      isVuePage.value = true
      options.showSuccessToast(
        `Converted to Vue page — edit ${path.split('/').slice(-3).join('/')}`,
        5000,
      )
    } catch (e) {
      alert(`Convert to Vue page failed: ${e}`)
    }
    converting.value = false
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
    isVuePage,
    converting,
    vuePageHue,
    unpublishing,
    convertToVuePage,
    triggerWebmentions,
    autoTriggerOnPublish,
    unpublish,
  }
}
