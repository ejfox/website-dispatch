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
    let client = reqwest::Client::new();
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

pub async fn get_post_stats(published_url: &str, days: u32) -> Result<PostStats, String> {
    let config = get_config()?;

    // Extract path from full URL: https://ejfox.com/blog/2026/slug -> /blog/2026/slug
    let path = if let Some(idx) = published_url.find("/blog/") {
        &published_url[idx..]
    } else {
        return Err("Invalid published URL".to_string());
    };

    let now = chrono::Utc::now();
    let start = now - chrono::Duration::days(days as i64);
    let start_ms = start.timestamp_millis();
    let end_ms = now.timestamp_millis();

    let token = match get_auth_token(&config).await {
        Ok(t) => t,
        Err(e) => return Err(e),
    };

    let client = reqwest::Client::new();
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

pub async fn get_site_stats(days: u32) -> Result<PostStats, String> {
    let config = get_config()?;

    let now = chrono::Utc::now();
    let start = now - chrono::Duration::days(days as i64);
    let start_ms = start.timestamp_millis();
    let end_ms = now.timestamp_millis();

    let token = get_auth_token(&config).await?;

    let client = reqwest::Client::new();
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

    let client = reqwest::Client::new();
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

    // Filter to blog posts only and limit
    posts.retain(|p| p.x.starts_with("/blog/"));
    posts.truncate(limit);

    Ok(posts)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_config_missing_vars() {
        // Without env vars set, should return error
        std::env::remove_var("UMAMI_USERNAME");
        std::env::remove_var("UMAMI_PASSWORD");
        std::env::remove_var("UMAMI_WEBSITE_ID");
        let result = get_config();
        assert!(result.is_err());
    }

    #[test]
    fn test_get_config_with_vars() {
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
}
