//! Dispatch pieces — standalone journalism, published from the vault folder
//! `dispatch/` to website2 `content/dispatch/<slug>.md` (no year folder),
//! served at `<domain>/dispatch/<slug>`.
//!
//! Same "one back door" rule as blog/: putting a note in `dispatch/` is the
//! publish intent; visibility comes from frontmatter (unlisted/password).
//!
//! Kept separate from publish.rs on purpose: publish_file() hands dispatch
//! notes here with a single early return, so blog publishing is untouched.

use crate::config::PublishTarget;
use std::fs;
use std::path::Path;

/// Vault folder that marks a note as a Dispatch piece.
pub const VAULT_DIR: &str = "dispatch";
/// Destination folder inside the website repo.
pub const CONTENT_DIR: &str = "content/dispatch";

/// True when `path` lives under `<vault_root>/dispatch/`.
pub fn is_dispatch_note(vault_root: &str, path: &str) -> bool {
    let path = path.replace('\\', "/");
    let root = vault_root.replace('\\', "/");
    let root = root.trim_end_matches('/');
    if root.is_empty() {
        return false;
    }
    path.strip_prefix(root)
        .map(|rest| rest.starts_with(&format!("/{}/", VAULT_DIR)))
        .unwrap_or(false)
}

/// Slugs map straight to a filename, so refuse anything path-like.
pub fn validate_slug(slug: &str) -> Result<(), String> {
    let ok = !slug.is_empty()
        && !slug.starts_with('.')
        && !slug.contains('/')
        && !slug.contains('\\')
        && !slug.contains("..");
    if ok {
        Ok(())
    } else {
        Err(format!("Invalid dispatch slug: '{}'", slug))
    }
}

/// Repo-relative destination: `content/dispatch/<slug>.md`.
pub fn relative_dest(slug: &str) -> String {
    format!("{}/{}.md", CONTENT_DIR, slug)
}

pub fn dest_path(repo_path: &str, slug: &str) -> String {
    format!(
        "{}/{}",
        repo_path.trim_end_matches('/'),
        relative_dest(slug)
    )
}

/// Public URL: `<domain>/dispatch/<slug>`. Domain may or may not carry a scheme.
pub fn public_url(domain: &str, slug: &str) -> String {
    let domain = domain.trim_end_matches('/');
    let domain = if domain.starts_with("http://") || domain.starts_with("https://") {
        domain.to_string()
    } else {
        format!("https://{}", domain)
    };
    format!("{}/dispatch/{}", domain, slug)
}

/// (url, mtime, content) of the published copy, mirroring vault::find_published_info.
pub fn find_published(
    target: &PublishTarget,
    slug: &str,
) -> (Option<String>, Option<u64>, Option<String>) {
    let path = dest_path(&target.repo_path, slug);
    let p = Path::new(&path);
    if !p.exists() {
        return (None, None, None);
    }
    let mtime = fs::metadata(p)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs());
    (
        Some(public_url(&target.domain, slug)),
        mtime,
        fs::read_to_string(p).ok(),
    )
}

/// Copy the vault note to content/dispatch/<slug>.md, then commit and push
/// to `origin <target.branch>`: same sequence and safety checks as blog
/// publishing (publish.rs).
pub fn publish(source_path: &str, slug: &str, target: &PublishTarget) -> Result<String, String> {
    validate_slug(slug)?;
    let repo = &target.repo_path;
    crate::publish::check_git_status(repo, &target.branch)?;
    crate::publish::sync_with_remote(repo, &target.branch)?;

    let dest = dest_path(repo, slug);
    let rel = relative_dest(slug);
    if let Some(parent) = Path::new(&dest).parent() {
        fs::create_dir_all(parent).map_err(|e| format!("Failed to create dir: {}", e))?;
    }
    log::warn!("Publishing dispatch {} -> {}", source_path, dest);
    fs::copy(source_path, &dest).map_err(|e| format!("Failed to copy: {}", e))?;

    crate::publish::commit_and_push(
        repo,
        &target.branch,
        &[&rel],
        &format!("Publish dispatch: {}", slug),
    )?;
    Ok(public_url(&target.domain, slug))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dispatch_note_detection() {
        let vault = "/Users/ej/Library/Mobile Documents/iCloud~md~obsidian/Documents/ejfox";
        assert!(is_dispatch_note(
            vault,
            &format!("{}/dispatch/water.md", vault)
        ));
        assert!(is_dispatch_note(
            &format!("{}/", vault),
            &format!("{}/dispatch/sub/water.md", vault)
        ));
        assert!(!is_dispatch_note(
            vault,
            &format!("{}/blog/2026/water.md", vault)
        ));
        // A folder merely named "dispatch" deeper in the tree is not the dispatch root.
        assert!(!is_dispatch_note(
            vault,
            &format!("{}/blog/dispatch/water.md", vault)
        ));
        assert!(!is_dispatch_note(
            vault,
            &format!("{}/dispatches/water.md", vault)
        ));
        assert!(!is_dispatch_note(vault, "/elsewhere/dispatch/water.md"));
    }

    #[test]
    fn dispatch_path_and_url_mapping() {
        assert_eq!(
            relative_dest("water-wars"),
            "content/dispatch/water-wars.md"
        );
        assert_eq!(
            dest_path("/Users/ej/code/website2/", "water-wars"),
            "/Users/ej/code/website2/content/dispatch/water-wars.md"
        );
        assert_eq!(
            public_url("https://ejfox.com", "water-wars"),
            "https://ejfox.com/dispatch/water-wars"
        );
        assert_eq!(
            public_url("ejfox.com/", "water-wars"),
            "https://ejfox.com/dispatch/water-wars"
        );
        // No year folder, unlike blog/{year}.
        assert!(!relative_dest("x").contains("20"));
    }

    #[test]
    fn slug_validation() {
        assert!(validate_slug("water-wars").is_ok());
        assert!(validate_slug("").is_err());
        assert!(validate_slug("../etc").is_err());
        assert!(validate_slug("a/b").is_err());
        assert!(validate_slug(".hidden").is_err());
    }

    #[test]
    fn find_published_maps_repo_copy_to_url() {
        let repo = std::env::temp_dir().join(format!("dispatch-test-repo-{}", std::process::id()));
        let _ = fs::remove_dir_all(&repo);
        fs::create_dir_all(repo.join(CONTENT_DIR)).unwrap();
        fs::write(
            repo.join("content/dispatch/piece.md"),
            "---\ntitle: P\n---\nbody",
        )
        .unwrap();
        let target = PublishTarget {
            name: "Website".into(),
            id: "website".into(),
            repo_path: repo.to_string_lossy().to_string(),
            domain: "https://ejfox.com".into(),
            content_path_pattern: "content/blog/{year}".into(),
            branch: "main".into(),
            is_default: true,
        };
        let (url, date, content) = find_published(&target, "piece");
        assert_eq!(url.as_deref(), Some("https://ejfox.com/dispatch/piece"));
        assert!(date.is_some());
        assert!(content.unwrap().contains("body"));
        assert_eq!(find_published(&target, "missing").0, None);
        let _ = fs::remove_dir_all(&repo);
    }
}
