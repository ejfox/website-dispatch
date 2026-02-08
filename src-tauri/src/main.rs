// This line hides the console window on Windows when running a release build.
// Without it, a black terminal window would pop up behind your app.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

// --- IMPORTS ---
use serde::{Deserialize, Serialize};
use std::fs;
use std::sync::Mutex;
use tauri::{CustomMenuItem, Manager, SystemTray, SystemTrayEvent, SystemTrayMenu, SystemTrayMenuItem};

// --- MODULE DECLARATIONS ---
// `mod X` tells Rust "there's a file called X.rs in this folder - include it"
// This is how Rust organizes code into separate files.
// Each module becomes accessible as `module_name::function_name()`
mod asset_usage;   // Tracks which Cloudinary images are used in which posts
mod cloudinary;    // Uploads images/videos to Cloudinary CDN
mod obsidian;      // Talks to Obsidian's Local REST API for backlinks
mod preview;       // Manages a local Node.js server for previewing posts
mod publish;       // Handles git operations to publish posts to your website
mod vault;         // Scans your Obsidian vault for markdown files

// --- DATA STRUCTURES ---
// These structs define the shape of data we pass between Rust and the Vue frontend.
// Think of them like TypeScript interfaces.

// #[derive(...)] is a "derive macro" - it auto-generates code for common traits:
// - Debug: allows printing with {:?} for debugging
// - Clone: allows making copies of this struct
// - Serialize: converts to JSON (for sending to frontend)
// - Deserialize: converts from JSON (for receiving from frontend)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarkdownFile {
    // `pub` means "public" - accessible from other modules
    // `String` is an owned string (like JavaScript's string, but you own the memory)
    pub path: String,           // Full path: "/Users/ej/vault/blog/my-post.md"
    pub filename: String,       // Just the filename: "my-post.md"

    // `Option<String>` means "maybe a String, maybe nothing"
    // Like TypeScript's `string | null`. Use Some("value") or None.
    pub title: Option<String>,  // Title from the # heading, if found
    pub dek: Option<String>,    // Subtitle/deck from frontmatter

    pub date: Option<String>,   // Date from frontmatter: "2026-01-15"

    // `Vec<String>` is a growable array/list of Strings
    pub tags: Vec<String>,      // Tags from frontmatter: ["coding", "rust"]

    // `u64` is an unsigned 64-bit integer (can't be negative)
    // Used for Unix timestamps (seconds since Jan 1, 1970)
    pub created: u64,           // When file was created (timestamp)
    pub modified: u64,          // When file was last modified (timestamp)

    // `usize` is an unsigned integer sized for your platform (64-bit on modern machines)
    // Used for counting/indexing
    pub word_count: usize,      // Number of words in the post body

    // `bool` is true or false
    pub is_safe: bool,          // True if no warnings (safe to publish)

    pub warnings: Vec<String>,  // List of issues: ["No date", "Has TODOs"]

    pub published_url: Option<String>,  // URL if already published: "https://ejfox.com/blog/..."
    pub published_date: Option<u64>,    // When it was published (timestamp)
    pub source_dir: String,             // Relative path in vault: "blog/2026"

    // Visibility controls for unlisted/password-protected posts
    pub unlisted: bool,              // If true, won't appear in listings
    pub password: Option<String>,    // If set, requires password to view
}

// Configuration for where to find things on this computer
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub vault_path: String,      // Path to Obsidian vault
    pub website_repo: String,    // Path to website git repo
    pub excluded_dirs: Vec<String>, // Folders to skip when scanning
}

// `impl` adds methods to a struct (like class methods in other languages)
// `Default` is a trait (interface) that provides a default() method
impl Default for Config {
    fn default() -> Self {
        // Get the HOME environment variable (e.g., "/Users/ejfox")
        // .unwrap_or_default() returns "" if HOME isn't set
        let home = std::env::var("HOME").unwrap_or_default();

        // Return a Config with sensible defaults
        // `format!` is like JavaScript template literals: `${home}/path`
        Config {
            vault_path: format!(
                "{}/Library/Mobile Documents/iCloud~md~obsidian/Documents/ejfox",
                home
            ),
            website_repo: format!("{}/code/website2", home),

            // .into() converts &str (string literal) to String
            // Rust distinguishes between borrowed strings (&str) and owned strings (String)
            excluded_dirs: vec![
                "week-notes".into(),
                "robot-notes".into(),
                "private".into(),
                "templates".into(),
                "attachments".into(),
                "drafts".into(),
            ],
        }
    }
}

// --- DISPATCH STATUS INTEROP ---
// Writes a .dispatch/status.json file to the vault root so the Obsidian
// companion plugin can read the current state of all tracked files.

#[derive(Serialize)]
struct DispatchStatusFile {
    path: String,
    slug: String,
    title: Option<String>,
    published_url: Option<String>,
    warnings: Vec<String>,
    word_count: usize,
    is_safe: bool,
    unlisted: bool,
    has_password: bool,
    modified: u64,
}

#[derive(Serialize)]
struct DispatchStats {
    total: usize,
    drafts: usize,
    published: usize,
    total_words: usize,
}

#[derive(Serialize)]
struct DispatchStatus {
    updated_at: String,
    files: Vec<DispatchStatusFile>,
    stats: DispatchStats,
}

fn write_dispatch_status(files: &[MarkdownFile]) {
    let config = Config::default();
    let dispatch_dir = format!("{}/.dispatch", config.vault_path);

    // Create .dispatch/ directory if it doesn't exist
    if let Err(e) = fs::create_dir_all(&dispatch_dir) {
        eprintln!("Failed to create .dispatch dir: {}", e);
        return;
    }

    let vault_prefix = format!("{}/", config.vault_path);

    let status_files: Vec<DispatchStatusFile> = files
        .iter()
        .map(|f| {
            let relative_path = f.path
                .strip_prefix(&vault_prefix)
                .unwrap_or(&f.path)
                .to_string();

            let slug = f.filename.trim_end_matches(".md").to_string();

            DispatchStatusFile {
                path: relative_path,
                slug,
                title: f.title.clone(),
                published_url: f.published_url.clone(),
                warnings: f.warnings.clone(),
                word_count: f.word_count,
                is_safe: f.is_safe,
                unlisted: f.unlisted,
                has_password: f.password.is_some(),
                modified: f.modified,
            }
        })
        .collect();

    let drafts = files.iter().filter(|f| f.published_url.is_none()).count();
    let published = files.iter().filter(|f| f.published_url.is_some()).count();
    let total_words: usize = files.iter().map(|f| f.word_count).sum();

    let status = DispatchStatus {
        updated_at: chrono::Utc::now().to_rfc3339(),
        files: status_files,
        stats: DispatchStats {
            total: files.len(),
            drafts,
            published,
            total_words,
        },
    };

    let status_path = format!("{}/status.json", dispatch_dir);
    match serde_json::to_string_pretty(&status) {
        Ok(json) => {
            if let Err(e) = fs::write(&status_path, json) {
                eprintln!("Failed to write dispatch status: {}", e);
            }
        }
        Err(e) => eprintln!("Failed to serialize dispatch status: {}", e),
    }
}

// --- TAURI COMMANDS ---
// These are functions the Vue frontend can call using `invoke("command_name", args)`
// The #[tauri::command] attribute makes them available to JavaScript.

// Get a list of recent markdown files from the vault
// Returns Result<success_type, error_type> - Rust forces you to handle errors
#[tauri::command]
fn get_recent_files(limit: usize) -> Result<Vec<MarkdownFile>, String> {
    // Call the function from the vault module
    // The ? would propagate errors, but we're just passing through here
    vault::get_recent_files(limit)
}

// Read the contents of a file as a string
#[tauri::command]
fn get_file_content(path: String) -> Result<String, String> {
    // fs::read_to_string reads a file and returns its contents
    // .map_err() converts the std::io::Error to a String for the frontend
    fs::read_to_string(&path).map_err(|e| e.to_string())
}

// Append content to the end of a file (used for adding to posts)
#[tauri::command]
fn append_to_file(path: String, content: String) -> Result<(), String> {
    // Import Write trait to get write!/writeln! macros
    use std::io::Write;

    // Open file in append mode
    // OpenOptions lets you specify how to open a file
    let mut file = std::fs::OpenOptions::new()
        .append(true)  // Add to end instead of overwriting
        .open(&path)
        .map_err(|e| e.to_string())?;  // ? = return early if error

    // Add blank line, content, and trailing newline
    writeln!(file).map_err(|e| e.to_string())?;
    write!(file, "{}", content).map_err(|e| e.to_string())?;
    writeln!(file).map_err(|e| e.to_string())?;

    // () is the "unit type" - like void, means "nothing to return"
    Ok(())
}

// Publish a markdown file to the website (copy + git commit + push)
#[tauri::command]
fn publish_file(source_path: String, slug: String) -> Result<String, String> {
    let result = publish::publish_file(&source_path, &slug)?;
    // Send native notification on success
    let _ = tauri::api::notification::Notification::new("com.ejfox.dispatch")
        .title("Post Published")
        .body(&format!("{} is now live", slug))
        .show();
    // Update dispatch status for Obsidian companion plugin
    if let Ok(files) = vault::get_recent_files(200) {
        write_dispatch_status(&files);
    }
    Ok(result)
}

// Unpublish a file (move from blog/ to drafts/ in the website repo)
#[tauri::command]
fn unpublish_file(slug: String) -> Result<(), String> {
    publish::unpublish_file(&slug)?;
    let _ = tauri::api::notification::Notification::new("com.ejfox.dispatch")
        .title("Post Unpublished")
        .body(&format!("{} moved to drafts", slug))
        .show();
    // Update dispatch status for Obsidian companion plugin
    if let Ok(files) = vault::get_recent_files(200) {
        write_dispatch_status(&files);
    }
    Ok(())
}

// Get the current git status of the website repo
#[tauri::command]
fn get_git_status() -> publish::GitStatus {
    publish::get_git_status()
}

// Add a tag to a markdown file's frontmatter
#[tauri::command]
fn add_tag_to_file(path: String, tag: String) -> Result<(), String> {
    vault::add_tag_to_file(&path, &tag)
}

// Get backlinks (other files that link to this one) via Obsidian's API
// `async` means this function can pause while waiting for network requests
#[tauri::command]
async fn get_backlinks(filename: String) -> Result<Vec<obsidian::Backlink>, String> {
    // .await pauses until the async operation completes
    obsidian::get_backlinks(&filename).await
}

// Check if Obsidian's Local REST API is running
#[tauri::command]
async fn check_obsidian_api() -> bool {
    obsidian::check_api_status().await
}

// Open a file in Obsidian using its URL scheme (obsidian://open?vault=...&file=...)
#[tauri::command]
fn open_in_obsidian(path: String) -> Result<(), String> {
    let config = Config::default();

    // Get path relative to vault root by stripping the vault_path prefix
    // .strip_prefix() returns Option, .unwrap_or() provides fallback
    let relative_path = path
        .strip_prefix(&config.vault_path)
        .unwrap_or(&path)
        .trim_start_matches('/');

    // Build the Obsidian URL scheme
    let vault_name = "ejfox";
    let url = format!(
        "obsidian://open?vault={}&file={}",
        vault_name,
        urlencoding::encode(relative_path)  // URL-encode special characters
    );

    // Run macOS `open` command with the URL
    // .spawn() starts the process without waiting for it to finish
    std::process::Command::new("open")
        .arg(&url)
        .spawn()
        .map_err(|e| e.to_string())?;
    Ok(())
}

// Open a file in any macOS app (e.g., "iA Writer", "VS Code")
#[tauri::command]
fn open_in_app(path: String, app: String) -> Result<(), String> {
    // `open -a "App Name" /path/to/file` opens file in specified app
    std::process::Command::new("open")
        .args(["-a", &app, &path])
        .spawn()
        .map_err(|e| e.to_string())?;
    Ok(())
}

// Open a terminal (iTerm or Terminal.app) and run a command with the file path
#[tauri::command]
fn open_in_terminal(path: String, cmd: String) -> Result<(), String> {
    // AppleScript to control iTerm
    // r#"..."# is a raw string literal - no escape sequences needed
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

    // Try iTerm first using osascript (AppleScript runner)
    let result = std::process::Command::new("osascript")
        .args(["-e", &iterm_script])
        .output();

    // .is_ok() checks if Result is Ok, .unwrap() extracts the value
    if result.is_ok() && result.unwrap().status.success() {
        return Ok(());
    }

    // Fallback to Terminal.app if iTerm isn't available
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

// Tell the preview server which file to display
#[tauri::command]
fn set_preview_file(path: String) {
    preview::set_file(&path);
}

// --- DISPATCH QUEUE COMMAND ---
// Read the queue file that the Obsidian companion plugin writes to mark
// files as "ready to publish".

#[tauri::command]
fn read_dispatch_queue() -> Result<String, String> {
    let config = Config::default();
    let queue_path = format!("{}/.dispatch/queue.json", config.vault_path);
    fs::read_to_string(&queue_path).map_err(|e| e.to_string())
}

// --- CLOUDINARY COMMANDS ---
// Cloudinary is a CDN service for hosting images/videos

// Status of Cloudinary configuration
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CloudinaryConfigStatus {
    pub configured: bool,           // Are credentials set?
    pub cloud_name: Option<String>, // The Cloudinary account name
}

// Check if Cloudinary API credentials are valid
#[tauri::command]
async fn check_cloudinary_status() -> Result<bool, String> {
    Ok(cloudinary::check_credentials().await)
}

// Get Cloudinary configuration (cloud name, whether it's set up)
#[tauri::command]
fn get_cloudinary_config() -> Result<CloudinaryConfigStatus, String> {
    // match is like switch/case but more powerful
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

// Upload a single file to Cloudinary
#[tauri::command]
async fn cloudinary_upload(
    file_path: String,
    folder: Option<String>,  // Optional folder to organize uploads
) -> Result<cloudinary::UploadResult, String> {
    // .as_deref() converts Option<String> to Option<&str>
    cloudinary::upload_file(&file_path, folder.as_deref(), None).await
}

// Upload multiple files to Cloudinary
#[tauri::command]
async fn cloudinary_upload_batch(
    file_paths: Vec<String>,
    folder: Option<String>,
) -> Result<Vec<cloudinary::UploadResult>, String> {
    let mut results = Vec::new();
    // Loop through each path and upload
    for path in file_paths {
        let result = cloudinary::upload_file(&path, folder.as_deref(), None).await?;
        results.push(result);
    }
    Ok(results)
}

// List assets from Cloudinary media library (paginated)
#[tauri::command]
async fn cloudinary_list_assets(
    resource_type: Option<String>,  // "image" or "video"
    max_results: Option<u32>,       // How many to return
    cursor: Option<String>,         // Pagination cursor for "load more"
) -> Result<cloudinary::MediaLibraryPage, String> {
    cloudinary::list_assets(resource_type.as_deref(), max_results, cursor.as_deref()).await
}

// Search Cloudinary assets by query
#[tauri::command]
async fn cloudinary_search(
    query: String,
    max_results: Option<u32>,
) -> Result<cloudinary::MediaLibraryPage, String> {
    cloudinary::search_assets(&query, max_results).await
}

// Find local media references in a markdown file (images that need uploading)
#[tauri::command]
fn get_local_media(path: String) -> Result<Vec<cloudinary::LocalMediaRef>, String> {
    let content = fs::read_to_string(&path).map_err(|e| e.to_string())?;

    // Get the directory containing this file
    let source_dir = std::path::Path::new(&path)
        .parent()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default();

    Ok(cloudinary::extract_local_media(&content, &source_dir))
}

// Upload local media to Cloudinary and get replacement text
#[tauri::command]
async fn fix_local_media(
    _source_path: String,  // _ prefix = unused parameter (kept for API compatibility)
    media_refs: Vec<cloudinary::LocalMediaRef>,
    folder: Option<String>,
) -> Result<Vec<cloudinary::MediaFixResult>, String> {
    let mut results = Vec::new();

    for media_ref in media_refs {
        // Check if the file actually exists on disk
        let resolved_path = match &media_ref.resolved_path {
            Some(p) => p.clone(),
            None => {
                // File not found - add error result and continue
                results.push(cloudinary::MediaFixResult {
                    original_ref: media_ref.clone(),
                    upload_result: cloudinary::UploadResult {
                        success: false,
                        asset: None,
                        error: Some("File not found".to_string()),
                    },
                    replacement_text: None,
                });
                continue;  // Skip to next item
            }
        };

        // Upload the file
        let upload_result =
            cloudinary::upload_file(&resolved_path, folder.as_deref(), None).await?;

        // Generate markdown replacement text if upload succeeded
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

// Apply text replacements to a file (swap local paths for Cloudinary URLs)
#[tauri::command]
fn apply_media_fixes(
    file_path: String,
    fixes: Vec<(String, String)>,  // Vec of (old_text, new_text) tuples
) -> Result<(), String> {
    cloudinary::apply_fixes_to_file(&file_path, &fixes)
}

// --- ASSET USAGE COMMANDS ---
// Track which Cloudinary assets are used in which posts

// Scan the entire vault for Cloudinary URL usage
#[tauri::command]
async fn scan_asset_usage() -> Result<asset_usage::UsageScanResult, String> {
    // spawn_blocking runs CPU-intensive work on a separate thread
    // so it doesn't block async operations
    tokio::task::spawn_blocking(|| asset_usage::scan_vault_for_usage())
        .await
        .map_err(|e| e.to_string())?
}

// Get list of posts that use a specific Cloudinary asset
#[tauri::command]
fn get_asset_usage(public_id: String) -> Result<Vec<asset_usage::AssetUsage>, String> {
    asset_usage::get_asset_usage(&public_id)
}

// Get list of Cloudinary assets used in a specific post
#[tauri::command]
fn get_post_assets(post_path: String) -> Result<Vec<String>, String> {
    asset_usage::get_post_assets(&post_path)
}

// List all folders in Cloudinary (for folder picker UI)
#[tauri::command]
async fn cloudinary_list_folders() -> Result<Vec<String>, String> {
    let config = cloudinary::get_config()?;

    let url = format!(
        "https://api.cloudinary.com/v1_1/{}/folders",
        config.cloud_name
    );

    // Make HTTP request with basic auth
    let client = reqwest::Client::new();
    let response = client
        .get(&url)
        .basic_auth(&config.api_key, Some(&config.api_secret))
        .send()
        .await
        .map_err(|e| format!("Request failed: {}", e))?;

    // Check for HTTP errors
    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(format!("HTTP {}: {}", status, body));
    }

    // Parse JSON response
    let json: serde_json::Value = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse response: {}", e))?;

    // Extract folder paths from the response
    // .as_array() returns Option<&Vec>, unwrap_or provides empty fallback
    let folders = json["folders"]
        .as_array()
        .unwrap_or(&vec![])
        .iter()
        .filter_map(|f| f["path"].as_str().map(|s| s.to_string()))
        .collect();

    Ok(folders)
}

// Open or focus the preview window
#[tauri::command]
async fn open_preview(app_handle: tauri::AppHandle) -> Result<String, String> {
    use tauri::WindowBuilder;

    // Check if preview window already exists
    if let Some(window) = app_handle.get_window("preview") {
        let _ = window.show();
        let _ = window.set_focus();
        return Ok("http://127.0.0.1:6419".into());
    }

    // Create new preview window pointing to the preview server
    let window = WindowBuilder::new(
        &app_handle,
        "preview",  // Window ID
        tauri::WindowUrl::External("http://127.0.0.1:6419".parse().unwrap())
    )
    .title("Preview")
    .inner_size(900.0, 800.0)      // Width x Height
    .min_inner_size(400.0, 300.0)  // Minimum size
    .decorations(true)              // Show title bar
    .resizable(true)
    .build()
    .map_err(|e| e.to_string())?;

    let _ = window.set_focus();

    Ok("http://127.0.0.1:6419".into())
}

// --- TRAY MENU ---
// Builds a rich tray menu showing recent drafts, stats, and actions.

/// State for mapping tray menu item IDs back to file paths
static TRAY_FILE_PATHS: Mutex<Vec<String>> = Mutex::new(Vec::new());

fn build_tray_menu(files: &[MarkdownFile]) -> SystemTrayMenu {
    let mut menu = SystemTrayMenu::new();
    let mut paths = Vec::new();

    // Add up to 5 recent unpublished files
    let recent_drafts: Vec<&MarkdownFile> = files
        .iter()
        .filter(|f| f.published_url.is_none())
        .take(5)
        .collect();

    for (i, file) in recent_drafts.iter().enumerate() {
        let label = file
            .title
            .clone()
            .unwrap_or_else(|| file.filename.trim_end_matches(".md").to_string());
        // Truncate long titles
        let label = if label.len() > 40 {
            format!("{}...", &label[..37])
        } else {
            label
        };
        menu = menu.add_item(CustomMenuItem::new(format!("file:{}", i), label));
        paths.push(file.path.clone());
    }

    // Stats line
    let draft_count = files.iter().filter(|f| f.published_url.is_none()).count();
    let published_count = files.iter().filter(|f| f.published_url.is_some()).count();
    let stats_label = format!("{} drafts · {} published", draft_count, published_count);

    menu = menu
        .add_native_item(SystemTrayMenuItem::Separator)
        .add_item(CustomMenuItem::new("stats", stats_label).disabled())
        .add_native_item(SystemTrayMenuItem::Separator)
        .add_item(CustomMenuItem::new("new_post", "New Post..."))
        .add_item(CustomMenuItem::new("open", "Open Dispatch"))
        .add_item(CustomMenuItem::new("quit", "Quit"));

    // Store paths for lookup in event handler
    if let Ok(mut guard) = TRAY_FILE_PATHS.lock() {
        *guard = paths;
    }

    menu
}

// --- TRAY REFRESH COMMAND ---
#[tauri::command]
fn refresh_tray(app_handle: tauri::AppHandle) -> Result<(), String> {
    let files = vault::get_recent_files(200)?;
    let menu = build_tray_menu(&files);
    app_handle
        .tray_handle()
        .set_menu(menu)
        .map_err(|e| e.to_string())?;
    write_dispatch_status(&files);
    Ok(())
}

// --- DOCK BADGE ---
#[tauri::command]
fn set_dock_badge(count: usize) {
    #[cfg(target_os = "macos")]
    unsafe {
        use cocoa::appkit::NSApp;
        use cocoa::base::nil;
        use cocoa::foundation::NSString;
        use objc::msg_send;
        use objc::sel;
        use objc::sel_impl;

        let dock_tile: cocoa::base::id = msg_send![NSApp(), dockTile];
        let label = if count > 0 {
            NSString::alloc(nil).init_str(&count.to_string())
        } else {
            nil
        };
        let _: () = msg_send![dock_tile, setBadgeLabel: label];
    }

    #[cfg(not(target_os = "macos"))]
    {
        let _ = count; // suppress unused warning on non-macOS
    }
}

// --- MAIN FUNCTION ---
fn main() {
    // --- LOAD ENVIRONMENT VARIABLES ---
    // Try to load .env file from multiple locations
    // let _ = ... ignores the result (we don't care if it fails)

    // 1. Current directory (works in dev mode: cargo run)
    let _ = dotenvy::dotenv();

    // 2. Project root (explicit path for dev)
    let _ = dotenvy::from_filename("/Users/ejfox/code/website-dispatch/.env");

    // 3. Next to executable (for bundled .app)
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let _ = dotenvy::from_filename(dir.join(".env"));
            let _ = dotenvy::from_filename(dir.join("../Resources/.env"));
        }
    }

    // --- START PREVIEW SERVER ---
    // Start the Node.js preview server immediately so it's ready when needed
    preview::init_server();

    // --- SET UP SYSTEM TRAY ---
    // Build initial tray menu with recent files
    let initial_menu = match vault::get_recent_files(200) {
        Ok(files) => build_tray_menu(&files),
        Err(_) => {
            // Fallback to simple menu if vault scan fails at startup
            SystemTrayMenu::new()
                .add_item(CustomMenuItem::new("open", "Open Dispatch"))
                .add_item(CustomMenuItem::new("quit", "Quit"))
        }
    };

    let system_tray = SystemTray::new().with_menu(initial_menu);

    // --- BUILD AND RUN THE APP ---
    tauri::Builder::default()
        // Plugin to remember window size/position between launches
        .plugin(tauri_plugin_window_state::Builder::default().build())

        // Add the system tray
        .system_tray(system_tray)

        // Handle tray events (clicks on the icon/menu)
        .on_system_tray_event(|app, event| match event {
            // Left-click on tray icon: show and focus the main window
            SystemTrayEvent::LeftClick { .. } => {
                if let Some(window) = app.get_window("main") {
                    let _ = window.show();
                    let _ = window.set_focus();
                }
            }
            // Menu item clicked
            SystemTrayEvent::MenuItemClick { id, .. } => {
                // Handle file:N clicks from tray menu
                if id.starts_with("file:") {
                    if let Ok(index) = id[5..].parse::<usize>() {
                        let path = TRAY_FILE_PATHS
                            .lock()
                            .ok()
                            .and_then(|paths| paths.get(index).cloned());
                        if let Some(path) = path {
                            let _ = app.emit_all("tray-select-file", path);
                            if let Some(window) = app.get_window("main") {
                                let _ = window.show();
                                let _ = window.set_focus();
                            }
                        }
                    }
                    return;
                }

                match id.as_str() {
                    "new_post" => {
                        let _ = app.emit_all("tray-new-post", ());
                        if let Some(window) = app.get_window("main") {
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                    }
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
                }
            },
            _ => {}
        })

        // File watcher: watch blog/ directory for changes and emit events
        .setup(|app| {
            let handle = app.handle();
            let config = Config::default();
            let blog_path = format!("{}/blog", config.vault_path);

            std::thread::spawn(move || {
                use notify::{Watcher, RecursiveMode, Config as NotifyConfig};
                let (tx, rx) = std::sync::mpsc::channel();
                let mut watcher = match notify::RecommendedWatcher::new(tx, NotifyConfig::default()) {
                    Ok(w) => w,
                    Err(e) => {
                        eprintln!("File watcher init failed: {}", e);
                        return;
                    }
                };
                if let Err(e) = watcher.watch(std::path::Path::new(&blog_path), RecursiveMode::Recursive) {
                    eprintln!("File watch failed for {}: {}", blog_path, e);
                    return;
                }
                eprintln!("Watching {} for changes", blog_path);

                let mut last_emit = std::time::Instant::now()
                    .checked_sub(std::time::Duration::from_secs(5))
                    .unwrap();

                loop {
                    match rx.recv_timeout(std::time::Duration::from_millis(300)) {
                        Ok(_) => {
                            // Debounce: only emit if >500ms since last emit
                            if last_emit.elapsed() >= std::time::Duration::from_millis(500) {
                                let _ = handle.emit_all("vault-changed", ());
                                last_emit = std::time::Instant::now();
                            }
                        }
                        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
                        Err(_) => break,
                    }
                }
            });

            Ok(())
        })

        // Register all the commands that JavaScript can call
        .invoke_handler(tauri::generate_handler![
            get_recent_files,
            get_file_content,
            append_to_file,
            publish_file,
            unpublish_file,
            get_git_status,
            add_tag_to_file,
            get_backlinks,
            check_obsidian_api,
            open_in_obsidian,
            open_in_app,
            set_preview_file,
            open_preview,
            // OS integration commands
            refresh_tray,
            set_dock_badge,
            // Obsidian companion plugin interop
            read_dispatch_queue,
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

        // Start the app with the context generated at compile time
        // (includes window config from tauri.conf.json)
        .run(tauri::generate_context!())

        // .expect() panics with this message if run() fails
        .expect("error while running tauri application");
}
