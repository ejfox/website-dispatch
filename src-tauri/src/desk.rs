//! The Desk — EJ's daily chart-desk feed, consumed from the `desk` CLI
//! (~/.local/bin/desk → ~/code/chart-desk/desk.py).
//!
//! Every call runs `desk` as a child process with a stable PATH, on a
//! blocking-pool thread (never the UI thread), with a timeout. Long jobs
//! (`desk check`, `desk build`) run in the background and report through the
//! `desk-job` event; `desk-changed` tells the Desk view to re-read `desk today`.
//!
//! Dev/test overrides: DISPATCH_DESK_BIN (path to a stub `desk`), and
//! CHART_DESK_CONFIG passes straight through to desk (e.g. a scratch vault).

use serde::Serialize;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};
use tauri::Emitter;

const TODAY_TIMEOUT: Duration = Duration::from_secs(60);
const DISPATCH_TIMEOUT: Duration = Duration::from_secs(300);
const SHIPPED_TIMEOUT: Duration = Duration::from_secs(120);
const JOB_TIMEOUT: Duration = Duration::from_secs(45 * 60);

fn home() -> String {
    std::env::var("HOME").unwrap_or_default()
}

pub fn desk_bin() -> PathBuf {
    std::env::var("DISPATCH_DESK_BIN")
        .ok()
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(format!("{}/.local/bin/desk", home())))
}

/// GUI apps get a bare launchd PATH; desk needs python3, node (renders), cld.
fn stable_path() -> String {
    let h = home();
    [
        format!("{}/.local/bin", h),
        "/usr/local/bin".into(),
        format!("{}/.nvm/versions/node/v22.22.2/bin", h),
        "/opt/homebrew/bin".into(),
        "/usr/bin".into(),
        "/bin".into(),
        "/usr/sbin".into(),
        "/sbin".into(),
    ]
    .join(":")
}

/// Bench ids look like `2026-10-03/01-some-slug` (day dir may carry a time suffix).
pub fn validate_id(id: &str) -> Result<(), String> {
    let ok = id.len() < 200
        && id.split('/').count() == 2
        && id.chars().next().is_some_and(|c| c.is_ascii_digit())
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '/'))
        && !id.contains("..");
    if ok {
        Ok(())
    } else {
        Err(format!("That doesn't look like a bench id: {}", id))
    }
}

pub struct DeskOutput {
    pub stdout: String,
    pub stderr: String,
    pub success: bool,
}

fn human_spawn_error(e: &std::io::Error) -> String {
    if e.kind() == std::io::ErrorKind::NotFound {
        format!(
            "Couldn't find the desk command at {}.",
            desk_bin().display()
        )
    } else {
        format!("Couldn't start the desk command: {}", e)
    }
}

/// Run `desk <args>` with a timeout. Blocking — call from a blocking thread.
pub fn run_desk(args: &[&str], timeout: Duration) -> Result<DeskOutput, String> {
    let mut child = Command::new(desk_bin())
        .args(args)
        .env("PATH", stable_path())
        .env("PYTHONUNBUFFERED", "1")
        .current_dir(home())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| human_spawn_error(&e))?;

    // Drain pipes on threads so a chatty child can't deadlock on a full pipe.
    let mut out_pipe = child.stdout.take();
    let mut err_pipe = child.stderr.take();
    let out_t = std::thread::spawn(move || {
        let mut s = String::new();
        if let Some(p) = out_pipe.as_mut() {
            let _ = p.read_to_string(&mut s);
        }
        s
    });
    let err_t = std::thread::spawn(move || {
        let mut s = String::new();
        if let Some(p) = err_pipe.as_mut() {
            let _ = p.read_to_string(&mut s);
        }
        s
    });

    let start = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if start.elapsed() > timeout => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!(
                    "desk {} took longer than {} minutes and was stopped.",
                    args.first().unwrap_or(&""),
                    timeout.as_secs() / 60
                ));
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(100)),
            Err(e) => return Err(format!("Lost track of the desk command: {}", e)),
        }
    };
    Ok(DeskOutput {
        stdout: out_t.join().unwrap_or_default(),
        stderr: err_t.join().unwrap_or_default(),
        success: status.success(),
    })
}

fn last_line(s: &str) -> String {
    s.lines()
        .rev()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .unwrap_or("")
        .chars()
        .take(300)
        .collect()
}

fn failure_sentence(what: &str, out: &DeskOutput) -> String {
    let detail = last_line(&out.stderr);
    let detail = if detail.is_empty() {
        last_line(&out.stdout)
    } else {
        detail
    };
    if detail.is_empty() {
        format!("{} didn't work.", what)
    } else {
        format!("{} didn't work: {}", what, detail)
    }
}

/// Parse the first JSON object in stdout (desk may print log lines first).
fn parse_json(stdout: &str) -> Option<Value> {
    if let Ok(v) = serde_json::from_str::<Value>(stdout.trim()) {
        return Some(v);
    }
    let start = stdout.find('{')?;
    serde_json::from_str(&stdout[start..]).ok()
}

async fn blocking<T: Send + 'static>(
    f: impl FnOnce() -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(f)
        .await
        .map_err(|e| format!("Background task failed: {}", e))?
}

// ---------------------------------------------------------------------------
// Tauri commands
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn desk_today() -> Result<Value, String> {
    blocking(|| {
        let out = run_desk(&["today"], TODAY_TIMEOUT)?;
        if !out.success {
            return Err(failure_sentence("Reading today's desk", &out));
        }
        parse_json(&out.stdout)
            .ok_or_else(|| "The desk answered, but not with something Dispatch can read.".into())
    })
    .await
}

/// `desk dispatch <id> --json` → the draft piece in the vault's dispatch/.
/// If `title` is given (an angle EJ picked) and the note is new, it becomes
/// the piece's title (minimal frontmatter edit).
#[tauri::command]
pub async fn desk_start_piece(
    app: tauri::AppHandle,
    id: String,
    title: Option<String>,
) -> Result<Value, String> {
    validate_id(&id)?;
    let result = blocking(move || {
        let out = run_desk(&["dispatch", &id, "--json"], DISPATCH_TIMEOUT)?;
        if !out.success {
            return Err(failure_sentence("Starting the piece", &out));
        }
        let mut v = parse_json(&out.stdout)
            .ok_or("The desk made the piece but didn't say where it put it.")?;
        let note = v["note"].as_str().unwrap_or_default().to_string();
        if note.is_empty() || !Path::new(&note).exists() {
            return Err("The desk didn't create a note file.".into());
        }
        let existed = v["detail"].as_str().unwrap_or("").starts_with("exists");
        let mut title_set = false;
        if let Some(t) = title
            .map(|t| t.trim().to_string())
            .filter(|t| !t.is_empty())
        {
            if !existed {
                let content = std::fs::read_to_string(&note).map_err(|e| e.to_string())?;
                let updated = crate::frontmatter_edit::set_scalar(&content, "title", &t)?;
                if updated != content {
                    std::fs::write(&note, updated).map_err(|e| e.to_string())?;
                }
                title_set = true;
            }
        }
        v["title_set"] = json!(title_set);
        v["existed"] = json!(existed);
        Ok(v)
    })
    .await?;
    let _ = app.emit("desk-changed", ());
    Ok(result)
}

#[derive(Debug, Clone, Serialize)]
pub struct DeskJob {
    pub kind: String,
    pub id: String,
    pub started_at: u64,
}

static JOBS: LazyLock<Mutex<HashMap<String, DeskJob>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

fn job_key(kind: &str, id: &str) -> String {
    format!("{}:{}", kind, id)
}

/// Start `desk check <id>` or `desk build <id>` in the background. Returns
/// immediately; completion arrives as a `desk-job` event.
#[tauri::command]
pub fn desk_start_job(app: tauri::AppHandle, kind: String, id: String) -> Result<DeskJob, String> {
    validate_id(&id)?;
    if kind != "check" && kind != "build" {
        return Err(format!("Unknown desk job: {}", kind));
    }
    let key = job_key(&kind, &id);
    let job = DeskJob {
        kind: kind.clone(),
        id: id.clone(),
        started_at: chrono::Utc::now().timestamp() as u64,
    };
    {
        let mut jobs = JOBS.lock().map_err(|e| e.to_string())?;
        if jobs.contains_key(&key) {
            return Err(format!("Already running {} for this story.", kind));
        }
        jobs.insert(key.clone(), job.clone());
    }
    let _ = app.emit(
        "desk-job",
        json!({ "kind": kind, "id": id, "state": "running" }),
    );
    std::thread::spawn(move || {
        let what = if kind == "check" {
            "The fact-check"
        } else {
            "The build"
        };
        let result = run_desk(&[&kind, &id], JOB_TIMEOUT);
        if let Ok(mut jobs) = JOBS.lock() {
            jobs.remove(&key);
        }
        let payload = match result {
            Ok(out) if out.success => json!({
                "kind": kind, "id": id, "state": "done",
                "summary": last_line(&out.stdout),
            }),
            Ok(out) => json!({
                "kind": kind, "id": id, "state": "failed",
                "error": failure_sentence(what, &out),
            }),
            Err(e) => json!({ "kind": kind, "id": id, "state": "failed", "error": e }),
        };
        let _ = app.emit("desk-job", payload);
        let _ = app.emit("desk-changed", ());
    });
    Ok(job)
}

#[tauri::command]
pub fn desk_jobs() -> Vec<DeskJob> {
    JOBS.lock()
        .map(|j| j.values().cloned().collect())
        .unwrap_or_default()
}

/// `desk shipped <id> <url> [url…]` — logs the ship, bumps the streak.
#[tauri::command]
pub async fn desk_shipped(
    app: tauri::AppHandle,
    id: String,
    urls: Vec<String>,
) -> Result<String, String> {
    validate_id(&id)?;
    let urls: Vec<String> = urls
        .into_iter()
        .map(|u| u.trim().to_string())
        .filter(|u| u.starts_with("https://") || u.starts_with("http://"))
        .collect();
    if urls.is_empty() {
        return Err("Need at least the published URL to log a ship.".into());
    }
    let msg = blocking(move || {
        let mut args: Vec<&str> = vec!["shipped", &id];
        args.extend(urls.iter().map(String::as_str));
        let out = run_desk(&args, SHIPPED_TIMEOUT)?;
        if !out.success {
            return Err(failure_sentence("Logging the ship", &out));
        }
        Ok(last_line(&out.stdout))
    })
    .await?;
    let _ = app.emit("desk-changed", ());
    Ok(msg)
}

/// Open a chart-desk file or folder (sketch, review page, bench folder) with
/// its default app. Limited to the chart-desk tree.
#[tauri::command]
pub fn desk_open(path: String) -> Result<(), String> {
    let root = format!("{}/code/chart-desk/", home());
    let p = Path::new(&path);
    let canonical = p
        .canonicalize()
        .map_err(|_| "That file isn't there anymore.".to_string())?;
    if !canonical.to_string_lossy().starts_with(&root) {
        return Err("Dispatch only opens files from the chart desk here.".into());
    }
    Command::new("open")
        .arg(&canonical)
        .spawn()
        .map_err(|e| format!("Couldn't open it: {}", e))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bench_ids() {
        assert!(validate_id("2026-10-03/01-palantir-self-made-federal-dollars").is_ok());
        assert!(validate_id("2026-10-02-0630/01-slug").is_ok());
        assert!(validate_id("--help/x").is_err());
        assert!(validate_id("2026-10-03/../../etc").is_err());
        assert!(validate_id("2026-10-03").is_err());
        assert!(validate_id("2026-10-03/a b").is_err());
    }

    #[test]
    fn parses_json_after_log_noise() {
        let v = parse_json("uploading…\n{\"note\": \"/v/dispatch/x.md\", \"detail\": \"ok\"}\n")
            .unwrap();
        assert_eq!(v["note"], "/v/dispatch/x.md");
    }

    #[test]
    fn failure_sentences_are_human() {
        let out = DeskOutput {
            stdout: String::new(),
            stderr: "Traceback…\nValueError: no such story\n".into(),
            success: false,
        };
        assert_eq!(
            failure_sentence("Starting the piece", &out),
            "Starting the piece didn't work: ValueError: no such story"
        );
    }
}
