import { ref, computed } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import type { AppConfig } from '../types'

// Singleton state — shared across all components that call useAppConfig()
const appConfig = ref<AppConfig | null>(null)
const loading = ref(false)
const loaded = ref(false)

async function fetchConfig() {
  if (loading.value) return
  loading.value = true
  try {
    appConfig.value = (await invoke('get_app_config')) as AppConfig
    loaded.value = true
  } catch (e) {
    console.error('Failed to load config:', e)
  }
  loading.value = false
}

async function saveConfig(config: AppConfig) {
  try {
    await invoke('save_app_config', { config })
    appConfig.value = config
  } catch (e) {
    throw e
  }
}

export function useAppConfig() {
  // Auto-fetch on first use
  if (!loaded.value && !loading.value) {
    fetchConfig()
  }

  const enabledEditors = computed(() => (appConfig.value?.editors || []).filter((e) => e.enabled))

  const publishTargets = computed(() => appConfig.value?.publish_targets || [])
  const hasMultipleTargets = computed(() => publishTargets.value.length > 1)
  const defaultEditor = computed(() => appConfig.value?.default_editor || 'iA Writer')

  return {
    appConfig,
    loading,
    loaded,
    enabledEditors,
    publishTargets,
    hasMultipleTargets,
    defaultEditor,
    fetchConfig,
    saveConfig,
  }
}
