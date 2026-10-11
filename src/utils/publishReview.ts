// Turns a `publish_preview` result into the plain sentences the review sheet
// shows. Every line is derived from what publish will actually do, so the
// sheet can't drift from the pipeline. Ghost's confirm step is the model:
// one sentence that states the consequence, then a button that repeats it.

import type { PublishPreview, WhenField } from '../types'

export type ReviewMode = 'publish' | 'unlisted'

export interface Review {
  headline: string
  /** What happens, in one or two sentences. */
  summary: string[]
  /** Who can see it. Empty when the summary already said so. */
  visibility: string
  syndication: string
  webmentions: string
  /** False when webmentions can't fire for this publish (an embargoed post
   *  isn't reachable yet), so the toggle has nothing to control. */
  webmentionsApply: boolean
  undo: string
  /** Lint issues that don't block (an update), as sentences. */
  notes: string[]
  /** Small grey git detail. */
  git: string
  /** Why it can't go ahead, or null. */
  blocker: string | null
  /** Lint issues that block a first publish, as sentences. */
  blockerList: string[]
  primaryLabel: string
}

const NETWORK_NAMES: Record<string, string> = {
  bluesky: 'Bluesky',
  mastodon: 'Mastodon',
  linkedin: 'LinkedIn',
  instagram: 'Instagram',
}

export function networkName(n: string): string {
  return NETWORK_NAMES[n] ?? n
}

/** "A", "A and B", "A, B and C". */
export function listJoin(items: string[]): string {
  if (items.length <= 1) return items[0] ?? ''
  return `${items.slice(0, -1).join(', ')} and ${items[items.length - 1]}`
}

export function plainUrl(url: string): string {
  return url.replace(/^https?:\/\//, '')
}

/** "Tue, Oct 14, 9:00 AM EDT" in this Mac's zone, or the raw value when the
 *  frontmatter gave no zone. */
export function formatWhen(w: WhenField): string {
  if (!w.utc) return `"${w.raw}" (no time zone given, so the server's clock decides)`
  return new Intl.DateTimeFormat(undefined, {
    weekday: 'short',
    month: 'short',
    day: 'numeric',
    hour: 'numeric',
    minute: '2-digit',
    timeZoneName: 'short',
  }).format(new Date(w.utc))
}

function minutes(secs: number): number {
  return Math.max(1, Math.round(secs / 60))
}

/** Lint warnings from vault.rs, as sentences. "Modified since publish" isn't
 *  a lint issue (it's why you'd update), so it's dropped. */
export function lintSentences(warnings: string[]): string[] {
  return warnings
    .filter((w) => w !== 'Modified since publish')
    .map((w) => {
      const alt = w.match(/^Missing alt text \((\d+)\)$/)
      if (alt) {
        const n = Number(alt[1])
        return n === 1 ? 'An image has no alt text.' : `${n} images have no alt text.`
      }
      if (w.startsWith('[privacy]')) return `Privacy: ${w.slice(9).trim()}`
      switch (w) {
        case 'No date':
          return 'It has no date in its frontmatter.'
        case 'Has TODOs':
          return 'It still has a TODO or FIXME in the text.'
        case 'Local images':
          return 'It points at images on this Mac, which won’t load on the site.'
        case 'Local media':
          return 'It links media that only exists on this Mac.'
        case 'Local video':
          return 'It links a video that only exists on this Mac.'
        case 'Broken link':
          return 'It has an empty or broken link.'
        case 'Long link text':
          return 'A link’s text is longer than four words.'
        default:
          return /[.!?]$/.test(w) ? w : `${w}.`
      }
    })
}

function syndicationLine(p: PublishPreview): string {
  const queued = p.syndication.filter((s) => s.mode === 'queued')
  const ask = p.syndication.filter((s) => s.mode === 'ask')
  const parts: string[] = []
  if (queued.length) {
    const names = listJoin([...new Set(queued.map((s) => networkName(s.network)))])
    parts.push(`It will also be posted to ${names}: that’s already queued, and it can’t be undone.`)
  }
  if (ask.length) {
    const names = listJoin(ask.map((s) => networkName(s.network)))
    parts.push(
      `${queued.length ? 'Beyond that, nothing' : 'Nothing'} is posted on its own: after the push, Dispatch opens the share sheet for ${names} so you can decide.`,
    )
  }
  if (!parts.length) parts.push('It won’t be posted to Bluesky or Mastodon.')
  return parts.join(' ')
}

function webmentionLine(p: PublishPreview, send: boolean, visibility: PublishPreview['visibility']): string {
  if (!p.webmentions_after_live) return 'No webmentions will be sent.'
  if (!send) return 'No webmentions will be sent.'
  const fedi = p.bridgy_fed ? ', and forwards them to the fediverse through Bridgy Fed' : ''
  const leak =
    visibility === 'public' ? '' : ' Those sites will see the link even though the post isn’t public.'
  return `Once it’s live, Dispatch tells the sites it links to (webmentions)${fedi}. That can’t be undone.${leak}`
}

export function composeReview(
  p: PublishPreview,
  opts: { mode?: ReviewMode; webmentions?: boolean; warnings?: string[] } = {},
): Review {
  const mode = opts.mode ?? 'publish'
  const send = opts.webmentions ?? true
  const where = plainUrl(p.url)
  const title = p.title || p.slug
  const mins = minutes(p.deploy_estimate_secs)
  // The Unlisted button writes `unlisted: true` before publishing; a password
  // still wins on the site.
  const visibility = mode === 'unlisted' && p.visibility === 'public' ? 'unlisted' : p.visibility
  const lint = lintSentences(opts.warnings ?? [])
  const now = Date.now()
  const future = (w: WhenField | null) => !!w && (!w.utc || Date.parse(w.utc) > now)

  const summary: string[] = []
  if (mode === 'unlisted') {
    summary.push(
      p.is_update
        ? `“${title}” will be republished as unlisted at ${where}: only people with the link can find it. The URL stays the same.`
        : `“${title}” will be published as unlisted at ${where}: only people with the link can find it.`,
    )
  } else if (p.is_update) {
    summary.push(`“${title}” replaces the live version at ${where}. The URL stays the same.`)
  } else {
    summary.push(`“${title}” will be published at ${where}.`)
  }
  const embargoed = future(p.embargo)
  if (embargoed) {
    const midnight = p.embargo!.date_only ? ' (a date with no time counts as midnight UTC)' : ''
    summary.push(
      `It’ll be committed to ${p.repo_name} on ${p.branch} and deployed in about ${mins} minutes.`,
      `The site deploys now, but the post stays hidden until ${formatWhen(p.embargo!)}${midnight}.`,
    )
  } else {
    summary.push(
      `It’ll be committed to ${p.repo_name} on ${p.branch} and go live in about ${mins} minutes; Dispatch will tell you when it’s live.`,
    )
  }
  if (future(p.dispatch_schedule)) {
    summary.push(
      `Dispatch was set to publish this on ${formatWhen(p.dispatch_schedule!)}; publishing now puts it live now instead.`,
    )
  }

  const visibilityText =
    mode === 'unlisted' && visibility === 'unlisted'
      ? ''
      : visibility === 'password'
        ? 'Password-protected.'
        : visibility === 'unlisted'
          ? 'Unlisted: only people with the link.'
          : 'Anyone can find it.'

  const others = p.other_uncommitted
  const git =
    `commit ${p.rel_path} to ${p.repo_name} on ${p.branch}` +
    (others
      ? ` · ${others} other uncommitted file${others === 1 ? '' : 's'} in ${p.repo_name} stay${others === 1 ? 's' : ''} out of this commit`
      : '')

  // Lint gates a first publish only; an update has always gone through.
  let blocker = p.blocked?.message ?? null
  let blockerList: string[] = []
  if (!blocker && !p.is_update && lint.length) {
    blocker = `Fix ${lint.length === 1 ? 'this' : 'these'} in Obsidian first, then come back:`
    blockerList = lint
  }

  const primaryLabel = future(p.embargo)
    ? 'Schedule'
    : p.is_update
      ? mode === 'unlisted'
        ? 'Update as unlisted'
        : 'Update live post'
      : mode === 'unlisted'
        ? 'Publish unlisted'
        : 'Publish now'

  const headline = blocker
    ? p.is_update
      ? 'Can’t update yet'
      : 'Can’t publish yet'
    : future(p.embargo)
      ? 'Ready to schedule'
      : p.is_update
        ? 'Ready to update'
        : 'Ready to publish'

  return {
    headline,
    summary,
    visibility: visibilityText,
    syndication: syndicationLine(p),
    // The deploy watcher only calls a post live when its URL answers 200, and
    // an embargoed post 404s until its time, so nothing is sent for it.
    webmentions: embargoed
      ? 'No webmentions go out on their own: the page is hidden until then. Resend them from the post once it appears.'
      : webmentionLine(p, send, visibility),
    webmentionsApply: !embargoed && p.webmentions_after_live,
    undo: `You can unpublish later; that deploys again (~${mins} min).`,
    notes: p.is_update ? lint : [],
    git,
    blocker,
    blockerList,
    primaryLabel,
  }
}
