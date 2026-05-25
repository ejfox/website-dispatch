//! Cloudflare R2 (S3-compatible) media uploader.
//!
//! Speaks AWS SigV4 directly against R2's S3 endpoint. We don't pull in the
//! full aws-sdk-s3 dependency tree because all we need is single-object PUT
//! and a HEAD-equivalent credential probe.
//!
//! Endpoint shape: `https://{account_id}.r2.cloudflarestorage.com/{bucket}/{key}`
//! Region: `auto`, Service: `s3`.

use chrono::{DateTime, Utc};
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::Path;
use std::time::Duration;

use crate::config::R2Creds;

type HmacSha256 = Hmac<Sha256>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct R2Asset {
    pub url: String,
    pub key: String,
    pub bytes: u64,
}

/// Resolve R2 credentials with env-var override (mirrors cloudinary's strategy).
pub fn get_creds() -> Result<R2Creds, String> {
    // Env-var override path: all five must be present.
    let env_account = std::env::var("R2_ACCOUNT_ID").ok();
    let env_key = std::env::var("R2_ACCESS_KEY_ID").ok();
    let env_secret = std::env::var("R2_SECRET_ACCESS_KEY").ok();
    let env_bucket = std::env::var("R2_BUCKET").ok();
    let env_public = std::env::var("R2_PUBLIC_URL_BASE").ok();
    if let (Some(a), Some(k), Some(s), Some(b), Some(p)) =
        (env_account, env_key, env_secret, env_bucket, env_public)
    {
        if !a.is_empty() && !k.is_empty() && !s.is_empty() && !b.is_empty() && !p.is_empty() {
            return Ok(R2Creds {
                account_id: a,
                access_key_id: k,
                secret_access_key: s,
                bucket: b,
                public_url_base: p,
            });
        }
    }

    let app_config = crate::config::get()
        .map_err(|e| format!("R2 creds unavailable — config load failed: {}", e))?;
    let creds = app_config.media.r2.ok_or(
        "R2 credentials not configured. Open Settings → Media and fill in account ID, access key, secret, bucket, and public URL.",
    )?;
    if creds.account_id.is_empty()
        || creds.access_key_id.is_empty()
        || creds.secret_access_key.is_empty()
        || creds.bucket.is_empty()
        || creds.public_url_base.is_empty()
    {
        return Err("R2 credentials incomplete. Open Settings → Media to fix.".into());
    }
    Ok(creds)
}

/// Upload a file to R2 at `{folder}/{filename}` (or just `{filename}` if no folder).
/// Returns the public URL composed from `public_url_base + key`.
pub async fn upload_file(file_path: &str, folder: Option<&str>) -> Result<R2Asset, String> {
    let creds = get_creds()?;

    let path = Path::new(file_path);
    let filename = path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| "Could not derive filename from path".to_string())?;
    let key = compose_key(folder, filename);

    let body = fs::read(file_path).map_err(|e| format!("Failed to read file: {}", e))?;
    let bytes = body.len() as u64;
    let content_type = mime_guess::from_path(path)
        .first_or_octet_stream()
        .to_string();

    put_object(&creds, &key, body, &content_type).await?;

    let url = compose_public_url(&creds.public_url_base, &key);
    Ok(R2Asset { url, key, bytes })
}

/// Cheap credential probe: HEAD on the bucket root. R2 returns 200 if creds
/// + bucket are valid, 401/403 if creds are bad, 404 if bucket doesn't exist.
pub async fn check_credentials() -> bool {
    let Ok(creds) = get_creds() else {
        return false;
    };
    let now: DateTime<Utc> = Utc::now();
    let amz_date = now.format("%Y%m%dT%H%M%SZ").to_string();
    let date_stamp = now.format("%Y%m%d").to_string();
    let host = format!("{}.r2.cloudflarestorage.com", creds.account_id);
    let canonical_uri = format!("/{}", uri_encode_segment(&creds.bucket));

    let empty_hash = sha256_hex(b"");
    let canonical_headers = format!(
        "host:{}\nx-amz-content-sha256:{}\nx-amz-date:{}\n",
        host, empty_hash, amz_date
    );
    let signed_headers = "host;x-amz-content-sha256;x-amz-date";
    let canonical_request = format!(
        "HEAD\n{}\n\n{}\n{}\n{}",
        canonical_uri, canonical_headers, signed_headers, empty_hash
    );

    let scope = format!("{}/auto/s3/aws4_request", date_stamp);
    let string_to_sign = format!(
        "AWS4-HMAC-SHA256\n{}\n{}\n{}",
        amz_date,
        scope,
        sha256_hex(canonical_request.as_bytes())
    );
    let signing_key = derive_signing_key(&creds.secret_access_key, &date_stamp, "auto", "s3");
    let signature = hex::encode(hmac_sha256(&signing_key, string_to_sign.as_bytes()));
    let authorization = format!(
        "AWS4-HMAC-SHA256 Credential={}/{}, SignedHeaders={}, Signature={}",
        creds.access_key_id, scope, signed_headers, signature
    );

    let url = format!("https://{}{}", host, canonical_uri);
    let client = match reqwest::Client::builder().timeout(Duration::from_secs(10)).build() {
        Ok(c) => c,
        Err(_) => return false,
    };
    match client
        .head(&url)
        .header("host", &host)
        .header("x-amz-content-sha256", &empty_hash)
        .header("x-amz-date", &amz_date)
        .header("authorization", &authorization)
        .send()
        .await
    {
        Ok(resp) => resp.status().is_success(),
        Err(_) => false,
    }
}

/// PUT one object. Retries up to 3 times with backoff on transient failures.
async fn put_object(
    creds: &R2Creds,
    key: &str,
    body: Vec<u8>,
    content_type: &str,
) -> Result<(), String> {
    let host = format!("{}.r2.cloudflarestorage.com", creds.account_id);
    let canonical_uri = format!(
        "/{}/{}",
        uri_encode_segment(&creds.bucket),
        uri_encode_path(key)
    );
    let url = format!("https://{}{}", host, canonical_uri);

    let mut last_error = String::new();
    for attempt in 0..3 {
        if attempt > 0 {
            tokio::time::sleep(Duration::from_millis(500 * (1 << attempt))).await;
        }

        let now: DateTime<Utc> = Utc::now();
        let amz_date = now.format("%Y%m%dT%H%M%SZ").to_string();
        let date_stamp = now.format("%Y%m%d").to_string();
        let payload_hash = sha256_hex(&body);

        // Headers must be in lowercase, sorted order in the canonical block.
        let canonical_headers = format!(
            "content-length:{}\ncontent-type:{}\nhost:{}\nx-amz-content-sha256:{}\nx-amz-date:{}\n",
            body.len(),
            content_type,
            host,
            payload_hash,
            amz_date
        );
        let signed_headers = "content-length;content-type;host;x-amz-content-sha256;x-amz-date";
        let canonical_request = format!(
            "PUT\n{}\n\n{}\n{}\n{}",
            canonical_uri, canonical_headers, signed_headers, payload_hash
        );
        let scope = format!("{}/auto/s3/aws4_request", date_stamp);
        let string_to_sign = format!(
            "AWS4-HMAC-SHA256\n{}\n{}\n{}",
            amz_date,
            scope,
            sha256_hex(canonical_request.as_bytes())
        );
        let signing_key =
            derive_signing_key(&creds.secret_access_key, &date_stamp, "auto", "s3");
        let signature = hex::encode(hmac_sha256(&signing_key, string_to_sign.as_bytes()));
        let authorization = format!(
            "AWS4-HMAC-SHA256 Credential={}/{}, SignedHeaders={}, Signature={}",
            creds.access_key_id, scope, signed_headers, signature
        );

        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(120))
            .build()
            .map_err(|e| format!("Failed to build HTTP client: {}", e))?;

        let resp = client
            .put(&url)
            .header("host", &host)
            .header("content-type", content_type)
            .header("content-length", body.len().to_string())
            .header("x-amz-content-sha256", &payload_hash)
            .header("x-amz-date", &amz_date)
            .header("authorization", &authorization)
            .body(body.clone())
            .send()
            .await;

        match resp {
            Ok(r) if r.status().is_success() => return Ok(()),
            Ok(r) => {
                let status = r.status();
                let body_txt = r.text().await.unwrap_or_default();
                last_error = format!("HTTP {}: {}", status, body_txt);
                // 4xx (other than 408/429) won't be fixed by retry.
                if status.is_client_error()
                    && status != reqwest::StatusCode::REQUEST_TIMEOUT
                    && status != reqwest::StatusCode::TOO_MANY_REQUESTS
                {
                    break;
                }
            }
            Err(e) => {
                last_error = format!("Request failed: {}", e);
            }
        }
    }
    Err(last_error)
}

fn compose_key(folder: Option<&str>, filename: &str) -> String {
    match folder {
        Some(f) if !f.is_empty() => format!("{}/{}", f.trim_matches('/'), filename),
        _ => filename.to_string(),
    }
}

fn compose_public_url(base: &str, key: &str) -> String {
    let trimmed = base.trim_end_matches('/');
    format!("{}/{}", trimmed, key)
}

/// SHA-256 of arbitrary bytes, lowercase hex.
fn sha256_hex(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    hex::encode(hasher.finalize())
}

fn hmac_sha256(key: &[u8], data: &[u8]) -> Vec<u8> {
    let mut mac = HmacSha256::new_from_slice(key).expect("HMAC accepts any key length");
    mac.update(data);
    mac.finalize().into_bytes().to_vec()
}

fn derive_signing_key(secret: &str, date_stamp: &str, region: &str, service: &str) -> Vec<u8> {
    let k_date = hmac_sha256(format!("AWS4{}", secret).as_bytes(), date_stamp.as_bytes());
    let k_region = hmac_sha256(&k_date, region.as_bytes());
    let k_service = hmac_sha256(&k_region, service.as_bytes());
    hmac_sha256(&k_service, b"aws4_request")
}

/// RFC-3986 percent-encode a single path segment (does NOT preserve `/`).
fn uri_encode_segment(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        if is_unreserved(b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{:02X}", b));
        }
    }
    out
}

/// Percent-encode a path that contains `/` separators we want to preserve.
fn uri_encode_path(s: &str) -> String {
    s.split('/')
        .map(uri_encode_segment)
        .collect::<Vec<_>>()
        .join("/")
}

fn is_unreserved(b: u8) -> bool {
    matches!(b, b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signing_key_matches_aws_test_vector() {
        // From https://docs.aws.amazon.com/general/latest/gr/signature-v4-examples.html
        // (using us-east-1/iam, but the derivation is identical for auto/s3)
        let key = derive_signing_key(
            "wJalrXUtnFEMI/K7MDENG+bPxRfiCYEXAMPLEKEY",
            "20150830",
            "us-east-1",
            "iam",
        );
        assert_eq!(
            hex::encode(&key),
            "c4afb1cc5771d871763a393e44b703571b55cc28424d1a5e86da6ed3c154a4b9"
        );
    }

    #[test]
    fn key_composition() {
        assert_eq!(compose_key(Some("blog/2026"), "foo.png"), "blog/2026/foo.png");
        assert_eq!(compose_key(Some("/blog/2026/"), "foo.png"), "blog/2026/foo.png");
        assert_eq!(compose_key(None, "foo.png"), "foo.png");
        assert_eq!(compose_key(Some(""), "foo.png"), "foo.png");
    }

    #[test]
    fn public_url_handles_trailing_slash() {
        assert_eq!(
            compose_public_url("https://r2.ejfox.com/", "blog/2026/foo.png"),
            "https://r2.ejfox.com/blog/2026/foo.png"
        );
        assert_eq!(
            compose_public_url("https://r2.ejfox.com", "blog/2026/foo.png"),
            "https://r2.ejfox.com/blog/2026/foo.png"
        );
    }

    #[test]
    fn uri_encoding_preserves_path_separators() {
        assert_eq!(uri_encode_path("blog/2026/foo bar.png"), "blog/2026/foo%20bar.png");
        assert_eq!(uri_encode_segment("foo bar"), "foo%20bar");
    }
}
