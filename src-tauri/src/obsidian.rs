use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION};
use serde::{Deserialize, Serialize};

const OBSIDIAN_API_URL: &str = "https://127.0.0.1:27124";
const OBSIDIAN_API_KEY: &str = "f246add73b5d8d1ca913c5770baa7da457f3839a69d5cf1b5c64cf4608662ef1";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Backlink {
    pub path: String,
    pub title: Option<String>,
    pub context: String,
}

#[derive(Debug, Deserialize)]
struct SearchResult {
    filename: String,
    #[serde(default)]
    matches: Vec<SearchMatch>,
}

#[derive(Debug, Deserialize)]
struct SearchMatch {
    #[serde(default)]
    match_: MatchInfo,
}

#[derive(Debug, Deserialize, Default)]
struct MatchInfo {
    #[serde(default)]
    content: String,
}

fn build_client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .danger_accept_invalid_certs(true) // Obsidian uses self-signed cert
        .build()
        .map_err(|e| e.to_string())
}

fn build_headers() -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert(
        AUTHORIZATION,
        HeaderValue::from_str(&format!("Bearer {}", OBSIDIAN_API_KEY)).unwrap(),
    );
    headers
}

pub async fn get_backlinks(filename: &str) -> Result<Vec<Backlink>, String> {
    let client = build_client()?;
    let headers = build_headers();

    // Strip .md extension for wiki-link search
    let base_name = filename.trim_end_matches(".md");

    // Search for [[filename]] wiki-links
    let search_query = format!("[[{}]]", base_name);
    let url = format!("{}/search/simple/?query={}", OBSIDIAN_API_URL, urlencoding::encode(&search_query));

    let response = client
        .get(&url)
        .headers(headers.clone())
        .send()
        .await
        .map_err(|e| format!("Failed to connect to Obsidian: {}", e))?;

    if !response.status().is_success() {
        // Try alternative search endpoint
        return get_backlinks_via_search(filename).await;
    }

    let results: Vec<SearchResult> = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse response: {}", e))?;

    let backlinks: Vec<Backlink> = results
        .into_iter()
        .filter(|r| r.filename != filename) // Exclude self-references
        .map(|r| {
            let context = r.matches
                .first()
                .map(|m| m.match_.content.clone())
                .unwrap_or_default();

            let title = extract_title_from_filename(&r.filename);

            Backlink {
                path: r.filename,
                title,
                context: truncate_context(&context, 100),
            }
        })
        .collect();

    Ok(backlinks)
}

async fn get_backlinks_via_search(filename: &str) -> Result<Vec<Backlink>, String> {
    let client = build_client()?;
    let headers = build_headers();

    let base_name = filename.trim_end_matches(".md");

    // Try the POST search endpoint with dataview-like query
    let url = format!("{}/search/", OBSIDIAN_API_URL);
    let query = serde_json::json!({
        "query": format!("[[{}]]", base_name)
    });

    let response = client
        .post(&url)
        .headers(headers)
        .json(&query)
        .send()
        .await
        .map_err(|e| format!("Search failed: {}", e))?;

    if !response.status().is_success() {
        // API might not be running or different version
        return Ok(Vec::new());
    }

    // Try to parse as array of filenames or search results
    let text = response.text().await.map_err(|e| e.to_string())?;

    // Parse various response formats
    if let Ok(filenames) = serde_json::from_str::<Vec<String>>(&text) {
        return Ok(filenames
            .into_iter()
            .filter(|f| f != filename)
            .map(|f| Backlink {
                title: extract_title_from_filename(&f),
                path: f,
                context: String::new(),
            })
            .collect());
    }

    Ok(Vec::new())
}

fn extract_title_from_filename(filename: &str) -> Option<String> {
    let name = filename
        .rsplit('/')
        .next()
        .unwrap_or(filename)
        .trim_end_matches(".md");

    if name.is_empty() {
        None
    } else {
        Some(name.replace('-', " "))
    }
}

fn truncate_context(context: &str, max_len: usize) -> String {
    if context.len() <= max_len {
        context.to_string()
    } else {
        format!("{}...", &context[..max_len])
    }
}

pub async fn check_api_status() -> bool {
    let client = match build_client() {
        Ok(c) => c,
        Err(_) => return false,
    };

    let headers = build_headers();

    client
        .get(format!("{}/", OBSIDIAN_API_URL))
        .headers(headers)
        .send()
        .await
        .map(|r| r.status().is_success())
        .unwrap_or(false)
}
