// --- IMPORTS ---
// `serde` is a library for converting Rust structs to/from JSON.
// Deserialize = JSON -> Rust struct, Serialize = Rust struct -> JSON
use serde::{Deserialize, Serialize};

// `std::fs` is Rust's standard library for file system operations (read, write, etc.)
use std::fs;

// Tauri-specific imports for building desktop apps
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::Manager;

// --- MODULE DECLARATIONS ---
// `mod X` tells Rust "there's a file called X.rs in this folder - include it"
// This is how Rust organizes code into separate files.
// Each module becomes accessible as `module_name::function_name()`
mod alttext; // AI-powered alt text generation for images
mod analytics; // Umami analytics integration
mod asset_usage; // Tracks which Cloudinary images are used in which posts
mod cloudinary; // Uploads images/videos to Cloudinary CDN
mod companion; // Companion web UI server for mobile access
pub mod config; // App configuration (vault path, publish targets, editors)
mod obsidian; // Talks to Obsidian's Local REST API for backlinks
mod preview; // Manages a local Node.js server for previewing posts
mod publish; // Handles git operations to publish posts to your website
mod syndication; // Post-publish social distribution (Mastodon, etc.)
mod syndication_queue; // Scheduled syndication queue with background sender
mod vault; // Scans your Obsidian vault for markdown files
mod webmention; // IndieWeb webmention sending
mod journal; // Publishing journal, streaks, milestones
mod vault_pulse; // Read-only vault intelligence (never publishes)

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
    pub fn from_app_config() -> Self {
        let app = config::get();
        let target = config::default_target();
        Config {
            vault_path: app.vault.path,
            website_repo: target.repo_path,
            excluded_dirs: app.vault.excluded_dirs,
        }
    }
}

// `impl` adds methods to a struct (like class methods in other languages)
// `Default` is a trait (interface) that provides a default() method
impl Default for Config {
    fn default() -> Self {
        Config::from_app_config()
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
    let is_republish = vault::find_published_info_for_target(&target, &slug).0.is_some();

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
            let _ = journal::record_event(
                event,
                &slug,
                file.title.as_deref(),
                file.word_count,
                &file.tags,
                &file.content_type,
                Some(&url),
                target_id.as_deref(),
                visibility,
            );
        }
    }

    Ok(url)
}

// Unpublish a file (move from blog/ to drafts/ in the website repo)
#[tauri::command]
fn unpublish_file(slug: String, target_id: Option<String>) -> Result<(), String> {
    publish::unpublish_file(&slug, target_id.as_deref())?;

    // Record in journal
    let _ = journal::record_event(
        "unpublish",
        &slug,
        None,
        0,
        &[],
        "post",
        None,
        target_id.as_deref(),
        "public",
    );

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

// Open a file in Obsidian using its URL scheme (obsidian://open?vault=...&file=...)
#[tauri::command]
fn open_in_obsidian(path: String) -> Result<(), String> {
    let app_config = config::get();

    // Get path relative to vault root by stripping the vault_path prefix
    let relative_path = path
        .strip_prefix(&app_config.vault.path)
        .unwrap_or(&path)
        .trim_start_matches('/');

    // Build the Obsidian URL scheme using configured vault name
    let url = format!(
        "obsidian://open?vault={}&file={}",
        app_config.vault.name,
        urlencoding::encode(relative_path)
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

    if matches!(result, Ok(ref output) if output.status.success()) {
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

// --- CONFIG COMMANDS ---

#[tauri::command]
fn get_app_config() -> config::AppConfig {
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
    let app_config = config::get();
    let vault_path = &app_config.vault.path;

    // Generate slug from title
    let slug: String = title
        .to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() || c == ' ' { c } else { ' ' })
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

    let content = format!(
        "---\ndate: {}\n---\n\n# {}\n\n",
        date, title
    );

    fs::write(&file_path, content).map_err(|e| format!("Failed to create file: {}", e))?;

    Ok(file_path)
}

// --- CROWN POST COMMANDS ---

#[tauri::command]
fn is_post_crowned(slug: String) -> Result<bool, String> {
    let app_config = config::get();
    let target = app_config.publish_targets.first()
        .ok_or_else(|| "No publish target configured".to_string())?;
    let vue_path = format!("{}/pages/blog/{}.vue", target.repo_path, slug);
    Ok(std::path::Path::new(&vue_path).exists())
}

#[tauri::command]
fn crown_post(slug: String, hue: u32) -> Result<String, String> {
    let app_config = config::get();
    let target = app_config.publish_targets.first()
        .ok_or_else(|| "No publish target configured".to_string())?;

    let vue_path = format!("{}/pages/blog/{}.vue", target.repo_path, slug);

    if std::path::Path::new(&vue_path).exists() {
        return Err(format!("Already crowned: {}", vue_path));
    }

    // Derive names from slug
    let name = slug.split('/').last().unwrap_or(&slug);
    let title_case: String = name.replace('-', " ")
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

    // Build the Vue SFC template
    let template = format!(r##"<script setup>
/**
 * CROWNED POST: {title}
 * Hue: {hue} | Scaffolded from Dispatch
 */

import {{ useIntersectionObserver }} from '@vueuse/core'
import PostMetadataBar from '~/components/blog/post/PostMetadataBar.vue'
import PostNav from '~/components/blog/post/PostNav.vue'
import PostRelated from '~/components/blog/post/PostRelated.vue'
import Webmentions from '~/components/blog/Webmentions.vue'

const {{
  post, nextPrevPosts, relatedPosts, readingStats,
  postTitle, postUrl, articleTags,
  renderedTitle, startAnimation,
  palette,
}} = await useCrownedPost({{
  slug: '{slug}',
  bodyClass: '{body_class}',
  hue: {hue},
  fallbackTitle: '{title}',
}})

const articleContent = ref(null)
const scrollProgress = ref(0)

onMounted(() => {{
  startAnimation()

  const handleScroll = () => {{
    const scrollHeight = document.documentElement.scrollHeight - window.innerHeight
    scrollProgress.value = scrollHeight <= 0 ? 0 : Math.min((window.scrollY / scrollHeight) * 100, 100)
  }}
  window.addEventListener('scroll', handleScroll)
  onUnmounted(() => window.removeEventListener('scroll', handleScroll))

  nextTick(() => {{
    if (!articleContent.value) return
    const elements = articleContent.value.querySelectorAll('h2, h3, h4, p, blockquote, pre, img, figure, ul, ol')

    import('animejs').then(({{ animate }}) => {{
      const seen = new WeakSet()
      elements.forEach((el, idx) => {{
        if (el.getBoundingClientRect().top < window.innerHeight) return
        const tag = el.tagName.toLowerCase()
        const isHeading = ['h2', 'h3', 'h4'].includes(tag)
        const isMedia = ['img', 'figure'].includes(tag)
        const isQuote = tag === 'blockquote'

        el.style.opacity = '0'
        if (isHeading) el.style.transform = 'translateX(-20px)'
        else if (isMedia) {{ el.style.transform = 'scale(0.97)'; el.style.filter = 'blur(4px)' }}
        else if (isQuote) el.style.transform = 'translateX(20px)'
        else el.style.transform = 'translateY(12px)'

        const obs = new IntersectionObserver(([entry]) => {{
          if (!entry.isIntersecting || seen.has(el)) return
          seen.add(el)
          obs.disconnect()
          const cleanup = () => {{ el.style.opacity = ''; el.style.transform = ''; el.style.filter = '' }}

          if (isHeading) animate(el, {{ opacity: [0, 1], translateX: [-20, 0], duration: 500, ease: 'outCubic', onComplete: cleanup }})
          else if (isMedia) animate(el, {{ opacity: [0, 1], scale: [0.97, 1], filter: ['blur(4px)', 'blur(0px)'], duration: 600, ease: 'outQuad', onComplete: cleanup }})
          else if (isQuote) animate(el, {{ opacity: [0, 1], translateX: [20, 0], duration: 500, ease: 'outCubic', onComplete: cleanup }})
          else animate(el, {{ opacity: [0, 1], translateY: [12, 0], duration: 400, delay: Math.min((idx % 3) * 60, 120), ease: 'outQuad', onComplete: cleanup }})
        }}, {{ threshold: 0.05, rootMargin: '0px 0px -10% 0px' }})
        obs.observe(el)
      }})
    }})
  }})
}})
</script>

<template>
  <div class="{css_class}">
    <div class="crowned-progress">
      <div class="crowned-progress-inner" :style="{{ width: scrollProgress + '%' }}" />
    </div>

    <div v-if="post" class="fixed top-0 left-0 right-0 z-[100] bg-black/80 backdrop-blur-sm print:hidden">
      <PostMetadataBar :date="post?.metadata?.date || post?.date" :stats="readingStats" />
    </div>

    <article v-if="post" class="h-entry">
      <header class="crowned-hero">
        <h1 class="crowned-title" v-html="renderedTitle" />
        <p v-if="post?.metadata?.dek || post?.dek" class="crowned-dek">
          {{{{ post?.metadata?.dek || post?.dek }}}}
        </p>
        <WidgetsScrollIndicator :color="palette.accentDim" />
      </header>

      <time v-if="post?.metadata?.date" :datetime="post.metadata.date" class="dt-published hidden">{{{{ post.metadata.date }}}}</time>
      <div class="p-author h-card hidden"><span class="p-name">EJ Fox</span><a class="u-url" href="https://ejfox.com">ejfox.com</a></div>
      <a :href="postUrl" class="u-url hidden">{{{{ postUrl }}}}</a>

      <div ref="articleContent" class="crowned-body">
        <div v-if="post?.html" class="blog-post-content e-content font-serif" v-html="post.html" />
      </div>

      <div v-if="articleTags.length" class="crowned-tags">
        <a v-for="tag in articleTags" :key="tag" :href="`/blog/tag/${{tag}}`" class="crowned-tag">{{{{ tag }}}}</a>
      </div>

      <PostNav class="print:hidden" :prev-post="nextPrevPosts?.prev" :next-post="nextPrevPosts?.next" />
      <PostRelated class="print:hidden" :related-posts="relatedPosts" />
      <Webmentions class="print:hidden" :url="postUrl" />
    </article>
  </div>
</template>

<style lang="postcss">
body.{body_class} {{ background: var(--pt-bg) !important; }}
body.{body_class} #app-container,
body.{body_class} #app-container > section {{ background: var(--pt-bg) !important; }}
body.{body_class} nav.sticky,
body.{body_class} nav.md\:hidden {{ display: none !important; }}
body.{body_class} #main-content {{ width: 100% !important; padding-top: 0 !important; }}
body.{body_class} footer {{ background: var(--pt-bg) !important; border-color: transparent !important; }}
body.{body_class} .blog-post-content blockquote::before,
body.{body_class} blockquote::before {{ content: none !important; display: none !important; }}

.{css_class} {{
  position: relative; min-height: 100vh;
  background: var(--pt-bg); color: var(--pt-text-dim);
}}

.crowned-progress {{ position: fixed; top: 0; left: 0; right: 0; height: 1px; z-index: 101; }}
.crowned-progress-inner {{ height: 100%; background: linear-gradient(90deg, var(--pt-accent-dim), var(--pt-accent-glow)); transition: width 75ms ease-out; }}

.crowned-hero {{
  position: relative; z-index: 1; max-width: 900px; margin: 0 auto;
  padding: 8rem 2rem 6rem; min-height: 70vh;
  display: flex; flex-direction: column; justify-content: center;
}}
@media (min-width: 768px) {{ .crowned-hero {{ padding: 10rem 2rem 8rem; min-height: 80vh; }} }}

.crowned-title {{
  font-size: clamp(2.5rem, 8vw, 5.5rem); font-weight: 900; line-height: 1.05;
  letter-spacing: -0.03em; color: var(--pt-text); overflow-wrap: break-word;
}}
.crowned-title .typing-char {{ display: inline; opacity: 0; transition: opacity 0.06s ease-out; }}
.crowned-title .typing-char.typed {{ opacity: 1; }}
.crowned-title .cursor {{
  display: inline-block; width: 3px; height: 0.85em; margin-left: 1px;
  background: var(--pt-accent); animation: crowned-blink 0.5s ease-in-out infinite; vertical-align: baseline;
}}
@keyframes crowned-blink {{ 0%, 40% {{ opacity: 0.85; }} 50%, 90% {{ opacity: 0; }} 100% {{ opacity: 0.85; }} }}

.crowned-dek {{
  margin-top: 1.5rem; font-family: Georgia, serif; font-size: 1.125rem;
  line-height: 1.6; color: var(--pt-text-muted); max-width: 42em; font-style: italic;
}}

.crowned-body {{ max-width: 900px; margin: 0 auto; padding: 0 2rem 4rem; }}
.{css_class} .blog-post-content {{ --body-margin: 2rem; }}

.{css_class} .blog-post-content p,
.{css_class} .blog-post-content ul,
.{css_class} .blog-post-content ol,
.{css_class} .blog-post-content li {{ color: var(--pt-text-dim); }}

.{css_class} .blog-post-content h1,
.{css_class} .blog-post-content h2 {{ color: var(--pt-text); border: none !important; }}
.{css_class} .blog-post-content h3,
.{css_class} .blog-post-content h4 {{ color: var(--pt-text-dim); }}

.{css_class} .blog-post-content a {{ color: var(--pt-accent); text-decoration-color: var(--pt-accent-faint); }}
.{css_class} .blog-post-content a:hover {{ color: var(--pt-accent-glow); text-shadow: 0 0 12px var(--pt-accent-faint); }}

.{css_class} .blog-post-content blockquote {{
  border-left: 2px solid var(--pt-accent-faint) !important;
  background: transparent !important; padding: 0.5rem 0 0.5rem 1.5rem !important;
  color: var(--pt-text-dim);
}}

.{css_class} .blog-post-content code {{ background: var(--pt-surface); color: var(--pt-accent-glow); border: 1px solid var(--pt-accent-faint); }}
.{css_class} .blog-post-content pre {{ background: var(--pt-surface); border: none; }}
.{css_class} .blog-post-content pre code {{ background: transparent; border: none; }}
.{css_class} .blog-post-content img {{ border-radius: 6px; }}
.{css_class} .blog-post-content hr {{ border: none; height: 1px; margin: 4rem 0; background: linear-gradient(90deg, transparent, var(--pt-accent-dim), transparent); }}

.crowned-tags {{ max-width: 900px; margin: 0 auto; padding: 1rem 2rem 2rem; display: flex; flex-wrap: wrap; gap: 0.5rem; }}
.crowned-tag {{
  font-family: ui-monospace, monospace; font-size: 0.6875rem;
  padding: 0.25rem 0.75rem; border-radius: 999px;
  border: 1px solid var(--pt-accent-faint); color: var(--pt-text-muted); text-decoration: none; transition: all 0.15s;
}}
.crowned-tag:hover {{ border-color: var(--pt-accent-dim); color: var(--pt-text); }}

@media (prefers-reduced-motion: reduce) {{
  .crowned-title .typing-char {{ opacity: 1 !important; transition: none; }}
  .crowned-title .cursor {{ display: none; }}
}}

@media print {{
  .crowned-progress {{ display: none !important; }}
  .{css_class} {{ background: white !important; color: black !important; }}
}}
</style>
"##, title = title_case, hue = hue, slug = slug, body_class = body_class, css_class = css_class);

    // Create directory and write file
    let parent = std::path::Path::new(&vue_path).parent()
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
    let target = config::default_target();
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
fn generate_og_variants(slug: String, batch: Option<u32>) -> Result<syndication::OgImageVariants, String> {
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

    // --- BUILD AND RUN THE APP ---
    tauri::Builder::default()
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_window_state::Builder::default().build())
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

            // Build tray menu
            let open_i = MenuItem::with_id(app, "open", "Open Dispatch", true, None::<&str>)?;
            let quit_i = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&open_i, &quit_i])?;

            TrayIconBuilder::new()
                .icon(app.default_window_icon().unwrap().clone())
                .menu(&menu)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "open" => {
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                    }
                    "quit" => {
                        preview::stop_server();
                        app.exit(0);
                    }
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        let app = tray.app_handle();
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                    }
                })
                .build(app)?;

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
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
