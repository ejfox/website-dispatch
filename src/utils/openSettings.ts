import { invoke } from '@tauri-apps/api/core'
import type { SettingsPane } from '../types'

/**
 * Open (or focus) the Settings window, optionally on one pane. The one way
 * the main window opens Settings. Equivalent ways in from elsewhere:
 *
 *   - emit the `menu:settings` event (payload: optional pane id) from Rust or
 *     any webview; src-tauri/src/settings.rs `register()` listens for it.
 *     The App menu's "Settings… ⌘," should emit this.
 *   - Rust: `settings::open_window(&app, Some("media"))`.
 */
export const OPEN_SETTINGS_EVENT = 'menu:settings'

export async function openSettings(pane?: SettingsPane): Promise<void> {
  try {
    await invoke('open_settings_window', { pane: pane ?? null })
  } catch (e) {
    console.error('Could not open Settings', e)
  }
}
