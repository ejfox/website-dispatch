use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use walkdir::WalkDir;

use crate::Config;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssetUsage {
    pub post_path: String,
    pub post_title: Option<String>,
    pub line_number: usize,
    pub context: String, // surrounding text
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssetUsageMap {
    /// Map from Cloudinary public_id to list of usages
    pub by_asset: HashMap<String, Vec<AssetUsage>>,
    /// Map from post path to list of asset public_ids used
    pub by_post: HashMap<String, Vec<String>>,
    /// Total unique assets found
    pub total_assets: usize,
    /// Total posts scanned
    pub total_posts: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageScanResult {
    pub usage_map: AssetUsageMap,
    pub cloudinary_urls: Vec<String>,
    pub scan_duration_ms: u64,
}

/// Extract Cloudinary URLs from markdown content
fn extract_cloudinary_urls(content: &str) -> Vec<(String, usize, String)> {
    let mut results = Vec::new();

    // Match Cloudinary URLs in various formats
    // https://res.cloudinary.com/CLOUD_NAME/image/upload/...
    // https://res.cloudinary.com/CLOUD_NAME/video/upload/...
    let url_re = Regex::new(
        r#"https://res\.cloudinary\.com/([^/]+)/(image|video|raw)/upload/(?:[^/]+/)*([^\s\)"'\]]+)"#
    ).unwrap();

    for (line_num, line) in content.lines().enumerate() {
        for cap in url_re.captures_iter(line) {
            let full_url = cap.get(0).map(|m| m.as_str()).unwrap_or("");
            let public_id = cap.get(3).map(|m| m.as_str()).unwrap_or("");

            // Remove file extension from public_id
            let public_id = public_id
                .rsplit_once('.')
                .map(|(id, _)| id)
                .unwrap_or(public_id);

            // Get some context (trimmed line)
            let context = line.trim().chars().take(100).collect::<String>();

            results.push((public_id.to_string(), line_num + 1, context));
        }
    }

    results
}

/// Extract title from markdown content
fn extract_title(content: &str) -> Option<String> {
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("# ") {
            return Some(trimmed[2..].trim().to_string());
        }
        if trimmed.starts_with("## ") {
            return Some(trimmed[3..].trim().to_string());
        }
    }
    None
}

/// Scan all markdown files in the vault for Cloudinary URLs
pub fn scan_vault_for_usage() -> Result<UsageScanResult, String> {
    let start = std::time::Instant::now();
    let config = Config::default();

    let mut by_asset: HashMap<String, Vec<AssetUsage>> = HashMap::new();
    let mut by_post: HashMap<String, Vec<String>> = HashMap::new();
    let mut all_urls: Vec<String> = Vec::new();
    let mut total_posts = 0;

    // Scan vault
    for entry in WalkDir::new(&config.vault_path)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().map_or(false, |ext| ext == "md"))
    {
        let path = entry.path();
        let path_str = path.to_string_lossy().to_string();

        // Skip certain directories
        if path_str.contains("/templates/") ||
           path_str.contains("/.obsidian/") ||
           path_str.contains("/node_modules/") {
            continue;
        }

        if let Ok(content) = fs::read_to_string(path) {
            total_posts += 1;
            let title = extract_title(&content);
            let urls = extract_cloudinary_urls(&content);

            if !urls.is_empty() {
                let mut post_assets = Vec::new();

                for (public_id, line_num, context) in urls {
                    // Track by asset
                    by_asset
                        .entry(public_id.clone())
                        .or_insert_with(Vec::new)
                        .push(AssetUsage {
                            post_path: path_str.clone(),
                            post_title: title.clone(),
                            line_number: line_num,
                            context: context.clone(),
                        });

                    post_assets.push(public_id.clone());

                    // Reconstruct approximate URL for reference
                    all_urls.push(format!("https://res.cloudinary.com/ejf/image/upload/{}", public_id));
                }

                // Track by post
                by_post.insert(path_str.clone(), post_assets);
            }
        }
    }

    // Also scan website repo for published posts
    let blog_path = format!("{}/content/blog", config.website_repo);
    for entry in WalkDir::new(&blog_path)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().map_or(false, |ext| ext == "md"))
    {
        let path = entry.path();
        let path_str = path.to_string_lossy().to_string();

        if let Ok(content) = fs::read_to_string(path) {
            total_posts += 1;
            let title = extract_title(&content);
            let urls = extract_cloudinary_urls(&content);

            if !urls.is_empty() {
                let mut post_assets = Vec::new();

                for (public_id, line_num, context) in urls {
                    by_asset
                        .entry(public_id.clone())
                        .or_insert_with(Vec::new)
                        .push(AssetUsage {
                            post_path: path_str.clone(),
                            post_title: title.clone(),
                            line_number: line_num,
                            context,
                        });

                    post_assets.push(public_id.clone());
                }

                by_post.insert(path_str, post_assets);
            }
        }
    }

    let total_assets = by_asset.len();
    let duration = start.elapsed().as_millis() as u64;

    Ok(UsageScanResult {
        usage_map: AssetUsageMap {
            by_asset,
            by_post,
            total_assets,
            total_posts,
        },
        cloudinary_urls: all_urls,
        scan_duration_ms: duration,
    })
}

/// Get usage info for a specific asset
pub fn get_asset_usage(public_id: &str) -> Result<Vec<AssetUsage>, String> {
    let scan = scan_vault_for_usage()?;
    Ok(scan.usage_map.by_asset.get(public_id).cloned().unwrap_or_default())
}

/// Get all assets used in a specific post
pub fn get_post_assets(post_path: &str) -> Result<Vec<String>, String> {
    let scan = scan_vault_for_usage()?;
    Ok(scan.usage_map.by_post.get(post_path).cloned().unwrap_or_default())
}

/// Get list of Cloudinary folders from a list of public_ids
pub fn extract_folders(public_ids: &[String]) -> Vec<String> {
    let mut folders: Vec<String> = public_ids
        .iter()
        .filter_map(|id| {
            let parts: Vec<&str> = id.split('/').collect();
            if parts.len() > 1 {
                Some(parts[..parts.len()-1].join("/"))
            } else {
                None
            }
        })
        .collect();

    folders.sort();
    folders.dedup();
    folders
}
