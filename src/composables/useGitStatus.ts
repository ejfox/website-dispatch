import { ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { useIntervalFn } from '@vueuse/core'
import type { GitStatus } from '../types'

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
