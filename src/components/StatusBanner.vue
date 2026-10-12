<script setup lang="ts">
/**
 * The inspector's Status section: a state dot, one word, and (when live) the
 * URL on one truncated line with a copy button. No tinted wash, no colored
 * bar — the dot carries the color, the same tokens the file list uses.
 *
 *   ● Live · Changed since live · Deploying… 1m20s · Deployed, appears …
 *   · Deploy failed / Not live yet / page isn’t loading (each says what to
 *   do, with View run / Check again) · Scheduled for … · Not published
 *
 * Diff toggling is owned by the parent (`diffOpen` + `toggle-diff`) because
 * the diff itself renders above the post, where there's room for it.
 */
import { computed } from 'vue'
import { CHANGED_LABEL, CHANGED_HINT, NOT_PUBLISHED_LABEL } from '../utils/vocab'
import { PhCopy, PhLockSimple } from '@phosphor-icons/vue'
import { formatScheduledTime } from '../utils/formatting'
import UiButton from './ui/UiButton.vue'
import { deployFailureTitle, type DeployState } from '../composables/usePublishFlow'

const props = defineProps<{
  isLive: boolean
  isScheduled: boolean
  isUnlisted: boolean
  isPasswordProtected: boolean
  hasUnpublishedChanges: boolean
  isSafe: boolean
  warnings: string[]
  liveUrl: string | null
  publishAt: string | null
  visibilityLabel: string | null
  publishing: boolean
  /** Path to the markdown file (identifies the post for the diff). */
  filePath: string
  /** Post-push deploy tracking for this post (null when none in flight). */
  deploy?: DeployState | null
  /** Whether the parent is showing the since-last-publish diff. */
  diffOpen?: boolean
}>()

defineEmits<{
  'copy-url': []
  'copy-url-password': []
  republish: []
  'cancel-schedule': []
  'dismiss-deploy': []
  'recheck-deploy': []
  'toggle-diff': []
}>()

function openExternal(url: string) {
  window.open(url, '_blank')
}

function fmtElapsed(secs: number) {
  return secs < 60 ? `${secs}s` : `${Math.floor(secs / 60)}m${String(secs % 60).padStart(2, '0')}s`
}

type Dot = 'live' | 'attention' | 'broken' | 'scheduled' | 'draft' | 'deploying'

const state = computed<{ dot: Dot; word: string; hint?: string }>(() => {
  const d = props.deploy
  if (d?.phase === 'deploying') {
    const elapsed = d.elapsedSecs ? ` ${fmtElapsed(d.elapsedSecs)}` : ''
    return { dot: 'deploying', word: `Deploying…${elapsed}` }
  }
  if (d?.phase === 'hidden') return { dot: 'scheduled', word: `Deployed, appears ${d.hiddenUntil ?? 'later'}` }
  if (d?.phase === 'failed') {
    // Still deploying, maybe: amber. A failed run or a dead page: red.
    const dot = d.failure === 'timeout' || d.failure === 'untracked' ? 'attention' : 'broken'
    return { dot, word: deployFailureTitle(d.failure) }
  }
  if (props.isLive && props.hasUnpublishedChanges) return { dot: 'attention', word: CHANGED_LABEL, hint: CHANGED_HINT }
  if (props.isLive) return { dot: 'live', word: 'Live' }
  if (props.isScheduled) return { dot: 'scheduled', word: `Scheduled for ${formatScheduledTime(props.publishAt)}` }
  return { dot: 'draft', word: NOT_PUBLISHED_LABEL }
})

/** What to do about a deploy that didn't end live. */
const nextStep = computed(() => {
  switch (props.deploy?.failure) {
    case 'run':
      return 'Your post is pushed, so the next deploy that succeeds includes it. The run shows what broke.'
    case 'timeout':
      return 'Check again in a minute, or open the run.'
    case 'unverified':
      return 'Open the page to see what it shows, then check again.'
    default:
      return ''
  }
})
const canRecheck = computed(() => props.deploy?.phase === 'failed' && !!props.deploy.sha && props.deploy.failure !== 'run')

// The URL reads better without its scheme; the full thing is in the title.
const displayUrl = computed(() => (props.liveUrl || '').replace(/^https?:\/\//, ''))
</script>

<template>
  <div class="status">
    <div class="status-line">
      <span class="status-dot" :data-status="state.dot" aria-hidden="true"></span>
      <span
        class="status-word"
        :class="{ wraps: state.dot === 'scheduled' || deploy?.phase === 'failed' }"
        :title="state.hint"
      >
        {{ state.word }}
      </span>
    </div>

    <!-- Pushed, not yet serving: which commit, and where to watch it. -->
    <div v-if="deploy?.phase === 'deploying'" class="insp-note status-note">
      pushed {{ deploy.sha.slice(0, 7) }} ·
      <a :href="deploy.runUrl" target="_blank" rel="noopener">Actions</a>
    </div>
    <div v-else-if="deploy?.phase === 'hidden'" class="insp-note status-note">
      The site keeps it hidden until then. No webmentions were sent.
    </div>
    <!-- Didn't end live: what happened, what to do, and the buttons for it. -->
    <template v-else-if="deploy?.phase === 'failed'">
      <div class="insp-note status-note">{{ deploy.message }} {{ nextStep }}</div>
      <div class="status-line status-actions">
        <UiButton v-if="canRecheck" size="small" label="Check again" @click="$emit('recheck-deploy')" />
        <UiButton
          v-if="deploy.failure === 'unverified'"
          size="small"
          variant="plain"
          label="Open page"
          @click="openExternal(deploy.url)"
        />
        <UiButton size="small" variant="plain" label="View run" @click="openExternal(deploy.runUrl)" />
        <UiButton size="small" variant="plain" label="Dismiss" @click="$emit('dismiss-deploy')" />
      </div>
    </template>

    <!-- Scheduled: the time is on the status line; this undoes it. -->
    <div v-if="isScheduled && !isLive" class="status-line">
      <UiButton
        size="small"
        variant="plain"
        class="status-plain"
        label="Cancel"
        title="Don’t publish it later"
        @click="$emit('cancel-schedule')"
      />
    </div>

    <div v-if="isLive && liveUrl" class="status-url">
      <a :href="liveUrl" target="_blank" rel="noopener" class="status-url-text" :title="liveUrl">{{ displayUrl }}</a>
      <UiButton size="small" variant="plain" :icon="PhCopy" aria-label="Copy link" @click="$emit('copy-url')" />
      <UiButton
        v-if="isPasswordProtected"
        size="small"
        variant="plain"
        :icon="PhLockSimple"
        aria-label="Copy link and password"
        @click="$emit('copy-url-password')"
      />
    </div>

    <!-- The vault copy differs from the site: offer the diff. The republish
         itself is the primary action right below. -->
    <div v-if="isLive && hasUnpublishedChanges" class="status-line">
      <UiButton
        size="small"
        variant="plain"
        class="see-changes status-plain"
        :label="diffOpen ? 'Hide changes' : 'See changes'"
        :title="diffOpen ? 'Hide the diff' : 'Show what changed since the last publish'"
        @click="$emit('toggle-diff')"
      />
    </div>
  </div>
</template>

<style scoped>
.status {
  display: flex;
  flex-direction: column;
  gap: var(--space-1);
  min-width: 0;
}

.status-line {
  display: flex;
  align-items: center;
  gap: var(--space-2);
  min-height: 22px;
  min-width: 0;
}

.status-word {
  flex: 1;
  min-width: 0;
  font-size: 12px;
  font-weight: 600;
  line-height: 16px;
  color: var(--label);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}
/* "Scheduled for Oct 14, 2026 at 9:00 AM" is the status: let it wrap. */
.status-word.wraps {
  white-space: normal;
}

.status-actions {
  flex-wrap: wrap;
  gap: var(--space-1);
}

.status-note a {
  color: var(--label-2);
  text-decoration: none;
  font-size: 11px;
  flex-shrink: 0;
}
.status-note a:hover {
  color: var(--label);
  text-decoration: underline;
}

.status-note {
  font-variant-numeric: tabular-nums;
  overflow-wrap: anywhere;
}

.status-url {
  display: flex;
  align-items: center;
  gap: var(--space-1);
  min-width: 0;
}
.status-url-text {
  flex: 1;
  min-width: 0;
  font-size: 11px;
  font-family: 'SF Mono', 'Menlo', monospace;
  color: var(--label-2);
  text-decoration: none;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}
.status-url-text:hover {
  color: var(--label);
  text-decoration: underline;
}

/* A plain small button's text sits flush with the dot and the URL. */
.status-plain {
  margin-left: calc(-1 * var(--space-2));
}
</style>
