//! Deploy confirmation: a `git push` is not a deploy.
//!
//! website2 deploys through GitHub Actions (~3 min) and rolls itself back if
//! the new build doesn't come up healthy. Its `/api/healthcheck` reports the
//! commit actually being served (`"commit": "d57aecc4"`, an 8-char short SHA,
//! plus `"manifest": "ok"`). We poll that until it reports the pushed commit,
//! then GET the post's URL and require a 200 before calling the post live.
//!
//! The parsing and matching logic is pure (no I/O) so it can be unit-tested
//! against recorded JSON; the async command below is the only part that
//! touches the network or git.

use crate::config;
use serde::{Deserialize, Serialize};
use std::process::Command;
use std::time::{Duration, Instant};
use tauri::Emitter;

const POLL_EVERY: Duration = Duration::from_secs(10);
const DEFAULT_TIMEOUT_SECS: u64 = 360;
/// Check the Actions run only every Nth poll, and only after this long —
/// unauthenticated GitHub API is limited to 60 req/hour.
const RUN_CHECK_EVERY_N_POLLS: u32 = 6;
const RUN_CHECK_AFTER: Duration = Duration::from_secs(120);
pub const ACTIONS_URL: &str = "https://github.com/ejfox/website2/actions";
const RUNS_API: &str =
    "https://api.github.com/repos/ejfox/website2/actions/workflows/deploy.yml/runs";

// ── Pure logic ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct Health {
    #[serde(default)]
    pub commit: Option<String>,
    #[serde(default)]
    pub manifest: Option<String>,
}

impl Health {
    /// A manifest that isn't "ok" means the build is up but not serving
    /// content — website2's own deploy treats that as not-yet-healthy.
    pub fn manifest_ok(&self) -> bool {
        self.manifest.as_deref().map_or(true, |m| m == "ok")
    }
}

pub fn parse_healthcheck(body: &str) -> Result<Health, String> {
    serde_json::from_str::<Health>(body).map_err(|e| format!("Bad healthcheck JSON: {}", e))
}

/// Does the (possibly short) SHA the site reports name our full pushed SHA?
/// Requires ≥7 hex chars so an empty or garbage `commit` never matches.
pub fn sha_matches(full: &str, reported: &str) -> bool {
    let full = full.trim().to_ascii_lowercase();
    let reported = reported.trim().to_ascii_lowercase();
    if reported.len() < 7 || full.len() < 7 || !reported.chars().all(|c| c.is_ascii_hexdigit()) {
        return false;
    }
    full.starts_with(&reported) || reported.starts_with(&full)
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Observation {
    /// Still serving the commit that was live when we started (or nothing).
    Waiting,
    /// Serving our commit, with a healthy manifest.
    Deployed,
    /// Serving some *other* new commit — could be a later push that includes
    /// ours (the deploy queue drops older pending runs) or a rollback target.
    /// Caller resolves with a git ancestry check.
    Superseded,
}

pub fn classify(baseline: Option<&str>, health: &Health, ours: &str) -> Observation {
    let Some(reported) = health.commit.as_deref().filter(|c| !c.trim().is_empty()) else {
        return Observation::Waiting;
    };
    if sha_matches(ours, reported) {
        return if health.manifest_ok() {
            Observation::Deployed
        } else {
            Observation::Waiting
        };
    }
    match baseline {
        Some(b) if b.trim().eq_ignore_ascii_case(reported.trim()) => Observation::Waiting,
        // No baseline (first poll failed) — can't tell new from old, keep waiting.
        None => Observation::Waiting,
        Some(_) if health.manifest_ok() => Observation::Superseded,
        Some(_) => Observation::Waiting,
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct RunInfo {
    pub status: String,
    pub conclusion: Option<String>,
    pub html_url: String,
}

impl RunInfo {
    /// A completed run that failed means the deploy errored or rolled back.
    /// `cancelled` is NOT failure: the queue cancels older pending runs when a
    /// newer push arrives, and that newer deploy carries our commit.
    pub fn failed(&self) -> bool {
        self.status == "completed"
            && matches!(
                self.conclusion.as_deref(),
                Some("failure") | Some("timed_out") | Some("startup_failure")
            )
    }
}

/// Pick the deploy-workflow run for `sha` out of a GitHub
/// `/actions/workflows/<file>/runs` response.
pub fn parse_runs(body: &str, sha: &str) -> Option<RunInfo> {
    let v: serde_json::Value = serde_json::from_str(body).ok()?;
    let runs = v.get("workflow_runs")?.as_array()?;
    runs.iter().find_map(|r| {
        let head = r.get("head_sha")?.as_str()?;
        if !sha_matches(head, sha) {
            return None;
        }
        Some(RunInfo {
            status: r.get("status")?.as_str()?.to_string(),
            conclusion: r
                .get("conclusion")
                .and_then(|c| c.as_str())
                .map(String::from),
            html_url: r.get("html_url")?.as_str()?.to_string(),
        })
    })
}

// ── I/O ─────────────────────────────────────────────────────────────────────

fn git(repo: &str, args: &[&str]) -> Result<std::process::Output, String> {
    Command::new(crate::bin_paths::git())
        .args(args)
        .current_dir(repo)
        .output()
        .map_err(|e| format!("git {}: {}", args.join(" "), e))
}

pub fn head_sha(repo_path: &str) -> Result<String, String> {
    let out = git(repo_path, &["rev-parse", "HEAD"])?;
    if !out.status.success() {
        return Err(format!(
            "git rev-parse HEAD failed: {}",
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// Is `ours` contained in `deployed`? Fetches once if `deployed` is unknown
/// locally (pushed from elsewhere).
fn is_ancestor(repo: &str, branch: &str, ours: &str, deployed: &str) -> bool {
    let check = |r: &str| {
        git(r, &["merge-base", "--is-ancestor", ours, deployed])
            .map(|o| o.status.code())
            .ok()
            .flatten()
    };
    match check(repo) {
        Some(0) => true,
        Some(1) => false,
        _ => {
            let _ = git(repo, &["fetch", "--quiet", "origin", branch]);
            check(repo) == Some(0)
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct DeployProgress {
    pub sha: String,
    pub elapsed_secs: u64,
    pub deployed_commit: Option<String>,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct DeployResult {
    pub sha: String,
    /// "live" | "deployed_unverified" | "timeout" | "failed"
    pub phase: String,
    pub ok: bool,
    pub message: String,
    pub deployed_commit: Option<String>,
    pub url_status: Option<u16>,
    /// The specific Actions run when we could find it, else the Actions page.
    pub run_url: String,
}

async fn fetch_health(client: &reqwest::Client, domain: &str) -> Result<Health, String> {
    let url = format!(
        "{}/api/healthcheck?t={}",
        domain.trim_end_matches('/'),
        chrono::Utc::now().timestamp()
    );
    let resp = client.get(&url).send().await.map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(format!("healthcheck HTTP {}", resp.status()));
    }
    let body = resp.text().await.map_err(|e| e.to_string())?;
    parse_healthcheck(&body)
}

async fn fetch_run(client: &reqwest::Client, sha: &str) -> Option<RunInfo> {
    let url = format!("{}?head_sha={}&per_page=5", RUNS_API, sha);
    let body = client
        .get(&url)
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .ok()?
        .text()
        .await
        .ok()?;
    parse_runs(&body, sha)
}

/// GET the post URL, retrying briefly (the new process may still be warming).
async fn check_url(client: &reqwest::Client, url: &str) -> Option<u16> {
    let mut last = None;
    for attempt in 0..3 {
        if attempt > 0 {
            tokio::time::sleep(Duration::from_secs(5)).await;
        }
        if let Ok(r) = client.get(url).send().await {
            let code = r.status().as_u16();
            last = Some(code);
            if code == 200 {
                break;
            }
        }
    }
    last
}

#[tauri::command]
pub async fn get_head_sha(target_id: Option<String>) -> Result<String, String> {
    let target = config::resolve_target(target_id.as_deref())?;
    tauri::async_runtime::spawn_blocking(move || head_sha(&target.repo_path))
        .await
        .map_err(|e| e.to_string())?
}

/// Poll the target's healthcheck until it serves `sha` (or a later commit that
/// contains it), then confirm `post_url` returns 200. Emits `deploy-progress`
/// events (filter on `sha`). Never returns Err for a deploy problem — the
/// outcome is in `DeployResult.phase` so the UI can show a proper message.
#[tauri::command]
pub async fn wait_for_deploy(
    app: tauri::AppHandle,
    sha: String,
    post_url: Option<String>,
    timeout_secs: Option<u64>,
    target_id: Option<String>,
    title: Option<String>,
    skip_url_check: Option<bool>,
) -> Result<DeployResult, String> {
    let target = config::resolve_target(target_id.as_deref())?;
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .user_agent("website-dispatch")
        .build()
        .map_err(|e| e.to_string())?;
    let timeout = Duration::from_secs(timeout_secs.unwrap_or(DEFAULT_TIMEOUT_SECS));
    let start = Instant::now();
    let short = sha.chars().take(7).collect::<String>();

    let mut baseline: Option<String> = None;
    let mut last_reported: Option<String> = None;
    let mut polls: u32 = 0;
    let mut deployed: Option<String> = None;
    let mut run_url = ACTIONS_URL.to_string();

    let progress = |reported: &Option<String>, message: String| {
        let _ = app.emit(
            "deploy-progress",
            DeployProgress {
                sha: sha.clone(),
                elapsed_secs: start.elapsed().as_secs(),
                deployed_commit: reported.clone(),
                message,
            },
        );
    };

    while start.elapsed() < timeout {
        if polls > 0 {
            tokio::time::sleep(POLL_EVERY).await;
        }
        polls += 1;

        match fetch_health(&client, &target.domain).await {
            Ok(h) => {
                last_reported = h.commit.clone();
                if polls == 1 || baseline.is_none() {
                    // Already serving ours (e.g. "Everything up-to-date")?
                    // classify handles that; otherwise remember what was live.
                    if !h.commit.as_deref().is_some_and(|c| sha_matches(&sha, c)) {
                        baseline = h.commit.clone();
                    }
                }
                match classify(baseline.as_deref(), &h, &sha) {
                    Observation::Deployed => {
                        deployed = h.commit.clone();
                        break;
                    }
                    Observation::Superseded => {
                        let reported = h.commit.clone().unwrap_or_default();
                        let (repo, branch, ours, rep) = (
                            target.repo_path.clone(),
                            target.branch.clone(),
                            sha.clone(),
                            reported.clone(),
                        );
                        let contains = tauri::async_runtime::spawn_blocking(move || {
                            is_ancestor(&repo, &branch, &ours, &rep)
                        })
                        .await
                        .unwrap_or(false);
                        if contains {
                            deployed = Some(reported);
                            break;
                        }
                        // A different commit that doesn't contain ours: a
                        // rollback or an unrelated deploy. Keep waiting — ours
                        // may still be queued — and let the run check decide.
                        progress(
                            &last_reported,
                            format!("site is serving {} (not ours yet)", reported),
                        );
                    }
                    Observation::Waiting => progress(
                        &last_reported,
                        format!(
                            "deploying {} — site still on {}",
                            short,
                            h.commit.as_deref().unwrap_or("?")
                        ),
                    ),
                }
            }
            Err(e) => progress(&last_reported, format!("healthcheck unreachable: {}", e)),
        }

        // Fail fast on a failed/rolled-back Actions run.
        if start.elapsed() >= RUN_CHECK_AFTER && polls % RUN_CHECK_EVERY_N_POLLS == 0 {
            if let Some(run) = fetch_run(&client, &sha).await {
                run_url = run.html_url.clone();
                if run.failed() {
                    return Ok(finish(
                        &app,
                        title.as_deref(),
                        DeployResult {
                            sha: sha.clone(),
                            phase: "failed".into(),
                            ok: false,
                            message: format!(
                                "Deploy run {} — the site was not updated (or rolled back)",
                                run.conclusion.as_deref().unwrap_or("failed")
                            ),
                            deployed_commit: last_reported,
                            url_status: None,
                            run_url,
                        },
                    ));
                }
            }
        }
    }

    let Some(deployed) = deployed else {
        if let Some(run) = fetch_run(&client, &sha).await {
            run_url = run.html_url;
        }
        return Ok(finish(
            &app,
            title.as_deref(),
            DeployResult {
                sha: sha.clone(),
                phase: "timeout".into(),
                ok: false,
                message: format!(
                    "Pushed {} but the site still reports {} after {}s",
                    short,
                    last_reported.as_deref().unwrap_or("nothing"),
                    start.elapsed().as_secs()
                ),
                deployed_commit: last_reported,
                url_status: None,
                run_url,
            },
        ));
    };

    // Password posts 404 at their own URL on website2 (isProtectedContent),
    // so a URL check would cry wolf — the matching SHA is the confirmation.
    let post_url = post_url.filter(|_| !skip_url_check.unwrap_or(false));
    let url_status = match &post_url {
        Some(u) => {
            progress(
                &Some(deployed.clone()),
                "deployed — checking the post URL".into(),
            );
            check_url(&client, u).await
        }
        None => None,
    };
    // A non-200 after a confirmed deploy is a soft failure: the commit is
    // live, but the page isn't reachable (a future `date`/`publishAt` 404s
    // by design on website2, so say so rather than cry "failed").
    let (phase, ok, message) = match (url_status, &post_url) {
        (Some(200), _) | (_, None) => ("live", true, "Live".to_string()),
        (Some(code), Some(_)) => (
            "deployed_unverified",
            false,
            format!(
                "Deployed {} but the post URL returned HTTP {} (scheduled post? future date?)",
                deployed, code
            ),
        ),
        (None, Some(_)) => (
            "deployed_unverified",
            false,
            format!("Deployed {} but the post URL didn't respond", deployed),
        ),
    };
    Ok(finish(
        &app,
        title.as_deref(),
        DeployResult {
            sha,
            phase: phase.into(),
            ok,
            message,
            deployed_commit: Some(deployed),
            url_status,
            run_url,
        },
    ))
}

/// Native macOS notification for the final outcome (moved here from the
/// publish command, which used to announce "live" at push time).
fn finish(app: &tauri::AppHandle, title: Option<&str>, result: DeployResult) -> DeployResult {
    use tauri_plugin_notification::NotificationExt;
    let name = title
        .map(|t| t.trim().trim_start_matches('#').trim().to_string())
        .filter(|t| !t.is_empty())
        .unwrap_or_else(|| "Your post".into());
    let (head, body) = if result.ok {
        ("Live", format!("\u{2018}{}\u{2019} is live", name))
    } else {
        (
            "Deploy problem",
            format!("\u{2018}{}\u{2019}: {}", name, result.message),
        )
    };
    let _ = app.notification().builder().title(head).body(body).show();
    std::thread::spawn(crate::sketchybar_cache::update);
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    const OURS: &str = "d57aecc4ff85880009177738ccd95fc05d3ffa78";

    // Recorded from https://ejfox.com/api/healthcheck on 2026-10-11.
    const LIVE: &str = r#"{
      "status": "healthy", "timestamp": "2026-10-11T20:07:43.041Z", "uptime": 152,
      "version": "2.0.0", "node": "v20.19.6", "commit": "d57aecc4",
      "env": "production", "memory": { "used": 38, "total": 42 }, "manifest": "ok"
    }"#;

    fn h(commit: Option<&str>, manifest: Option<&str>) -> Health {
        Health {
            commit: commit.map(String::from),
            manifest: manifest.map(String::from),
        }
    }

    #[test]
    fn parses_recorded_healthcheck() {
        let got = parse_healthcheck(LIVE).unwrap();
        assert_eq!(got.commit.as_deref(), Some("d57aecc4"));
        assert!(got.manifest_ok());
    }

    #[test]
    fn parses_missing_fields_and_rejects_garbage() {
        let got = parse_healthcheck(r#"{"status":"healthy"}"#).unwrap();
        assert_eq!(got, h(None, None));
        assert!(parse_healthcheck("<html>502 Bad Gateway</html>").is_err());
    }

    #[test]
    fn sha_matching() {
        assert!(sha_matches(OURS, "d57aecc4"));
        assert!(sha_matches(OURS, "D57AECC4"));
        assert!(sha_matches(OURS, OURS));
        assert!(sha_matches("d57aecc4", OURS));
        assert!(!sha_matches(OURS, "a30dc6fb"));
        assert!(!sha_matches(OURS, ""));
        assert!(!sha_matches(OURS, "d57a")); // too short to trust
        assert!(!sha_matches(OURS, "unknown"));
    }

    #[test]
    fn classify_deployed_waiting_superseded() {
        let base = Some("a30dc6fb");
        assert_eq!(
            classify(base, &parse_healthcheck(LIVE).unwrap(), OURS),
            Observation::Deployed
        );
        assert_eq!(
            classify(base, &h(Some("a30dc6fb"), Some("ok")), OURS),
            Observation::Waiting
        );
        assert_eq!(
            classify(base, &h(Some("7499b155"), Some("ok")), OURS),
            Observation::Superseded
        );
        assert_eq!(classify(base, &h(None, None), OURS), Observation::Waiting);
        // No baseline → can't tell a new commit from the old one.
        assert_eq!(
            classify(None, &h(Some("7499b155"), Some("ok")), OURS),
            Observation::Waiting
        );
    }

    #[test]
    fn our_commit_with_bad_manifest_is_not_live() {
        assert_eq!(
            classify(
                Some("a30dc6fb"),
                &h(Some("d57aecc4"), Some("missing")),
                OURS
            ),
            Observation::Waiting
        );
    }

    // Trimmed from the GitHub workflow-runs API on 2026-10-11.
    const RUNS: &str = r#"{"total_count": 2, "workflow_runs": [
      {"head_sha": "d57aecc4ff85880009177738ccd95fc05d3ffa78", "status": "completed",
       "conclusion": "success", "name": "Deploy to VPS",
       "html_url": "https://github.com/ejfox/website2/actions/runs/38170117755"},
      {"head_sha": "a30dc6fbdf3ac138ab42cbe60e0302cb5cbf2839", "status": "completed",
       "conclusion": "cancelled", "name": "Deploy to VPS",
       "html_url": "https://github.com/ejfox/website2/actions/runs/38170114087"},
      {"head_sha": "1111111aaaaaaa", "status": "completed", "conclusion": "failure",
       "html_url": "https://github.com/ejfox/website2/actions/runs/1"},
      {"head_sha": "2222222bbbbbbb", "status": "in_progress", "conclusion": null,
       "html_url": "https://github.com/ejfox/website2/actions/runs/2"}
    ]}"#;

    #[test]
    fn parses_runs_and_classifies_failure() {
        let ok = parse_runs(RUNS, OURS).unwrap();
        assert!(ok.html_url.ends_with("/38170117755"));
        assert!(!ok.failed());

        // Cancelled = superseded by a newer queued deploy, not a failure.
        let cancelled = parse_runs(RUNS, "a30dc6fbdf3ac138ab42cbe60e0302cb5cbf2839").unwrap();
        assert!(!cancelled.failed());

        assert!(parse_runs(RUNS, "1111111aaaaaaa").unwrap().failed());
        let running = parse_runs(RUNS, "2222222bbbbbbb").unwrap();
        assert_eq!(running.conclusion, None);
        assert!(!running.failed());

        assert!(parse_runs(RUNS, "ffffffffffffff").is_none());
        assert!(parse_runs(r#"{"message":"API rate limit exceeded"}"#, OURS).is_none());
    }
}
