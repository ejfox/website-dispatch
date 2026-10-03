//! Minimal, text-preserving frontmatter reads + edits.
//!
//! Used by omnipublish to read the handful of fields it needs (title, dek,
//! image, image_alt, tags, syndication, visibility) and to write back
//! `syndication:` entries and a generated `image_alt:`.
//!
//! Edits never round-trip the YAML through a serializer: they splice new lines
//! into the frontmatter block and leave every other byte — other keys, their
//! order, comments, and the entire body — exactly as it was.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyndicationEntry {
    pub network: String,
    pub url: String,
}

/// Byte range of the frontmatter *contents* (between the `---` fences).
/// `start` = first byte after the opening fence line, `end` = first byte of
/// the closing fence line. `start == end` for an empty block.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FmSpan {
    pub start: usize,
    pub end: usize,
}

/// Locate the frontmatter block. Requires the file to open with a `---` line
/// and a later line that is exactly `---` (CRLF tolerated).
pub fn locate(content: &str) -> Option<FmSpan> {
    let start = if content.starts_with("---\r\n") {
        5
    } else if content.starts_with("---\n") {
        4
    } else {
        return None;
    };
    let mut pos = start;
    for line in content[start..].split_inclusive('\n') {
        let bare = line.trim_end_matches('\n').trim_end_matches('\r');
        if bare == "---" {
            return Some(FmSpan { start, end: pos });
        }
        pos += line.len();
    }
    None
}

fn newline_for(fm: &str) -> &'static str {
    if fm.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    }
}

/// Lines of the block with their absolute byte offsets: (line_start, line_end_incl_newline, text_without_newline)
fn lines_with_offsets(content: &str, span: FmSpan) -> Vec<(usize, usize, &str)> {
    let mut out = Vec::new();
    let mut pos = span.start;
    for line in content[span.start..span.end].split_inclusive('\n') {
        let bare = line.trim_end_matches('\n').trim_end_matches('\r');
        out.push((pos, pos + line.len(), bare));
        pos += line.len();
    }
    out
}

/// Is this line a top-level `key:` line? Returns the raw value after the colon.
fn top_level_value<'a>(line: &'a str, key: &str) -> Option<&'a str> {
    if line.starts_with(' ') || line.starts_with('\t') {
        return None;
    }
    let rest = line.strip_prefix(key)?;
    let rest = rest.trim_start_matches([' ', '\t']);
    rest.strip_prefix(':').map(|v| v.trim())
}

fn unquote(v: &str) -> String {
    let v = v.trim();
    if v.len() >= 2
        && ((v.starts_with('"') && v.ends_with('"')) || (v.starts_with('\'') && v.ends_with('\'')))
    {
        let inner = &v[1..v.len() - 1];
        if v.starts_with('"') {
            return inner.replace("\\\"", "\"").replace("\\\\", "\\");
        }
        return inner.replace("''", "'");
    }
    // Strip a trailing comment on plain scalars (` #`).
    match v.find(" #") {
        Some(i) => v[..i].trim_end().to_string(),
        None => v.to_string(),
    }
}

/// Is this a continuation line of a block value (indented, a list item, or blank)?
fn is_continuation(line: &str) -> bool {
    line.is_empty()
        || line.trim().is_empty()
        || line.starts_with(' ')
        || line.starts_with('\t')
        || line.starts_with("- ")
        || line == "-"
}

/// Read a top-level scalar. Supports plain/quoted values and `>`/`|` block scalars.
/// Returns None if missing or empty.
pub fn get_scalar(content: &str, key: &str) -> Option<String> {
    let span = locate(content)?;
    let lines = lines_with_offsets(content, span);
    for (i, (_, _, line)) in lines.iter().enumerate() {
        let Some(raw) = top_level_value(line, key) else {
            continue;
        };
        if matches!(raw, ">" | ">-" | ">+" | "|" | "|-" | "|+") {
            let block: Vec<&str> = lines[i + 1..]
                .iter()
                .map(|(_, _, l)| *l)
                .take_while(|l| l.starts_with(' ') || l.starts_with('\t') || l.trim().is_empty())
                .map(|l| l.trim())
                .collect();
            let joined = if raw.starts_with('>') {
                block.join(" ")
            } else {
                block.join("\n")
            };
            let joined = joined.trim().to_string();
            return (!joined.is_empty()).then_some(joined);
        }
        let v = unquote(raw);
        return (!v.is_empty() && v != "~" && v != "null").then_some(v);
    }
    None
}

/// Read a string list (`[a, b]`, block `- a`, or a single scalar).
pub fn get_list(content: &str, key: &str) -> Vec<String> {
    let Some(span) = locate(content) else {
        return vec![];
    };
    let lines = lines_with_offsets(content, span);
    for (i, (_, _, line)) in lines.iter().enumerate() {
        let Some(raw) = top_level_value(line, key) else {
            continue;
        };
        if raw.starts_with('[') {
            return raw
                .trim_matches(|c| c == '[' || c == ']')
                .split(',')
                .map(unquote)
                .filter(|s| !s.is_empty())
                .collect();
        }
        if !raw.is_empty() {
            return vec![unquote(raw)];
        }
        return lines[i + 1..]
            .iter()
            .map(|(_, _, l)| *l)
            .take_while(|l| is_continuation(l))
            .filter_map(|l| l.trim().strip_prefix('-').map(|s| unquote(s.trim())))
            .filter(|s| !s.is_empty())
            .collect();
    }
    vec![]
}

pub fn get_bool(content: &str, key: &str) -> bool {
    get_scalar(content, key)
        .map(|v| matches!(v.to_ascii_lowercase().as_str(), "true" | "yes"))
        .unwrap_or(false)
}

fn parse_kv_into(entry: &mut (Option<String>, Option<String>), kv: &str) {
    if let Some((k, v)) = kv.split_once(':') {
        match k.trim() {
            "network" => entry.0 = Some(unquote(v).to_ascii_lowercase()),
            "url" => entry.1 = Some(unquote(v)),
            _ => {}
        }
    }
}

fn finish(entries: &mut Vec<SyndicationEntry>, cur: (Option<String>, Option<String>)) {
    if let (Some(network), Some(url)) = cur {
        if !network.is_empty() && !url.is_empty() {
            entries.push(SyndicationEntry { network, url });
        }
    }
}

/// Parse a flow sequence like `[{network: a, url: b}, {...}]` (single line).
fn parse_flow_entries(raw: &str) -> Vec<SyndicationEntry> {
    let mut out = Vec::new();
    for chunk in raw.split('{').skip(1) {
        let body = chunk.split('}').next().unwrap_or("");
        let mut cur = (None, None);
        for kv in body.split(", ") {
            parse_kv_into(&mut cur, kv.trim().trim_end_matches(','));
        }
        finish(&mut out, cur);
    }
    out
}

/// Where the `syndication:` key lives, if anywhere.
struct SyndicationBlock {
    key_line_start: usize,
    key_line_end: usize,
    /// Inline value after `syndication:` (empty for block style).
    inline: String,
    /// End of the last non-blank continuation line (insertion point for block style).
    block_end: usize,
    item_indent: Option<String>,
    entries: Vec<SyndicationEntry>,
}

fn find_syndication(content: &str, span: FmSpan) -> Option<SyndicationBlock> {
    let lines = lines_with_offsets(content, span);
    let idx = lines
        .iter()
        .position(|(_, _, l)| top_level_value(l, "syndication").is_some())?;
    let (ks, ke, kl) = lines[idx];
    let inline = top_level_value(kl, "syndication").unwrap_or("").to_string();
    let mut block_end = ke;
    let mut item_indent = None;
    let mut entries = Vec::new();
    if inline.starts_with('[') {
        entries = parse_flow_entries(&inline);
    } else if inline.is_empty() {
        let mut cur: (Option<String>, Option<String>) = (None, None);
        for &(_, le, l) in &lines[idx + 1..] {
            if !is_continuation(l) {
                break;
            }
            if l.trim().is_empty() {
                continue;
            }
            block_end = le;
            let trimmed = l.trim_start();
            if let Some(rest) = trimmed.strip_prefix('-') {
                if item_indent.is_none() {
                    item_indent = Some(l[..l.len() - trimmed.len()].to_string());
                }
                finish(&mut entries, std::mem::take(&mut cur));
                let rest = rest.trim();
                if rest.starts_with('{') {
                    entries.extend(parse_flow_entries(rest));
                } else {
                    parse_kv_into(&mut cur, rest);
                }
            } else {
                parse_kv_into(&mut cur, trimmed);
            }
        }
        finish(&mut entries, cur);
    }
    Some(SyndicationBlock {
        key_line_start: ks,
        key_line_end: ke,
        inline,
        block_end,
        item_indent,
        entries,
    })
}

/// Existing `syndication:` entries in the note.
pub fn read_syndication(content: &str) -> Vec<SyndicationEntry> {
    locate(content)
        .and_then(|span| find_syndication(content, span))
        .map(|b| b.entries)
        .unwrap_or_default()
}

pub fn has_network(entries: &[SyndicationEntry], network: &str) -> bool {
    entries
        .iter()
        .any(|e| e.network.eq_ignore_ascii_case(network) && !e.url.is_empty())
}

/// Render a YAML scalar: plain when unambiguous, double-quoted otherwise.
pub fn yaml_scalar(s: &str) -> String {
    let plain_ok = !s.is_empty()
        && !s.contains(": ")
        && !s.contains(" #")
        && !s.ends_with(':')
        && !s.contains('\n')
        && !s.contains('"')
        && !s.starts_with(|c: char| "!&*[]{}|>'\"%@`#,?-:".contains(c) || c.is_whitespace())
        && !s.ends_with(char::is_whitespace)
        && !matches!(
            s.to_ascii_lowercase().as_str(),
            "true" | "false" | "yes" | "no" | "null" | "~" | "on" | "off"
        );
    if plain_ok {
        s.to_string()
    } else {
        format!(
            "\"{}\"",
            s.replace('\\', "\\\\")
                .replace('"', "\\\"")
                .replace('\n', " ")
        )
    }
}

fn render_items(entries: &[SyndicationEntry], indent: &str, nl: &str) -> String {
    let mut out = String::new();
    for e in entries {
        out.push_str(&format!(
            "{indent}- network: {}{nl}{indent}  url: {}{nl}",
            yaml_scalar(&e.network),
            yaml_scalar(&e.url)
        ));
    }
    out
}

/// Merge syndication entries into the note text. Entries for a network that
/// already has a URL are dropped (idempotent). Everything outside the inserted
/// lines is preserved byte-for-byte.
pub fn merge_syndication(content: &str, new: &[SyndicationEntry]) -> Result<String, String> {
    let existing = read_syndication(content);
    let mut fresh: Vec<SyndicationEntry> = Vec::new();
    for e in new {
        if !has_network(&existing, &e.network) && !has_network(&fresh, &e.network) {
            fresh.push(e.clone());
        }
    }
    if fresh.is_empty() {
        return Ok(content.to_string());
    }

    let Some(span) = locate(content) else {
        let block = format!(
            "---\nsyndication:\n{}---\n",
            render_items(&fresh, "  ", "\n")
        );
        return Ok(format!("{}{}", block, content));
    };
    let nl = newline_for(&content[..span.end.max(span.start)]);

    let Some(block) = find_syndication(content, span) else {
        // Append a new key at the end of the frontmatter.
        let insert = format!("syndication:{nl}{}", render_items(&fresh, "  ", nl));
        return Ok(splice(content, span.end, span.end, &insert));
    };

    if block.inline.is_empty() {
        let indent = block.item_indent.clone().unwrap_or_else(|| "  ".into());
        let insert = render_items(&fresh, &indent, nl);
        // block_end is the end of the last item line (includes its newline).
        return Ok(splice(content, block.block_end, block.block_end, &insert));
    }

    if block.inline.starts_with('[') && !block.inline.trim_end().ends_with(']') {
        return Err("syndication: multi-line flow sequence not supported; edit by hand".into());
    }
    if !block.inline.starts_with('[') && !matches!(block.inline.as_str(), "~" | "null") {
        return Err(format!(
            "syndication: unexpected scalar value '{}'; refusing to overwrite",
            block.inline
        ));
    }
    // Flow / null style: rewrite just that line as a block list.
    let mut all = block.entries.clone();
    all.extend(fresh);
    let replacement = format!("syndication:{nl}{}", render_items(&all, "  ", nl));
    Ok(splice(
        content,
        block.key_line_start,
        block.key_line_end,
        &replacement,
    ))
}

/// Set `key: value` only when the key is absent or empty. Inserted right after
/// `after_key`'s line when that key is a single-line scalar, otherwise at the
/// end of the frontmatter. Returns the content unchanged if the key has a value.
pub fn set_scalar_if_absent(
    content: &str,
    key: &str,
    value: &str,
    after_key: Option<&str>,
) -> Result<String, String> {
    if get_scalar(content, key).is_some() {
        return Ok(content.to_string());
    }
    let span = locate(content).ok_or("note has no frontmatter block")?;
    let nl = newline_for(&content[..span.end]);
    let new_line = format!("{}: {}{nl}", key, yaml_scalar(value));
    let lines = lines_with_offsets(content, span);

    // Existing-but-empty key: replace that line (only if it has no block below it).
    if let Some(i) = lines
        .iter()
        .position(|(_, _, l)| top_level_value(l, key).is_some())
    {
        let next_is_block = lines
            .get(i + 1)
            .map(|(_, _, l)| !l.trim().is_empty() && is_continuation(l))
            .unwrap_or(false);
        if next_is_block {
            return Err(format!("{} has a nested value; refusing to overwrite", key));
        }
        let (s, e, _) = lines[i];
        return Ok(splice(content, s, e, &new_line));
    }

    if let Some(after) = after_key {
        if let Some(i) = lines
            .iter()
            .position(|(_, _, l)| top_level_value(l, after).is_some_and(|v| !v.is_empty()))
        {
            let next_is_block = lines
                .get(i + 1)
                .map(|(_, _, l)| !l.trim().is_empty() && is_continuation(l))
                .unwrap_or(false);
            if !next_is_block {
                let (_, e, _) = lines[i];
                return Ok(splice(content, e, e, &new_line));
            }
        }
    }
    Ok(splice(content, span.end, span.end, &new_line))
}

/// Index range of a top-level single-line scalar `key:` line (refuses block values).
fn scalar_line(content: &str, span: FmSpan, key: &str) -> Result<Option<(usize, usize)>, String> {
    let lines = lines_with_offsets(content, span);
    let Some(i) = lines
        .iter()
        .position(|(_, _, l)| top_level_value(l, key).is_some())
    else {
        return Ok(None);
    };
    let raw = top_level_value(lines[i].2, key).unwrap_or("");
    let next_is_block = lines
        .get(i + 1)
        .map(|(_, _, l)| !l.trim().is_empty() && is_continuation(l))
        .unwrap_or(false);
    if next_is_block || matches!(raw, ">" | ">-" | ">+" | "|" | "|-" | "|+") {
        return Err(format!("{} has a multi-line value; edit it by hand", key));
    }
    Ok(Some((lines[i].0, lines[i].1)))
}

/// Set `key: value`, replacing an existing single-line value in place or
/// appending at the end of the frontmatter.
pub fn set_scalar(content: &str, key: &str, value: &str) -> Result<String, String> {
    let span = locate(content).ok_or("note has no frontmatter block")?;
    let nl = newline_for(&content[..span.end]);
    let new_line = format!("{}: {}{nl}", key, yaml_scalar(value));
    Ok(match scalar_line(content, span, key)? {
        Some((s, e)) => splice(content, s, e, &new_line),
        None => splice(content, span.end, span.end, &new_line),
    })
}

/// Remove a top-level single-line `key:` (e.g. `draft: true`). No-op if absent.
pub fn remove_scalar(content: &str, key: &str) -> Result<String, String> {
    let Some(span) = locate(content) else {
        return Ok(content.to_string());
    };
    Ok(match scalar_line(content, span, key)? {
        Some((s, e)) => splice(content, s, e, ""),
        None => content.to_string(),
    })
}

fn splice(content: &str, start: usize, end: usize, insert: &str) -> String {
    let mut out = String::with_capacity(content.len() + insert.len());
    out.push_str(&content[..start]);
    out.push_str(insert);
    out.push_str(&content[end..]);
    out
}

/// Body = everything after the closing fence line.
pub fn body(content: &str) -> &str {
    match locate(content) {
        Some(span) => {
            let rest = &content[span.end..];
            match rest.find('\n') {
                Some(i) => &rest[i + 1..],
                None => "",
            }
        }
        None => content,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOTE: &str = "---\ntitle: \"Water: who gets it\"\ndek: A dek with émoji 🌊\n# a comment\ntags: [water, data]\nimage: https://res.cloudinary.com/x/image/upload/v1/a.jpg\nsources:\n  - title: USGS\n    url: https://usgs.gov\nunlisted: false\n---\n\nBody line with --- dashes\n---\nand a fake fence above.\n";

    fn e(n: &str, u: &str) -> SyndicationEntry {
        SyndicationEntry {
            network: n.into(),
            url: u.into(),
        }
    }

    #[test]
    fn locate_finds_fences() {
        let span = locate(NOTE).unwrap();
        assert_eq!(&NOTE[..span.start], "---\n");
        assert!(NOTE[span.end..].starts_with("---\n\nBody"));
    }

    #[test]
    fn reads_scalars_and_lists() {
        assert_eq!(get_scalar(NOTE, "title").unwrap(), "Water: who gets it");
        assert_eq!(get_scalar(NOTE, "dek").unwrap(), "A dek with émoji 🌊");
        assert_eq!(get_list(NOTE, "tags"), vec!["water", "data"]);
        assert!(get_scalar(NOTE, "image_alt").is_none());
        assert!(!get_bool(NOTE, "unlisted"));
        let block = "---\ntags:\n  - a\n  - \"b c\"\ndek: >-\n  folded\n  text\n---\nx";
        assert_eq!(get_list(block, "tags"), vec!["a", "b c"]);
        assert_eq!(get_scalar(block, "dek").unwrap(), "folded text");
    }

    #[test]
    fn writeback_appends_new_key_preserving_everything_else() {
        let out = merge_syndication(
            NOTE,
            &[e("bluesky", "https://bsky.app/profile/ejfox.com/post/3abc")],
        )
        .unwrap();
        let span_old = locate(NOTE).unwrap();
        // Everything before the insertion point is identical...
        assert_eq!(&out[..span_old.end], &NOTE[..span_old.end]);
        // ...the body (from the closing fence on) is byte-for-byte identical...
        assert!(out.ends_with(&NOTE[span_old.end..]));
        assert_eq!(body(&out), body(NOTE));
        // ...and the only difference is the inserted block.
        assert_eq!(out.len() - NOTE.len(), "syndication:\n  - network: bluesky\n    url: https://bsky.app/profile/ejfox.com/post/3abc\n".len());
        assert_eq!(
            read_syndication(&out),
            vec![e("bluesky", "https://bsky.app/profile/ejfox.com/post/3abc")]
        );
        assert_eq!(get_scalar(&out, "title").unwrap(), "Water: who gets it");
    }

    #[test]
    fn writeback_extends_existing_block_and_is_idempotent() {
        let note = "---\ntitle: T\nsyndication:\n- network: mastodon\n  url: https://m.social/@ej/1\ndate: 2026-10-01\n---\nbody\n";
        let out = merge_syndication(
            note,
            &[
                e("mastodon", "https://m.social/@ej/2"), // dup network: dropped
                e("bluesky", "https://bsky.app/profile/x/post/y"),
            ],
        )
        .unwrap();
        assert_eq!(
            out,
            "---\ntitle: T\nsyndication:\n- network: mastodon\n  url: https://m.social/@ej/1\n- network: bluesky\n  url: https://bsky.app/profile/x/post/y\ndate: 2026-10-01\n---\nbody\n"
        );
        // Re-running is a no-op.
        let again = merge_syndication(&out, &[e("bluesky", "https://other")]).unwrap();
        assert_eq!(again, out);
    }

    #[test]
    fn writeback_handles_flow_and_crlf_and_no_frontmatter() {
        let flow = "---\r\nsyndication: []\r\ntitle: T\r\n---\r\nbody\r\n";
        let out = merge_syndication(flow, &[e("bluesky", "https://b/1")]).unwrap();
        assert_eq!(
            out,
            "---\r\nsyndication:\r\n  - network: bluesky\r\n    url: https://b/1\r\ntitle: T\r\n---\r\nbody\r\n"
        );
        let bare = "# Just a body\n";
        let out = merge_syndication(bare, &[e("mastodon", "https://m/1")]).unwrap();
        assert!(out.ends_with(bare));
        assert_eq!(read_syndication(&out), vec![e("mastodon", "https://m/1")]);
    }

    #[test]
    fn image_alt_inserted_after_image() {
        let out =
            set_scalar_if_absent(NOTE, "image_alt", "Map of: dry wells", Some("image")).unwrap();
        assert!(out.contains(
            "image: https://res.cloudinary.com/x/image/upload/v1/a.jpg\nimage_alt: \"Map of: dry wells\"\nsources:"
        ));
        assert_eq!(body(&out), body(NOTE));
        assert_eq!(get_scalar(&out, "image_alt").unwrap(), "Map of: dry wells");
        // Present → untouched.
        assert_eq!(
            set_scalar_if_absent(&out, "image_alt", "other", None).unwrap(),
            out
        );
    }

    #[test]
    fn set_and_remove_scalar_are_minimal() {
        let note = "---\ntitle: Old\ndraft: true\nbench: 2026-10-03/01-x\ntags:\n  - dispatch\n---\nBody\n";
        let titled = set_scalar(note, "title", "Army: Palantir's anchor").unwrap();
        assert_eq!(
            titled,
            "---\ntitle: \"Army: Palantir's anchor\"\ndraft: true\nbench: 2026-10-03/01-x\ntags:\n  - dispatch\n---\nBody\n"
        );
        let undrafted = remove_scalar(&titled, "draft").unwrap();
        assert_eq!(
            undrafted,
            "---\ntitle: \"Army: Palantir's anchor\"\nbench: 2026-10-03/01-x\ntags:\n  - dispatch\n---\nBody\n"
        );
        assert_eq!(remove_scalar(&undrafted, "draft").unwrap(), undrafted);
        assert!(remove_scalar(note, "tags").is_err(), "refuses block values");
    }
}
