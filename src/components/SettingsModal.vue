<script setup lang="ts">
import { ref, onMounted } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { open } from '@tauri-apps/plugin-dialog'
import { X } from 'lucide-vue-next'

interface VaultConfig {
  path: string
  name: string
  excluded_dirs: string[]
  publishable_dirs: string[]
}

interface PublishTarget {
  name: string
  id: string
  repo_path: string
  domain: string
  content_path_pattern: string
  branch: string
  is_default: boolean
}

interface EditorConfig {
  name: string
  app_name: string
  enabled: boolean
}

interface AppConfig {
  version: number
  vault: VaultConfig
  publish_targets: PublishTarget[]
  editors: EditorConfig[]
  default_editor: string
  cloudinary_cloud_name: string | null
  analytics_url: string | null
}

const emit = defineEmits<{ close: [], saved: [] }>()

const activeTab = ref<'vault' | 'publishing' | 'editors' | 'connections'>('vault')
const config = ref<AppConfig | null>(null)
const configPath = ref('')
const saving = ref(false)
const saveMessage = ref('')

// Validation state
const vaultPathValid = ref<boolean | null>(null)
const repoPathValid = ref<Record<number, boolean | null>>({})

// New dir input
const newExcludedDir = ref('')
const newPublishableDir = ref('')

// New editor inputs
const newEditorName = ref('')
const newEditorApp = ref('')

onMounted(async () => {
  try {
    config.value = await invoke('get_app_config') as AppConfig
    configPath.value = await invoke('get_config_path') as string
  } catch (e) {
    console.error('Failed to load config:', e)
  }
})

async function browseVaultPath() {
  const selected = await open({ directory: true, title: 'Select Obsidian Vault' })
  if (selected && config.value) {
    config.value.vault.path = selected as string
    validateVaultPath()
  }
}

async function browseRepoPath(index: number) {
  const selected = await open({ directory: true, title: 'Select Website Repo' })
  if (selected && config.value) {
    config.value.publish_targets[index].repo_path = selected as string
    validateRepoPath(index)
  }
}

async function validateVaultPath() {
  if (!config.value) return
  vaultPathValid.value = await invoke('validate_vault_path', { path: config.value.vault.path }) as boolean
}

async function validateRepoPath(index: number) {
  if (!config.value) return
  repoPathValid.value[index] = await invoke('validate_repo_path', {
    path: config.value.publish_targets[index].repo_path
  }) as boolean
}

function addExcludedDir() {
  if (!config.value || !newExcludedDir.value.trim()) return
  config.value.vault.excluded_dirs.push(newExcludedDir.value.trim())
  newExcludedDir.value = ''
}

function removeExcludedDir(index: number) {
  if (!config.value) return
  config.value.vault.excluded_dirs.splice(index, 1)
}

function addPublishableDir() {
  if (!config.value || !newPublishableDir.value.trim()) return
  config.value.vault.publishable_dirs.push(newPublishableDir.value.trim())
  newPublishableDir.value = ''
}

function removePublishableDir(index: number) {
  if (!config.value) return
  config.value.vault.publishable_dirs.splice(index, 1)
}

function addTarget() {
  if (!config.value) return
  const id = `target-${Date.now()}`
  config.value.publish_targets.push({
    name: 'New Target',
    id,
    repo_path: '',
    domain: 'https://',
    content_path_pattern: 'content/blog/{year}',
    branch: 'main',
    is_default: config.value.publish_targets.length === 0,
  })
}

function removeTarget(index: number) {
  if (!config.value) return
  const wasDefault = config.value.publish_targets[index].is_default
  config.value.publish_targets.splice(index, 1)
  if (wasDefault && config.value.publish_targets.length > 0) {
    config.value.publish_targets[0].is_default = true
  }
}

function setDefaultTarget(index: number) {
  if (!config.value) return
  config.value.publish_targets.forEach((t, i) => {
    t.is_default = i === index
  })
}

function addEditor() {
  if (!config.value || !newEditorName.value.trim() || !newEditorApp.value.trim()) return
  config.value.editors.push({
    name: newEditorName.value.trim(),
    app_name: newEditorApp.value.trim(),
    enabled: true,
  })
  newEditorName.value = ''
  newEditorApp.value = ''
}

function removeEditor(index: number) {
  if (!config.value) return
  const removed = config.value.editors.splice(index, 1)
  if (config.value.default_editor === removed[0]?.app_name && config.value.editors.length > 0) {
    config.value.default_editor = config.value.editors[0].app_name
  }
}

async function save() {
  if (!config.value || saving.value) return
  saving.value = true
  try {
    await invoke('save_app_config', { configData: config.value })
    saveMessage.value = 'Saved!'
    setTimeout(() => { saveMessage.value = '' }, 2000)
    emit('saved')
  } catch (e) {
    saveMessage.value = `Error: ${e}`
  }
  saving.value = false
}
</script>

<template>
  <Transition name="modal">
    <div class="settings-overlay" @click.self="emit('close')">
      <div class="settings-modal">
        <div class="settings-header">
          <h2>Settings</h2>
          <button class="close-btn" @click="emit('close')"><X :size="16" /></button>
        </div>

        <div class="settings-tabs">
          <button :class="{ active: activeTab === 'vault' }" @click="activeTab = 'vault'">Vault</button>
          <button :class="{ active: activeTab === 'publishing' }" @click="activeTab = 'publishing'">Publishing</button>
          <button :class="{ active: activeTab === 'editors' }" @click="activeTab = 'editors'">Editors</button>
          <button :class="{ active: activeTab === 'connections' }" @click="activeTab = 'connections'">Connections</button>
        </div>

        <div class="settings-body" v-if="config">
          <!-- Vault Tab -->
          <div v-if="activeTab === 'vault'" class="tab-content">
            <div class="field">
              <label>Vault Path</label>
              <div class="path-input">
                <input
                  v-model="config.vault.path"
                  @blur="validateVaultPath"
                  :class="{ invalid: vaultPathValid === false, valid: vaultPathValid === true }"
                />
                <button @click="browseVaultPath" class="browse-btn">Browse</button>
              </div>
            </div>

            <div class="field">
              <label>Vault Name <span class="hint">for obsidian:// URLs</span></label>
              <input v-model="config.vault.name" />
            </div>

            <div class="field">
              <label>Excluded Directories</label>
              <div class="chip-list">
                <span v-for="(dir, i) in config.vault.excluded_dirs" :key="i" class="chip">
                  {{ dir }}
                  <button @click="removeExcludedDir(i)" class="chip-remove">&times;</button>
                </span>
                <div class="chip-input-row">
                  <input
                    v-model="newExcludedDir"
                    placeholder="Add directory..."
                    @keydown.enter="addExcludedDir"
                  />
                  <button @click="addExcludedDir" class="add-btn">+</button>
                </div>
              </div>
            </div>

            <div class="field">
              <label>Publishable Directories</label>
              <div class="chip-list">
                <span v-for="(dir, i) in config.vault.publishable_dirs" :key="i" class="chip">
                  {{ dir }}
                  <button @click="removePublishableDir(i)" class="chip-remove">&times;</button>
                </span>
                <div class="chip-input-row">
                  <input
                    v-model="newPublishableDir"
                    placeholder="Add directory..."
                    @keydown.enter="addPublishableDir"
                  />
                  <button @click="addPublishableDir" class="add-btn">+</button>
                </div>
              </div>
            </div>
          </div>

          <!-- Publishing Tab -->
          <div v-if="activeTab === 'publishing'" class="tab-content">
            <div v-for="(target, i) in config.publish_targets" :key="target.id" class="target-card">
              <div class="target-header">
                <input v-model="target.name" class="target-name" />
                <div class="target-actions">
                  <label class="default-radio">
                    <input type="radio" :checked="target.is_default" @change="setDefaultTarget(i)" />
                    Default
                  </label>
                  <button v-if="config.publish_targets.length > 1" @click="removeTarget(i)" class="remove-btn">&times;</button>
                </div>
              </div>

              <div class="field">
                <label>Repo Path</label>
                <div class="path-input">
                  <input
                    v-model="target.repo_path"
                    @blur="validateRepoPath(i)"
                    :class="{ invalid: repoPathValid[i] === false, valid: repoPathValid[i] === true }"
                  />
                  <button @click="browseRepoPath(i)" class="browse-btn">Browse</button>
                </div>
              </div>

              <div class="field-row">
                <div class="field">
                  <label>Domain</label>
                  <input v-model="target.domain" placeholder="https://example.com" />
                </div>
                <div class="field">
                  <label>Branch</label>
                  <input v-model="target.branch" />
                </div>
              </div>

              <div class="field">
                <label>Content Path Pattern <span class="hint">{year} = current year</span></label>
                <input v-model="target.content_path_pattern" placeholder="content/blog/{year}" />
              </div>
            </div>

            <button @click="addTarget" class="add-target-btn">+ Add Target</button>
          </div>

          <!-- Editors Tab -->
          <div v-if="activeTab === 'editors'" class="tab-content">
            <div v-for="(editor, i) in config.editors" :key="i" class="editor-row">
              <label class="editor-toggle">
                <input type="checkbox" v-model="editor.enabled" />
                <span class="editor-name">{{ editor.name }}</span>
                <span class="editor-app">{{ editor.app_name }}</span>
              </label>
              <div class="editor-actions">
                <label class="default-radio">
                  <input
                    type="radio"
                    :checked="config.default_editor === editor.app_name"
                    @change="config.default_editor = editor.app_name"
                  />
                  Default
                </label>
                <button @click="removeEditor(i)" class="remove-btn">&times;</button>
              </div>
            </div>

            <div class="add-editor-form">
              <input v-model="newEditorName" placeholder="Name (e.g. Sublime)" />
              <input v-model="newEditorApp" placeholder="App name (e.g. Sublime Text)" />
              <button @click="addEditor" class="add-btn" :disabled="!newEditorName || !newEditorApp">+</button>
            </div>
          </div>

          <!-- Connections Tab -->
          <div v-if="activeTab === 'connections'" class="tab-content">
            <div class="field">
              <label>Cloudinary Cloud Name</label>
              <input v-model="config.cloudinary_cloud_name" placeholder="your-cloud-name" />
              <span class="hint">API keys configured via .env file</span>
            </div>

            <div class="field">
              <label>Analytics URL</label>
              <input v-model="config.analytics_url" placeholder="https://analytics.example.com" />
              <span class="hint">Umami instance URL. Credentials via .env</span>
            </div>

            <div class="config-path">
              Config file: <code>{{ configPath }}</code>
            </div>
          </div>
        </div>

        <div class="settings-footer">
          <span class="save-msg" :class="{ error: saveMessage.startsWith('Error') }">{{ saveMessage }}</span>
          <button @click="emit('close')" class="cancel-btn">Cancel</button>
          <button @click="save" class="save-btn" :disabled="saving">
            {{ saving ? 'Saving...' : 'Save' }}
          </button>
        </div>
      </div>
    </div>
  </Transition>
</template>

<style scoped>
.settings-overlay {
  position: fixed;
  inset: 0;
  background: rgba(0, 0, 0, 0.5);
  backdrop-filter: blur(4px);
  -webkit-backdrop-filter: blur(4px);
  z-index: 200;
  display: flex;
  align-items: center;
  justify-content: center;
}

.settings-modal {
  width: 580px;
  max-width: 95vw;
  max-height: 85vh;
  background: var(--modal-bg);
  backdrop-filter: blur(24px) saturate(180%);
  -webkit-backdrop-filter: blur(24px) saturate(180%);
  border: 1px solid var(--border-light);
  border-radius: 12px;
  display: flex;
  flex-direction: column;
  box-shadow: var(--shadow-lg);
  animation: scaleIn 0.2s var(--ease-out-expo);
}

.settings-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 16px 20px 12px;
  border-bottom: 1px solid var(--border);
}

.settings-header h2 {
  font-size: 15px;
  font-weight: 600;
}

.close-btn {
  background: none;
  border: none;
  color: var(--text-tertiary);
  cursor: pointer;
  padding: 4px;
  border-radius: 4px;
}

.close-btn:hover {
  background: var(--accent);
  color: var(--text-primary);
}

.settings-tabs {
  display: flex;
  border-bottom: 1px solid var(--border);
  padding: 0 16px;
}

.settings-tabs button {
  padding: 8px 14px;
  font-size: 11px;
  font-weight: 500;
  background: none;
  border: none;
  color: var(--text-tertiary);
  cursor: pointer;
  border-bottom: 2px solid transparent;
  margin-bottom: -1px;
}

.settings-tabs button:hover { color: var(--text-secondary); }
.settings-tabs button.active {
  color: var(--text-primary);
  border-bottom-color: var(--text-primary);
}

.settings-body {
  flex: 1;
  overflow-y: auto;
  padding: 16px 20px;
}

.tab-content {
  display: flex;
  flex-direction: column;
  gap: 14px;
}

.field {
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.field label {
  font-size: 11px;
  font-weight: 600;
  color: var(--text-secondary);
  text-transform: uppercase;
  letter-spacing: 0.5px;
}

.hint {
  font-size: 10px;
  color: var(--text-tertiary);
  font-weight: 400;
  text-transform: none;
  letter-spacing: 0;
}

.field input, .add-editor-form input {
  padding: 6px 10px;
  font-size: 12px;
  background: var(--bg-tertiary);
  border: 1px solid var(--border);
  border-radius: 6px;
  color: var(--text-primary);
  font-family: 'SF Mono', monospace;
}

.field input:focus, .add-editor-form input:focus {
  outline: none;
  border-color: var(--selection-bg);
}

.field input.valid { border-color: var(--success); }
.field input.invalid { border-color: var(--danger); }

.path-input {
  display: flex;
  gap: 6px;
}

.path-input input { flex: 1; }

.browse-btn, .add-btn {
  padding: 6px 12px;
  font-size: 11px;
  background: var(--accent);
  border: 1px solid var(--border);
  border-radius: 6px;
  color: var(--text-primary);
  cursor: pointer;
}

.browse-btn:hover, .add-btn:hover {
  background: var(--bg-tertiary);
}

.chip-list {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
}

.chip {
  display: flex;
  align-items: center;
  gap: 4px;
  padding: 3px 8px;
  font-size: 11px;
  background: var(--accent);
  border: 1px solid var(--border);
  border-radius: 12px;
  color: var(--text-secondary);
  font-family: 'SF Mono', monospace;
}

.chip-remove {
  background: none;
  border: none;
  color: var(--text-tertiary);
  cursor: pointer;
  font-size: 14px;
  padding: 0;
  line-height: 1;
}

.chip-remove:hover { color: var(--danger); }

.chip-input-row {
  display: flex;
  gap: 4px;
  width: 100%;
  margin-top: 4px;
}

.chip-input-row input {
  flex: 1;
  padding: 4px 8px;
  font-size: 11px;
  background: var(--bg-tertiary);
  border: 1px solid var(--border);
  border-radius: 6px;
  color: var(--text-primary);
  font-family: 'SF Mono', monospace;
}

.chip-input-row input:focus {
  outline: none;
  border-color: var(--selection-bg);
}

/* Publish Targets */
.target-card {
  border: 1px solid var(--border);
  border-radius: 8px;
  padding: 12px;
  display: flex;
  flex-direction: column;
  gap: 10px;
}

.target-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
}

.target-name {
  font-size: 13px;
  font-weight: 600;
  background: none;
  border: none;
  color: var(--text-primary);
  padding: 2px 4px;
  border-radius: 4px;
}

.target-name:focus {
  outline: none;
  background: var(--bg-tertiary);
}

.target-actions {
  display: flex;
  align-items: center;
  gap: 8px;
}

.default-radio {
  display: flex;
  align-items: center;
  gap: 4px;
  font-size: 10px;
  color: var(--text-tertiary);
  cursor: pointer;
}

.remove-btn {
  background: none;
  border: none;
  color: var(--text-tertiary);
  cursor: pointer;
  font-size: 16px;
  padding: 2px 6px;
  border-radius: 4px;
}

.remove-btn:hover {
  background: color-mix(in srgb, var(--danger) 20%, transparent);
  color: var(--danger);
}

.field-row {
  display: flex;
  gap: 10px;
}

.field-row .field { flex: 1; }

.add-target-btn {
  padding: 8px;
  font-size: 11px;
  background: var(--accent);
  border: 1px solid var(--border);
  border-radius: 6px;
  color: var(--text-secondary);
  cursor: pointer;
  text-align: center;
}

.add-target-btn:hover {
  background: var(--bg-tertiary);
  color: var(--text-primary);
}

/* Editors */
.editor-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 8px 10px;
  border: 1px solid var(--border);
  border-radius: 6px;
}

.editor-toggle {
  display: flex;
  align-items: center;
  gap: 8px;
  cursor: pointer;
  font-size: 12px;
}

.editor-name {
  color: var(--text-primary);
  font-weight: 500;
}

.editor-app {
  color: var(--text-tertiary);
  font-family: 'SF Mono', monospace;
  font-size: 10px;
}

.editor-actions {
  display: flex;
  align-items: center;
  gap: 8px;
}

.add-editor-form {
  display: flex;
  gap: 6px;
  margin-top: 4px;
}

.add-editor-form input {
  flex: 1;
  padding: 6px 10px;
  font-size: 11px;
  background: var(--bg-tertiary);
  border: 1px solid var(--border);
  border-radius: 6px;
  color: var(--text-primary);
}

/* Connections */
.config-path {
  margin-top: 16px;
  font-size: 10px;
  color: var(--text-tertiary);
}

.config-path code {
  font-family: 'SF Mono', monospace;
  background: var(--kbd-bg);
  padding: 2px 6px;
  border-radius: 3px;
  font-size: 9px;
}

/* Footer */
.settings-footer {
  display: flex;
  align-items: center;
  justify-content: flex-end;
  gap: 8px;
  padding: 12px 20px;
  border-top: 1px solid var(--border);
}

.save-msg {
  margin-right: auto;
  font-size: 11px;
  color: var(--success);
}

.save-msg.error { color: var(--danger); }

.cancel-btn {
  padding: 6px 14px;
  font-size: 12px;
  background: var(--accent);
  border: 1px solid var(--border);
  border-radius: 6px;
  color: var(--text-secondary);
  cursor: pointer;
}

.cancel-btn:hover { background: var(--bg-tertiary); }

.save-btn {
  padding: 6px 14px;
  font-size: 12px;
  background: var(--selection-bg);
  border: none;
  border-radius: 6px;
  color: #fff;
  cursor: pointer;
  font-weight: 500;
}

.save-btn:hover { opacity: 0.9; }
.save-btn:disabled { opacity: 0.5; cursor: not-allowed; }

/* Transitions */
.modal-enter-active, .modal-leave-active {
  transition: all 0.2s var(--ease-out-expo);
}
.modal-enter-from, .modal-leave-to {
  opacity: 0;
}
.modal-enter-from .settings-modal {
  transform: scale(0.96);
  opacity: 0;
}
.modal-leave-to .settings-modal {
  transform: scale(0.98);
  opacity: 0;
}
</style>
