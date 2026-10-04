//! Bluesky via raw AT Protocol XRPC (reqwest, no SDK).
//!
//! Flow: com.atproto.server.createSession → com.atproto.repo.uploadBlob (card
//! thumbnail / image) → com.atproto.repo.createRecord (app.bsky.feed.post).
//!
//! Payload builders are pure functions so dry-run and tests can exercise the
//! exact JSON that would be sent.

use regex::Regex;
use serde_json::{json, Value};
use std::sync::LazyLock;

/// Bluesky's post limit is 300 graphemes.
pub const MAX_GRAPHEMES: usize = 300;
/// uploadBlob limit for images is ~976KB.
pub const MAX_BLOB_BYTES: usize = 976_560;

static URL_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"https?://[^\s<>"]+"#).expect("valid regex"));
static TAG_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?:^|\s)(#[\p{L}\p{N}_]+)").expect("valid regex"));

#[derive(Debug, Clone)]
pub struct NetError {
    pub message: String,
    /// Network failure, 5xx or 429: worth retrying. 4xx is not.
    pub retryable: bool,
}

impl NetError {
    pub fn fatal(m: impl Into<String>) -> Self {
        NetError {
            message: m.into(),
            retryable: false,
        }
    }
    pub fn transient(m: impl Into<String>) -> Self {
        NetError {
            message: m.into(),
            retryable: true,
        }
    }
    pub fn from_status(context: &str, status: reqwest::StatusCode, body: &str) -> Self {
        let retryable = status.is_server_error() || status.as_u16() == 429;
        let snippet: String = body.chars().take(300).collect();
        NetError {
            message: format!("{} {} — {}", context, status, snippet),
            retryable,
        }
    }
}

// ---------------------------------------------------------------------------
// Text + facets (pure)
// ---------------------------------------------------------------------------

/// Upper bound on grapheme count. Every grapheme cluster is ≥1 char, so
/// counting chars never under-counts — text that passes this check always
/// passes Bluesky's 300-grapheme limit (it may truncate a hair early on
/// ZWJ-emoji-heavy text, which is the safe direction).
pub fn grapheme_len(s: &str) -> usize {
    s.chars().count()
}

/// Truncate to at most `max` chars, ending with an ellipsis if cut.
pub fn truncate_chars(s: &str, max: usize) -> String {
    if grapheme_len(s) <= max {
        return s.to_string();
    }
    if max == 0 {
        return String::new();
    }
    let cut: String = s.chars().take(max.saturating_sub(1)).collect();
    format!("{}…", cut.trim_end())
}

/// Compose post text: lead (dek/title), blank line, URL, optional hashtags.
/// Drops hashtags first, then truncates the lead, to fit 300 graphemes. The URL
/// is never truncated.
pub fn compose_text(lead: &str, url: &str, hashtags: &[String]) -> String {
    let tags = hashtags.join(" ");
    let with_tags = if tags.is_empty() {
        format!("{}\n\n{}", lead, url)
    } else {
        format!("{}\n\n{}\n\n{}", lead, url, tags)
    };
    if grapheme_len(&with_tags) <= MAX_GRAPHEMES {
        return with_tags;
    }
    let without_tags = format!("{}\n\n{}", lead, url);
    if grapheme_len(&without_tags) <= MAX_GRAPHEMES {
        return without_tags;
    }
    let budget = MAX_GRAPHEMES.saturating_sub(grapheme_len(url) + 2);
    format!("{}\n\n{}", truncate_chars(lead, budget), url)
}

/// Fit arbitrary (user-edited) text into the limit, preserving any URL at the end.
pub fn fit_text(text: &str) -> String {
    if grapheme_len(text) <= MAX_GRAPHEMES {
        return text.to_string();
    }
    if let Some(m) = URL_RE.find_iter(text).last() {
        let url = trim_url(m.as_str());
        let lead = text[..m.start()].trim_end();
        return compose_text(lead, url, &[]);
    }
    truncate_chars(text, MAX_GRAPHEMES)
}

fn trim_url(u: &str) -> &str {
    u.trim_end_matches(['.', ',', ';', ':', '!', '?', ')', ']', '}', '\'', '"'])
}

/// Rich-text facets for links and hashtags. Offsets are UTF-8 BYTE offsets
/// into `text` (Rust string indices already are), as the lexicon requires.
#[cfg(test)]
pub fn build_facets(text: &str) -> Vec<Value> {
    build_facets_mapped(text, &|u| u.to_string())
}

/// Like `build_facets`, but the link target can differ from the displayed
/// text: `link_target(displayed_url)` gives the facet `uri` (e.g. the clean
/// URL is shown, the UTM-tagged URL is linked). Byte offsets always index the
/// displayed text.
pub fn build_facets_mapped(text: &str, link_target: &dyn Fn(&str) -> String) -> Vec<Value> {
    let mut facets = Vec::new();
    for m in URL_RE.find_iter(text) {
        let shown = trim_url(m.as_str());
        facets.push(json!({
            "index": { "byteStart": m.start(), "byteEnd": m.start() + shown.len() },
            "features": [{ "$type": "app.bsky.richtext.facet#link", "uri": link_target(shown) }]
        }));
    }
    for caps in TAG_RE.captures_iter(text) {
        let Some(m) = caps.get(1) else { continue };
        let tag = &m.as_str()[1..];
        if tag.chars().all(|c| c.is_ascii_digit()) || grapheme_len(tag) > 64 {
            continue;
        }
        facets.push(json!({
            "index": { "byteStart": m.start(), "byteEnd": m.end() },
            "features": [{ "$type": "app.bsky.richtext.facet#tag", "tag": tag }]
        }));
    }
    facets
}

/// External link card embed. `thumb` is the blob object from uploadBlob.
pub fn external_embed(uri: &str, title: &str, description: &str, thumb: Option<Value>) -> Value {
    let mut external = json!({
        "uri": uri,
        "title": title,
        "description": description,
    });
    if let Some(t) = thumb {
        external["thumb"] = t;
    }
    json!({ "$type": "app.bsky.embed.external", "external": external })
}

/// Image embed (used when there's no page URL). Alt text is mandatory here.
pub fn images_embed(blob: Value, alt: &str) -> Value {
    json!({
        "$type": "app.bsky.embed.images",
        "images": [{ "image": blob, "alt": alt }]
    })
}

pub fn post_record_mapped(
    text: &str,
    embed: Option<Value>,
    created_at: &str,
    link_target: &dyn Fn(&str) -> String,
) -> Value {
    let mut record = json!({
        "$type": "app.bsky.feed.post",
        "text": text,
        "createdAt": created_at,
        "langs": ["en"],
    });
    let facets = build_facets_mapped(text, link_target);
    if !facets.is_empty() {
        record["facets"] = Value::Array(facets);
    }
    if let Some(e) = embed {
        record["embed"] = e;
    }
    record
}

/// Placeholder blob used by dry-run so the payload has the real shape.
pub fn placeholder_blob(source_url: &str) -> Value {
    json!({
        "$type": "blob",
        "ref": { "$link": format!("<dry-run: uploadBlob({})>", source_url) },
        "mimeType": "image/jpeg",
        "size": 0
    })
}

/// at://did:plc:xyz/app.bsky.feed.post/3kabc → https://bsky.app/profile/<handle>/post/3kabc
pub fn web_url(at_uri: &str, handle: &str) -> Option<String> {
    let rkey = at_uri.rsplit('/').next().filter(|r| !r.is_empty())?;
    if !at_uri.starts_with("at://") || !at_uri.contains("/app.bsky.feed.post/") {
        return None;
    }
    Some(format!("https://bsky.app/profile/{}/post/{}", handle, rkey))
}

// ---------------------------------------------------------------------------
// Config + network
// ---------------------------------------------------------------------------

pub struct BskyConfig {
    pub service: String,
    pub handle: String,
    app_password: String,
}

pub fn is_configured() -> bool {
    config_from_env().is_ok()
}

pub fn config_from_env() -> Result<BskyConfig, String> {
    let handle = std::env::var("BLUESKY_HANDLE")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .ok_or("BLUESKY_HANDLE not set in .env")?;
    let app_password = std::env::var("BLUESKY_APP_PASSWORD")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .ok_or("BLUESKY_APP_PASSWORD not set in .env")?;
    let service = std::env::var("BLUESKY_SERVICE")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| "https://bsky.social".into());
    Ok(BskyConfig {
        service: service.trim_end_matches('/').to_string(),
        handle: handle.trim().trim_start_matches('@').to_string(),
        app_password,
    })
}

pub fn configured_handle() -> Option<String> {
    config_from_env().ok().map(|c| c.handle)
}

pub struct Session {
    pub access_jwt: String,
    pub did: String,
    pub handle: String,
}

fn send_json(rb: reqwest::blocking::RequestBuilder, context: &str) -> Result<Value, NetError> {
    let resp = rb
        .send()
        .map_err(|e| NetError::transient(format!("{}: request failed: {}", context, e)))?;
    let status = resp.status();
    let text = resp.text().unwrap_or_default();
    if !status.is_success() {
        return Err(NetError::from_status(context, status, &text));
    }
    serde_json::from_str(&text)
        .map_err(|e| NetError::fatal(format!("{}: bad JSON: {}", context, e)))
}

pub fn create_session(
    client: &reqwest::blocking::Client,
    cfg: &BskyConfig,
) -> Result<Session, NetError> {
    let data = send_json(
        client
            .post(format!(
                "{}/xrpc/com.atproto.server.createSession",
                cfg.service
            ))
            .json(&json!({ "identifier": cfg.handle, "password": cfg.app_password })),
        "Bluesky createSession",
    )?;
    let get = |k: &str| {
        data[k]
            .as_str()
            .map(String::from)
            .ok_or_else(|| NetError::fatal(format!("Bluesky createSession: missing {}", k)))
    };
    Ok(Session {
        access_jwt: get("accessJwt")?,
        did: get("did")?,
        handle: get("handle").unwrap_or_else(|_| cfg.handle.clone()),
    })
}

pub fn upload_blob(
    client: &reqwest::blocking::Client,
    cfg: &BskyConfig,
    session: &Session,
    bytes: Vec<u8>,
    mime: &str,
) -> Result<Value, NetError> {
    if bytes.len() > MAX_BLOB_BYTES {
        return Err(NetError::fatal(format!(
            "image is {} bytes; Bluesky limit is {}",
            bytes.len(),
            MAX_BLOB_BYTES
        )));
    }
    let data = send_json(
        client
            .post(format!("{}/xrpc/com.atproto.repo.uploadBlob", cfg.service))
            .bearer_auth(&session.access_jwt)
            .header(reqwest::header::CONTENT_TYPE, mime)
            .body(bytes),
        "Bluesky uploadBlob",
    )?;
    data.get("blob")
        .cloned()
        .ok_or_else(|| NetError::fatal("Bluesky uploadBlob: no blob in response"))
}

/// createRecord for a post. Returns the at:// URI.
pub fn create_post(
    client: &reqwest::blocking::Client,
    cfg: &BskyConfig,
    session: &Session,
    record: &Value,
) -> Result<String, NetError> {
    let data = send_json(
        client
            .post(format!(
                "{}/xrpc/com.atproto.repo.createRecord",
                cfg.service
            ))
            .bearer_auth(&session.access_jwt)
            .json(&json!({
                "repo": session.did,
                "collection": "app.bsky.feed.post",
                "record": record,
            })),
        "Bluesky createRecord",
    )?;
    data["uri"]
        .as_str()
        .map(String::from)
        .ok_or_else(|| NetError::fatal("Bluesky createRecord: no uri in response"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn slice_by_facet<'a>(text: &'a str, facet: &Value) -> &'a str {
        let s = facet["index"]["byteStart"].as_u64().unwrap() as usize;
        let e = facet["index"]["byteEnd"].as_u64().unwrap() as usize;
        // Panics unless s/e fall on UTF-8 char boundaries — part of the check.
        &text[s..e]
    }

    #[test]
    fn facet_byte_offsets_with_emoji_and_multibyte() {
        let url = "https://ejfox.com/dispatch/água-🌊";
        let text = format!("Água é vida 🌊👩‍👩‍👧 — São Paulo\n\n{}\n\n#água #data", url);
        let facets = build_facets(&text);
        let link = facets
            .iter()
            .find(|f| f["features"][0]["$type"] == "app.bsky.richtext.facet#link")
            .unwrap();
        // Byte offsets, not char offsets: the slice must be exactly the URL.
        assert_eq!(slice_by_facet(&text, link), url);
        let start = link["index"]["byteStart"].as_u64().unwrap() as usize;
        assert_eq!(start, text.find("https://").unwrap());
        assert_ne!(start, text.chars().position(|c| c == 'h').unwrap());
        assert_eq!(link["features"][0]["uri"], url);

        let tags: Vec<&Value> = facets
            .iter()
            .filter(|f| f["features"][0]["$type"] == "app.bsky.richtext.facet#tag")
            .collect();
        assert_eq!(tags.len(), 2);
        assert_eq!(slice_by_facet(&text, tags[0]), "#água");
        assert_eq!(tags[0]["features"][0]["tag"], "água");
        assert_eq!(slice_by_facet(&text, tags[1]), "#data");
    }

    #[test]
    fn trailing_punctuation_not_in_link() {
        let text = "read it (https://ejfox.com/dispatch/x).";
        let f = &build_facets(text)[0];
        assert_eq!(slice_by_facet(text, f), "https://ejfox.com/dispatch/x");
    }

    #[test]
    fn url_fragment_is_not_a_hashtag() {
        let text = "see https://ejfox.com/a#section";
        let facets = build_facets(text);
        assert_eq!(facets.len(), 1);
        assert_eq!(
            facets[0]["features"][0]["$type"],
            "app.bsky.richtext.facet#link"
        );
    }

    #[test]
    fn compose_respects_limit_and_keeps_url() {
        let url = "https://ejfox.com/dispatch/a-very-long-slug";
        let lead = "🌊".repeat(400);
        let text = compose_text(&lead, url, &["#water".into()]);
        assert!(grapheme_len(&text) <= MAX_GRAPHEMES);
        assert!(text.ends_with(url));
        assert!(text.contains('…'));
        // Facet still lines up after truncation.
        let f = &build_facets(&text)[0];
        assert_eq!(slice_by_facet(&text, f), url);

        let short = compose_text("Short dek", url, &["#water".into()]);
        assert_eq!(short, format!("Short dek\n\n{}\n\n#water", url));
    }

    #[test]
    fn web_url_from_at_uri() {
        assert_eq!(
            web_url("at://did:plc:abc/app.bsky.feed.post/3kxyz", "ejfox.com").unwrap(),
            "https://bsky.app/profile/ejfox.com/post/3kxyz"
        );
        assert!(web_url("https://nope", "x").is_none());
    }
}
