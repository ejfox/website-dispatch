use regex::Regex;
use reqwest::blocking::Client;
use reqwest::header::LINK;
use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebmentionResult {
    pub target: String,
    pub endpoint: Option<String>,
    pub status: String, // "sent", "no_endpoint", "error"
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebmentionReport {
    pub source: String,
    pub results: Vec<WebmentionResult>,
    pub total_links: usize,
    pub sent: usize,
    pub no_endpoint: usize,
    pub errors: usize,
}

/// Extract all outbound HTTP(S) links from HTML content
pub fn extract_links(html: &str, own_domain: &str) -> Vec<String> {
    let re = Regex::new(r#"<a[^>]+href="(https?://[^"]+)"[^>]*>"#).unwrap();
    let own = own_domain.trim_end_matches('/').to_lowercase();

    re.captures_iter(html)
        .filter_map(|cap| {
            let url = cap[1].to_string();
            let lower = url.to_lowercase();
            // Skip own domain, localhost, common non-webmention targets
            if lower.contains(&own)
                || lower.contains("localhost")
                || lower.contains("127.0.0.1")
                || lower.contains("github.com")
                || lower.contains("youtube.com")
                || lower.contains("twitter.com")
                || lower.contains("x.com/")
            {
                None
            } else {
                Some(url)
            }
        })
        .collect()
}

/// Discover the webmention endpoint for a target URL
pub fn discover_endpoint(client: &Client, target: &str) -> Option<String> {
    let resp = client.get(target).send().ok()?;
    let final_url = resp.url().clone();

    // 1. Check HTTP Link header
    if let Some(link_header) = resp.headers().get(LINK) {
        if let Ok(header_str) = link_header.to_str() {
            if let Some(endpoint) = parse_link_header(header_str) {
                return Some(resolve_url(&final_url, &endpoint));
            }
        }
    }

    let body = resp.text().ok()?;

    // 2. Check <link rel="webmention"> in HTML
    let link_re =
        Regex::new(r#"<link[^>]*rel=["']?webmention["']?[^>]*href=["']([^"']+)["'][^>]*/?\s*>"#)
            .unwrap();
    // Also match when href comes before rel
    let link_re2 =
        Regex::new(r#"<link[^>]*href=["']([^"']+)["'][^>]*rel=["']?webmention["']?[^>]*/?\s*>"#)
            .unwrap();

    if let Some(cap) = link_re.captures(&body).or_else(|| link_re2.captures(&body)) {
        return Some(resolve_url(&final_url, &cap[1]));
    }

    // 3. Check <a rel="webmention"> in HTML
    let a_re =
        Regex::new(r#"<a[^>]*rel=["']?webmention["']?[^>]*href=["']([^"']+)["'][^>]*>"#)
            .unwrap();
    let a_re2 =
        Regex::new(r#"<a[^>]*href=["']([^"']+)["'][^>]*rel=["']?webmention["']?[^>]*>"#)
            .unwrap();

    if let Some(cap) = a_re.captures(&body).or_else(|| a_re2.captures(&body)) {
        return Some(resolve_url(&final_url, &cap[1]));
    }

    None
}

/// Send a webmention from source to target via the discovered endpoint
pub fn send_webmention(
    client: &Client,
    endpoint: &str,
    source: &str,
    target: &str,
) -> Result<String, String> {
    let resp = client
        .post(endpoint)
        .header("Content-Type", "application/x-www-form-urlencoded")
        .body(format!(
            "source={}&target={}",
            urlencoding::encode(source),
            urlencoding::encode(target)
        ))
        .send()
        .map_err(|e| format!("Request failed: {}", e))?;

    let status = resp.status();
    if status.is_success() {
        Ok(format!("{}", status.as_u16()))
    } else {
        let body = resp.text().unwrap_or_default();
        Err(format!("HTTP {}: {}", status.as_u16(), body))
    }
}

/// Send webmentions for all outbound links in a published post.
/// Fetches the published URL to get the rendered HTML, extracts links,
/// discovers endpoints, and sends webmentions.
pub fn send_webmentions_for_post(
    post_url: &str,
    own_domain: &str,
    bridgy_fed: bool,
) -> WebmentionReport {
    let client = Client::builder()
        .timeout(Duration::from_secs(15))
        .redirect(reqwest::redirect::Policy::limited(10))
        .user_agent("Dispatch/0.3 (webmention; +https://ejfox.com)")
        .build()
        .unwrap_or_else(|_| Client::new());

    // Fetch the published post HTML
    let html = match client.get(post_url).send().and_then(|r| r.text()) {
        Ok(h) => h,
        Err(_e) => {
            return WebmentionReport {
                source: post_url.to_string(),
                results: vec![],
                total_links: 0,
                sent: 0,
                no_endpoint: 0,
                errors: 1,
            };
        }
    };

    let mut links = extract_links(&html, own_domain);

    // Optionally add Bridgy Fed as a target
    if bridgy_fed {
        links.push("https://fed.brid.gy/".to_string());
    }

    let total_links = links.len();
    let mut results = Vec::new();
    let mut sent = 0;
    let mut no_endpoint = 0;
    let mut errors = 0;

    // Deduplicate
    links.sort();
    links.dedup();

    for target in &links {
        eprintln!("Discovering webmention endpoint for: {}", target);

        match discover_endpoint(&client, target) {
            Some(endpoint) => {
                eprintln!("  Found endpoint: {}", endpoint);
                match send_webmention(&client, &endpoint, post_url, target) {
                    Ok(status) => {
                        eprintln!("  Sent! Status: {}", status);
                        sent += 1;
                        results.push(WebmentionResult {
                            target: target.clone(),
                            endpoint: Some(endpoint),
                            status: "sent".to_string(),
                            message: Some(format!("HTTP {}", status)),
                        });
                    }
                    Err(e) => {
                        eprintln!("  Send failed: {}", e);
                        errors += 1;
                        results.push(WebmentionResult {
                            target: target.clone(),
                            endpoint: Some(endpoint),
                            status: "error".to_string(),
                            message: Some(e),
                        });
                    }
                }
            }
            None => {
                eprintln!("  No endpoint found");
                no_endpoint += 1;
                results.push(WebmentionResult {
                    target: target.clone(),
                    endpoint: None,
                    status: "no_endpoint".to_string(),
                    message: None,
                });
            }
        }
    }

    WebmentionReport {
        source: post_url.to_string(),
        results,
        total_links,
        sent,
        no_endpoint,
        errors,
    }
}

// --- Helpers ---

fn parse_link_header(header: &str) -> Option<String> {
    // Parse Link: <url>; rel="webmention"
    for part in header.split(',') {
        let part = part.trim();
        if part.contains("rel=\"webmention\"") || part.contains("rel=webmention") {
            let re = Regex::new(r"<([^>]+)>").unwrap();
            if let Some(cap) = re.captures(part) {
                return Some(cap[1].to_string());
            }
        }
    }
    None
}

fn resolve_url(base: &reqwest::Url, href: &str) -> String {
    if href.starts_with("http://") || href.starts_with("https://") {
        href.to_string()
    } else {
        base.join(href)
            .map(|u| u.to_string())
            .unwrap_or_else(|_| href.to_string())
    }
}
