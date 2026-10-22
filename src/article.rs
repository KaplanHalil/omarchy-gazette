//! On-demand full article text extraction.
//!
//! The engine never renders HTML. This module turns a fetched article page into
//! plain paragraphs using a small readability heuristic:
//!   1. JSON-LD `articleBody` when the publisher provides it
//!   2. the largest `<article>` / `<main>` container
//!   3. otherwise every `<p>`/heading block in the document
//!
//! Site chrome (nav, header, footer, scripts, forms) is removed first, and
//! boilerplate lines (cookies, newsletters, "read more", share prompts) are
//! filtered out.

use crate::util::{attr_value, collapse_whitespace, http_get, is_http, strip_html, truncate_chars};
use serde_json::Value;

const MAX_PAGE_BYTES: usize = 2 * 1024 * 1024;
const MAX_TEXT_CHARS: usize = 16_000;
const MIN_TEXT_CHARS: usize = 120;

pub fn fetch(url: &str) -> Result<String, String> {
    if !is_http(url) {
        return Err("article has no web URL".into());
    }
    let (bytes, _, _) = http_get(url, MAX_PAGE_BYTES, 12, None, None)?;
    let html = String::from_utf8_lossy(&bytes);
    let text = extract(&html);
    if text.chars().count() < MIN_TEXT_CHARS {
        return Err("could not extract readable article text".into());
    }
    Ok(text)
}

pub fn extract(html: &str) -> String {
    if let Some(body) = json_ld_article_body(html) {
        let clean = clean_json_body(&body);
        if clean.chars().count() >= 200 {
            return cap_text(&clean);
        }
    }

    let cleaned = remove_blocks(
        html,
        &[
            "script", "style", "noscript", "svg", "nav", "header", "footer", "aside", "form",
            "iframe", "button", "select", "template",
        ],
    );
    let container = pick_container(&cleaned);
    let mut text = blocks_to_text(container);
    if let Some(div_text) = content_div_text(&cleaned) {
        // Div-based articles (Phoronix and older CMSes) have few or no <p>
        // tags. Prefer the named/leaf content container when it carries
        // substantially more prose than the paragraph scan.
        if div_text.chars().count() > text.chars().count() + 200 {
            text = div_text;
        }
    }
    if text.chars().count() < 120 {
        text = blocks_to_text(&cleaned);
    }
    if text.chars().count() < 120 {
        let fallback = strip_html(&cleaned);
        if fallback.chars().count() > text.chars().count() {
            text = fallback;
        }
    }
    if text.chars().count() < MIN_TEXT_CHARS {
        if let Some(description) = meta_description(html) {
            text = description;
        }
    }
    cap_text(&text)
}

/// Remove entire elements by tag name, including their content. Only used for
/// elements that do not nest inside themselves (script/style/nav/...).
fn remove_blocks(html: &str, tags: &[&str]) -> String {
    let lower = html.to_ascii_lowercase();
    let mut out = String::with_capacity(html.len());
    let mut i = 0usize;
    while i < html.len() {
        let Some(relative) = lower[i..].find('<') else {
            out.push_str(&html[i..]);
            break;
        };
        let start = i + relative;
        let after = &lower[start + 1..];
        let closing = after.starts_with('/');
        let name: String = after
            .trim_start_matches('/')
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric())
            .collect();
        if !closing && tags.contains(&name.as_str()) {
            let close = format!("</{}", name);
            if let Some(end_relative) = lower[start..].find(&close) {
                if let Some(gt) = lower[start + end_relative..].find('>') {
                    i = start + end_relative + gt + 1;
                    continue;
                }
            } else if let Some(gt) = lower[start..].find('>') {
                // Void or self-closing element: drop just the opening tag.
                i = start + gt + 1;
                continue;
            }
        }
        out.push_str(&html[i..start + 1]);
        i = start + 1;
    }
    out
}

fn pick_container(html: &str) -> &str {
    for tag in ["article", "main"] {
        if let Some(segment) = largest_element(html, tag) {
            if strip_html(segment).chars().count() > 200 {
                return segment;
            }
        }
    }
    html
}

fn largest_element<'a>(html: &'a str, tag: &str) -> Option<&'a str> {
    let lower = html.to_ascii_lowercase();
    let open = format!("<{}", tag);
    let close = format!("</{}>", tag);
    let mut best: Option<&str> = None;
    let mut best_len = 0usize;
    let mut search = 0usize;

    while let Some(relative) = lower[search..].find(&open) {
        let start = search + relative;
        let boundary = lower[start + open.len()..].chars().next();
        if !matches!(boundary, Some(c) if c.is_whitespace() || c == '>' || c == '/') {
            search = start + open.len();
            continue;
        }
        let Some(end_relative) = lower[start..].find(&close) else {
            break;
        };
        let end = start + end_relative + close.len();
        let segment = &html[start..end];
        let length = strip_html(segment).chars().count();
        if length > best_len {
            best_len = length;
            best = Some(segment);
        }
        search = start + 4;
    }
    best
}

/// Find the largest leaf `<div>`/`<section>` whose class or id looks like an
/// article body. This covers CMSes that never emit `<p>` tags.
fn content_div_text(html: &str) -> Option<String> {
    let lower = html.to_ascii_lowercase();
    let keywords = [
        "article", "content", "story", "entry", "post", "prose", "body", "text",
    ];
    let mut strong: Vec<(usize, String)> = Vec::new();
    let mut weak: Vec<(usize, String)> = Vec::new();
    let mut examined = 0usize;
    let mut search = 0usize;

    while examined < 300 {
        let Some(relative) = lower[search..].find("<div") else {
            break;
        };
        let start = search + relative;
        let boundary = lower[start + 4..].chars().next();
        if !matches!(boundary, Some(c) if c.is_whitespace() || c == '>') {
            search = start + 4;
            continue;
        }
        let Some(open_end) = lower[start..].find('>') else {
            break;
        };
        let opening = &html[start..start + open_end];
        let attributes = format!(
            "{} {}",
            attr_value(opening, "class").unwrap_or_default(),
            attr_value(opening, "id").unwrap_or_default()
        )
        .to_ascii_lowercase();
        if !keywords.iter().any(|key| attributes.contains(key)) {
            search = start + 4;
            continue;
        }
        let strong_match = [
            "article-body",
            "articlebody",
            "article-content",
            "article__content",
            "article_body",
            "story-body",
            "story-content",
            "entry-content",
            "post-content",
            "content-body",
            "prose",
        ]
        .iter()
        .any(|key| attributes.contains(key))
            || attributes
                .split_whitespace()
                .any(|token| matches!(token, "content" | "article" | "prose"));
        examined += 1;

        let Some(end) = element_end(&lower, start, "div") else {
            break;
        };
        let inner = &html[start + open_end + 1..end];
        let nested = lower[start + open_end + 1..end].contains("<div")
            || lower[start + open_end + 1..end].contains("<section");
        if !strong_match && nested {
            // Do not jump to `end`: the real content div may be nested inside
            // this wrapper (site-content, page-wrapper, ...).
            search = start + 4;
            continue;
        }
        let text = strip_html(inner);
        let length = text.chars().count();
        if length > 300 {
            if strong_match {
                strong.push((length, text));
            } else {
                weak.push((length, text));
            }
        }
        search = start + 4;
    }

    let pool = if strong.is_empty() { weak } else { strong };
    let max = pool.iter().map(|(length, _)| *length).max()?;
    // Prefer the most specific (smallest) container that still carries most
    // of the prose; this drops page wrappers that repeat the headline.
    let threshold = std::cmp::max(500, max * 3 / 4);
    pool.into_iter()
        .filter(|(length, _)| *length >= threshold)
        .min_by_key(|(length, _)| *length)
        .map(|(_, text)| text)
}

/// Byte offset of the matching close tag for `tag`, counting nesting.
fn element_end(lower: &str, start: usize, tag: &str) -> Option<usize> {
    let open = format!("<{}", tag);
    let close = format!("</{}>", tag);
    let mut depth = 0usize;
    let mut cursor = start;
    while cursor < lower.len() {
        let next_open = lower[cursor..].find(&open).map(|p| cursor + p);
        let next_close = lower[cursor..].find(&close).map(|p| cursor + p);
        match (next_open, next_close) {
            (Some(o), Some(c)) if o < c => {
                depth += 1;
                cursor = o + open.len();
            }
            (_, Some(c)) => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return Some(c);
                }
                cursor = c + close.len();
            }
            (Some(o), None) => {
                depth += 1;
                cursor = o + open.len();
            }
            (None, None) => break,
        }
    }
    None
}

fn blocks_to_text(html: &str) -> String {
    let lower = html.to_ascii_lowercase();
    let mut blocks: Vec<String> = Vec::new();
    let mut i = 0usize;

    while i < html.len() {
        let Some(relative) = lower[i..].find('<') else {
            break;
        };
        let start = i + relative;
        let after = &lower[start + 1..];
        let closing = after.starts_with('/');
        let name: String = after
            .trim_start_matches('/')
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric())
            .collect();

        if !closing && matches!(name.as_str(), "p" | "h2" | "h3" | "blockquote" | "li") {
            let boundary = lower[start + 1 + name.len()..].chars().next();
            let valid_boundary =
                matches!(boundary, Some(c) if c.is_whitespace() || c == '>' || c == '/');
            if valid_boundary {
                if let Some(end_relative) = lower[start..].find(&format!("</{}", name)) {
                    if let Some(gt) = lower[start..].find('>') {
                        let inner_start = start + gt + 1;
                        let inner_end = start + end_relative;
                        if inner_end > inner_start {
                            let raw = &html[inner_start..inner_end];
                            let text = collapse_whitespace(&strip_html(raw));
                            if accept_block(&text, name.starts_with('h')) {
                                let normalized = text.to_lowercase();
                                if !blocks.iter().any(|b| b.to_lowercase() == normalized) {
                                    blocks.push(text);
                                }
                            }
                        }
                        i = start + end_relative;
                        if let Some(gt) = lower[i..].find('>') {
                            i += gt + 1;
                        }
                        continue;
                    }
                }
            }
        }
        i = start + 1;
    }
    blocks.join("\n\n")
}

fn accept_block(text: &str, heading: bool) -> bool {
    let length = text.chars().count();
    if length < if heading { 12 } else { 40 } {
        return false;
    }
    let lower = text.to_lowercase();
    const PREFIXES: &[&str] = &[
        "subscribe",
        "sign up",
        "sign in",
        "log in",
        "advertisement",
        "read more",
        "related",
        "recommended",
        "follow us",
        "share this",
        "share on",
        "download the",
        "get the app",
        "listen to",
        "watch:",
        "photo:",
        "image:",
        "getty",
        "all rights reserved",
        "©",
        "cookie",
        "privacy policy",
        "terms of",
        "we use cookies",
        "this site is protected",
        "support our journalism",
        "become a subscriber",
        "already a subscriber",
        "newsletter",
        "contact us",
        "advertise with",
        "jobs at",
        "about us",
        "published",
        "updated",
    ];
    if PREFIXES.iter().any(|prefix| lower.starts_with(prefix)) {
        return false;
    }
    // Related-content carousels often expose assistive text ("list 1 of 3")
    // inside <li> elements. Drop those, not real bullet lists.
    if lower.starts_with("list ")
        || (lower.starts_with("image ") && lower.contains(" of "))
        || (lower.starts_with("slide ") && lower.contains(" of "))
    {
        return false;
    }
    const CONTAINS: &[&str] = &[
        "javascript is disabled",
        "enable javascript",
        "sign up to our newsletter",
        "click here to subscribe",
        "this content is not available",
        "accessibility help",
        "skip to main content",
    ];
    !CONTAINS.iter().any(|needle| lower.contains(needle))
}

fn meta_description(html: &str) -> Option<String> {
    let lower = html.to_ascii_lowercase();
    let mut search = 0usize;
    while let Some(relative) = lower[search..].find("<meta") {
        let start = search + relative;
        let end = start + lower[start..].find('>')?;
        let tag = &html[start..end];
        let key = attr_value(tag, "property")
            .or_else(|| attr_value(tag, "name"))
            .unwrap_or_default()
            .to_ascii_lowercase();
        if key == "og:description" || key == "description" || key == "twitter:description" {
            if let Some(content) = attr_value(tag, "content") {
                let text = collapse_whitespace(&content);
                if text.chars().count() >= 80 {
                    return Some(text);
                }
            }
        }
        search = end + 1;
    }
    None
}

fn json_ld_article_body(html: &str) -> Option<String> {
    let lower = html.to_ascii_lowercase();
    let mut search = 0usize;
    while let Some(relative) = lower[search..].find("application/ld+json") {
        let marker = search + relative;
        let start = lower[..marker].rfind("<script").unwrap_or(marker);
        let open_end = start + lower[start..].find('>')?;
        let content_start = open_end + 1;
        let close = content_start + lower[content_start..].find("</script")?;
        let json = &html[content_start..close];
        if let Ok(value) = serde_json::from_str::<Value>(json) {
            if let Some(body) = find_key(&value, "articleBody") {
                if body.chars().count() >= 200 {
                    return Some(body);
                }
            }
        }
        search = close + 8;
    }
    None
}

fn find_key(value: &Value, key: &str) -> Option<String> {
    match value {
        Value::Object(map) => {
            if let Some(Value::String(text)) = map.get(key) {
                if text.chars().count() >= 200 {
                    return Some(text.clone());
                }
            }
            for child in map.values() {
                if let Some(found) = find_key(child, key) {
                    return Some(found);
                }
            }
            None
        }
        Value::Array(items) => {
            for child in items {
                if let Some(found) = find_key(child, key) {
                    return Some(found);
                }
            }
            None
        }
        _ => None,
    }
}

fn clean_json_body(raw: &str) -> String {
    let text = if raw.contains('<') {
        strip_html(raw)
    } else {
        raw.to_string()
    };
    let mut parts = Vec::new();
    for line in text.split('\n') {
        let clean = collapse_whitespace(line);
        if clean.chars().count() >= 30 {
            parts.push(clean);
        }
    }
    if parts.is_empty() {
        collapse_whitespace(&text)
    } else {
        parts.join("\n\n")
    }
}

fn cap_text(text: &str) -> String {
    if text.chars().count() <= MAX_TEXT_CHARS {
        return text.trim().to_string();
    }
    let sliced: String = text.chars().take(MAX_TEXT_CHARS).collect();
    if let Some(position) = sliced.rfind(". ") {
        if position > MAX_TEXT_CHARS / 2 {
            return sliced[..position + 1].to_string();
        }
    }
    truncate_chars(&sliced, MAX_TEXT_CHARS)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_article_and_ignores_chrome() {
        let html = r#"
        <html><head><title>Story</title></head><body>
          <header><nav><a>Home</a><a>World</a></nav></header>
          <div class="cookie-banner">We use cookies to improve your experience.</div>
          <article>
            <h1>A real headline</h1>
            <p>First paragraph with enough words to survive the boilerplate filter, talking about the actual event.</p>
            <p>Second paragraph continues the report with more useful detail and a little context for the reader.</p>
            <div class="share">Share this article on your favourite social network.</div>
          </article>
          <footer>All rights reserved. Contact us. Privacy policy.</footer>
          <script>console.log("secret")</script>
        </body></html>"#;
        let text = extract(html);
        assert!(text.contains("First paragraph"));
        assert!(text.contains("Second paragraph"));
        assert!(!text.contains("secret"));
        assert!(!text.contains("cookies"));
        assert!(!text.contains("All rights reserved"));
        assert!(!text.contains("Home"));
    }

    #[test]
    fn uses_json_ld_article_body() {
        let body = "This is the complete article body provided by the publisher in structured data. \
                    It is long enough to be preferred over scraping the visible page, and it contains \
                    several sentences of actual reporting about the topic at hand.";
        let html = format!(
            r#"<html><head><script type="application/ld+json">{{"@type":"NewsArticle","articleBody":"{}"}}</script></head>
               <body><nav>Menu</nav><article><p>short</p></article></body></html>"#,
            body
        );
        assert_eq!(extract(&html), body);
    }

    #[test]
    fn falls_back_to_meta_description() {
        let html = r#"<html><head><meta name="description" content="This is a fallback description long enough to be shown when a page has no readable paragraphs at all in its body."></head><body><div></div></body></html>"#;
        let text = extract(html);
        assert!(text.starts_with("This is a fallback description"));
    }

    #[test]
    fn filters_newsletter_and_navigation_items() {
        let html = r#"<body>
          <p>Subscribe to our newsletter for the latest updates delivered to your inbox every morning.</p>
          <p>The committee approved the new budget after a long debate that lasted well into the night.</p>
          <p>Read more: another story you might have missed this week.</p>
          <p>Officials said the measure would take effect in January and apply to all districts.</p>
        </body>"#;
        let text = extract(html);
        assert!(text.contains("approved the new budget"));
        assert!(text.contains("take effect in January"));
        assert!(!text.contains("newsletter"));
        assert!(!text.contains("Read more"));
    }

    #[test]
    fn caps_very_long_articles() {
        let paragraph = "Sentence about the topic that keeps going for a while. ".repeat(200);
        let html = format!("<article><p>{}</p></article>", paragraph);
        let text = extract(&html);
        assert!(text.chars().count() <= MAX_TEXT_CHARS + 1);
    }
}

#[cfg(test)]
mod div_tests {
    use super::*;

    #[test]
    fn extracts_nested_content_div_without_paragraphs() {
        let long = "The committee approved the new budget after a long debate that lasted well into the night, with several amendments attached to the final text. ";
        let html = format!(
            r#"<html><body><nav>Home World</nav>
            <div class="content"><div class="advert">Advertisement</div>{} {}</div>
            <footer>All rights reserved</footer></body></html>"#,
            long, long
        );
        let text = extract(&html);
        assert!(text.contains("committee approved"), "got: {}", text);
    }
}
