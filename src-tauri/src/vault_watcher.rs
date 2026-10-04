//! Watches the Obsidian vault for changes and emits a `vault-changed`
//! event so the frontend can auto-refresh without a manual ⌘R.
//!
//! Uses notify + notify-debouncer-mini so a write-then-rename save (which
//! triggers multiple raw events) collapses to a single notification.

use notify_debouncer_mini::{new_debouncer, notify::RecursiveMode};
use std::path::Path;
use std::sync::mpsc::channel;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};

/// Hard floor between `vault-changed` emits. The vault lives on iCloud, which
/// continuously re-touches synced `.md` files in the background — without this,
/// every sync tick fired a full ~180-file rescan (get_recent_files) plus a
/// sketchybar cache rebuild, pegging the CPU (a debug build hit ~167%). One
/// rescan every few seconds is plenty for auto-refresh.
const MIN_EMIT_INTERVAL: Duration = Duration::from_secs(5);

/// True only for a real content change we care about: a `.md` file that isn't
/// inside a hidden/noise directory. Obsidian rewrites `.obsidian/workspace.json`
/// on every cursor move, `.git`/`.trash`/`.smart-env` churn constantly, and
/// iCloud scatters dot-prefixed placeholder files — none of which should trigger
/// a rescan.
fn is_relevant(path: &Path) -> bool {
    let is_md = path
        .extension()
        .and_then(|s| s.to_str())
        .map(|s| s.eq_ignore_ascii_case("md"))
        .unwrap_or(false);
    if !is_md {
        return false;
    }
    // Reject if any path segment is hidden (starts with '.') — covers
    // .obsidian, .git, .trash, .smart-env, and iCloud placeholders.
    !path.components().any(|c| {
        c.as_os_str()
            .to_str()
            .map(|s| s.starts_with('.'))
            .unwrap_or(false)
    })
}

/// Spawn the watcher on a background thread. Safe to call multiple times
/// (the first call wins; subsequent calls are no-ops via OnceLock).
pub fn start(app: AppHandle, vault_path: String) {
    use std::sync::OnceLock;
    static STARTED: OnceLock<bool> = OnceLock::new();
    if STARTED.set(true).is_err() {
        return;
    }

    if vault_path.is_empty() || !Path::new(&vault_path).exists() {
        log::warn!(
            "vault_watcher: vault_path missing ({:?}); not starting",
            vault_path
        );
        return;
    }

    log::info!("vault_watcher: watching {}", vault_path);

    std::thread::spawn(move || {
        let (tx, rx) = channel();
        // 500ms debounce — saves in iA Writer / Obsidian fire several
        // events; we want one notification per logical save.
        let mut debouncer = match new_debouncer(Duration::from_millis(500), tx) {
            Ok(d) => d,
            Err(e) => {
                log::warn!("vault_watcher: failed to create debouncer: {}", e);
                return;
            }
        };

        if let Err(e) = debouncer
            .watcher()
            .watch(Path::new(&vault_path), RecursiveMode::Recursive)
        {
            log::warn!("vault_watcher: watch failed: {}", e);
            return;
        }

        // Loop forever. Emit only for real content changes, and never more
        // than once per MIN_EMIT_INTERVAL — iCloud's background sync re-touches
        // `.md` files constantly, and each emit costs a full vault rescan on
        // the frontend, so an unthrottled watcher storms the CPU.
        let mut last_emit: Option<Instant> = None;
        for result in rx {
            match result {
                Ok(events) => {
                    if !events.iter().any(|ev| is_relevant(&ev.path)) {
                        continue;
                    }
                    if let Some(t) = last_emit {
                        if t.elapsed() < MIN_EMIT_INTERVAL {
                            continue; // coalesce a sync storm into silence
                        }
                    }
                    last_emit = Some(Instant::now());
                    log::info!(
                        "vault_watcher: {} event(s), emitting vault-changed",
                        events.len()
                    );
                    let _ = app.emit("vault-changed", ());
                    // Refresh the ambient cache too so sketchybar reflects
                    // the new state without waiting on its 120s tick.
                    crate::sketchybar_cache::update();
                }
                Err(e) => log::warn!("vault_watcher: debouncer error: {:?}", e),
            }
        }
    });
}
