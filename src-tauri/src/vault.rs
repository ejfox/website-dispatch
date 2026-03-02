use crate::{config, Config, MarkdownFile};
use regex::Regex;
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::time::SystemTime;
use walkdir::WalkDir;

pub fn get_recent_files(limit: usize) -> Result<Vec<MarkdownFile>, String> {
    let config = Config::default();
    let mut files: Vec<MarkdownFile> = Vec::new();

    // Use publishable dirs from config
    let app_config = config::get();
    let publishable_dirs = app_config.vault.publishable_dirs.clone();

    for entry in WalkDir::new(&config.vault_path)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().is_some_and(|ext| ext == "md"))
    {
        let path = entry.path();
        let path_str = path.to_string_lossy();

        // Only include files from publishable directories
        let in_publishable = publishable_dirs
            .iter()
            .any(|dir| path_str.contains(&format!("/{}/", dir)));

        // Determine content type from path
        let content_type = if path_str.contains("/week-notes/") {
            "weeknote"
        } else {
            "post"
        };

        if !in_publishable {
            continue;
        }

        // Skip excluded directories
        if config
            .excluded_dirs
            .iter()
            .any(|dir| path_str.contains(&format!("/{}/", dir)))
        {
            continue;
        }

        // Skip stale/archive folders
        if path_str.contains("/_stale/") || path_str.contains("/_archive/") {
            continue;
        }

        // Skip week-notes nested inside blog/ (stray copies) — week-notes/ is its own publishable dir
        if path_str.contains("/blog/week-notes/") {
            continue;
        }

        if let Ok(metadata) = fs::metadata(path) {
            let fs_modified = get_timestamp(metadata.modified());
            let fs_created = metadata
                .created()
                .map(|t| get_timestamp(Ok(t)))
                .unwrap_or(fs_modified);

            let content = fs::read_to_string(path).unwrap_or_default();
            let (frontmatter, body) = parse_frontmatter(&content);

            // Prefer frontmatter dates over filesystem dates
            // For week notes, derive date from filename (e.g. "2025-37.md" = week 37 of 2025)
            let filename_date = if content_type == "weeknote" {
                parse_weeknote_filename(path)
            } else {
                None
            };
            // For week notes: skip old ones (>8 weeks by filename date)
            if content_type == "weeknote" {
                if let Some(fn_date) = filename_date {
                    let eight_weeks_ago = SystemTime::now()
                        .duration_since(SystemTime::UNIX_EPOCH)
                        .map(|d| d.as_secs())
                        .unwrap_or(0)
                        .saturating_sub(86400 * 56);
                    if fn_date < eight_weeks_ago {
                        continue;
                    }
                }
            }

            let modified = frontmatter
                .get("modified")
                .and_then(|d| parse_iso_date(d))
                .unwrap_or(fs_modified);
            let created = frontmatter
                .get("date")
                .and_then(|d| parse_iso_date(d))
                .or(filename_date)
                .unwrap_or(fs_created);
            let title = extract_h1_title(&body);
            let filename = path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            let slug = filename.trim_end_matches(".md");

            let (published_url, published_date, published_content) =
                find_published_info(&config.website_repo, slug);
            let source_dir = path
                .parent()
                .and_then(|p| p.strip_prefix(&config.vault_path).ok())
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_default();

            let mut warnings = check_warnings(&body, &frontmatter, title.is_some(), content_type);

            // Check if content actually differs from published version
            if let Some(ref pub_content) = published_content {
                if content_differs(&content, pub_content) {
                    warnings.insert(0, "Modified since publish".into());
                }
            }

            // Parse visibility controls
            let unlisted = frontmatter
                .get("unlisted")
                .map(|v| v == "true" || v == "yes")
                .unwrap_or(false);
            let password = frontmatter.get("password").cloned();
            let dek = frontmatter.get("dek").cloned();
            let publish_at = frontmatter.get("publish_at").cloned();

            files.push(MarkdownFile {
                path: path.to_string_lossy().to_string(),
                filename,
                title,
                dek,
                date: frontmatter.get("date").cloned(),
                tags: parse_tags(&frontmatter),
                created,
                modified,
                word_count: body.split_whitespace().count(),
                is_safe: warnings.is_empty(),
                warnings,
                published_url,
                published_date,
                source_dir,
                unlisted,
                password,
                publish_at,
                content_type: content_type.into(),
            });
        }
    }

    files.sort_by(|a, b| b.modified.cmp(&a.modified));
    files.truncate(limit);
    Ok(files)
}

fn get_timestamp(time: std::io::Result<SystemTime>) -> u64 {
    time.unwrap_or(SystemTime::UNIX_EPOCH)
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Parse a week note filename like "2025-37.md" or "2025-52-raw.md" into a timestamp
fn parse_weeknote_filename(path: &Path) -> Option<u64> {
    let stem = path.file_stem()?.to_str()?;
    let mut parts = stem.split('-');
    let year: i32 = parts.next()?.parse().ok()?;
    let week_str = parts.next()?;
    // Extract leading digits from week part (handles "52-raw" → "52")
    let week_digits: String = week_str.chars().take_while(|c| c.is_ascii_digit()).collect();
    let week: u32 = week_digits.parse().ok()?;
    if !(2000..=2100).contains(&year) || week == 0 || week > 53 {
        return None;
    }
    use chrono::NaiveDate;
    let date = NaiveDate::from_isoywd_opt(year, week, chrono::Weekday::Mon)?;
    let dt = date.and_hms_opt(0, 0, 0)?;
    Some(dt.and_utc().timestamp() as u64)
}

fn parse_iso_date(date_str: &str) -> Option<u64> {
    // Parse ISO 8601 dates like "2024-01-01T00:00:00-05:00" or "2024-01-01"
    use chrono::{DateTime, NaiveDate};

    // Try full datetime with timezone
    if let Ok(dt) = DateTime::parse_from_rfc3339(date_str) {
        return Some(dt.timestamp() as u64);
    }

    // Try datetime without timezone (assume UTC)
    if let Ok(dt) = date_str.parse::<DateTime<chrono::Utc>>() {
        return Some(dt.timestamp() as u64);
    }

    // Try just date
    if let Ok(date) = NaiveDate::parse_from_str(date_str, "%Y-%m-%d") {
        return Some(date.and_hms_opt(0, 0, 0)?.and_utc().timestamp() as u64);
    }

    None
}

fn find_published_info(
    website_repo: &str,
    slug: &str,
) -> (Option<String>, Option<u64>, Option<String>) {
    let target = config::default_target();
    // Derive blog content base from content_path_pattern (e.g. "content/blog/{year}" -> "content/blog")
    let content_base = target
        .content_path_pattern
        .split("/{year}")
        .next()
        .unwrap_or("content/blog");
    let blog_path = format!("{}/{}", website_repo, content_base);

    if let Ok(entries) = fs::read_dir(&blog_path) {
        for entry in entries.flatten() {
            if entry.path().is_dir() {
                let dir_name = entry.file_name().to_string_lossy().to_string();
                // Only look in year folders (4 digits), skip drafts/projects/etc
                if dir_name.len() != 4 || !dir_name.chars().all(|c| c.is_ascii_digit()) {
                    continue;
                }
                let file_path = format!("{}/{}/{}.md", blog_path, dir_name, slug);
                let path = Path::new(&file_path);
                if path.exists() {
                    let url = format!("{}/blog/{}/{}", target.domain, dir_name, slug);
                    let date = fs::metadata(path)
                        .and_then(|m| m.modified())
                        .map(|t| get_timestamp(Ok(t)))
                        .ok();
                    let published_content = fs::read_to_string(path).ok();
                    return (Some(url), date, published_content);
                }
            }
        }
    }
    (None, None, None)
}

fn normalize_content(content: &str) -> String {
    // Extract body after frontmatter and normalize whitespace
    let body = if let Some(after_start) = content.strip_prefix("---") {
        if let Some(end) = after_start.find("---") {
            after_start[end + 3..].to_string()
        } else {
            content.to_string()
        }
    } else {
        content.to_string()
    };

    // Normalize: trim, collapse whitespace, remove trailing newlines
    body.trim()
        .lines()
        .map(|l| l.trim_end())
        .collect::<Vec<_>>()
        .join("\n")
}

fn content_differs(source: &str, published: &str) -> bool {
    // Simple comparison - if files are byte-identical, no diff
    if source == published {
        return false;
    }
    // Fall back to normalized comparison
    normalize_content(source) != normalize_content(published)
}

fn parse_frontmatter(content: &str) -> (HashMap<String, String>, String) {
    let mut frontmatter = HashMap::new();
    let mut body = content.to_string();

    if let Some(after_start) = content.strip_prefix("---") {
        if let Some(end) = after_start.find("---") {
            let yaml = &after_start[..end];
            body = after_start[end + 3..].to_string();

            let lines: Vec<&str> = yaml.lines().collect();
            let mut i = 0;
            while i < lines.len() {
                let line = lines[i];
                if let Some((key, value)) = line.split_once(':') {
                    let key = key.trim().to_string();
                    let value = value.trim().trim_matches('"').to_string();

                    // Handle YAML block scalar indicators (>-, >, |-, |)
                    if value == ">-" || value == ">" || value == "|-" || value == "|" {
                        // Read indented continuation lines
                        let mut block_lines = Vec::new();
                        while i + 1 < lines.len() {
                            let next = lines[i + 1];
                            if next.starts_with(' ') || next.starts_with('\t') {
                                block_lines.push(next.trim());
                                i += 1;
                            } else {
                                break;
                            }
                        }
                        let joined = if value.starts_with('>') {
                            block_lines.join(" ") // folded: join with spaces
                        } else {
                            block_lines.join("\n") // literal: preserve newlines
                        };
                        frontmatter.insert(key, joined);
                    } else {
                        frontmatter.insert(key, value);
                    }
                }
                i += 1;
            }
        }
    }

    (frontmatter, body)
}

fn extract_h1_title(body: &str) -> Option<String> {
    // Find first heading (# or ##)
    body.lines()
        .find(|line| {
            let trimmed = line.trim();
            trimmed.starts_with("# ") || trimmed.starts_with("## ")
        })
        .map(|line| {
            let trimmed = line.trim();
            if let Some(stripped) = trimmed.strip_prefix("## ") {
                stripped.trim().to_string()
            } else if let Some(stripped) = trimmed.strip_prefix("# ") {
                stripped.trim().to_string()
            } else {
                trimmed.to_string()
            }
        })
}

fn parse_tags(frontmatter: &HashMap<String, String>) -> Vec<String> {
    frontmatter
        .get("tags")
        .map(|t| {
            t.trim_matches(|c| c == '[' || c == ']')
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect()
        })
        .unwrap_or_default()
}

pub fn add_tag_to_file(path: &str, tag: &str) -> Result<(), String> {
    let content = fs::read_to_string(path).map_err(|e| format!("Failed to read file: {}", e))?;

    // Parse frontmatter
    if !content.starts_with("---") {
        // No frontmatter - add it with the tag
        let new_content = format!("---\ntags: [{}]\n---\n{}", tag, content);
        fs::write(path, new_content).map_err(|e| format!("Failed to write file: {}", e))?;
        return Ok(());
    }

    // Find frontmatter boundaries
    let end_pos = content[3..]
        .find("---")
        .ok_or("Invalid frontmatter: no closing ---")?;
    let fm_content = &content[3..end_pos + 3];
    let body = &content[end_pos + 6..];

    // Check if tags line exists - filter empty lines to keep frontmatter clean
    let mut lines: Vec<String> = fm_content
        .lines()
        .map(|l| l.to_string())
        .filter(|l| !l.trim().is_empty())
        .collect();
    let mut found_tags = false;

    for line in lines.iter_mut() {
        if line.starts_with("tags:") {
            // Parse existing tags and add the new one
            let existing = line.trim_start_matches("tags:").trim();
            let mut tags: Vec<String> = if existing.starts_with('[') {
                // Array format: [tag1, tag2]
                existing
                    .trim_matches(|c| c == '[' || c == ']')
                    .split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect()
            } else {
                // Single value or empty
                if existing.is_empty() {
                    vec![]
                } else {
                    vec![existing.to_string()]
                }
            };

            // Don't add duplicate
            if !tags.iter().any(|t| t.eq_ignore_ascii_case(tag)) {
                tags.push(tag.to_string());
            }

            *line = format!("tags: [{}]", tags.join(", "));
            found_tags = true;
            break;
        }
    }

    if !found_tags {
        // Insert tags after first line (or at end of frontmatter)
        lines.push(format!("tags: [{}]", tag));
    }

    // Rebuild the file
    let new_content = format!("---\n{}\n---{}", lines.join("\n"), body);
    fs::write(path, new_content).map_err(|e| format!("Failed to write file: {}", e))?;

    Ok(())
}

pub fn set_frontmatter_field(path: &str, key: &str, value: &str) -> Result<(), String> {
    let content = fs::read_to_string(path).map_err(|e| format!("Failed to read file: {}", e))?;

    if !content.starts_with("---") {
        // No frontmatter - add it with the field
        let new_content = format!("---\n{}: {}\n---\n{}", key, value, content);
        fs::write(path, new_content).map_err(|e| format!("Failed to write file: {}", e))?;
        return Ok(());
    }

    let end_pos = content[3..]
        .find("---")
        .ok_or("Invalid frontmatter: no closing ---")?;
    let fm_content = &content[3..end_pos + 3];
    let body = &content[end_pos + 6..];

    let mut lines: Vec<String> = fm_content
        .lines()
        .map(|l| l.to_string())
        .filter(|l| !l.trim().is_empty())
        .collect();

    let mut found = false;
    for line in lines.iter_mut() {
        if line.starts_with(&format!("{}:", key)) {
            *line = format!("{}: {}", key, value);
            found = true;
            break;
        }
    }

    if !found {
        lines.push(format!("{}: {}", key, value));
    }

    let new_content = format!("---\n{}\n---{}", lines.join("\n"), body);
    fs::write(path, new_content).map_err(|e| format!("Failed to write file: {}", e))?;
    Ok(())
}

pub fn remove_frontmatter_field(path: &str, key: &str) -> Result<(), String> {
    let content = fs::read_to_string(path).map_err(|e| format!("Failed to read file: {}", e))?;

    if !content.starts_with("---") {
        return Ok(()); // No frontmatter, nothing to remove
    }

    let end_pos = content[3..]
        .find("---")
        .ok_or("Invalid frontmatter: no closing ---")?;
    let fm_content = &content[3..end_pos + 3];
    let body = &content[end_pos + 6..];

    let lines: Vec<String> = fm_content
        .lines()
        .map(|l| l.to_string())
        .filter(|l| !l.trim().is_empty())
        .filter(|l| !l.starts_with(&format!("{}:", key)))
        .collect();

    let new_content = format!("---\n{}\n---{}", lines.join("\n"), body);
    fs::write(path, new_content).map_err(|e| format!("Failed to write file: {}", e))?;
    Ok(())
}

fn check_warnings(
    body: &str,
    frontmatter: &HashMap<String, String>,
    _has_title: bool,
    content_type: &str,
) -> Vec<String> {
    let mut warnings = Vec::new();

    // Title is now optional - website will derive from filename if missing
    // Date is still required for proper sorting/display
    if !frontmatter.contains_key("date") {
        warnings.push("No date".into());
    }
    if body.contains("TODO") || body.contains("FIXME") {
        warnings.push("Has TODOs".into());
    }
    if body.contains("](./") || body.contains("](/attachments") {
        warnings.push("Local images".into());
    }

    // Privacy linter for weeknotes — flag PII before publishing
    if content_type == "weeknote" {
        let privacy_warnings = check_privacy(body);
        warnings.extend(privacy_warnings);
    }

    // Check for broken/empty links
    let broken_link_patterns = [
        "]()",     // Empty link
        "](#)",    // Empty anchor
        "](http)", // Incomplete http
        "[[]]",    // Empty wikilink
        "![](",    // Image with no alt text (not critical but worth noting)
    ];
    for pattern in broken_link_patterns {
        if body.contains(pattern) {
            warnings.push("Broken link".into());
            break;
        }
    }

    // Check for potentially broken image embeds
    let broken_image_patterns = [
        "![]()",          // Empty image
        "src=\"\"",       // Empty src in HTML
        "src=''",         // Empty src single quotes
        ".png)",          // Might be local
        ".jpg)",          // Might be local
        ".jpeg)",         // Might be local
        ".gif)",          // Might be local
        "](attachments/", // Obsidian attachments folder
        "](Attachments/", // Obsidian attachments (capitalized)
        "](assets/",      // Common local assets folder
        "](images/",      // Common local images folder
    ];

    let mut has_local_media = false;
    for pattern in broken_image_patterns {
        if body.contains(pattern) {
            // Skip if it's a cloudinary URL (those are fine)
            if !body.contains("cloudinary") || pattern == "![]()" || pattern == "src=\"\"" {
                has_local_media = true;
                break;
            }
        }
    }

    if has_local_media && !warnings.contains(&"Local images".to_string()) {
        warnings.push("Local media".into());
    }

    // Check for broken HTML embeds
    if body.contains("<img")
        && !body.contains("cloudinary")
        && !body.contains("https://")
        && !warnings.contains(&"Local media".to_string())
    {
        warnings.push("Local media".into());
    }

    // Check for video embeds that might be broken
    if (body.contains("<video") || body.contains(".mp4)") || body.contains(".webm)"))
        && !body.contains("cloudinary")
        && !body.contains("https://")
    {
        warnings.push("Local video".into());
    }

    // Check for long link text (over 4 words)
    if has_long_link_text(body, 4) {
        warnings.push("Long link text".into());
    }

    warnings
}

fn has_long_link_text(body: &str, max_words: usize) -> bool {
    let link_re = Regex::new(r"(!)?\[([^\]]+)\]\(([^)]+)\)").unwrap();
    for caps in link_re.captures_iter(body) {
        if caps.get(1).is_some() {
            continue;
        }
        let text = caps.get(2).map(|m| m.as_str()).unwrap_or("");
        let word_count = text.split_whitespace().count();
        if word_count > max_words {
            return true;
        }
    }
    false
}

/// Privacy linter for weeknotes — flags content that might be too personal for public publishing.
/// Uses regex patterns to detect PII: phone numbers, emails, dollar amounts, SSNs,
/// addresses, and references to specific people by name.
fn check_privacy(body: &str) -> Vec<String> {
    let mut warnings = Vec::new();

    // Phone numbers (US formats: 555-123-4567, (555) 123-4567, +1 555 123 4567)
    let phone_re = Regex::new(r"(?:\+1[\s.-]?)?\(?\d{3}\)?[\s.-]\d{3}[\s.-]\d{4}").unwrap();
    if phone_re.is_match(body) {
        warnings.push("[privacy]Phone number detected".into());
    }

    // Email addresses
    let email_re = Regex::new(r"[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}").unwrap();
    if email_re.is_match(body) {
        warnings.push("[privacy]Email address detected".into());
    }

    // Dollar amounts ($X,XXX or $XX,XXX+ — flag larger amounts as financial info)
    let money_re = Regex::new(r"\$\d{1,3}(?:,\d{3})+(?:\.\d{2})?|\$\d{4,}(?:\.\d{2})?").unwrap();
    if money_re.is_match(body) {
        warnings.push("[privacy]Financial amount detected".into());
    }

    // SSN patterns (XXX-XX-XXXX)
    let ssn_re = Regex::new(r"\b\d{3}-\d{2}-\d{4}\b").unwrap();
    if ssn_re.is_match(body) {
        warnings.push("[privacy]Possible SSN detected".into());
    }

    // Street addresses (number + street name + type)
    let addr_re = Regex::new(r"\b\d{1,5}\s+[A-Z][a-z]+(?:\s+[A-Z][a-z]+)*\s+(?:St|Ave|Blvd|Dr|Rd|Ln|Ct|Way|Pl|Circle|Terrace|Court)\b").unwrap();
    if addr_re.is_match(body) {
        warnings.push("[privacy]Street address detected".into());
    }

    // "Met with [Name]" / "talked to [Name]" / "called [Name]" patterns
    // Catches "met with John Smith", "called Sarah", "lunch with Mike", etc.
    let people_re = Regex::new(r"(?i)\b(?:met with|talked to|called|lunch with|dinner with|coffee with|meeting with|spoke with|visited|hanging out with|texted|emailed|DMed)\s+([A-Z][a-z]+(?:\s+[A-Z][a-z]+)?)").unwrap();
    if people_re.is_match(body) {
        warnings.push("[privacy]Named person reference".into());
    }

    // Health/medical terms
    let health_re = Regex::new(r"(?i)\b(?:diagnosis|prescribed|medication|therapist|therapy session|doctor(?:'s)? appointment|blood (?:test|pressure|work)|symptoms?|mg\s+(?:of|daily)|medical|hospital|surgery|prescription)\b").unwrap();
    if health_re.is_match(body) {
        warnings.push("[privacy]Health/medical info".into());
    }

    warnings
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_add_tag_to_file() {
        let test_file = "/tmp/test-tag.md";

        // Create test file with frontmatter
        std::fs::write(test_file, "---\ndate: 2026-01-31\n---\n\n# Test\n").unwrap();

        // Add a tag
        add_tag_to_file(test_file, "politics").unwrap();

        let content = std::fs::read_to_string(test_file).unwrap();
        assert!(
            content.contains("tags: [politics]"),
            "Should have tags: {}",
            content
        );

        // Add another tag
        add_tag_to_file(test_file, "coding").unwrap();

        let content = std::fs::read_to_string(test_file).unwrap();
        assert!(
            content.contains("politics") && content.contains("coding"),
            "Should have both tags: {}",
            content
        );

        // Try adding duplicate (should not duplicate)
        add_tag_to_file(test_file, "politics").unwrap();

        let content = std::fs::read_to_string(test_file).unwrap();
        let count = content.matches("politics").count();
        assert_eq!(count, 1, "Should not duplicate: {}", content);
    }

    #[test]
    fn test_set_frontmatter_field() {
        let test_file = "/tmp/test-set-field.md";

        // Test adding field to existing frontmatter
        std::fs::write(test_file, "---\ndate: 2026-01-31\n---\n\n# Test\n").unwrap();
        set_frontmatter_field(test_file, "publish_at", "2026-03-01T09:00:00Z").unwrap();
        let content = std::fs::read_to_string(test_file).unwrap();
        assert!(
            content.contains("publish_at: 2026-03-01T09:00:00Z"),
            "Should have publish_at: {}",
            content
        );
        assert!(
            content.contains("date: 2026-01-31"),
            "Should preserve existing fields: {}",
            content
        );

        // Test overwriting existing field
        set_frontmatter_field(test_file, "publish_at", "2026-04-01T12:00:00Z").unwrap();
        let content = std::fs::read_to_string(test_file).unwrap();
        assert!(
            content.contains("publish_at: 2026-04-01T12:00:00Z"),
            "Should overwrite: {}",
            content
        );
        assert_eq!(
            content.matches("publish_at").count(),
            1,
            "Should only have one publish_at: {}",
            content
        );

        // Test adding field when no frontmatter exists
        std::fs::write(test_file, "# Just a heading\n\nSome text.").unwrap();
        set_frontmatter_field(test_file, "publish_at", "2026-05-01T00:00:00Z").unwrap();
        let content = std::fs::read_to_string(test_file).unwrap();
        assert!(
            content.starts_with("---\npublish_at: 2026-05-01T00:00:00Z\n---\n"),
            "Should create frontmatter: {}",
            content
        );
        assert!(
            content.contains("# Just a heading"),
            "Should preserve body: {}",
            content
        );
    }

    #[test]
    fn test_remove_frontmatter_field() {
        let test_file = "/tmp/test-remove-field.md";

        // Test removing a field
        std::fs::write(
            test_file,
            "---\ndate: 2026-01-31\npublish_at: 2026-03-01T09:00:00Z\ntags: [test]\n---\n\n# Test\n",
        )
        .unwrap();
        remove_frontmatter_field(test_file, "publish_at").unwrap();
        let content = std::fs::read_to_string(test_file).unwrap();
        assert!(
            !content.contains("publish_at"),
            "Should remove publish_at: {}",
            content
        );
        assert!(
            content.contains("date: 2026-01-31"),
            "Should preserve date: {}",
            content
        );
        assert!(
            content.contains("tags: [test]"),
            "Should preserve tags: {}",
            content
        );

        // Test removing non-existent field (should be no-op)
        let before = std::fs::read_to_string(test_file).unwrap();
        remove_frontmatter_field(test_file, "nonexistent").unwrap();
        let after = std::fs::read_to_string(test_file).unwrap();
        assert_eq!(before, after, "Removing non-existent field should be no-op");

        // Test on file without frontmatter (should be no-op)
        std::fs::write(test_file, "# No frontmatter\n").unwrap();
        remove_frontmatter_field(test_file, "anything").unwrap();
        let content = std::fs::read_to_string(test_file).unwrap();
        assert_eq!(content, "# No frontmatter\n", "Should be unchanged");
    }

    #[test]
    fn test_parse_frontmatter_multiline_tags() {
        let content = "---\ntags:\n  - weekly-notes\n  - reflections\n  - progress\ndate: 2026-03-01T16:00:00-05:00\nmodified: 2026-03-01T19:04:57-05:00\n---\n## Week 2026-W09\n";
        let (fm, body) = parse_frontmatter(content);
        assert_eq!(fm.get("date").unwrap(), "2026-03-01T16:00:00-05:00", "date should be parsed");
        assert_eq!(fm.get("modified").unwrap(), "2026-03-01T19:04:57-05:00", "modified should be parsed");
        assert!(body.contains("## Week 2026-W09"), "body should contain heading");
        // Verify date actually parses to a timestamp
        assert!(parse_iso_date("2026-03-01T16:00:00-05:00").is_some(), "date should parse to timestamp");
        assert!(parse_iso_date("2026-03-01T19:04:57-05:00").is_some(), "modified should parse to timestamp");
    }

    #[test]
    fn test_privacy_linter() {
        // Phone number
        let warnings = check_privacy("Called them at 555-123-4567 today");
        assert!(warnings.iter().any(|w| w.contains("Phone")), "Should detect phone: {:?}", warnings);

        // Email
        let warnings = check_privacy("Emailed john@example.com about the project");
        assert!(warnings.iter().any(|w| w.contains("Email")), "Should detect email: {:?}", warnings);

        // Money
        let warnings = check_privacy("Got paid $12,500 for the project");
        assert!(warnings.iter().any(|w| w.contains("Financial")), "Should detect money: {:?}", warnings);

        // Named person
        let warnings = check_privacy("Met with Sarah Johnson for coffee");
        assert!(warnings.iter().any(|w| w.contains("Named person")), "Should detect person: {:?}", warnings);

        // Health
        let warnings = check_privacy("Had my therapy session on Tuesday");
        assert!(warnings.iter().any(|w| w.contains("Health")), "Should detect health: {:?}", warnings);

        // Clean content
        let warnings = check_privacy("This week I worked on the website redesign and wrote some code.");
        assert!(warnings.is_empty(), "Should be clean: {:?}", warnings);
    }

    #[test]
    fn test_parse_frontmatter_with_publish_at() {
        let content = "---\ndate: 2026-01-31\npublish_at: 2026-03-01T09:00:00Z\n---\n\n# Test\n";
        let (fm, body) = parse_frontmatter(content);
        assert_eq!(fm.get("publish_at").unwrap(), "2026-03-01T09:00:00Z");
        assert_eq!(fm.get("date").unwrap(), "2026-01-31");
        assert!(body.contains("# Test"));
    }
}
