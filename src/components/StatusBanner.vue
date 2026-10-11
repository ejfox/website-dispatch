<script setup lang="ts">
import { CHANGED_LABEL, CHANGED_HINT } from '../utils/vocab'
import { ref, watch } from 'vue'
import {
  PhLockSimple,
  PhEye,
  PhCheckCircle,
  PhClock,
  PhCaretDown,
  PhWarningCircle,
} from '@phosphor-icons/vue'
import { formatScheduledTime } from '../utils/formatting'
import PublishDiffPanel from './PublishDiffPanel.vue'
import type { DeployState } from '../composables/usePublishFlow'

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
  /** Path to the markdown file — needed to fetch the diff. */
  filePath: string
  /** Post-push deploy tracking for this post (null when none in flight). */
  deploy?: DeployState | null
}>()

defineEmits<{
  'copy-url': []
  'copy-url-password': []
  republish: []
  'cancel-schedule': []
  'dismiss-deploy': []
}>()

function fmtElapsed(secs: number) {
  return secs < 60 ? `${secs}s` : `${Math.floor(secs / 60)}m${String(secs % 60).padStart(2, '0')}s`
}

const diffOpen = ref(false)

// Reset the panel state when the user switches to a different file so the
// next "modified" file doesn't open with stale diff state.
watch(
  () => props.filePath,
  () => {
    diffOpen.value = false
  },
)
</script>

<template>
  <!-- Pushed, not yet serving: quiet and persistent until the site confirms. -->
  <div v-if="deploy?.phase === 'deploying'" class="banner deploying">
    <span class="banner-text">
      <span class="status-dot" data-status="deploying" aria-hidden="true"></span>
      DEPLOYING…
    </span>
    <span class="deploy-msg">
      pushed {{ deploy.sha.slice(0, 7) }}
      <template v-if="deploy.elapsedSecs">· {{ fmtElapsed(deploy.elapsedSecs) }}</template>
    </span>
    <a :href="deploy.runUrl" target="_blank">Actions</a>
  </div>
  <div v-else-if="deploy?.phase === 'failed'" class="banner deploy-failed">
    <span class="banner-text">
      <PhWarningCircle :size="13" weight="bold" />
      NOT LIVE
    </span>
    <span class="deploy-msg">{{ deploy.message }}</span>
    <a :href="deploy.runUrl" target="_blank">View run</a>
    <button @click="$emit('dismiss-deploy')">Dismiss</button>
  </div>
  <div v-else-if="isLive && hasUnpublishedChanges">
    <div class="banner modified">
      <span class="banner-text" :data-tip="CHANGED_HINT">{{ CHANGED_LABEL.toUpperCase() }}</span>
      <span v-if="visibilityLabel" class="visibility-badge">{{ visibilityLabel }}</span>
      <span class="modified-msg">{{ CHANGED_HINT }}</span>
      <button
        class="see-changes"
        :class="{ open: diffOpen }"
        @click="diffOpen = !diffOpen"
        :data-tip="diffOpen ? 'Hide diff' : 'Show what changed since last publish'"
      >
        <PhCaretDown :size="9" weight="bold" class="see-caret" />
        {{ diffOpen ? 'Hide changes' : 'See changes' }}
      </button>
      <button class="republish-btn" @click="$emit('republish')" :disabled="publishing">
        {{ publishing ? '...' : 'Republish' }}
      </button>
    </div>
    <PublishDiffPanel :file-path="filePath" :open="diffOpen" @close="diffOpen = false" />
  </div>
  <div v-else-if="isLive && isPasswordProtected" class="banner protected">
    <span class="banner-text">
      <PhLockSimple :size="13" weight="bold" />
      PROTECTED
    </span>
    <a :href="liveUrl!" target="_blank">{{ liveUrl }}</a>
    <button @click="$emit('copy-url-password')" data-tip="Copy URL + password to share">Copy + Pass</button>
    <button @click="$emit('copy-url')" data-tip="Copy URL only">Copy URL</button>
  </div>
  <div v-else-if="isLive && isUnlisted" class="banner unlisted">
    <span class="banner-text">
      <PhEye :size="13" weight="bold" />
      UNLISTED
    </span>
    <a :href="liveUrl!" target="_blank">{{ liveUrl }}</a>
    <button @click="$emit('copy-url')" data-tip="Share this link — not indexed anywhere">Copy</button>
  </div>
  <div v-else-if="isLive" class="banner live">
    <span class="banner-text">
      <PhCheckCircle :size="13" weight="fill" />
      {{ deploy?.phase === 'live' ? 'LIVE ✓' : 'LIVE' }}
    </span>
    <a :href="liveUrl!" target="_blank">{{ liveUrl }}</a>
    <a v-if="deploy?.phase === 'live'" :href="liveUrl!" target="_blank" class="open-link">Open</a>
    <button @click="$emit('copy-url')">Copy</button>
  </div>
  <div v-else-if="isScheduled" class="banner scheduled">
    <span class="banner-text">
      <PhClock :size="13" weight="bold" />
      PUBLISHES LATER
    </span>
    <span data-tip="Dispatch publishes it then, if Dispatch is running">{{ formatScheduledTime(publishAt!) }}</span>
    <button @click="$emit('cancel-schedule')" data-tip="Don’t publish it later">Cancel</button>
  </div>
  <div v-else-if="!isSafe" class="banner warn">
    {{ warnings.join(' · ') }}
  </div>
  <div v-else-if="isPasswordProtected" class="banner ready protected-ready">
    <span class="visibility-badge">
      <PhLockSimple :size="12" weight="bold" />
      PASSWORD
    </span>
    <span class="visibility-hint">Link + password required to view</span>
  </div>
  <div v-else-if="isUnlisted" class="banner ready unlisted-ready">
    <span class="visibility-badge">
      <PhEye :size="12" weight="bold" />
      UNLISTED
    </span>
    <span class="visibility-hint">Link only — won't appear in listings or feeds</span>
  </div>
  <div v-else class="banner ready public-ready">
    <span class="visibility-badge">
      <PhCheckCircle :size="10" weight="fill" />
      PUBLIC
    </span>
    <span class="visibility-hint">Will appear in listings, feeds, and search</span>
  </div>
</template>

<style scoped>
/* Status banners — one recipe for every state: a 3px stripe and a 12%
   tint in the state's status color, the glyph in that color, and the text
   in neutral labels (status hues fail contrast as small text). Set --st per
   state and everything else follows.
     live (incl. unlisted/password) green · modified orange · failed and
     blocked red · scheduled purple · deploying neutral + pulsing accent dot
   Unlisted/password are visibility, not status: those banners are live
   green, and the eye/lock glyph stays neutral. */
.banner {
  --st: transparent;
  padding: 6px 16px 6px 13px;
  font-size: 11px;
  display: flex;
  align-items: center;
  gap: 10px;
  transition: background 0.2s ease;
  color: var(--label);
  background: color-mix(in srgb, var(--st) 12%, transparent);
  border-left: 3px solid var(--st);
}

.banner-text {
  display: flex;
  align-items: center;
  gap: 5px;
  font-weight: 600;
  letter-spacing: 0.02em;
}
.banner-text svg {
  color: var(--st);
}

.banner a {
  color: var(--label-2);
  text-decoration: none;
}
.banner a:hover {
  color: var(--label);
  text-decoration: underline;
}

/* Secondary buttons inside a banner are neutral. The one primary action
   (Republish) is accent. */
.banner button {
  background: var(--fill-2);
  border: none;
  color: var(--label);
  padding: 2px 8px;
  border-radius: 4px;
  font-size: 10px;
  cursor: pointer;
  font-family: inherit;
  transition: background 0.12s;
}
.banner button:hover:not(:disabled) {
  background: color-mix(in srgb, var(--label) 18%, transparent);
}

.banner.live,
.banner.unlisted,
.banner.protected {
  --st: var(--status-live);
}
.banner.live a,
.banner.unlisted a,
.banner.protected a {
  flex: 1;
}
.banner.live a.open-link {
  flex: 0;
  color: var(--accent);
}
.banner.unlisted .banner-text svg,
.banner.protected .banner-text svg {
  color: var(--label-2);
}
.banner.unlisted button,
.banner.protected button {
  margin-left: 4px;
}

/* DEPLOYING — pushed, waiting on GitHub Actions. Neutral banner; the only
   color is the pulsing accent dot (static under reduced motion). */
.banner.deploying {
  --st: var(--label-3);
  color: var(--label-2);
}
.banner.deploying .banner-text {
  color: var(--label);
}
.banner.deploying .deploy-msg,
.banner.deploy-failed .deploy-msg {
  flex: 1;
  font-variant-numeric: tabular-nums;
  color: var(--label-2);
}
.banner.deploy-failed {
  --st: var(--status-broken);
}

.banner.scheduled {
  --st: var(--status-scheduled);
}

/* Not safe to publish (privacy lint) — broken. */
.banner.warn {
  --st: var(--status-broken);
}

.banner.modified {
  --st: var(--status-attention);
}
.banner.modified .modified-msg {
  flex: 1;
  color: var(--label-2);
  font-weight: 400;
}
.banner.modified button {
  padding: 3px 9px;
  font-weight: 500;
  display: inline-flex;
  align-items: center;
  gap: 4px;
}
.banner.modified .see-changes .see-caret {
  transition: transform 0.15s;
}
.banner.modified .see-changes.open .see-caret {
  transform: rotate(180deg);
}
/* The screen's primary action: accent, with system accent-text. */
.banner.modified .republish-btn {
  background: var(--accent);
  color: var(--on-accent);
  font-weight: 600;
}
.banner.modified .republish-btn:hover:not(:disabled) {
  filter: brightness(1.1);
}

.banner.ready {
  background: var(--fill);
  color: var(--label-2);
}

.visibility-badge {
  font-size: 8px;
  font-weight: 700;
  padding: 2px 5px;
  border-radius: 3px;
  background: var(--fill-2);
  color: var(--label-2);
  display: inline-flex;
  align-items: center;
  gap: 3px;
}

.visibility-hint {
  font-size: 10px;
  color: var(--text-tertiary);
  margin-left: auto;
}
</style>
