use std::process::{Child, Command};
use std::sync::{Mutex, OnceLock};
use std::thread;
use std::time::Duration;

const PORT: u16 = 6419;

fn get_preview_server_path() -> String {
    // Prefer the bundled copy when running from a packaged .app — that way
    // a portable Dispatch.app stays self-contained instead of silently using
    // the developer's source tree if it happens to be on the same machine.
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let resources_path = dir.join("../Resources/preview-server.mjs");
            if resources_path.exists() {
                return resources_path.to_string_lossy().to_string();
            }
            let prod_path = dir.join("preview-server.mjs");
            if prod_path.exists() {
                return prod_path.to_string_lossy().to_string();
            }
        }
    }
    // Dev fallback: project root via CARGO_MANIFEST_DIR.
    concat!(env!("CARGO_MANIFEST_DIR"), "/../preview-server.mjs").to_string()
}

// Global state for the Node.js server process
static NODE_SERVER: OnceLock<Mutex<Option<Child>>> = OnceLock::new();
static SERVER_STARTED: OnceLock<bool> = OnceLock::new();

fn get_node_server() -> &'static Mutex<Option<Child>> {
    NODE_SERVER.get_or_init(|| Mutex::new(None))
}

pub fn init_server() {
    if SERVER_STARTED.get().is_some() {
        return; // Already started
    }
    SERVER_STARTED.set(true).ok();

    // Kill any existing server on the port
    let _ = Command::new("lsof")
        .args(["-ti", &format!(":{}", PORT)])
        .output()
        .map(|output| {
            if !output.stdout.is_empty() {
                let pids = String::from_utf8_lossy(&output.stdout);
                for pid in pids.trim().lines() {
                    let _ = Command::new("kill").args(["-9", pid]).output();
                }
            }
        });

    thread::sleep(Duration::from_millis(500));

    // Start the Node.js preview server. Pass WEBSITE2_PATH from the default
    // publish target's repo_path so the bundled server doesn't have to
    // hardcode /Users/ejfox/code/website2 — this is what makes Dispatch
    // portable to a different machine or vault layout.
    let server_path = get_preview_server_path();
    let website2_path = crate::config::default_target()
        .ok()
        .map(|t| t.repo_path)
        .unwrap_or_default();
    let mut cmd = Command::new(crate::bin_paths::node());
    cmd.arg(&server_path);
    if !website2_path.is_empty() {
        cmd.env("WEBSITE2_PATH", &website2_path);
    }
    match cmd.spawn() {
        Ok(child) => {
            let mut server = get_node_server().lock().unwrap_or_else(|e| e.into_inner());
            *server = Some(child);
            log::info!(
                "Node.js preview server started on http://localhost:{}",
                PORT
            );
        }
        Err(e) => {
            log::warn!("Failed to start Node.js preview server: {}", e);
        }
    }

    // Give the server time to start
    thread::sleep(Duration::from_millis(1000));
}

pub fn set_file(path: &str) {
    // Send file path to Node server via HTTP
    let client = reqwest::blocking::Client::new();
    let url = format!("http://127.0.0.1:{}/set-file", PORT);

    let body = serde_json::json!({
        "path": path
    });

    // Swallow errors silently. Any stderr write here would panic in a .app
    // bundle when stderr is connected to an exited parent process (e.g. user
    // launched from terminal, then closed terminal). The frontend handles
    // missing-preview UI on its own.
    let _ = client
        .post(&url)
        .header("Content-Type", "application/json")
        .body(body.to_string())
        .timeout(Duration::from_secs(5))
        .send();
}

// Stop the server when the app closes
pub fn stop_server() {
    let mut server = get_node_server().lock().unwrap_or_else(|e| e.into_inner());
    if let Some(mut child) = server.take() {
        let _ = child.kill();
    }

    // Also kill by port in case something else started
    let _ = Command::new("lsof")
        .args(["-ti", &format!(":{}", PORT)])
        .output()
        .map(|output| {
            if !output.stdout.is_empty() {
                let pids = String::from_utf8_lossy(&output.stdout);
                for pid in pids.trim().lines() {
                    let _ = Command::new("kill").args(["-9", pid]).output();
                }
            }
        });
}
