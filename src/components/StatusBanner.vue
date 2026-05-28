<script setup lang="ts">
import { ref, watch } from 'vue'
import { PhLockSimple, PhEye, PhCheckCircle, PhClock, PhCaretDown } from '@phosphor-icons/vue'
import { formatScheduledTime } from '../utils/formatting'
import PublishDiffPanel from './PublishDiffPanel.vue'

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
}>()

defineEmits<{
  'copy-url': []
  'copy-url-password': []
  republish: []
  'cancel-schedule': []
}>()

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
  <div v-if="isLive && hasUnpublishedChanges">
    <div class="banner modified">
      <span class="banner-text">MODIFIED</span>
      <span v-if="visibilityLabel" class="visibility-badge">{{ visibilityLabel }}</span>
      <span class="modified-msg">Source changed since last publish</span>
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
      LIVE
    </span>
    <a :href="liveUrl!" target="_blank">{{ liveUrl }}</a>
    <button @click="$emit('copy-url')">Copy</button>
  </div>
  <div v-else-if="isScheduled" class="banner scheduled">
    <span class="banner-text">
      <PhClock :size="13" weight="bold" />
      SCHEDULED
    </span>
    <span>{{ formatScheduledTime(publishAt!) }}</span>
    <button @click="$emit('cancel-schedule')">Cancel</button>
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
/* Status banners — soft state callouts, not bright full-bleed signs.
   Each state uses a translucent tint of its semantic color with a 3px
   left stripe, matching Apple Mail's restrained "Time-Sensitive Update"
   chip language rather than a screaming colored bar. */
.banner {
  padding: 6px 16px 6px 13px;
  font-size: 11px;
  display: flex;
  align-items: center;
  gap: 10px;
  transition: background 0.2s ease;
  border-left: 3px solid transparent;
}

.banner-text {
  display: flex;
  align-items: center;
  gap: 5px;
  font-weight: 600;
  letter-spacing: 0.02em;
}

/* LIVE state — soft green tint, accent on the text, no black-on-bright. */
.banner.live {
  background: color-mix(in srgb, var(--success) 10%, transparent);
  color: var(--success);
  border-left-color: var(--success);
}
.banner.live a {
  color: var(--success);
  opacity: 0.78;
  text-decoration: none;
  flex: 1;
}
.banner.live a:hover {
  opacity: 1;
  text-decoration: underline;
}
.banner.live button {
  background: color-mix(in srgb, var(--success) 18%, transparent);
  border: none;
  color: var(--success);
  padding: 2px 8px;
  border-radius: 4px;
  font-size: 10px;
  cursor: pointer;
  font-family: inherit;
  transition: background 0.12s;
}
.banner.live button:hover {
  background: color-mix(in srgb, var(--success) 28%, transparent);
}

/* SCHEDULED state — already restrained; just normalize. */
.banner.scheduled {
  background: color-mix(in srgb, var(--warning) 10%, transparent);
  color: var(--warning);
  border-left-color: var(--warning);
}
.banner.scheduled button {
  background: color-mix(in srgb, var(--warning) 18%, transparent);
  border: none;
  color: var(--warning);
  padding: 2px 8px;
  border-radius: 4px;
  font-size: 10px;
  font-family: inherit;
  cursor: pointer;
  transition: background 0.12s;
}
.banner.scheduled button:hover {
  background: color-mix(in srgb, var(--warning) 28%, transparent);
}

.banner.warn {
  background: color-mix(in srgb, var(--warning) 10%, transparent);
  color: var(--warning);
  border-left-color: var(--warning);
}

/* MODIFIED state — the one users see most. Was the loudest bar; now a
   soft amber wash with amber text, in line with the other states. */
.banner.modified {
  background: color-mix(in srgb, var(--warning) 12%, transparent);
  color: var(--warning);
  border-left-color: var(--warning);
}
.banner.modified .modified-msg {
  flex: 1;
  color: color-mix(in srgb, var(--warning) 78%, var(--text-secondary));
  font-weight: 400;
}
.banner.modified button {
  background: color-mix(in srgb, var(--warning) 18%, transparent);
  border: none;
  color: var(--warning);
  padding: 3px 9px;
  border-radius: 4px;
  font-size: 10px;
  font-weight: 500;
  cursor: pointer;
  font-family: inherit;
  display: inline-flex;
  align-items: center;
  gap: 4px;
  transition: background 0.12s;
}
.banner.modified button:hover:not(:disabled) {
  background: color-mix(in srgb, var(--warning) 28%, transparent);
}
.banner.modified .see-changes .see-caret {
  transition: transform 0.15s;
}
.banner.modified .see-changes.open .see-caret {
  transform: rotate(180deg);
}
/* Primary action button on the modified banner — sits on the soft amber
   wash, so it gets a solid amber pill to read as "the action to take." */
.banner.modified .republish-btn {
  background: var(--warning);
  color: var(--bg-solid);
  font-weight: 600;
}
.banner.modified .republish-btn:hover:not(:disabled) {
  background: color-mix(in srgb, var(--warning) 88%, white);
}

.banner.ready {
  background: var(--hover-bg);
  color: var(--text-secondary);
}

/* UNLISTED + PROTECTED — same accent palette, soft wash treatment. */
.banner.unlisted,
.banner.protected {
  background: color-mix(in srgb, var(--accent) 12%, transparent);
  color: var(--accent);
  border-left-color: var(--accent);
}

.banner.unlisted a,
.banner.protected a {
  color: var(--accent);
  opacity: 0.78;
  text-decoration: none;
  flex: 1;
}

.banner.unlisted a:hover,
.banner.protected a:hover {
  opacity: 1;
  text-decoration: underline;
}

.banner.unlisted button,
.banner.protected button {
  background: color-mix(in srgb, var(--accent) 18%, transparent);
  border: none;
  color: var(--accent);
  padding: 2px 8px;
  border-radius: 4px;
  font-size: 10px;
  cursor: pointer;
  font-family: inherit;
  margin-left: 4px;
  transition: background 0.12s;
}
.banner.unlisted button:hover,
.banner.protected button:hover {
  background: color-mix(in srgb, var(--accent) 28%, transparent);
}

.visibility-badge {
  font-size: 8px;
  font-weight: 700;
  padding: 2px 5px;
  border-radius: 3px;
  background: var(--hover-bg);
  display: inline-flex;
  align-items: center;
  gap: 3px;
}

.unlisted-ready .visibility-badge {
  background: var(--accent);
  color: #fff;
}

.protected-ready .visibility-badge {
  background: var(--accent);
  color: #fff;
}

.public-ready .visibility-badge {
  background: var(--success);
  color: #000;
}

.visibility-hint {
  font-size: 10px;
  color: var(--text-tertiary);
  margin-left: auto;
}
</style>
