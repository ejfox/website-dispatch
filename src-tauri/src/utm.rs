//! UTM tagging for shared links, so every presence feeds the Umami funnel.
//!
//! The canonical URL (site, frontmatter) stays clean; only links Dispatch
//! shares — cross-posts, "copy link for X/newsletter" — are tagged:
//!   ?utm_source=<bluesky|mastodon|x|newsletter>&utm_medium=<social|email>
//!    &utm_campaign=<dispatch|blog>-<slug>

const UTM_KEYS: [&str; 3] = ["utm_source", "utm_medium", "utm_campaign"];

/// Add (or replace) utm_source/medium/campaign, keeping other query params
/// and any #fragment.
pub fn utm_url(url: &str, source: &str, medium: &str, campaign: &str) -> String {
    let (before_frag, fragment) = match url.find('#') {
        Some(i) => (&url[..i], Some(&url[i..])),
        None => (url, None),
    };
    let (base, query) = match before_frag.find('?') {
        Some(i) => (&before_frag[..i], &before_frag[i + 1..]),
        None => (before_frag, ""),
    };
    let mut params: Vec<String> = query
        .split('&')
        .filter(|kv| !kv.is_empty())
        .filter(|kv| {
            let key = kv.split('=').next().unwrap_or("");
            !UTM_KEYS.contains(&key)
        })
        .map(String::from)
        .collect();
    for (k, v) in UTM_KEYS.iter().zip([source, medium, campaign]) {
        params.push(format!("{}={}", k, urlencoding::encode(v)));
    }
    format!("{}?{}{}", base, params.join("&"), fragment.unwrap_or(""))
}

/// Last path segment of a URL (query/fragment ignored).
fn slug_of(url: &str) -> &str {
    let end = url.find(['?', '#']).unwrap_or(url.len());
    url[..end]
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .unwrap_or("")
}

/// `dispatch-<slug>` for /dispatch/ pieces, `blog-<slug>` otherwise.
pub fn campaign_for(url: &str) -> String {
    let kind = if url.contains("/dispatch/") {
        "dispatch"
    } else {
        "blog"
    };
    format!("{}-{}", kind, slug_of(url))
}

/// The link to share on a given source, campaign derived from the URL.
pub fn share_url(url: &str, source: &str, medium: &str) -> String {
    utm_url(url, source, medium, &campaign_for(url))
}

/// Value of `key` in a query string (leading `?` optional), URL-decoded.
pub fn query_param(query: &str, key: &str) -> Option<String> {
    let q = query.split_once('?').map(|(_, q)| q).unwrap_or(query);
    q.split('&').find_map(|kv| {
        let (k, v) = kv.split_once('=')?;
        (k == key).then(|| {
            urlencoding::decode(&v.replace('+', " "))
                .map(|s| s.into_owned())
                .unwrap_or_else(|_| v.to_string())
        })
    })
}

/// Replace standalone occurrences of `clean` in `text` with `tagged`. An
/// occurrence that continues as a longer URL (`/more`, `?q`, `#f`, `-x`) is
/// left alone, so already-tagged or different links are never double-tagged.
pub fn replace_link(text: &str, clean: &str, tagged: &str) -> String {
    if clean.is_empty() || tagged.is_empty() {
        return text.to_string();
    }
    let mut out = String::with_capacity(text.len() + tagged.len());
    let mut rest = text;
    while let Some(i) = rest.find(clean) {
        let after = &rest[i + clean.len()..];
        let continues = after.chars().next().is_some_and(|c| {
            c.is_alphanumeric() || matches!(c, '/' | '?' | '#' | '-' | '_' | '=' | '&' | '%')
        });
        out.push_str(&rest[..i]);
        out.push_str(if continues { clean } else { tagged });
        rest = after;
    }
    out.push_str(rest);
    out
}

#[tauri::command]
pub fn share_link(url: String, source: String, medium: String) -> String {
    share_url(&url, &source, &medium)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tags_a_clean_url() {
        assert_eq!(
            share_url("https://ejfox.com/dispatch/water-wars", "bluesky", "social"),
            "https://ejfox.com/dispatch/water-wars?utm_source=bluesky&utm_medium=social&utm_campaign=dispatch-water-wars"
        );
        assert_eq!(
            share_url("https://ejfox.com/blog/2026/my-post", "newsletter", "email"),
            "https://ejfox.com/blog/2026/my-post?utm_source=newsletter&utm_medium=email&utm_campaign=blog-my-post"
        );
    }

    #[test]
    fn keeps_existing_params_and_fragment() {
        assert_eq!(
            utm_url("https://ejfox.com/dispatch/x?ref=a&b=2#chart", "x", "social", "dispatch-x"),
            "https://ejfox.com/dispatch/x?ref=a&b=2&utm_source=x&utm_medium=social&utm_campaign=dispatch-x#chart"
        );
        // Fragment without query.
        assert_eq!(
            utm_url("https://ejfox.com/blog/2026/p#s", "mastodon", "social", "blog-p"),
            "https://ejfox.com/blog/2026/p?utm_source=mastodon&utm_medium=social&utm_campaign=blog-p#s"
        );
    }

    #[test]
    fn replaces_existing_utm_instead_of_duplicating() {
        let once = share_url("https://ejfox.com/dispatch/x", "bluesky", "social");
        let twice = share_url(&once, "mastodon", "social");
        assert_eq!(
            twice,
            "https://ejfox.com/dispatch/x?utm_source=mastodon&utm_medium=social&utm_campaign=dispatch-x"
        );
        assert_eq!(twice.matches("utm_source").count(), 1);
    }

    #[test]
    fn campaign_ignores_query_trailing_slash_and_encodes_values() {
        assert_eq!(
            campaign_for("https://ejfox.com/dispatch/x/?a=1#f"),
            "dispatch-x"
        );
        assert!(utm_url("https://e.com/blog/p", "a b", "m&n", "c")
            .contains("utm_source=a%20b&utm_medium=m%26n"));
    }

    #[test]
    fn replaces_only_standalone_links() {
        let clean = "https://ejfox.com/dispatch/x";
        let tagged = share_url(clean, "mastodon", "social");
        let text = format!(
            "Read it: {}.\n\nAlso {}-two and {}?a=1",
            clean, clean, clean
        );
        let out = replace_link(&text, clean, &tagged);
        assert!(out.starts_with(&format!("Read it: {}.", tagged)));
        assert!(out.contains(&format!("{}-two", clean)));
        assert!(out.contains(&format!("{}?a=1", clean)));
        // Idempotent: tagged links aren't re-tagged.
        assert_eq!(replace_link(&out, clean, &tagged), out);
    }

    #[test]
    fn reads_query_params() {
        assert_eq!(
            query_param("?utm_source=bluesky&x=1", "utm_source").as_deref(),
            Some("bluesky")
        );
        assert_eq!(
            query_param("a=1&utm_source=news%20letter", "utm_source").as_deref(),
            Some("news letter")
        );
        assert_eq!(query_param("ref=hn", "utm_source"), None);
    }
}
