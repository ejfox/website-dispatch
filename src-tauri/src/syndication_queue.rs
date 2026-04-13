//! Syndication queue — SQLite-backed scheduled post distribution.
//!
//! Stores pending/scheduled posts for each platform, runs a background
//! scheduler that sends them at the right time, retries failures.

use crate::syndication::{self, PostContent, SyndicationResult};
use tauri::Emitter;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueueItem {
    pub id: i64,
    pub post_slug: String,
    pub post_title: String,
    pub post_url: String,
    pub platform: String,
    pub status: String,
    pub platform_text: String,
    pub media_url: Option<String>,
    pub scheduled_at: Option<String>,
    pub sent_at: Option<String>,
    pub platform_url: Option<String>,
    pub error_message: Option<String>,
    pub attempt_count: i32,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewQueueItem {
    pub post_slug: String,
    pub post_title: String,
    pub post_url: String,
    pub platform: String,
    pub platform_text: String,
    pub media_url: Option<String>,
    pub scheduled_at: Option<String>,
}

// ---------------------------------------------------------------------------
// Database
// ---------------------------------------------------------------------------

static DB: OnceLock<Result<Mutex<Connection>, String>> = OnceLock::new();

fn db_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_default();
    PathBuf::from(home)
        .join("Library/Application Support/com.ejfox.dispatch")
        .join("syndication.db")
}

fn init_db() -> Result<Mutex<Connection>, String> {
    let path = db_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let conn = Connection::open(&path)
        .map_err(|e| format!("Failed to open syndication database: {}", e))?;
    conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")
        .map_err(|e| format!("Failed to set pragmas: {}", e))?;
    init_schema(&conn)?;
    Ok(Mutex::new(conn))
}

fn get_db() -> Result<&'static Mutex<Connection>, String> {
    DB.get_or_init(|| init_db())
        .as_ref()
        .map_err(|e| e.clone())
}

fn init_schema(conn: &Connection) -> Result<(), String> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS syndication_queue (
            id              INTEGER PRIMARY KEY AUTOINCREMENT,
            post_slug       TEXT NOT NULL,
            post_title      TEXT NOT NULL,
            post_url        TEXT NOT NULL,
            platform        TEXT NOT NULL,
            status          TEXT NOT NULL DEFAULT 'scheduled',
            platform_text   TEXT NOT NULL,
            media_url       TEXT,
            scheduled_at    TEXT,
            sent_at         TEXT,
            platform_url    TEXT,
            error_message   TEXT,
            attempt_count   INTEGER NOT NULL DEFAULT 0,
            created_at      TEXT NOT NULL,
            updated_at      TEXT NOT NULL
        );
        CREATE INDEX IF NOT EXISTS idx_queue_status ON syndication_queue(status);
        CREATE INDEX IF NOT EXISTS idx_queue_scheduled ON syndication_queue(scheduled_at);
        CREATE INDEX IF NOT EXISTS idx_queue_slug ON syndication_queue(post_slug);",
    )
    .map_err(|e| format!("Failed to create syndication schema: {}", e))?;
    Ok(())
}

fn now_iso() -> String {
    chrono::Utc::now().to_rfc3339()
}

// ---------------------------------------------------------------------------
// Queue operations
// ---------------------------------------------------------------------------

/// Add an item to the queue. Returns the new row ID.
pub fn queue_item(item: &NewQueueItem) -> Result<i64, String> {
    let db = get_db()?.lock().map_err(|e| format!("DB lock: {}", e))?;
    let now = now_iso();
    let status = if item.scheduled_at.is_some() {
        "scheduled"
    } else {
        "pending"
    };
    db.execute(
        "INSERT INTO syndication_queue
         (post_slug, post_title, post_url, platform, status, platform_text, media_url, scheduled_at, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        params![
            item.post_slug,
            item.post_title,
            item.post_url,
            item.platform,
            status,
            item.platform_text,
            item.media_url,
            item.scheduled_at,
            now,
            now,
        ],
    )
    .map_err(|e| format!("Insert failed: {}", e))?;
    Ok(db.last_insert_rowid())
}

/// Queue multiple items at once. Returns their IDs.
pub fn queue_items(items: &[NewQueueItem]) -> Result<Vec<i64>, String> {
    items.iter().map(queue_item).collect()
}

/// Get queue items, optionally filtered by status.
pub fn get_queue(status_filter: Option<&str>, limit: usize) -> Result<Vec<QueueItem>, String> {
    let db = get_db()?.lock().map_err(|e| format!("DB lock: {}", e))?;
    let (sql, filter_params): (String, Vec<Box<dyn rusqlite::types::ToSql>>) = match status_filter {
        Some(status) => (
            format!(
                "SELECT * FROM syndication_queue WHERE status = ?1 ORDER BY scheduled_at ASC, created_at ASC LIMIT {}",
                limit
            ),
            vec![Box::new(status.to_string())],
        ),
        None => (
            format!(
                "SELECT * FROM syndication_queue ORDER BY created_at DESC LIMIT {}",
                limit
            ),
            vec![],
        ),
    };

    let mut stmt = db.prepare(&sql).map_err(|e| format!("Query failed: {}", e))?;
    let rows = stmt
        .query_map(rusqlite::params_from_iter(filter_params.iter()), |row| {
            Ok(QueueItem {
                id: row.get(0)?,
                post_slug: row.get(1)?,
                post_title: row.get(2)?,
                post_url: row.get(3)?,
                platform: row.get(4)?,
                status: row.get(5)?,
                platform_text: row.get(6)?,
                media_url: row.get(7)?,
                scheduled_at: row.get(8)?,
                sent_at: row.get(9)?,
                platform_url: row.get(10)?,
                error_message: row.get(11)?,
                attempt_count: row.get(12)?,
                created_at: row.get(13)?,
                updated_at: row.get(14)?,
            })
        })
        .map_err(|e| format!("Query failed: {}", e))?;

    let mut items = Vec::new();
    for row in rows {
        items.push(row.map_err(|e| format!("Row parse: {}", e))?);
    }
    Ok(items)
}

/// Get items for a specific post slug.
pub fn get_queue_for_post(slug: &str) -> Result<Vec<QueueItem>, String> {
    let db = get_db()?.lock().map_err(|e| format!("DB lock: {}", e))?;
    let mut stmt = db
        .prepare("SELECT * FROM syndication_queue WHERE post_slug = ?1 ORDER BY created_at DESC")
        .map_err(|e| format!("Query: {}", e))?;
    let rows = stmt
        .query_map(params![slug], |row| {
            Ok(QueueItem {
                id: row.get(0)?,
                post_slug: row.get(1)?,
                post_title: row.get(2)?,
                post_url: row.get(3)?,
                platform: row.get(4)?,
                status: row.get(5)?,
                platform_text: row.get(6)?,
                media_url: row.get(7)?,
                scheduled_at: row.get(8)?,
                sent_at: row.get(9)?,
                platform_url: row.get(10)?,
                error_message: row.get(11)?,
                attempt_count: row.get(12)?,
                created_at: row.get(13)?,
                updated_at: row.get(14)?,
            })
        })
        .map_err(|e| format!("Query: {}", e))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("Row parse: {}", e))
}

/// Update a queue item's text, schedule, or media.
pub fn update_item(
    id: i64,
    platform_text: Option<&str>,
    scheduled_at: Option<&str>,
    media_url: Option<&str>,
) -> Result<(), String> {
    let db = get_db()?.lock().map_err(|e| format!("DB lock: {}", e))?;
    let now = now_iso();

    if let Some(text) = platform_text {
        db.execute(
            "UPDATE syndication_queue SET platform_text = ?1, updated_at = ?2 WHERE id = ?3",
            params![text, now, id],
        )
        .map_err(|e| format!("Update text: {}", e))?;
    }
    if let Some(sched) = scheduled_at {
        db.execute(
            "UPDATE syndication_queue SET scheduled_at = ?1, status = 'scheduled', updated_at = ?2 WHERE id = ?3",
            params![sched, now, id],
        )
        .map_err(|e| format!("Update schedule: {}", e))?;
    }
    if let Some(media) = media_url {
        db.execute(
            "UPDATE syndication_queue SET media_url = ?1, updated_at = ?2 WHERE id = ?3",
            params![media, now, id],
        )
        .map_err(|e| format!("Update media: {}", e))?;
    }
    Ok(())
}

/// Delete a queue item.
pub fn delete_item(id: i64) -> Result<(), String> {
    let db = get_db()?.lock().map_err(|e| format!("DB lock: {}", e))?;
    db.execute("DELETE FROM syndication_queue WHERE id = ?1", params![id])
        .map_err(|e| format!("Delete: {}", e))?;
    Ok(())
}

/// Mark an item as sent.
fn mark_sent(id: i64, platform_url: &str) -> Result<(), String> {
    let db = get_db()?.lock().map_err(|e| format!("DB lock: {}", e))?;
    let now = now_iso();
    db.execute(
        "UPDATE syndication_queue SET status = 'sent', sent_at = ?1, platform_url = ?2, updated_at = ?3 WHERE id = ?4",
        params![now, platform_url, now, id],
    )
    .map_err(|e| format!("Mark sent: {}", e))?;
    Ok(())
}

/// Mark an item as failed.
fn mark_failed(id: i64, error: &str) -> Result<(), String> {
    let db = get_db()?.lock().map_err(|e| format!("DB lock: {}", e))?;
    let now = now_iso();
    db.execute(
        "UPDATE syndication_queue SET status = 'failed', error_message = ?1, attempt_count = attempt_count + 1, updated_at = ?2 WHERE id = ?3",
        params![error, now, id],
    )
    .map_err(|e| format!("Mark failed: {}", e))?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Send a single queue item
// ---------------------------------------------------------------------------

/// Send a queue item immediately, regardless of schedule. Returns result.
pub fn send_item(id: i64) -> Result<SyndicationResult, String> {
    let items = get_queue(None, 1000)?;
    let item = items
        .iter()
        .find(|i| i.id == id)
        .ok_or_else(|| format!("Queue item {} not found", id))?;

    let _post = PostContent {
        title: item.post_title.clone(),
        url: item.post_url.clone(),
        slug: item.post_slug.clone(),
        tags: vec![], // Tags are already baked into platform_text
        dek: None,
        content_type: "post".into(),
        visibility: "public".into(),
    };

    // Override the composed text with the user-edited platform_text
    let result = match item.platform.as_str() {
        "mastodon" => syndication::post_to_mastodon_with_text(&item.platform_text),
        // "linkedin" => syndication::post_to_linkedin_with_text(&item.platform_text, item.media_url.as_deref()),
        // "instagram" => syndication::post_to_instagram_with_text(&item.platform_text, item.media_url.as_deref().unwrap_or("")),
        _ => SyndicationResult {
            platform: item.platform.clone(),
            success: false,
            url: None,
            error: Some(format!("Platform '{}' not yet supported", item.platform)),
        },
    };

    if result.success {
        let _ = mark_sent(id, result.url.as_deref().unwrap_or(""));
    } else {
        let _ = mark_failed(id, result.error.as_deref().unwrap_or("Unknown error"));
    }

    Ok(result)
}

// ---------------------------------------------------------------------------
// Background scheduler
// ---------------------------------------------------------------------------

/// Runs every 30 seconds, checks for due items, sends them.
pub async fn run_syndication_scheduler(app_handle: tauri::AppHandle) {
    use tokio::time::{interval, Duration};
    let mut ticker = interval(Duration::from_secs(30));

    loop {
        ticker.tick().await;

        // Get items where scheduled_at <= now and status = 'scheduled'
        let due = match get_due_items() {
            Ok(items) => items,
            Err(e) => {
                eprintln!("Syndication scheduler error: {}", e);
                continue;
            }
        };

        for item in due {
            // Skip if too many attempts
            if item.attempt_count >= 3 {
                eprintln!(
                    "Syndication: giving up on {} #{} after {} attempts",
                    item.platform, item.id, item.attempt_count
                );
                continue;
            }

            eprintln!(
                "Syndication: sending {} to {} (scheduled: {:?})",
                item.post_slug, item.platform, item.scheduled_at
            );

            match send_item(item.id) {
                Ok(result) => {
                    let _ = app_handle.emit(
                        "syndication-sent",
                        serde_json::json!({
                            "id": item.id,
                            "platform": item.platform,
                            "slug": item.post_slug,
                            "success": result.success,
                            "url": result.url,
                            "error": result.error,
                        }),
                    );

                    if result.success {
                        eprintln!(
                            "Syndication: {} sent to {} -> {}",
                            item.post_slug,
                            item.platform,
                            result.url.as_deref().unwrap_or("?")
                        );
                    }
                }
                Err(e) => {
                    eprintln!("Syndication: failed to send {} #{}: {}", item.platform, item.id, e);
                }
            }
        }
    }
}

/// Get items that are due to be sent (scheduled_at <= now, status = 'scheduled').
fn get_due_items() -> Result<Vec<QueueItem>, String> {
    let db = get_db()?.lock().map_err(|e| format!("DB lock: {}", e))?;
    let now = now_iso();
    let mut stmt = db
        .prepare(
            "SELECT * FROM syndication_queue
             WHERE status = 'scheduled' AND scheduled_at IS NOT NULL AND scheduled_at <= ?1
             ORDER BY scheduled_at ASC",
        )
        .map_err(|e| format!("Query: {}", e))?;

    let rows = stmt
        .query_map(params![now], |row| {
            Ok(QueueItem {
                id: row.get(0)?,
                post_slug: row.get(1)?,
                post_title: row.get(2)?,
                post_url: row.get(3)?,
                platform: row.get(4)?,
                status: row.get(5)?,
                platform_text: row.get(6)?,
                media_url: row.get(7)?,
                scheduled_at: row.get(8)?,
                sent_at: row.get(9)?,
                platform_url: row.get(10)?,
                error_message: row.get(11)?,
                attempt_count: row.get(12)?,
                created_at: row.get(13)?,
                updated_at: row.get(14)?,
            })
        })
        .map_err(|e| format!("Query: {}", e))?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("Row parse: {}", e))
}
