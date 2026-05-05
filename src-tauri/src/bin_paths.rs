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
    // Spawn the user's login shell so PATH is fully populated.
    // SHELL env var falls back to zsh (macOS default) if not set.
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".to_string());
    if let Ok(output) = Command::new(&shell)
        .args(["-l", "-c", &format!("command -v {}", name)])
        .output()
    {
        if output.status.success() {
            let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !path.is_empty() && std::path::Path::new(&path).exists() {
                log::info!("Resolved `{}` -> {}", name, path);
                return PathBuf::from(path);
            }
        }
    }
    log::warn!(
        "Could not resolve `{}` via login shell; falling back to {}",
        name,
        fallback
    );
    PathBuf::from(fallback)
}
