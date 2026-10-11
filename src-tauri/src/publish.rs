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

/// Why a publish can't go ahead, in words for the review sheet. `kind` is
/// stable for the UI and tests; `message` is the sentence shown to EJ.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Blocker {
    pub kind: String,
    pub message: String,
}

impl Blocker {
    fn new(kind: &str, message: impl Into<String>) -> Blocker {
        Blocker {
            kind: kind.into(),
            message: message.into(),
        }
    }
}

/// Folder name of the website repo ("website2"), for plain-language messages.
fn repo_name(repo_path: &str) -> String {
    Path::new(repo_path.trim_end_matches('/'))
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| repo_path.to_string())
}

/// The branch checked out in `repo_path` ("" when detached).
pub(crate) fn current_branch(repo_path: &str) -> Result<String, String> {
    git_ok(repo_path, &["branch", "--show-current"], LOCAL_TIMEOUT)
}

/// Pre-flight: a git repo, on `expected_branch`, with no merge conflicts.
/// Never switches branches or stashes; that repo may hold work in progress.
/// Other uncommitted files don't block: a publish commits only its own path.
fn git_preflight(repo_path: &str, expected_branch: &str) -> Result<(), Blocker> {
    let name = repo_name(repo_path);
    if git_ok(repo_path, &["rev-parse", "--git-dir"], LOCAL_TIMEOUT).is_err() {
        return Err(Blocker::new(
            "not_a_repo",
            format!("{} isn't a git repository, so there's nothing to publish to.", repo_path),
        ));
    }
    let branch = current_branch(repo_path).map_err(|e| Blocker::new("git_error", e))?;
    if branch.is_empty() {
        return Err(Blocker::new(
            "detached",
            format!(
                "{} isn't on a branch (detached HEAD). Check out {} in {}, then come back.",
                name, expected_branch, name
            ),
        ));
    }
    if branch != expected_branch {
        return Err(Blocker::new(
            "wrong_branch",
            format!(
                "{} is on {}, not {}. Switch branches in {}, then come back.",
                name, branch, expected_branch, name
            ),
        ));
    }
    let conflicts = git_ok(
        repo_path,
        &["diff", "--name-only", "--diff-filter=U"],
        LOCAL_TIMEOUT,
    )
    .map_err(|e| Blocker::new("git_error", e))?;
    if !conflicts.is_empty() {
        return Err(Blocker::new(
            "conflicts",
            format!(
                "{} has unresolved merge conflicts. Resolve them, then come back.",
                name
            ),
        ));
    }
    Ok(())
}

pub(crate) fn check_git_status(repo_path: &str, expected_branch: &str) -> Result<(), String> {
    git_preflight(repo_path, expected_branch).map_err(|b| b.message)
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
fn clobber_verdict(
    repo_path: &str,
    source_path: &str,
    rel_path: &str,
    slug: &str,
) -> Result<(), Blocker> {
    let dest = Path::new(repo_path).join(rel_path);
    if !dest.exists() {
        return Ok(());
    }
    let read_err = |e: String| Blocker::new("unreadable", e);
    let source = fs::read(source_path).map_err(|e| read_err(format!("Failed to read source: {}", e)))?;
    let current = fs::read(&dest).map_err(|e| read_err(format!("Failed to read repo copy: {}", e)))?;
    if source == current {
        return Ok(());
    }
    let name = repo_name(repo_path);

    let dirty = git_ok(
        repo_path,
        &["status", "--porcelain", "--", rel_path],
        LOCAL_TIMEOUT,
    )
    .map_err(|e| Blocker::new("git_error", e))?;
    if !dirty.is_empty() {
        return Err(Blocker::new(
            "clobber_uncommitted",
            format!(
                "{} has uncommitted edits in {}, and publishing would overwrite them \
                 with the vault copy. Commit or discard them in {}, then come back.",
                rel_path, name, name
            ),
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
    )
    .map_err(|e| Blocker::new("git_error", e))?;
    if last.is_empty() || last.ends_with(&format!("Publish: {}", slug)) {
        return Ok(());
    }
    Err(Blocker::new(
        "clobber_edited",
        format!(
            "{} was edited in {} after it was last published from the vault \
             (last commit: {}). Publishing would replace those edits with the vault \
             copy. Bring the repo version into the vault first, or keep editing it \
             in the repo.",
            rel_path, name, last
        ),
    ))
}

fn guard_against_clobber(
    repo_path: &str,
    source_path: &str,
    rel_path: &str,
    slug: &str,
) -> Result<(), String> {
    clobber_verdict(repo_path, source_path, rel_path, slug)
        .map_err(|b| format!("Publish blocked: {}", b.message))
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
    // Hard refusal first: week-notes/, drafts/, `type: weeknote`, empty slug.
    // (publish_guard owns this; it runs before any config or path logic.)
    let guard_slug = bare_slug(source_path).unwrap_or_default();
    crate::publish_guard::refuse_unpublishable(source_path, &guard_slug)?;
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

// ── Publish preview ─────────────────────────────────────────────────────────
//
// What the review sheet states before anything is written. It runs the same
// checks as `publish_file`, in the same order, minus the writes: no pull, no
// copy, no commit, no push. `publish_file` re-runs every check after its
// `pull --ff-only`, so a preview that goes stale can't cause a wrong publish;
// the publish just fails with the newer reason.

/// Roughly how long website2 takes from push to live (GitHub Actions build +
/// deploy + healthcheck). Shown as "about 3 minutes".
pub const DEPLOY_ESTIMATE_SECS: u64 = 180;

/// A publish time read from frontmatter.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct WhenField {
    /// Frontmatter key it came from: `publishAt`, `date` or `publish_at`.
    pub key: String,
    /// The value as written.
    pub raw: String,
    /// The moment as UTC RFC 3339, when it carries or implies a zone.
    pub utc: Option<String>,
    /// A bare `YYYY-MM-DD`: the site reads it as UTC midnight.
    pub date_only: bool,
}

/// A syndication post that would go out. `mode` is `queued` (already in
/// Dispatch's queue with a time: it sends on its own) or `ask` (Dispatch
/// opens the share sheet and waits for EJ).
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct SyndicationPlan {
    pub network: String,
    pub mode: String,
    pub scheduled_at: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct PublishPreview {
    pub title: String,
    pub slug: String,
    /// `blog` or `dispatch`.
    pub kind: String,
    /// True when the post is already in the repo: this replaces it in place.
    pub is_update: bool,
    /// Public URL (unchanged on an update).
    pub url: String,
    /// Repo-relative path the note is copied to.
    pub rel_path: String,
    pub repo_name: String,
    pub repo_path: String,
    /// The configured branch publishes are pushed to.
    pub branch: String,
    /// The branch website2 has checked out right now ("" if detached).
    pub current_branch: String,
    pub on_branch: bool,
    /// Other uncommitted files in the repo. Informational: they stay out of
    /// the publish commit and don't block it.
    pub other_uncommitted: usize,
    /// The clobber guard's verdict on the destination file (blog posts only).
    pub clobber_ok: bool,
    /// `public`, `unlisted` or `password`.
    pub visibility: String,
    /// website2's request-time embargo (`publishAt`, else `date`), when it's
    /// in the future: the site deploys now and hides the post until then.
    pub embargo: Option<WhenField>,
    /// Dispatch's own `publish_at` hold, if set.
    pub dispatch_schedule: Option<WhenField>,
    /// Webmentions are sent once the deploy is confirmed live.
    pub webmentions_after_live: bool,
    /// ...and forwarded to the fediverse through Bridgy Fed.
    pub bridgy_fed: bool,
    pub syndication: Vec<SyndicationPlan>,
    pub deploy_estimate_secs: u64,
    /// The first reason the publish would be refused, in plain words.
    pub blocked: Option<Blocker>,
}

/// Parse a frontmatter time the way website2's `new Date(value)` would.
fn parse_when(key: &str, raw: &str) -> WhenField {
    use chrono::{DateTime, NaiveDate, Utc};
    let raw = raw.trim().trim_matches('\'').to_string();
    let date_only = NaiveDate::parse_from_str(&raw, "%Y-%m-%d").ok();
    let utc = DateTime::parse_from_rfc3339(&raw)
        .map(|d| d.with_timezone(&Utc))
        .ok()
        .or_else(|| date_only.and_then(|d| d.and_hms_opt(0, 0, 0)).map(|d| d.and_utc()))
        .map(|d| d.to_rfc3339_opts(chrono::SecondsFormat::Secs, true));
    WhenField {
        key: key.into(),
        raw,
        utc,
        date_only: date_only.is_some(),
    }
}

/// website2's `publishTime`: `publishAt || date`. Returned only while it's
/// in the future. A value with no zone (`"2026-10-01 09:00"`) can't be placed
/// in time from here, so it's returned with `utc: None` for the sheet to flag.
fn embargo_from(fm: &std::collections::HashMap<String, String>, now: chrono::DateTime<chrono::Utc>) -> Option<WhenField> {
    let (key, raw) = ["publishAt", "date"]
        .iter()
        .find_map(|k| fm.get(*k).filter(|v| !v.trim().is_empty()).map(|v| (*k, v)))?;
    let when = parse_when(key, raw);
    match &when.utc {
        Some(t) => chrono::DateTime::parse_from_rfc3339(t)
            .ok()
            .filter(|t| *t > now)
            .map(|_| when.clone()),
        // `date` without a zone is the common case for old posts; only an
        // explicit publishAt we can't place is worth flagging.
        None if key == "publishAt" => Some(when),
        None => None,
    }
}

fn visibility_of(fm: &std::collections::HashMap<String, String>) -> &'static str {
    if fm.get("password").is_some_and(|p| !p.trim().is_empty()) {
        "password"
    } else if fm
        .get("unlisted")
        .is_some_and(|v| v == "true" || v == "yes")
    {
        "unlisted"
    } else {
        "public"
    }
}

fn count_uncommitted(repo_path: &str, except: &str) -> usize {
    git_ok(repo_path, &["status", "--porcelain"], LOCAL_TIMEOUT)
        .map(|s| {
            s.lines()
                .filter(|l| !l.trim().is_empty() && !l.ends_with(except))
                .count()
        })
        .unwrap_or(0)
}

/// Everything about a publish that can be known from the vault note and the
/// local repo, without writing. Config-free so it can be tested against a
/// temp repo; `publish_preview` adds config-dependent extras.
pub fn preview_note(
    target: &config::PublishTarget,
    vault_root: &str,
    source_path: &str,
    current_year: &str,
    now: chrono::DateTime<chrono::Utc>,
) -> PublishPreview {
    let repo = &target.repo_path;
    let content = fs::read_to_string(source_path).unwrap_or_default();
    let (fm, body) = crate::vault::parse_frontmatter(&content);
    let is_dispatch = crate::dispatch::is_dispatch_note(vault_root, source_path);
    let slug = bare_slug(source_path).unwrap_or_default();
    let title = (if is_dispatch {
        fm.get("title").filter(|t| !t.is_empty()).cloned()
    } else {
        None
    })
    .or_else(|| crate::vault::extract_h1_title(&body))
    .or_else(|| fm.get("title").filter(|t| !t.is_empty()).cloned())
    .unwrap_or_else(|| slug.clone());

    let current = current_branch(repo).unwrap_or_default();
    let mut p = PublishPreview {
        title,
        slug: slug.clone(),
        kind: if is_dispatch { "dispatch" } else { "blog" }.into(),
        is_update: false,
        url: String::new(),
        rel_path: String::new(),
        repo_name: repo_name(repo),
        repo_path: repo.clone(),
        branch: target.branch.clone(),
        on_branch: !current.is_empty() && current == target.branch,
        current_branch: current,
        other_uncommitted: 0,
        clobber_ok: true,
        visibility: visibility_of(&fm).into(),
        embargo: embargo_from(&fm, now),
        dispatch_schedule: fm
            .get("publish_at")
            .filter(|v| !v.trim().is_empty())
            .map(|v| parse_when("publish_at", v)),
        webmentions_after_live: true,
        bridgy_fed: false,
        syndication: vec![],
        deploy_estimate_secs: DEPLOY_ESTIMATE_SECS,
        blocked: None,
    };

    // Same order as publish_file: hard refusal, destination, git, clobber.
    if let Err(e) = crate::publish_guard::refuse_unpublishable(source_path, &slug) {
        p.blocked = Some(Blocker::new("never_publishable", e));
        return p;
    }
    if is_dispatch {
        if let Err(e) = crate::dispatch::validate_slug(&slug) {
            p.blocked = Some(Blocker::new("bad_slug", e));
            return p;
        }
        p.rel_path = crate::dispatch::relative_dest(&slug);
        p.url = crate::dispatch::public_url(&target.domain, &slug);
        p.is_update = crate::dispatch::find_published(target, &slug).0.is_some();
    } else {
        match blog_destination(target, vault_root, source_path, current_year) {
            Ok(d) => {
                p.rel_path = d.rel_path;
                p.url = d.url;
                p.is_update = d.existing;
            }
            Err(e) => {
                p.blocked = Some(Blocker::new("outside_blog", e));
                return p;
            }
        }
    }
    p.other_uncommitted = count_uncommitted(repo, &p.rel_path);
    if let Err(b) = git_preflight(repo, &target.branch) {
        p.blocked = Some(b);
        return p;
    }
    // dispatch::publish has no clobber guard, so the preview doesn't either.
    if !is_dispatch {
        if let Err(b) = clobber_verdict(repo, source_path, &p.rel_path, &slug) {
            p.clobber_ok = false;
            p.blocked = Some(b);
        }
    }
    p
}

/// Syndication that would follow this publish: posts already queued with a
/// time for this slug (they send on their own), plus, for a first publish of
/// a Dispatch piece, the share sheet that opens and asks. Blog posts are
/// never posted anywhere automatically.
fn syndication_plan(p: &PublishPreview, source_path: &str) -> Vec<SyndicationPlan> {
    let year_slug = source_path
        .replace('\\', "/")
        .split("/blog/")
        .nth(1)
        .map(|r| r.trim_end_matches(".md").to_string());
    let mut keys = vec![p.slug.clone()];
    keys.extend(year_slug);
    keys.push(
        p.rel_path
            .trim_start_matches("content/blog/")
            .trim_end_matches(".md")
            .to_string(),
    );
    keys.sort();
    keys.dedup();

    let mut plan: Vec<SyndicationPlan> = keys
        .iter()
        .filter_map(|k| crate::syndication_queue::get_queue_for_post(k).ok())
        .flatten()
        .filter(|i| i.status == "scheduled" && i.scheduled_at.is_some())
        .map(|i| SyndicationPlan {
            network: i.platform,
            mode: "queued".into(),
            scheduled_at: i.scheduled_at,
        })
        .collect();

    if p.kind == "dispatch" && !p.is_update {
        let mut nets: Vec<String> = crate::omnipublish::network_status()
            .into_iter()
            .filter(|(_, ok)| *ok)
            .map(|(n, _)| n)
            .collect();
        nets.sort();
        plan.extend(nets.into_iter().map(|network| SyndicationPlan {
            network,
            mode: "ask".into(),
            scheduled_at: None,
        }));
    }
    plan
}

/// The `publish_preview` command: `preview_note` with the app config, plus
/// Bridgy Fed and syndication. Reads only. Blocking (git): call via `off_main`.
pub fn publish_preview(source_path: &str, target_id: Option<&str>) -> Result<PublishPreview, String> {
    let app_config = config::get()?;
    let target = config::resolve_target(target_id)?;
    let year = chrono::Utc::now().format("%Y").to_string();
    let mut p = preview_note(
        &target,
        &app_config.vault.path,
        source_path,
        &year,
        chrono::Utc::now(),
    );
    p.bridgy_fed = app_config.webmentions_bridgy_fed;
    if p.blocked.is_none() {
        p.syndication = syndication_plan(&p, source_path);
    }
    Ok(p)
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
            let work = format!("{}/website2", r);
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

    // ── publish_preview ──────────────────────────────────────────────────

    fn now() -> chrono::DateTime<chrono::Utc> {
        chrono::DateTime::parse_from_rfc3339("2026-10-11T12:00:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc)
    }

    /// Repo state that a preview must never change.
    fn snapshot(f: &Fixture) -> (String, String, String, Option<String>) {
        (
            sh(&f.work, &["status", "--porcelain"]),
            sh(&f.work, &["branch", "--show-current"]),
            sh(&f.work, &["rev-parse", "HEAD"]),
            f.remote_file("main", "content/blog/2026/fresh.md"),
        )
    }

    fn preview(f: &Fixture, branch: &str, note: &str) -> PublishPreview {
        let before = snapshot(f);
        let p = preview_note(&f.target(branch), &f.vault, &f.note(note), "2026", now());
        assert_eq!(snapshot(f), before, "preview wrote something");
        p
    }

    #[test]
    fn preview_of_a_new_post() {
        let f = Fixture::new("pv-new", "main");
        fs::write(f.note("blog/2026/fresh.md"), "# Paperclip\n\nbody\n").unwrap();
        let p = preview(&f, "main", "blog/2026/fresh.md");
        assert_eq!(p.blocked, None);
        assert_eq!(p.title, "Paperclip");
        assert!(!p.is_update);
        assert_eq!(p.url, "https://ejfox.com/blog/2026/fresh");
        assert_eq!(p.rel_path, "content/blog/2026/fresh.md");
        assert_eq!(p.repo_name, "website2");
        assert_eq!((p.branch.as_str(), p.current_branch.as_str()), ("main", "main"));
        assert!(p.on_branch && p.clobber_ok);
        assert_eq!(p.visibility, "public");
        assert_eq!(p.embargo, None);
        assert!(p.webmentions_after_live);
        assert_eq!(p.deploy_estimate_secs, 180);
        assert!(!Path::new(&f.work).join(&p.rel_path).exists());
    }

    #[test]
    fn preview_of_an_update_keeps_the_live_url() {
        let f = Fixture::new("pv-update", "main");
        let p = preview(&f, "main", "blog/2026/old.md");
        assert_eq!(p.blocked, None);
        assert!(p.is_update);
        assert_eq!(p.url, "https://ejfox.com/blog/2025/old");
        assert_eq!(p.rel_path, "content/blog/2025/old.md");
    }

    #[test]
    fn preview_on_the_wrong_branch_says_so() {
        let f = Fixture::new("pv-branch", "fix/x");
        let p = preview(&f, "main", "blog/2026/fresh.md");
        let b = p.blocked.expect("blocked");
        assert_eq!(b.kind, "wrong_branch");
        assert_eq!(
            b.message,
            "website2 is on fix/x, not main. Switch branches in website2, then come back."
        );
        assert_eq!(p.current_branch, "fix/x");
        assert!(!p.on_branch);
        // Still says where it would go, for context.
        assert_eq!(p.url, "https://ejfox.com/blog/2026/fresh");
    }

    #[test]
    fn preview_reports_the_clobber_guard() {
        let f = Fixture::new("pv-clobber", "main");
        fs::write(format!("{}/content/blog/2025/old.md", f.work), "hand edit\n").unwrap();
        let p = preview(&f, "main", "blog/2026/old.md");
        assert_eq!(p.blocked.expect("blocked").kind, "clobber_uncommitted");
        assert!(!p.clobber_ok);

        // Committed by hand, not by Dispatch: also refused.
        let f = Fixture::new("pv-clobber2", "main");
        fs::write(format!("{}/content/blog/2025/old.md", f.work), "hand edit\n").unwrap();
        sh(&f.work, &["commit", "-qam", "Fix typo"]);
        let p = preview(&f, "main", "blog/2026/old.md");
        assert_eq!(p.blocked.expect("blocked").kind, "clobber_edited");
    }

    #[test]
    fn unrelated_dirty_files_dont_block_the_preview() {
        let f = Fixture::new("pv-dirty", "main");
        fs::write(format!("{}/wip.txt", f.work), "wip\n").unwrap();
        sh(&f.work, &["add", "wip.txt"]);
        let p = preview(&f, "main", "blog/2026/fresh.md");
        assert_eq!(p.blocked, None);
        assert_eq!(p.other_uncommitted, 1);
    }

    #[test]
    fn preview_refuses_week_notes_and_outside_blog() {
        let f = Fixture::new("pv-guard", "main");
        fs::create_dir_all(format!("{}/week-notes", f.vault)).unwrap();
        fs::write(f.note("week-notes/2026-41.md"), "notes\n").unwrap();
        let p = preview(&f, "main", "week-notes/2026-41.md");
        assert_eq!(p.blocked.expect("blocked").kind, "never_publishable");

        fs::create_dir_all(format!("{}/notes", f.vault)).unwrap();
        fs::write(f.note("notes/x.md"), "x\n").unwrap();
        let p = preview(&f, "main", "notes/x.md");
        assert_eq!(p.blocked.expect("blocked").kind, "outside_blog");
    }

    #[test]
    fn preview_reads_visibility() {
        let f = Fixture::new("pv-vis", "main");
        let note = "blog/2026/fresh.md";
        fs::write(f.note(note), "---\nunlisted: true\n---\n# T\n").unwrap();
        assert_eq!(preview(&f, "main", note).visibility, "unlisted");
        fs::write(f.note(note), "---\nunlisted: true\npassword: hunter2\n---\n# T\n").unwrap();
        assert_eq!(preview(&f, "main", note).visibility, "password");
        fs::write(f.note(note), "---\npassword: \"\"\n---\n# T\n").unwrap();
        assert_eq!(preview(&f, "main", note).visibility, "public");
    }

    #[test]
    fn preview_reads_the_site_embargo_and_the_dispatch_hold() {
        let f = Fixture::new("pv-when", "main");
        let note = "blog/2026/fresh.md";

        fs::write(
            f.note(note),
            "---\ndate: 2026-01-02\npublishAt: \"2026-10-14T09:00:00-04:00\"\n---\n# T\n",
        )
        .unwrap();
        let e = preview(&f, "main", note).embargo.expect("embargo");
        assert_eq!(e.key, "publishAt");
        assert_eq!(e.utc.as_deref(), Some("2026-10-14T13:00:00Z"));
        assert!(!e.date_only);

        // A future `date` alone embargoes too; date-only is UTC midnight.
        fs::write(f.note(note), "---\ndate: 2026-11-01\n---\n# T\n").unwrap();
        let e = preview(&f, "main", note).embargo.expect("embargo");
        assert_eq!((e.key.as_str(), e.date_only), ("date", true));
        assert_eq!(e.utc.as_deref(), Some("2026-11-01T00:00:00Z"));

        // publishAt in the past wins over a future date: live now.
        fs::write(
            f.note(note),
            "---\ndate: 2026-11-01\npublishAt: 2026-01-01T00:00:00Z\n---\n# T\n",
        )
        .unwrap();
        assert_eq!(preview(&f, "main", note).embargo, None);

        // No zone: can't be placed in time, so it's flagged, not guessed.
        fs::write(f.note(note), "---\npublishAt: \"2026-10-14 09:00\"\n---\n# T\n").unwrap();
        let e = preview(&f, "main", note).embargo.expect("embargo");
        assert_eq!((e.raw.as_str(), e.utc), ("2026-10-14 09:00", None));

        fs::write(f.note(note), "---\npublish_at: 2026-10-20T13:00:00Z\n---\n# T\n").unwrap();
        let p = preview(&f, "main", note);
        assert_eq!(p.embargo, None);
        assert_eq!(p.dispatch_schedule.expect("hold").key, "publish_at");
    }

    #[test]
    fn preview_of_a_dispatch_piece() {
        let f = Fixture::new("pv-dispatch", "main");
        fs::create_dir_all(format!("{}/dispatch", f.vault)).unwrap();
        fs::write(f.note("dispatch/water.md"), "---\ntitle: Water\n---\nbody\n").unwrap();
        let p = preview(&f, "main", "dispatch/water.md");
        assert_eq!(p.blocked, None);
        assert_eq!(p.kind, "dispatch");
        assert_eq!(p.title, "Water");
        assert_eq!(p.url, "https://ejfox.com/dispatch/water");
        assert_eq!(p.rel_path, "content/dispatch/water.md");
        assert!(!p.is_update);
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
