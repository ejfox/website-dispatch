use reqwest::header::{HeaderMap, HeaderValue, CONTENT_TYPE};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AltTextSuggestion {
    pub image_url: String,
    pub alt_text: String,
    pub confidence: f64,
    pub line_number: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AltTextResult {
    pub file_path: String,
    pub suggestions: Vec<AltTextSuggestion>,
    pub skipped: usize,
}

// ---------------------------------------------------------------------------
// Provider abstraction
// ---------------------------------------------------------------------------

enum Provider {
    Anthropic { api_key: String },
    OpenAI { api_key: String },
}

fn detect_provider() -> Result<Provider, String> {
    // Prefer OpenAI if set, fall back to Anthropic
    if let Ok(key) = std::env::var("OPENAI_API_KEY") {
        return Ok(Provider::OpenAI { api_key: key });
    }
    if let Ok(key) = std::env::var("ANTHROPIC_API_KEY") {
        return Ok(Provider::Anthropic { api_key: key });
    }
    Err("No API key found. Set OPENAI_API_KEY or ANTHROPIC_API_KEY in .env".into())
}

const ALT_TEXT_PROMPT: &str = r#"Write alt text for this image. Return JSON only:
{"alt": "...", "confidence": 0.95}

Your job is to describe the CONTENT — what is this about, what does it show, what does it mean to a reader who can't see it.

- One sentence, max 25 words. Shorter is better.
- Go DEEP, not wide. Don't describe the medium — describe the subject.
  Bad: "Three film contact sheets with black and white photographs"
  Good: "Contact sheets of street portraits and candid city scenes shot on black and white film"
  Bad: "Hand-drawn notes with sections and bullet points"
  Good: "Project planning sketch outlining milestones, weekly goals, and design method priorities"
  Bad: "Code editor with JavaScript source code"
  Good: "JavaScript CSV parser that groups rows by date and calculates running totals"
- If there's readable text, tell me what it SAYS, not that text exists.
- If it's a collection (contact sheet, grid, collage), describe what the individual items are OF.
- No editorializing or interpreting. No "represents" or "symbolizes."
- Never start with "Image of", "Photo of", "Screenshot of"
- Never hedge: no "appears", "seems", "looks like"
- confidence: 0-1. Low if blurry or too small to read the actual content."#;

// ---------------------------------------------------------------------------
// Image URL helpers
// ---------------------------------------------------------------------------

fn is_video_url(url: &str) -> bool {
    let lower = url.to_lowercase();
    lower.contains("/video/upload/")
        || lower.ends_with(".mp4")
        || lower.ends_with(".webm")
        || lower.ends_with(".mov")
        || lower.ends_with(".gif")
}

fn is_cloudinary_url(url: &str) -> bool {
    url.contains("res.cloudinary.com/")
}

fn get_image_url(url: &str) -> String {
    if !is_cloudinary_url(url) {
        return url.to_string();
    }
    let url = url.replace("http://", "https://");
    let parts: Vec<&str> = url.splitn(2, "/upload/").collect();
    if parts.len() != 2 {
        return url;
    }
    format!(
        "{}/upload/c_scale,w_1280,f_jpg,q_auto/{}",
        parts[0], parts[1]
    )
}

/// Find all images with missing or junk alt text in a markdown string.
pub fn find_missing_alt_images(content: &str) -> Vec<(String, String, usize)> {
    let mut results = Vec::new();

    for (line_idx, line) in content.lines().enumerate() {
        for caps in crate::patterns::MD_IMAGE.captures_iter(line) {
            let alt = caps.get(1).unwrap().as_str();
            if !crate::vault::is_junk_alt(alt) {
                continue;
            }
            let full_match = caps.get(0).unwrap().as_str().to_string();
            let url = caps.get(2).unwrap().as_str().to_string();
            results.push((full_match, url, line_idx + 1));
        }
    }

    results
}

// ---------------------------------------------------------------------------
// Provider-specific API calls
// ---------------------------------------------------------------------------

async fn call_anthropic(api_key: &str, image_url: &str) -> Result<String, String> {
    let body = serde_json::json!({
        "model": "claude-haiku-4-5-20251001",
        "max_tokens": 200,
        "messages": [{
            "role": "user",
            "content": [
                { "type": "image", "source": { "type": "url", "url": image_url } },
                { "type": "text", "text": ALT_TEXT_PROMPT }
            ]
        }]
    });

    let mut headers = HeaderMap::new();
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    headers.insert(
        "x-api-key",
        HeaderValue::from_str(api_key).map_err(|e| format!("Invalid API key: {}", e))?,
    );
    headers.insert("anthropic-version", HeaderValue::from_static("2023-06-01"));

    let client = reqwest::Client::new();
    let resp = client
        .post("https://api.anthropic.com/v1/messages")
        .headers(headers)
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("API request failed: {}", e))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(format!("Anthropic error {}: {}", status, text));
    }

    let data: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| format!("Failed to parse response: {}", e))?;

    data["content"][0]["text"]
        .as_str()
        .map(|s| s.trim().to_string())
        .ok_or_else(|| "No text in Anthropic response".into())
}

async fn call_openai(api_key: &str, image_url: &str) -> Result<String, String> {
    let body = serde_json::json!({
        "model": "gpt-4o-mini",
        "max_tokens": 200,
        "messages": [{
            "role": "user",
            "content": [
                { "type": "image_url", "image_url": { "url": image_url, "detail": "low" } },
                { "type": "text", "text": ALT_TEXT_PROMPT }
            ]
        }]
    });

    let mut headers = HeaderMap::new();
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    headers.insert(
        "Authorization",
        HeaderValue::from_str(&format!("Bearer {}", api_key))
            .map_err(|e| format!("Invalid API key: {}", e))?,
    );

    let client = reqwest::Client::new();
    let resp = client
        .post("https://api.openai.com/v1/chat/completions")
        .headers(headers)
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("API request failed: {}", e))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(format!("OpenAI error {}: {}", status, text));
    }

    let data: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| format!("Failed to parse response: {}", e))?;

    data["choices"][0]["message"]["content"]
        .as_str()
        .map(|s| s.trim().to_string())
        .ok_or_else(|| "No text in OpenAI response".into())
}

/// Call the detected provider's vision API. Returns (alt_text, confidence).
async fn call_vision_api(provider: &Provider, image_url: &str) -> Result<(String, f64), String> {
    let url = get_image_url(image_url);

    let raw = match provider {
        Provider::Anthropic { api_key } => call_anthropic(api_key, &url).await?,
        Provider::OpenAI { api_key } => call_openai(api_key, &url).await?,
    };

    // Parse JSON response
    let cleaned = raw
        .trim_start_matches("```json")
        .trim_start_matches("```")
        .trim_end_matches("```")
        .trim();

    if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(cleaned) {
        let alt = parsed["alt"]
            .as_str()
            .unwrap_or(&raw)
            .trim()
            .replace('[', "(")
            .replace(']', ")");
        let confidence = parsed["confidence"].as_f64().unwrap_or(0.5);
        Ok((alt, confidence))
    } else {
        Ok((raw.replace('[', "(").replace(']', ")"), 0.5))
    }
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Scan a markdown file and generate alt text suggestions for images missing it.
pub async fn generate_suggestions(file_path: &str) -> Result<AltTextResult, String> {
    let provider = detect_provider()?;
    let content =
        std::fs::read_to_string(file_path).map_err(|e| format!("Failed to read file: {}", e))?;

    let images = find_missing_alt_images(&content);
    let mut suggestions = Vec::new();
    let mut skipped = 0;

    for (_full_match, url, line_number) in &images {
        if is_video_url(url) || !is_cloudinary_url(url) {
            skipped += 1;
            continue;
        }

        match call_vision_api(&provider, url).await {
            Ok((alt_text, confidence)) => {
                suggestions.push(AltTextSuggestion {
                    image_url: url.clone(),
                    alt_text,
                    confidence,
                    line_number: *line_number,
                });
            }
            Err(e) => {
                log::warn!("Failed to generate alt text for {}: {}", url, e);
                skipped += 1;
            }
        }

        tokio::time::sleep(tokio::time::Duration::from_millis(600)).await;
    }

    Ok(AltTextResult {
        file_path: file_path.to_string(),
        suggestions,
        skipped,
    })
}

/// Apply alt text suggestions to a markdown file, writing the changes.
/// Handles both empty alt ![](url) and junk alt ![junk](url).
pub fn apply_suggestions(
    file_path: &str,
    suggestions: &[AltTextSuggestion],
) -> Result<usize, String> {
    let mut content =
        std::fs::read_to_string(file_path).map_err(|e| format!("Failed to read file: {}", e))?;

    let mut applied = 0;

    for suggestion in suggestions {
        // Find any image tag with this URL (regardless of current alt text)
        let new_tag = format!("![{}]({})", suggestion.alt_text, suggestion.image_url);
        let mut found = false;
        if let Some(caps) = crate::patterns::MD_IMAGE
            .captures_iter(&content.clone())
            .find(|c| c.get(2).map(|m| m.as_str()) == Some(&suggestion.image_url))
        {
            let old_tag = caps.get(0).unwrap().as_str().to_string();
            if old_tag != new_tag {
                content = content.replacen(&old_tag, &new_tag, 1);
                found = true;
            }
        }
        if found {
            applied += 1;
        }
    }

    if applied > 0 {
        std::fs::write(file_path, &content).map_err(|e| format!("Failed to write file: {}", e))?;

        // Best-effort sync to Cloudinary context metadata
        let cloud_suggestions: Vec<_> = suggestions
            .iter()
            .filter(|s| is_cloudinary_url(&s.image_url))
            .collect();
        if !cloud_suggestions.is_empty() {
            if let Ok(config) = crate::cloudinary::get_config() {
                for s in &cloud_suggestions {
                    if let Some(public_id) = extract_public_id(&s.image_url) {
                        let _ = sync_alt_to_cloudinary(&config, &public_id, &s.alt_text);
                    }
                }
            }
        }
    }

    Ok(applied)
}

/// Extract the public_id from a Cloudinary URL.
fn extract_public_id(url: &str) -> Option<String> {
    let url = url.replace("http://", "https://");
    let after_upload = url.split("/upload/").nth(1)?;
    let parts: Vec<&str> = after_upload.split('/').collect();
    let filtered: Vec<&str> = parts
        .iter()
        .filter(|p| {
            // Skip version segments (v1234567) and transform segments (c_scale,w_512)
            if crate::patterns::CLOUDINARY_VERSION.is_match(p) {
                return false;
            }
            if p.contains(',') || crate::patterns::CLOUDINARY_TRANSFORM.is_match(p) {
                return false;
            }
            true
        })
        .copied()
        .collect();
    let joined = filtered.join("/");
    // Strip file extension
    Some(
        crate::patterns::IMAGE_FILE_EXT
            .replace(&joined, "")
            .to_string(),
    )
}

/// Push alt text to Cloudinary's context metadata (blocking, best-effort).
fn sync_alt_to_cloudinary(
    config: &crate::cloudinary::CloudinaryConfig,
    public_id: &str,
    alt_text: &str,
) -> Result<(), String> {
    // Escape pipe characters in alt text (Cloudinary context delimiter)
    let safe_alt = alt_text.replace(['|', '='], " ");
    let url = format!(
        "https://api.cloudinary.com/v1_1/{}/image/explicit",
        config.cloud_name
    );
    let body = serde_json::json!({
        "public_id": public_id,
        "type": "upload",
        "context": format!("alt={}|caption={}", safe_alt, safe_alt),
    });

    let client = reqwest::blocking::Client::new();
    let resp = client
        .post(&url)
        .basic_auth(&config.api_key, Some(&config.api_secret))
        .json(&body)
        .send()
        .map_err(|e| format!("Cloudinary sync failed: {}", e))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().unwrap_or_default();
        log::warn!(
            "Cloudinary context update failed for {}: {} {}",
            public_id,
            status,
            text
        );
    }
    Ok(())
}
