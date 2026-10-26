<script setup lang="ts">
import { computed, onMounted, ref, watch } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { open } from '@tauri-apps/plugin-dialog'
import { useLocalStorage } from '@vueuse/core'
import { PhMinus, PhPlus } from '@phosphor-icons/vue'
import UiButton from '../ui/UiButton.vue'
import UiButtonGroup from '../ui/UiButtonGroup.vue'
import SettingsRow from './SettingsRow.vue'
import { useSettings } from '../../composables/useSettings'
import type { VaultCheck } from '../../types'

const { config, commit } = useSettings()

// ── Vault ───────────────────────────────────────────────────────────────────
const vaultPath = ref(config.value?.vault.path ?? '')
const vaultCheck = ref<VaultCheck | null>(null)
const vaultError = ref<string | null>(null)
const vaultChanged = ref(false)

async function checkVault(path: string): Promise<VaultCheck | null> {
  try {
    return (await invoke('check_vault_folder', {
      path,
      publishableDirs: config.value?.vault.publishable_dirs ?? [],
    })) as VaultCheck
  } catch {
    return null
  }
}

/** Commit a new vault folder, but only one that exists: a wrong path would
 *  empty the post list. */
async function commitVault(path: string) {
  if (!config.value) return
  const trimmed = path.trim()
  const check = await checkVault(trimmed)
  if (!check?.exists) {
    vaultError.value = 'That folder doesn’t exist. Dispatch is still using the previous one.'
    return
  }
  vaultError.value = null
  vaultCheck.value = check
  if (trimmed === config.value.vault.path) return
  // Picking a new vault usually means a new vault name too.
  if (check.folder_name && config.value.vault.name === folderName(config.value.vault.path)) {
    config.value.vault.name = check.folder_name
  }
  config.value.vault.path = trimmed
  vaultChanged.value = true
  commit()
}

function folderName(p: string) {
  return p.replace(/\/+$/, '').split('/').pop() ?? ''
}

async function browseVault() {
  const selected = await open({ directory: true, title: 'Choose your Obsidian vault', defaultPath: vaultPath.value || undefined })
  if (typeof selected === 'string') {
    vaultPath.value = selected
    commitVault(selected)
  }
}

onMounted(async () => {
  if (vaultPath.value) vaultCheck.value = await checkVault(vaultPath.value)
})

// ── Open to (per-Mac, lives in localStorage like before) ────────────────────
// 'preview' isn't offered: App.vue treats a stored 'preview' as the old
// default and lands on the Desk anyway.
const homeTab = useLocalStorage<string>('dispatch-home-tab', 'desk')
const homeOptions = [
  { id: 'desk', label: 'Desk' },
  { id: 'journal', label: 'Journal' },
  { id: 'modified', label: 'Changed since live' },
  { id: 'media', label: 'Media' },
  { id: 'activity', label: 'Activity' },
]
if (!homeOptions.some((o) => o.id === homeTab.value)) homeTab.value = 'desk'

// ── Editors ─────────────────────────────────────────────────────────────────
const enabledEditors = computed(() => (config.value?.editors ?? []).filter((e) => e.enabled))
const installed = ref<Record<string, boolean>>({})

async function checkInstalled(appName: string) {
  try {
    installed.value[appName] = (await invoke('check_app_installed', { appName })) as boolean
  } catch {
    /* unknown: show nothing */
  }
}
watch(
  () => config.value?.editors.map((e) => e.app_name) ?? [],
  (names) => names.forEach((n) => n && installed.value[n] === undefined && checkInstalled(n)),
  { immediate: true },
)

function toggleEditor(i: number, on: boolean) {
  if (!config.value) return
  const ed = config.value.editors[i]
  ed.enabled = on
  // The default editor has to be one that's turned on.
  if (!on && config.value.default_editor === ed.app_name) {
    const next = config.value.editors.find((e) => e.enabled)
    if (next) config.value.default_editor = next.app_name
  }
  if (on && !enabledEditors.value.some((e) => e.app_name === config.value!.default_editor)) {
    config.value.default_editor = ed.app_name
  }
  commit()
}

const selectedEditor = ref<number | null>(null)
const newEditorApp = ref('')
const addingEditor = ref(false)

function addEditor() {
  if (!config.value) return
  const app = newEditorApp.value.trim().replace(/\.app$/, '')
  if (!app) {
    addingEditor.value = false
    return
  }
  if (!config.value.editors.some((e) => e.app_name === app)) {
    config.value.editors.push({ name: app, app_name: app, enabled: true })
    commit()
  }
  newEditorApp.value = ''
  addingEditor.value = false
}

function removeSelectedEditor() {
  if (!config.value || selectedEditor.value === null) return
  const [removed] = config.value.editors.splice(selectedEditor.value, 1)
  selectedEditor.value = null
  if (removed && config.value.default_editor === removed.app_name) {
    config.value.default_editor = config.value.editors.find((e) => e.enabled)?.app_name ?? ''
  }
  commit()
}
</script>

<template>
  <div v-if="config" class="st-form">
    <SettingsRow
      label="Vault folder:"
      for="st-vault-path"
      help="The Obsidian vault Dispatch reads posts from. Nothing outside its publishable folders is ever published."
    >
      <div class="st-line">
        <input
          id="st-vault-path"
          v-model="vaultPath"
          class="st-input is-mono"
          spellcheck="false"
          :aria-invalid="!!vaultError"
          @change="commitVault(vaultPath)"
        />
        <UiButton size="small" label="Choose…" @click="browseVault" />
      </div>
      <template #note>
        <p v-if="vaultError" class="st-note is-error">{{ vaultError }}</p>
        <template v-else-if="vaultCheck?.exists">
          <p v-if="vaultCheck.is_obsidian_vault" class="st-note is-ok">Obsidian vault found.</p>
          <p v-else class="st-note is-warn">
            This folder has no <code>.obsidian</code> folder, so it may not be a vault.
          </p>
        </template>
        <p v-if="vaultChanged" class="st-note">
          The post list now reads from here. Restart Dispatch so it also watches this folder for edits.
        </p>
      </template>
    </SettingsRow>

    <SettingsRow
      label="Vault name:"
      for="st-vault-name"
      help="Your vault’s name in Obsidian, used by “Open in Obsidian” links. Usually the folder’s name."
    >
      <input
        id="st-vault-name"
        v-model="config.vault.name"
        class="st-input is-short"
        spellcheck="false"
        @change="commit()"
      />
      <template #note>
        <p
          v-if="vaultCheck?.folder_name && config.vault.name && vaultCheck.folder_name !== config.vault.name"
          class="st-note is-warn"
        >
          The folder is named “{{ vaultCheck.folder_name }}”. If Obsidian links open the wrong vault, use that.
        </p>
      </template>
    </SettingsRow>

    <div class="st-section"><h3>Window</h3></div>

    <SettingsRow label="Open to:" for="st-home" help="The view Dispatch shows when it launches.">
      <select id="st-home" v-model="homeTab" class="st-select">
        <option v-for="o in homeOptions" :key="o.id" :value="o.id">{{ o.label }}</option>
      </select>
    </SettingsRow>

    <div class="st-section"><h3>Editors</h3></div>

    <SettingsRow
      label="Open posts in:"
      for="st-default-editor"
      help="The app that opens a post when you press I or choose Open in Editor."
    >
      <select
        id="st-default-editor"
        v-model="config.default_editor"
        class="st-select"
        :disabled="!enabledEditors.length"
        @change="commit()"
      >
        <option v-for="e in enabledEditors" :key="e.app_name" :value="e.app_name">{{ e.name }}</option>
      </select>
    </SettingsRow>

    <SettingsRow
      label="Show in “Open in”:"
      help="Checked apps get a button in each post’s toolbar. Add any Mac app by its name in the Applications folder."
    >
      <div class="st-list" role="listbox" aria-label="Editors">
        <div
          v-for="(e, i) in config.editors"
          :key="e.app_name + i"
          class="st-list-row"
          :class="{ 'is-selected': selectedEditor === i }"
          role="option"
          :aria-selected="selectedEditor === i"
          @click="selectedEditor = i"
        >
          <input
            type="checkbox"
            :checked="e.enabled"
            :aria-label="`Show ${e.name}`"
            @change="toggleEditor(i, ($event.target as HTMLInputElement).checked)"
          />
          <span class="st-grow">{{ e.name }}</span>
          <span v-if="installed[e.app_name] === false" class="st-note is-warn">Not in Applications</span>
          <span v-else-if="e.name !== e.app_name" class="st-muted st-mono">{{ e.app_name }}</span>
        </div>
        <div v-if="addingEditor" class="st-list-row">
          <input
            v-model="newEditorApp"
            class="st-input"
            placeholder="App name, e.g. Sublime Text"
            spellcheck="false"
            autofocus
            @keydown.enter="addEditor"
            @keydown.esc="addingEditor = false"
            @blur="addEditor"
          />
        </div>
      </div>
      <UiButtonGroup label="Add or remove editors">
        <UiButton size="small" :icon="PhPlus" aria-label="Add an editor" @click="addingEditor = true" />
        <UiButton
          size="small"
          :icon="PhMinus"
          aria-label="Remove the selected editor"
          :disabled="selectedEditor === null"
          @click="removeSelectedEditor"
        />
      </UiButtonGroup>
    </SettingsRow>
  </div>
</template>

<style scoped>
.st-list-row.is-selected {
  background: var(--accent-selected);
}
</style>
