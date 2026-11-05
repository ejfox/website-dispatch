// The Desk is chart-desk's front end. Without the `desk` command it has nothing
// to show, so everything that leads to it stays out of the way.

import { ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'

export const deskAvailable = ref(false)

export async function checkDesk(): Promise<boolean> {
  deskAvailable.value = await invoke<boolean>('desk_available').catch(() => false)
  return deskAvailable.value
}
