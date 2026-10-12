<script setup lang="ts">
import { computed, ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import UiButton from '../ui/UiButton.vue'
import SettingsRow from './SettingsRow.vue'
import { useSettings } from '../../composables/useSettings'

const { config, env, commit } = useSettings()

type Result = { tone: string; text: string } | null

/** Run a probe command and turn its answer into a status line. */
function useProbe(run: () => Promise<Result>) {
  const busy = ref(false)
  const result = ref<Result>(null)
  async function test() {
    busy.value = true
    try {
      result.value = await run()
    } catch (e) {
      result.value = { tone: 'is-error', text: String(e) }
    } finally {
      busy.value = false
    }
  }
  return { busy, result, test }
}

// ── Analytics (Umami) ───────────────────────────────────────────────────────
const analytics = useProbe(async () =>
  (await invoke('check_analytics_status'))
    ? { tone: 'is-ok', text: 'Signed in to Umami. Post stats will load.' }
    : {
        tone: 'is-error',
        text: env.value?.umami
          ? 'Couldn’t sign in to Umami with the credentials in .env.'
          : 'Add UMAMI_USERNAME, UMAMI_PASSWORD and UMAMI_WEBSITE_ID to .env to see post stats.',
      },
)
const analyticsUrlNote = computed(() => {
  const v = config.value?.analytics_url?.trim()
  if (!v) return null
  try {
    new URL(v)
    return null
  } catch {
    return 'Enter a full address, like https://analytics.example.com'
  }
})

// ── Mastodon ────────────────────────────────────────────────────────────────
const mastodon = useProbe(async () => {
  const who = (await invoke('verify_mastodon')) as string
  return { tone: 'is-ok', text: `Signed in as ${who}.` }
})
const mastodonInstance = computed({
  get: () => config.value?.mastodon_instance ?? '',
  set: (v: string) => {
    if (!config.value) return
    const host = v
      .trim()
      .replace(/^https?:\/\//, '')
      .replace(/\/.*$/, '')
    config.value.mastodon_instance = host || null
  },
})

// ── Obsidian Local REST API (backlinks) ────────────────────────────────────
const obsidian = useProbe(async () =>
  (await invoke('check_obsidian_api'))
    ? { tone: 'is-ok', text: 'Connected. Backlinks will show for each post.' }
    : {
        tone: 'is-error',
        text: env.value?.obsidian_api_key
          ? 'Couldn’t reach Obsidian. Is it open with the Local REST API plugin on?'
          : 'Add OBSIDIAN_API_KEY to .env, from the Local REST API plugin’s settings.',
      },
)
</script>

<template>
  <div v-if="config && env" class="st-form">
    <div class="st-section">
      <h3>Analytics</h3>
      <UiButton size="small" :label="analytics.busy.value ? 'Testing…' : 'Test'" :disabled="analytics.busy.value" @click="analytics.test" />
    </div>
    <SettingsRow
      label="Dashboard address:"
      for="st-analytics"
      help="Opens from the status bar’s analytics menu. Stats are fetched with the Umami login in .env."
    >
      <input
        id="st-analytics"
        v-model="config.analytics_url"
        class="st-input"
        placeholder="https://analytics.example.com"
        spellcheck="false"
        :aria-invalid="!!analyticsUrlNote"
        @change="commit()"
      />
      <template #note>
        <p v-if="analyticsUrlNote" class="st-note is-error">{{ analyticsUrlNote }}</p>
        <p v-if="analytics.result.value" class="st-note" :class="analytics.result.value.tone">
          {{ analytics.result.value.text }}
        </p>
        <p v-else class="st-note" :class="env.umami ? '' : 'is-warn'">
          {{ env.umami ? `Umami login found in .env${env.umami_url ? ` for ${env.umami_url}` : ''}.` : 'No Umami login in .env.' }}
        </p>
      </template>
    </SettingsRow>

    <div class="st-section">
      <h3>Mastodon</h3>
      <UiButton size="small" :label="mastodon.busy.value ? 'Checking…' : 'Verify'" :disabled="mastodon.busy.value" @click="mastodon.test" />
    </div>
    <SettingsRow
      label="Server:"
      for="st-mastodon"
      help="The Mastodon server your account is on. Syndicate posts there after publishing."
    >
      <input
        id="st-mastodon"
        v-model.lazy="mastodonInstance"
        class="st-input is-short is-mono"
        placeholder="mastodon.social"
        spellcheck="false"
        :disabled="!!env.mastodon_instance"
        @change="commit()"
      />
      <template #note>
        <p v-if="env.mastodon_instance" class="st-note is-warn">
          Using <code>MASTODON_INSTANCE={{ env.mastodon_instance }}</code> from .env.
        </p>
        <p v-if="mastodon.result.value" class="st-note" :class="mastodon.result.value.tone">
          {{ mastodon.result.value.text }}
        </p>
        <p v-else class="st-note" :class="env.mastodon_token ? '' : 'is-warn'">
          {{ env.mastodon_token ? 'Access token found in .env.' : 'Add MASTODON_ACCESS_TOKEN to .env to post to Mastodon.' }}
        </p>
      </template>
    </SettingsRow>

    <div class="st-section"><h3>Bluesky</h3></div>
    <SettingsRow label="Account:" help="Syndicate posts to Bluesky after publishing. Use an app password, never your main one.">
      <p class="st-note" :class="env.bluesky ? 'is-ok' : 'is-warn'">
        {{ env.bluesky ? 'Handle and app password found in .env.' : 'Add BLUESKY_HANDLE and BLUESKY_APP_PASSWORD to .env.' }}
      </p>
    </SettingsRow>

    <div class="st-section"><h3>Webmentions</h3></div>
    <SettingsRow
      label=""
      help="Every publish already notifies the sites you link to. This also relays it through Bridgy Fed so Mastodon servers see the post."
    >
      <label class="st-check">
        <input v-model="config.webmentions_bridgy_fed" type="checkbox" @change="commit()" />
        Also notify the fediverse via Bridgy Fed
      </label>
    </SettingsRow>

    <div class="st-section">
      <h3>Obsidian</h3>
      <UiButton size="small" :label="obsidian.busy.value ? 'Testing…' : 'Test'" :disabled="obsidian.busy.value" @click="obsidian.test" />
    </div>
    <SettingsRow label="Backlinks:" help="Shows which notes link to a post, through Obsidian’s Local REST API plugin.">
      <p v-if="obsidian.result.value" class="st-note" :class="obsidian.result.value.tone">
        {{ obsidian.result.value.text }}
      </p>
      <p v-else class="st-note" :class="env.obsidian_api_key ? '' : 'is-warn'">
        {{ env.obsidian_api_key ? 'API key found in .env.' : 'Add OBSIDIAN_API_KEY to .env.' }}
      </p>
    </SettingsRow>

    <div class="st-section"><h3>AI alt text</h3></div>
    <SettingsRow label="Provider:" help="Drafts image descriptions for you to review before they’re saved.">
      <p class="st-note" :class="env.alt_text_provider ? 'is-ok' : 'is-warn'">
        {{
          env.alt_text_provider === 'openai'
            ? 'Using OpenAI (OPENAI_API_KEY in .env).'
            : env.alt_text_provider === 'anthropic'
              ? 'Using Claude (ANTHROPIC_API_KEY in .env).'
              : 'Add OPENAI_API_KEY or ANTHROPIC_API_KEY to .env to draft alt text.'
        }}
      </p>
    </SettingsRow>
  </div>
</template>
