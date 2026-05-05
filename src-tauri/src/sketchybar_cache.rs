//! Writes a tiny JSON snapshot to a known path so sketchybar (or any
//! other ambient surface) can render Dispatch state without spinning up
//! an HTTP request or talking to the running app.
//!
//! Cache lives at `~/Library/Caches/com.ejfox.dispatch/sketchybar.json`
//! and is rewritten after every vault scan, publish, or unpublish.
//! Sketchybar plugins poll this file (essentially free — local disk read)
//! every minute or two; missing/stale file just means "fall back to the
//! older RSS-based heuristic."

use serde::Serialize;
use std::path::PathBuf;
use std::sync::OnceLock;

#[derive(Serialize)]
struct SketchybarState {
    /// Local files in `blog/` modified in last ~21d that aren't in RSS yet.
    pending: usize,
    /// Days of consecutive publishing per the journal DB.
    streak_days: u32,
    /// Any LIVE post has been edited locally without republishing.
    has_modified: bool,
    /// Total LIVE posts (handy for "Live 62" widgets).
    live_count: usize,
    /// Total drafts (anything in blog/ without a published_url).
    draft_count: usize,
    /// Unix timestamp of the most recent publish event.
    last_publish_ts: Option<i64>,
    /// When this cache was written (for staleness checks if anyone cares).
    written_at: String,
}

fn cache_path() -> &'static PathBuf {
    static PATH: OnceLock<PathBuf> = OnceLock::new();
    PATH.get_or_init(|| {
        let home = std::env::var("HOME").unwrap_or_default();
        PathBuf::from(home)
            .join("Library/Caches/com.ejfox.dispatch")
            .join("sketchybar.json")
    })
}

/// Recompute and persist the cache. Cheap (fraction of a second on EJ's vault),
/// safe to call on every relevant event. Errors are logged and swallowed —
/// missing cache is non-fatal for the app.
pub fn update() {
    let path = cache_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    // Pull files from the vault. We re-scan rather than depend on the
    // frontend's cached list — this function may be called from background
    // threads (publish completion, schedule trigger) where no frontend state
    // is available.
    let files = match crate::vault::get_recent_files(10_000) {
        Ok(f) => f,
        Err(e) => {
            log::warn!("sketchybar_cache: vault scan failed: {}", e);
            return;
        }
    };

    let live_count = files.iter().filter(|f| f.published_url.is_some()).count();
    let draft_count = files.len().saturating_sub(live_count);
    let has_modified = files
        .iter()
        .any(|f| f.warnings.iter().any(|w| w == "Modified since publish"));

    // "Pending" mirrors the older RSS-based heuristic but locally: posts
    // modified in the last 21 days that aren't yet live.
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let cutoff = now.saturating_sub(21 * 86_400);
    let pending = files
        .iter()
        .filter(|f| f.published_url.is_none() && f.modified >= cutoff)
        .count();

    // Streak + last publish from the journal DB.
    let (streak_days, last_publish_ts) = crate::journal::get_stats()
        .ok()
        .map(|s| {
            let last_ts = s.last_publish_at.as_ref().and_then(|iso| {
                chrono::DateTime::parse_from_rfc3339(iso)
                    .ok()
                    .map(|dt| dt.timestamp())
            });
            (s.current_streak_days, last_ts)
        })
        .unwrap_or((0, None));

    let state = SketchybarState {
        pending,
        streak_days,
        has_modified,
        live_count,
        draft_count,
        last_publish_ts,
        written_at: chrono::Utc::now().to_rfc3339(),
    };

    let json = match serde_json::to_string_pretty(&state) {
        Ok(s) => s,
        Err(e) => {
            log::warn!("sketchybar_cache: serialize failed: {}", e);
            return;
        }
    };

    if let Err(e) = std::fs::write(path, json) {
        log::warn!("sketchybar_cache: write failed: {}", e);
    }
}
