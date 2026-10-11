use chrono::{Datelike, Local, Timelike, Utc};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JournalEntry {
    pub id: i64,
    pub timestamp: String,
    pub event: String, // "publish", "republish", "unpublish"
    pub slug: String,
    pub title: Option<String>,
    pub word_count: i64,
    pub tags: String, // comma-separated
    pub content_type: String,
    pub url: Option<String>,
    pub target_id: Option<String>,
    pub visibility: String, // "public", "unlisted", "protected"
}

// ---------------------------------------------------------------------------
// Database singleton
// ---------------------------------------------------------------------------

static DB: OnceLock<Result<Mutex<Connection>, String>> = OnceLock::new();

fn db_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_default();
    PathBuf::from(home)
        .join("Library/Application Support/com.ejfox.dispatch")
        .join("journal.db")
}

fn init_db() -> Result<Mutex<Connection>, String> {
    let path = db_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    // First attempt: open existing DB.
    match try_open(&path) {
        Ok(mutex) => Ok(mutex),
        Err(first_err) => {
            // The file is corrupt or schema-incompatible. Rather than abort,
            // rename it aside and start fresh — the user's publishing history
            // is regenerable from git, so losing it is recoverable.
            log::warn!(
                "Journal DB unhealthy ({}); quarantining and recreating.",
                first_err
            );
            let stamp = chrono::Utc::now().format("%Y%m%dT%H%M%S");
            let quarantine = path.with_file_name(format!("journal.corrupt-{}.db", stamp));
            let _ = std::fs::rename(&path, &quarantine);
            // Also nuke WAL/SHM siblings — leftover ones can re-corrupt the new DB.
            let _ = std::fs::remove_file(path.with_extension("db-wal"));
            let _ = std::fs::remove_file(path.with_extension("db-shm"));
            try_open(&path)
        }
    }
}

fn try_open(path: &Path) -> Result<Mutex<Connection>, String> {
    let conn =
        Connection::open(path).map_err(|e| format!("Failed to open journal database: {}", e))?;
    conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")
        .map_err(|e| format!("Failed to set pragmas: {}", e))?;
    init_schema(&conn)?;
    // Light integrity probe: a corrupt DB will error here even if open() succeeded.
    conn.query_row("PRAGMA integrity_check", [], |row| row.get::<_, String>(0))
        .map_err(|e| format!("integrity_check failed: {}", e))
        .and_then(|s| {
            if s == "ok" {
                Ok(())
            } else {
                Err(format!("integrity_check: {}", s))
            }
        })?;
    Ok(Mutex::new(conn))
}

fn get_db() -> Result<&'static Mutex<Connection>, String> {
    DB.get_or_init(init_db).as_ref().map_err(|e| e.clone())
}

fn init_schema(conn: &Connection) -> Result<(), String> {
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS events (
            id          INTEGER PRIMARY KEY AUTOINCREMENT,
            timestamp   TEXT NOT NULL,
            local_date  TEXT NOT NULL,
            local_hour  INTEGER NOT NULL,
            day_of_week INTEGER NOT NULL,
            event       TEXT NOT NULL,
            slug        TEXT NOT NULL,
            title       TEXT,
            word_count  INTEGER NOT NULL DEFAULT 0,
            tags        TEXT NOT NULL DEFAULT '',
            content_type TEXT NOT NULL DEFAULT 'post',
            url         TEXT,
            target_id   TEXT,
            visibility  TEXT NOT NULL DEFAULT 'public'
        );

        CREATE INDEX IF NOT EXISTS idx_events_local_date ON events(local_date);
        CREATE INDEX IF NOT EXISTS idx_events_event ON events(event);
        CREATE INDEX IF NOT EXISTS idx_events_slug ON events(slug);
        ",
    )
    .map_err(|e| format!("Failed to create journal schema: {}", e))
}

// ---------------------------------------------------------------------------
// Record events
// ---------------------------------------------------------------------------

pub struct EventRecord<'a> {
    pub event: &'a str,
    pub slug: &'a str,
    pub title: Option<&'a str>,
    pub word_count: usize,
    pub tags: &'a [String],
    pub content_type: &'a str,
    pub url: Option<&'a str>,
    pub target_id: Option<&'a str>,
    pub visibility: &'a str,
}

pub fn record_event(rec: EventRecord<'_>) -> Result<i64, String> {
    let now = Utc::now();
    let local = Local::now();
    let timestamp = now.to_rfc3339();
    let local_date = local.format("%Y-%m-%d").to_string();
    let local_hour = local.hour() as i32;
    let day_of_week = local.weekday().num_days_from_monday() as i32;
    let tags_str = rec.tags.join(",");

    let db = get_db()?.lock().map_err(|e| format!("DB lock: {}", e))?;
    db.execute(
        "INSERT INTO events (timestamp, local_date, local_hour, day_of_week, event, slug, title, word_count, tags, content_type, url, target_id, visibility)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
        params![
            timestamp,
            local_date,
            local_hour,
            day_of_week,
            rec.event,
            rec.slug,
            rec.title,
            rec.word_count as i64,
            tags_str,
            rec.content_type,
            rec.url,
            rec.target_id,
            rec.visibility,
        ],
    )
    .map_err(|e| format!("Failed to record event: {}", e))?;

    Ok(db.last_insert_rowid())
}

// ---------------------------------------------------------------------------
// Read entries
// ---------------------------------------------------------------------------

pub fn get_recent_entries(limit: usize) -> Result<Vec<JournalEntry>, String> {
    let db = get_db()?.lock().map_err(|e| format!("DB lock: {}", e))?;
    let mut stmt = db
        .prepare(
            "SELECT id, timestamp, event, slug, title, word_count, tags, content_type, url, target_id, visibility
             FROM events ORDER BY id DESC LIMIT ?1",
        )
        .map_err(|e| format!("Query error: {}", e))?;

    let rows = stmt
        .query_map(params![limit as i64], |row| {
            Ok(JournalEntry {
                id: row.get(0)?,
                timestamp: row.get(1)?,
                event: row.get(2)?,
                slug: row.get(3)?,
                title: row.get(4)?,
                word_count: row.get(5)?,
                tags: row.get(6)?,
                content_type: row.get(7)?,
                url: row.get(8)?,
                target_id: row.get(9)?,
                visibility: row.get(10)?,
            })
        })
        .map_err(|e| format!("Query error: {}", e))?;

    let mut entries = Vec::new();
    for row in rows {
        entries.push(row.map_err(|e| format!("Row error: {}", e))?);
    }
    Ok(entries)
}

/// Returns a map of "YYYY-MM-DD" -> publish count for the last `days` days.
pub fn get_heatmap(days: u32) -> Result<Vec<(String, u32)>, String> {
    let db = get_db()?.lock().map_err(|e| format!("DB lock: {}", e))?;
    let cutoff = Local::now() - chrono::Duration::days(days as i64);
    let cutoff_str = cutoff.format("%Y-%m-%d").to_string();

    let mut stmt = db
        .prepare(
            "SELECT local_date, COUNT(*) FROM events
             WHERE event IN ('publish', 'republish') AND local_date >= ?1
             GROUP BY local_date ORDER BY local_date ASC",
        )
        .map_err(|e| format!("Query error: {}", e))?;

    let rows = stmt
        .query_map(params![cutoff_str], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, u32>(1)?))
        })
        .map_err(|e| format!("Query error: {}", e))?;

    let mut result = Vec::new();
    for row in rows {
        result.push(row.map_err(|e| format!("Row error: {}", e))?);
    }
    Ok(result)
}

// ---------------------------------------------------------------------------
// Backfill from git history (Phase 6 — called once if DB is empty)
// ---------------------------------------------------------------------------

pub fn backfill_from_git(repo_path: &str, domain: &str) -> Result<u32, String> {
    // Most posts are added via `yarn blog:process` without a dedicated
    // "Publish:" commit, so parsing git log alone yields almost nothing.
    // The processed manifest is the authoritative list of what's actually
    // live, so we backfill from there. Idempotent: skips slugs already in
    // the events table.
    let mut inserted = backfill_from_manifest(repo_path, domain).unwrap_or(0);

    let db = get_db()?.lock().map_err(|e| format!("DB lock: {}", e))?;

    // Also pick up any explicit "Publish: SLUG" commits not represented in
    // the manifest (e.g. unlisted posts processed outside the main flow).
    let output = std::process::Command::new(crate::bin_paths::git())
        .args(["log", "--oneline", "--format=%aI %s", "--all"])
        .current_dir(repo_path)
        .output()
        .map_err(|e| format!("git log failed: {}", e))?;

    let stdout = String::from_utf8_lossy(&output.stdout);

    for line in stdout.lines().rev() {
        if let Some(cap) = crate::patterns::GIT_PUBLISH_LOG.captures(line) {
            let ts = &cap[1];
            let slug = cap[2].trim();

            let exists: i64 = db
                .query_row(
                    "SELECT COUNT(*) FROM events WHERE slug = ?1 AND event = 'publish'",
                    params![slug],
                    |r| r.get(0),
                )
                .unwrap_or(0);
            if exists > 0 {
                continue;
            }

            if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(ts) {
                let local = dt.with_timezone(&Local);
                let local_date = local.format("%Y-%m-%d").to_string();
                let local_hour = local.hour() as i32;
                let day_of_week = local.weekday().num_days_from_monday() as i32;
                let url = format!("{}/blog/{}/{}", domain, local.year(), slug);

                let _ = db.execute(
                    "INSERT INTO events (timestamp, local_date, local_hour, day_of_week, event, slug, title, word_count, tags, content_type, url, target_id, visibility)
                     VALUES (?1, ?2, ?3, ?4, 'publish', ?5, NULL, 0, '', 'post', ?6, NULL, 'public')",
                    params![ts, local_date, local_hour, day_of_week, slug, url],
                );
                inserted += 1;
            }
        }
    }

    Ok(inserted)
}

/// Backfill publish events from website2's processed manifest. This is the
/// real source of truth — each entry there is a post that actually shipped.
fn backfill_from_manifest(repo_path: &str, domain: &str) -> Result<u32, String> {
    use serde_json::Value;

    let manifest_path = Path::new(repo_path).join("content/processed/manifest-lite.json");
    let raw = std::fs::read_to_string(&manifest_path)
        .map_err(|e| format!("read manifest: {}", e))?;
    let entries: Vec<Value> =
        serde_json::from_str(&raw).map_err(|e| format!("parse manifest: {}", e))?;

    let db = get_db()?.lock().map_err(|e| format!("DB lock: {}", e))?;
    let mut inserted = 0u32;

    for entry in &entries {
        let slug = entry.get("slug").and_then(|v| v.as_str()).unwrap_or("");
        if slug.is_empty() {
            continue;
        }
        // Skip hidden/draft posts.
        if entry.get("hidden").and_then(|v| v.as_bool()).unwrap_or(false) {
            continue;
        }

        let exists: i64 = db
            .query_row(
                "SELECT COUNT(*) FROM events WHERE slug = ?1 AND event = 'publish'",
                params![slug],
                |r| r.get(0),
            )
            .unwrap_or(0);
        if exists > 0 {
            continue;
        }

        let date_str = entry.get("date").and_then(|v| v.as_str()).unwrap_or("");
        let dt = chrono::DateTime::parse_from_rfc3339(date_str)
            .or_else(|_| {
                // Some dates are bare YYYY-MM-DD or end in Z without subseconds
                chrono::DateTime::parse_from_rfc3339(&format!("{}T10:00:00+00:00", date_str))
            })
            .ok();
        let Some(dt) = dt else { continue };

        let local = dt.with_timezone(&Local);
        let local_date = local.format("%Y-%m-%d").to_string();
        let local_hour = local.hour() as i32;
        let day_of_week = local.weekday().num_days_from_monday() as i32;
        let ts = dt.to_rfc3339();
        let url = format!("{}/blog/{}", domain.trim_end_matches('/'), slug);
        let title = entry.get("title").and_then(|v| v.as_str()).unwrap_or("");
        let word_count = entry
            .pointer("/metadata/words")
            .and_then(|v| v.as_i64())
            .unwrap_or(0);
        let content_type = entry
            .get("type")
            .and_then(|v| v.as_str())
            .unwrap_or("post");
        let tags = entry
            .get("tags")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str())
                    .collect::<Vec<_>>()
                    .join(",")
            })
            .unwrap_or_default();

        let _ = db.execute(
            "INSERT INTO events (timestamp, local_date, local_hour, day_of_week, event, slug, title, word_count, tags, content_type, url, target_id, visibility)
             VALUES (?1, ?2, ?3, ?4, 'publish', ?5, ?6, ?7, ?8, ?9, ?10, NULL, 'public')",
            params![ts, local_date, local_hour, day_of_week, slug, title, word_count, tags, content_type, url],
        );
        inserted += 1;
    }

    Ok(inserted)
}
