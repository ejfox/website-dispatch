import { ref, computed } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { useIntervalFn } from '@vueuse/core'
import type { AppConfig, GitStatus } from '../types'

// Environment/vault state composables, grouped: app config (singleton),
// service connection checks, and git status polling.

// ── App config (singleton state, shared across all callers) ─────────────────
const appConfig = ref<AppConfig | null>(null)
const configLoading = ref(false)
const configLoaded = ref(false)

async function fetchConfig() {
  if (configLoading.value) return
  configLoading.value = true
  try {
    appConfig.value = (await invoke('get_app_config')) as AppConfig
    configLoaded.value = true
  } catch (e) {
    console.error('Failed to load config:', e)
  }
  configLoading.value = false
}

async function saveConfig(config: AppConfig) {
  await invoke('save_app_config', { config })
  appConfig.value = config
}

export function useAppConfig() {
  // Auto-fetch on first use
  if (!configLoaded.value && !configLoading.value) {
    fetchConfig()
  }

  const enabledEditors = computed(() => (appConfig.value?.editors || []).filter((e) => e.enabled))
  const publishTargets = computed(() => appConfig.value?.publish_targets || [])
  const hasMultipleTargets = computed(() => publishTargets.value.length > 1)
  const defaultEditor = computed(() => appConfig.value?.default_editor || 'iA Writer')

  return {
    appConfig,
    loading: configLoading,
    loaded: configLoaded,
    enabledEditors,
    publishTargets,
    hasMultipleTargets,
    defaultEditor,
    fetchConfig,
    saveConfig,
  }
}

// ── Service connection status ───────────────────────────────────────────────
export function useConnectionStatus() {
  const cloudinaryConnected = ref(false)
  const obsidianConnected = ref(false)
  const analyticsConnected = ref(false)
  const companionUrl = ref<string | null>(null)
  const companionPin = ref('')
  const gitBranch = ref<string | null>(null)

  async function checkAll() {
    invoke('check_cloudinary_status')
      .then((connected: unknown) => {
        cloudinaryConnected.value = connected as boolean
      })
      .catch(() => {
        cloudinaryConnected.value = false
      })

    invoke('check_obsidian_api')
      .then((connected: unknown) => {
        obsidianConnected.value = connected as boolean
      })
      .catch(() => {
        obsidianConnected.value = false
      })

    invoke('check_analytics_status')
      .then((connected: unknown) => {
        analyticsConnected.value = connected as boolean
      })
      .catch(() => {
        analyticsConnected.value = false
      })

    invoke('get_companion_info')
      .then((info: any) => {
        companionUrl.value = info.url
        companionPin.value = info.pin
      })
      .catch(() => {})

    invoke('get_git_status')
      .then((status: any) => {
        if (status?.ok) gitBranch.value = status.branch
      })
      .catch(() => {})
  }

  // Auto-check on creation
  checkAll()

  return {
    cloudinaryConnected,
    obsidianConnected,
    analyticsConnected,
    companionUrl,
    companionPin,
    gitBranch,
    checkAll,
  }
}

// ── Git status (polled) ─────────────────────────────────────────────────────
export function useGitStatus(intervalMs = 10000) {
  const gitStatus = ref<GitStatus | null>(null)

  async function checkGitStatus() {
    try {
      gitStatus.value = (await invoke('get_git_status')) as GitStatus
    } catch (e) {
      console.error('Git status check failed:', e)
    }
  }

  // Initial check + auto-polling with lifecycle cleanup
  checkGitStatus()
  useIntervalFn(checkGitStatus, intervalMs)

  return { gitStatus, checkGitStatus }
}
