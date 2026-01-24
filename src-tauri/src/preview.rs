use std::process::{Child, Command};
use std::sync::{Mutex, OnceLock};
use std::thread;
use std::time::Duration;

const PORT: u16 = 6419;
const PREVIEW_SERVER_PATH: &str = "/Users/ejfox/code/website-dispatch/preview-server.mjs";

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

    // Start the Node.js preview server
    match Command::new("node")
        .arg(PREVIEW_SERVER_PATH)
        .spawn()
    {
        Ok(child) => {
            let mut server = get_node_server().lock().unwrap();
            *server = Some(child);
            println!("Node.js preview server started on http://localhost:{}", PORT);
        }
        Err(e) => {
            eprintln!("Failed to start Node.js preview server: {}", e);
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

    match client.post(&url)
        .header("Content-Type", "application/json")
        .body(body.to_string())
        .timeout(Duration::from_secs(5))
        .send()
    {
        Ok(_) => {}
        Err(e) => {
            eprintln!("Failed to set preview file: {}", e);
        }
    }
}

pub fn open_preview() -> Result<String, String> {
    let url = format!("http://localhost:{}", PORT);
    Command::new("open")
        .arg(&url)
        .spawn()
        .map_err(|e| e.to_string())?;
    Ok(url)
}

// Stop the server when the app closes
pub fn stop_server() {
    let mut server = get_node_server().lock().unwrap();
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
