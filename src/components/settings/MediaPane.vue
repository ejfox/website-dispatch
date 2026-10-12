<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import UiButton from '../ui/UiButton.vue'
import UiButtonGroup from '../ui/UiButtonGroup.vue'
import SettingsRow from './SettingsRow.vue'
import SecretField from './SecretField.vue'
import { useSettings } from '../../composables/useSettings'
import type { MediaDestinationKind, MediaStatus } from '../../types'

const { config, env, commit } = useSettings()

const media = computed(() => config.value!.media)
const other = computed<MediaDestinationKind>(() => (media.value.primary === 'cloudinary' ? 'r2' : 'cloudinary'))
const NAMES: Record<MediaDestinationKind, string> = { cloudinary: 'Cloudinary', r2: 'Cloudflare R2' }

function setPrimary(kind: MediaDestinationKind) {
  if (media.value.primary === kind) return
  media.value.primary = kind
  // The primary can't also be its own mirror.
  media.value.mirrors = media.value.mirrors.filter((m) => m !== kind)
  commit()
}

const mirrored = computed(() => media.value.mirrors.includes(other.value))
function setMirror(on: boolean) {
  media.value.mirrors = on ? [other.value] : []
  commit()
}

// ── Test: media.rs `status()` probes both destinations in parallel ──────────
// It probes what's saved, which is everything: fields commit on Return/blur.
const status = ref<MediaStatus | null>(null)
const testing = ref(false)
const tested = ref(false)
async function test() {
  testing.value = true
  try {
    status.value = (await invoke('check_media_status')) as MediaStatus
  } catch {
    status.value = null
  } finally {
    testing.value = false
    tested.value = true
  }
}
onMounted(test)

function note(kind: MediaDestinationKind): { tone: string; text: string } {
  if (testing.value) return { tone: '', text: 'Checking…' }
  if (!tested.value || !status.value) return { tone: '', text: 'Not checked yet.' }
  const ok = status.value[kind]
  if (ok) return { tone: 'is-ok', text: `Connected. Uploads to ${NAMES[kind]} work.` }
  const used = media.value.primary === kind || media.value.mirrors.includes(kind)
  return used
    ? { tone: 'is-error', text: `Couldn’t connect. Check the fields below; uploads to ${NAMES[kind]} will fail.` }
    : { tone: '', text: 'Not connected. Fill this in only if you use it.' }
}

// A destination you don't use and haven't filled in folds to one line.
const expanded = ref<Record<MediaDestinationKind, boolean>>({ cloudinary: false, r2: false })
function inUse(kind: MediaDestinationKind) {
  if (expanded.value[kind] || media.value.primary === kind || media.value.mirrors.includes(kind)) return true
  if (kind === 'cloudinary') return !!(media.value.cloudinary?.cloud_name || env.value?.cloudinary)
  return !!(media.value.r2?.account_id || media.value.r2?.bucket || env.value?.r2)
}

const cloudinaryFromEnv = computed(() => !!env.value?.cloudinary)
const r2FromEnv = computed(() => !!env.value?.r2)

const r2UrlNote = computed(() => {
  const v = media.value.r2?.public_url_base.trim() ?? ''
  if (!v) return null
  try {
    const u = new URL(v)
    return u.protocol === 'https:' ? null : 'Use an https:// address so images load on your site.'
  } catch {
    return 'Enter a full address, like https://media.example.com'
  }
})
</script>

<template>
  <div v-if="config" class="st-form">
    <SettingsRow
      label="Upload images to:"
      help="Where a dropped image or screenshot is uploaded. Its link from here is what goes into the post."
    >
      <UiButtonGroup label="Upload destination">
        <UiButton
          size="small"
          label="Cloudinary"
          :variant="media.primary === 'cloudinary' ? 'primary' : 'secondary'"
          :aria-pressed="media.primary === 'cloudinary'"
          @click="setPrimary('cloudinary')"
        />
        <UiButton
          size="small"
          label="Cloudflare R2"
          :variant="media.primary === 'r2' ? 'primary' : 'secondary'"
          :aria-pressed="media.primary === 'r2'"
          @click="setPrimary('r2')"
        />
      </UiButtonGroup>
    </SettingsRow>

    <SettingsRow label="" help="A backup copy. If it fails, the upload still succeeds.">
      <label class="st-check">
        <input type="checkbox" :checked="mirrored" @change="setMirror(($event.target as HTMLInputElement).checked)" />
        Also keep a copy in {{ NAMES[other] }}
      </label>
    </SettingsRow>

    <!-- Cloudinary -->
    <div class="st-section">
      <h3>Cloudinary</h3>
      <UiButton v-if="inUse('cloudinary')" size="small" :label="testing ? 'Testing…' : 'Test'" :disabled="testing" @click="test" />
    </div>
    <SettingsRow v-if="!inUse('cloudinary')" label="" help="Not used. Set it up to upload here or keep copies here.">
      <UiButton size="small" label="Set Up Cloudinary…" @click="expanded.cloudinary = true" />
    </SettingsRow>
    <template v-else>
    <SettingsRow label="Status:">
      <p class="st-note" :class="note('cloudinary').tone">{{ note('cloudinary').text }}</p>
      <p v-if="cloudinaryFromEnv" class="st-note is-warn">
        Using the <code>CLOUDINARY_*</code> keys from your .env file. These fields are ignored while those are set.
      </p>
    </SettingsRow>
    <SettingsRow label="Cloud name:" for="st-cl-name" help="Shown at the top of your Cloudinary dashboard.">
      <input
        id="st-cl-name"
        v-model="media.cloudinary!.cloud_name"
        class="st-input is-short is-mono"
        placeholder="your-cloud-name"
        spellcheck="false"
        :disabled="cloudinaryFromEnv"
        @change="commit()"
      />
    </SettingsRow>
    <SettingsRow label="API key:" for="st-cl-key">
      <SecretField
        id="st-cl-key"
        v-model="media.cloudinary!.api_key"
        placeholder="From Settings → API Keys"
        :disabled="cloudinaryFromEnv"
        @commit="commit()"
      />
    </SettingsRow>
    <SettingsRow
      label="API secret:"
      for="st-cl-secret"
      help="Stored in your settings file on this Mac only. Dispatch shows just the last four characters."
    >
      <SecretField
        id="st-cl-secret"
        v-model="media.cloudinary!.api_secret"
        :disabled="cloudinaryFromEnv"
        @commit="commit()"
      />
    </SettingsRow>
    </template>

    <!-- R2 -->
    <div class="st-section">
      <h3>Cloudflare R2</h3>
      <UiButton v-if="inUse('r2')" size="small" :label="testing ? 'Testing…' : 'Test'" :disabled="testing" @click="test" />
    </div>
    <SettingsRow v-if="!inUse('r2')" label="" help="Not used. Set it up to upload here or keep copies here.">
      <UiButton size="small" label="Set Up R2…" @click="expanded.r2 = true" />
    </SettingsRow>
    <template v-else>
    <SettingsRow label="Status:">
      <p class="st-note" :class="note('r2').tone">{{ note('r2').text }}</p>
      <p v-if="r2FromEnv" class="st-note is-warn">
        Using the <code>R2_*</code> keys from your .env file. These fields are ignored while those are set.
      </p>
    </SettingsRow>
    <SettingsRow
      label="Account ID:"
      for="st-r2-acct"
      help="In the Cloudflare dashboard, on the right side of any R2 page."
    >
      <input
        id="st-r2-acct"
        v-model="media.r2!.account_id"
        class="st-input is-mono"
        spellcheck="false"
        :disabled="r2FromEnv"
        @change="commit()"
      />
    </SettingsRow>
    <SettingsRow label="Bucket:" for="st-r2-bucket" help="The R2 bucket uploads go into.">
      <input
        id="st-r2-bucket"
        v-model="media.r2!.bucket"
        class="st-input is-short is-mono"
        placeholder="my-media"
        spellcheck="false"
        :disabled="r2FromEnv"
        @change="commit()"
      />
    </SettingsRow>
    <SettingsRow label="Access key ID:" for="st-r2-key">
      <SecretField id="st-r2-key" v-model="media.r2!.access_key_id" :disabled="r2FromEnv" @commit="commit()" />
    </SettingsRow>
    <SettingsRow
      label="Secret access key:"
      for="st-r2-secret"
      help="From R2 → Manage API tokens. Shown here as its last four characters only."
    >
      <SecretField id="st-r2-secret" v-model="media.r2!.secret_access_key" :disabled="r2FromEnv" @commit="commit()" />
    </SettingsRow>
    <SettingsRow
      label="Public address:"
      for="st-r2-url"
      help="The web address that serves this bucket. An image’s link is this address plus its file name."
    >
      <input
        id="st-r2-url"
        v-model="media.r2!.public_url_base"
        class="st-input is-mono"
        placeholder="https://media.example.com"
        spellcheck="false"
        :disabled="r2FromEnv"
        :aria-invalid="!!r2UrlNote"
        @change="commit()"
      />
      <template #note>
        <p v-if="r2UrlNote" class="st-note is-error">{{ r2UrlNote }}</p>
      </template>
    </SettingsRow>
    </template>
  </div>
</template>
