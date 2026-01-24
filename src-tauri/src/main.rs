#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use serde::{Deserialize, Serialize};
use std::fs;
use tauri::{CustomMenuItem, Manager, SystemTray, SystemTrayEvent, SystemTrayMenu};

mod obsidian;
mod preview;
mod publish;
mod vault;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarkdownFile {
    pub path: String,
    pub filename: String,
    pub title: Option<String>,
    pub date: Option<String>,
    pub tags: Vec<String>,
    pub created: u64,
    pub modified: u64,
    pub word_count: usize,
    pub is_safe: bool,
    pub warnings: Vec<String>,
    pub published_url: Option<String>,
    pub published_date: Option<u64>,
    pub source_dir: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub vault_path: String,
    pub website_repo: String,
    pub excluded_dirs: Vec<String>,
}

impl Default for Config {
    fn default() -> Self {
        let home = std::env::var("HOME").unwrap_or_default();
        Config {
            vault_path: format!(
                "{}/Library/Mobile Documents/iCloud~md~obsidian/Documents/ejfox",
                home
            ),
            website_repo: format!("{}/code/website2", home),
            excluded_dirs: vec![
                "week-notes".into(),
                "robot-notes".into(),
                "private".into(),
                "templates".into(),
                "attachments".into(),
            ],
        }
    }
}

#[tauri::command]
fn get_recent_files(limit: usize) -> Result<Vec<MarkdownFile>, String> {
    vault::get_recent_files(limit)
}

#[tauri::command]
fn get_file_content(path: String) -> Result<String, String> {
    fs::read_to_string(&path).map_err(|e| e.to_string())
}

#[tauri::command]
fn publish_file(source_path: String, slug: String) -> Result<String, String> {
    publish::publish_file(&source_path, &slug)
}

#[tauri::command]
fn get_git_status() -> publish::GitStatus {
    publish::get_git_status()
}

#[tauri::command]
async fn get_backlinks(filename: String) -> Result<Vec<obsidian::Backlink>, String> {
    obsidian::get_backlinks(&filename).await
}

#[tauri::command]
async fn check_obsidian_api() -> bool {
    obsidian::check_api_status().await
}

#[tauri::command]
fn open_in_obsidian(path: String) -> Result<(), String> {
    let config = Config::default();

    // Get relative path from vault root
    let relative_path = path
        .strip_prefix(&config.vault_path)
        .unwrap_or(&path)
        .trim_start_matches('/');

    // Obsidian URL scheme - vault name is the folder name
    let vault_name = "ejfox";
    let url = format!(
        "obsidian://open?vault={}&file={}",
        vault_name,
        urlencoding::encode(relative_path)
    );

    std::process::Command::new("open")
        .arg(&url)
        .spawn()
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
fn open_in_app(path: String, app: String) -> Result<(), String> {
    std::process::Command::new("open")
        .args(["-a", &app, &path])
        .spawn()
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
fn open_in_terminal(path: String, cmd: String) -> Result<(), String> {
    // Try iTerm first, fall back to Terminal
    let iterm_script = format!(
        r#"tell application "iTerm"
            activate
            try
                set newWindow to (create window with default profile)
                tell current session of newWindow
                    write text "{} \"{}\""
                end tell
            on error
                tell current window
                    create tab with default profile
                    tell current session
                        write text "{} \"{}\""
                    end tell
                end tell
            end try
        end tell"#,
        cmd, path, cmd, path
    );

    let result = std::process::Command::new("osascript")
        .args(["-e", &iterm_script])
        .output();

    if result.is_ok() && result.unwrap().status.success() {
        return Ok(());
    }

    // Fallback to Terminal.app
    let terminal_script = format!(
        r#"tell application "Terminal"
            activate
            do script "{} \"{}\""
        end tell"#,
        cmd, path
    );

    std::process::Command::new("osascript")
        .args(["-e", &terminal_script])
        .spawn()
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
fn set_preview_file(path: String) {
    preview::set_file(&path);
}

#[tauri::command]
async fn open_preview(app_handle: tauri::AppHandle) -> Result<String, String> {
    use tauri::WindowBuilder;

    // Check if preview window already exists
    if let Some(window) = app_handle.get_window("preview") {
        let _ = window.show();
        let _ = window.set_focus();
        return Ok("http://127.0.0.1:6419".into());
    }

    // Create new preview window
    let window = WindowBuilder::new(
        &app_handle,
        "preview",
        tauri::WindowUrl::External("http://127.0.0.1:6419".parse().unwrap())
    )
    .title("Preview")
    .inner_size(900.0, 800.0)
    .min_inner_size(400.0, 300.0)
    .decorations(true)
    .resizable(true)
    .build()
    .map_err(|e| e.to_string())?;

    let _ = window.set_focus();

    Ok("http://127.0.0.1:6419".into())
}

fn main() {
    // Start preview server immediately
    preview::init_server();

    let tray_menu = SystemTrayMenu::new()
        .add_item(CustomMenuItem::new("open", "Open Dispatch"))
        .add_item(CustomMenuItem::new("quit", "Quit"));

    let system_tray = SystemTray::new().with_menu(tray_menu);

    tauri::Builder::default()
        .system_tray(system_tray)
        .on_system_tray_event(|app, event| match event {
            SystemTrayEvent::LeftClick { .. } => {
                if let Some(window) = app.get_window("main") {
                    let _ = window.show();
                    let _ = window.set_focus();
                }
            }
            SystemTrayEvent::MenuItemClick { id, .. } => match id.as_str() {
                "open" => {
                    if let Some(window) = app.get_window("main") {
                        let _ = window.show();
                        let _ = window.set_focus();
                    }
                }
                "quit" => {
                    preview::stop_server();
                    std::process::exit(0)
                }
                _ => {}
            },
            _ => {}
        })
        .invoke_handler(tauri::generate_handler![
            get_recent_files,
            get_file_content,
            publish_file,
            get_git_status,
            get_backlinks,
            check_obsidian_api,
            open_in_obsidian,
            open_in_app,
            set_preview_file,
            open_preview,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
