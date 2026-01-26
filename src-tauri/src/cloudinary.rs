use reqwest::multipart;
use serde::{Deserialize, Serialize};
use sha1::{Digest, Sha1};
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloudinaryConfig {
    pub cloud_name: String,
    pub api_key: String,
    pub api_secret: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloudinaryAsset {
    pub public_id: String,
    pub secure_url: String,
    pub resource_type: String,
    pub format: String,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub bytes: u64,
    pub created_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UploadResult {
    pub success: bool,
    pub asset: Option<CloudinaryAsset>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaLibraryPage {
    pub assets: Vec<CloudinaryAsset>,
    pub next_cursor: Option<String>,
    pub total_count: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalMediaRef {
    pub original_text: String,
    pub path: String,
    pub resolved_path: Option<String>,
    pub alt_text: Option<String>,
    pub media_type: String, // "image" or "video"
    pub line_number: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaFixResult {
    pub original_ref: LocalMediaRef,
    pub upload_result: UploadResult,
    pub replacement_text: Option<String>,
}

/// Load Cloudinary configuration from environment variables
pub fn get_config() -> Result<CloudinaryConfig, String> {
    let cloud_name = std::env::var("CLOUDINARY_CLOUD_NAME")
        .map_err(|_| "CLOUDINARY_CLOUD_NAME not set")?;
    let api_key = std::env::var("CLOUDINARY_API_KEY")
        .map_err(|_| "CLOUDINARY_API_KEY not set")?;
    let api_secret = std::env::var("CLOUDINARY_API_SECRET")
        .map_err(|_| "CLOUDINARY_API_SECRET not set")?;

    Ok(CloudinaryConfig {
        cloud_name,
        api_key,
        api_secret,
    })
}

/// Generate SHA-1 signature for Cloudinary API
fn generate_signature(params: &BTreeMap<String, String>, api_secret: &str) -> String {
    let mut to_sign = params
        .iter()
        .map(|(k, v)| format!("{}={}", k, v))
        .collect::<Vec<_>>()
        .join("&");
    to_sign.push_str(api_secret);

    let mut hasher = Sha1::new();
    hasher.update(to_sign.as_bytes());
    format!("{:x}", hasher.finalize())
}

/// Get resource type from file extension
fn get_resource_type(path: &str) -> &'static str {
    let ext = Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    match ext.as_str() {
        "mp4" | "mov" | "avi" | "webm" | "mkv" | "m4v" => "video",
        "jpg" | "jpeg" | "png" | "gif" | "webp" | "svg" | "bmp" | "ico" | "tiff" => "image",
        _ => "raw",
    }
}

/// Upload a file to Cloudinary
pub async fn upload_file(
    file_path: &str,
    folder: Option<&str>,
    public_id: Option<&str>,
) -> Result<UploadResult, String> {
    let config = get_config()?;

    // Check file size
    let file_size = fs::metadata(file_path)
        .map_err(|e| format!("Failed to read file: {}", e))?
        .len() as usize;

    // Check file size limits (10MB for images, 100MB for videos)
    let resource_type = get_resource_type(file_path);
    let max_size = if resource_type == "video" {
        100 * 1024 * 1024
    } else {
        10 * 1024 * 1024
    };

    if file_size > max_size {
        return Ok(UploadResult {
            success: false,
            asset: None,
            error: Some(format!(
                "File too large: {}MB (max {}MB)",
                file_size / (1024 * 1024),
                max_size / (1024 * 1024)
            )),
        });
    }

    let mime_type = mime_guess::from_path(file_path)
        .first_or_octet_stream()
        .to_string();

    // Upload with retry
    let url = format!(
        "https://api.cloudinary.com/v1_1/{}/{}/upload",
        config.cloud_name, resource_type
    );

    let client = reqwest::Client::new();
    let mut last_error = String::new();

    for attempt in 0..3 {
        if attempt > 0 {
            tokio::time::sleep(tokio::time::Duration::from_millis(500 * (1 << attempt))).await;
        }

        // Need to rebuild form for each attempt since it's consumed
        let file_data = fs::read(file_path).map_err(|e| format!("Failed to read file: {}", e))?;
        let file_part = multipart::Part::bytes(file_data)
            .file_name(
                Path::new(file_path)
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("file")
                    .to_string(),
            )
            .mime_str(&mime_type)
            .map_err(|e| e.to_string())?;

        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs()
            .to_string();

        let mut params = BTreeMap::new();
        params.insert("timestamp".to_string(), timestamp.clone());
        if let Some(f) = folder {
            params.insert("folder".to_string(), f.to_string());
        }
        if let Some(pid) = public_id {
            params.insert("public_id".to_string(), pid.to_string());
        }
        let signature = generate_signature(&params, &config.api_secret);

        let mut form = multipart::Form::new()
            .part("file", file_part)
            .text("api_key", config.api_key.clone())
            .text("timestamp", timestamp)
            .text("signature", signature);

        if let Some(f) = folder {
            form = form.text("folder", f.to_string());
        }
        if let Some(pid) = public_id {
            form = form.text("public_id", pid.to_string());
        }

        match client.post(&url).multipart(form).send().await {
            Ok(response) => {
                if response.status().is_success() {
                    let json: serde_json::Value = response
                        .json()
                        .await
                        .map_err(|e| format!("Failed to parse response: {}", e))?;

                    let asset = CloudinaryAsset {
                        public_id: json["public_id"].as_str().unwrap_or("").to_string(),
                        secure_url: json["secure_url"].as_str().unwrap_or("").to_string(),
                        resource_type: json["resource_type"].as_str().unwrap_or("").to_string(),
                        format: json["format"].as_str().unwrap_or("").to_string(),
                        width: json["width"].as_u64().map(|w| w as u32),
                        height: json["height"].as_u64().map(|h| h as u32),
                        bytes: json["bytes"].as_u64().unwrap_or(0),
                        created_at: json["created_at"].as_str().map(|s| s.to_string()),
                    };

                    return Ok(UploadResult {
                        success: true,
                        asset: Some(asset),
                        error: None,
                    });
                } else {
                    let status = response.status();
                    let body = response.text().await.unwrap_or_default();
                    last_error = format!("HTTP {}: {}", status, body);
                }
            }
            Err(e) => {
                last_error = format!("Request failed: {}", e);
            }
        }
    }

    Ok(UploadResult {
        success: false,
        asset: None,
        error: Some(last_error),
    })
}

/// List assets from Cloudinary media library
pub async fn list_assets(
    resource_type: Option<&str>,
    max_results: Option<u32>,
    cursor: Option<&str>,
) -> Result<MediaLibraryPage, String> {
    let config = get_config()?;

    let res_type = resource_type.unwrap_or("image");
    let max = max_results.unwrap_or(30);

    // Use search API instead of resources API to avoid duplicates
    // Search API lets us sort by created_at and get cleaner results
    let url = format!(
        "https://api.cloudinary.com/v1_1/{}/resources/search",
        config.cloud_name
    );

    let mut body = serde_json::json!({
        "expression": format!("resource_type:{}", res_type),
        "max_results": max,
        "sort_by": [{"created_at": "desc"}],
        "with_field": ["context", "tags"]
    });

    if let Some(c) = cursor {
        body["next_cursor"] = serde_json::json!(c);
    }

    let client = reqwest::Client::new();
    let response = client
        .post(&url)
        .basic_auth(&config.api_key, Some(&config.api_secret))
        .json(&body)
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

    // Deduplicate by public_id (in case of any duplicates)
    let mut seen = std::collections::HashSet::new();
    let assets: Vec<CloudinaryAsset> = json["resources"]
        .as_array()
        .unwrap_or(&vec![])
        .iter()
        .filter_map(|r| {
            let public_id = r["public_id"].as_str().unwrap_or("").to_string();
            if seen.contains(&public_id) {
                None
            } else {
                seen.insert(public_id.clone());
                Some(CloudinaryAsset {
                    public_id,
                    secure_url: r["secure_url"].as_str().unwrap_or("").to_string(),
                    resource_type: r["resource_type"].as_str().unwrap_or("").to_string(),
                    format: r["format"].as_str().unwrap_or("").to_string(),
                    width: r["width"].as_u64().map(|w| w as u32),
                    height: r["height"].as_u64().map(|h| h as u32),
                    bytes: r["bytes"].as_u64().unwrap_or(0),
                    created_at: r["created_at"].as_str().map(|s| s.to_string()),
                })
            }
        })
        .collect();

    Ok(MediaLibraryPage {
        assets,
        next_cursor: json["next_cursor"].as_str().map(|s| s.to_string()),
        total_count: json["total_count"].as_u64().map(|c| c as u32),
    })
}

/// Search assets in Cloudinary
pub async fn search_assets(query: &str, max_results: Option<u32>) -> Result<MediaLibraryPage, String> {
    let config = get_config()?;

    let max = max_results.unwrap_or(30);

    let url = format!(
        "https://api.cloudinary.com/v1_1/{}/resources/search",
        config.cloud_name
    );

    let body = serde_json::json!({
        "expression": query,
        "max_results": max,
        "sort_by": [{"created_at": "desc"}]
    });

    let client = reqwest::Client::new();
    let response = client
        .post(&url)
        .basic_auth(&config.api_key, Some(&config.api_secret))
        .json(&body)
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

    // Deduplicate by public_id
    let mut seen = std::collections::HashSet::new();
    let assets: Vec<CloudinaryAsset> = json["resources"]
        .as_array()
        .unwrap_or(&vec![])
        .iter()
        .filter_map(|r| {
            let public_id = r["public_id"].as_str().unwrap_or("").to_string();
            if seen.contains(&public_id) {
                None
            } else {
                seen.insert(public_id.clone());
                Some(CloudinaryAsset {
                    public_id,
                    secure_url: r["secure_url"].as_str().unwrap_or("").to_string(),
                    resource_type: r["resource_type"].as_str().unwrap_or("").to_string(),
                    format: r["format"].as_str().unwrap_or("").to_string(),
                    width: r["width"].as_u64().map(|w| w as u32),
                    height: r["height"].as_u64().map(|h| h as u32),
                    bytes: r["bytes"].as_u64().unwrap_or(0),
                    created_at: r["created_at"].as_str().map(|s| s.to_string()),
                })
            }
        })
        .collect();

    Ok(MediaLibraryPage {
        assets,
        next_cursor: json["next_cursor"].as_str().map(|s| s.to_string()),
        total_count: json["total_count"].as_u64().map(|c| c as u32),
    })
}

/// Check if Cloudinary credentials are configured and valid
pub async fn check_credentials() -> bool {
    if get_config().is_err() {
        return false;
    }

    // Try a simple API call to verify credentials
    match list_assets(Some("image"), Some(1), None).await {
        Ok(_) => true,
        Err(_) => false,
    }
}

/// Extract local media references from markdown content
pub fn extract_local_media(content: &str, source_dir: &str) -> Vec<LocalMediaRef> {
    let mut refs = Vec::new();

    // Regex patterns for media references
    let md_image_re =
        regex::Regex::new(r"!\[([^\]]*)\]\(([^)]+)\)").unwrap();
    let html_img_re =
        regex::Regex::new(r#"<img[^>]+src=["']([^"']+)["'][^>]*>"#).unwrap();
    let html_video_re =
        regex::Regex::new(r#"<video[^>]+src=["']([^"']+)["'][^>]*>"#).unwrap();

    for (line_num, line) in content.lines().enumerate() {
        // Skip lines with cloudinary URLs or https URLs
        if line.contains("cloudinary") {
            continue;
        }

        // Markdown images: ![alt](path)
        for cap in md_image_re.captures_iter(line) {
            let alt = cap.get(1).map(|m| m.as_str().to_string());
            let path = cap.get(2).map(|m| m.as_str()).unwrap_or("");

            // Skip external URLs
            if path.starts_with("http://") || path.starts_with("https://") {
                continue;
            }

            let original_text = cap.get(0).map(|m| m.as_str()).unwrap_or("").to_string();
            let resolved = resolve_path(path, source_dir);
            let media_type = if is_video_extension(path) {
                "video"
            } else {
                "image"
            };

            refs.push(LocalMediaRef {
                original_text,
                path: path.to_string(),
                resolved_path: resolved,
                alt_text: alt,
                media_type: media_type.to_string(),
                line_number: line_num + 1,
            });
        }

        // HTML img tags
        for cap in html_img_re.captures_iter(line) {
            let path = cap.get(1).map(|m| m.as_str()).unwrap_or("");

            if path.starts_with("http://") || path.starts_with("https://") {
                continue;
            }

            let original_text = cap.get(0).map(|m| m.as_str()).unwrap_or("").to_string();
            let resolved = resolve_path(path, source_dir);

            refs.push(LocalMediaRef {
                original_text,
                path: path.to_string(),
                resolved_path: resolved,
                alt_text: None,
                media_type: "image".to_string(),
                line_number: line_num + 1,
            });
        }

        // HTML video tags
        for cap in html_video_re.captures_iter(line) {
            let path = cap.get(1).map(|m| m.as_str()).unwrap_or("");

            if path.starts_with("http://") || path.starts_with("https://") {
                continue;
            }

            let original_text = cap.get(0).map(|m| m.as_str()).unwrap_or("").to_string();
            let resolved = resolve_path(path, source_dir);

            refs.push(LocalMediaRef {
                original_text,
                path: path.to_string(),
                resolved_path: resolved,
                alt_text: None,
                media_type: "video".to_string(),
                line_number: line_num + 1,
            });
        }
    }

    refs
}

/// Resolve a relative path to an absolute path
fn resolve_path(path: &str, source_dir: &str) -> Option<String> {
    let path_buf = if path.starts_with("./") {
        Path::new(source_dir).join(&path[2..])
    } else if path.starts_with('/') {
        // Absolute from vault root - need to find vault root
        let config = crate::Config::default();
        Path::new(&config.vault_path).join(&path[1..])
    } else {
        Path::new(source_dir).join(path)
    };

    if path_buf.exists() {
        Some(path_buf.to_string_lossy().to_string())
    } else {
        None
    }
}

/// Check if a path has a video extension
fn is_video_extension(path: &str) -> bool {
    let ext = Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    matches!(ext.as_str(), "mp4" | "mov" | "avi" | "webm" | "mkv" | "m4v")
}

/// Generate replacement text for an uploaded asset
pub fn generate_replacement(original: &LocalMediaRef, asset: &CloudinaryAsset) -> String {
    if original.media_type == "video" {
        // For videos, use HTML video tag
        format!(
            r#"<video src="{}" controls></video>"#,
            asset.secure_url
        )
    } else {
        // For images, use markdown syntax
        let alt = original.alt_text.as_deref().unwrap_or("");
        format!("![{}]({})", alt, asset.secure_url)
    }
}

/// Apply fixes to a markdown file
pub fn apply_fixes_to_file(file_path: &str, fixes: &[(String, String)]) -> Result<(), String> {
    let content = fs::read_to_string(file_path)
        .map_err(|e| format!("Failed to read file: {}", e))?;

    let mut new_content = content;
    for (original, replacement) in fixes {
        new_content = new_content.replace(original, replacement);
    }

    fs::write(file_path, new_content)
        .map_err(|e| format!("Failed to write file: {}", e))?;

    Ok(())
}
