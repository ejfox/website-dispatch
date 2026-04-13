use regex::Regex;
use std::sync::LazyLock;

// ---------------------------------------------------------------------------
// Junk alt text detection
// ---------------------------------------------------------------------------

/// Matches junk alt text: screenshots, pasted images, camera prefixes, etc.
pub static JUNK_ALT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)^(Screenshot|Screen Shot|Pasted image|IMG_|DSC|DJI_|DSCF|CleanShot|Untitled|image\d*)",
    )
    .expect("valid regex")
});

/// Matches UUID-like prefixes (8hex-4hex...).
pub static UUID_PREFIX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^[A-Fa-f0-9]{8}-[A-Fa-f0-9]{4}").expect("valid regex")
});

/// Matches bare filenames with image extensions.
pub static FILENAME_WITH_EXT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)^[A-Za-z0-9_.-]+\.(png|jpe?g|gif|webp|svg|tiff?)$").expect("valid regex")
});

/// Matches date-prefixed junk alt text (YYYY-MM-DD...).
pub static DATE_PREFIX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^\d{4}-\d{2}-\d{2}").expect("valid regex")
});

// ---------------------------------------------------------------------------
// Markdown patterns
// ---------------------------------------------------------------------------

/// Matches markdown images: ![alt](url)
pub static MD_IMAGE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"!\[([^\]]*)\]\(([^)]+)\)").expect("valid regex")
});

/// Matches markdown images with non-empty alt text only: ![alt](url)
/// (same pattern as MD_IMAGE but used in alt-text checking context)
pub static MD_IMAGE_ALT_CHECK: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"!\[([^\]]*)\]\([^)]+\)").expect("valid regex")
});

/// Matches markdown links (and images): (!)?\[text\](url)
/// Group 1 = "!" if image, Group 2 = link text, Group 3 = URL
pub static MD_LINK: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(!)?\[([^\]]+)\]\(([^)]+)\)").expect("valid regex")
});

// ---------------------------------------------------------------------------
// HTML media patterns (cloudinary.rs)
// ---------------------------------------------------------------------------

/// Matches HTML img tags: <img ... src="..." ...>
pub static HTML_IMG: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"<img[^>]+src=["']([^"']+)["'][^>]*>"#).expect("valid regex")
});

/// Matches HTML video tags: <video ... src="..." ...>
pub static HTML_VIDEO: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"<video[^>]+src=["']([^"']+)["'][^>]*>"#).expect("valid regex")
});

// ---------------------------------------------------------------------------
// Cloudinary URL extraction (asset_usage.rs)
// ---------------------------------------------------------------------------

/// Extracts Cloudinary URLs in various formats.
pub static CLOUDINARY_URL: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r#"https://res\.cloudinary\.com/([^/]+)/(image|video|raw)/upload/(?:[^/]+/)*([^\s\)"'\]]+)"#,
    )
    .expect("valid regex")
});

// ---------------------------------------------------------------------------
// Cloudinary public_id helpers (alttext.rs)
// ---------------------------------------------------------------------------

/// Matches Cloudinary version segments (v1234567).
pub static CLOUDINARY_VERSION: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^v\d+$").expect("valid regex")
});

/// Matches Cloudinary transform segments (c_scale, w_512, etc.).
pub static CLOUDINARY_TRANSFORM: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^[cwhfqget]_").expect("valid regex")
});

/// Matches common image file extensions (for stripping from public_id).
pub static IMAGE_FILE_EXT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\.(jpg|jpeg|png|gif|webp|svg|tiff?|bmp)$").expect("valid regex")
});

// ---------------------------------------------------------------------------
// Webmention patterns
// ---------------------------------------------------------------------------

/// Extracts href from HTML anchor tags.
pub static HTML_LINK: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"<a[^>]+href="(https?://[^"]+)"[^>]*>"#).expect("valid regex")
});

/// Matches <link rel="webmention" href="..."> (rel before href).
pub static WEBMENTION_LINK_REL_FIRST: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"<link[^>]*rel=["']?webmention["']?[^>]*href=["']([^"']+)["'][^>]*/?\s*>"#)
        .expect("valid regex")
});

/// Matches <link href="..." rel="webmention"> (href before rel).
pub static WEBMENTION_LINK_HREF_FIRST: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"<link[^>]*href=["']([^"']+)["'][^>]*rel=["']?webmention["']?[^>]*/?\s*>"#)
        .expect("valid regex")
});

/// Matches <a rel="webmention" href="..."> (rel before href).
pub static WEBMENTION_A_REL_FIRST: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"<a[^>]*rel=["']?webmention["']?[^>]*href=["']([^"']+)["'][^>]*>"#)
        .expect("valid regex")
});

/// Matches <a href="..." rel="webmention"> (href before rel).
pub static WEBMENTION_A_HREF_FIRST: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"<a[^>]*href=["']([^"']+)["'][^>]*rel=["']?webmention["']?[^>]*>"#)
        .expect("valid regex")
});

/// Extracts URL from Link header: <url>.
pub static LINK_HEADER_URL: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"<([^>]+)>").expect("valid regex")
});

// ---------------------------------------------------------------------------
// Privacy detection (vault.rs weeknotes)
// ---------------------------------------------------------------------------

/// Matches US phone numbers.
pub static PHONE_NUMBER: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?:\+1[\s.-]?)?\(?\d{3}\)?[\s.-]\d{3}[\s.-]\d{4}").expect("valid regex")
});

/// Matches email addresses.
pub static EMAIL_ADDRESS: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}").expect("valid regex")
});

/// Matches large dollar amounts.
pub static MONEY_AMOUNT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\$\d{1,3}(?:,\d{3})+(?:\.\d{2})?|\$\d{4,}(?:\.\d{2})?").expect("valid regex")
});

/// Matches SSN patterns (XXX-XX-XXXX).
pub static SSN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\b\d{3}-\d{2}-\d{4}\b").expect("valid regex")
});

/// Matches street addresses.
pub static STREET_ADDRESS: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\b\d{1,5}\s+[A-Z][a-z]+(?:\s+[A-Z][a-z]+)*\s+(?:St|Ave|Blvd|Dr|Rd|Ln|Ct|Way|Pl|Circle|Terrace|Court)\b").expect("valid regex")
});

/// Matches references to named people ("met with John", "called Sarah", etc.).
pub static PEOPLE_REFERENCE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\b(?:met with|talked to|called|lunch with|dinner with|coffee with|meeting with|spoke with|visited|hanging out with|texted|emailed|DMed)\s+([A-Z][a-z]+(?:\s+[A-Z][a-z]+)?)").expect("valid regex")
});

/// Matches health/medical terms.
pub static HEALTH_MEDICAL: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\b(?:diagnosis|prescribed|medication|therapist|therapy session|doctor(?:'s)? appointment|blood (?:test|pressure|work)|symptoms?|mg\s+(?:of|daily)|medical|hospital|surgery|prescription)\b").expect("valid regex")
});

// ---------------------------------------------------------------------------
// Git log parsing (journal.rs)
// ---------------------------------------------------------------------------

/// Matches git log lines: "TIMESTAMP Publish: SLUG"
pub static GIT_PUBLISH_LOG: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^(\S+)\s+Publish:\s+(.+)$").expect("valid regex")
});
