#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use serde::{Deserialize, Serialize};
use std::fs;
use tauri::{CustomMenuItem, Manager, SystemTray, SystemTrayEvent, SystemTrayMenu};

mod asset_usage;
mod cloudinary;
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
fn append_to_file(path: String, content: String) -> Result<(), String> {
    use std::io::Write;
    let mut file = std::fs::OpenOptions::new()
        .append(true)
        .open(&path)
        .map_err(|e| e.to_string())?;

    // Add newlines before content to ensure it's on its own line
    writeln!(file).map_err(|e| e.to_string())?;
    write!(file, "{}", content).map_err(|e| e.to_string())?;
    writeln!(file).map_err(|e| e.to_string())?;

    Ok(())
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

// Cloudinary commands
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CloudinaryConfigStatus {
    pub configured: bool,
    pub cloud_name: Option<String>,
}

#[tauri::command]
async fn check_cloudinary_status() -> Result<bool, String> {
    Ok(cloudinary::check_credentials().await)
}

#[tauri::command]
fn get_cloudinary_config() -> Result<CloudinaryConfigStatus, String> {
    match cloudinary::get_config() {
        Ok(config) => Ok(CloudinaryConfigStatus {
            configured: true,
            cloud_name: Some(config.cloud_name),
        }),
        Err(_) => Ok(CloudinaryConfigStatus {
            configured: false,
            cloud_name: None,
        }),
    }
}

#[tauri::command]
async fn cloudinary_upload(
    file_path: String,
    folder: Option<String>,
) -> Result<cloudinary::UploadResult, String> {
    cloudinary::upload_file(&file_path, folder.as_deref(), None).await
}

#[tauri::command]
async fn cloudinary_upload_batch(
    file_paths: Vec<String>,
    folder: Option<String>,
) -> Result<Vec<cloudinary::UploadResult>, String> {
    let mut results = Vec::new();
    for path in file_paths {
        let result = cloudinary::upload_file(&path, folder.as_deref(), None).await?;
        results.push(result);
    }
    Ok(results)
}

#[tauri::command]
async fn cloudinary_list_assets(
    resource_type: Option<String>,
    max_results: Option<u32>,
    cursor: Option<String>,
) -> Result<cloudinary::MediaLibraryPage, String> {
    cloudinary::list_assets(resource_type.as_deref(), max_results, cursor.as_deref()).await
}

#[tauri::command]
async fn cloudinary_search(
    query: String,
    max_results: Option<u32>,
) -> Result<cloudinary::MediaLibraryPage, String> {
    cloudinary::search_assets(&query, max_results).await
}

#[tauri::command]
fn get_local_media(path: String) -> Result<Vec<cloudinary::LocalMediaRef>, String> {
    let content = fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let source_dir = std::path::Path::new(&path)
        .parent()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default();
    Ok(cloudinary::extract_local_media(&content, &source_dir))
}

#[tauri::command]
async fn fix_local_media(
    _source_path: String,
    media_refs: Vec<cloudinary::LocalMediaRef>,
    folder: Option<String>,
) -> Result<Vec<cloudinary::MediaFixResult>, String> {
    let mut results = Vec::new();

    for media_ref in media_refs {
        let resolved_path = match &media_ref.resolved_path {
            Some(p) => p.clone(),
            None => {
                results.push(cloudinary::MediaFixResult {
                    original_ref: media_ref.clone(),
                    upload_result: cloudinary::UploadResult {
                        success: false,
                        asset: None,
                        error: Some("File not found".to_string()),
                    },
                    replacement_text: None,
                });
                continue;
            }
        };

        let upload_result =
            cloudinary::upload_file(&resolved_path, folder.as_deref(), None).await?;

        let replacement_text = if upload_result.success {
            upload_result
                .asset
                .as_ref()
                .map(|a| cloudinary::generate_replacement(&media_ref, a))
        } else {
            None
        };

        results.push(cloudinary::MediaFixResult {
            original_ref: media_ref,
            upload_result,
            replacement_text,
        });
    }

    Ok(results)
}

#[tauri::command]
fn apply_media_fixes(
    file_path: String,
    fixes: Vec<(String, String)>,
) -> Result<(), String> {
    cloudinary::apply_fixes_to_file(&file_path, &fixes)
}

// Asset usage commands
#[tauri::command]
async fn scan_asset_usage() -> Result<asset_usage::UsageScanResult, String> {
    // Run in blocking thread since it does file I/O
    tokio::task::spawn_blocking(|| asset_usage::scan_vault_for_usage())
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
fn get_asset_usage(public_id: String) -> Result<Vec<asset_usage::AssetUsage>, String> {
    asset_usage::get_asset_usage(&public_id)
}

#[tauri::command]
fn get_post_assets(post_path: String) -> Result<Vec<String>, String> {
    asset_usage::get_post_assets(&post_path)
}

#[tauri::command]
async fn cloudinary_list_folders() -> Result<Vec<String>, String> {
    let config = cloudinary::get_config()?;

    let url = format!(
        "https://api.cloudinary.com/v1_1/{}/folders",
        config.cloud_name
    );

    let client = reqwest::Client::new();
    let response = client
        .get(&url)
        .basic_auth(&config.api_key, Some(&config.api_secret))
        .send()
        .await
        .map_err(|e| format!("Request failed: {}", e))?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(format!("HTTP {}: {}", status, body));
    }

    let json: serde_json::Value = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse response: {}", e))?;

    let folders = json["folders"]
        .as_array()
        .unwrap_or(&vec![])
        .iter()
        .filter_map(|f| f["path"].as_str().map(|s| s.to_string()))
        .collect();

    Ok(folders)
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
    // Load .env file - try multiple locations for dev and prod
    // 1. Current directory (works in dev mode)
    let _ = dotenvy::dotenv();
    // 2. Project root (explicit path for dev)
    let _ = dotenvy::from_filename("/Users/ejfox/code/website-dispatch/.env");
    // 3. Next to executable (for bundled app)
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let _ = dotenvy::from_filename(dir.join(".env"));
            let _ = dotenvy::from_filename(dir.join("../Resources/.env"));
        }
    }

    // Start preview server immediately
    preview::init_server();

    let tray_menu = SystemTrayMenu::new()
        .add_item(CustomMenuItem::new("open", "Open Dispatch"))
        .add_item(CustomMenuItem::new("quit", "Quit"));

    let system_tray = SystemTray::new().with_menu(tray_menu);

    tauri::Builder::default()
        .plugin(tauri_plugin_window_state::Builder::default().build())
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
            append_to_file,
            publish_file,
            get_git_status,
            get_backlinks,
            check_obsidian_api,
            open_in_obsidian,
            open_in_app,
            set_preview_file,
            open_preview,
            // Cloudinary commands
            check_cloudinary_status,
            get_cloudinary_config,
            cloudinary_upload,
            cloudinary_upload_batch,
            cloudinary_list_assets,
            cloudinary_search,
            cloudinary_list_folders,
            get_local_media,
            fix_local_media,
            apply_media_fixes,
            // Asset usage commands
            scan_asset_usage,
            get_asset_usage,
            get_post_assets,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
