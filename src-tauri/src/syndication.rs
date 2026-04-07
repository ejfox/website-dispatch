//! Syndication module — post-publish distribution to social platforms.
//!
//! Currently supports: Mastodon
//! Designed for: Instagram, LinkedIn, Bluesky, custom webhooks
//!
//! Each platform implements the same pattern:
//!   1. Config validation (instance URL + token from .env)
//!   2. Post composition (title, URL, optional media)
//!   3. API call
//!   4. Return syndication URL (the post on the platform)

use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION, CONTENT_TYPE};
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Shared types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyndicationResult {
    pub platform: String,
    pub success: bool,
    pub url: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PostContent {
    pub title: String,
    pub url: String,
    pub slug: String,
    pub tags: Vec<String>,
    pub dek: Option<String>,
    pub content_type: String, // "post" or "weeknote"
    pub visibility: String,   // "public", "unlisted", "protected"
}

// ---------------------------------------------------------------------------
// Mastodon
// ---------------------------------------------------------------------------

fn get_mastodon_config() -> Result<(String, String), String> {
    let instance = std::env::var("MASTODON_INSTANCE")
        .unwrap_or_else(|_| "mastodon.social".to_string());
    let token = std::env::var("MASTODON_ACCESS_TOKEN")
        .map_err(|_| "MASTODON_ACCESS_TOKEN not set in .env".to_string())?;
    Ok((instance, token))
}

/// Compose a toot from post content.
/// Keeps it concise: title + URL + hashtags.
fn compose_toot(post: &PostContent) -> String {
    let mut parts = Vec::new();

    // Title or dek
    if let Some(ref dek) = post.dek {
        parts.push(dek.clone());
    } else {
        parts.push(post.title.clone());
    }

    // URL
    parts.push(post.url.clone());

    // Hashtags from tags (max 3, skip generic ones)
    let skip_tags = ["post", "weeknote", "blog"];
    let hashtags: Vec<String> = post
        .tags
        .iter()
        .filter(|t| !skip_tags.contains(&t.as_str()))
        .take(3)
        .map(|t| format!("#{}", t.replace('-', "")))
        .collect();
    if !hashtags.is_empty() {
        parts.push(hashtags.join(" "));
    }

    parts.join("\n\n")
}

/// Post to Mastodon. Returns the toot URL on success.
pub fn post_to_mastodon(post: &PostContent) -> SyndicationResult {
    let (instance, token) = match get_mastodon_config() {
        Ok(c) => c,
        Err(e) => {
            return SyndicationResult {
                platform: "mastodon".into(),
                success: false,
                url: None,
                error: Some(e),
            }
        }
    };

    let status_text = compose_toot(post);
    let url = format!("https://{}/api/v1/statuses", instance);

    // Mastodon visibility mapping
    let masto_visibility = match post.visibility.as_str() {
        "unlisted" | "protected" => "unlisted",
        _ => "public",
    };

    let body = serde_json::json!({
        "status": status_text,
        "visibility": masto_visibility,
    });

    let client = reqwest::blocking::Client::new();
    let mut headers = HeaderMap::new();
    headers.insert(
        AUTHORIZATION,
        HeaderValue::from_str(&format!("Bearer {}", token)).unwrap(),
    );
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));

    match client.post(&url).headers(headers).json(&body).send() {
        Ok(resp) => {
            if resp.status().is_success() {
                let data: serde_json::Value = resp.json().unwrap_or_default();
                let toot_url = data["url"].as_str().map(|s| s.to_string());
                SyndicationResult {
                    platform: "mastodon".into(),
                    success: true,
                    url: toot_url,
                    error: None,
                }
            } else {
                let status = resp.status();
                let text = resp.text().unwrap_or_default();
                SyndicationResult {
                    platform: "mastodon".into(),
                    success: false,
                    url: None,
                    error: Some(format!("Mastodon {} — {}", status, text)),
                }
            }
        }
        Err(e) => SyndicationResult {
            platform: "mastodon".into(),
            success: false,
            url: None,
            error: Some(format!("Request failed: {}", e)),
        },
    }
}

/// Post to Mastodon with pre-composed text (from the queue, user-edited).
pub fn post_to_mastodon_with_text(status_text: &str) -> SyndicationResult {
    let (instance, token) = match get_mastodon_config() {
        Ok(c) => c,
        Err(e) => {
            return SyndicationResult {
                platform: "mastodon".into(),
                success: false,
                url: None,
                error: Some(e),
            }
        }
    };

    let url = format!("https://{}/api/v1/statuses", instance);
    let body = serde_json::json!({
        "status": status_text,
        "visibility": "public",
    });

    let client = reqwest::blocking::Client::new();
    let mut headers = HeaderMap::new();
    headers.insert(
        AUTHORIZATION,
        HeaderValue::from_str(&format!("Bearer {}", token)).unwrap(),
    );
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));

    match client.post(&url).headers(headers).json(&body).send() {
        Ok(resp) => {
            if resp.status().is_success() {
                let data: serde_json::Value = resp.json().unwrap_or_default();
                let toot_url = data["url"].as_str().map(|s| s.to_string());
                SyndicationResult {
                    platform: "mastodon".into(),
                    success: true,
                    url: toot_url,
                    error: None,
                }
            } else {
                let status = resp.status();
                let text = resp.text().unwrap_or_default();
                SyndicationResult {
                    platform: "mastodon".into(),
                    success: false,
                    url: None,
                    error: Some(format!("Mastodon {} — {}", status, text)),
                }
            }
        }
        Err(e) => SyndicationResult {
            platform: "mastodon".into(),
            success: false,
            url: None,
            error: Some(format!("Request failed: {}", e)),
        },
    }
}

/// Validate that the Mastodon config works (verifies token).
pub fn verify_mastodon() -> Result<String, String> {
    let (instance, token) = get_mastodon_config()?;
    let url = format!("https://{}/api/v1/accounts/verify_credentials", instance);

    let client = reqwest::blocking::Client::new();
    let resp = client
        .get(&url)
        .header(AUTHORIZATION, format!("Bearer {}", token))
        .send()
        .map_err(|e| format!("Connection failed: {}", e))?;

    if !resp.status().is_success() {
        return Err(format!("Auth failed: {}", resp.status()));
    }

    let data: serde_json::Value = resp.json().unwrap_or_default();
    let username = data["acct"].as_str().unwrap_or("unknown");
    let display = data["display_name"].as_str().unwrap_or("");
    Ok(format!("{}@{} ({})", username, instance, display))
}

// ---------------------------------------------------------------------------
// Promo image generation via Cloudinary text overlays
// ---------------------------------------------------------------------------

/// Generate a promo card image URL using Cloudinary text overlays.
/// Returns a Cloudinary URL that renders a 1200x630 card with title + URL.
/// No upload needed — Cloudinary generates it on the fly from URL params.
pub fn generate_promo_image_url(title: &str, url: &str) -> Result<String, String> {
    let config = crate::cloudinary::get_config()?;

    // URL-encode the title for Cloudinary text overlay
    let safe_title = title
        .replace('%', "%25")
        .replace('/', "%2F")
        .replace('#', "%23")
        .replace('?', "%3F")
        .replace('&', "%26")
        .replace(',', "%252C")
        .replace(' ', "%20");

    let safe_url = url
        .replace('%', "%25")
        .replace('/', "%2F")
        .replace('#', "%23")
        .replace(':', "%3A");

    // Cloudinary text overlay URL — generates a 1200x630 OG card
    // Dark zinc background, white Georgia title, gray monospace URL
    Ok(format!(
        "https://res.cloudinary.com/{cloud}/image/upload/\
         w_1200,h_630,c_fill,b_rgb:18181b/\
         l_text:Georgia_44_bold_line_spacing_8:{title},co_rgb:e4e4e7,g_north_west,x_60,y_80,w_1000,c_fit/\
         l_text:courier_20:{url},co_rgb:71717a,g_south_west,x_60,y_60,w_1000/\
         v1/blank_og.png",
        cloud = config.cloud_name,
        title = safe_title,
        url = safe_url,
    ))
}

// ---------------------------------------------------------------------------
// OG Image generation — calls the Node script in the website repo
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OgImageVariants {
    pub slug: String,
    pub paths: Vec<String>,
    pub preview_html: String,
}

/// Generate 4 OG image variants by calling the Node script in the website repo.
/// Returns file paths to the generated PNGs.
pub fn generate_og_variants(slug: &str, batch: u32) -> Result<OgImageVariants, String> {
    let config = crate::config::get();
    let target = crate::config::default_target();
    let repo_path = &target.repo_path;
    let script = format!("{}/scripts/og-image/index.mjs", repo_path);

    // Check script exists
    if !std::path::Path::new(&script).exists() {
        return Err(format!("OG script not found at {}", script));
    }

    // The batch number offsets the variant seeds so rerolls produce new images
    // We pass it by modifying the slug: "2025/post:batch3"
    let batch_slug = if batch > 0 {
        format!("{}:batch{}", slug, batch)
    } else {
        slug.to_string()
    };

    // Call: node scripts/og-image/index.mjs --slug {slug}
    // This generates 4 variants to data/og-previews/{slug}/
    let output = std::process::Command::new("node")
        .arg(&script)
        .arg("--slug")
        .arg(&batch_slug)
        .current_dir(repo_path)
        .output()
        .map_err(|e| format!("Failed to run OG script: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("OG script failed: {}", stderr));
    }

    // Find the generated files
    let safe_slug = slug.replace('/', "-");
    let preview_dir = format!("{}/data/og-previews/{}", repo_path, safe_slug);
    let mut paths = Vec::new();
    for i in 0..4 {
        let path = format!("{}/variant-{}.png", preview_dir, i);
        if std::path::Path::new(&path).exists() {
            paths.push(path);
        }
    }

    let preview_html = format!("{}/preview.html", preview_dir);

    Ok(OgImageVariants {
        slug: slug.to_string(),
        paths,
        preview_html,
    })
}

/// Upload a specific variant PNG to Cloudinary. Returns the URL.
pub fn upload_og_image(file_path: &str, slug: &str) -> Result<String, String> {
    let config = crate::cloudinary::get_config()?;
    let public_id = format!("og/{}", slug.replace('/', "-"));

    // Read file
    let data = std::fs::read(file_path)
        .map_err(|e| format!("Failed to read {}: {}", file_path, e))?;

    // Upload via multipart
    let ts = chrono::Utc::now().timestamp().to_string();
    let to_sign = format!("public_id={}&timestamp={}{}", public_id, ts, config.api_secret);
    use sha1::{Digest, Sha1};
    let mut hasher = Sha1::new();
    hasher.update(to_sign.as_bytes());
    let signature = format!("{:x}", hasher.finalize());

    let form = reqwest::blocking::multipart::Form::new()
        .part("file", reqwest::blocking::multipart::Part::bytes(data).file_name("og.png"))
        .text("public_id", public_id.clone())
        .text("timestamp", ts)
        .text("api_key", config.api_key.clone())
        .text("signature", signature)
        .text("overwrite", "true");

    let url = format!(
        "https://api.cloudinary.com/v1_1/{}/image/upload",
        config.cloud_name
    );

    let client = reqwest::blocking::Client::new();
    let resp = client.post(&url).multipart(form).send()
        .map_err(|e| format!("Upload failed: {}", e))?;

    if !resp.status().is_success() {
        let text = resp.text().unwrap_or_default();
        return Err(format!("Cloudinary error: {}", text));
    }

    let json: serde_json::Value = resp.json().unwrap_or_default();
    let secure_url = json["secure_url"]
        .as_str()
        .map(|s| s.to_string())
        .ok_or_else(|| "No URL in Cloudinary response".to_string())?;

    // Write to data/og-images.json in the website repo so blog:process picks it up
    let target = crate::config::default_target();
    let og_map_path = format!("{}/data/og-images.json", target.repo_path);
    if let Ok(mut map) = std::fs::read_to_string(&og_map_path)
        .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e)))
    {
        map[slug] = serde_json::Value::String(secure_url.clone());
        let _ = std::fs::write(&og_map_path, serde_json::to_string_pretty(&map).unwrap_or_default());
    }

    Ok(secure_url)
}

// ---------------------------------------------------------------------------
// Future platforms
// ---------------------------------------------------------------------------
// pub fn post_to_bluesky(post: &PostContent) -> SyndicationResult { ... }
// pub fn post_to_linkedin(post: &PostContent) -> SyndicationResult { ... }

// ---------------------------------------------------------------------------
// Dispatch all configured platforms
// ---------------------------------------------------------------------------

/// Syndicate to all enabled platforms. Returns results per platform.
pub fn syndicate_post(post: &PostContent) -> Vec<SyndicationResult> {
    let mut results = Vec::new();

    // Mastodon
    if std::env::var("MASTODON_ACCESS_TOKEN").is_ok() {
        results.push(post_to_mastodon(post));
    }

    // Future: check BLUESKY_*, LINKEDIN_*, etc.

    results
}
