use crate::{Config, MarkdownFile};
use regex::Regex;
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::time::SystemTime;
use walkdir::WalkDir;

pub fn get_recent_files(limit: usize) -> Result<Vec<MarkdownFile>, String> {
    let config = Config::default();
    let mut files: Vec<MarkdownFile> = Vec::new();

    // Only scan publishable folders: blog/ and drafts/
    let publishable_dirs = vec!["blog", "drafts"];

    for entry in WalkDir::new(&config.vault_path)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().map_or(false, |ext| ext == "md"))
    {
        let path = entry.path();
        let path_str = path.to_string_lossy();

        // Only include files from publishable directories
        let in_publishable = publishable_dirs
            .iter()
            .any(|dir| path_str.contains(&format!("/{}/", dir)));

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

        if let Ok(metadata) = fs::metadata(path) {
            let fs_modified = get_timestamp(metadata.modified());
            let fs_created = metadata
                .created()
                .map(|t| get_timestamp(Ok(t)))
                .unwrap_or(fs_modified);

            let content = fs::read_to_string(path).unwrap_or_default();
            let (frontmatter, body) = parse_frontmatter(&content);

            // Prefer frontmatter dates over filesystem dates
            let modified = frontmatter.get("modified")
                .and_then(|d| parse_iso_date(d))
                .unwrap_or(fs_modified);
            let created = frontmatter.get("date")
                .and_then(|d| parse_iso_date(d))
                .unwrap_or(fs_created);
            let title = extract_h1_title(&body);
            let filename = path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            let slug = filename.trim_end_matches(".md");

            let (published_url, published_date, published_content) = find_published_info(&config.website_repo, slug);
            let source_dir = path
                .parent()
                .and_then(|p| p.strip_prefix(&config.vault_path).ok())
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_default();

            let mut warnings = check_warnings(&body, &frontmatter, title.is_some());

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

fn find_published_info(website_repo: &str, slug: &str) -> (Option<String>, Option<u64>, Option<String>) {
    // Posts are in content/blog/{year}/
    let blog_path = format!("{}/content/blog", website_repo);

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
                    let url = format!("https://ejfox.com/blog/{}/{}", dir_name, slug);
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
    let body = if content.starts_with("---") {
        if let Some(end) = content[3..].find("---") {
            content[end + 6..].to_string()
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

    if content.starts_with("---") {
        if let Some(end) = content[3..].find("---") {
            let yaml = &content[3..end + 3];
            body = content[end + 6..].to_string();

            for line in yaml.lines() {
                if let Some((key, value)) = line.split_once(':') {
                    frontmatter.insert(
                        key.trim().to_string(),
                        value.trim().trim_matches('"').to_string(),
                    );
                }
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
            if trimmed.starts_with("## ") {
                trimmed[3..].trim().to_string()
            } else {
                trimmed[2..].trim().to_string()
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

fn check_warnings(body: &str, frontmatter: &HashMap<String, String>, _has_title: bool) -> Vec<String> {
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

    // Check for broken/empty links
    let broken_link_patterns = [
        "]()",           // Empty link
        "](#)",          // Empty anchor
        "](http)",       // Incomplete http
        "[[]]",          // Empty wikilink
        "![](",          // Image with no alt text (not critical but worth noting)
    ];
    for pattern in broken_link_patterns {
        if body.contains(pattern) {
            warnings.push("Broken link".into());
            break;
        }
    }

    // Check for potentially broken image embeds
    let broken_image_patterns = [
        "![]()",                    // Empty image
        "src=\"\"",                 // Empty src in HTML
        "src=''",                   // Empty src single quotes
        ".png)",                    // Might be local
        ".jpg)",                    // Might be local
        ".jpeg)",                   // Might be local
        ".gif)",                    // Might be local
        "](attachments/",           // Obsidian attachments folder
        "](Attachments/",           // Obsidian attachments (capitalized)
        "](assets/",                // Common local assets folder
        "](images/",                // Common local images folder
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
    if body.contains("<img") && !body.contains("cloudinary") && !body.contains("https://") {
        if !warnings.contains(&"Local media".to_string()) {
            warnings.push("Local media".into());
        }
    }

    // Check for video embeds that might be broken
    if body.contains("<video") || body.contains(".mp4)") || body.contains(".webm)") {
        if !body.contains("cloudinary") && !body.contains("https://") {
            warnings.push("Local video".into());
        }
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
        assert!(content.contains("tags: [politics]"), "Should have tags: {}", content);
        
        // Add another tag
        add_tag_to_file(test_file, "coding").unwrap();
        
        let content = std::fs::read_to_string(test_file).unwrap();
        assert!(content.contains("politics") && content.contains("coding"), "Should have both tags: {}", content);
        
        // Try adding duplicate (should not duplicate)
        add_tag_to_file(test_file, "politics").unwrap();
        
        let content = std::fs::read_to_string(test_file).unwrap();
        let count = content.matches("politics").count();
        assert_eq!(count, 1, "Should not duplicate: {}", content);
    }
}
