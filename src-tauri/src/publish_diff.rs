//! Compute a human-readable diff between a vault source markdown and the
//! published copy in the website repo.
//!
//! Powers the "See changes" panel in `StatusBanner`. We want a structured
//! payload (not raw unified diff text) so the Vue side can render colored
//! hunks with line numbers.

use serde::{Deserialize, Serialize};
use similar::{ChangeTag, TextDiff};
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiffLine {
    /// "equal" | "added" | "removed"
    pub tag: String,
    pub content: String,
    /// 1-based line number in the source file (None for pure-deletions).
    pub source_line: Option<u32>,
    /// 1-based line number in the published file (None for pure-additions).
    pub published_line: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiffHunk {
    pub source_start: u32,
    pub published_start: u32,
    pub lines: Vec<DiffLine>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublishDiff {
    pub has_diff: bool,
    pub source_path: String,
    pub published_path: Option<String>,
    /// Stripped of frontmatter — same shape used by `content_differs`.
    pub words_added: usize,
    pub words_removed: usize,
    pub lines_added: usize,
    pub lines_removed: usize,
    pub hunks: Vec<DiffHunk>,
    /// Set when we couldn't find a published copy to compare against
    /// (the post was never published, or path resolution failed).
    pub error: Option<String>,
}

/// Strip YAML frontmatter and return body only. Mirrors `normalize_content`
/// in vault.rs but returns owned String without further whitespace munging
/// (we want to preserve real line-by-line text for the diff renderer).
fn strip_frontmatter(content: &str) -> String {
    if let Some(after_start) = content.strip_prefix("---") {
        if let Some(end) = after_start.find("---") {
            return after_start[end + 3..].trim_start_matches('\n').to_string();
        }
    }
    content.to_string()
}

fn word_count(s: &str) -> usize {
    s.split_whitespace().count()
}

/// Compute diff between source file and published copy, looking up the
/// published copy via the existing `vault::find_published_info_for_target`
/// resolver so we honor the user's configured publish target.
pub fn compute_publish_diff(source_file_path: &str) -> Result<PublishDiff, String> {
    let source_path = Path::new(source_file_path);
    if !source_path.exists() {
        return Err(format!("Source file not found: {}", source_file_path));
    }

    let slug = source_path
        .file_stem()
        .and_then(|s| s.to_str())
        .ok_or_else(|| "Could not derive slug from source path".to_string())?;

    // Use the default publish target — same resolution path the rest of
    // Dispatch uses when checking modified status.
    let target = crate::config::default_target()?;
    let (_url, _date, published_content) =
        crate::vault::find_published_info_for_target(&target, slug);

    let source_raw = fs::read_to_string(source_file_path)
        .map_err(|e| format!("Failed to read source: {}", e))?;

    let Some(published_raw) = published_content else {
        return Ok(PublishDiff {
            has_diff: false,
            source_path: source_file_path.to_string(),
            published_path: None,
            words_added: 0,
            words_removed: 0,
            lines_added: 0,
            lines_removed: 0,
            hunks: Vec::new(),
            error: Some("No published copy found to compare against.".into()),
        });
    };

    // Diff the bodies (not the frontmatter), since YAML changes are mostly
    // metadata noise the user already sees in MetadataPanel.
    let source_body = strip_frontmatter(&source_raw);
    let published_body = strip_frontmatter(&published_raw);

    let diff = TextDiff::from_lines(&published_body, &source_body);

    let mut hunks: Vec<DiffHunk> = Vec::new();
    let mut lines_added = 0usize;
    let mut lines_removed = 0usize;
    let mut words_added = 0usize;
    let mut words_removed = 0usize;

    // ~3 lines of context around each change is what feels right for
    // markdown review (matches `git diff -U3` default).
    for group in diff.grouped_ops(3) {
        if group.is_empty() {
            continue;
        }
        let mut lines: Vec<DiffLine> = Vec::new();
        let mut source_start: Option<u32> = None;
        let mut published_start: Option<u32> = None;

        for op in &group {
            for change in diff.iter_changes(op) {
                let tag = match change.tag() {
                    ChangeTag::Equal => "equal",
                    ChangeTag::Insert => "added",
                    ChangeTag::Delete => "removed",
                };
                let raw = change.to_string();
                let content = raw.trim_end_matches('\n').to_string();
                if change.tag() == ChangeTag::Insert {
                    lines_added += 1;
                    words_added += word_count(&content);
                } else if change.tag() == ChangeTag::Delete {
                    lines_removed += 1;
                    words_removed += word_count(&content);
                }
                // similar uses 0-based indices; we present 1-based for UI.
                let source_line = change.new_index().map(|i| (i + 1) as u32);
                let published_line = change.old_index().map(|i| (i + 1) as u32);
                if source_start.is_none() {
                    source_start = source_line;
                }
                if published_start.is_none() {
                    published_start = published_line;
                }
                lines.push(DiffLine {
                    tag: tag.into(),
                    content,
                    source_line,
                    published_line,
                });
            }
        }
        hunks.push(DiffHunk {
            source_start: source_start.unwrap_or(1),
            published_start: published_start.unwrap_or(1),
            lines,
        });
    }

    Ok(PublishDiff {
        has_diff: !hunks.is_empty(),
        source_path: source_file_path.to_string(),
        published_path: None, // resolved internally; surface if needed later
        words_added,
        words_removed,
        lines_added,
        lines_removed,
        hunks,
        error: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn word_count_handles_whitespace() {
        assert_eq!(word_count("hello   world"), 2);
        assert_eq!(word_count("  "), 0);
        assert_eq!(word_count(""), 0);
    }

    #[test]
    fn strip_frontmatter_handles_yaml_block() {
        let content = "---\ntitle: foo\n---\n\nBody paragraph.\n";
        assert_eq!(strip_frontmatter(content), "Body paragraph.\n");
    }

    #[test]
    fn strip_frontmatter_passthrough_when_no_yaml() {
        let content = "# Heading\n\nBody.";
        assert_eq!(strip_frontmatter(content), content);
    }
}
