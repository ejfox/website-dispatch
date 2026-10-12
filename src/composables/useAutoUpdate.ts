import { check, type Update } from '@tauri-apps/plugin-updater'
import { relaunch } from '@tauri-apps/plugin-process'
import { useToasts } from './useUiState'

/**
 * Check GitHub Releases for a newer Dispatch build and surface a sticky
 * toast when one's available. The user clicks "Install & Restart" — we
 * download, apply, and relaunch.
 *
 * Silent on no-update / network errors. Idempotent — safe to call from
 * onMounted; the updater plugin caches a single in-flight request.
 * `manual` (Dispatch › Check for Updates…) reports those outcomes too, so
 * the menu item never looks dead.
 */
export async function checkForUpdate({ manual = false }: { manual?: boolean } = {}) {
  const toasts = useToasts()
  let update: Update | null = null
  try {
    update = await check()
  } catch (e) {
    // Common, expected: offline, GitHub rate-limited, no signed releases yet.
    console.debug('[updater] check failed:', e)
    if (manual) toasts.error('Couldn’t check for updates', String(e))
    return
  }
  if (!update) {
    if (manual) toasts.success('Dispatch is up to date')
    return
  }

  const id = toasts.push({
    kind: 'info',
    message: `Dispatch ${update.version} available`,
    detail: update.body || undefined,
    ttl: 0, // sticky until user acts
    action: {
      label: 'Install & Restart',
      run: async () => {
        toasts.dismiss(id)
        const downloading = toasts.info('Downloading update…')
        try {
          await update!.downloadAndInstall((progress) => {
            // Could surface percent; for now just trust the toast.
            console.debug('[updater]', progress)
          })
          toasts.dismiss(downloading)
          await relaunch()
        } catch (err) {
          toasts.dismiss(downloading)
          toasts.error('Update failed', String(err))
        }
      },
    },
  })
}
