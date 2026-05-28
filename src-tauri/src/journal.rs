use chrono::{Datelike, Local, NaiveDate, Timelike, Utc};
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JournalStats {
    // Streaks
    pub current_streak_days: u32,
    pub longest_streak_days: u32,
    pub current_streak_start: Option<String>,
    pub current_weekly_streak: u32,
    pub longest_weekly_streak: u32,

    // Totals
    pub total_publishes: u32,
    pub total_republishes: u32,
    pub total_unpublishes: u32,
    pub total_words_published: i64,
    pub unique_posts_published: u32,

    // Time windows
    pub publishes_this_week: u32,
    pub publishes_last_week: u32,
    pub words_this_week: i64,
    pub words_last_week: i64,
    pub publishes_this_month: u32,
    pub words_this_month: i64,
    pub words_last_month: i64,

    // Monthly history (last 6 months, newest first): [{ month: "2026-03", words: 12345, posts: 5 }]
    pub monthly_history: Vec<MonthStat>,

    // Averages
    pub avg_publishes_per_week: f64,
    pub avg_words_per_post: f64,

    // Rhythm
    pub most_active_hour: Option<u8>,
    pub most_active_day_of_week: Option<String>,
    pub publish_hour_distribution: Vec<u32>,

    // Milestones
    pub milestones: Vec<Milestone>,

    // Recency
    pub last_publish_at: Option<String>,
    pub days_since_last_publish: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonthStat {
    pub month: String, // "2026-03"
    pub words: i64,
    pub posts: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Milestone {
    pub id: String,
    pub label: String,
    pub description: String,
    pub achieved_at: Option<String>,
    /// Where you are right now (e.g. 5 of 7 days into a streak milestone).
    /// Used by the frontend to draw real progress bars on locked entries
    /// instead of a generic "locked" pill. For achieved ones this equals
    /// `target` (or whatever the high-water mark is).
    pub current: i64,
    pub target: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Nudge {
    pub message: String,
    pub kind: String, // "streak", "encouragement", "reminder", "celebration"
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
// Stats computation
// ---------------------------------------------------------------------------

pub fn get_stats() -> Result<JournalStats, String> {
    let db = get_db()?.lock().map_err(|e| format!("DB lock: {}", e))?;

    // Total counts by event type
    let total_publishes: u32 = db
        .query_row(
            "SELECT COUNT(*) FROM events WHERE event = 'publish'",
            [],
            |r| r.get(0),
        )
        .unwrap_or(0);
    let total_republishes: u32 = db
        .query_row(
            "SELECT COUNT(*) FROM events WHERE event = 'republish'",
            [],
            |r| r.get(0),
        )
        .unwrap_or(0);
    let total_unpublishes: u32 = db
        .query_row(
            "SELECT COUNT(*) FROM events WHERE event = 'unpublish'",
            [],
            |r| r.get(0),
        )
        .unwrap_or(0);

    // Total words (publish + republish only)
    let total_words_published: i64 = db
        .query_row(
            "SELECT COALESCE(SUM(word_count), 0) FROM events WHERE event IN ('publish', 'republish')",
            [],
            |r| r.get(0),
        )
        .unwrap_or(0);

    // Unique posts published
    let unique_posts_published: u32 = db
        .query_row(
            "SELECT COUNT(DISTINCT slug) FROM events WHERE event = 'publish'",
            [],
            |r| r.get(0),
        )
        .unwrap_or(0);

    // Time window stats
    let today = Local::now();
    let week_start = today - chrono::Duration::days(today.weekday().num_days_from_monday() as i64);
    let week_start_str = week_start.format("%Y-%m-%d").to_string();
    let last_week_start = week_start - chrono::Duration::days(7);
    let last_week_start_str = last_week_start.format("%Y-%m-%d").to_string();
    let month_start = format!("{}-{:02}-01", today.year(), today.month());

    let publishes_this_week: u32 = db
        .query_row(
            "SELECT COUNT(*) FROM events WHERE event IN ('publish', 'republish') AND local_date >= ?1",
            params![week_start_str],
            |r| r.get(0),
        )
        .unwrap_or(0);

    let publishes_last_week: u32 = db
        .query_row(
            "SELECT COUNT(*) FROM events WHERE event IN ('publish', 'republish') AND local_date >= ?1 AND local_date < ?2",
            params![last_week_start_str, week_start_str],
            |r| r.get(0),
        )
        .unwrap_or(0);

    let words_this_week: i64 = db
        .query_row(
            "SELECT COALESCE(SUM(word_count), 0) FROM events WHERE event IN ('publish', 'republish') AND local_date >= ?1",
            params![week_start_str],
            |r| r.get(0),
        )
        .unwrap_or(0);

    let words_last_week: i64 = db
        .query_row(
            "SELECT COALESCE(SUM(word_count), 0) FROM events WHERE event IN ('publish', 'republish') AND local_date >= ?1 AND local_date < ?2",
            params![last_week_start_str, week_start_str],
            |r| r.get(0),
        )
        .unwrap_or(0);

    let publishes_this_month: u32 = db
        .query_row(
            "SELECT COUNT(*) FROM events WHERE event IN ('publish', 'republish') AND local_date >= ?1",
            params![month_start],
            |r| r.get(0),
        )
        .unwrap_or(0);

    // Words this month
    let words_this_month: i64 = db
        .query_row(
            "SELECT COALESCE(SUM(word_count), 0) FROM events WHERE event IN ('publish', 'republish') AND local_date >= ?1",
            params![month_start],
            |r| r.get(0),
        )
        .unwrap_or(0);

    // Words last month
    let last_month_start = if today.month() == 1 {
        format!("{}-12-01", today.year() - 1)
    } else {
        format!("{}-{:02}-01", today.year(), today.month() - 1)
    };
    let words_last_month: i64 = db
        .query_row(
            "SELECT COALESCE(SUM(word_count), 0) FROM events WHERE event IN ('publish', 'republish') AND local_date >= ?1 AND local_date < ?2",
            params![last_month_start, month_start],
            |r| r.get(0),
        )
        .unwrap_or(0);

    // Monthly history (last 6 months)
    let mut monthly_history = Vec::new();
    {
        let mut stmt = db
            .prepare(
                "SELECT strftime('%Y-%m', local_date) as m, COALESCE(SUM(word_count), 0), COUNT(*)
                 FROM events WHERE event IN ('publish', 'republish')
                 GROUP BY m ORDER BY m DESC LIMIT 6",
            )
            .map_err(|e| format!("Query error: {}", e))?;
        let rows = stmt
            .query_map([], |row| {
                Ok(MonthStat {
                    month: row.get(0)?,
                    words: row.get(1)?,
                    posts: row.get(2)?,
                })
            })
            .map_err(|e| format!("Query error: {}", e))?;
        for ms in rows.flatten() {
            monthly_history.push(ms);
        }
    }

    // Average publishes per week (across all weeks with activity)
    let total_pub_events = total_publishes + total_republishes;
    let first_date: Option<String> = db
        .query_row(
            "SELECT MIN(local_date) FROM events WHERE event IN ('publish', 'republish')",
            [],
            |r| r.get(0),
        )
        .unwrap_or(None);

    let avg_publishes_per_week = if let Some(ref first) = first_date {
        if let Ok(first_d) = NaiveDate::parse_from_str(first, "%Y-%m-%d") {
            let today_d = today.date_naive();
            let weeks = ((today_d - first_d).num_days() as f64 / 7.0).max(1.0);
            total_pub_events as f64 / weeks
        } else {
            0.0
        }
    } else {
        0.0
    };

    let avg_words_per_post = if total_pub_events > 0 {
        total_words_published as f64 / total_pub_events as f64
    } else {
        0.0
    };

    // Hour distribution
    let mut publish_hour_distribution = vec![0u32; 24];
    {
        let mut stmt = db
            .prepare(
                "SELECT local_hour, COUNT(*) FROM events WHERE event IN ('publish', 'republish') GROUP BY local_hour",
            )
            .map_err(|e| format!("Query error: {}", e))?;
        let rows = stmt
            .query_map([], |row| Ok((row.get::<_, i32>(0)?, row.get::<_, u32>(1)?)))
            .map_err(|e| format!("Query error: {}", e))?;
        for (hour, count) in rows.flatten() {
            if (0..24).contains(&hour) {
                publish_hour_distribution[hour as usize] = count;
            }
        }
    }

    let most_active_hour = publish_hour_distribution
        .iter()
        .enumerate()
        .max_by_key(|(_, c)| *c)
        .and_then(|(h, c)| if *c > 0 { Some(h as u8) } else { None });

    // Day of week distribution
    let mut day_counts = [0u32; 7];
    {
        let mut stmt = db
            .prepare(
                "SELECT day_of_week, COUNT(*) FROM events WHERE event IN ('publish', 'republish') GROUP BY day_of_week",
            )
            .map_err(|e| format!("Query error: {}", e))?;
        let rows = stmt
            .query_map([], |row| Ok((row.get::<_, i32>(0)?, row.get::<_, u32>(1)?)))
            .map_err(|e| format!("Query error: {}", e))?;
        for (day, count) in rows.flatten() {
            if (0..7).contains(&day) {
                day_counts[day as usize] = count;
            }
        }
    }

    let day_names = [
        "Monday",
        "Tuesday",
        "Wednesday",
        "Thursday",
        "Friday",
        "Saturday",
        "Sunday",
    ];
    let most_active_day_of_week = day_counts
        .iter()
        .enumerate()
        .max_by_key(|(_, c)| *c)
        .and_then(|(d, c)| {
            if *c > 0 {
                Some(day_names[d].to_string())
            } else {
                None
            }
        });

    // Streaks — get all unique publish dates sorted
    let publish_dates = get_publish_dates(&db)?;
    let today_str = today.format("%Y-%m-%d").to_string();
    let (current_streak_days, current_streak_start) =
        compute_current_streak(&publish_dates, &today_str);
    let longest_streak_days = compute_longest_streak(&publish_dates);

    // Weekly streaks
    let (current_weekly_streak, longest_weekly_streak) = compute_weekly_streaks(&publish_dates);

    // Last publish
    let last_publish_at: Option<String> = db
        .query_row(
            "SELECT timestamp FROM events WHERE event IN ('publish', 'republish') ORDER BY id DESC LIMIT 1",
            [],
            |r| r.get(0),
        )
        .unwrap_or(None);

    let days_since_last_publish = publish_dates.last().and_then(|last| {
        NaiveDate::parse_from_str(last, "%Y-%m-%d").ok().map(|d| {
            let today_d = today.date_naive();
            (today_d - d).num_days().max(0) as u32
        })
    });

    // Milestones — the standard count ladder plus a handful of "unexpected"
    // ones (night owl, pithy, comeback, etc.) computed from richer DB queries.
    let weird = compute_weird_stats(&db, today.date_naive());
    let milestones = compute_milestones(
        total_publishes,
        current_streak_days,
        longest_streak_days,
        current_weekly_streak,
        total_words_published,
        unique_posts_published,
        &weird,
    );

    Ok(JournalStats {
        current_streak_days,
        longest_streak_days,
        current_streak_start,
        current_weekly_streak,
        longest_weekly_streak,
        total_publishes,
        total_republishes,
        total_unpublishes,
        total_words_published,
        unique_posts_published,
        publishes_this_week,
        publishes_last_week,
        words_this_week,
        words_last_week,
        publishes_this_month,
        words_this_month,
        words_last_month,
        monthly_history,
        avg_publishes_per_week,
        avg_words_per_post,
        most_active_hour,
        most_active_day_of_week,
        publish_hour_distribution,
        milestones,
        last_publish_at,
        days_since_last_publish,
    })
}

// ---------------------------------------------------------------------------
// Fun stats — feeds the "unexpected" milestones (night owl, pithy, comeback,
// etc.) so the wins list reads as personal observations, not a 10/25/50
// post ladder.
// ---------------------------------------------------------------------------

#[derive(Default)]
struct WeirdStats {
    night_owl_count: i64,
    dawn_count: i64,
    min_post_words: i64,  // smallest non-zero word_count seen, or 0 if none
    max_post_words: i64,
    unique_weekdays: i64,
    unique_months_active: i64,
    days_since_first_publish: i64,
    biggest_month_posts: i64,
    comeback_count: i64,  // times you returned after a 30+ day silence

    // ── personal-pattern stats (driven by EJ's actual data) ───────────
    /// Longest streak of consecutive Sundays with at least one publish.
    /// Sunday is the dominant publish day in EJ's history (~45% of all
    /// posts) so this is the most identity-aligned streak available.
    longest_sunday_streak: i64,
    /// Posts in the current calendar year. Pairs with `prior_year_posts`
    /// for the "beat last year" rolling challenge.
    current_year_posts: i64,
    prior_year_posts: i64,
    /// Posts in the current calendar month vs. the all-time biggest month.
    /// Drives "beat your best month" — resets every month-roll.
    current_month_posts: i64,
    /// Distinct active days in the current calendar year. Best year so far
    /// is ~57 active days; 100 is the genuine aspirational target.
    current_year_active_days: i64,
    /// Publishes between 22:00 and 01:59 (inclusive). Cinderella shift —
    /// publish-right-before-or-after-the-day-flips ritual.
    midnight_hour_count: i64,
}

fn compute_weird_stats(db: &Connection, today: chrono::NaiveDate) -> WeirdStats {
    let mut w = WeirdStats::default();

    // Late night (>=22) or pre-dawn (<5)
    w.night_owl_count = db.query_row(
        "SELECT COUNT(*) FROM events WHERE event IN ('publish','republish')
         AND (local_hour >= 22 OR local_hour < 5)",
        [], |r| r.get(0),
    ).unwrap_or(0);

    // Early morning (5-7 inclusive)
    w.dawn_count = db.query_row(
        "SELECT COUNT(*) FROM events WHERE event IN ('publish','republish')
         AND local_hour >= 5 AND local_hour < 8",
        [], |r| r.get(0),
    ).unwrap_or(0);

    w.min_post_words = db.query_row(
        "SELECT COALESCE(MIN(word_count), 0) FROM events
         WHERE event = 'publish' AND word_count > 0",
        [], |r| r.get(0),
    ).unwrap_or(0);

    w.max_post_words = db.query_row(
        "SELECT COALESCE(MAX(word_count), 0) FROM events WHERE event = 'publish'",
        [], |r| r.get(0),
    ).unwrap_or(0);

    w.unique_weekdays = db.query_row(
        "SELECT COUNT(DISTINCT day_of_week) FROM events WHERE event IN ('publish','republish')",
        [], |r| r.get(0),
    ).unwrap_or(0);

    w.unique_months_active = db.query_row(
        "SELECT COUNT(DISTINCT substr(local_date, 1, 7)) FROM events
         WHERE event IN ('publish','republish')",
        [], |r| r.get(0),
    ).unwrap_or(0);

    let first_date: Option<String> = db.query_row(
        "SELECT MIN(local_date) FROM events WHERE event IN ('publish','republish')",
        [], |r| r.get(0),
    ).unwrap_or(None);
    if let Some(d) = first_date {
        if let Ok(first) = chrono::NaiveDate::parse_from_str(&d, "%Y-%m-%d") {
            w.days_since_first_publish = (today - first).num_days().max(0);
        }
    }

    w.biggest_month_posts = db.query_row(
        "SELECT COALESCE(MAX(c), 0) FROM (
            SELECT substr(local_date, 1, 7) AS m, COUNT(*) AS c
            FROM events WHERE event = 'publish'
            GROUP BY m
         )",
        [], |r| r.get(0),
    ).unwrap_or(0);

    // Comebacks: count consecutive publish-day gaps > 30. Cheap enough since
    // it's distinct dates (one row per active day).
    if let Ok(dates) = get_publish_dates(db) {
        let mut prev: Option<chrono::NaiveDate> = None;
        for d in &dates {
            if let Ok(cur) = chrono::NaiveDate::parse_from_str(d, "%Y-%m-%d") {
                if let Some(p) = prev {
                    if (cur - p).num_days() > 30 {
                        w.comeback_count += 1;
                    }
                }
                prev = Some(cur);
            }
        }
    }

    // ── personal-pattern queries ──────────────────────────────────────
    let cur_year = today.format("%Y").to_string();
    let prev_year_s = today
        .pred_opt()
        .map(|_| (today.year() - 1).to_string())
        .unwrap_or_default();
    let cur_month = today.format("%Y-%m").to_string();

    w.current_year_posts = db.query_row(
        "SELECT COUNT(*) FROM events WHERE event='publish' AND substr(local_date,1,4) = ?1",
        rusqlite::params![cur_year],
        |r| r.get(0),
    ).unwrap_or(0);

    w.prior_year_posts = db.query_row(
        "SELECT COUNT(*) FROM events WHERE event='publish' AND substr(local_date,1,4) = ?1",
        rusqlite::params![prev_year_s],
        |r| r.get(0),
    ).unwrap_or(0);

    w.current_month_posts = db.query_row(
        "SELECT COUNT(*) FROM events WHERE event='publish' AND substr(local_date,1,7) = ?1",
        rusqlite::params![cur_month],
        |r| r.get(0),
    ).unwrap_or(0);

    w.current_year_active_days = db.query_row(
        "SELECT COUNT(DISTINCT local_date) FROM events
         WHERE event='publish' AND substr(local_date,1,4) = ?1",
        rusqlite::params![cur_year],
        |r| r.get(0),
    ).unwrap_or(0);

    // Cinderella window: 22, 23, 0, 1 — narrower than night_owl_count.
    w.midnight_hour_count = db.query_row(
        "SELECT COUNT(*) FROM events WHERE event='publish'
         AND local_hour IN (22, 23, 0, 1)",
        [], |r| r.get(0),
    ).unwrap_or(0);

    // Longest consecutive-Sundays streak. Pull all Sunday publish-dates,
    // walk forward in 7-day jumps, count the longest unbroken run.
    if let Ok(mut stmt) = db.prepare(
        "SELECT DISTINCT local_date FROM events
         WHERE event='publish' AND day_of_week = 0 ORDER BY local_date ASC",
    ) {
        let rows = stmt.query_map([], |r| r.get::<_, String>(0)).ok();
        if let Some(rows) = rows {
            let sundays: Vec<chrono::NaiveDate> = rows
                .filter_map(|r| r.ok())
                .filter_map(|s| chrono::NaiveDate::parse_from_str(&s, "%Y-%m-%d").ok())
                .collect();
            let set: std::collections::HashSet<chrono::NaiveDate> =
                sundays.iter().copied().collect();
            let week = chrono::Duration::days(7);
            let mut longest: i64 = 0;
            for &s in &sundays {
                // Only walk from the start of a run.
                if set.contains(&(s - week)) {
                    continue;
                }
                let mut len: i64 = 1;
                let mut next = s + week;
                while set.contains(&next) {
                    len += 1;
                    next = next + week;
                }
                if len > longest {
                    longest = len;
                }
            }
            w.longest_sunday_streak = longest;
        }
    }

    w
}

// ---------------------------------------------------------------------------
// Streak computation
// ---------------------------------------------------------------------------

fn get_publish_dates(conn: &Connection) -> Result<Vec<String>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT DISTINCT local_date FROM events WHERE event IN ('publish', 'republish') ORDER BY local_date ASC",
        )
        .map_err(|e| format!("Query error: {}", e))?;
    let rows = stmt
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(|e| format!("Query error: {}", e))?;
    let mut dates = Vec::new();
    for row in rows {
        dates.push(row.map_err(|e| format!("Row error: {}", e))?);
    }
    Ok(dates)
}

fn compute_current_streak(dates: &[String], today: &str) -> (u32, Option<String>) {
    if dates.is_empty() {
        return (0, None);
    }

    let today_d = match NaiveDate::parse_from_str(today, "%Y-%m-%d") {
        Ok(d) => d,
        Err(_) => return (0, None),
    };

    // Parse all dates
    let mut parsed: Vec<NaiveDate> = dates
        .iter()
        .filter_map(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok())
        .collect();
    parsed.sort();
    parsed.dedup();

    // The streak must include today or yesterday to be "current"
    let yesterday = today_d - chrono::Duration::days(1);
    let last = parsed.last().copied().unwrap_or(today_d);
    if last < yesterday {
        return (0, None);
    }

    // Walk backwards from the most recent date
    let mut streak = 1u32;
    let mut streak_start = last;
    for i in (0..parsed.len() - 1).rev() {
        let expected = parsed[i + 1] - chrono::Duration::days(1);
        if parsed[i] == expected {
            streak += 1;
            streak_start = parsed[i];
        } else {
            break;
        }
    }

    (streak, Some(streak_start.format("%Y-%m-%d").to_string()))
}

fn compute_longest_streak(dates: &[String]) -> u32 {
    if dates.is_empty() {
        return 0;
    }

    let mut parsed: Vec<NaiveDate> = dates
        .iter()
        .filter_map(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok())
        .collect();
    parsed.sort();
    parsed.dedup();

    let mut longest = 1u32;
    let mut current = 1u32;

    for i in 1..parsed.len() {
        if parsed[i] - parsed[i - 1] == chrono::Duration::days(1) {
            current += 1;
            longest = longest.max(current);
        } else {
            current = 1;
        }
    }

    longest
}

fn compute_weekly_streaks(dates: &[String]) -> (u32, u32) {
    if dates.is_empty() {
        return (0, 0);
    }

    let parsed: Vec<NaiveDate> = dates
        .iter()
        .filter_map(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok())
        .collect();

    // Convert to ISO week numbers (year, week)
    let mut weeks: Vec<(i32, u32)> = parsed
        .iter()
        .map(|d| (d.iso_week().year(), d.iso_week().week()))
        .collect();
    weeks.sort();
    weeks.dedup();

    if weeks.is_empty() {
        return (0, 0);
    }

    // Compute consecutive weeks
    let mut longest = 1u32;
    let mut current = 1u32;

    for i in 1..weeks.len() {
        let (py, pw) = weeks[i - 1];
        let (cy, cw) = weeks[i];
        let consecutive = if py == cy {
            cw == pw + 1
        } else if cy == py + 1 {
            // Handle year boundary: last week of previous year -> week 1 of new year
            pw >= 52 && cw == 1
        } else {
            false
        };

        if consecutive {
            current += 1;
            longest = longest.max(current);
        } else {
            current = 1;
        }
    }

    // Check if current weekly streak is active (includes this or last week)
    let now = Local::now();
    let this_week = (now.iso_week().year(), now.iso_week().week());
    let last_week_date = now - chrono::Duration::days(7);
    let last_week = (
        last_week_date.iso_week().year(),
        last_week_date.iso_week().week(),
    );

    let current_streak = match weeks.last() {
        Some(last_active) if *last_active == this_week || *last_active == last_week => current,
        _ => 0,
    };

    (current_streak, longest)
}

// ---------------------------------------------------------------------------
// Milestones
// ---------------------------------------------------------------------------

fn compute_milestones(
    total_publishes: u32,
    _current_streak: u32,
    longest_streak: u32,
    weekly_streak: u32,
    total_words: i64,
    _unique_posts: u32,
    w: &WeirdStats,
) -> Vec<Milestone> {
    // (id, label, description, current_value, target_value)
    // Copy is intentionally warm + concrete — these are personal milestones,
    // not gamification badges. Description should read like a friend
    // congratulating you, not like an achievement panel in a video game.
    let posts = total_publishes as i64;
    let streak = longest_streak as i64;
    let weekly = weekly_streak as i64;
    let defs: Vec<(&str, &str, &str, i64, i64)> = vec![
        (
            "first_publish",
            "First spark",
            "You started a thing. That's the hardest part.",
            posts.min(1),
            1,
        ),
        (
            "posts_10",
            "Ten in the wild",
            "Ten ideas finished and out the door.",
            posts.min(10),
            10,
        ),
        (
            "posts_25",
            "Twenty-five strong",
            "A small archive of your own.",
            posts.min(25),
            25,
        ),
        (
            "posts_50",
            "Fifty in",
            "Half a hundred. You're a writer who ships.",
            posts.min(50),
            50,
        ),
        (
            "posts_100",
            "A hundred posts",
            "A hundred ideas, finished. That's a body of work.",
            posts.min(100),
            100,
        ),
        (
            "streak_3",
            "Three in a row",
            "Three days back-to-back. Momentum.",
            streak.min(3),
            3,
        ),
        (
            "streak_7",
            "Seven straight",
            "A week of daily posts. That's a practice.",
            streak.min(7),
            7,
        ),
        (
            "streak_14",
            "Two weeks unbroken",
            "Fourteen days. The habit's stuck.",
            streak.min(14),
            14,
        ),
        (
            "streak_30",
            "A month, daily",
            "Thirty days of posts. The well runs deep.",
            streak.min(30),
            30,
        ),
        (
            "weekly_4",
            "Steady month",
            "Posted every week for a full month.",
            weekly.min(4),
            4,
        ),
        (
            "weekly_12",
            "Steady quarter",
            "Posted every week for three months. Reliable.",
            weekly.min(12),
            12,
        ),
        (
            "words_10k",
            "Ten thousand words",
            "A novella's worth, out in the world.",
            total_words.min(10_000),
            10_000,
        ),
        (
            "words_50k",
            "Fifty-thousand words",
            "NaNoWriMo distance — published, not drafted.",
            total_words.min(50_000),
            50_000,
        ),
        (
            "words_100k",
            "A novel's worth",
            "Six figures of words. A whole novel's worth of thinking.",
            total_words.min(100_000),
            100_000,
        ),

        // ── unexpected ones — patterns in your own behavior you maybe
        // didn't notice you were building. ─────────────────────────────
        (
            "night_owl",
            "Night owl",
            "Ten posts after 10 PM. The 1 AM brain has things to say.",
            w.night_owl_count.min(10),
            10,
        ),
        (
            "dawn_writer",
            "Dawn writer",
            "Five posts before 8 AM. Sunrise has been good to you.",
            w.dawn_count.min(5),
            5,
        ),
        (
            "pithy",
            "Pithy",
            "Published a post under 100 words. Sometimes the shortest one says the most.",
            // Binary: 1 if any non-zero post is below 100 words.
            if w.min_post_words > 0 && w.min_post_words < 100 { 1 } else { 0 },
            1,
        ),
        (
            "longread",
            "Longread",
            "A single post over 5,000 words — a whole essay, finished.",
            if w.max_post_words >= 5_000 { 1 } else { 0 },
            1,
        ),
        (
            "across_the_week",
            "Across the week",
            "You've published on every weekday at some point. All seven sides.",
            w.unique_weekdays.min(7),
            7,
        ),
        (
            "year_on_the_wire",
            "A year on the wire",
            "You've shown up in twelve different calendar months.",
            w.unique_months_active.min(12),
            12,
        ),
        (
            "long_haul",
            "Long haul",
            "A thousand days since your first post. You've been at this a while.",
            w.days_since_first_publish.min(1000),
            1000,
        ),
        (
            "dump_month",
            "The dump month",
            "Ten posts in a single calendar month. Sometimes the well overflows.",
            w.biggest_month_posts.min(10),
            10,
        ),
        (
            "comeback_kid",
            "Comeback kid",
            "You came back after 30+ days of silence. The most important post is the next one.",
            w.comeback_count.min(1),
            1,
        ),

        // ── personal-pattern milestones — grounded in your actual data ──
        // Sunday is your strongest identity (~45% of all publishes land on
        // Sunday). Three tiers: a month, a quarter, a year.
        (
            "sunday_devotional_4",
            "Sunday devotional",
            "Four Sundays in a row. The rhythm you already trust.",
            w.longest_sunday_streak.min(4),
            4,
        ),
        (
            "sunday_devotional_12",
            "Sunday quarter",
            "Twelve consecutive Sundays. A season of showing up.",
            w.longest_sunday_streak.min(12),
            12,
        ),
        (
            "sunday_devotional_52",
            "Year of Sundays",
            "A full year of Sundays, unbroken. The writer you already are.",
            w.longest_sunday_streak.min(52),
            52,
        ),
        // Rolling year-over-year challenge. Target = beat prior year's count.
        // If prior year was zero, just clear the floor of 1.
        (
            "beat_last_year",
            "Beat last year",
            "Publish more this year than last. Past you is the only fair competitor.",
            w.current_year_posts,
            (w.prior_year_posts + 1).max(1),
        ),
        // Personal-record month — beat your biggest. biggest_month_posts is
        // computed all-time, so once this month becomes the new biggest the
        // record holds and a fresh month resets the challenge.
        (
            "beat_best_month",
            "Beat your best month",
            "More posts this month than any single month before.",
            w.current_month_posts,
            (w.biggest_month_posts + 1).max(1),
        ),
        // Aspirational. Best year so far has been ~57 distinct active days.
        // 100 means you stop being weekly and become more than weekly.
        (
            "hundred_day_year",
            "100-day year",
            "A hundred distinct days of publishing in one calendar year. More than weekly. A real practice.",
            w.current_year_active_days.min(100),
            100,
        ),
        // Cinderella shift — your actual midnight-publishing ritual. 22–01.
        (
            "cinderella",
            "Cinderella shift",
            "Fifty posts pushed through right before or after midnight. The hour you actually write.",
            w.midnight_hour_count.min(50),
            50,
        ),
    ];

    defs.into_iter()
        .map(|(id, label, desc, current, target)| Milestone {
            id: id.to_string(),
            label: label.to_string(),
            description: desc.to_string(),
            achieved_at: if current >= target {
                Some("achieved".to_string())
            } else {
                None
            },
            current,
            target,
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Nudges
// ---------------------------------------------------------------------------

pub fn get_nudge() -> Result<Option<Nudge>, String> {
    let stats = get_stats()?;

    // Best month ever
    if stats.words_this_month > 0 && !stats.monthly_history.is_empty() {
        let past_max = stats
            .monthly_history
            .iter()
            .skip(1) // skip current month
            .map(|m| m.words)
            .max()
            .unwrap_or(0);
        if stats.words_this_month > past_max && past_max > 0 {
            return Ok(Some(Nudge {
                message: format!(
                    "Your most prolific month yet — {} words and counting",
                    format_words_friendly(stats.words_this_month)
                ),
                kind: "celebration".to_string(),
            }));
        }
    }

    // Word count color: contextualize monthly words
    if stats.words_this_month >= 5000 && stats.words_last_month > 0 {
        let pct = ((stats.words_this_month as f64 / stats.words_last_month as f64) * 100.0) as i64;
        if pct > 120 {
            return Ok(Some(Nudge {
                message: format!(
                    "{} words this month — outpacing last month by {}%",
                    format_words_friendly(stats.words_this_month),
                    pct - 100
                ),
                kind: "celebration".to_string(),
            }));
        }
    }

    // Streak celebrations — escalating warmth
    if stats.current_streak_days >= 30 {
        return Ok(Some(Nudge {
            message: format!(
                "Day {} of your streak. A month of showing up. That's the whole game.",
                stats.current_streak_days
            ),
            kind: "encouragement".to_string(),
        }));
    }
    if stats.current_streak_days >= 14 {
        return Ok(Some(Nudge {
            message: format!(
                "{} days straight. This isn't luck — it's a practice now.",
                stats.current_streak_days
            ),
            kind: "encouragement".to_string(),
        }));
    }
    if stats.current_streak_days >= 7 {
        return Ok(Some(Nudge {
            message: format!(
                "A full week! {}-day streak and {} words this week.",
                stats.current_streak_days,
                format_words_friendly(stats.words_this_week)
            ),
            kind: "encouragement".to_string(),
        }));
    }
    if stats.current_streak_days >= 3 {
        return Ok(Some(Nudge {
            message: format!(
                "{}-day streak — momentum is building.",
                stats.current_streak_days
            ),
            kind: "encouragement".to_string(),
        }));
    }

    // Above weekly average
    if stats.publishes_this_week as f64 > stats.avg_publishes_per_week
        && stats.avg_publishes_per_week > 0.5
        && stats.total_publishes > 5
    {
        return Ok(Some(Nudge {
            message: format!(
                "{} posts this week — above your usual {:.0}/week",
                stats.publishes_this_week, stats.avg_publishes_per_week
            ),
            kind: "celebration".to_string(),
        }));
    }

    // Word total milestones (encouraging, not badge-like)
    if stats.total_words_published >= 100_000 {
        return Ok(Some(Nudge {
            message: format!(
                "{} words published. That's a novel's worth of thinking, out in the world.",
                format_words_friendly(stats.total_words_published)
            ),
            kind: "encouragement".to_string(),
        }));
    }
    if stats.total_words_published >= 50_000 {
        return Ok(Some(Nudge {
            message: format!(
                "{} total words published — you've built something real.",
                format_words_friendly(stats.total_words_published)
            ),
            kind: "encouragement".to_string(),
        }));
    }
    if stats.total_words_published >= 10_000 {
        return Ok(Some(Nudge {
            message: format!(
                "{} words out in the world so far.",
                format_words_friendly(stats.total_words_published)
            ),
            kind: "encouragement".to_string(),
        }));
    }

    // Streak at risk
    if let Some(days) = stats.days_since_last_publish {
        if days >= 2 && stats.longest_streak_days >= 3 {
            return Ok(Some(Nudge {
                message: format!(
                    "Your {}-day streak ended {} days ago. Start a new one?",
                    stats.longest_streak_days, days
                ),
                kind: "streak".to_string(),
            }));
        }

        // Gentle reminder after a week
        if days >= 7 {
            return Ok(Some(Nudge {
                message: "It's been a quiet week. What's been on your mind?".to_string(),
                kind: "reminder".to_string(),
            }));
        }
    }

    // Monthly words encouragement when just getting started
    if stats.words_this_month > 0 && stats.words_this_month < 5000 && stats.total_publishes > 1 {
        return Ok(Some(Nudge {
            message: format!(
                "{} words this month so far.",
                format_words_friendly(stats.words_this_month)
            ),
            kind: "encouragement".to_string(),
        }));
    }

    Ok(None)
}

fn format_words_friendly(words: i64) -> String {
    if words >= 1000 {
        format!("{:.1}k", words as f64 / 1000.0)
    } else {
        format!("{}", words)
    }
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
