<script setup lang="ts">
import { PhLockSimple, PhEye, PhCheckCircle, PhClock } from '@phosphor-icons/vue'
import { formatScheduledTime } from '../utils/formatting'

defineProps<{
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
}>()

defineEmits<{
  'copy-url': []
  'copy-url-password': []
  republish: []
  'cancel-schedule': []
}>()
</script>

<template>
  <div v-if="isLive && hasUnpublishedChanges" class="banner modified">
    <span class="banner-text">MODIFIED</span>
    <span v-if="visibilityLabel" class="visibility-badge">{{ visibilityLabel }}</span>
    <span>Source changed since last publish</span>
    <button @click="$emit('republish')" :disabled="publishing">{{ publishing ? '...' : 'Republish' }}</button>
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
.banner {
  padding: 8px 16px;
  font-size: 11px;
  display: flex;
  align-items: center;
  gap: 12px;
  transition: all 0.2s ease;
}

.banner-text {
  display: flex;
  align-items: center;
  gap: 5px;
}

.banner.live {
  background: var(--success);
  color: #000;
}

.banner.live .banner-text {
  font-weight: 700;
}

.banner.live a {
  color: #000;
  opacity: 0.8;
  text-decoration: none;
  flex: 1;
}

.banner.live a:hover {
  opacity: 1;
  text-decoration: underline;
}

.banner.live button {
  background: rgba(0, 0, 0, 0.2);
  border: none;
  color: #000;
  padding: 2px 8px;
  border-radius: 3px;
  font-size: 10px;
  cursor: pointer;
}

.banner.scheduled {
  background: rgba(255, 159, 10, 0.15);
  color: var(--warning);
}

.banner.scheduled button {
  background: rgba(255, 159, 10, 0.2);
  border: none;
  color: var(--warning);
  padding: 2px 8px;
  border-radius: 3px;
  font-size: 10px;
  cursor: pointer;
}

.banner.scheduled button:hover {
  background: rgba(255, 159, 10, 0.35);
}

.banner.warn {
  background: rgba(255, 159, 10, 0.15);
  color: var(--warning);
}

.banner.modified {
  background: var(--warning);
  color: #000;
}

.banner.modified .banner-text {
  font-weight: 700;
}

.banner.modified button {
  background: rgba(0, 0, 0, 0.2);
  border: none;
  color: #000;
  padding: 2px 8px;
  border-radius: 3px;
  font-size: 10px;
  cursor: pointer;
  margin-left: auto;
}

.banner.ready {
  background: var(--accent);
  color: var(--text-secondary);
}

.banner.unlisted {
  background: #6366f1;
  color: #fff;
}

.banner.unlisted .banner-text {
  font-weight: 700;
}

.banner.unlisted a {
  color: rgba(255, 255, 255, 0.8);
  text-decoration: none;
  flex: 1;
}

.banner.unlisted a:hover {
  color: #fff;
  text-decoration: underline;
}

.banner.unlisted button {
  background: rgba(0, 0, 0, 0.2);
  border: none;
  color: #fff;
  padding: 2px 8px;
  border-radius: 3px;
  font-size: 10px;
  cursor: pointer;
}

.banner.protected {
  background: #8b5cf6;
  color: #fff;
}

.banner.protected .banner-text {
  font-weight: 700;
}

.banner.protected a {
  color: rgba(255, 255, 255, 0.8);
  text-decoration: none;
  flex: 1;
}

.banner.protected a:hover {
  color: #fff;
  text-decoration: underline;
}

.banner.protected button {
  background: rgba(0, 0, 0, 0.2);
  border: none;
  color: #fff;
  padding: 2px 8px;
  border-radius: 3px;
  font-size: 10px;
  cursor: pointer;
  margin-left: 4px;
}

.visibility-badge {
  font-size: 8px;
  font-weight: 700;
  padding: 2px 5px;
  border-radius: 3px;
  background: var(--accent);
  display: inline-flex;
  align-items: center;
  gap: 3px;
}

.unlisted-ready .visibility-badge {
  background: #6366f1;
  color: #fff;
}

.protected-ready .visibility-badge {
  background: #8b5cf6;
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
