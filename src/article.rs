//! On-demand full article text and inline image extraction.
//!
//! The engine never renders HTML. This module turns a fetched article page into
//! ordered blocks:
//!   - `text`  a paragraph / heading / list item, filtered for boilerplate
//!   - `image` an inline publisher photo, downloaded into the local cache
//!
//! Site chrome (nav, header, footer, scripts, forms) is removed first. Content
//! is taken from JSON-LD `articleBody`, the largest `<article>`/`<main>`, a
//! named content `<div>`, or the paragraph scan as a last resort.

use crate::images;
use crate::util::{
    attr_value, collapse_whitespace, http_get, is_http, resolve_url, strip_html, truncate_chars,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashSet;
use std::sync::Mutex;

const MAX_PAGE_BYTES: usize = 2 * 1024 * 1024;
const MAX_TEXT_CHARS: usize = 16_000;
const MIN_TEXT_CHARS: usize = 120;
const MAX_IMAGES: usize = 8;
const MAX_CAPTION_CHARS: usize = 220;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Block {
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub text: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub file: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub url: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub caption: String,
}

impl Block {
    pub fn text(value: String) -> Self {
        Self {
            kind: "text".into(),
            text: value,
            ..Self::default()
        }
    }

    pub fn image(url: String, caption: String) -> Self {
        Self {
            kind: "image".into(),
            url,
            caption,
            ..Self::default()
        }
    }
}

pub struct Extraction {
    pub text: String,
    pub blocks: Vec<Block>,
}

pub fn fetch(url: &str, exclude_image: &str, exclude_file: &str) -> Result<Extraction, String> {
    if !is_http(url) {
        return Err("article has no web URL".into());
    }
    let (bytes, _, _) = http_get(url, MAX_PAGE_BYTES, 12, None, None)?;
    let html = String::from_utf8_lossy(&bytes);
    let mut extraction = extract(&html, url, exclude_image);
    download_images(&mut extraction.blocks, exclude_file);
    extraction.text = text_of(&extraction.blocks);
    if extraction.text.chars().count() < MIN_TEXT_CHARS {
        return Err("could not extract readable article text".into());
    }
    Ok(extraction)
}

pub fn extract(html: &str, base_url: &str, exclude_image: &str) -> Extraction {
    let cleaned = remove_blocks(
        html,
        &[
            "script", "style", "noscript", "svg", "nav", "header", "footer", "aside", "form",
            "iframe", "button", "select", "template",
        ],
    );

    // Prefer visible HTML: it keeps photos next to the paragraph that
    // introduces them. JSON-LD is the fallback for JS-heavy pages.
    let mut scope = pick_container(&cleaned);
    let mut texts = text_blocks(scope);
    let mut total = text_chars(&texts);

    if total < MIN_TEXT_CHARS {
        if let Some(div) = content_div_segment(&cleaned) {
            let div_texts = text_blocks(div);
            let div_total = text_chars(&div_texts);
            if div_total > total {
                scope = div;
                texts = div_texts;
                total = div_total;
            }
            if total < MIN_TEXT_CHARS {
                let blob = strip_html(div);
                if blob.chars().count() > total {
                    scope = div;
                    texts = vec![(0, blob)];
                    total = text_chars(&texts);
                }
            }
        }
    }

    if total < MIN_TEXT_CHARS {
        scope = &cleaned;
        texts = text_blocks(scope);
        total = text_chars(&texts);
        if total < MIN_TEXT_CHARS {
            let blob = strip_html(scope);
            if blob.chars().count() > total {
                texts = vec![(0, blob)];
            }
        }
    }

    let images = collect_images(scope, base_url, exclude_image);
    let mut blocks = merge_blocks(texts, images);

    if text_of(&blocks).chars().count() >= 400 {
        return finish(blocks);
    }

    if let Some(body) = json_ld_article_body(html) {
        let text = clean_json_body(&body);
        if text.chars().count() >= 200 {
            let mut structured = vec![Block::text(cap_text(&text))];
            let mut structured_images = collect_images(scope, base_url, exclude_image);
            structured_images.truncate(MAX_IMAGES);
            for (_, image) in structured_images {
                structured.push(image);
            }
            return finish(structured);
        }
    }

    if text_of(&blocks).chars().count() < MIN_TEXT_CHARS {
        if let Some(description) = meta_description(html) {
            blocks.push(Block::text(description));
        }
    }
    finish(blocks)
}

fn choose_scope(html: &str) -> &str {
    let container = pick_container(html);
    if text_chars(&text_blocks(container)) > 200 {
        return container;
    }
    content_div_segment(html).unwrap_or(container)
}

fn text_chars(blocks: &[(usize, String)]) -> usize {
    blocks.iter().map(|(_, text)| text.chars().count()).sum()
}

fn text_of(blocks: &[Block]) -> String {
    blocks
        .iter()
        .filter(|block| block.kind == "text")
        .map(|block| block.text.as_str())
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn finish(blocks: Vec<Block>) -> Extraction {
    let text = cap_text(&text_of(&blocks));
    Extraction { text, blocks }
}

// --------------------------------------------------------------------- images

fn download_images(blocks: &mut Vec<Block>, exclude_file: &str) {
    let mut candidates: Vec<(usize, String)> = Vec::new();
    for (index, block) in blocks.iter().enumerate() {
        if block.kind == "image" && !block.url.is_empty() {
            candidates.push((index, block.url.clone()));
            if candidates.len() >= MAX_IMAGES {
                break;
            }
        }
    }
    if candidates.is_empty() {
        return;
    }

    let results: Mutex<Vec<(usize, String)>> = Mutex::new(Vec::new());
    let queue = Mutex::new(candidates);
    let workers = 4.min(queue.lock().map(|q| q.len()).unwrap_or(1)).max(1);
    std::thread::scope(|scope| {
        for _ in 0..workers {
            scope.spawn(|| loop {
                let next = {
                    let mut queue = queue.lock().unwrap();
                    queue.pop()
                };
                let Some((index, url)) = next else { break };
                if let Ok(path) = images::ensure_cached(&url) {
                    if !exclude_file.is_empty() && path == exclude_file {
                        continue;
                    }
                    // Author avatars, icons and ad pixels are often tiny; keep
                    // only files that look like real article photography.
                    let too_small = std::fs::metadata(&path)
                        .map(|meta| meta.len() < 8 * 1024)
                        .unwrap_or(true);
                    if !too_small {
                        results.lock().unwrap().push((index, path));
                    }
                }
            });
        }
    });
    for (index, path) in results.into_inner().unwrap() {
        if let Some(block) = blocks.get_mut(index) {
            block.file = path;
        }
    }
    let mut seen_files: HashSet<String> = HashSet::new();
    blocks.retain(|block| {
        block.kind != "image" || (!block.file.is_empty() && seen_files.insert(block.file.clone()))
    });
}

fn collect_images(html: &str, base_url: &str, exclude_image: &str) -> Vec<(usize, Block)> {
    let lower = html.to_ascii_lowercase();
    let mut out: Vec<(usize, Block)> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    let mut search = 0usize;

    while let Some(relative) = lower[search..].find("<img") {
        let start = search + relative;
        let boundary = lower[start + 4..].chars().next();
        if !matches!(boundary, Some(c) if c.is_whitespace() || c == '/' || c == '>') {
            search = start + 4;
            continue;
        }
        let Some(tag_end) = lower[start..].find('>') else {
            break;
        };
        let tag = &html[start..start + tag_end];

        if image_allowed(tag) {
            if let Some(raw) = image_source(tag) {
                let url = resolve_url(base_url, &raw);
                if !url.is_empty()
                    && url != exclude_image
                    && plausible_image(&url)
                    && seen.insert(
                        url.split(|c| c == '?' || c == '#')
                            .next()
                            .unwrap_or(&url)
                            .to_string(),
                    )
                {
                    let caption = figure_caption(html, &lower, start)
                        .or_else(|| attr_value(tag, "alt"))
                        .map(|value| {
                            truncate_chars(
                                &collapse_whitespace(&strip_html(&value)),
                                MAX_CAPTION_CHARS,
                            )
                        })
                        .unwrap_or_default();
                    out.push((start, Block::image(url, caption)));
                    if out.len() >= MAX_IMAGES {
                        break;
                    }
                }
            }
        }
        search = start + tag_end + 1;
    }
    out
}

fn image_source(tag: &str) -> Option<String> {
    for attribute in ["data-src", "data-original", "data-lazy-src", "data-url"] {
        if let Some(value) = attr_value(tag, attribute) {
            if !value.trim().is_empty() {
                return Some(value);
            }
        }
    }
    for attribute in ["srcset", "data-srcset"] {
        if let Some(value) = attr_value(tag, attribute) {
            if let Some(best) = best_srcset(&value) {
                return Some(best);
            }
        }
    }
    attr_value(tag, "src")
}

fn best_srcset(value: &str) -> Option<String> {
    let mut best: Option<(f64, String)> = None;
    for part in value.split(',') {
        let mut pieces = part.trim().split_whitespace();
        let Some(url) = pieces.next() else { continue };
        let weight = pieces
            .next()
            .and_then(|descriptor| descriptor.trim_end_matches(['w', 'x']).parse::<f64>().ok())
            .unwrap_or(0.0);
        if best
            .as_ref()
            .map(|(current, _)| weight >= *current)
            .unwrap_or(true)
        {
            best = Some((weight, url.to_string()));
        }
    }
    best.map(|(_, url)| url)
}

fn image_allowed(tag: &str) -> bool {
    let width = attr_value(tag, "width").and_then(|v| v.parse::<u32>().ok());
    let height = attr_value(tag, "height").and_then(|v| v.parse::<u32>().ok());
    if width.map(|w| w < 200).unwrap_or(false) || height.map(|h| h < 200).unwrap_or(false) {
        return false;
    }
    let alt = attr_value(tag, "alt")
        .unwrap_or_default()
        .to_ascii_lowercase();
    !["logo", "icon", "avatar", "sprite", "badge", "author"]
        .iter()
        .any(|needle| alt.contains(needle))
}

fn plausible_image(url: &str) -> bool {
    let lower = url.to_ascii_lowercase();
    if lower.starts_with("data:") || lower.ends_with(".svg") {
        return false;
    }
    ![
        "logo",
        "icon",
        "avatar",
        "sprite",
        "pixel",
        "spacer",
        "1x1",
        "tracking",
        "advert",
        "doubleclick",
        "googlesyndication",
        "google-analytics",
    ]
    .iter()
    .any(|needle| lower.contains(needle))
}

fn figure_caption(html: &str, lower: &str, image_start: usize) -> Option<String> {
    let before = &lower[..image_start];
    let figure_start = before.rfind("<figure")?;
    let previous_close = before.rfind("</figure>").map(|pos| pos + 9).unwrap_or(0);
    if figure_start < previous_close {
        return None;
    }
    let figure_end = lower[image_start..]
        .find("</figure>")
        .map(|pos| image_start + pos)
        .unwrap_or(html.len());
    let scope = &html[figure_start..figure_end];
    let scope_lower = scope.to_ascii_lowercase();
    let caption_start = scope_lower.find("<figcaption")?;
    let open_end = scope[caption_start..].find('>')? + caption_start + 1;
    let close = scope_lower[open_end..]
        .find("</figcaption")
        .map(|pos| open_end + pos)
        .unwrap_or(scope.len());
    let text = collapse_whitespace(&strip_html(&scope[open_end..close]));
    if text.is_empty() {
        None
    } else {
        Some(truncate_chars(&text, MAX_CAPTION_CHARS))
    }
}

// ----------------------------------------------------------------------- text

fn merge_blocks(texts: Vec<(usize, String)>, images: Vec<(usize, Block)>) -> Vec<Block> {
    let mut pieces: Vec<(usize, u8, Block)> = Vec::new();
    for (offset, text) in texts {
        pieces.push((offset, 0, Block::text(text)));
    }
    for (offset, image) in images {
        pieces.push((offset, 1, image));
    }
    pieces.sort_by_key(|(offset, order, _)| (*offset, *order));

    let mut out = Vec::new();
    let mut chars = 0usize;
    let mut images_added = 0usize;
    for (_, _, mut block) in pieces {
        if block.kind == "image" {
            if images_added < MAX_IMAGES {
                out.push(block);
                images_added += 1;
            }
            continue;
        }
        if chars >= MAX_TEXT_CHARS {
            break;
        }
        let remaining = MAX_TEXT_CHARS - chars;
        let count = block.text.chars().count();
        if count > remaining {
            block.text = truncate_chars(&block.text, remaining);
        }
        chars += block.text.chars().count();
        if !block.text.is_empty() {
            out.push(block);
        }
        if chars >= MAX_TEXT_CHARS {
            break;
        }
    }
    out
}

fn text_blocks(html: &str) -> Vec<(usize, String)> {
    let lower = html.to_ascii_lowercase();
    let mut blocks: Vec<(usize, String)> = Vec::new();
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
                                if !blocks.iter().any(|(_, b)| b.to_lowercase() == normalized) {
                                    blocks.push((start, text));
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
    blocks
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

/// Remove entire elements by tag name, including their content.
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
        search = end;
    }
    best
}

fn content_div_segment(html: &str) -> Option<&str> {
    let lower = html.to_ascii_lowercase();
    let keywords = [
        "article", "content", "story", "entry", "post", "prose", "body", "text",
    ];
    let mut strong: Vec<(usize, &str)> = Vec::new();
    let mut weak: Vec<(usize, &str)> = Vec::new();
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
        let segment = &html[start..end];
        let inner = &html[start + open_end + 1..end];
        let nested = lower[start + open_end + 1..end].contains("<div")
            || lower[start + open_end + 1..end].contains("<section");
        if !strong_match && nested {
            search = start + 4;
            continue;
        }
        let length = strip_html(inner).chars().count();
        if length > 300 {
            if strong_match {
                strong.push((length, segment));
            } else {
                weak.push((length, segment));
            }
        }
        search = start + 4;
    }

    let pool = if strong.is_empty() { weak } else { strong };
    let max = pool.iter().map(|(length, _)| *length).max()?;
    let threshold = std::cmp::max(500, max * 3 / 4);
    pool.into_iter()
        .filter(|(length, _)| *length >= threshold)
        .min_by_key(|(length, _)| *length)
        .map(|(_, segment)| segment)
}

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

    fn extract_text(html: &str) -> String {
        extract(html, "https://example.com/", "").text
    }

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
        let text = extract_text(html);
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
        assert_eq!(extract_text(&html), body);
    }

    #[test]
    fn falls_back_to_meta_description() {
        let html = r#"<html><head><meta name="description" content="This is a fallback description long enough to be shown when a page has no readable paragraphs at all in its body."></head><body><div></div></body></html>"#;
        let text = extract_text(html);
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
        let text = extract_text(html);
        assert!(text.contains("approved the new budget"));
        assert!(text.contains("take effect in January"));
        assert!(!text.contains("newsletter"));
        assert!(!text.contains("Read more"));
    }

    #[test]
    fn caps_very_long_articles() {
        let paragraph = "Sentence about the topic that keeps going for a while. ".repeat(200);
        let html = format!("<article><p>{}</p></article>", paragraph);
        let text = extract_text(&html);
        assert!(text.chars().count() <= MAX_TEXT_CHARS + 1);
    }

    #[test]
    fn extracts_inline_image_with_caption() {
        let html = r#"<article>
          <p>First paragraph with enough words to be kept by the extraction filter and provide context.</p>
          <figure>
            <img data-src="../img/photo.jpg" alt="Photo alt" width="800" height="500">
            <figcaption>A publisher caption for the photo.</figcaption>
          </figure>
          <p>Second paragraph also long enough to survive and follow the photograph in the article.</p>
        </article>"#;
        let extracted = extract(html, "https://example.com/news/story", "");
        let image = extracted
            .blocks
            .iter()
            .find(|block| block.kind == "image")
            .expect("image block");
        assert_eq!(image.url, "https://example.com/img/photo.jpg");
        assert_eq!(image.caption, "A publisher caption for the photo.");
        let image_index = extracted
            .blocks
            .iter()
            .position(|block| block.kind == "image")
            .unwrap();
        let text_index = extracted
            .blocks
            .iter()
            .position(|block| block.kind == "text")
            .unwrap();
        assert!(image_index > text_index);
    }

    #[test]
    fn picks_largest_srcset_and_skips_tracking() {
        assert_eq!(
            best_srcset("small.jpg 400w, big.jpg 1200w, mid.jpg 800w").as_deref(),
            Some("big.jpg")
        );
        let html = r#"<article><p>Long enough paragraph for the extractor to keep this article body intact.</p>
          <img src="https://example.com/pixel.gif" width="1" height="1">
          <img src="https://example.com/logo.png" alt="Site logo" width="600" height="200">
        </article>"#;
        let extracted = extract(html, "https://example.com/", "");
        assert!(!extracted.blocks.iter().any(|block| block.kind == "image"));
    }

    #[test]
    fn extracts_nested_content_div_without_paragraphs() {
        let long = "The committee approved the new budget after a long debate that lasted well into the night, with several amendments attached to the final text. ";
        let html = format!(
            r#"<html><body><nav>Home World</nav>
            <div class="content"><div class="advert">Advertisement</div>{} {}</div>
            <footer>All rights reserved</footer></body></html>"#,
            long, long
        );
        let text = extract_text(&html);
        assert!(text.contains("committee approved"), "got: {}", text);
    }
}
