// Grouped publish-flow composables: the publish/confirm/success lifecycle
// and post-level actions (webmentions, unpublish, etc).

import { ref, computed, watch, type Ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { ask, message } from '@tauri-apps/plugin-dialog'
import { useToasts } from './useUiState'
import type { WebmentionReport } from '../types'

// ── Publishing lifecycle ────────────────────────────────────────────────────

/** A push is not a deploy. website2 builds in GitHub Actions (~3 min) and can
 *  roll back, so after `publish_file` returns we poll the site's healthcheck
 *  (Rust `wait_for_deploy`) and only then call the post live. */
export interface DeployState {
  slug: string
  sha: string
  url: string
  phase: 'deploying' | 'live' | 'failed'
  message: string
  elapsedSecs: number
  runUrl: string
}

interface DeployResult {
  sha: string
  phase: 'live' | 'deployed_unverified' | 'timeout' | 'failed'
  ok: boolean
  message: string
  deployed_commit: string | null
  url_status: number | null
  run_url: string
}

const ACTIONS_URL = 'https://github.com/ejfox/website2/actions'

// Fire-and-forget native macOS system sound. Names map to files in
// /System/Library/Sounds (Glass, Hero, Pop, Tink, Sosumi, Submarine, etc).
function playSound(name: string) {
  invoke('play_system_sound', { name }).catch(() => {})
}

export function usePublishing(options: {
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
  /** Called once the deploy is confirmed and the post URL answers 200.
   *  Webmentions belong here — the page has to exist before discovery. */
  onLive?: (url: string) => void
  /** Post title, for the "is live" macOS notification. */
  getTitle?: () => string | null
}) {
  const publishing = ref(false)
  const justPublished = ref<string | null>(null)
  const justPublishedGlow = ref(false)
  const showSuccess = ref(false)
  const successMessage = ref('')
  const showPublishConfirm = ref(false)
  const publishConfirmStep = ref<1 | 2>(1)
  const publishConfirmChecked = ref(false)
  const publishConfirmText = ref('')
  const publishConfirmRepublish = ref(false)
  const showSyndicationWizard = ref(false)
  const showAltTextReviewer = ref(false)

  // Keyed by slug: FilePreview survives file switches, and a deploy keeps
  // running in the background after you click away.
  const deploys = ref<Record<string, DeployState>>({})
  const currentDeploy = computed(() => deploys.value[options.getFilePath()] ?? null)

  const toasts = useToasts()

  async function openPublishConfirm(isRepublish = false) {
    publishConfirmRepublish.value = isRepublish
    publishConfirmStep.value = 1
    publishConfirmChecked.value = false
    publishConfirmText.value = ''
    showPublishConfirm.value = true
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
      const url = await invoke<string>('publish_file', {
        sourcePath: options.getFilePath(),
        targetId: options.getActiveTargetId() || null,
      })
      justPublished.value = url

      // Push-time feedback is quiet: the post isn't live yet.
      setTimeout(() => options.onPublished(), 500)
      // Fire-and-forget post-push hook (desk shipping, syndication wizard).
      if (options.onPublishSuccess) {
        try {
          options.onPublishSuccess(url)
        } catch (e) {
          console.warn('onPublishSuccess threw', e)
        }
      }

      // Don't await: the deploy takes minutes and shouldn't lock the UI.
      void watchDeploy(options.getFilePath(), url, options.getActiveTargetId(), isRepublish)
    } catch (e) {
      toasts.error('Publish failed', String(e))
    }
    publishing.value = false
  }

  async function watchDeploy(slug: string, url: string, targetId: string | undefined, isRepublish: boolean) {
    let sha: string
    try {
      sha = await invoke<string>('get_head_sha', { targetId: targetId || null })
    } catch (e) {
      deploys.value[slug] = {
        slug,
        sha: '',
        url,
        phase: 'failed',
        elapsedSecs: 0,
        runUrl: ACTIONS_URL,
        message: `Pushed, but couldn't read the commit to track the deploy: ${e}`,
      }
      return
    }
    deploys.value[slug] = {
      slug,
      sha,
      url,
      phase: 'deploying',
      elapsedSecs: 0,
      runUrl: ACTIONS_URL,
      message: `pushed ${sha.slice(0, 7)}`,
    }

    const unlisten = await listen<{ sha: string; elapsed_secs: number; message: string }>('deploy-progress', (ev) => {
      const d = deploys.value[slug]
      if (!d || d.sha !== ev.payload.sha || d.phase !== 'deploying') return
      d.elapsedSecs = ev.payload.elapsed_secs
      d.message = ev.payload.message
    }).catch(() => null)

    let result: DeployResult
    try {
      result = await invoke<DeployResult>('wait_for_deploy', {
        sha,
        postUrl: url,
        timeoutSecs: 360,
        targetId: targetId || null,
        title: options.getTitle?.() ?? null,
        skipUrlCheck: options.isPasswordProtected(),
      })
    } catch (e) {
      result = {
        sha,
        phase: 'failed',
        ok: false,
        message: String(e),
        deployed_commit: null,
        url_status: null,
        run_url: ACTIONS_URL,
      }
    } finally {
      unlisten?.()
    }

    const d = deploys.value[slug]
    // A newer publish of the same slug replaced this one — let it report.
    if (!d || d.sha !== sha) return
    d.runUrl = result.run_url || ACTIONS_URL
    d.message = result.message
    if (result.ok) {
      d.phase = 'live'
      playSound('Glass')
      if (slug === options.getFilePath()) {
        const visibilityContext = options.isPasswordProtected()
          ? ' (protected)'
          : options.isUnlisted()
            ? ' (unlisted)'
            : ''
        showSuccessToast(isRepublish ? 'Republished — live' : `Live${visibilityContext}`)
        justPublishedGlow.value = true
        setTimeout(() => {
          justPublishedGlow.value = false
        }, 1500)
      }
      options.onLive?.(url)
    } else {
      d.phase = 'failed'
      toasts.error('Deploy not confirmed', result.message)
    }
  }

  function dismissDeploy(slug: string) {
    delete deploys.value[slug]
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
    currentDeploy,
    dismissDeploy,
    showSuccess,
    successMessage,
    showPublishConfirm,
    publishConfirmStep,
    publishConfirmChecked,
    publishConfirmText,
    publishConfirmRepublish,
    showSyndicationWizard,
    showAltTextReviewer,
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
      options.showSuccessToast(`Converted to Vue page — edit ${path.split('/').slice(-3).join('/')}`, 5000)
    } catch (e) {
      await message(String(e), { title: 'Couldn’t convert to a Vue page', kind: 'error' })
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
    opts: { bridgyFed?: boolean; silent?: boolean; force?: boolean; url?: string } = {},
  ) {
    // Explicit url: the deploy may finish after the user switched files.
    const url = opts.url ?? options.getLiveUrl()
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
      if (!opts.silent) await message(String(e), { title: 'Couldn’t send webmentions', kind: 'error' })
      console.warn('webmention send failed', e)
    }
    sendingWebmentions.value = false
  }

  /** Auto-fired from `usePublishing.onLive` — after the deploy is confirmed
   *  and the post URL answered 200, so discovery sees the real page.
   *  Always silent — the result surfaces inline via the StatusBanner chip. */
  async function autoTriggerOnPublish(bridgyFed: boolean, url?: string) {
    await triggerWebmentions({ bridgyFed, silent: true, force: true, url })
  }

  async function unpublish() {
    if (!options.isLive() || !options.slug.value || unpublishing.value) return
    // Native NSAlert: wry implements no WKUIDelegate confirm panel, so a
    // webview confirm() silently returns false and Unpublish never ran.
    const ok = await ask(`"${options.slug.value}" will come off the site and move back to drafts.`, {
      title: 'Unpublish this post?',
      kind: 'warning',
      okLabel: 'Unpublish',
      cancelLabel: 'Cancel',
    })
    if (!ok) return
    unpublishing.value = true
    try {
      await invoke('unpublish_file', { slug: options.slug.value, targetId: options.getActiveTargetId() || null })
      options.showSuccessToast('Unpublished — moved to drafts')
      setTimeout(() => options.onRefresh(), 500)
    } catch (e) {
      await message(String(e), { title: 'Couldn’t unpublish', kind: 'error' })
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
