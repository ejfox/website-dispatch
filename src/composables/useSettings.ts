import { ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import type { AppConfig, EnvReport, SettingsSnapshot } from '../types'

// State for the Settings window. One instance per window (module singleton).
//
// Changes apply immediately, like a native Mac Settings window: every control
// calls `commit()` once its value is final (a toggle at once, a text field on
// Return or when it loses focus). There is no Save or Cancel.
//
// Secrets arrive masked ("••••1234"). Sending a masked value back keeps the
// stored secret (config.rs `merge_secrets`); the backend returns the saved
// config masked again, so a just-typed key never lingers in the webview.

const config = ref<AppConfig | null>(null)
const env = ref<EnvReport | null>(null)
const configPath = ref('')
const configExists = ref(false)
const loadError = ref<string | null>(null)
const saveError = ref<string | null>(null)
const saving = ref(false)

/** Missing optional sections get an empty shape so the panes can bind to them. */
export function ensureShape(c: AppConfig): AppConfig {
  if (!c.media) c.media = { primary: 'cloudinary', mirrors: [], cloudinary: null, r2: null }
  if (!c.media.mirrors) c.media.mirrors = []
  if (!c.media.cloudinary) c.media.cloudinary = { cloud_name: '', api_key: '', api_secret: '' }
  if (!c.media.r2) {
    c.media.r2 = { account_id: '', access_key_id: '', secret_access_key: '', bucket: '', public_url_base: '' }
  }
  return c
}

async function load() {
  loadError.value = null
  try {
    const snap = (await invoke('get_settings')) as SettingsSnapshot
    config.value = ensureShape(snap.config)
    env.value = snap.env
    configPath.value = snap.config_path
    configExists.value = snap.config_exists
  } catch (e) {
    loadError.value = String(e)
  }
}

// Saves are serialized: a commit during a save runs once more afterwards with
// the latest state, so the last change always wins and none interleave.
let pending = false
async function commit() {
  if (!config.value) return
  if (saving.value) {
    pending = true
    return
  }
  saving.value = true
  try {
    const saved = (await invoke('save_settings', { configData: config.value })) as AppConfig | null
    saveError.value = null
    configExists.value = true
    // Re-mask secrets that were just typed; leave every other field alone so
    // a value being edited elsewhere isn't clobbered.
    if (saved?.media && config.value.media) {
      const c = config.value.media
      if (saved.media.cloudinary && c.cloudinary) {
        c.cloudinary.api_key = saved.media.cloudinary.api_key
        c.cloudinary.api_secret = saved.media.cloudinary.api_secret
      }
      if (saved.media.r2 && c.r2) {
        c.r2.access_key_id = saved.media.r2.access_key_id
        c.r2.secret_access_key = saved.media.r2.secret_access_key
      }
    }
  } catch (e) {
    saveError.value = `Couldn't save: ${e}`
  } finally {
    saving.value = false
  }
  if (pending) {
    pending = false
    await commit()
  }
}

export function useSettings() {
  return { config, env, configPath, configExists, loadError, saveError, saving, load, commit }
}

/** True for a value the backend masked ("••••" + last 4). */
export function isMasked(v: string | null | undefined): boolean {
  return !!v && v.startsWith('••••')
}
