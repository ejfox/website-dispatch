//! Resolve absolute paths to external binaries (node, git) at startup.
//!
//! macOS GUI apps inherit a minimal launchctl PATH that excludes user shells'
//! additions — homebrew (`/opt/homebrew/bin`), nvm (`~/.nvm/...`), asdf
//! shims, etc. So a bare `Command::new("node")` from a packaged `.app` can
//! fail "command not found" even when `node` is happily on the user's
//! `$PATH` in their terminal. We resolve via a login shell once and cache.

use std::path::PathBuf;
use std::process::Command;
use std::sync::OnceLock;

static NODE: OnceLock<PathBuf> = OnceLock::new();
static GIT: OnceLock<PathBuf> = OnceLock::new();

pub fn node() -> &'static PathBuf {
    NODE.get_or_init(|| resolve("node", "/usr/local/bin/node"))
}

pub fn git() -> &'static PathBuf {
    GIT.get_or_init(|| resolve("git", "/usr/bin/git"))
}

/// Eagerly resolve all known binaries. Call once at startup so the cost
/// (spawning a login shell) lands during init rather than on first use.
pub fn warm() {
    let _ = node();
    let _ = git();
}

fn resolve(name: &str, fallback: &str) -> PathBuf {
    // Strategy: fast filesystem probes FIRST. Spawning a login shell at app
    // startup is expensive (~700ms on a typical zsh config) and dangerous —
    // any HTTP-using prompt/MOTD in the user's rc files can hang the shell
    // when their network is flaky, which then blocks Dispatch's main thread
    // forever. So we only fall back to the shell when the obvious paths fail.
    let home = std::env::var("HOME").unwrap_or_default();

    // 1. Common install locations on macOS (Apple Silicon homebrew first).
    let known_paths: Vec<String> = vec![
        format!("/opt/homebrew/bin/{}", name),
        format!("/usr/local/bin/{}", name),
        format!("{}/.nvm/versions/node/current/bin/{}", home, name),
    ];
    for candidate in &known_paths {
        if std::path::Path::new(candidate).exists() {
            log::info!("Resolved `{}` via known path -> {}", name, candidate);
            return PathBuf::from(candidate);
        }
    }

    // 2. If nvm is installed but no `current` symlink, pick the newest version.
    if name == "node" {
        let nvm_dir = PathBuf::from(&home).join(".nvm/versions/node");
        if let Ok(entries) = std::fs::read_dir(&nvm_dir) {
            let mut versions: Vec<PathBuf> = entries
                .filter_map(|e| e.ok())
                .map(|e| e.path())
                .filter(|p| p.is_dir())
                .collect();
            versions.sort();
            if let Some(latest) = versions.last() {
                let candidate = latest.join("bin").join(name);
                if candidate.exists() {
                    log::info!("Resolved `{}` via nvm latest -> {}", name, candidate.display());
                    return candidate;
                }
            }
        }
    }

    // 3. Last resort: spawn the user's login shell. This is the slow/risky
    //    path — only reached if no known binary path worked.
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".to_string());
    if let Ok(output) = Command::new(&shell)
        .args(["-l", "-c", &format!("command -v {}", name)])
        .output()
    {
        if output.status.success() {
            let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !path.is_empty() && std::path::Path::new(&path).exists() {
                log::info!("Resolved `{}` via login shell -> {}", name, path);
                return PathBuf::from(path);
            }
        }
    }

    log::warn!(
        "Could not resolve `{}`; falling back to {}",
        name,
        fallback
    );
    PathBuf::from(fallback)
}
