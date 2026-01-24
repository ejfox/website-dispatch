use crate::{Config, MarkdownFile};
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
            let modified = get_timestamp(metadata.modified());
            let created = metadata
                .created()
                .map(|t| get_timestamp(Ok(t)))
                .unwrap_or(modified);

            let content = fs::read_to_string(path).unwrap_or_default();
            let (frontmatter, body) = parse_frontmatter(&content);
            let title = extract_h1_title(&body);
            let filename = path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            let slug = filename.trim_end_matches(".md");

            let (published_url, published_date) = find_published_info(&config.website_repo, slug);
            let source_dir = path
                .parent()
                .and_then(|p| p.strip_prefix(&config.vault_path).ok())
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_default();

            let warnings = check_warnings(&body, &frontmatter, title.is_some());

            files.push(MarkdownFile {
                path: path.to_string_lossy().to_string(),
                filename,
                title,
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

fn find_published_info(website_repo: &str, slug: &str) -> (Option<String>, Option<u64>) {
    let blog_path = format!("{}/content/blog", website_repo);

    if let Ok(entries) = fs::read_dir(&blog_path) {
        for entry in entries.flatten() {
            if entry.path().is_dir() {
                let year = entry.file_name().to_string_lossy().to_string();
                let file_path = format!("{}/{}/{}.md", blog_path, year, slug);
                let path = Path::new(&file_path);
                if path.exists() {
                    let url = format!("https://ejfox.com/blog/{}/{}", year, slug);
                    let date = fs::metadata(path)
                        .and_then(|m| m.modified())
                        .map(|t| get_timestamp(Ok(t)))
                        .ok();
                    return (Some(url), date);
                }
            }
        }
    }
    (None, None)
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

fn check_warnings(body: &str, frontmatter: &HashMap<String, String>, has_title: bool) -> Vec<String> {
    let mut warnings = Vec::new();

    if !has_title {
        warnings.push("No title".into());
    }
    if !frontmatter.contains_key("date") {
        warnings.push("No date".into());
    }
    if body.contains("TODO") || body.contains("FIXME") {
        warnings.push("Has TODOs".into());
    }
    if body.contains("](./") || body.contains("](/attachments") {
        warnings.push("Local images".into());
    }

    warnings
}
