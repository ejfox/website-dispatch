//! Omnipublish — fan a published note out to every enabled network.
//!
//! * One entry point (`syndicate_everywhere`) posts to each network
//!   independently: one failing never blocks the others.
//! * Bounded retry with backoff on transient errors (network / 5xx / 429).
//! * Idempotent: a network that already has a URL in the note's
//!   `syndication:` frontmatter is skipped, so re-running never double-posts.
//! * Write-back: each success is merged into the VAULT note's frontmatter
//!   immediately (minimal text edit — body and other keys untouched), so
//!   republishing carries the links to the site.
//! * Dry run (`dry_run` flag or DISPATCH_SYNDICATE_DRY_RUN=1): every payload is
//!   built and returned/logged, but no network call and no file write happens.
//! * Images are never posted without alt text: a missing `image_alt` is
//!   generated with alttext.rs and recorded back; if that fails the image is
//!   dropped from the posts.

use crate::bluesky::{self, NetError};
use crate::frontmatter_edit::{self as fm, SyndicationEntry};
use crate::syndication;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::{LazyLock, Mutex};
use std::time::Duration;

pub const NETWORKS: [&str; 2] = ["mastodon", "bluesky"];
const MAX_ATTEMPTS: u32 = 3;
const MASTODON_MAX_IMAGE_BYTES: usize = 8 * 1024 * 1024;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default, Deserialize)]
pub struct OmniRequest {
    pub source_path: String,
    /// Networks to post to. None = every configured network (all, in dry run).
    #[serde(default)]
    pub networks: Option<Vec<String>>,
    /// Per-network text overrides (e.g. edited in the wizard).
    #[serde(default)]
    pub texts: Option<HashMap<String, String>>,
    #[serde(default)]
    pub dry_run: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NetworkOutcome {
    pub network: String,
    /// "posted" | "skipped" | "failed" | "not_configured" | "dry_run"
    pub status: String,
    pub url: Option<String>,
    pub error: Option<String>,
    pub attempts: u32,
    /// The request sequence that was (or, in dry run, would be) sent.
    pub payload: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OmniReport {
    pub dry_run: bool,
    pub source_path: String,
    pub post_url: String,
    pub results: Vec<NetworkOutcome>,
    pub image_alt: Option<String>,
    pub image_alt_generated: bool,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Visibility {
    Public,
    Unlisted,
    Protected,
}

/// Everything the payload builders need, parsed from the note.
#[derive(Debug, Clone)]
pub struct NotePlan {
    pub post_url: String,
    pub title: String,
    pub description: String,
    pub lead: String,
    pub hashtags: Vec<String>,
    pub image: Option<String>,
    pub image_alt: Option<String>,
    pub visibility: Visibility,
    pub draft: bool,
    pub existing: Vec<SyndicationEntry>,
}

pub fn env_dry_run() -> bool {
    matches!(
        std::env::var("DISPATCH_SYNDICATE_DRY_RUN")
            .unwrap_or_default()
            .to_ascii_lowercase()
            .as_str(),
        "1" | "true" | "yes"
    )
}

pub fn is_configured(network: &str) -> bool {
    match network {
        "mastodon" => syndication::mastodon_configured(),
        "bluesky" => bluesky::is_configured(),
        _ => false,
    }
}

fn missing_env_hint(network: &str) -> &'static str {
    match network {
        "mastodon" => "set MASTODON_ACCESS_TOKEN (and MASTODON_INSTANCE) in .env",
        "bluesky" => "set BLUESKY_HANDLE and BLUESKY_APP_PASSWORD in .env",
        _ => "unknown network",
    }
}

// ---------------------------------------------------------------------------
// Planning (pure)
// ---------------------------------------------------------------------------

fn first_paragraph(body: &str) -> String {
    let mut para = Vec::new();
    for line in body.lines() {
        let t = line.trim();
        if t.is_empty() {
            if !para.is_empty() {
                break;
            }
            continue;
        }
        if t.starts_with('#')
            || t.starts_with("![")
            || t.starts_with('<')
            || t.starts_with('>')
            || t.starts_with("---")
            || t.starts_with("```")
        {
            if !para.is_empty() {
                break;
            }
            continue;
        }
        para.push(t);
    }
    let text = para.join(" ");
    // [text](url) → text
    let text = crate::patterns::MD_LINK
        .replace_all(&text, "$2")
        .to_string();
    let text = text.replace(['*', '_', '`'], "");
    bluesky::truncate_chars(text.trim(), 280)
}

fn hashtags_from(tags: &[String]) -> Vec<String> {
    let skip = ["post", "weeknote", "blog", "dispatch"];
    tags.iter()
        .filter(|t| !skip.contains(&t.to_ascii_lowercase().as_str()))
        .map(|t| {
            t.chars()
                .filter(|c| c.is_alphanumeric() || *c == '_')
                .collect::<String>()
        })
        .filter(|t| !t.is_empty() && !t.chars().all(|c| c.is_ascii_digit()))
        .take(3)
        .map(|t| format!("#{}", t))
        .collect()
}

pub fn plan_from_content(content: &str, slug: &str, post_url: &str) -> NotePlan {
    let body = fm::body(content);
    let title = fm::get_scalar(content, "title")
        .or_else(|| {
            body.lines()
                .map(str::trim)
                .find(|l| l.starts_with("# "))
                .map(|l| l.trim_start_matches("# ").trim().to_string())
        })
        .unwrap_or_else(|| slug.to_string());
    let dek = fm::get_scalar(content, "dek");
    let description = dek.clone().unwrap_or_else(|| first_paragraph(body));
    let lead = dek.unwrap_or_else(|| title.clone());
    let image = fm::get_scalar(content, "image").filter(|u| u.starts_with("http"));
    let visibility = if fm::get_scalar(content, "password").is_some() {
        Visibility::Protected
    } else if fm::get_bool(content, "unlisted") {
        Visibility::Unlisted
    } else {
        Visibility::Public
    };
    NotePlan {
        post_url: post_url.to_string(),
        title,
        description,
        lead,
        hashtags: hashtags_from(&fm::get_list(content, "tags")),
        image,
        image_alt: fm::get_scalar(content, "image_alt"),
        visibility,
        draft: fm::get_bool(content, "draft"),
        existing: fm::read_syndication(content),
    }
}

/// Why a note must not be syndicated at all, if any reason applies.
pub fn refuse_reason(plan: &NotePlan) -> Option<String> {
    if plan.visibility == Visibility::Protected {
        return Some("password-protected notes are never syndicated".into());
    }
    if plan.draft {
        return Some("note has draft: true".into());
    }
    None
}

pub fn default_text(network: &str, plan: &NotePlan) -> String {
    match network {
        "bluesky" => bluesky::compose_text(&plan.lead, &plan.post_url, &plan.hashtags),
        _ => {
            let mut parts = vec![plan.lead.clone(), plan.post_url.clone()];
            if !plan.hashtags.is_empty() {
                parts.push(plan.hashtags.join(" "));
            }
            parts.join("\n\n")
        }
    }
}

/// Image the posts will carry — only when it has alt text.
fn postable_image(plan: &NotePlan) -> Option<(&str, &str)> {
    match (&plan.image, &plan.image_alt) {
        (Some(img), Some(alt)) if !alt.trim().is_empty() => Some((img.as_str(), alt.as_str())),
        _ => None,
    }
}

pub fn mastodon_status_json(plan: &NotePlan, text: &str, media_ids: &[String]) -> Value {
    let visibility = match plan.visibility {
        Visibility::Public => "public",
        _ => "unlisted",
    };
    let mut status = json!({ "status": text, "visibility": visibility });
    if !media_ids.is_empty() {
        status["media_ids"] = json!(media_ids);
    }
    status
}

pub fn bluesky_record_json(
    plan: &NotePlan,
    text: &str,
    blob: Option<Value>,
    created_at: &str,
) -> Value {
    let embed = if !plan.post_url.is_empty() {
        Some(bluesky::external_embed(
            &plan.post_url,
            &plan.title,
            &plan.description,
            blob,
        ))
    } else {
        match (blob, postable_image(plan)) {
            (Some(b), Some((_, alt))) => Some(bluesky::images_embed(b, alt)),
            _ => None,
        }
    };
    bluesky::post_record(text, embed, created_at)
}

/// The full request sequence for a network, with placeholders for values only
/// the network can supply (session DID, blob refs, media ids). Never includes
/// credentials. This is what dry run returns.
pub fn describe_requests(network: &str, plan: &NotePlan, text: &str) -> Value {
    let image = postable_image(plan);
    match network {
        "mastodon" => {
            let mut reqs = Vec::new();
            let mut media_ids = Vec::new();
            if let Some((img, alt)) = image {
                reqs.push(json!({
                    "method": "POST",
                    "endpoint": "/api/v2/media",
                    "multipart": {
                        "file": format!("<bytes of {}>", img),
                        "description": alt.chars().take(syndication::MASTODON_ALT_MAX).collect::<String>(),
                    }
                }));
                media_ids.push("<media id from /api/v2/media>".to_string());
            }
            reqs.push(json!({
                "method": "POST",
                "endpoint": "/api/v1/statuses",
                "json": mastodon_status_json(plan, text, &media_ids),
            }));
            json!({ "network": "mastodon", "requests": reqs })
        }
        "bluesky" => {
            let handle =
                bluesky::configured_handle().unwrap_or_else(|| "<BLUESKY_HANDLE unset>".into());
            let mut reqs = vec![json!({
                "method": "POST",
                "endpoint": "/xrpc/com.atproto.server.createSession",
                "json": { "identifier": handle, "password": "<BLUESKY_APP_PASSWORD redacted>" }
            })];
            let blob = image.map(|(img, _)| {
                reqs.push(json!({
                    "method": "POST",
                    "endpoint": "/xrpc/com.atproto.repo.uploadBlob",
                    "body": format!("<bytes of {}>", img),
                }));
                bluesky::placeholder_blob(img)
            });
            let record = bluesky_record_json(plan, text, blob, &now_iso());
            reqs.push(json!({
                "method": "POST",
                "endpoint": "/xrpc/com.atproto.repo.createRecord",
                "json": {
                    "repo": "<did from createSession>",
                    "collection": "app.bsky.feed.post",
                    "record": record,
                }
            }));
            json!({ "network": "bluesky", "requests": reqs })
        }
        other => json!({ "network": other, "error": "unknown network" }),
    }
}

fn now_iso() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

// ---------------------------------------------------------------------------
// Retry + fan-out (network-agnostic, testable)
// ---------------------------------------------------------------------------

/// Run `f` up to `max` times, sleeping 2s, 4s, 8s… (capped at 8s) between
/// transient failures. Fatal errors return immediately. Returns attempts used.
pub fn with_retry(
    max: u32,
    sleeper: &dyn Fn(Duration),
    mut f: impl FnMut() -> Result<String, NetError>,
) -> (Result<String, NetError>, u32) {
    let mut attempt = 0;
    loop {
        attempt += 1;
        match f() {
            Ok(v) => return (Ok(v), attempt),
            Err(e) if e.retryable && attempt < max => {
                let secs = (2u64 << (attempt - 1)).min(8);
                log::warn!(
                    "omnipublish: transient error ({}), retry in {}s",
                    e.message,
                    secs
                );
                sleeper(Duration::from_secs(secs));
            }
            Err(e) => return (Err(e), attempt),
        }
    }
}

pub fn normalize_networks(requested: &[String]) -> Vec<String> {
    let mut seen = HashSet::new();
    requested
        .iter()
        .map(|n| n.trim().to_ascii_lowercase())
        .filter(|n| !n.is_empty() && seen.insert(n.clone()))
        .collect()
}

pub struct FanOutEnv<'a> {
    pub dry_run: bool,
    pub configured: &'a dyn Fn(&str) -> bool,
    pub texts: &'a HashMap<String, String>,
    pub sleeper: &'a dyn Fn(Duration),
}

/// Post to each network independently. `poster(network, text)` performs the
/// real post and returns the public URL; `record(entry)` persists a success.
pub fn fan_out(
    plan: &NotePlan,
    networks: &[String],
    env: &FanOutEnv,
    poster: &mut dyn FnMut(&str, &str) -> Result<String, NetError>,
    record: &mut dyn FnMut(&SyndicationEntry) -> Result<(), String>,
    warnings: &mut Vec<String>,
) -> Vec<NetworkOutcome> {
    let mut out = Vec::new();
    for network in normalize_networks(networks) {
        let mut outcome = NetworkOutcome {
            network: network.clone(),
            status: "failed".into(),
            url: None,
            error: None,
            attempts: 0,
            payload: None,
        };
        if !NETWORKS.contains(&network.as_str()) {
            outcome.error = Some(format!("unknown network '{}'", network));
            out.push(outcome);
            continue;
        }
        // Idempotency: already syndicated → skip, report the existing link.
        if let Some(e) = plan
            .existing
            .iter()
            .find(|e| e.network.eq_ignore_ascii_case(&network) && !e.url.is_empty())
        {
            outcome.status = "skipped".into();
            outcome.url = Some(e.url.clone());
            outcome.error = Some("already syndicated (in note frontmatter)".into());
            out.push(outcome);
            continue;
        }
        if let Some(reason) = refuse_reason(plan) {
            outcome.status = "skipped".into();
            outcome.error = Some(reason);
            out.push(outcome);
            continue;
        }
        if plan.visibility == Visibility::Unlisted && network == "bluesky" {
            outcome.status = "skipped".into();
            outcome.error = Some("unlisted note: Bluesky has no unlisted visibility".into());
            out.push(outcome);
            continue;
        }

        let text = env
            .texts
            .get(&network)
            .filter(|t| !t.trim().is_empty())
            .cloned()
            .unwrap_or_else(|| default_text(&network, plan));
        let text = if network == "bluesky" {
            bluesky::fit_text(&text)
        } else {
            text
        };
        let payload = describe_requests(&network, plan, &text);

        if env.dry_run {
            log::warn!(
                "omnipublish DRY RUN {}: {}",
                network,
                serde_json::to_string(&payload).unwrap_or_default()
            );
            outcome.status = "dry_run".into();
            outcome.payload = Some(payload);
            out.push(outcome);
            continue;
        }
        if !(env.configured)(&network) {
            outcome.status = "not_configured".into();
            outcome.error = Some(missing_env_hint(&network).into());
            out.push(outcome);
            continue;
        }

        let (result, attempts) = with_retry(MAX_ATTEMPTS, env.sleeper, || poster(&network, &text));
        outcome.attempts = attempts;
        outcome.payload = Some(payload);
        match result {
            Ok(url) => {
                let entry = SyndicationEntry {
                    network: network.clone(),
                    url: url.clone(),
                };
                if let Err(e) = record(&entry) {
                    warnings.push(format!(
                        "Posted to {} but could not write the link back to the note ({}). \
                         Add `{}: {}` under syndication: by hand before re-running, or it will post again.",
                        network, e, network, url
                    ));
                }
                outcome.status = "posted".into();
                outcome.url = Some(url);
            }
            Err(e) => {
                outcome.error = Some(e.message);
            }
        }
        out.push(outcome);
    }
    out
}

// ---------------------------------------------------------------------------
// Live posting
// ---------------------------------------------------------------------------

fn cloudinary_variant(url: &str, transform: &str) -> Option<String> {
    if !url.contains("res.cloudinary.com/") || !url.contains("/image/upload/") {
        return None;
    }
    let (head, tail) = url.split_once("/image/upload/")?;
    Some(format!("{}/image/upload/{}/{}", head, transform, tail))
}

/// Download an image, preferring Cloudinary-resized JPEGs so it fits `max_bytes`.
fn fetch_image(
    client: &reqwest::blocking::Client,
    url: &str,
    max_bytes: usize,
) -> Result<(Vec<u8>, String), NetError> {
    let mut candidates: Vec<String> = [
        "c_limit,w_1600,f_jpg,q_auto:good",
        "c_limit,w_1000,f_jpg,q_70",
    ]
    .iter()
    .filter_map(|t| cloudinary_variant(url, t))
    .collect();
    candidates.push(url.to_string());

    let mut last_err = NetError::fatal(format!("image {} too large", url));
    for candidate in candidates {
        let resp = match client.get(&candidate).send() {
            Ok(r) => r,
            Err(e) => {
                last_err = NetError::transient(format!("image fetch failed: {}", e));
                continue;
            }
        };
        if !resp.status().is_success() {
            last_err = NetError::from_status("image fetch", resp.status(), "");
            continue;
        }
        let mime = resp
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .map(|v| v.split(';').next().unwrap_or("").trim().to_string())
            .filter(|m| m.starts_with("image/"))
            .unwrap_or_else(|| "image/jpeg".into());
        let bytes = resp
            .bytes()
            .map_err(|e| NetError::transient(format!("image read failed: {}", e)))?
            .to_vec();
        if bytes.len() <= max_bytes {
            return Ok((bytes, mime));
        }
    }
    Err(last_err)
}

/// Posts to real networks. Caches the Bluesky session and downloaded image.
pub struct LivePoster {
    client: reqwest::blocking::Client,
    plan: NotePlan,
    bsky_session: Option<bluesky::Session>,
    pub warnings: Vec<String>,
}

impl LivePoster {
    pub fn new(plan: NotePlan) -> Self {
        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(60))
            .user_agent("Dispatch (ejfox.com)")
            .build()
            .unwrap_or_default();
        LivePoster {
            client,
            plan,
            bsky_session: None,
            warnings: Vec::new(),
        }
    }

    pub fn post(&mut self, network: &str, text: &str) -> Result<String, NetError> {
        match network {
            "mastodon" => self.post_mastodon(text),
            "bluesky" => self.post_bluesky(text),
            other => Err(NetError::fatal(format!("unknown network {}", other))),
        }
    }

    fn post_mastodon(&mut self, text: &str) -> Result<String, NetError> {
        let mut media_ids = Vec::new();
        if let Some((img, alt)) = postable_image(&self.plan) {
            match fetch_image(&self.client, img, MASTODON_MAX_IMAGE_BYTES) {
                Ok((bytes, mime)) => {
                    media_ids.push(syndication::mastodon_upload_media(
                        &self.client,
                        bytes,
                        &mime,
                        alt,
                    )?);
                }
                Err(e) => self
                    .warnings
                    .push(format!("Mastodon: posting without image ({})", e.message)),
            }
        }
        let status = mastodon_status_json(&self.plan, text, &media_ids);
        syndication::mastodon_post_status(&self.client, &status)
    }

    fn post_bluesky(&mut self, text: &str) -> Result<String, NetError> {
        let cfg = bluesky::config_from_env().map_err(NetError::fatal)?;
        if self.bsky_session.is_none() {
            self.bsky_session = Some(bluesky::create_session(&self.client, &cfg)?);
        }
        let session = self.bsky_session.as_ref().expect("session set above");
        let mut blob = None;
        if let Some((img, _alt)) = postable_image(&self.plan) {
            match fetch_image(&self.client, img, bluesky::MAX_BLOB_BYTES).and_then(
                |(bytes, mime)| bluesky::upload_blob(&self.client, &cfg, session, bytes, &mime),
            ) {
                Ok(b) => blob = Some(b),
                Err(e) => self.warnings.push(format!(
                    "Bluesky: link card without thumbnail ({})",
                    e.message
                )),
            }
        }
        let record = bluesky_record_json(&self.plan, text, blob, &now_iso());
        let at_uri = bluesky::create_post(&self.client, &cfg, session, &record)?;
        bluesky::web_url(&at_uri, &session.handle)
            .ok_or_else(|| NetError::fatal(format!("unexpected at-uri {}", at_uri)))
    }
}

// ---------------------------------------------------------------------------
// Note resolution + write-back
// ---------------------------------------------------------------------------

static IN_FLIGHT: LazyLock<Mutex<HashSet<String>>> = LazyLock::new(|| Mutex::new(HashSet::new()));

/// Prevents two concurrent runs for the same note (double-click → double post).
struct InFlightGuard(String);

impl InFlightGuard {
    fn acquire(key: &str) -> Result<Self, String> {
        let mut set = IN_FLIGHT.lock().map_err(|e| e.to_string())?;
        if !set.insert(key.to_string()) {
            return Err("Syndication already running for this note".into());
        }
        Ok(InFlightGuard(key.to_string()))
    }
}

impl Drop for InFlightGuard {
    fn drop(&mut self) {
        if let Ok(mut set) = IN_FLIGHT.lock() {
            set.remove(&self.0);
        }
    }
}

/// Merge one entry into the vault note (re-reads the file first so edits made
/// while posting aren't clobbered).
pub fn write_back(path: &str, entry: &SyndicationEntry) -> Result<(), String> {
    let content = std::fs::read_to_string(path).map_err(|e| format!("read {}: {}", path, e))?;
    let updated = fm::merge_syndication(&content, std::slice::from_ref(entry))?;
    if updated != content {
        std::fs::write(path, updated).map_err(|e| format!("write {}: {}", path, e))?;
    }
    Ok(())
}

fn write_image_alt(path: &str, alt: &str) -> Result<(), String> {
    let content = std::fs::read_to_string(path).map_err(|e| format!("read {}: {}", path, e))?;
    let updated = fm::set_scalar_if_absent(&content, "image_alt", alt, Some("image"))?;
    if updated != content {
        std::fs::write(path, updated).map_err(|e| format!("write {}: {}", path, e))?;
    }
    Ok(())
}

/// (content, slug, post_url, is_published) for a vault note.
fn resolve_note(source_path: &str) -> Result<(String, String, String, bool), String> {
    let app = crate::config::get()?;
    let target = crate::config::default_target()?;
    let normalized = source_path.replace('\\', "/");
    if !normalized.starts_with(&app.vault.path) {
        return Err("Note must live in the vault".into());
    }
    let content =
        std::fs::read_to_string(source_path).map_err(|e| format!("Failed to read note: {}", e))?;
    let slug = Path::new(source_path)
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .ok_or("Note has no filename")?;
    if crate::dispatch::is_dispatch_note(&app.vault.path, &normalized) {
        let published = Path::new(&crate::dispatch::dest_path(&target.repo_path, &slug)).exists();
        return Ok((
            content,
            slug.clone(),
            crate::dispatch::public_url(&target.domain, &slug),
            published,
        ));
    }
    let (url, _, _) = crate::vault::find_published_info_for_target(&target, &slug);
    let published = url.is_some();
    let url = url.unwrap_or_else(|| {
        format!(
            "{}/blog/{}/{}",
            target.domain.trim_end_matches('/'),
            chrono::Utc::now().format("%Y"),
            slug
        )
    });
    Ok((content, slug, url, published))
}

/// Fan a vault note out to every requested/configured network.
pub async fn syndicate_everywhere(req: OmniRequest) -> Result<OmniReport, String> {
    let _guard = InFlightGuard::acquire(&req.source_path)?;
    let dry_run = req.dry_run || env_dry_run();
    let (content, slug, post_url, published) = resolve_note(&req.source_path)?;
    if !published && !dry_run {
        return Err("Not published yet — publish the note before syndicating".into());
    }
    let mut plan = plan_from_content(&content, &slug, &post_url);
    if let Some(reason) = refuse_reason(&plan) {
        return Err(format!("Not syndicating: {}", reason));
    }

    let mut warnings = Vec::new();
    let mut image_alt_generated = false;
    if let (Some(img), None) = (plan.image.clone(), plan.image_alt.as_ref()) {
        if dry_run {
            warnings.push(
                "image has no image_alt — a live run generates one (alttext) and writes it to frontmatter; \
                 this dry run shows posts without the image"
                    .into(),
            );
        } else {
            match crate::alttext::generate_alt_for_url(&img).await {
                Ok(alt) => {
                    if let Err(e) = write_image_alt(&req.source_path, &alt) {
                        warnings.push(format!("Generated image_alt but could not save it: {}", e));
                    }
                    plan.image_alt = Some(alt);
                    image_alt_generated = true;
                }
                Err(e) => {
                    warnings.push(format!(
                        "No image_alt and alt-text generation failed ({}); posting without the image",
                        e
                    ));
                    plan.image = None;
                }
            }
        }
    }

    let networks: Vec<String> = match &req.networks {
        Some(n) => n.clone(),
        None if dry_run => NETWORKS.iter().map(|s| s.to_string()).collect(),
        None => NETWORKS
            .iter()
            .filter(|n| is_configured(n))
            .map(|s| s.to_string())
            .collect(),
    };
    let texts = req.texts.clone().unwrap_or_default();
    let source_path = req.source_path.clone();
    let image_alt = plan.image_alt.clone();

    let (results, warnings) = tauri::async_runtime::spawn_blocking(move || {
        let mut warnings = warnings;
        let env = FanOutEnv {
            dry_run,
            configured: &is_configured,
            texts: &texts,
            sleeper: &std::thread::sleep,
        };
        let mut poster = LivePoster::new(plan.clone());
        let path = source_path.clone();
        let results = fan_out(
            &plan,
            &networks,
            &env,
            &mut |n, t| poster.post(n, t),
            &mut |entry| write_back(&path, entry),
            &mut warnings,
        );
        warnings.append(&mut poster.warnings);
        (results, warnings)
    })
    .await
    .map_err(|e| format!("syndication task failed: {}", e))?;

    Ok(OmniReport {
        dry_run,
        source_path: req.source_path,
        post_url,
        results,
        image_alt,
        image_alt_generated,
        warnings,
    })
}

/// Which networks have credentials (no network calls).
pub fn network_status() -> HashMap<String, bool> {
    NETWORKS
        .iter()
        .map(|n| (n.to_string(), is_configured(n)))
        .collect()
}

// ---------------------------------------------------------------------------
// Queue integration
// ---------------------------------------------------------------------------

/// Find the vault note for a queue slug ("2026/foo" or "foo"). Prefers the
/// dispatch/ copy when the post URL is a /dispatch/ URL.
pub fn find_note_by_slug(slug: &str, post_url: &str) -> Option<String> {
    let app = crate::config::get().ok()?;
    let stem = slug.rsplit('/').next()?.to_string();
    let want_dispatch = post_url.contains("/dispatch/");
    let mut fallback = None;
    for entry in walkdir::WalkDir::new(&app.vault.path)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
    {
        let p = entry.path();
        if p.file_stem()
            .map(|s| s.to_string_lossy() != stem.as_str())
            .unwrap_or(true)
            || p.extension().map(|e| e != "md").unwrap_or(true)
        {
            continue;
        }
        let ps = p.to_string_lossy().to_string();
        let is_dispatch = crate::dispatch::is_dispatch_note(&app.vault.path, &ps);
        let in_blog = ps.contains("/blog/") || ps.contains("/week-notes/");
        if is_dispatch == want_dispatch && (is_dispatch || in_blog) {
            return Some(ps);
        }
        if (is_dispatch || in_blog) && fallback.is_none() {
            fallback = Some(ps);
        }
    }
    fallback
}

/// Send one queue item through the same machinery (idempotency, alt text,
/// media, retry, write-back). Blocking — call off the async runtime.
pub fn send_queue_item(
    network: &str,
    post_slug: &str,
    post_title: &str,
    post_url: &str,
    platform_text: &str,
    media_url: Option<&str>,
) -> syndication::SyndicationResult {
    let fail = |e: String| syndication::SyndicationResult {
        platform: network.to_string(),
        success: false,
        url: None,
        error: Some(e),
    };
    let note_path = find_note_by_slug(post_slug, post_url);
    let content = note_path
        .as_deref()
        .and_then(|p| std::fs::read_to_string(p).ok());
    let stem = post_slug.rsplit('/').next().unwrap_or(post_slug);
    let mut plan = match &content {
        Some(c) => plan_from_content(c, stem, post_url),
        None => NotePlan {
            post_url: post_url.to_string(),
            title: post_title.to_string(),
            description: String::new(),
            lead: post_title.to_string(),
            hashtags: vec![],
            image: None,
            image_alt: None,
            visibility: Visibility::Public,
            draft: false,
            existing: vec![],
        },
    };
    plan.post_url = post_url.to_string();
    if plan.title.is_empty() {
        plan.title = post_title.to_string();
    }

    // Queue-picked media (e.g. OG card) overrides the note image.
    if let Some(m) = media_url.filter(|m| !m.is_empty()) {
        if plan.image.as_deref() != Some(m) {
            plan.image = Some(m.to_string());
            plan.image_alt = None;
        }
    }
    let dry_run = env_dry_run();
    if plan.image.is_some() && plan.image_alt.is_none() && !dry_run {
        let img = plan.image.clone().unwrap_or_default();
        match tauri::async_runtime::block_on(crate::alttext::generate_alt_for_url(&img)) {
            Ok(alt) => {
                // Only the note's own image gets its alt recorded in frontmatter.
                if media_url.is_none() {
                    if let Some(p) = &note_path {
                        let _ = write_image_alt(p, &alt);
                    }
                }
                plan.image_alt = Some(alt);
            }
            Err(e) => {
                log::warn!(
                    "queue {}: no alt text ({}); sending without image",
                    network,
                    e
                );
                plan.image = None;
            }
        }
    }

    let mut texts = HashMap::new();
    texts.insert(network.to_string(), platform_text.to_string());
    let env = FanOutEnv {
        dry_run,
        configured: &is_configured,
        texts: &texts,
        sleeper: &std::thread::sleep,
    };
    let mut poster = LivePoster::new(plan.clone());
    let mut warnings = Vec::new();
    let results = fan_out(
        &plan,
        &[network.to_string()],
        &env,
        &mut |n, t| poster.post(n, t),
        &mut |entry| match &note_path {
            Some(p) => write_back(p, entry),
            None => Err("vault note not found".into()),
        },
        &mut warnings,
    );
    for w in warnings.iter().chain(poster.warnings.iter()) {
        log::warn!("queue {}: {}", network, w);
    }
    let Some(r) = results.into_iter().next() else {
        return fail("no result".into());
    };
    match r.status.as_str() {
        "posted" => syndication::SyndicationResult {
            platform: network.to_string(),
            success: true,
            url: r.url,
            error: None,
        },
        // Already syndicated: treat as sent so the queue item settles.
        "skipped" if r.url.is_some() => syndication::SyndicationResult {
            platform: network.to_string(),
            success: true,
            url: r.url,
            error: None,
        },
        "dry_run" => fail("DRY RUN — payload logged, nothing sent".into()),
        _ => fail(r.error.unwrap_or_else(|| r.status.clone())),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    const NOTE: &str = "---\ntitle: Who gets the water 🌊\ndek: \"A dispatch on wells, drought, and who pays — São Joaquin edition\"\ndate: 2026-10-01\nimage: https://res.cloudinary.com/ejf/image/upload/v1/dispatch/wells.jpg\nimage_alt: Dry irrigation canal beside a pistachio orchard\ntags: [water, data-journalism, dispatch]\nsources:\n  - title: USGS\n    url: https://usgs.gov\n---\n\nThe valley's wells are going dry.\n";
    const URL: &str = "https://ejfox.com/dispatch/who-gets-the-water";

    fn no_sleep(_: Duration) {}
    fn all_configured(_: &str) -> bool {
        true
    }

    #[test]
    fn plan_reads_note() {
        let plan = plan_from_content(NOTE, "who-gets-the-water", URL);
        assert_eq!(plan.title, "Who gets the water 🌊");
        assert!(plan.lead.starts_with("A dispatch on wells"));
        assert_eq!(plan.hashtags, vec!["#water", "#datajournalism"]);
        assert_eq!(plan.visibility, Visibility::Public);
        assert!(plan.existing.is_empty());
        assert!(postable_image(&plan).is_some());
    }

    #[test]
    fn dry_run_builds_payloads_without_posting() {
        let plan = plan_from_content(NOTE, "who-gets-the-water", URL);
        let texts = HashMap::new();
        let env = FanOutEnv {
            dry_run: true,
            configured: &|_| false, // dry run doesn't need creds
            texts: &texts,
            sleeper: &no_sleep,
        };
        let mut warnings = vec![];
        let results = fan_out(
            &plan,
            &["mastodon".into(), "bluesky".into()],
            &env,
            &mut |_, _| panic!("dry run must not post"),
            &mut |_| panic!("dry run must not write back"),
            &mut warnings,
        );
        assert_eq!(results.len(), 2);
        assert!(results
            .iter()
            .all(|r| r.status == "dry_run" && r.attempts == 0));

        // Mastodon: media upload with alt text, then status referencing media.
        let m = results[0].payload.as_ref().unwrap();
        let reqs = m["requests"].as_array().unwrap();
        assert_eq!(reqs[0]["endpoint"], "/api/v2/media");
        assert_eq!(
            reqs[0]["multipart"]["description"],
            "Dry irrigation canal beside a pistachio orchard"
        );
        assert_eq!(reqs[1]["endpoint"], "/api/v1/statuses");
        assert_eq!(reqs[1]["json"]["visibility"], "public");
        assert_eq!(reqs[1]["json"]["media_ids"].as_array().unwrap().len(), 1);
        assert!(reqs[1]["json"]["status"].as_str().unwrap().contains(URL));

        // Bluesky: session → blob → createRecord with facets + external card.
        let b = results[1].payload.as_ref().unwrap();
        let reqs = b["requests"].as_array().unwrap();
        let endpoints: Vec<&str> = reqs
            .iter()
            .map(|r| r["endpoint"].as_str().unwrap())
            .collect();
        assert_eq!(
            endpoints,
            vec![
                "/xrpc/com.atproto.server.createSession",
                "/xrpc/com.atproto.repo.uploadBlob",
                "/xrpc/com.atproto.repo.createRecord"
            ]
        );
        assert!(!b.to_string().contains("BLUESKY_APP_PASSWORD=")); // never leaks a secret value
        let body = &reqs[2]["json"];
        assert_eq!(body["collection"], "app.bsky.feed.post");
        let record = &body["record"];
        assert_eq!(record["$type"], "app.bsky.feed.post");
        let text = record["text"].as_str().unwrap();
        assert!(bluesky::grapheme_len(text) <= bluesky::MAX_GRAPHEMES);
        let link = record["facets"]
            .as_array()
            .unwrap()
            .iter()
            .find(|f| f["features"][0]["$type"] == "app.bsky.richtext.facet#link")
            .unwrap();
        let (s, e) = (
            link["index"]["byteStart"].as_u64().unwrap() as usize,
            link["index"]["byteEnd"].as_u64().unwrap() as usize,
        );
        assert_eq!(&text[s..e], URL);
        let ext = &record["embed"]["external"];
        assert_eq!(record["embed"]["$type"], "app.bsky.embed.external");
        assert_eq!(ext["uri"], URL);
        assert_eq!(ext["title"], "Who gets the water 🌊");
        assert!(ext["description"]
            .as_str()
            .unwrap()
            .starts_with("A dispatch"));
        assert_eq!(ext["thumb"]["$type"], "blob");
        assert!(record["createdAt"].as_str().unwrap().ends_with('Z'));
    }

    #[test]
    fn image_without_alt_is_never_attached() {
        let note = NOTE.replace(
            "image_alt: Dry irrigation canal beside a pistachio orchard\n",
            "",
        );
        let plan = plan_from_content(&note, "s", URL);
        let m = describe_requests("mastodon", &plan, "t");
        assert_eq!(
            m["requests"].as_array().unwrap().len(),
            1,
            "no media upload"
        );
        assert!(m["requests"][0]["json"].get("media_ids").is_none());
        let b = describe_requests("bluesky", &plan, "t");
        assert!(!b.to_string().contains("uploadBlob"));
    }

    #[test]
    fn idempotent_skips_already_syndicated_networks() {
        let content = RefCell::new(
            fm::merge_syndication(
                NOTE,
                &[SyndicationEntry {
                    network: "bluesky".into(),
                    url: "https://bsky.app/profile/ejfox.com/post/3old".into(),
                }],
            )
            .unwrap(),
        );
        let calls = RefCell::new(Vec::<String>::new());
        let texts = HashMap::new();
        let env = FanOutEnv {
            dry_run: false,
            configured: &all_configured,
            texts: &texts,
            sleeper: &no_sleep,
        };
        let run = || {
            let plan = plan_from_content(&content.borrow(), "s", URL);
            let mut w = vec![];
            fan_out(
                &plan,
                &["mastodon".into(), "bluesky".into(), "Mastodon".into()],
                &env,
                &mut |n, _| {
                    calls.borrow_mut().push(n.to_string());
                    Ok(format!("https://{}.example/1", n))
                },
                &mut |entry| {
                    let next =
                        fm::merge_syndication(&content.borrow(), std::slice::from_ref(entry))?;
                    *content.borrow_mut() = next;
                    Ok(())
                },
                &mut w,
            )
        };

        let first = run();
        assert_eq!(
            *calls.borrow(),
            vec!["mastodon"],
            "bluesky already had a URL"
        );
        assert_eq!(first.len(), 2, "duplicate network names collapse");
        assert_eq!(first[0].status, "posted");
        assert_eq!(first[1].status, "skipped");
        assert_eq!(
            first[1].url.as_deref(),
            Some("https://bsky.app/profile/ejfox.com/post/3old")
        );

        // Second run: both now recorded → zero posts.
        let second = run();
        assert_eq!(calls.borrow().len(), 1, "re-running never double-posts");
        assert!(second.iter().all(|r| r.status == "skipped"));
        assert!(fm::body(&content.borrow()) == fm::body(NOTE));
    }

    #[test]
    fn one_network_failing_does_not_block_others_and_retries_are_bounded() {
        let plan = plan_from_content(NOTE, "s", URL);
        let texts = HashMap::new();
        let sleeps = RefCell::new(0);
        let sleeper = |_: Duration| *sleeps.borrow_mut() += 1;
        let env = FanOutEnv {
            dry_run: false,
            configured: &all_configured,
            texts: &texts,
            sleeper: &sleeper,
        };
        let mut w = vec![];
        let results = fan_out(
            &plan,
            &["mastodon".into(), "bluesky".into()],
            &env,
            &mut |n, _| match n {
                "mastodon" => Err(NetError::transient("503")),
                _ => Ok("https://bsky.app/profile/x/post/y".into()),
            },
            &mut |_| Ok(()),
            &mut w,
        );
        assert_eq!(results[0].status, "failed");
        assert_eq!(results[0].attempts, MAX_ATTEMPTS);
        assert_eq!(*sleeps.borrow(), (MAX_ATTEMPTS - 1) as i32);
        assert_eq!(results[1].status, "posted");

        // Fatal (4xx) errors are not retried.
        let (r, attempts) = with_retry(3, &no_sleep, || Err(NetError::fatal("401")));
        assert!(r.is_err());
        assert_eq!(attempts, 1);
    }

    #[test]
    fn unlisted_and_protected_rules() {
        let unlisted = NOTE.replace("date: 2026-10-01\n", "date: 2026-10-01\nunlisted: true\n");
        let plan = plan_from_content(&unlisted, "s", URL);
        assert_eq!(plan.visibility, Visibility::Unlisted);
        let texts = HashMap::new();
        let env = FanOutEnv {
            dry_run: true,
            configured: &all_configured,
            texts: &texts,
            sleeper: &no_sleep,
        };
        let r = fan_out(
            &plan,
            &["mastodon".into(), "bluesky".into()],
            &env,
            &mut |_, _| unreachable!(),
            &mut |_| unreachable!(),
            &mut vec![],
        );
        assert_eq!(
            r[0].payload.as_ref().unwrap()["requests"][1]["json"]["visibility"],
            "unlisted"
        );
        assert_eq!(r[1].status, "skipped");

        let protected = NOTE.replace("date: 2026-10-01\n", "date: 2026-10-01\npassword: birds\n");
        assert!(refuse_reason(&plan_from_content(&protected, "s", URL)).is_some());
    }
}
