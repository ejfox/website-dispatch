// Grouped publish-flow composables: the publish/confirm/success lifecycle
// and post-level actions (webmentions, unpublish, etc).

import { ref, computed, watch, type Ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { ask, message } from '@tauri-apps/plugin-dialog'
import { useToasts } from './useUiState'
import type { WebmentionReport } from '../types'
import type { ReviewMode } from '../utils/publishReview'

// ── Publishing lifecycle ────────────────────────────────────────────────────

/** A push is not a deploy. website2 builds in GitHub Actions (~3 min) and can
 *  roll back, so after `publish_file` returns we poll the site's healthcheck
 *  (Rust `wait_for_deploy`) and only then call the post live.
 *
 *  `hidden`: the site embargoes the post (`publishAt`/`date` in the future),
 *  so a confirmed deploy is the success and the page appears on its own.
 *  `failed` carries why, so the UI can say what happened and what to do. */
export interface DeployState {
  slug: string
  sha: string
  url: string
  phase: 'deploying' | 'live' | 'hidden' | 'failed'
  /** Why it isn't live (phase `failed`):
   *  run        GitHub's deploy failed or rolled back; the site kept the old version
   *  timeout    the site never reported the commit; it may still be deploying
   *  unverified the commit is deployed but the post URL didn't answer 200
   *  untracked  pushed, but the commit couldn't be read to watch the deploy */
  failure?: 'run' | 'timeout' | 'unverified' | 'untracked'
  message: string
  elapsedSecs: number
  runUrl: string
  /** What `recheckDeploy` needs to watch the same push again. */
  title: string
  targetId?: string
  isRepublish: boolean
  webmentions: boolean
  hiddenUntil: string | null
}

interface DeployResult {
  sha: string
  phase: 'live' | 'hidden' | 'deployed_unverified' | 'timeout' | 'failed'
  ok: boolean
  message: string
  deployed_commit: string | null
  url_status: number | null
  run_url: string
}

const ACTIONS_URL = 'https://github.com/ejfox/website2/actions'

const FAILURE_OF: Record<string, DeployState['failure']> = {
  failed: 'run',
  timeout: 'timeout',
  deployed_unverified: 'unverified',
}

/** Headline for a deploy that didn't end live — the toast and the
 *  inspector say the same thing. */
export function deployFailureTitle(f: DeployState['failure']): string {
  switch (f) {
    case 'run':
      return 'Deploy failed'
    case 'timeout':
      return 'Not live yet'
    case 'unverified':
      return 'Deployed, but the page isn’t loading'
    default:
      return 'Pushed, but not tracked'
  }
}

// Fire-and-forget native macOS system sound. Names map to files in
// /System/Library/Sounds (Glass, Hero, Pop, Tink, Sosumi, Submarine, etc).
function playSound(name: string) {
  invoke('play_system_sound', { name }).catch(() => {})
}

/** Rust errors put a plain sentence on the first line and git's own output
 *  after it. Split them so the toast leads with the sentence. */
function splitError(e: unknown): { message: string; detail?: string } {
  const text = String(e).trim()
  const nl = text.indexOf('\n')
  if (nl > 0) return { message: text.slice(0, nl).trim(), detail: text.slice(nl + 1).trim() }
  return { message: 'Didn’t publish', detail: text }
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
   *  Webmentions belong here — the page has to exist before discovery.
   *  `webmentions` is the review sheet's toggle for this publish. */
  onLive?: (url: string, opts: { webmentions: boolean }) => void
  /** Post title, for the "is live" toast and macOS notification. */
  getTitle?: () => string | null
  /** Reopen the review sheet for `path` (a failed push's "Try again…"). */
  onRetry?: (isRepublish: boolean, path: string) => void
}) {
  const publishing = ref(false)
  const justPublished = ref<string | null>(null)
  const justPublishedGlow = ref(false)
  const showSuccess = ref(false)
  const successMessage = ref('')
  const showPublishConfirm = ref(false)
  const publishConfirmRepublish = ref(false)
  /** `unlisted`: the sheet for the Unlisted button, which writes
   *  `unlisted: true` before publishing. */
  const publishConfirmMode = ref<ReviewMode>('publish')
  const showSyndicationWizard = ref(false)
  const showAltTextReviewer = ref(false)

  // Keyed by slug: FilePreview survives file switches, and a deploy keeps
  // running in the background after you click away.
  const deploys = ref<Record<string, DeployState>>({})
  const currentDeploy = computed(() => deploys.value[options.getFilePath()] ?? null)

  const toasts = useToasts()

  /** Every publish and republish goes through the review sheet. */
  function openPublishConfirm(isRepublish = false, mode: ReviewMode = 'publish') {
    publishConfirmRepublish.value = isRepublish
    publishConfirmMode.value = mode
    showPublishConfirm.value = true
  }

  function closePublishConfirm() {
    showPublishConfirm.value = false
  }

  async function publish(isRepublish = false, opts: { webmentions?: boolean; hiddenUntil?: string | null } = {}) {
    // In-flight guard: publishing commits + pushes, so a second call while the
    // first is still running would publish (and git-commit) the post twice.
    // The confirm button/Enter can both fire in the same burst before the modal
    // unmounts, so guard here — the load-bearing spot — not just in the UI.
    if (publishing.value) return
    if (!isRepublish && !options.getFileIsSafe()) return
    publishing.value = true
    const path = options.getFilePath()
    const title = options.getTitle?.() || 'Your post'
    try {
      const url = await invoke<string>('publish_file', {
        sourcePath: path,
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
      void watchDeploy({
        slug: path,
        url,
        title,
        targetId: options.getActiveTargetId(),
        isRepublish,
        // Nothing to notify while the page is hidden.
        webmentions: !opts.hiddenUntil && (opts.webmentions ?? true),
        hiddenUntil: opts.hiddenUntil ?? null,
      })
    } catch (e) {
      // Say what state things are in (Rust's first line), and offer the way
      // back: the sheet re-checks everything before trying again.
      const { message: msg, detail } = splitError(e)
      playSound('Sosumi')
      const id: number = toasts.push({
        kind: 'error',
        message: msg,
        detail,
        action: options.onRetry
          ? {
              label: 'Try again…',
              run: () => {
                toasts.dismiss(id)
                options.onRetry!(isRepublish, path)
              },
            }
          : undefined,
        ttl: 0,
      })
    }
    publishing.value = false
  }

  type Watch = Pick<DeployState, 'slug' | 'url' | 'title' | 'targetId' | 'isRepublish' | 'webmentions' | 'hiddenUntil'>

  async function watchDeploy(w: Watch) {
    let sha: string
    try {
      sha = await invoke<string>('get_head_sha', { targetId: w.targetId || null })
    } catch (e) {
      deploys.value[w.slug] = {
        ...w,
        sha: '',
        phase: 'failed',
        failure: 'untracked',
        elapsedSecs: 0,
        runUrl: ACTIONS_URL,
        message: `It was pushed, but Dispatch couldn’t read the commit to follow the deploy (${e}). Check the site in a few minutes.`,
      }
      return
    }
    await trackDeploy(w, sha, 360)
  }

  /** Watch the push of `sha` until the site serves it. Re-entrant: "Check
   *  again" calls it with the same sha, replacing the failed state. */
  async function trackDeploy(w: Watch, sha: string, timeoutSecs: number) {
    const slug = w.slug
    clearFailureToast(slug)
    deploys.value[slug] = {
      slug,
      url: w.url,
      title: w.title,
      targetId: w.targetId,
      isRepublish: w.isRepublish,
      webmentions: w.webmentions,
      hiddenUntil: w.hiddenUntil,
      sha,
      phase: 'deploying',
      elapsedSecs: 0,
      runUrl: deploys.value[slug]?.sha === sha ? deploys.value[slug].runUrl : ACTIONS_URL,
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
        postUrl: w.url,
        timeoutSecs,
        targetId: w.targetId || null,
        title: w.title,
        skipUrlCheck: options.isPasswordProtected(),
        hiddenUntil: w.hiddenUntil,
      })
    } catch (e) {
      result = {
        sha,
        phase: 'timeout',
        ok: false,
        message: `Dispatch lost track of the deploy: ${e}`,
        deployed_commit: null,
        url_status: null,
        run_url: ACTIONS_URL,
      }
    } finally {
      unlisten?.()
    }

    const d = deploys.value[slug]
    // A newer publish of the same slug replaced this one — let it report.
    if (!d || d.sha !== sha || d.phase !== 'deploying') return
    d.runUrl = result.run_url || ACTIONS_URL
    d.message = result.message
    if (result.ok && result.phase === 'hidden') {
      d.phase = 'hidden'
      playSound('Glass')
      toasts.success(`“${w.title}” is deployed`, w.hiddenUntil ? `It appears ${w.hiddenUntil}` : undefined)
    } else if (result.ok) {
      d.phase = 'live'
      playSound('Glass')
      announceLive(w)
      if (slug === options.getFilePath()) {
        justPublishedGlow.value = true
        setTimeout(() => {
          justPublishedGlow.value = false
        }, 1500)
      }
      options.onLive?.(w.url, { webmentions: w.webmentions })
    } else {
      d.phase = 'failed'
      d.failure = FAILURE_OF[result.phase] ?? 'timeout'
      playSound('Sosumi')
      const id: number = (failureToast[slug] = toasts.push({
        kind: 'error',
        message: `${deployFailureTitle(d.failure)}: “${w.title}”`,
        detail: result.message,
        action:
          d.failure === 'run'
            ? { label: 'View run', run: () => void window.open(d.runUrl, '_blank') }
            : {
                label: 'Check again',
                run: () => {
                  toasts.dismiss(id)
                  void recheckDeploy(slug)
                },
              },
        ttl: 0,
      }))
    }
  }

  // One failure toast per post: a re-check or a dismiss in the inspector
  // takes the stale one with it.
  const failureToast: Record<string, number> = {}
  function clearFailureToast(slug: string) {
    if (failureToast[slug] !== undefined) toasts.dismiss(failureToast[slug])
    delete failureToast[slug]
  }

  /** The it's-live moment: which post, where, and the link one click away.
   *  Shown whichever post is selected now; the deploy took minutes. */
  function announceLive(w: Watch) {
    const visibility = options.isPasswordProtected() ? ' (protected)' : options.isUnlisted() ? ' (unlisted)' : ''
    const id: number = toasts.push({
      kind: 'success',
      message: w.isRepublish ? `“${w.title}” is updated${visibility}` : `“${w.title}” is live${visibility}`,
      detail: w.url.replace(/^https?:\/\//, ''),
      action: {
        label: 'Copy link',
        run: async () => {
          await navigator.clipboard.writeText(w.url).catch(() => {})
          toasts.dismiss(id)
          toasts.success('Copied link', w.url)
        },
      },
      ttl: 8000,
    })
  }

  /** "Check again" on a deploy that timed out or whose page didn't answer:
   *  watch the same commit again, briefly (it's usually already there). */
  async function recheckDeploy(slug: string) {
    const d = deploys.value[slug]
    if (!d || !d.sha || d.phase === 'deploying') return
    await trackDeploy(d, d.sha, 180)
  }

  function dismissDeploy(slug: string) {
    clearFailureToast(slug)
    delete deploys.value[slug]
  }

  /** Confirmed from the review sheet in `unlisted` mode: write
   *  `unlisted: true`, then publish. */
  async function publishUnlisted(
    isRepublish = false,
    opts: { webmentions?: boolean; hiddenUntil?: string | null } = {},
  ) {
    if (!isRepublish && !options.getFileIsSafe()) return
    try {
      await invoke('set_frontmatter', { path: options.getFilePath(), key: 'unlisted', value: 'true' })
    } catch (e) {
      toasts.error('Failed to set unlisted', String(e))
      return
    }
    await publish(isRepublish, opts)
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
    recheckDeploy,
    showSuccess,
    successMessage,
    showPublishConfirm,
    publishConfirmRepublish,
    publishConfirmMode,
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
      options.showSuccessToast('Unpublished: it\'ll come down in about 3 minutes')
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
