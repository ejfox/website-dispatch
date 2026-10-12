<script setup lang="ts">
import { ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { PhX } from '@phosphor-icons/vue'
import UiButton from '../ui/UiButton.vue'
import SettingsRow from './SettingsRow.vue'
import { useSettings } from '../../composables/useSettings'

const { config, env, configPath, configExists, commit } = useSettings()

// ── Hidden folders (excluded_dirs) ──────────────────────────────────────────
const newDir = ref('')
function addDir() {
  if (!config.value) return
  const dir = newDir.value.trim().replace(/^\/+|\/+$/g, '')
  if (dir && !config.value.vault.excluded_dirs.includes(dir)) {
    config.value.vault.excluded_dirs.push(dir)
    commit()
  }
  newDir.value = ''
}
function removeDir(i: number) {
  if (!config.value) return
  config.value.vault.excluded_dirs.splice(i, 1)
  commit()
}

const revealError = ref<string | null>(null)
async function reveal() {
  try {
    await invoke('reveal_config_file')
    revealError.value = null
  } catch (e) {
    revealError.value = String(e)
  }
}
</script>

<template>
  <div v-if="config && env" class="st-form">
    <SettingsRow
      label="Skip folders:"
      help="Dispatch never looks inside these vault folders, even within a publishable one. Use it for templates and attachments."
    >
      <div class="st-chips">
        <span v-for="(d, i) in config.vault.excluded_dirs" :key="d" class="st-chip">
          {{ d }}
          <UiButton variant="plain" size="small" :icon="PhX" :aria-label="`Stop skipping ${d}`" @click="removeDir(i)" />
        </span>
        <input
          v-model="newDir"
          class="st-input is-short is-mono"
          placeholder="Add a folder"
          spellcheck="false"
          @keydown.enter="addDir"
          @change="addDir"
        />
      </div>
    </SettingsRow>

    <div class="st-section"><h3>Where settings live</h3></div>

    <SettingsRow
      label="Settings file:"
      help="Everything in this window is saved here as you change it. It stays on this Mac."
    >
      <span class="st-path">{{ configPath }}</span>
      <UiButton size="small" label="Show in Finder" :disabled="!configExists" @click="reveal" />
      <template #note>
        <p v-if="!configExists" class="st-note">Created the first time you change a setting.</p>
        <p v-if="revealError" class="st-note is-error">{{ revealError }}</p>
      </template>
    </SettingsRow>

    <SettingsRow
      label="Keys file:"
      help="API keys for Mastodon, Bluesky, Umami, Obsidian and AI alt text are read from a .env file when Dispatch launches. Restart after editing it."
    >
      <template v-if="env.dotenv_files.length">
        <span v-for="f in env.dotenv_files" :key="f" class="st-path">{{ f }}</span>
      </template>
      <p v-else class="st-note is-warn">No .env file found. Copy .env.example to .env in the Dispatch folder.</p>
    </SettingsRow>
  </div>
</template>
