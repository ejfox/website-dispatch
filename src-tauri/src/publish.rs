use crate::config;
use std::fs;
use std::io::Read;
use std::path::Path;
use std::process::{Command, Output, Stdio};
use std::thread;
use std::time::{Duration, Instant};
use tauri::Emitter;

/// Local git operations (status, add, commit) should never take this long.
const LOCAL_TIMEOUT: Duration = Duration::from_secs(30);
/// Network git operations (pull, push). A hung remote fails instead of
/// hanging the app forever.
const NETWORK_TIMEOUT: Duration = Duration::from_secs(60);

/// Run a command, killing it if it outlives `timeout`.
fn run_with_timeout(mut cmd: Command, timeout: Duration) -> Result<Output, String> {
    let mut child = cmd
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| e.to_string())?;
    // Drain the pipes on their own threads so a chatty child can't deadlock
    // on a full pipe buffer while we poll.
    let mut out = child.stdout.take().ok_or("no stdout")?;
    let mut err = child.stderr.take().ok_or("no stderr")?;
    let out_reader = thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = out.read_to_end(&mut buf);
        buf
    });
    let err_reader = thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = err.read_to_end(&mut buf);
        buf
    });
    let deadline = Instant::now() + timeout;
    let status = loop {
        if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
            break status;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err(format!("timed out after {}s", timeout.as_secs()));
        }
        thread::sleep(Duration::from_millis(25));
    };
    Ok(Output {
        status,
        stdout: out_reader.join().unwrap_or_default(),
        stderr: err_reader.join().unwrap_or_default(),
    })
}

/// Run git in `repo` with a timeout. Never prompts for credentials.
pub(crate) fn git(repo: &str, args: &[&str], timeout: Duration) -> Result<Output, String> {
    let mut cmd = Command::new(crate::bin_paths::git());
    cmd.args(args)
        .current_dir(repo)
        .env("GIT_TERMINAL_PROMPT", "0");
    run_with_timeout(cmd, timeout).map_err(|e| format!("git {} failed: {}", args.join(" "), e))
}

/// Like `git`, but a non-zero exit is an error carrying git's own message.
fn git_ok(repo: &str, args: &[&str], timeout: Duration) -> Result<String, String> {
    let out = git(repo, args, timeout)?;
    if !out.status.success() {
        return Err(format!(
            "git {} failed: {}{}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim(),
            String::from_utf8_lossy(&out.stdout).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// Pre-flight: a git repo, on `expected_branch`, with no merge conflicts.
/// Never switches branches or stashes; that repo may hold work in progress.
pub(crate) fn check_git_status(repo_path: &str, expected_branch: &str) -> Result<(), String> {
    git_ok(repo_path, &["rev-parse", "--git-dir"], LOCAL_TIMEOUT)
        .map_err(|_| "Not a git repository".to_string())?;

    let branch = git_ok(repo_path, &["branch", "--show-current"], LOCAL_TIMEOUT)?;
    if branch.is_empty() {
        return Err("Detached HEAD state - please checkout a branch".into());
    }
    if branch != expected_branch {
        return Err(format!(
            "website2 is on {}, not {}: switch branches or publish from a clean {} checkout",
            branch, expected_branch, expected_branch
        ));
    }

    let conflicts = git_ok(
        repo_path,
        &["diff", "--name-only", "--diff-filter=U"],
        LOCAL_TIMEOUT,
    )?;
    if !conflicts.is_empty() {
        return Err("Merge conflicts detected - please resolve before publishing".into());
    }
    Ok(())
}

/// Bring the local branch up to date with origin before anything is written.
/// Fast-forward only: no rebase, no stash, so unrelated uncommitted work in
/// the repo is left exactly as it was. A diverged branch is refused.
pub(crate) fn sync_with_remote(repo_path: &str, branch: &str) -> Result<(), String> {
    git_ok(
        repo_path,
        &["pull", "--ff-only", "origin", branch],
        NETWORK_TIMEOUT,
    )
    .map(|_| ())
    .map_err(|e| {
        format!(
            "Couldn't update {} from origin (nothing was published): {}",
            branch, e
        )
    })
}

/// Commit exactly `paths` (other staged or dirty files are left alone) and
/// push to `origin <branch>`. Any failure is an error; a republish with no
/// changes skips the commit and still pushes.
pub(crate) fn commit_and_push(
    repo_path: &str,
    branch: &str,
    paths: &[&str],
    message: &str,
) -> Result<(), String> {
    let mut add = vec!["add", "-A", "--"];
    add.extend_from_slice(paths);
    git_ok(repo_path, &add, LOCAL_TIMEOUT)?;

    let mut staged = vec!["diff", "--cached", "--quiet", "--"];
    staged.extend_from_slice(paths);
    let has_changes = !git(repo_path, &staged, LOCAL_TIMEOUT)?.status.success();
    if has_changes {
        let mut commit = vec!["commit", "-m", message, "--"];
        commit.extend_from_slice(paths);
        git_ok(repo_path, &commit, LOCAL_TIMEOUT)?;
    }

    git_ok(repo_path, &["push", "origin", branch], NETWORK_TIMEOUT)?;
    Ok(())
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct GitStatus {
    pub ok: bool,
    pub branch: String,
    pub error: Option<String>,
    pub dirty_files: Vec<String>,
    pub has_conflicts: bool,
}

pub fn get_git_status(target_id: Option<&str>) -> GitStatus {
    let target = match config::resolve_target(target_id) {
        Ok(t) => t,
        Err(e) => {
            return GitStatus {
                ok: false,
                branch: String::new(),
                error: Some(e),
                dirty_files: vec![],
                has_conflicts: false,
            }
        }
    };
    let repo_path = &target.repo_path;

    // Get branch name
    let branch = Command::new(crate::bin_paths::git())
        .args(["branch", "--show-current"])
        .current_dir(repo_path)
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default();

    // Check for dirty files
    let status = Command::new(crate::bin_paths::git())
        .args(["status", "--porcelain"])
        .current_dir(repo_path)
        .output();

    let dirty_files: Vec<String> = status
        .map(|o| {
            String::from_utf8_lossy(&o.stdout)
                .lines()
                .filter(|l| !l.contains("content/blog/"))
                .map(|l| l.to_string())
                .collect()
        })
        .unwrap_or_default();

    // Check for conflicts
    let conflicts = Command::new(crate::bin_paths::git())
        .args(["diff", "--name-only", "--diff-filter=U"])
        .current_dir(repo_path)
        .output()
        .map(|o| !String::from_utf8_lossy(&o.stdout).trim().is_empty())
        .unwrap_or(false);

    let error = if conflicts {
        Some("Merge conflicts - resolve before publishing".into())
    } else if !dirty_files.is_empty() {
        Some(format!("{} uncommitted changes", dirty_files.len()))
    } else if branch.is_empty() {
        Some("Detached HEAD".into())
    } else if branch != target.branch {
        Some(format!("On {}, not {}", branch, target.branch))
    } else {
        None
    };

    GitStatus {
        ok: error.is_none(),
        branch,
        error,
        dirty_files,
        has_conflicts: conflicts,
    }
}

/// Where a vault blog note lands in the website repo, and the URL it serves
/// at. The one place that maps a vault file to its repo destination; path and
/// URL are derived together so they can't disagree.
#[derive(Debug, Clone, PartialEq)]
pub struct BlogDest {
    /// Bare slug: the vault file's name without `.md`.
    pub slug: String,
    /// Repo-relative path, e.g. `content/blog/2026/the-knife.md`.
    pub rel_path: String,
    /// Public URL, e.g. `https://ejfox.com/blog/2026/the-knife`.
    pub url: String,
    /// True when the post is already in the repo (a republish).
    pub existing: bool,
}

/// Bare slug from a vault path or a `year/slug` string: the last segment,
/// without `.md`. Anything path-like left over is refused.
pub fn bare_slug(s: &str) -> Result<String, String> {
    let last = s.replace('\\', "/");
    let last = last.trim_end_matches('/').rsplit('/').next().unwrap_or("");
    let slug = last.strip_suffix(".md").unwrap_or(last).to_string();
    if slug.is_empty() || slug.starts_with('.') || slug.contains("..") {
        return Err(format!("Invalid slug: '{}'", s));
    }
    Ok(slug)
}

/// `content/blog/{year}` -> `content/blog`.
fn content_base(target: &config::PublishTarget) -> &str {
    target
        .content_path_pattern
        .split("/{year}")
        .next()
        .unwrap_or("content/blog")
        .trim_end_matches('/')
}

/// Repo-relative path of an already-published `<slug>.md` anywhere under the
/// content base (a year folder, projects/, week-notes/). Skips private/
/// (sealed posts) and drafts/. From EJ's republish-in-place WIP.
fn find_existing_copy(repo_path: &str, content_base: &str, slug: &str) -> Option<String> {
    let file_name = format!("{}.md", slug);
    let base = Path::new(repo_path).join(content_base);
    let mut hits: Vec<String> = walkdir::WalkDir::new(&base)
        .max_depth(3)
        .into_iter()
        .flatten()
        .filter(|e| e.file_type().is_file() && e.file_name().to_string_lossy() == file_name)
        .filter_map(|e| {
            let rel = e
                .path()
                .strip_prefix(repo_path)
                .ok()?
                .to_string_lossy()
                .to_string();
            let private = rel.split('/').any(|c| c == "private" || c == "drafts");
            (!private).then_some(rel)
        })
        .collect();
    hits.sort(); // deterministic if a slug somehow exists twice
    hits.into_iter().next()
}

/// Map a vault note to its repo destination.
///
/// `<vault>/blog/<YYYY>/<slug>.md` -> `content/blog/<YYYY>/<slug>.md`;
/// `<vault>/blog/<slug>.md` (no year folder) -> `current_year`. A post already
/// in the repo is updated where it lives, never duplicated into another year.
pub fn blog_destination(
    target: &config::PublishTarget,
    vault_root: &str,
    source_path: &str,
    current_year: &str,
) -> Result<BlogDest, String> {
    let source = source_path.replace('\\', "/");
    let root = vault_root.replace('\\', "/");
    let rel = source
        .strip_prefix(root.trim_end_matches('/'))
        .and_then(|r| r.strip_prefix("/blog/"))
        .ok_or("Publish blocked: file must live in the vault's blog/ folder")?;
    let slug = bare_slug(rel)?;
    let base = content_base(target);

    let (rel_path, existing) = match find_existing_copy(&target.repo_path, base, &slug) {
        Some(found) => (found, true),
        None => {
            let year = match rel.split('/').collect::<Vec<_>>()[..] {
                [y, _] if y.len() == 4 && y.chars().all(|c| c.is_ascii_digit()) => y,
                _ => current_year,
            };
            let dir = target.content_path_pattern.replace("{year}", year);
            (format!("{}/{}.md", dir.trim_end_matches('/'), slug), false)
        }
    };

    let route = rel_path
        .strip_prefix(&format!("{}/", base))
        .unwrap_or(&rel_path)
        .trim_end_matches(".md");
    let url = format!("{}/blog/{}", target.domain.trim_end_matches('/'), route);
    Ok(BlogDest {
        slug,
        rel_path,
        url,
        existing,
    })
}

/// Refuse to overwrite a repo copy that was edited outside Dispatch. The repo
/// copy is safe to replace only if it matches the vault copy, or if it has no
/// uncommitted edits and the last commit touching it was Dispatch's own
/// "Publish: <slug>". From EJ's clobber-guard WIP.
fn guard_against_clobber(
    repo_path: &str,
    source_path: &str,
    rel_path: &str,
    slug: &str,
) -> Result<(), String> {
    let dest = Path::new(repo_path).join(rel_path);
    if !dest.exists() {
        return Ok(());
    }
    let source = fs::read(source_path).map_err(|e| format!("Failed to read source: {}", e))?;
    let current = fs::read(&dest).map_err(|e| format!("Failed to read repo copy: {}", e))?;
    if source == current {
        return Ok(());
    }

    let dirty = git_ok(
        repo_path,
        &["status", "--porcelain", "--", rel_path],
        LOCAL_TIMEOUT,
    )?;
    if !dirty.is_empty() {
        return Err(format!(
            "Publish blocked: {} has uncommitted edits in the website repo. \
             Publishing would overwrite them with the vault copy.",
            rel_path
        ));
    }

    let last = git_ok(
        repo_path,
        &[
            "log",
            "-1",
            "--format=%h %ad %s",
            "--date=short",
            "--",
            rel_path,
        ],
        LOCAL_TIMEOUT,
    )?;
    if last.is_empty() || last.ends_with(&format!("Publish: {}", slug)) {
        return Ok(());
    }
    Err(format!(
        "Publish blocked: {} was edited in the website repo after it was last \
         published from the vault (last commit: {}). Publishing would replace \
         those edits with the vault copy. Bring the repo version into the vault \
         first, or keep editing it in the repo.",
        rel_path, last
    ))
}

/// Publish a vault blog note to `target`: pre-flight, fast-forward from
/// origin, copy, commit just that file, push `origin <branch>`.
pub fn publish_blog(
    target: &config::PublishTarget,
    vault_root: &str,
    source_path: &str,
    current_year: &str,
) -> Result<BlogDest, String> {
    let repo = &target.repo_path;
    // Validate the vault path before touching git at all.
    blog_destination(target, vault_root, source_path, current_year)?;
    check_git_status(repo, &target.branch)?;
    sync_with_remote(repo, &target.branch)?;

    // Resolve again after the pull so a copy that arrived from origin is found.
    let dest = blog_destination(target, vault_root, source_path, current_year)?;
    guard_against_clobber(repo, source_path, &dest.rel_path, &dest.slug)?;

    let abs = Path::new(repo).join(&dest.rel_path);
    if let Some(dir) = abs.parent() {
        fs::create_dir_all(dir).map_err(|e| format!("Failed to create dir: {}", e))?;
    }
    log::warn!("Publishing {} -> {}", source_path, dest.rel_path);
    fs::copy(source_path, &abs).map_err(|e| format!("Failed to copy: {}", e))?;

    commit_and_push(
        repo,
        &target.branch,
        &[&dest.rel_path],
        &format!("Publish: {}", dest.slug),
    )?;
    log::warn!("Pushed {} to origin/{}", dest.rel_path, target.branch);
    Ok(dest)
}

/// Result of a successful publish (blog post or dispatch piece).
#[derive(Debug, Clone)]
pub struct Published {
    pub url: String,
    pub slug: String,
    pub republish: bool,
}

/// Publish a vault note using the app config. Blocking (git + network): call
/// it through `off_main`, never on the UI thread.
pub fn publish_file(source_path: &str, target_id: Option<&str>) -> Result<Published, String> {
    let app_config = config::get()?;
    let target = config::resolve_target(target_id)?;
    // Dispatch pieces (vault dispatch/) → content/dispatch/<slug>.md.
    if crate::dispatch::is_dispatch_note(&app_config.vault.path, source_path) {
        let slug = bare_slug(source_path)?;
        let republish = crate::dispatch::find_published(&target, &slug).0.is_some();
        let url = crate::dispatch::publish(source_path, &slug, &target)?;
        return Ok(Published {
            url,
            slug,
            republish,
        });
    }
    let year = chrono::Utc::now().format("%Y").to_string();
    let dest = publish_blog(&target, &app_config.vault.path, source_path, &year)?;
    Ok(Published {
        url: dest.url,
        slug: dest.slug,
        republish: dest.existing,
    })
}

/// Run blocking publish work on tokio's blocking pool so git and network
/// calls never freeze the UI thread.
pub async fn off_main<T, F>(f: F) -> Result<T, String>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, String> + Send + 'static,
{
    tokio::task::spawn_blocking(f)
        .await
        .map_err(|e| format!("Publish task failed: {}", e))?
}

/// Move a published post to `content/drafts/` and push. Accepts a bare slug
/// or a `year/slug` string.
pub fn unpublish_file(slug: &str, target_id: Option<&str>) -> Result<(), String> {
    let target = config::resolve_target(target_id)?;
    let slug = bare_slug(slug)?;
    let repo = &target.repo_path;

    check_git_status(repo, &target.branch)?;
    sync_with_remote(repo, &target.branch)?;

    let source_rel = find_existing_copy(repo, content_base(&target), &slug)
        .ok_or_else(|| "Published file not found".to_string())?;
    let dest_rel = format!("content/drafts/{}.md", slug);
    let dest_abs = Path::new(repo).join(&dest_rel);
    if dest_abs.exists() {
        return Err(format!("Draft already exists: {}", dest_rel));
    }
    if let Some(dir) = dest_abs.parent() {
        fs::create_dir_all(dir).map_err(|e| format!("Failed to create drafts dir: {}", e))?;
    }

    log::warn!("Unpublishing {} -> {}", source_rel, dest_rel);
    fs::rename(Path::new(repo).join(&source_rel), &dest_abs)
        .map_err(|e| format!("Failed to move file: {}", e))?;

    commit_and_push(
        repo,
        &target.branch,
        &[&source_rel, &dest_rel],
        &format!("Unpublish: {}", slug),
    )?;
    log::warn!("Unpublished successfully!");
    Ok(())
}

pub async fn run_schedule_checker(app_handle: tauri::AppHandle) {
    use tokio::time::{interval, Duration};
    let mut ticker = interval(Duration::from_secs(60));

    loop {
        ticker.tick().await;
        if let Ok(files) = crate::vault::get_recent_files(500) {
            let now = chrono::Utc::now();
            for file in files {
                if let Some(ref publish_at_str) = file.publish_at {
                    // Skip already-published files
                    if file.published_url.is_some() {
                        continue;
                    }
                    // Try parsing the scheduled datetime
                    if let Ok(scheduled_time) = chrono::DateTime::parse_from_rfc3339(publish_at_str)
                    {
                        if now >= scheduled_time {
                            let slug = file.filename.trim_end_matches(".md");
                            log::warn!("Scheduled publish triggered for: {}", slug);
                            let path = file.path.clone();
                            match off_main(move || publish_file(&path, None)).await {
                                Ok(Published { url, .. }) => {
                                    // Remove publish_at from frontmatter
                                    let _ = crate::vault::remove_frontmatter_field(
                                        &file.path,
                                        "publish_at",
                                    );
                                    // Emit event to frontend
                                    let _ = app_handle.emit(
                                        "scheduled-publish",
                                        serde_json::json!({
                                            "slug": slug,
                                            "url": url,
                                            "title": file.title
                                        }),
                                    );
                                    log::warn!("Scheduled publish succeeded: {}", url);
                                }
                                Err(e) => {
                                    log::warn!("Scheduled publish failed for {}: {}", slug, e);
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// Throwaway setup: a bare `remote.git` (main + release), a working clone,
    /// and a vault with `blog/2026/fresh.md` and `blog/2026/old.md`. The repo
    /// already has `content/blog/2025/old.md`, published by Dispatch.
    struct Fixture {
        root: PathBuf,
        remote: String,
        work: String,
        vault: String,
    }

    fn sh(dir: &str, args: &[&str]) -> String {
        let out = Command::new("git")
            .args(args)
            .current_dir(dir)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "git {:?}: {}",
            args,
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    }

    impl Fixture {
        fn new(name: &str, checkout: &str) -> Fixture {
            let root = std::env::temp_dir().join(format!(
                "dispatch-publish-{}-{}",
                std::process::id(),
                name
            ));
            let _ = fs::remove_dir_all(&root);
            fs::create_dir_all(&root).unwrap();
            let r = root.to_string_lossy().to_string();
            let remote = format!("{}/remote.git", r);
            let work = format!("{}/work", r);
            let vault = format!("{}/vault", r);

            sh(&r, &["init", "-q", "--bare", "-b", "main", &remote]);
            sh(&r, &["clone", "-q", &remote, &work]);
            sh(&work, &["checkout", "-q", "-b", "main"]);
            for (k, v) in [
                ("user.name", "Test"),
                ("user.email", "test@example.com"),
                ("commit.gpgsign", "false"),
            ] {
                sh(&work, &["config", k, v]);
            }
            fs::create_dir_all(format!("{}/content/blog/2025", work)).unwrap();
            fs::write(format!("{}/README", work), "repo\n").unwrap();
            sh(&work, &["add", "README"]);
            sh(&work, &["commit", "-q", "-m", "init"]);
            fs::write(format!("{}/content/blog/2025/old.md", work), "old v1\n").unwrap();
            sh(&work, &["add", "content"]);
            sh(&work, &["commit", "-q", "-m", "Publish: old"]);
            sh(&work, &["push", "-q", "-u", "origin", "main"]);
            sh(&work, &["push", "-q", "origin", "main:release"]);
            if checkout != "main" {
                sh(&work, &["checkout", "-q", "-B", checkout, "origin/release"]);
                if checkout == "release" {
                    sh(&work, &["branch", "-q", "-u", "origin/release"]);
                }
            }

            fs::create_dir_all(format!("{}/blog/2026", vault)).unwrap();
            fs::write(format!("{}/blog/2026/fresh.md", vault), "fresh\n").unwrap();
            fs::write(format!("{}/blog/2026/old.md", vault), "old v2\n").unwrap();
            fs::write(format!("{}/blog/undated.md", vault), "undated\n").unwrap();
            Fixture {
                root,
                remote,
                work,
                vault,
            }
        }

        fn target(&self, branch: &str) -> config::PublishTarget {
            config::PublishTarget {
                name: "Website".into(),
                id: "website".into(),
                repo_path: self.work.clone(),
                domain: "https://ejfox.com".into(),
                content_path_pattern: "content/blog/{year}".into(),
                branch: branch.into(),
                is_default: true,
            }
        }

        fn note(&self, rel: &str) -> String {
            format!("{}/{}", self.vault, rel)
        }

        /// File contents on the remote branch, or None if absent.
        fn remote_file(&self, branch: &str, path: &str) -> Option<String> {
            let out = Command::new("git")
                .args([
                    "--git-dir",
                    &self.remote,
                    "show",
                    &format!("{}:{}", branch, path),
                ])
                .output()
                .unwrap();
            out.status
                .success()
                .then(|| String::from_utf8_lossy(&out.stdout).to_string())
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    #[test]
    fn destination_uses_vault_year_folder() {
        let f = Fixture::new("dest", "main");
        let d = blog_destination(
            &f.target("main"),
            &f.vault,
            &f.note("blog/2026/fresh.md"),
            "2031",
        )
        .unwrap();
        assert_eq!(d.rel_path, "content/blog/2026/fresh.md");
        assert_eq!(d.url, "https://ejfox.com/blog/2026/fresh");
        assert_eq!(d.slug, "fresh");
        assert!(!d.existing);

        // No year folder in the vault: falls back to the current year.
        let d = blog_destination(
            &f.target("main"),
            &f.vault,
            &f.note("blog/undated.md"),
            "2031",
        )
        .unwrap();
        assert_eq!(d.rel_path, "content/blog/2031/undated.md");

        // Outside blog/ is refused.
        assert!(
            blog_destination(&f.target("main"), &f.vault, &f.note("drafts/x.md"), "2031").is_err()
        );
    }

    #[test]
    fn slug_is_normalized_from_any_shape() {
        assert_eq!(bare_slug("2026/the-knife").unwrap(), "the-knife");
        assert_eq!(bare_slug("the-knife").unwrap(), "the-knife");
        assert_eq!(bare_slug("/v/blog/2026/the-knife.md").unwrap(), "the-knife");
        assert!(bare_slug("").is_err());
        assert!(bare_slug("2026/..").is_err());
    }

    #[test]
    fn publish_lands_at_the_right_path_on_main() {
        let f = Fixture::new("publish", "main");
        let d = publish_blog(
            &f.target("main"),
            &f.vault,
            &f.note("blog/2026/fresh.md"),
            "2026",
        )
        .unwrap();
        assert_eq!(d.url, "https://ejfox.com/blog/2026/fresh");
        assert_eq!(
            f.remote_file("main", "content/blog/2026/fresh.md")
                .as_deref(),
            Some("fresh\n")
        );
        assert!(f
            .remote_file("main", "content/blog/2026/2026/fresh.md")
            .is_none());
        assert_eq!(sh(&f.work, &["log", "-1", "--format=%s"]), "Publish: fresh");
    }

    #[test]
    fn republish_updates_older_year_in_place_with_matching_url() {
        let f = Fixture::new("republish", "main");
        let d = publish_blog(
            &f.target("main"),
            &f.vault,
            &f.note("blog/2026/old.md"),
            "2026",
        )
        .unwrap();
        assert!(d.existing);
        assert_eq!(d.rel_path, "content/blog/2025/old.md");
        assert_eq!(d.url, "https://ejfox.com/blog/2025/old");
        assert_eq!(
            f.remote_file("main", "content/blog/2025/old.md").as_deref(),
            Some("old v2\n")
        );
        assert!(f.remote_file("main", "content/blog/2026/old.md").is_none());
    }

    #[test]
    fn refuses_on_the_wrong_branch() {
        let f = Fixture::new("wrongbranch", "fix/x");
        let err = publish_blog(
            &f.target("main"),
            &f.vault,
            &f.note("blog/2026/fresh.md"),
            "2026",
        )
        .unwrap_err();
        assert!(err.contains("website2 is on fix/x, not main"), "{}", err);
        // Nothing written, nothing pushed, still on fix/x.
        assert!(!Path::new(&f.work)
            .join("content/blog/2026/fresh.md")
            .exists());
        assert!(f
            .remote_file("main", "content/blog/2026/fresh.md")
            .is_none());
        assert_eq!(sh(&f.work, &["branch", "--show-current"]), "fix/x");
    }

    #[test]
    fn pushes_to_the_configured_branch() {
        let f = Fixture::new("release", "release");
        publish_blog(
            &f.target("release"),
            &f.vault,
            &f.note("blog/2026/fresh.md"),
            "2026",
        )
        .unwrap();
        assert_eq!(
            f.remote_file("release", "content/blog/2026/fresh.md")
                .as_deref(),
            Some("fresh\n")
        );
        assert!(f
            .remote_file("main", "content/blog/2026/fresh.md")
            .is_none());
    }

    #[test]
    fn refuses_to_clobber_uncommitted_edits() {
        let f = Fixture::new("dirty", "main");
        fs::write(
            format!("{}/content/blog/2025/old.md", f.work),
            "hand edit\n",
        )
        .unwrap();
        let err = publish_blog(
            &f.target("main"),
            &f.vault,
            &f.note("blog/2026/old.md"),
            "2026",
        )
        .unwrap_err();
        assert!(err.contains("uncommitted edits"), "{}", err);
        assert_eq!(
            fs::read_to_string(format!("{}/content/blog/2025/old.md", f.work)).unwrap(),
            "hand edit\n"
        );
    }

    #[test]
    fn unrelated_staged_work_is_not_swept_into_the_publish_commit() {
        let f = Fixture::new("staged", "main");
        fs::write(format!("{}/wip.txt", f.work), "wip\n").unwrap();
        sh(&f.work, &["add", "wip.txt"]);
        publish_blog(
            &f.target("main"),
            &f.vault,
            &f.note("blog/2026/fresh.md"),
            "2026",
        )
        .unwrap();
        assert!(f.remote_file("main", "wip.txt").is_none());
        assert_eq!(sh(&f.work, &["status", "--porcelain"]), "A  wip.txt");
    }

    #[test]
    fn commit_failure_is_an_error() {
        let f = Fixture::new("hook", "main");
        let hook = format!("{}/.git/hooks/pre-commit", f.work);
        fs::write(&hook, "#!/bin/sh\necho nope >&2\nexit 1\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&hook, fs::Permissions::from_mode(0o755)).unwrap();
        }
        let err = publish_blog(
            &f.target("main"),
            &f.vault,
            &f.note("blog/2026/fresh.md"),
            "2026",
        )
        .unwrap_err();
        assert!(
            err.contains("git commit") && err.contains("nope"),
            "{}",
            err
        );
        assert!(f
            .remote_file("main", "content/blog/2026/fresh.md")
            .is_none());
    }

    #[test]
    fn hung_command_times_out_as_an_error() {
        let mut cmd = Command::new("sleep");
        cmd.arg("10");
        let start = Instant::now();
        let err = run_with_timeout(cmd, Duration::from_millis(200)).unwrap_err();
        assert!(err.contains("timed out"), "{}", err);
        assert!(start.elapsed() < Duration::from_secs(3));
    }

    #[tokio::test]
    async fn publish_work_runs_off_the_calling_thread() {
        let caller = thread::current().id();
        let start = Instant::now();
        let handle = tokio::spawn(off_main(move || {
            thread::sleep(Duration::from_millis(300));
            Ok(thread::current().id())
        }));
        // The caller is free immediately, not after the 300ms of "git".
        assert!(start.elapsed() < Duration::from_millis(100));
        let worker = handle.await.unwrap().unwrap();
        assert_ne!(worker, caller);
    }
}
