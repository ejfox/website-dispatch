//! Watches the Obsidian vault for changes and emits a `vault-changed`
//! event so the frontend can auto-refresh without a manual ⌘R.
//!
//! Uses notify + notify-debouncer-mini so a write-then-rename save (which
//! triggers multiple raw events) collapses to a single notification.

use notify_debouncer_mini::{new_debouncer, notify::RecursiveMode};
use std::path::Path;
use std::sync::mpsc::channel;
use std::time::Duration;
use tauri::{AppHandle, Emitter};

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

        // Loop forever, emitting events. Filter out non-markdown changes
        // (Obsidian writes a lot of .obsidian/ workspace state we don't
        // care about).
        for result in rx {
            match result {
                Ok(events) => {
                    let any_md = events.iter().any(|ev| {
                        ev.path
                            .extension()
                            .and_then(|s| s.to_str())
                            .map(|s| s.eq_ignore_ascii_case("md"))
                            .unwrap_or(false)
                    });
                    if !any_md {
                        continue;
                    }
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
