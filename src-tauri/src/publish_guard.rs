//! Hard backend refusal for content that must never reach the public site.
//!
//! Week notes leaked client screenshots, finances and people's names, and were
//! retired from ejfox.com. `drafts/` is private thinking. Neither may be
//! published, whatever the config or the UI says — this guard runs first in
//! `publish::publish_file`, so no UI path (or a stale config listing these
//! folders as publishable) can bypass it.

use std::path::Path;

/// Vault folders that are never publishable. Matched as whole path
/// components, at any depth (`week-notes/x.md`, `blog/week-notes/x.md`).
pub const NEVER_PUBLISHABLE_DIRS: &[&str] = &["week-notes", "drafts"];

/// Frontmatter `type:` values that mark a week note.
const WEEKNOTE_TYPES: &[&str] = &["weeknote", "weeknotes", "week-note", "week-notes"];

/// True if `dir` (a config entry like `"week-notes"` or `"blog/week-notes"`)
/// names, or sits under, a never-publishable folder.
pub fn is_never_publishable_dir(dir: &str) -> bool {
    dir.replace('\\', "/")
        .split('/')
        .any(|c| NEVER_PUBLISHABLE_DIRS.contains(&c.trim().to_lowercase().as_str()))
}

/// Refuse to publish `source_path` if it lives under a never-publishable
/// folder or is typed as a week note, or if `slug` is empty. Reads the file's
/// frontmatter for the type check (an unreadable file is left to fail later).
pub fn refuse_unpublishable(source_path: &str, slug: &str) -> Result<(), String> {
    let content = std::fs::read_to_string(source_path).unwrap_or_default();
    check_publishable(source_path, &content, slug)
}

/// Pure version of [`refuse_unpublishable`], for tests.
pub fn check_publishable(source_path: &str, content: &str, slug: &str) -> Result<(), String> {
    let normalized = source_path.replace('\\', "/");
    if let Some(dir) = Path::new(&normalized)
        .parent()
        .into_iter()
        .flat_map(|p| p.components())
        .filter_map(|c| c.as_os_str().to_str())
        .find(|c| NEVER_PUBLISHABLE_DIRS.contains(&c.to_lowercase().as_str()))
    {
        return Err(format!(
            "Publish refused: files in a {}/ folder are never published (week notes are retired from the site; drafts/ is private). {}",
            dir, source_path
        ));
    }
    if let Some(t) = frontmatter_type(content) {
        if WEEKNOTE_TYPES.contains(&t.as_str()) {
            return Err(format!(
                "Publish refused: `type: {}` marks a week note, and week notes are retired from the site. {}",
                t, source_path
            ));
        }
    }
    if slug.trim().is_empty() || slug.trim_matches('/').is_empty() {
        return Err(format!(
            "Publish refused: empty slug for {}",
            source_path
        ));
    }
    Ok(())
}

/// The lowercased `type:` value from leading YAML frontmatter, if any.
fn frontmatter_type(content: &str) -> Option<String> {
    let rest = content.trim_start_matches('\u{feff}').strip_prefix("---")?;
    let end = rest.find("\n---").unwrap_or(rest.len());
    rest[..end].lines().find_map(|line| {
        let (key, value) = line.split_once(':')?;
        if key.trim() != "type" || line.starts_with([' ', '\t']) {
            return None;
        }
        Some(
            value
                .trim()
                .trim_matches(|c| c == '"' || c == '\'')
                .trim()
                .to_lowercase(),
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const V: &str = "/Users/x/Documents/ejfox";

    fn check(rel: &str, content: &str) -> Result<(), String> {
        check_publishable(&format!("{}/{}", V, rel), content, "2026/x")
    }

    #[test]
    fn refuses_week_notes_dir() {
        assert!(check("week-notes/2026-01-25.md", "").is_err());
    }

    #[test]
    fn refuses_drafts_dir() {
        assert!(check("drafts/x.md", "").is_err());
    }

    #[test]
    fn refuses_week_notes_nested_in_blog() {
        assert!(check("blog/week-notes/x.md", "").is_err());
    }

    #[test]
    fn refuses_weeknote_type_anywhere() {
        let fm = "---\ntitle: Hi\ntype: weeknote\n---\nbody";
        assert!(check("blog/2026/x.md", fm).is_err());
        let quoted = "---\ntype: \"Week-Note\"\n---\n";
        assert!(check("blog/2026/x.md", quoted).is_err());
    }

    #[test]
    fn allows_blog_post() {
        assert!(check("blog/2026/x.md", "---\ntitle: Hi\ntype: post\n---\nbody").is_ok());
        assert!(check("blog/2026/x.md", "no frontmatter").is_ok());
        // `type:` in the body (not frontmatter) doesn't count
        assert!(check("blog/2026/x.md", "---\ntitle: a\n---\ntype: weeknote").is_ok());
    }

    #[test]
    fn refuses_empty_slug() {
        let p = format!("{}/blog/2026/x.md", V);
        assert!(check_publishable(&p, "", "").is_err());
        assert!(check_publishable(&p, "", "  ").is_err());
        assert!(check_publishable(&p, "", "/").is_err());
    }

    #[test]
    fn filename_containing_drafts_is_fine() {
        assert!(check("blog/2026/drafts-of-history.md", "").is_ok());
    }

    #[test]
    fn dir_matcher() {
        assert!(is_never_publishable_dir("week-notes"));
        assert!(is_never_publishable_dir("drafts"));
        assert!(is_never_publishable_dir("blog/week-notes/"));
        assert!(is_never_publishable_dir(" Drafts "));
        assert!(!is_never_publishable_dir("blog"));
        assert!(!is_never_publishable_dir("dispatch"));
    }
}
