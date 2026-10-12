//! The Settings window: a separate native window (label `settings`), plus the
//! commands its panes use to read, save and check the config.
//!
//! Opening it — one entry point, three ways in:
//!   * Rust:  `settings::open_window(&app, None)`
//!   * Event: emit `menu:settings` (payload: optional pane id string) from Rust
//!            or JS. `register()` listens for it app-wide. The App menu's
//!            "Settings… ⌘," should emit this.
//!   * JS:    `invoke('open_settings_window', { pane })` (src/utils/openSettings.ts)
//!
//! Pane ids: general · publishing · media · connections · advanced.
//! If the window is already open it is focused and switched to the pane via
//! the `settings:show-pane` event.
//!
//! Secrets never reach the webview in full: `get_settings` returns
//! `config::masked`, `save_settings` goes through `config::update_from_ui`.
//! Every save emits `config-changed` so the main window reloads its config.

use serde::Serialize;
use std::path::Path;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Listener, Manager, WebviewUrl, WebviewWindowBuilder};

use crate::config;

pub const WINDOW_LABEL: &str = "settings";
/// Event that opens Settings (payload: optional pane id).
pub const OPEN_EVENT: &str = "menu:settings";
/// Event sent to an already-open Settings window to switch panes.
pub const SHOW_PANE_EVENT: &str = "settings:show-pane";
/// Broadcast after every save so other windows reload their config.
pub const CHANGED_EVENT: &str = "config-changed";

const PANES: &[&str] = &["general", "publishing", "media", "connections", "advanced"];
const WIDTH: f64 = 640.0;
const INITIAL_HEIGHT: f64 = 440.0;

fn valid_pane(pane: Option<&str>) -> Option<&'static str> {
    let pane = pane?.trim().trim_matches('"');
    PANES.iter().copied().find(|p| *p == pane)
}

/// Open (or focus) the Settings window, optionally on a given pane.
pub fn open_window(app: &AppHandle, pane: Option<&str>) -> Result<(), String> {
    let pane = valid_pane(pane);
    if let Some(win) = app.get_webview_window(WINDOW_LABEL) {
        let _ = win.unminimize();
        let _ = win.show();
        let _ = win.set_focus();
        if let Some(p) = pane {
            let _ = app.emit_to(WINDOW_LABEL, SHOW_PANE_EVENT, p);
        }
        return Ok(());
    }
    let url = match pane {
        Some(p) => format!("index.html?window=settings&pane={}", p),
        None => "index.html?window=settings".to_string(),
    };
    let builder = WebviewWindowBuilder::new(app, WINDOW_LABEL, WebviewUrl::App(url.into()))
        .title("Settings")
        .inner_size(WIDTH, INITIAL_HEIGHT)
        .resizable(false)
        .maximizable(false)
        .minimizable(false)
        .focused(true)
        .center();
    // Unified titlebar + toolbar like a native Settings window: the webview
    // draws the pane title and the toolbar of pane icons under the lights.
    #[cfg(target_os = "macos")]
    let builder = builder
        .title_bar_style(tauri::TitleBarStyle::Overlay)
        .hidden_title(true);
    builder.build().map_err(|e| e.to_string())?;
    Ok(())
}

/// Listen for `menu:settings` from anywhere (Rust menu handler or a webview).
pub fn register(app: &AppHandle) {
    let handle = app.clone();
    app.listen_any(OPEN_EVENT, move |event| {
        let pane: Option<String> = serde_json::from_str::<Option<String>>(event.payload())
            .ok()
            .flatten();
        let handle = handle.clone();
        // Window creation must not block the emitter's thread.
        tauri::async_runtime::spawn(async move {
            if let Err(e) = open_window(&handle, pane.as_deref()) {
                log::warn!("settings: could not open window: {}", e);
            }
        });
    });
}

// --- COMMANDS ---

#[tauri::command]
pub async fn open_settings_window(app: AppHandle, pane: Option<String>) -> Result<(), String> {
    open_window(&app, pane.as_deref())
}

/// Which credentials come from the environment (.env) instead of config.json.
/// Booleans and non-secret hostnames only; never a key, token or password.
#[derive(Debug, Clone, Serialize)]
pub struct EnvReport {
    /// All three CLOUDINARY_* vars set: they win over the Media pane fields.
    pub cloudinary: bool,
    /// All five R2_* vars set: they win over the Media pane fields.
    pub r2: bool,
    pub mastodon_token: bool,
    /// MASTODON_INSTANCE (a hostname, not a secret), overrides the setting.
    pub mastodon_instance: Option<String>,
    pub bluesky: bool,
    /// UMAMI_USERNAME + UMAMI_PASSWORD + UMAMI_WEBSITE_ID all set.
    pub umami: bool,
    /// UMAMI_URL (not a secret): where stats are fetched from.
    pub umami_url: Option<String>,
    pub obsidian_api_key: bool,
    /// "anthropic" / "openai" / None: which key AI alt text would use.
    pub alt_text_provider: Option<String>,
    /// .env files Dispatch reads at launch that exist on this Mac (paths only).
    pub dotenv_files: Vec<String>,
}

fn env_set(name: &str) -> bool {
    std::env::var(name)
        .map(|v| !v.trim().is_empty())
        .unwrap_or(false)
}

fn env_value(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.trim().is_empty())
}

/// The same candidate list `lib.rs::run` loads, filtered to files that exist.
fn dotenv_candidates() -> Vec<String> {
    let mut paths = vec![];
    if let Ok(cwd) = std::env::current_dir() {
        paths.push(cwd.join(".env"));
    }
    paths.push(Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../.env")).to_path_buf());
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            paths.push(dir.join(".env"));
            paths.push(dir.join("../Resources/.env"));
        }
    }
    let mut out: Vec<String> = vec![];
    for p in paths {
        if let Ok(canon) = p.canonicalize() {
            let s = canon.to_string_lossy().to_string();
            if canon.is_file() && !out.contains(&s) {
                out.push(s);
            }
        }
    }
    out
}

pub fn env_report() -> EnvReport {
    EnvReport {
        cloudinary: [
            "CLOUDINARY_CLOUD_NAME",
            "CLOUDINARY_API_KEY",
            "CLOUDINARY_API_SECRET",
        ]
        .iter()
        .all(|v| env_set(v)),
        r2: [
            "R2_ACCOUNT_ID",
            "R2_ACCESS_KEY_ID",
            "R2_SECRET_ACCESS_KEY",
            "R2_BUCKET",
            "R2_PUBLIC_URL_BASE",
        ]
        .iter()
        .all(|v| env_set(v)),
        mastodon_token: env_set("MASTODON_ACCESS_TOKEN"),
        mastodon_instance: env_value("MASTODON_INSTANCE"),
        bluesky: env_set("BLUESKY_HANDLE") && env_set("BLUESKY_APP_PASSWORD"),
        umami: ["UMAMI_USERNAME", "UMAMI_PASSWORD", "UMAMI_WEBSITE_ID"]
            .iter()
            .all(|v| env_set(v)),
        umami_url: env_value("UMAMI_URL"),
        obsidian_api_key: env_set("OBSIDIAN_API_KEY"),
        // Same order as alttext.rs `detect_provider`: OpenAI wins when both are set.
        alt_text_provider: if env_set("OPENAI_API_KEY") {
            Some("openai".into())
        } else if env_set("ANTHROPIC_API_KEY") {
            Some("anthropic".into())
        } else {
            None
        },
        dotenv_files: dotenv_candidates(),
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct SettingsSnapshot {
    /// The config with every secret masked to its last 4.
    pub config: config::AppConfig,
    pub config_path: String,
    pub config_exists: bool,
    pub env: EnvReport,
}

#[tauri::command]
pub fn get_settings() -> Result<SettingsSnapshot, String> {
    let cfg = config::get()?;
    let path = config::get_config_file_path();
    Ok(SettingsSnapshot {
        config: config::masked(&cfg),
        config_exists: Path::new(&path).exists(),
        config_path: path,
        env: env_report(),
    })
}

/// Save from the Settings window. Masked secrets keep their stored values.
/// Returns the saved config, masked, and tells every window it changed.
#[tauri::command]
pub fn save_settings(
    app: AppHandle,
    config_data: config::AppConfig,
) -> Result<config::AppConfig, String> {
    config::update_from_ui(config_data)?;
    let _ = app.emit(CHANGED_EVENT, ());
    Ok(config::masked(&config::get()?))
}

/// Show config.json in Finder (or its folder if it hasn't been written yet).
#[tauri::command]
pub fn reveal_config_file() -> Result<(), String> {
    let path = config::get_config_file_path();
    let p = Path::new(&path);
    let mut cmd = std::process::Command::new("open");
    if p.exists() {
        cmd.arg("-R").arg(p);
    } else if let Some(dir) = p.parent().filter(|d| d.exists()) {
        cmd.arg(dir);
    } else {
        return Err("The settings file hasn't been created yet.".into());
    }
    cmd.spawn().map_err(|e| e.to_string())?;
    Ok(())
}

// --- CHECKS (inline validation) ---

#[derive(Debug, Clone, Serialize, Default)]
pub struct VaultCheck {
    pub exists: bool,
    /// Has a `.obsidian/` folder, so it's really an Obsidian vault.
    pub is_obsidian_vault: bool,
    /// The folder's own name, which is usually the Obsidian vault name.
    pub folder_name: String,
    /// Publishable folders that don't exist inside this vault.
    pub missing_dirs: Vec<String>,
}

#[tauri::command]
pub fn check_vault_folder(path: String, publishable_dirs: Vec<String>) -> VaultCheck {
    let p = Path::new(&path);
    if path.trim().is_empty() || !p.is_dir() {
        return VaultCheck::default();
    }
    VaultCheck {
        exists: true,
        is_obsidian_vault: p.join(".obsidian").is_dir(),
        folder_name: p
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default(),
        missing_dirs: publishable_dirs
            .into_iter()
            .filter(|d| !d.trim().is_empty() && !p.join(d.trim()).is_dir())
            .collect(),
    }
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct RepoCheck {
    pub exists: bool,
    pub is_git: bool,
    /// Branch checked out right now ("" when detached).
    pub current_branch: String,
    /// The configured branch exists in this repo.
    pub branch_exists: bool,
    /// Checked-out branch == configured branch (publishing requires this).
    pub on_branch: bool,
    /// An `origin` remote exists (publishing pushes there).
    pub has_origin: bool,
    /// The content folder (pattern up to `/{year}`) exists in the repo.
    pub content_dir_exists: bool,
    pub content_dir: String,
}

const CHECK_TIMEOUT: Duration = Duration::from_secs(5);

fn git_out(repo: &str, args: &[&str]) -> Option<String> {
    let out = crate::publish::git(repo, args, CHECK_TIMEOUT).ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// Local-only checks (no fetch): is it a repo, on the right branch, with an
/// origin, and does the content folder exist.
#[tauri::command]
pub async fn check_publish_target(
    repo_path: String,
    branch: String,
    content_path_pattern: String,
) -> RepoCheck {
    tauri::async_runtime::spawn_blocking(move || {
        let p = Path::new(&repo_path);
        if repo_path.trim().is_empty() || !p.is_dir() {
            return RepoCheck::default();
        }
        let mut c = RepoCheck {
            exists: true,
            ..Default::default()
        };
        c.content_dir = content_path_pattern
            .split("/{year}")
            .next()
            .unwrap_or("")
            .trim_end_matches('/')
            .to_string();
        c.content_dir_exists = !c.content_dir.is_empty() && p.join(&c.content_dir).is_dir();
        if git_out(&repo_path, &["rev-parse", "--git-dir"]).is_none() {
            return c;
        }
        c.is_git = true;
        c.current_branch = git_out(&repo_path, &["branch", "--show-current"]).unwrap_or_default();
        let branch = branch.trim();
        c.on_branch = !branch.is_empty() && c.current_branch == branch;
        c.branch_exists = !branch.is_empty()
            && git_out(
                &repo_path,
                &[
                    "rev-parse",
                    "--verify",
                    "--quiet",
                    &format!("refs/heads/{}", branch),
                ],
            )
            .is_some();
        // Only whether it exists: a remote URL can carry a token.
        c.has_origin = git_out(&repo_path, &["remote"])
            .map(|r| r.lines().any(|l| l.trim() == "origin"))
            .unwrap_or(false);
        c
    })
    .await
    .unwrap_or_default()
}

/// Is a macOS app with this name installed in one of the usual places?
#[tauri::command]
pub fn check_app_installed(app_name: String) -> bool {
    let name = app_name.trim().trim_end_matches(".app");
    if name.is_empty() || name.contains('/') {
        return false;
    }
    let home = std::env::var("HOME").unwrap_or_default();
    let dirs = [
        "/Applications".to_string(),
        "/Applications/Utilities".to_string(),
        "/System/Applications".to_string(),
        "/System/Applications/Utilities".to_string(),
        format!("{}/Applications", home),
    ];
    dirs.iter()
        .any(|d| Path::new(d).join(format!("{}.app", name)).exists())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pane_ids_are_validated() {
        assert_eq!(valid_pane(Some("media")), Some("media"));
        assert_eq!(valid_pane(Some("\"media\"")), Some("media"));
        assert_eq!(valid_pane(Some("../../etc")), None);
        assert_eq!(valid_pane(None), None);
    }

    #[test]
    fn vault_check_reports_missing_dirs() {
        let dir = std::env::temp_dir().join("dispatch-settings-vault-check");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join(".obsidian")).unwrap();
        std::fs::create_dir_all(dir.join("blog")).unwrap();
        let c = check_vault_folder(
            dir.to_string_lossy().to_string(),
            vec!["blog".into(), "dispatch".into()],
        );
        assert!(c.exists && c.is_obsidian_vault);
        assert_eq!(c.missing_dirs, vec!["dispatch"]);
        assert_eq!(c.folder_name, "dispatch-settings-vault-check");
        let _ = std::fs::remove_dir_all(&dir);

        let missing = check_vault_folder("/definitely/not/here".into(), vec![]);
        assert!(!missing.exists);
    }

    #[test]
    fn env_report_never_carries_secret_values() {
        // The struct only has bools and the two non-secret hostnames.
        let r = env_report();
        let json = serde_json::to_string(&r).unwrap();
        for var in [
            "MASTODON_ACCESS_TOKEN",
            "BLUESKY_APP_PASSWORD",
            "UMAMI_PASSWORD",
            "CLOUDINARY_API_SECRET",
        ] {
            if let Ok(v) = std::env::var(var) {
                if v.len() > 3 {
                    assert!(!json.contains(&v));
                }
            }
        }
    }
}
