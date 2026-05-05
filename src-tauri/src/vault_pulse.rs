use crate::config;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use walkdir::WalkDir;

/// A signal detected from vault observation — never actionable toward publishing,
/// purely informational about writing patterns.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VaultSignal {
    pub kind: String, // "active_draft", "growing_draft", "tag_cluster", "dormant_idea"
    pub message: String, // Human-readable observation
    pub path: Option<String>,
    pub metadata: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VaultPulse {
    pub total_files: u32,
    pub total_words: u64,
    pub drafts_count: u32,
    pub blog_count: u32,
    pub signals: Vec<VaultSignal>,
    pub tag_cloud: Vec<(String, u32)>, // (tag, count) sorted by count desc
    pub recent_edits: Vec<RecentEdit>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecentEdit {
    pub path: String,
    pub filename: String,
    pub title: Option<String>,
    pub word_count: usize,
    pub modified: u64,
    pub source_dir: String,
    pub is_publishable: bool, // true = in blog/, false = drafts/other
}

pub fn scan_vault() -> Result<VaultPulse, String> {
    let app_config = config::get()?;
    let vault_path = &app_config.vault.path;
    let publishable_dirs = &app_config.vault.publishable_dirs;
    let excluded_dirs = &app_config.vault.excluded_dirs;

    let mut total_files = 0u32;
    let mut total_words = 0u64;
    let mut drafts_count = 0u32;
    let mut blog_count = 0u32;
    let mut tag_counts: HashMap<String, u32> = HashMap::new();
    let mut all_files: Vec<RecentEdit> = Vec::new();
    let mut draft_edits: Vec<(String, String, Option<String>, usize, u64)> = Vec::new(); // path, filename, title, words, modified

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    for entry in WalkDir::new(vault_path)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().is_some_and(|ext| ext == "md"))
    {
        let path = entry.path();
        let path_str = path.to_string_lossy().to_string();

        // Skip hidden dirs, .obsidian, templates, attachments
        if path_str.contains("/.obsidian/")
            || path_str.contains("/templates/")
            || path_str.contains("/attachments/")
            || path_str.contains("/_stale/")
            || path_str.contains("/_archive/")
            || path_str.contains("/node_modules/")
        {
            continue;
        }

        // Skip excluded dirs
        if excluded_dirs
            .iter()
            .any(|dir| path_str.contains(&format!("/{}/", dir)))
        {
            continue;
        }

        let content = match fs::read_to_string(path) {
            Ok(c) => c,
            Err(_) => continue,
        };

        let modified = fs::metadata(path)
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs())
            .unwrap_or(0);

        let word_count = count_words(&content);
        let (title, tags) = parse_frontmatter_light(&content);

        // Determine source dir
        let rel_path = path_str
            .strip_prefix(vault_path)
            .unwrap_or(&path_str)
            .trim_start_matches('/');
        let source_dir = rel_path
            .rsplit_once('/')
            .map(|(dir, _)| dir.to_string())
            .unwrap_or_default();

        let is_publishable = publishable_dirs
            .iter()
            .any(|dir| path_str.contains(&format!("/{}/", dir)));

        let filename = path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();

        total_files += 1;
        total_words += word_count as u64;

        if is_publishable {
            blog_count += 1;
        } else {
            drafts_count += 1;
        }

        // Count tags
        for tag in &tags {
            *tag_counts.entry(tag.clone()).or_insert(0) += 1;
        }

        // Track non-publishable files that have been edited recently (for signals)
        if !is_publishable && modified > 0 {
            draft_edits.push((
                path_str.clone(),
                filename.clone(),
                title.clone(),
                word_count,
                modified,
            ));
        }

        all_files.push(RecentEdit {
            path: path_str,
            filename,
            title,
            word_count,
            modified,
            source_dir,
            is_publishable,
        });
    }

    // Sort all files by modified desc, take top 20
    all_files.sort_by(|a, b| b.modified.cmp(&a.modified));
    let recent_edits: Vec<RecentEdit> = all_files.into_iter().take(20).collect();

    // Build tag cloud (top 30 tags)
    let mut tag_cloud: Vec<(String, u32)> = tag_counts.into_iter().collect();
    tag_cloud.sort_by(|a, b| b.1.cmp(&a.1));
    tag_cloud.truncate(30);

    // Generate signals
    let mut signals = Vec::new();

    // Signal: drafts edited in last 7 days with substantial word count
    let week_ago = now.saturating_sub(7 * 86400);
    let mut active_drafts: Vec<&(String, String, Option<String>, usize, u64)> = draft_edits
        .iter()
        .filter(|(_, _, _, words, modified)| *modified > week_ago && *words > 200)
        .collect();
    active_drafts.sort_by(|a, b| b.4.cmp(&a.4));

    for (path, filename, title, words, _modified) in active_drafts.iter().take(5) {
        let fallback = filename.replace(".md", "").replace('-', " ");
        let display = title.as_deref().unwrap_or(&fallback);
        signals.push(VaultSignal {
            kind: "active_draft".to_string(),
            message: format!("You've been working on \"{}\" — {} words", display, words),
            path: Some(path.clone()),
            metadata: HashMap::from([("word_count".to_string(), words.to_string())]),
        });
    }

    // Signal: big drafts that are clearly growing — 2000+ words and recently edited
    let three_days_ago = now.saturating_sub(3 * 86400);
    let mut growing: Vec<&(String, String, Option<String>, usize, u64)> = draft_edits
        .iter()
        .filter(|(_, _, _, words, modified)| *words >= 2000 && *modified > three_days_ago)
        .collect();
    growing.sort_by(|a, b| b.3.cmp(&a.3));

    for (path, filename, title, words, _) in growing.iter().take(2) {
        let fallback_g = filename.replace(".md", "").replace('-', " ");
        let display = title.as_deref().unwrap_or(&fallback_g);
        let word_label = if *words >= 4000 {
            format!("{} words — that's a real piece", words)
        } else if *words >= 3000 {
            format!("{} words and growing", words)
        } else {
            format!("{} words so far", words)
        };
        signals.push(VaultSignal {
            kind: "growing_draft".to_string(),
            message: format!("\"{}\" — {}", display, word_label),
            path: Some(path.clone()),
            metadata: HashMap::from([("word_count".to_string(), words.to_string())]),
        });
    }

    // Signal: large drafts that haven't been touched in 2+ weeks (dormant ideas)
    let two_weeks_ago = now.saturating_sub(14 * 86400);
    let month_ago = now.saturating_sub(30 * 86400);
    let mut dormant: Vec<&(String, String, Option<String>, usize, u64)> = draft_edits
        .iter()
        .filter(|(_, _, _, words, modified)| {
            *modified < two_weeks_ago && *modified > month_ago && *words > 500
        })
        .collect();
    dormant.sort_by(|a, b| b.3.cmp(&a.3)); // sort by word count

    for (path, filename, title, words, _) in dormant.iter().take(3) {
        let fallback_d = filename.replace(".md", "").replace('-', " ");
        let display = title.as_deref().unwrap_or(&fallback_d);
        signals.push(VaultSignal {
            kind: "dormant_idea".to_string(),
            message: format!(
                "\"{}\" has {} words but hasn't been touched in a while",
                display, words
            ),
            path: Some(path.clone()),
            metadata: HashMap::from([("word_count".to_string(), words.to_string())]),
        });
    }

    // Signal: tag clusters (tags that appear in both publishable and non-publishable files)
    // This hints at thematic threads the user is developing
    // (skip for now — tag_cloud alone is useful)

    Ok(VaultPulse {
        total_files,
        total_words,
        drafts_count,
        blog_count,
        signals,
        tag_cloud,
        recent_edits,
    })
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn count_words(content: &str) -> usize {
    // Strip frontmatter
    let body = if let Some(rest) = content.strip_prefix("---") {
        if let Some(end) = rest.find("---") {
            &rest[end + 3..]
        } else {
            content
        }
    } else {
        content
    };
    body.split_whitespace().count()
}

fn parse_frontmatter_light(content: &str) -> (Option<String>, Vec<String>) {
    let mut title = None;
    let mut tags = Vec::new();

    if !content.starts_with("---") {
        // Try to get title from first heading
        for line in content.lines().take(5) {
            if let Some(h) = line.strip_prefix("# ") {
                title = Some(h.trim().to_string());
                break;
            }
        }
        return (title, tags);
    }

    let after = &content[3..];
    let end = match after.find("---") {
        Some(e) => e,
        None => return (title, tags),
    };
    let fm = &after[..end];

    for line in fm.lines() {
        let line = line.trim();
        if let Some(val) = line.strip_prefix("title:") {
            let val = val.trim().trim_matches('"').trim_matches('\'');
            if !val.is_empty() {
                title = Some(val.to_string());
            }
        }
        if let Some(val) = line.strip_prefix("tags:") {
            let val = val.trim().trim_start_matches('[').trim_end_matches(']');
            for tag in val.split(',') {
                let t = tag.trim().trim_matches('"').trim_matches('\'').trim();
                if !t.is_empty() {
                    tags.push(t.to_string());
                }
            }
        }
        // Also handle YAML list-style tags
        if line.starts_with("- ") && !tags.is_empty() {
            // We're probably in a tags list
            let t = line
                .strip_prefix("- ")
                .unwrap_or(line)
                .trim()
                .trim_matches('"');
            if !t.is_empty() {
                tags.push(t.to_string());
            }
        }
    }

    // Fallback: title from first heading
    if title.is_none() {
        let body = &after[end + 3..];
        for line in body.lines().take(10) {
            if let Some(h) = line.strip_prefix("# ") {
                title = Some(h.trim().to_string());
                break;
            }
        }
    }

    (title, tags)
}
