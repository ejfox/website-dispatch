import { ref, computed } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { useToasts } from './useToasts'

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
