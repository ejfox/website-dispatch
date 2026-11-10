// Portfolio projects: vault `blog/projects/<slug>.md` notes are processed by
// website2's blog pipeline, but their canonical public home is
// `/projects/<slug>`. Keep that distinction in one place so scanning,
// publishing, diffs, and syndication all agree.

use crate::config::PublishTarget;
use std::fs;
use std::path::Path;

/// True only for a note below the vault's `blog/projects/` collection.
pub fn is_project_note(vault_root: &str, source_path: &str) -> bool {
    let root = vault_root.replace('\\', "/");
    let source = source_path.replace('\\', "/");
    source
        .strip_prefix(root.trim_end_matches('/'))
        .is_some_and(|rel| rel.starts_with("/blog/projects/") && rel.ends_with(".md"))
}

/// The non-year portion of a target's configured blog destination.
/// `content/blog/{year}` becomes `content/blog`.
fn content_base(target: &PublishTarget) -> &str {
    target
        .content_path_pattern
        .split("/{year}")
        .next()
        .unwrap_or("content/blog")
        .trim_end_matches('/')
}

pub fn relative_dest(target: &PublishTarget, slug: &str) -> String {
    format!("{}/projects/{}.md", content_base(target), slug)
}

pub fn public_url(domain: &str, slug: &str) -> String {
    format!("{}/projects/{}", domain.trim_end_matches('/'), slug)
}

/// Published project metadata in the same shape used by the vault scanner.
pub fn find_published(
    target: &PublishTarget,
    slug: &str,
) -> (Option<String>, Option<u64>, Option<String>) {
    let path = Path::new(&target.repo_path).join(relative_dest(target, slug));
    if !path.exists() {
        return (None, None, None);
    }
    let content = fs::read_to_string(path).ok();
    // website2 keeps draft projects in this same collection for local preview,
    // but production deliberately returns 404 for them. A repo file is not by
    // itself proof that the project is publicly live.
    let is_draft = content.as_deref().is_some_and(|raw| {
        let (fm, _) = crate::vault::parse_frontmatter(raw);
        fm.get("draft")
            .is_some_and(|v| matches!(v.as_str(), "true" | "yes"))
    });
    let url = (!is_draft).then(|| public_url(&target.domain, slug));
    let date = (!is_draft)
        .then(|| content.as_deref().and_then(crate::vault::published_date_of))
        .flatten();
    (url, date, content)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognizes_only_the_projects_collection() {
        assert!(is_project_note(
            "/vault",
            "/vault/blog/projects/websdr-vcv.md"
        ));
        assert!(!is_project_note("/vault", "/vault/blog/2026/projects.md"));
        assert!(!is_project_note(
            "/vault",
            "/other/blog/projects/websdr-vcv.md"
        ));
    }

    #[test]
    fn canonical_destination_and_url_are_project_shaped() {
        let target = PublishTarget {
            name: "Website".into(),
            id: "website".into(),
            repo_path: "/repo".into(),
            domain: "https://ejfox.com/".into(),
            content_path_pattern: "content/blog/{year}".into(),
            branch: "main".into(),
            is_default: true,
        };
        assert_eq!(
            relative_dest(&target, "websdr-vcv"),
            "content/blog/projects/websdr-vcv.md"
        );
        assert_eq!(
            public_url(&target.domain, "websdr-vcv"),
            "https://ejfox.com/projects/websdr-vcv"
        );
    }

    #[test]
    fn repo_draft_is_not_reported_as_publicly_live() {
        let root =
            std::env::temp_dir().join(format!("dispatch-project-draft-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("content/blog/projects")).unwrap();
        fs::write(
            root.join("content/blog/projects/private-preview.md"),
            "---\ntitle: Private Preview\ndate: 2026-01-02\ndraft: true\n---\nbody\n",
        )
        .unwrap();
        let target = PublishTarget {
            name: "Website".into(),
            id: "website".into(),
            repo_path: root.to_string_lossy().into_owned(),
            domain: "https://ejfox.com".into(),
            content_path_pattern: "content/blog/{year}".into(),
            branch: "main".into(),
            is_default: true,
        };

        let (url, date, content) = find_published(&target, "private-preview");
        assert_eq!(url, None);
        assert_eq!(date, None);
        assert!(content.is_some());
        let _ = fs::remove_dir_all(root);
    }
}
