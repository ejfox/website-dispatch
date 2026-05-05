// --- IMPORTS ---
// `serde` is a library for converting Rust structs to/from JSON.
// Deserialize = JSON -> Rust struct, Serialize = Rust struct -> JSON
use serde::{Deserialize, Serialize};

// `std::fs` is Rust's standard library for file system operations (read, write, etc.)
use std::fs;

// Tauri-specific imports for building desktop apps
use tauri::Manager;

// --- MODULE DECLARATIONS ---
// `mod X` tells Rust "there's a file called X.rs in this folder - include it"
// This is how Rust organizes code into separate files.
// Each module becomes accessible as `module_name::function_name()`
mod alttext; // AI-powered alt text generation for images
mod analytics; // Umami analytics integration
mod asset_usage; // Tracks which Cloudinary images are used in which posts
mod bin_paths; // Login-shell-resolved paths to node/git
mod cloudinary; // Uploads images/videos to Cloudinary CDN
mod companion; // Companion web UI server for mobile access
pub mod config; // App configuration (vault path, publish targets, editors)
mod gear;
mod journal; // Publishing journal, streaks, milestones
mod menu; // Application menu bar builder
mod obsidian; // Talks to Obsidian's Local REST API for backlinks
mod open; // Open files in Obsidian, editors, terminal
mod patterns; // Shared compiled regex patterns (LazyLock statics)
mod preview; // Manages a local Node.js server for previewing posts
mod publish; // Handles git operations to publish posts to your website
mod sketchybar_cache; // Snapshot JSON for sketchybar / ambient surfaces
mod syndication; // Post-publish social distribution (Mastodon, etc.)
mod syndication_queue; // Scheduled syndication queue with background sender
mod vault; // Scans your Obsidian vault for markdown files
mod vault_pulse; // Read-only vault intelligence (never publishes)
mod vault_watcher; // fs::notify-driven auto-refresh on vault changes
mod webmention; // IndieWeb webmention sending // Gear inventory hygiene (Last_Used, Location, Scan_3D_URL)

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
    pub path: String,     // Full path: "/Users/ej/vault/blog/my-post.md"
    pub filename: String, // Just the filename: "my-post.md"

    // `Option<String>` means "maybe a String, maybe nothing"
    // Like TypeScript's `string | null`. Use Some("value") or None.
    pub title: Option<String>, // Title from the # heading, if found
    pub dek: Option<String>,   // Subtitle/deck from frontmatter

    pub date: Option<String>, // Date from frontmatter: "2026-01-15"

    // `Vec<String>` is a growable array/list of Strings
    pub tags: Vec<String>, // Tags from frontmatter: ["coding", "rust"]

    // `u64` is an unsigned 64-bit integer (can't be negative)
    // Used for Unix timestamps (seconds since Jan 1, 1970)
    pub created: u64,  // When file was created (timestamp)
    pub modified: u64, // When file was last modified (timestamp)

    // `usize` is an unsigned integer sized for your platform (64-bit on modern machines)
    // Used for counting/indexing
    pub word_count: usize, // Number of words in the post body

    // `bool` is true or false
    pub is_safe: bool, // True if no warnings (safe to publish)

    pub warnings: Vec<String>, // List of issues: ["No date", "Has TODOs"]

    pub published_url: Option<String>, // URL if already published: "https://ejfox.com/blog/..."
    pub published_date: Option<u64>,   // When it was published (timestamp)
    pub source_dir: String,            // Relative path in vault: "blog/2026"

    // Visibility controls for unlisted/password-protected posts
    pub unlisted: bool,           // If true, won't appear in listings
    pub password: Option<String>, // If set, requires password to view

    // Scheduling
    pub publish_at: Option<String>, // ISO 8601 datetime for scheduled publishing

    // Content type: "post" or "weeknote"
    pub content_type: String,
}

// Configuration for where to find things on this computer
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub vault_path: String,         // Path to Obsidian vault
    pub website_repo: String,       // Path to website git repo
    pub excluded_dirs: Vec<String>, // Folders to skip when scanning
}

impl Config {
    /// Build a Config from the new AppConfig system.
    /// This bridges old code that uses Config with the new config module.
    pub fn from_app_config() -> Result<Self, String> {
        let app = config::get()?;
        let target = config::default_target()?;
        Ok(Config {
            vault_path: app.vault.path,
            website_repo: target.repo_path,
            excluded_dirs: app.vault.excluded_dirs,
        })
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
        .append(true) // Add to end instead of overwriting
        .open(&path)
        .map_err(|e| e.to_string())?; // ? = return early if error

    // Add blank line, content, and trailing newline
    writeln!(file).map_err(|e| e.to_string())?;
    write!(file, "{}", content).map_err(|e| e.to_string())?;
    writeln!(file).map_err(|e| e.to_string())?;

    // () is the "unit type" - like void, means "nothing to return"
    Ok(())
}

// Publish a markdown file to the website (copy + git commit + push)
#[tauri::command]
fn publish_file(
    source_path: String,
    slug: String,
    target_id: Option<String>,
) -> Result<String, String> {
    // Check if this is a republish (file already exists in website repo)
    let target = config::resolve_target(target_id.as_deref())?;
    let is_republish = vault::find_published_info_for_target(&target, &slug)
        .0
        .is_some();

    let url = publish::publish_file(&source_path, &slug, target_id.as_deref())?;

    // Record in journal — read source file metadata
    if let Ok(files) = vault::get_recent_files(9999) {
        if let Some(file) = files.iter().find(|f| f.path == source_path) {
            let event = if is_republish { "republish" } else { "publish" };
            let visibility = if file.password.is_some() {
                "protected"
            } else if file.unlisted {
                "unlisted"
            } else {
                "public"
            };
            let _ = journal::record_event(journal::EventRecord {
                event,
                slug: &slug,
                title: file.title.as_deref(),
                word_count: file.word_count,
                tags: &file.tags,
                content_type: &file.content_type,
                url: Some(&url),
                target_id: target_id.as_deref(),
                visibility,
            });
        }
    }

    // Refresh the ambient cache so sketchybar reflects the new state on its
    // next tick (~120s by default).
    std::thread::spawn(sketchybar_cache::update);

    Ok(url)
}

// Unpublish a file (move from blog/ to drafts/ in the website repo)
#[tauri::command]
fn unpublish_file(slug: String, target_id: Option<String>) -> Result<(), String> {
    publish::unpublish_file(&slug, target_id.as_deref())?;

    // Record in journal
    let _ = journal::record_event(journal::EventRecord {
        event: "unpublish",
        slug: &slug,
        title: None,
        word_count: 0,
        tags: &[],
        content_type: "post",
        url: None,
        target_id: target_id.as_deref(),
        visibility: "public",
    });

    std::thread::spawn(sketchybar_cache::update);

    Ok(())
}

// Get the current git status of the website repo
#[tauri::command]
fn get_git_status(target_id: Option<String>) -> publish::GitStatus {
    publish::get_git_status(target_id.as_deref())
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

// Play a built-in macOS system sound by name (Glass, Hero, Pop, Tink, Sosumi, etc).
// Sounds live in /System/Library/Sounds — these are the same beeps every Mac app uses.
#[tauri::command]
fn play_system_sound(name: String) {
    #[cfg(target_os = "macos")]
    {
        let safe: String = name.chars().filter(|c| c.is_ascii_alphanumeric()).collect();
        if safe.is_empty() {
            return;
        }
        let path = format!("/System/Library/Sounds/{}.aiff", safe);
        let _ = std::process::Command::new("/usr/bin/afplay")
            .arg(path)
            .spawn();
    }
}

// Open a file in Obsidian using its URL scheme
#[tauri::command]
fn open_in_obsidian(path: String) -> Result<(), String> {
    open::open_in_obsidian(&path)
}

// Open a file in any macOS app
#[tauri::command]
fn open_in_app(path: String, app: String) -> Result<(), String> {
    open::open_in_app(&path, &app)
}

// Open a terminal and run a command with the file path
#[tauri::command]
fn open_in_terminal(path: String, cmd: String) -> Result<(), String> {
    open::open_in_terminal(&path, &cmd)
}

// --- CONFIG COMMANDS ---

#[tauri::command]
fn get_app_config() -> Result<config::AppConfig, String> {
    config::get()
}

#[tauri::command]
fn save_app_config(config_data: config::AppConfig) -> Result<(), String> {
    config::update(config_data)
}

#[tauri::command]
fn get_config_path() -> String {
    config::get_config_file_path()
}

#[tauri::command]
fn validate_vault_path(path: String) -> bool {
    let p = std::path::Path::new(&path);
    p.exists() && p.is_dir()
}

#[tauri::command]
fn validate_repo_path(path: String) -> bool {
    let p = std::path::Path::new(&path);
    p.exists() && p.is_dir() && p.join(".git").exists()
}

// --- NEW POST CREATION ---

#[tauri::command]
fn create_new_post(title: String) -> Result<String, String> {
    let app_config = config::get()?;
    let vault_path = &app_config.vault.path;

    // Generate slug from title
    let slug: String = title
        .to_lowercase()
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == ' ' {
                c
            } else {
                ' '
            }
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join("-");

    // Use current year
    let year = chrono::Utc::now().format("%Y").to_string();
    let date = chrono::Utc::now().format("%Y-%m-%d").to_string();

    // Create blog/year directory if needed
    let dir = format!("{}/blog/{}", vault_path, year);
    fs::create_dir_all(&dir).map_err(|e| format!("Failed to create directory: {}", e))?;

    let file_path = format!("{}/{}.md", dir, slug);

    // Don't overwrite existing files
    if std::path::Path::new(&file_path).exists() {
        return Err(format!("File already exists: {}", slug));
    }

    let content = format!("---\ndate: {}\n---\n\n# {}\n\n", date, title);

    fs::write(&file_path, content).map_err(|e| format!("Failed to create file: {}", e))?;

    Ok(file_path)
}

// --- CROWN POST COMMANDS ---

#[tauri::command]
fn is_post_crowned(slug: String) -> Result<bool, String> {
    let app_config = config::get()?;
    let target = app_config
        .publish_targets
        .first()
        .ok_or_else(|| "No publish target configured".to_string())?;
    let vue_path = format!("{}/pages/blog/{}.vue", target.repo_path, slug);
    Ok(std::path::Path::new(&vue_path).exists())
}

#[tauri::command]
fn crown_post(slug: String, hue: u32) -> Result<String, String> {
    let app_config = config::get()?;
    let target = app_config
        .publish_targets
        .first()
        .ok_or_else(|| "No publish target configured".to_string())?;

    let vue_path = format!("{}/pages/blog/{}.vue", target.repo_path, slug);

    if std::path::Path::new(&vue_path).exists() {
        return Err(format!("Already crowned: {}", vue_path));
    }

    // Derive names from slug
    let name = slug.split('/').next_back().unwrap_or(&slug);
    let title_case: String = name
        .replace('-', " ")
        .split_whitespace()
        .map(|w| {
            let mut c = w.chars();
            match c.next() {
                None => String::new(),
                Some(f) => f.to_uppercase().to_string() + c.as_str(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ");
    let body_class = format!("{}-takeover", name);
    let css_class = format!("{}-page", name);

    // Load template at compile time and interpolate placeholders
    let template = include_str!("../templates/crowned-post.vue")
        .replace("__SLUG__", &slug)
        .replace("__HUE__", &hue.to_string())
        .replace("__TITLE__", &title_case)
        .replace("__BODY_CLASS__", &body_class)
        .replace("__CSS_CLASS__", &css_class);

    // Create directory and write file
    let parent = std::path::Path::new(&vue_path)
        .parent()
        .ok_or_else(|| "Invalid path".to_string())?;
    fs::create_dir_all(parent).map_err(|e| format!("Failed to create directory: {}", e))?;
    fs::write(&vue_path, template).map_err(|e| format!("Failed to write crowned post: {}", e))?;

    Ok(vue_path)
}

// --- COMPANION COMMANDS ---

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompanionInfo {
    pub pin: String,
    pub url: Option<String>,
    pub port: u16,
}

#[tauri::command]
fn get_companion_info() -> CompanionInfo {
    CompanionInfo {
        pin: companion::get_pin(),
        url: companion::get_local_url(),
        port: 6420,
    }
}

// --- ANALYTICS COMMANDS ---

#[tauri::command]
async fn check_analytics_status() -> bool {
    analytics::check_connection().await
}

#[tauri::command]
async fn get_post_analytics(
    url: String,
    days: Option<u32>,
) -> Result<analytics::PostStats, String> {
    analytics::get_post_stats(&url, days.unwrap_or(30)).await
}

#[tauri::command]
async fn get_site_analytics(days: Option<u32>) -> Result<analytics::PostStats, String> {
    analytics::get_site_stats(days.unwrap_or(30)).await
}

#[tauri::command]
async fn get_top_posts(
    days: Option<u32>,
    limit: Option<usize>,
) -> Result<Vec<analytics::TopPost>, String> {
    analytics::get_top_posts(days.unwrap_or(7), limit.unwrap_or(10)).await
}

// --- SCHEDULING COMMANDS ---

#[tauri::command]
fn schedule_publish(path: String, publish_at: String) -> Result<(), String> {
    vault::set_frontmatter_field(&path, "publish_at", &publish_at)
}

#[tauri::command]
fn cancel_schedule(path: String) -> Result<(), String> {
    vault::remove_frontmatter_field(&path, "publish_at")
}

#[tauri::command]
fn set_frontmatter(path: String, key: String, value: String) -> Result<(), String> {
    vault::set_frontmatter_field(&path, &key, &value)
}

// --- WEBMENTION COMMANDS ---

#[tauri::command]
async fn send_webmentions(
    post_url: String,
    bridgy_fed: Option<bool>,
    target_id: Option<String>,
) -> Result<webmention::WebmentionReport, String> {
    let target = config::resolve_target(target_id.as_deref())?;
    let domain = target.domain.replace("https://", "").replace("http://", "");
    let bridgy = bridgy_fed.unwrap_or(false);

    // Run in blocking thread since we use reqwest::blocking
    tokio::task::spawn_blocking(move || {
        webmention::send_webmentions_for_post(&post_url, &domain, bridgy)
    })
    .await
    .map_err(|e| format!("Task failed: {}", e))
}

// --- JOURNAL COMMANDS ---

#[tauri::command]
fn get_journal_stats() -> Result<journal::JournalStats, String> {
    journal::get_stats()
}

#[tauri::command]
fn get_journal_entries(limit: Option<usize>) -> Result<Vec<journal::JournalEntry>, String> {
    journal::get_recent_entries(limit.unwrap_or(50))
}

#[tauri::command]
fn get_journal_nudge() -> Result<Option<journal::Nudge>, String> {
    journal::get_nudge()
}

#[tauri::command]
fn get_journal_heatmap(days: Option<u32>) -> Result<Vec<(String, u32)>, String> {
    journal::get_heatmap(days.unwrap_or(91)) // ~13 weeks
}

#[tauri::command]
fn backfill_journal() -> Result<u32, String> {
    let target = config::default_target()?;
    journal::backfill_from_git(&target.repo_path, &target.domain)
}

// --- VAULT PULSE ---

#[tauri::command]
fn get_vault_pulse() -> Result<vault_pulse::VaultPulse, String> {
    vault_pulse::scan_vault()
}

// Tell the preview server which file to display
#[tauri::command]
fn set_preview_file(path: String) {
    preview::set_file(&path);
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
    folder: Option<String>, // Optional folder to organize uploads
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
    resource_type: Option<String>, // "image" or "video"
    max_results: Option<u32>,      // How many to return
    cursor: Option<String>,        // Pagination cursor for "load more"
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
    _source_path: String, // _ prefix = unused parameter (kept for API compatibility)
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
                continue; // Skip to next item
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
    fixes: Vec<(String, String)>, // Vec of (old_text, new_text) tuples
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
    tokio::task::spawn_blocking(asset_usage::scan_vault_for_usage)
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

// Generate alt text suggestions for images missing alt text in a file
#[tauri::command]
async fn generate_alt_text(file_path: String) -> Result<alttext::AltTextResult, String> {
    alttext::generate_suggestions(&file_path).await
}

// Apply generated alt text suggestions to a markdown file
#[tauri::command]
fn apply_alt_text(
    file_path: String,
    suggestions: Vec<alttext::AltTextSuggestion>,
) -> Result<usize, String> {
    alttext::apply_suggestions(&file_path, &suggestions)
}

// Syndicate a published post to social platforms (Mastodon, etc.)
#[tauri::command]
fn syndicate_post(
    post_url: String,
    title: String,
    slug: String,
    tags: Vec<String>,
    dek: Option<String>,
    content_type: String,
    visibility: String,
) -> Vec<syndication::SyndicationResult> {
    let post = syndication::PostContent {
        title,
        url: post_url,
        slug,
        tags,
        dek,
        content_type,
        visibility,
    };
    syndication::syndicate_post(&post)
}

// Verify Mastodon connection
#[tauri::command]
fn verify_mastodon() -> Result<String, String> {
    syndication::verify_mastodon()
}

// Queue posts for syndication (from the wizard)
#[tauri::command]
fn queue_syndication(items: Vec<syndication_queue::NewQueueItem>) -> Result<Vec<i64>, String> {
    syndication_queue::queue_items(&items)
}

// Get syndication queue items
#[tauri::command]
fn get_syndication_queue(
    status: Option<String>,
    limit: Option<usize>,
) -> Result<Vec<syndication_queue::QueueItem>, String> {
    syndication_queue::get_queue(status.as_deref(), limit.unwrap_or(50))
}

// Get syndication queue for a specific post
#[tauri::command]
fn get_post_syndication(slug: String) -> Result<Vec<syndication_queue::QueueItem>, String> {
    syndication_queue::get_queue_for_post(&slug)
}

// Update a queue item (text, schedule, media)
#[tauri::command]
fn update_syndication_item(
    id: i64,
    platform_text: Option<String>,
    scheduled_at: Option<String>,
    media_url: Option<String>,
) -> Result<(), String> {
    syndication_queue::update_item(
        id,
        platform_text.as_deref(),
        scheduled_at.as_deref(),
        media_url.as_deref(),
    )
}

// Delete a queue item
#[tauri::command]
fn delete_syndication_item(id: i64) -> Result<(), String> {
    syndication_queue::delete_item(id)
}

// Send a queue item immediately
#[tauri::command]
fn send_syndication_now(id: i64) -> Result<syndication::SyndicationResult, String> {
    syndication_queue::send_item(id)
}

// Generate a promo card image URL via Cloudinary text overlays (legacy)
#[tauri::command]
fn generate_promo_image(title: String, url: String) -> Result<String, String> {
    syndication::generate_promo_image_url(&title, &url)
}

// Generate 4 OG image variants for a post
#[tauri::command]
fn generate_og_variants(
    slug: String,
    batch: Option<u32>,
) -> Result<syndication::OgImageVariants, String> {
    syndication::generate_og_variants(&slug, batch.unwrap_or(0))
}

// Upload a picked OG image variant to Cloudinary
#[tauri::command]
fn upload_og_image(file_path: String, slug: String) -> Result<String, String> {
    syndication::upload_og_image(&file_path, &slug)
}

// List all folders in Cloudinary (for folder picker UI)
#[tauri::command]
async fn cloudinary_list_folders() -> Result<Vec<String>, String> {
    cloudinary::list_folders().await
}

// Open or focus the preview window
#[tauri::command]
async fn open_preview(app_handle: tauri::AppHandle) -> Result<String, String> {
    use tauri::WebviewUrl;
    use tauri::WebviewWindowBuilder;

    // Check if preview window already exists
    if let Some(window) = app_handle.get_webview_window("preview") {
        let _ = window.show();
        let _ = window.set_focus();
        return Ok("http://127.0.0.1:6419".into());
    }

    // Create new preview window pointing to the preview server
    let preview_url: tauri::Url = "http://127.0.0.1:6419"
        .parse()
        .map_err(|e| format!("Invalid preview URL: {}", e))?;
    let window =
        WebviewWindowBuilder::new(&app_handle, "preview", WebviewUrl::External(preview_url))
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

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // --- STDIO PANIC GUARD ---
    // When Dispatch runs from a .app bundle, stdout/stderr inherit from the
    // parent process — usually a terminal. If that parent dies (e.g. user
    // closes the terminal they launched from), our `eprintln!` calls EPIPE,
    // and Rust's stdio macros panic on write failure. Installing this hook
    // BEFORE any stderr write swallows those specific stdio panics so they
    // can't take down the app.
    let prev_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let payload = info.payload();
        let msg = payload
            .downcast_ref::<String>()
            .map(String::as_str)
            .or_else(|| payload.downcast_ref::<&str>().copied())
            .unwrap_or("");
        if msg.contains("failed printing to stdout") || msg.contains("failed printing to stderr") {
            return;
        }
        prev_hook(info);
    }));

    // --- RESOLVE EXTERNAL BINARIES ---
    // Run before anything that shells out (preview server, git, etc.)
    bin_paths::warm();

    // --- LOAD ENVIRONMENT VARIABLES ---
    let _ = dotenvy::dotenv();
    let _ = dotenvy::from_filename(concat!(env!("CARGO_MANIFEST_DIR"), "/../.env"));
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let _ = dotenvy::from_filename(dir.join(".env"));
            let _ = dotenvy::from_filename(dir.join("../Resources/.env"));
        }
    }

    // --- START PREVIEW SERVER ---
    preview::init_server();

    // --- WARM SKETCHYBAR CACHE ---
    // Off-thread so a slow first-vault-scan doesn't block app launch.
    std::thread::spawn(sketchybar_cache::update);

    // --- BUILD AND RUN THE APP ---
    tauri::Builder::default()
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .plugin(
            tauri_plugin_log::Builder::new()
                .level(log::LevelFilter::Info)
                .targets([
                    tauri_plugin_log::Target::new(tauri_plugin_log::TargetKind::Stdout),
                    tauri_plugin_log::Target::new(tauri_plugin_log::TargetKind::LogDir {
                        file_name: Some("dispatch".into()),
                    }),
                ])
                .build(),
        )
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_window_state::Builder::default().build())
        // --- APPLICATION MENU BAR ---
        // macOS apps need a proper menu bar for Cmd+C/V/Z to work in text fields.
        .menu(menu::build_app_menu)
        // Handle custom menu item clicks — emit events to the frontend
        .on_menu_event(menu::handle_menu_event)
        .setup(|app| {
            // Initialize config before anything else
            config::init();

            // Start schedule checker background task
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                publish::run_schedule_checker(handle).await;
            });

            // Start syndication scheduler background task
            let syndication_handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                syndication_queue::run_syndication_scheduler(syndication_handle).await;
            });

            // Start companion web server
            tauri::async_runtime::spawn(async {
                companion::start_server().await;
            });

            // Build tray icon with menu
            menu::build_tray(app)?;

            // --- VAULT FILE WATCHER ---
            // Auto-emits `vault-changed` to the frontend on .md file
            // creation/modification/deletion so the file list refreshes
            // without a manual ⌘R.
            let vault_path = config::get().ok().map(|c| c.vault.path).unwrap_or_default();
            vault_watcher::start(app.handle().clone(), vault_path);

            // --- HIDE WINDOW ON CLOSE ---
            // macOS convention: tray apps hide the window on Cmd+W / red close button
            // instead of quitting. The user quits via tray menu or Cmd+Q.
            if let Some(window) = app.get_webview_window("main") {
                let window_clone = window.clone();
                window.on_window_event(move |event| {
                    if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                        api.prevent_close();
                        let _ = window_clone.hide();
                    }
                });
            }

            Ok(())
        })
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
            open_in_terminal,
            set_preview_file,
            open_preview,
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
            scan_asset_usage,
            get_asset_usage,
            get_post_assets,
            schedule_publish,
            cancel_schedule,
            set_frontmatter,
            send_webmentions,
            get_journal_stats,
            get_journal_entries,
            get_journal_nudge,
            backfill_journal,
            get_journal_heatmap,
            get_vault_pulse,
            check_analytics_status,
            get_post_analytics,
            get_site_analytics,
            get_top_posts,
            get_companion_info,
            get_app_config,
            save_app_config,
            get_config_path,
            validate_vault_path,
            validate_repo_path,
            create_new_post,
            crown_post,
            is_post_crowned,
            generate_alt_text,
            apply_alt_text,
            syndicate_post,
            verify_mastodon,
            queue_syndication,
            get_syndication_queue,
            get_post_syndication,
            update_syndication_item,
            delete_syndication_item,
            send_syndication_now,
            generate_promo_image,
            generate_og_variants,
            upload_og_image,
            gear::list_gear,
            gear::mark_gear_used,
            gear::update_gear_location,
            gear::set_gear_scan_url,
            gear::gear_pending_changes,
            gear::commit_gear_changes,
            play_system_sound,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
