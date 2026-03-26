use chrono::{Datelike, Local, NaiveDate, Timelike, Utc};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Nudge {
    pub message: String,
    pub kind: String, // "streak", "encouragement", "reminder", "celebration"
}

// ---------------------------------------------------------------------------
// Database singleton
// ---------------------------------------------------------------------------

static DB: OnceLock<Mutex<Connection>> = OnceLock::new();

fn db_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_default();
    PathBuf::from(home)
        .join("Library/Application Support/com.ejfox.dispatch")
        .join("journal.db")
}

fn get_db() -> &'static Mutex<Connection> {
    DB.get_or_init(|| {
        let path = db_path();
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let conn = Connection::open(&path).expect("Failed to open journal database");
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")
            .expect("Failed to set pragmas");
        init_schema(&conn);
        Mutex::new(conn)
    })
}

fn init_schema(conn: &Connection) {
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
    .expect("Failed to create journal schema");
}

// ---------------------------------------------------------------------------
// Record events
// ---------------------------------------------------------------------------

pub fn record_event(
    event: &str,
    slug: &str,
    title: Option<&str>,
    word_count: usize,
    tags: &[String],
    content_type: &str,
    url: Option<&str>,
    target_id: Option<&str>,
    visibility: &str,
) -> Result<i64, String> {
    let now = Utc::now();
    let local = Local::now();
    let timestamp = now.to_rfc3339();
    let local_date = local.format("%Y-%m-%d").to_string();
    let local_hour = local.hour() as i32;
    let day_of_week = local.weekday().num_days_from_monday() as i32;
    let tags_str = tags.join(",");

    let db = get_db().lock().map_err(|e| format!("DB lock: {}", e))?;
    db.execute(
        "INSERT INTO events (timestamp, local_date, local_hour, day_of_week, event, slug, title, word_count, tags, content_type, url, target_id, visibility)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
        params![
            timestamp,
            local_date,
            local_hour,
            day_of_week,
            event,
            slug,
            title,
            word_count as i64,
            tags_str,
            content_type,
            url,
            target_id,
            visibility,
        ],
    )
    .map_err(|e| format!("Failed to record event: {}", e))?;

    Ok(db.last_insert_rowid())
}

// ---------------------------------------------------------------------------
// Read entries
// ---------------------------------------------------------------------------

pub fn get_recent_entries(limit: usize) -> Result<Vec<JournalEntry>, String> {
    let db = get_db().lock().map_err(|e| format!("DB lock: {}", e))?;
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
    let db = get_db().lock().map_err(|e| format!("DB lock: {}", e))?;
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
    let db = get_db().lock().map_err(|e| format!("DB lock: {}", e))?;

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
        for row in rows {
            if let Ok(ms) = row {
                monthly_history.push(ms);
            }
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
        for row in rows {
            if let Ok((hour, count)) = row {
                if (0..24).contains(&hour) {
                    publish_hour_distribution[hour as usize] = count;
                }
            }
        }
    }

    let most_active_hour = publish_hour_distribution
        .iter()
        .enumerate()
        .max_by_key(|(_, c)| *c)
        .and_then(|(h, c)| if *c > 0 { Some(h as u8) } else { None });

    // Day of week distribution
    let mut day_counts = vec![0u32; 7];
    {
        let mut stmt = db
            .prepare(
                "SELECT day_of_week, COUNT(*) FROM events WHERE event IN ('publish', 'republish') GROUP BY day_of_week",
            )
            .map_err(|e| format!("Query error: {}", e))?;
        let rows = stmt
            .query_map([], |row| Ok((row.get::<_, i32>(0)?, row.get::<_, u32>(1)?)))
            .map_err(|e| format!("Query error: {}", e))?;
        for row in rows {
            if let Ok((day, count)) = row {
                if (0..7).contains(&day) {
                    day_counts[day as usize] = count;
                }
            }
        }
    }

    let day_names = ["Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday", "Sunday"];
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

    // Milestones
    let milestones = compute_milestones(
        total_publishes,
        current_streak_days,
        longest_streak_days,
        current_weekly_streak,
        total_words_published,
        unique_posts_published,
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
    let last = *parsed.last().unwrap();
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

    let last_active = weeks.last().unwrap();
    let current_streak = if *last_active == this_week || *last_active == last_week {
        current
    } else {
        0
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
) -> Vec<Milestone> {
    let defs: Vec<(&str, &str, &str, bool)> = vec![
        ("first_publish", "First Post!", "Published your first post", total_publishes >= 1),
        ("posts_10", "Getting Started", "Published 10 posts", total_publishes >= 10),
        ("posts_25", "Quarter Century", "Published 25 posts", total_publishes >= 25),
        ("posts_50", "Half a Hundred", "Published 50 posts", total_publishes >= 50),
        ("posts_100", "Century Mark", "Published 100 posts", total_publishes >= 100),
        ("streak_3", "Hat Trick", "3-day publishing streak", longest_streak >= 3),
        ("streak_7", "Week Warrior", "7-day publishing streak", longest_streak >= 7),
        ("streak_14", "Two-Week Flow", "14-day publishing streak", longest_streak >= 14),
        ("streak_30", "Monthly Machine", "30-day publishing streak", longest_streak >= 30),
        ("weekly_4", "Monthly Regular", "Published every week for a month", weekly_streak >= 4),
        ("weekly_12", "Quarterly Cadence", "Published every week for 3 months", weekly_streak >= 12),
        ("words_10k", "10,000 Words", "Published 10,000 words total", total_words >= 10_000),
        ("words_50k", "NaNoWriMo", "Published 50,000 words total", total_words >= 50_000),
        ("words_100k", "Novel Length", "Published 100,000 words total", total_words >= 100_000),
    ];

    defs.into_iter()
        .map(|(id, label, desc, achieved)| Milestone {
            id: id.to_string(),
            label: label.to_string(),
            description: desc.to_string(),
            achieved_at: if achieved {
                Some("achieved".to_string()) // We could query the exact date, but this is simpler
            } else {
                None
            },
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
        let past_max = stats.monthly_history.iter()
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
    let db = get_db().lock().map_err(|e| format!("DB lock: {}", e))?;

    // Only backfill if the DB is empty
    let count: i64 = db
        .query_row("SELECT COUNT(*) FROM events", [], |r| r.get(0))
        .unwrap_or(0);
    if count > 0 {
        return Ok(0);
    }

    // Parse git log for "Publish: slug" commits
    let output = std::process::Command::new("git")
        .args(["log", "--oneline", "--format=%aI %s", "--all"])
        .current_dir(repo_path)
        .output()
        .map_err(|e| format!("git log failed: {}", e))?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut inserted = 0u32;
    let re = regex::Regex::new(r"^(\S+)\s+Publish:\s+(.+)$").unwrap();

    for line in stdout.lines().rev() {
        // Process oldest first
        if let Some(cap) = re.captures(line) {
            let ts = &cap[1];
            let slug = cap[2].trim();

            // Parse the ISO timestamp for local date/hour
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
