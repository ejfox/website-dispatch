import { ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'

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
