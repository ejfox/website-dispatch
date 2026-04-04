<script setup lang="ts">
import { ref, computed, watch } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { PhCheck, PhX, PhClock, PhPaperPlaneTilt, PhArrowRight, PhArrowLeft, PhImage } from '@phosphor-icons/vue'
import OgImagePicker from './OgImagePicker.vue'

interface NewQueueItem {
  post_slug: string
  post_title: string
  post_url: string
  platform: string
  platform_text: string
  media_url: string | null
  scheduled_at: string | null
}

const props = defineProps<{
  postUrl: string
  title: string
  slug: string
  dek: string | null
  tags: string[]
  contentType: string
  visibility: string
}>()

const emit = defineEmits<{ close: []; queued: [] }>()

// Wizard state
const step = ref(1)
const queuing = ref(false)
const error = ref<string | null>(null)
const queuedIds = ref<number[]>([])

// Step 1: Platform selection
const platforms = ref<Record<string, boolean>>({
  mastodon: true,
  linkedin: false,
  instagram: false,
})

const platformStatus = ref<Record<string, string>>({})

// Check which platforms are configured
async function checkPlatforms() {
  try {
    const result = await invoke<string>('verify_mastodon')
    platformStatus.value.mastodon = result
  } catch {
    platformStatus.value.mastodon = 'not configured'
  }
  // LinkedIn and Instagram checked via env vars on backend — for now just mark as coming soon
  platformStatus.value.linkedin = 'coming soon'
  platformStatus.value.instagram = 'coming soon'
}
checkPlatforms()

const selectedPlatforms = computed(() =>
  Object.entries(platforms.value)
    .filter(([, enabled]) => enabled)
    .map(([platform]) => platform)
)

// Step 2: Per-platform text
const CHAR_LIMITS: Record<string, number> = { mastodon: 500, linkedin: 3000, instagram: 2200 }

function defaultText(platform: string): string {
  const title = props.dek || props.title
  const url = props.postUrl
  const tags = props.tags
    .filter(t => !['post', 'weeknote', 'blog'].includes(t))
    .slice(0, 3)
    .map(t => `#${t.replace(/-/g, '')}`)
    .join(' ')

  switch (platform) {
    case 'mastodon':
      return [title, url, tags].filter(Boolean).join('\n\n')
    case 'linkedin':
      return `${title}\n\n${url}`
    case 'instagram':
      return `${title}\n\n${props.dek || ''}\n\nLink in bio\n\n${props.tags.slice(0, 10).map(t => `#${t.replace(/-/g, '')}`).join(' ')}`
    default:
      return `${title}\n\n${url}`
  }
}

const platformTexts = ref<Record<string, string>>({})
const activePlatformTab = ref('mastodon')

// Initialize texts when platforms change
watch(selectedPlatforms, (plats) => {
  for (const p of plats) {
    if (!platformTexts.value[p]) {
      platformTexts.value[p] = defaultText(p)
    }
  }
  if (!plats.includes(activePlatformTab.value) && plats.length > 0) {
    activePlatformTab.value = plats[0]
  }
}, { immediate: true })

// Step 3: Media
const promoImageUrl = ref<string | null>(null)

function onOgPicked(url: string) {
  promoImageUrl.value = url
}

// Step 4: Schedule
const DRIP_PRESETS: Record<string, Record<string, string>> = {
  now: { mastodon: 'now', linkedin: 'now', instagram: 'now' },
  drip: { mastodon: 'now', linkedin: '+2h', instagram: '+1d' },
  tomorrow: { mastodon: '+1d', linkedin: '+1d', instagram: '+2d' },
}

const schedulePreset = ref('drip')
const customSchedules = ref<Record<string, string>>({})

function getScheduledAt(platform: string): string | null {
  if (schedulePreset.value === 'custom') {
    return customSchedules.value[platform] || null
  }
  const preset = DRIP_PRESETS[schedulePreset.value]?.[platform] || 'now'
  if (preset === 'now') return new Date().toISOString()
  const match = preset.match(/^\+(\d+)(h|d)$/)
  if (match) {
    const amount = parseInt(match[1])
    const unit = match[2]
    const date = new Date()
    if (unit === 'h') date.setHours(date.getHours() + amount)
    if (unit === 'd') date.setDate(date.getDate() + amount)
    return date.toISOString()
  }
  return null
}

function formatSchedule(platform: string): string {
  const at = getScheduledAt(platform)
  if (!at) return 'Manual'
  const diff = new Date(at).getTime() - Date.now()
  if (diff < 60000) return 'Now'
  if (diff < 3600000) return `In ${Math.round(diff / 60000)}m`
  if (diff < 86400000) return `In ${Math.round(diff / 3600000)}h`
  return `In ${Math.round(diff / 86400000)}d`
}

// Queue all
async function queueAll() {
  queuing.value = true
  error.value = null
  try {
    const items: NewQueueItem[] = selectedPlatforms.value.map(platform => ({
      post_slug: props.slug,
      post_title: props.title,
      post_url: props.postUrl,
      platform,
      platform_text: platformTexts.value[platform] || defaultText(platform),
      media_url: promoImageUrl.value,
      scheduled_at: getScheduledAt(platform),
    }))

    const ids = await invoke<number[]>('queue_syndication', { items })
    queuedIds.value = ids
    step.value = 6 // success
    emit('queued')
  } catch (e) {
    error.value = `${e}`
  }
  queuing.value = false
}
</script>

<template>
  <div class="modal-overlay" @click.self="$emit('close')">
    <div class="modal">
      <!-- Header -->
      <div class="modal-header">
        <PhPaperPlaneTilt :size="16" weight="fill" class="header-icon" />
        <h2>Syndicate</h2>
        <span class="step-indicator">{{ step < 6 ? `Step ${step} of 5` : 'Done' }}</span>
        <button class="close-btn" @click="$emit('close')"><PhX :size="14" weight="bold" /></button>
      </div>

      <!-- Error -->
      <div v-if="error" class="error-banner">{{ error }}</div>

      <!-- Step 1: Platforms -->
      <div v-if="step === 1" class="step-content">
        <div class="step-title">Where do you want to share?</div>
        <div class="platform-list">
          <label v-for="(enabled, platform) in platforms" :key="platform" class="platform-option" :class="{ disabled: platformStatus[platform] === 'coming soon' }">
            <input
              type="checkbox"
              v-model="platforms[platform]"
              :disabled="platformStatus[platform] === 'coming soon'"
            />
            <span class="platform-name">{{ platform }}</span>
            <span v-if="platformStatus[platform] === 'coming soon'" class="platform-status soon">coming soon</span>
            <span v-else-if="platformStatus[platform] === 'not configured'" class="platform-status warn">not configured</span>
            <span v-else-if="platformStatus[platform]" class="platform-status ok">connected</span>
          </label>
        </div>
      </div>

      <!-- Step 2: Edit text -->
      <div v-if="step === 2" class="step-content">
        <div class="step-title">Edit post text</div>
        <div class="platform-tabs">
          <button
            v-for="p in selectedPlatforms"
            :key="p"
            class="tab-btn"
            :class="{ active: activePlatformTab === p }"
            @click="activePlatformTab = p"
          >{{ p }}</button>
        </div>
        <div class="text-editor">
          <textarea
            v-model="platformTexts[activePlatformTab]"
            class="platform-textarea"
            rows="6"
          />
          <div class="char-count" :class="{ over: (platformTexts[activePlatformTab] || '').length > CHAR_LIMITS[activePlatformTab] }">
            {{ (platformTexts[activePlatformTab] || '').length }} / {{ CHAR_LIMITS[activePlatformTab] }}
          </div>
        </div>
      </div>

      <!-- Step 3: Media — generative OG image picker -->
      <div v-if="step === 3" class="step-content">
        <div class="step-title">Pick an OG image</div>
        <OgImagePicker :slug="slug" @picked="onOgPicked" />
      </div>

      <!-- Step 4: Schedule -->
      <div v-if="step === 4" class="step-content">
        <div class="step-title">When to share?</div>
        <div class="schedule-presets">
          <label class="schedule-option" :class="{ active: schedulePreset === 'now' }">
            <input type="radio" v-model="schedulePreset" value="now" />
            <span class="schedule-label">All now</span>
            <span class="schedule-desc">Send to all platforms immediately</span>
          </label>
          <label class="schedule-option" :class="{ active: schedulePreset === 'drip' }">
            <input type="radio" v-model="schedulePreset" value="drip" />
            <span class="schedule-label">Drip</span>
            <span class="schedule-desc">Mastodon now → LinkedIn +2h → Instagram +1d</span>
          </label>
          <label class="schedule-option" :class="{ active: schedulePreset === 'tomorrow' }">
            <input type="radio" v-model="schedulePreset" value="tomorrow" />
            <span class="schedule-label">Tomorrow</span>
            <span class="schedule-desc">Everything goes out tomorrow</span>
          </label>
        </div>
      </div>

      <!-- Step 5: Review -->
      <div v-if="step === 5" class="step-content">
        <div class="step-title">Review & queue</div>
        <div class="review-list">
          <div v-for="p in selectedPlatforms" :key="p" class="review-card">
            <div class="review-header">
              <span class="review-platform">{{ p }}</span>
              <span class="review-schedule"><PhClock :size="10" /> {{ formatSchedule(p) }}</span>
            </div>
            <div class="review-text">{{ (platformTexts[p] || '').slice(0, 120) }}{{ (platformTexts[p] || '').length > 120 ? '...' : '' }}</div>
          </div>
        </div>
      </div>

      <!-- Step 6: Success -->
      <div v-if="step === 6" class="step-content success-step">
        <div class="success-icon"><PhCheck :size="32" weight="bold" /></div>
        <div class="success-title">Queued!</div>
        <div class="success-detail">{{ queuedIds.length }} post(s) scheduled for syndication</div>
      </div>

      <!-- Footer -->
      <div class="modal-footer">
        <button v-if="step > 1 && step < 6" class="btn secondary" @click="step--">
          <PhArrowLeft :size="12" /> Back
        </button>
        <div class="footer-spacer"></div>
        <button v-if="step < 5" class="btn primary" :disabled="step === 1 && selectedPlatforms.length === 0" @click="step++">
          Next <PhArrowRight :size="12" />
        </button>
        <button v-else-if="step === 5" class="btn primary" :disabled="queuing" @click="queueAll">
          {{ queuing ? 'Queuing...' : `Queue ${selectedPlatforms.length} post(s)` }}
        </button>
        <button v-else class="btn primary" @click="$emit('close')">Done</button>
      </div>
    </div>
  </div>
</template>

<style scoped>
.modal-overlay { position: fixed; inset: 0; background: rgba(0,0,0,0.6); backdrop-filter: blur(4px); z-index: 200; display: flex; align-items: center; justify-content: center; }
.modal { width: 480px; max-width: 95vw; max-height: 85vh; background: rgba(30,30,34,0.95); backdrop-filter: blur(24px) saturate(180%); border: 1px solid rgba(255,255,255,0.1); border-radius: 12px; display: flex; flex-direction: column; overflow: hidden; box-shadow: 0 24px 48px rgba(0,0,0,0.5); }
.modal-header { padding: 14px 16px; border-bottom: 1px solid rgba(255,255,255,0.08); display: flex; align-items: center; gap: 10px; }
.header-icon { color: #a78bfa; }
.modal-header h2 { font-size: 14px; font-weight: 600; margin: 0; flex: 1; }
.step-indicator { font-size: 10px; color: var(--text-tertiary); }
.close-btn { background: none; border: none; color: var(--text-tertiary); cursor: pointer; padding: 4px; border-radius: 4px; display: flex; }
.close-btn:hover { color: var(--text-primary); background: rgba(255,255,255,0.05); }
.error-banner { padding: 8px 16px; background: rgba(239,68,68,0.15); border-bottom: 1px solid rgba(239,68,68,0.2); color: #fca5a5; font-size: 11px; }

.step-content { padding: 16px; overflow-y: auto; flex: 1; }
.step-title { font-size: 12px; font-weight: 600; color: var(--text-secondary); margin-bottom: 12px; }

/* Step 1: Platforms */
.platform-list { display: flex; flex-direction: column; gap: 8px; }
.platform-option { display: flex; align-items: center; gap: 10px; padding: 10px 12px; background: rgba(255,255,255,0.03); border: 1px solid rgba(255,255,255,0.06); border-radius: 6px; cursor: pointer; font-size: 12px; }
.platform-option:hover { border-color: rgba(255,255,255,0.12); }
.platform-option.disabled { opacity: 0.4; cursor: not-allowed; }
.platform-name { font-weight: 500; text-transform: capitalize; flex: 1; }
.platform-status { font-size: 9px; padding: 2px 6px; border-radius: 4px; }
.platform-status.ok { background: rgba(34,197,94,0.15); color: #4ade80; }
.platform-status.warn { background: rgba(251,191,36,0.15); color: #fbbf24; }
.platform-status.soon { background: rgba(255,255,255,0.05); color: var(--text-tertiary); }

/* Step 2: Text editor */
.platform-tabs { display: flex; gap: 4px; margin-bottom: 8px; }
.tab-btn { padding: 4px 12px; font-size: 10px; font-family: inherit; background: rgba(255,255,255,0.04); color: var(--text-tertiary); border: 1px solid rgba(255,255,255,0.08); border-radius: 4px; cursor: pointer; text-transform: capitalize; }
.tab-btn.active { background: #7c3aed; color: #fff; border-color: #7c3aed; }
.platform-textarea { width: 100%; font-size: 12px; font-family: inherit; line-height: 1.5; padding: 10px; background: rgba(0,0,0,0.3); color: var(--text-primary); border: 1px solid rgba(255,255,255,0.1); border-radius: 6px; resize: vertical; outline: none; }
.platform-textarea:focus { border-color: #7c3aed; }
.char-count { font-size: 10px; color: var(--text-tertiary); text-align: right; margin-top: 4px; }
.char-count.over { color: #ef4444; }

/* Step 3: Media */
.media-options { display: flex; gap: 8px; margin-bottom: 12px; }
.media-option { padding: 8px 12px; font-size: 11px; background: rgba(255,255,255,0.03); border: 1px solid rgba(255,255,255,0.08); border-radius: 6px; cursor: pointer; }
.media-option.active { border-color: #7c3aed; background: rgba(124,58,237,0.1); }
.promo-preview img { width: 100%; border-radius: 6px; border: 1px solid rgba(255,255,255,0.08); }
.promo-loading { font-size: 11px; color: var(--text-tertiary); font-style: italic; }

/* Step 4: Schedule */
.schedule-presets { display: flex; flex-direction: column; gap: 8px; }
.schedule-option { display: flex; align-items: flex-start; gap: 10px; padding: 12px; background: rgba(255,255,255,0.03); border: 1px solid rgba(255,255,255,0.06); border-radius: 6px; cursor: pointer; }
.schedule-option.active { border-color: #7c3aed; background: rgba(124,58,237,0.08); }
.schedule-label { font-size: 12px; font-weight: 600; }
.schedule-desc { font-size: 10px; color: var(--text-tertiary); }

/* Step 5: Review */
.review-list { display: flex; flex-direction: column; gap: 8px; }
.review-card { padding: 10px 12px; background: rgba(255,255,255,0.03); border: 1px solid rgba(255,255,255,0.06); border-radius: 6px; }
.review-header { display: flex; justify-content: space-between; align-items: center; margin-bottom: 6px; }
.review-platform { font-size: 11px; font-weight: 600; text-transform: capitalize; color: #a78bfa; }
.review-schedule { font-size: 10px; color: var(--text-tertiary); display: flex; align-items: center; gap: 4px; }
.review-text { font-size: 11px; color: var(--text-secondary); line-height: 1.4; }

/* Step 6: Success */
.success-step { text-align: center; padding: 32px 16px; }
.success-icon { color: #4ade80; margin-bottom: 8px; }
.success-title { font-size: 18px; font-weight: 700; margin-bottom: 4px; }
.success-detail { font-size: 12px; color: var(--text-tertiary); }

/* Footer */
.modal-footer { padding: 12px 16px; border-top: 1px solid rgba(255,255,255,0.08); display: flex; align-items: center; gap: 8px; }
.footer-spacer { flex: 1; }
.btn { padding: 6px 14px; font-size: 11px; font-weight: 500; border-radius: 6px; cursor: pointer; border: none; display: flex; align-items: center; gap: 4px; }
.btn.secondary { background: rgba(255,255,255,0.06); color: var(--text-secondary); }
.btn.secondary:hover { background: rgba(255,255,255,0.1); }
.btn.primary { background: #7c3aed; color: #fff; }
.btn.primary:hover { background: #6d28d9; }
.btn.primary:disabled { opacity: 0.4; cursor: not-allowed; }
</style>
