use serde::{Deserialize, Serialize};
use std::sync::{Mutex, OnceLock};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UmamiConfig {
    pub url: String,
    pub username: String,
    pub password: String,
    pub website_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PostStats {
    pub pageviews: u64,
    pub visitors: u64,
    pub visits: u64,
    pub bounces: u64,
    pub totaltime: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TopPost {
    pub x: String, // URL path
    pub y: u64,    // pageview count
}

// Cached auth token: (token, expiry_timestamp)
static AUTH_TOKEN: OnceLock<Mutex<Option<(String, u64)>>> = OnceLock::new();

fn token_cache() -> &'static Mutex<Option<(String, u64)>> {
    AUTH_TOKEN.get_or_init(|| Mutex::new(None))
}

pub fn get_config() -> Result<UmamiConfig, String> {
    let url =
        std::env::var("UMAMI_URL").unwrap_or_else(|_| "https://umami.tools.ejfox.com".to_string());
    let username =
        std::env::var("UMAMI_USERNAME").map_err(|_| "UMAMI_USERNAME not set".to_string())?;
    let password =
        std::env::var("UMAMI_PASSWORD").map_err(|_| "UMAMI_PASSWORD not set".to_string())?;
    let website_id =
        std::env::var("UMAMI_WEBSITE_ID").map_err(|_| "UMAMI_WEBSITE_ID not set".to_string())?;
    Ok(UmamiConfig {
        url,
        username,
        password,
        website_id,
    })
}

async fn get_auth_token(config: &UmamiConfig) -> Result<String, String> {
    // Check cache first
    {
        let cache = token_cache().lock().map_err(|e| e.to_string())?;
        if let Some((ref token, expiry)) = *cache {
            let now = chrono::Utc::now().timestamp() as u64;
            if now < expiry {
                return Ok(token.clone());
            }
        }
    }

    // Fetch new token
    let client = reqwest::Client::builder().timeout(std::time::Duration::from_secs(4)).build().map_err(|e| e.to_string())?;
    let response = client
        .post(format!("{}/api/auth/login", config.url))
        .json(&serde_json::json!({
            "username": config.username,
            "password": config.password
        }))
        .send()
        .await
        .map_err(|e| format!("Umami auth failed: {}", e))?;

    if !response.status().is_success() {
        return Err(format!("Umami auth returned {}", response.status()));
    }

    let json: serde_json::Value = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse auth response: {}", e))?;

    let token = json["token"]
        .as_str()
        .ok_or("No token in auth response")?
        .to_string();

    // Cache for 1 hour
    let expiry = chrono::Utc::now().timestamp() as u64 + 3600;
    {
        let mut cache = token_cache().lock().map_err(|e| e.to_string())?;
        *cache = Some((token.clone(), expiry));
    }

    Ok(token)
}

fn clear_token_cache() {
    if let Ok(mut cache) = token_cache().lock() {
        *cache = None;
    }
}

pub async fn check_connection() -> bool {
    let config = match get_config() {
        Ok(c) => c,
        Err(_) => return false,
    };
    get_auth_token(&config).await.is_ok()
}

/// Site path for a published URL: https://ejfox.com/blog/2026/slug -> /blog/2026/slug,
/// https://ejfox.com/dispatch/slug -> /dispatch/slug. Query/fragment dropped.
pub fn post_path(published_url: &str) -> Option<&str> {
    let idx = published_url
        .find("/blog/")
        .or_else(|| published_url.find("/dispatch/"))?;
    let path = &published_url[idx..];
    let end = path.find(['?', '#']).unwrap_or(path.len());
    Some(&path[..end])
}

fn is_post_path(p: &str) -> bool {
    p.starts_with("/blog/") || p.starts_with("/dispatch/")
}

pub async fn get_post_stats(published_url: &str, days: u32) -> Result<PostStats, String> {
    let config = get_config()?;

    let path = post_path(published_url).ok_or("Invalid published URL")?;

    let now = chrono::Utc::now();
    let start = now - chrono::Duration::days(days as i64);
    let start_ms = start.timestamp_millis();
    let end_ms = now.timestamp_millis();

    let token = match get_auth_token(&config).await {
        Ok(t) => t,
        Err(e) => return Err(e),
    };

    let client = reqwest::Client::builder().timeout(std::time::Duration::from_secs(4)).build().map_err(|e| e.to_string())?;
    let url = format!(
        "{}/api/websites/{}/stats?startAt={}&endAt={}&url={}",
        config.url,
        config.website_id,
        start_ms,
        end_ms,
        urlencoding::encode(path)
    );

    let response = client
        .get(&url)
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await
        .map_err(|e| format!("Umami request failed: {}", e))?;

    if response.status().as_u16() == 401 {
        clear_token_cache();
        return Err("Umami auth expired, retry".to_string());
    }

    if !response.status().is_success() {
        return Err(format!("Umami stats returned {}", response.status()));
    }

    let stats: PostStats = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse stats: {}", e))?;

    Ok(stats)
}

/// Daily pageview timeseries for a post — feeds the inline sparkline.
pub async fn get_post_pageview_series(
    published_url: &str,
    days: u32,
) -> Result<Vec<u32>, String> {
    let config = get_config()?;

    let path = post_path(published_url).ok_or("Invalid published URL")?;

    let now = chrono::Utc::now();
    let start = now - chrono::Duration::days(days as i64);
    let start_ms = start.timestamp_millis();
    let end_ms = now.timestamp_millis();

    let token = get_auth_token(&config).await?;

    let client = reqwest::Client::builder().timeout(std::time::Duration::from_secs(4)).build().map_err(|e| e.to_string())?;
    let url = format!(
        "{}/api/websites/{}/pageviews?startAt={}&endAt={}&unit=day&timezone=UTC&url={}",
        config.url,
        config.website_id,
        start_ms,
        end_ms,
        urlencoding::encode(path)
    );

    let response = client
        .get(&url)
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await
        .map_err(|e| format!("Umami request failed: {}", e))?;

    if response.status().as_u16() == 401 {
        clear_token_cache();
        return Err("Umami auth expired, retry".to_string());
    }

    if !response.status().is_success() {
        return Err(format!("Umami pageviews returned {}", response.status()));
    }

    // Response shape: { pageviews: [{x: "2026-04-01T00:00:00Z", y: 12}, ...], sessions: [...] }
    let json: serde_json::Value = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse pageviews: {}", e))?;

    let series = json
        .get("pageviews")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .map(|p| p.get("y").and_then(|y| y.as_u64()).unwrap_or(0) as u32)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    Ok(series)
}

pub async fn get_site_stats(days: u32) -> Result<PostStats, String> {
    let config = get_config()?;

    let now = chrono::Utc::now();
    let start = now - chrono::Duration::days(days as i64);
    let start_ms = start.timestamp_millis();
    let end_ms = now.timestamp_millis();

    let token = get_auth_token(&config).await?;

    let client = reqwest::Client::builder().timeout(std::time::Duration::from_secs(4)).build().map_err(|e| e.to_string())?;
    let url = format!(
        "{}/api/websites/{}/stats?startAt={}&endAt={}",
        config.url, config.website_id, start_ms, end_ms
    );

    let response = client
        .get(&url)
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await
        .map_err(|e| format!("Umami request failed: {}", e))?;

    if response.status().as_u16() == 401 {
        clear_token_cache();
        return Err("Umami auth expired, retry".to_string());
    }

    if !response.status().is_success() {
        return Err(format!("Umami stats returned {}", response.status()));
    }

    response
        .json()
        .await
        .map_err(|e| format!("Failed to parse stats: {}", e))
}

pub async fn get_top_posts(days: u32, limit: usize) -> Result<Vec<TopPost>, String> {
    let config = get_config()?;

    let now = chrono::Utc::now();
    let start = now - chrono::Duration::days(days as i64);
    let start_ms = start.timestamp_millis();
    let end_ms = now.timestamp_millis();

    let token = get_auth_token(&config).await?;

    let client = reqwest::Client::builder().timeout(std::time::Duration::from_secs(4)).build().map_err(|e| e.to_string())?;
    let url = format!(
        "{}/api/websites/{}/metrics?startAt={}&endAt={}&type=url",
        config.url, config.website_id, start_ms, end_ms
    );

    let response = client
        .get(&url)
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await
        .map_err(|e| format!("Umami request failed: {}", e))?;

    if response.status().as_u16() == 401 {
        clear_token_cache();
        return Err("Umami auth expired, retry".to_string());
    }

    if !response.status().is_success() {
        return Err(format!("Umami metrics returned {}", response.status()));
    }

    let mut posts: Vec<TopPost> = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse metrics: {}", e))?;

    // Filter to blog posts + dispatch pieces and limit
    posts.retain(|p| is_post_path(&p.x));
    posts.truncate(limit);

    Ok(posts)
}

/// Visits to one post broken down by `utm_source` (read-only). Uses Umami's
/// `type=query` metrics filtered to the post path; untagged traffic is left out.
pub async fn get_post_utm_sources(
    published_url: &str,
    days: u32,
) -> Result<Vec<TopPost>, String> {
    let config = get_config()?;
    let path = post_path(published_url).ok_or("Invalid published URL")?;
    let now = chrono::Utc::now();
    let start_ms = (now - chrono::Duration::days(days as i64)).timestamp_millis();
    let end_ms = now.timestamp_millis();
    let token = get_auth_token(&config).await?;

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(4))
        .build()
        .map_err(|e| e.to_string())?;
    let url = format!(
        "{}/api/websites/{}/metrics?startAt={}&endAt={}&type=query&url={}",
        config.url,
        config.website_id,
        start_ms,
        end_ms,
        urlencoding::encode(path)
    );
    let response = client
        .get(&url)
        .header("Authorization", format!("Bearer {}", token))
        .send()
        .await
        .map_err(|e| format!("Umami request failed: {}", e))?;
    if response.status().as_u16() == 401 {
        clear_token_cache();
        return Err("Umami auth expired, retry".to_string());
    }
    if !response.status().is_success() {
        return Err(format!("Umami metrics returned {}", response.status()));
    }
    let rows: Vec<TopPost> = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse metrics: {}", e))?;
    Ok(aggregate_utm_sources(&rows))
}

/// Sum query-string rows by their utm_source value, largest first.
pub fn aggregate_utm_sources(rows: &[TopPost]) -> Vec<TopPost> {
    let mut totals: Vec<TopPost> = Vec::new();
    for row in rows {
        let Some(source) = crate::utm::query_param(&row.x, "utm_source") else {
            continue;
        };
        let source = source.to_ascii_lowercase();
        match totals.iter_mut().find(|t| t.x == source) {
            Some(t) => t.y += row.y,
            None => totals.push(TopPost { x: source, y: row.y }),
        }
    }
    totals.sort_by_key(|t| std::cmp::Reverse(t.y));
    totals
}

#[cfg(test)]
mod tests {
    use super::*;

    // One test, not two: these mutate process env vars, and parallel tests
    // racing on the same vars made the suite flaky.
    #[test]
    fn test_get_config_env() {
        // Without env vars set, should return error
        std::env::remove_var("UMAMI_USERNAME");
        std::env::remove_var("UMAMI_PASSWORD");
        std::env::remove_var("UMAMI_WEBSITE_ID");
        let result = get_config();
        assert!(result.is_err());

        std::env::set_var("UMAMI_USERNAME", "test_user");
        std::env::set_var("UMAMI_PASSWORD", "test_pass");
        std::env::set_var("UMAMI_WEBSITE_ID", "test_id");
        let config = get_config().unwrap();
        assert_eq!(config.username, "test_user");
        assert_eq!(config.password, "test_pass");
        assert_eq!(config.website_id, "test_id");
        // Default URL
        assert!(config.url.contains("umami"));
        // Clean up
        std::env::remove_var("UMAMI_USERNAME");
        std::env::remove_var("UMAMI_PASSWORD");
        std::env::remove_var("UMAMI_WEBSITE_ID");
    }

    #[test]
    fn test_post_stats_deserialization() {
        let json = r#"{"pageviews":42,"visitors":31,"visits":35,"bounces":5,"totaltime":1200}"#;
        let stats: PostStats = serde_json::from_str(json).unwrap();
        assert_eq!(stats.pageviews, 42);
        assert_eq!(stats.visitors, 31);
        assert_eq!(stats.totaltime, 1200);
    }

    #[test]
    fn test_top_post_deserialization() {
        let json = r#"[{"x":"/blog/2026/my-post","y":100},{"x":"/about","y":50}]"#;
        let mut posts: Vec<TopPost> = serde_json::from_str(json).unwrap();
        assert_eq!(posts.len(), 2);
        // Filter to blog posts
        posts.retain(|p| p.x.starts_with("/blog/"));
        assert_eq!(posts.len(), 1);
        assert_eq!(posts[0].x, "/blog/2026/my-post");
        assert_eq!(posts[0].y, 100);
    }

    #[test]
    fn test_post_path_blog_and_dispatch() {
        assert_eq!(
            post_path("https://ejfox.com/blog/2026/my-post"),
            Some("/blog/2026/my-post")
        );
        assert_eq!(
            post_path("https://ejfox.com/dispatch/water?utm_source=x#top"),
            Some("/dispatch/water")
        );
        assert_eq!(post_path("https://ejfox.com/about"), None);
        assert!(is_post_path("/dispatch/water"));
    }

    #[test]
    fn test_aggregate_utm_sources() {
        let rows: Vec<TopPost> = serde_json::from_str(
            r#"[{"x":"utm_source=bluesky&utm_medium=social","y":7},
                {"x":"?utm_medium=social&utm_source=Mastodon","y":3},
                {"x":"utm_source=bluesky&utm_campaign=dispatch-water","y":2},
                {"x":"ref=hn","y":40}]"#,
        )
        .unwrap();
        let agg = aggregate_utm_sources(&rows);
        assert_eq!(agg.len(), 2);
        assert_eq!((agg[0].x.as_str(), agg[0].y), ("bluesky", 9));
        assert_eq!((agg[1].x.as_str(), agg[1].y), ("mastodon", 3));
    }
}
