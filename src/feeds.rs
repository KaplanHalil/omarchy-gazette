//! Feed fetching, parsing and merging.
//!
//! RSS 2.0, Atom, RSS 1.0 and JSON Feed are handled by feed-rs. Media RSS
//! thumbnails, enclosures and inline `<img>` tags are reduced to a single
//! thumbnail URL per article, which is cached locally by `images`.

use crate::config::save_store;
use crate::images;
use crate::model::{Article, Config, FeedCfg, Pending, Store, TabIndex};
use crate::util::{
    attr_value, first_img_src, is_http, now, resolve_url, sha_hex, strip_html, truncate_chars,
};
use feed_rs::model::{Entry, Feed};
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::sync::Mutex;

#[derive(Serialize)]
pub struct FeedError {
    pub feed: String,
    pub error: String,
}

#[derive(Serialize)]
pub struct RefreshReport {
    pub ok: bool,
    pub new_count: usize,
    pub updated: usize,
    pub total: usize,
    pub errors: Vec<FeedError>,
    pub pending: Vec<Pending>,
    pub fetched_at: i64,
}

struct ParsedEntry {
    id: String,
    title: String,
    url: String,
    summary: String,
    content: String,
    author: String,
    published: i64,
    image_url: String,
}

struct FetchOutcome {
    feed_id: String,
    feed_title: String,
    site_url: String,
    etag: String,
    last_modified: String,
    entries: Vec<ParsedEntry>,
    error: String,
}

pub fn refresh(config: &mut Config, store: &mut Store) -> RefreshReport {
    let feeds: Vec<FeedCfg> = config.feeds.iter().filter(|f| f.enabled).cloned().collect();
    let limit = config.settings.per_feed_limit.max(5);
    let outcomes: Mutex<Vec<FetchOutcome>> = Mutex::new(Vec::new());
    let queue = Mutex::new(feeds);
    let worker_count = 6.min(queue.lock().map(|q| q.len()).unwrap_or(1)).max(1);

    std::thread::scope(|scope| {
        for _ in 0..worker_count {
            scope.spawn(|| loop {
                let feed = {
                    let mut q = queue.lock().unwrap();
                    match q.pop() {
                        Some(f) => f,
                        None => break,
                    }
                };
                let outcome = fetch_feed(&feed, limit);
                outcomes.lock().unwrap().push(outcome);
            });
        }
    });

    let outcomes = outcomes.into_inner().unwrap();
    let first_run = !store.meta.has_refreshed;
    let mut new_articles: Vec<Article> = Vec::new();
    let mut errors: Vec<FeedError> = Vec::new();
    let mut updated_count = 0usize;

    let index: HashMap<String, usize> = store
        .articles
        .iter()
        .enumerate()
        .map(|(i, a)| (a.id.clone(), i))
        .collect();
    let mut index = index;

    for outcome in outcomes {
        if let Some(feed) = config.feeds.iter_mut().find(|f| f.id == outcome.feed_id) {
            feed.last_fetch = now();
            if !outcome.error.is_empty() {
                feed.last_error = outcome.error.clone();
            } else {
                feed.last_error.clear();
            }
            if !outcome.feed_title.is_empty() {
                feed.title = outcome.feed_title;
            }
            if !outcome.site_url.is_empty() {
                feed.site_url = outcome.site_url;
            }
            feed.etag = outcome.etag;
            feed.last_modified = outcome.last_modified;
            if !outcome.error.is_empty() {
                errors.push(FeedError {
                    feed: feed.title.clone(),
                    error: outcome.error,
                });
            }
        }

        for parsed in outcome.entries {
            if let Some(&pos) = index.get(&parsed.id) {
                let existing = &mut store.articles[pos];
                if existing.title != parsed.title {
                    existing.title = parsed.title;
                }
                if !parsed.summary.is_empty() && existing.summary != parsed.summary {
                    existing.summary = parsed.summary;
                }
                if !parsed.content.is_empty() && existing.content != parsed.content {
                    existing.content = parsed.content;
                }
                if existing.url.is_empty() {
                    existing.url = parsed.url;
                }
                if existing.image_url != parsed.image_url {
                    existing.image_url = parsed.image_url;
                    existing.image_path.clear();
                }
                if existing.author.is_empty() {
                    existing.author = parsed.author;
                }
                updated_count += 1;
                continue;
            }

            let article = Article {
                id: parsed.id.clone(),
                feed_id: outcome.feed_id.clone(),
                title: parsed.title,
                url: parsed.url,
                summary: parsed.summary,
                content: parsed.content,
                author: parsed.author,
                published: parsed.published,
                fetched: now(),
                image_url: parsed.image_url,
                image_path: String::new(),
                read: false,
                bookmarked: false,
            };
            index.insert(article.id.clone(), store.articles.len());
            new_articles.push(article);
        }
    }

    let new_count = new_articles.len();
    store.articles.extend(new_articles);

    // Notifications are queued only when the user has already seen the feed
    // once, so a first import does not fire hundreds of notifications.
    let mut pending = Vec::new();
    if !first_run && config.settings.notifications {
        let min_gap = config.settings.notify_min_interval_minutes as i64 * 60;
        let can_notify = now() - store.meta.last_notify >= min_gap;
        if can_notify {
            let tab_index = TabIndex {
                tabs: config.tabs.clone(),
            };
            let feed_names: HashMap<&str, &str> = config
                .feeds
                .iter()
                .map(|f| (f.id.as_str(), f.title.as_str()))
                .collect();
            for article in store.articles.iter().rev().take(new_count).rev() {
                let haystack = format!("{} {}", article.title, article.summary);
                let tabs = tab_index.tabs_for(&article.feed_id, &haystack);
                let notify = if tabs.is_empty() {
                    true
                } else {
                    config
                        .tabs
                        .iter()
                        .filter(|t| tabs.contains(&t.id))
                        .any(|t| t.notify)
                };
                if notify {
                    pending.push(Pending {
                        title: truncate_chars(&article.title, 90),
                        feed: feed_names
                            .get(article.feed_id.as_str())
                            .copied()
                            .unwrap_or("Gazette")
                            .to_string(),
                    });
                    if pending.len() >= 5 {
                        break;
                    }
                }
            }
            if !pending.is_empty() {
                store.meta.last_notify = now();
            }
        }
    }

    // Cache thumbnails for the newest articles that do not have one yet. A
    // first run gets a larger budget so the initial view is not a wall of
    // placeholders. Turning images off also stops network image work.
    if config.settings.show_images {
        cache_thumbnails(store, if first_run { 100 } else { 24 });
    }

    store.articles.sort_by(|a, b| b.published.cmp(&a.published));
    let max_articles = config.settings.max_articles.max(100);
    let mut kept: Vec<Article> = Vec::with_capacity(store.articles.len());
    for (pos, article) in store.articles.drain(..).enumerate() {
        if pos < max_articles || article.bookmarked {
            kept.push(article);
        }
    }
    store.articles = kept;

    store.meta.last_refresh = now();
    store.meta.has_refreshed = true;
    if !pending.is_empty() {
        store.meta.pending = pending.clone();
    }

    let _ = save_store(store);

    let referenced: HashSet<String> = store
        .articles
        .iter()
        .filter(|a| !a.image_path.is_empty())
        .map(|a| a.image_path.clone())
        .collect();
    images::cleanup(referenced);

    RefreshReport {
        ok: true,
        new_count,
        updated: updated_count,
        total: store.articles.len(),
        errors,
        pending,
        fetched_at: now(),
    }
}

fn cache_thumbnails(store: &mut Store, budget: usize) {
    // Some feeds (Al Jazeera, Phoronix, Hugging Face…) carry no image in the
    // feed at all. Fetch the article page's og:image for the newest handful so
    // those feeds are not permanently text-only. Once set, the URL is reused.
    let mut unknown: Vec<(usize, String)> = Vec::new();
    for (index, article) in store.articles.iter().enumerate() {
        if article.image_url.is_empty() && !article.url.is_empty() && is_http(&article.url) {
            unknown.push((index, article.url.clone()));
            if unknown.len() >= 12 {
                break;
            }
        }
    }
    if !unknown.is_empty() {
        let found: Mutex<Vec<(usize, String)>> = Mutex::new(Vec::new());
        let queue = Mutex::new(unknown);
        std::thread::scope(|scope| {
            for _ in 0..3 {
                scope.spawn(|| loop {
                    let next = {
                        let mut q = queue.lock().unwrap();
                        q.pop()
                    };
                    let Some((index, page)) = next else { break };
                    if let Some(image) = fetch_og_image(&page) {
                        found.lock().unwrap().push((index, image));
                    }
                });
            }
        });
        for (index, image) in found.into_inner().unwrap() {
            if let Some(article) = store.articles.get_mut(index) {
                if article.image_url.is_empty() {
                    article.image_url = image;
                }
            }
        }
    }

    let mut candidates: Vec<(usize, String, i64)> = Vec::new();
    for (index, article) in store.articles.iter().enumerate() {
        if !article.image_url.is_empty() && article.image_path.is_empty() {
            candidates.push((index, article.image_url.clone(), article.published));
        }
    }
    candidates.sort_by(|a, b| b.2.cmp(&a.2));
    candidates.truncate(budget);
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
                    let mut q = queue.lock().unwrap();
                    q.pop()
                };
                let Some((index, url, _)) = next else { break };
                if let Ok(path) = images::ensure_cached(&url) {
                    results.lock().unwrap().push((index, path));
                }
            });
        }
    });
    for (index, path) in results.into_inner().unwrap() {
        if let Some(article) = store.articles.get_mut(index) {
            article.image_path = path;
        }
    }
}

/// Extract `og:image` / `twitter:image` from an article page as a thumbnail
/// fallback. Pages are capped and failures are silent.
fn fetch_og_image(page: &str) -> Option<String> {
    let (bytes, _, _) = crate::util::http_get(page, 768 * 1024, 8, None, None).ok()?;
    let html = String::from_utf8_lossy(&bytes);
    let lower = html.to_ascii_lowercase();
    let mut search = 0usize;
    while let Some(found) = lower[search..].find("<meta") {
        let start = search + found;
        let end = start + lower[start..].find('>')?;
        let tag = &html[start..end];
        let property = attr_value(tag, "property")
            .or_else(|| attr_value(tag, "name"))
            .unwrap_or_default()
            .to_ascii_lowercase();
        if matches!(
            property.as_str(),
            "og:image" | "og:image:url" | "twitter:image" | "twitter:image:src"
        ) {
            if let Some(content) = attr_value(tag, "content") {
                let resolved = resolve_url(page, &crate::util::decode_entities(&content));
                if !resolved.is_empty() && plausible_image(&resolved) {
                    return Some(resolved);
                }
            }
        }
        search = end + 1;
    }
    None
}

/// Feed titles are often long SEO sentences ("Al Jazeera – Breaking News…").
/// Keep the readable prefix so badges stay compact.
fn clean_title(raw: &str) -> String {
    let mut title = strip_html(raw);
    for separator in [" – ", " — ", " | ", " - ", " :: ", " » "] {
        if let Some(pos) = title.find(separator) {
            title.truncate(pos);
        }
    }
    truncate_chars(title.trim(), 40)
}

pub fn fetch_article_image(store: &mut Store, id: &str) -> Result<String, String> {
    let article = store
        .articles
        .iter_mut()
        .find(|a| a.id == id)
        .ok_or_else(|| "article not found".to_string())?;
    if !article.image_path.is_empty() {
        return Ok(article.image_path.clone());
    }
    if article.image_url.is_empty() {
        return Err("article has no image".into());
    }
    let path = images::ensure_cached(&article.image_url)?;
    article.image_path = path.clone();
    Ok(path)
}

fn fetch_feed(feed: &FeedCfg, limit: usize) -> FetchOutcome {
    let mut outcome = FetchOutcome {
        feed_id: feed.id.clone(),
        feed_title: String::new(),
        site_url: String::new(),
        etag: String::new(),
        last_modified: String::new(),
        entries: Vec::new(),
        error: String::new(),
    };

    let (bytes, etag, last_modified) =
        match crate::util::http_get(&feed.url, crate::util::MAX_FEED_BYTES, 15, None, None) {
            Ok(v) => v,
            Err(e) => {
                outcome.error = e;
                return outcome;
            }
        };
    outcome.etag = etag;
    outcome.last_modified = last_modified;

    let parsed: Feed = match feed_rs::parser::parse(&bytes[..]) {
        Ok(f) => f,
        Err(e) => {
            outcome.error = format!("feed parse error: {}", e);
            return outcome;
        }
    };

    if let Some(title) = &parsed.title {
        outcome.feed_title = clean_title(&title.content);
    }
    outcome.site_url = parsed
        .links
        .iter()
        .find(|l| {
            l.rel.as_deref().unwrap_or("alternate") == "alternate"
                && l.media_type
                    .as_deref()
                    .map(|m| m.starts_with("text/html"))
                    .unwrap_or(true)
                && is_http(&l.href)
        })
        .map(|l| l.href.clone())
        .unwrap_or_else(|| feed.url.clone());

    let base = outcome.site_url.clone();
    for entry in parsed.entries.into_iter().take(limit) {
        if let Some(parsed) = parse_entry(&entry, &base, &feed.id) {
            outcome.entries.push(parsed);
        }
    }
    outcome
}

fn parse_entry(entry: &Entry, base: &str, feed_id: &str) -> Option<ParsedEntry> {
    let title = entry
        .title
        .as_ref()
        .map(|t| strip_html(&t.content))
        .unwrap_or_default();

    let url = entry_url(entry);
    let entry_base = entry.base.clone().unwrap_or_else(|| {
        if url.is_empty() {
            base.to_string()
        } else {
            url.clone()
        }
    });

    let raw_summary = entry
        .summary
        .as_ref()
        .map(|t| t.content.clone())
        .or_else(|| entry.content.as_ref().and_then(|c| c.body.clone()))
        .unwrap_or_default();
    let raw_content = entry
        .content
        .as_ref()
        .and_then(|c| c.body.clone())
        .unwrap_or_else(|| raw_summary.clone());

    let image_url = entry_image(entry, &entry_base);
    let summary = truncate_chars(&strip_html(&raw_summary), 400);
    let content = truncate_chars(&strip_html(&raw_content), 6000);

    let published = entry
        .published
        .or(entry.updated)
        .map(|d| d.timestamp())
        .unwrap_or_else(now);

    if title.is_empty() && url.is_empty() {
        return None;
    }

    let identity = if entry.id.is_empty() {
        url.clone()
    } else {
        entry.id.clone()
    };

    Some(ParsedEntry {
        id: sha_hex(&format!("{}::{}", feed_id, identity), 24),
        title: if title.is_empty() {
            truncate_chars(&strip_html(&raw_summary), 90)
        } else {
            title
        },
        url,
        summary,
        content,
        author: entry
            .authors
            .first()
            .map(|p| p.name.clone())
            .unwrap_or_default(),
        published,
        image_url,
    })
}

fn entry_url(entry: &Entry) -> String {
    for link in &entry.links {
        let rel = link.rel.as_deref().unwrap_or("alternate");
        if rel == "enclosure" {
            continue;
        }
        let media = link.media_type.as_deref().unwrap_or("text/html");
        if (media.starts_with("text/html") || media.is_empty()) && is_http(&link.href) {
            return link.href.clone();
        }
    }
    for link in &entry.links {
        if is_http(&link.href) && !link.href.contains("/enclosure") {
            return link.href.clone();
        }
    }
    if is_http(&entry.id) {
        return entry.id.clone();
    }
    String::new()
}

fn entry_image(entry: &Entry, base: &str) -> String {
    for media in &entry.media {
        for thumb in &media.thumbnails {
            let url = resolve_url(base, &thumb.image.uri);
            if !url.is_empty() && plausible_image(&url) {
                return url;
            }
        }
        for content in &media.content {
            let Some(raw) = &content.url else { continue };
            let media_type = content
                .content_type
                .as_ref()
                .map(|t| t.to_string())
                .unwrap_or_default();
            if media_type.starts_with("image/") || media_type.is_empty() {
                let url = resolve_url(base, raw.as_str());
                if !url.is_empty() && plausible_image(&url) {
                    return url;
                }
            }
        }
    }

    for link in &entry.links {
        let rel = link.rel.as_deref().unwrap_or("");
        let media_type = link.media_type.as_deref().unwrap_or("");
        if (rel == "enclosure" || rel == "image")
            && (media_type.starts_with("image/") || media_type.is_empty())
        {
            let url = resolve_url(base, &link.href);
            if !url.is_empty() && plausible_image(&url) {
                return url;
            }
        }
    }

    if let Some(content) = &entry.content {
        if let Some(body) = &content.body {
            if let Some(src) = first_img_src(body) {
                let url = resolve_url(base, &src);
                if !url.is_empty() && plausible_image(&url) {
                    return url;
                }
            }
        }
    }
    if let Some(summary) = &entry.summary {
        if let Some(src) = first_img_src(&summary.content) {
            let url = resolve_url(base, &src);
            if !url.is_empty() && plausible_image(&url) {
                return url;
            }
        }
    }
    String::new()
}

fn plausible_image(url: &str) -> bool {
    let lower = url.to_ascii_lowercase();
    if lower.contains("feedburner.com/~ff") {
        return false;
    }
    if lower.contains("doubleclick.net")
        || lower.contains("googlesyndication")
        || lower.contains("google-analytics")
    {
        return false;
    }
    if lower.contains("1x1")
        || lower.contains("pixel.gif")
        || lower.contains("spacer.gif")
        || lower.contains("blank.gif")
    {
        return false;
    }
    if !is_http(url) {
        return false;
    }
    let path = lower.split('?').next().unwrap_or("");
    if path.ends_with(".svg") || path.ends_with(".ico") {
        return false;
    }
    true
}

/// Accept either a direct feed URL or a site homepage. For homepages the first
/// `<link rel="alternate" type="application/rss+xml|atom+xml">` is followed.
pub fn discover(url: &str) -> Result<(String, String), String> {
    let (bytes, _, _) = crate::util::http_get(url, crate::util::MAX_FEED_BYTES, 15, None, None)?;

    if let Ok(feed) = feed_rs::parser::parse(&bytes[..]) {
        let title = feed
            .title
            .as_ref()
            .map(|t| clean_title(&t.content))
            .unwrap_or_default();
        return Ok((url.to_string(), title));
    }

    let html = String::from_utf8_lossy(&bytes).to_string();
    if let Some(link) = find_feed_link(&html, url) {
        if let Ok((linked_bytes, _, _)) =
            crate::util::http_get(&link, crate::util::MAX_FEED_BYTES, 15, None, None)
        {
            if let Ok(feed) = feed_rs::parser::parse(&linked_bytes[..]) {
                let title = feed
                    .title
                    .as_ref()
                    .map(|t| strip_html(&t.content))
                    .unwrap_or_default();
                return Ok((link, title));
            }
        }
    }

    // WordPress sites almost always expose /feed/ even without a link tag.
    if html.contains("wp-content") {
        let alt = format!("{}/feed/", url.trim_end_matches('/'));
        if let Ok((wp_bytes, _, _)) =
            crate::util::http_get(&alt, crate::util::MAX_FEED_BYTES, 15, None, None)
        {
            if let Ok(feed) = feed_rs::parser::parse(&wp_bytes[..]) {
                let title = feed
                    .title
                    .as_ref()
                    .map(|t| strip_html(&t.content))
                    .unwrap_or_default();
                return Ok((alt, title));
            }
        }
    }

    Err("no RSS/Atom feed found at this URL".into())
}

fn find_feed_link(html: &str, base: &str) -> Option<String> {
    let lower = html.to_ascii_lowercase();
    let mut search = 0usize;
    while let Some(found) = lower[search..].find("<link") {
        let start = search + found;
        let end = start + lower[start..].find('>')?;
        let tag = &html[start..end];
        let tag_lower = &lower[start..end];
        let is_feed = tag_lower.contains("alternate")
            && (tag_lower.contains("rss+xml")
                || tag_lower.contains("atom+xml")
                || tag_lower.contains("feed+json"));
        if is_feed {
            if let Some(href) = attr_value(tag, "href") {
                let resolved = resolve_url(base, &href);
                if !resolved.is_empty() {
                    return Some(resolved);
                }
            }
        }
        search = end + 1;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn first_entry(xml: &str) -> ParsedEntry {
        let feed = feed_rs::parser::parse(xml.as_bytes()).expect("valid feed");
        parse_entry(&feed.entries[0], "https://example.com/", "feed123").expect("entry")
    }

    const RSS_MEDIA: &str = r#"<?xml version="1.0"?>
<rss version="2.0" xmlns:media="http://search.yahoo.com/mrss/" xmlns:content="http://purl.org/rss/1.0/modules/content/">
<channel><title>T</title><link>https://example.com</link><description>d</description>
<item>
  <title>Hello &amp; world</title>
  <link>https://example.com/post/1</link>
  <guid>post-1</guid>
  <pubDate>Wed, 21 Oct 2026 10:00:00 GMT</pubDate>
  <description><![CDATA[<p>Summary with <img src="/inline.jpg"></p>]]></description>
  <media:thumbnail url="https://cdn.example.com/thumb.jpg" width="320" height="180"/>
  <media:content url="https://cdn.example.com/video.mp4" type="video/mp4"/>
</item></channel></rss>"#;

    #[test]
    fn prefers_media_thumbnail() {
        let entry = first_entry(RSS_MEDIA);
        assert_eq!(entry.image_url, "https://cdn.example.com/thumb.jpg");
        assert_eq!(entry.title, "Hello & world");
        assert_eq!(entry.url, "https://example.com/post/1");
        assert!(entry.summary.contains("Summary with"));
    }

    const RSS_ENCLOSURE: &str = r#"<?xml version="1.0"?>
<rss version="2.0"><channel><title>T</title><link>https://example.com</link><description>d</description>
<item><title>Pic</title><link>https://example.com/p</link><guid>p</guid>
<enclosure url="https://cdn.example.com/pic.png" type="image/png" length="1234"/>
<description>body</description></item></channel></rss>"#;

    #[test]
    fn falls_back_to_enclosure() {
        let entry = first_entry(RSS_ENCLOSURE);
        assert_eq!(entry.image_url, "https://cdn.example.com/pic.png");
    }

    const ATOM_INLINE: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<feed xmlns="http://www.w3.org/2005/Atom">
<title>Atom</title><link href="https://example.com/"/>
<entry><title>Inline</title><id>urn:1</id><updated>2026-10-20T12:00:00Z</updated>
<link href="https://example.com/a/1"/>
<content type="html">&lt;p&gt;hi&lt;/p&gt;&lt;img src="../images/a.webp"/&gt;</content>
</entry></feed>"#;

    #[test]
    fn resolves_inline_relative_image() {
        let entry = first_entry(ATOM_INLINE);
        assert_eq!(entry.image_url, "https://example.com/images/a.webp");
        assert_eq!(entry.content, "hi");
    }

    #[test]
    fn rejects_tracking_pixels() {
        assert!(!plausible_image("https://x.test/pixel.gif"));
        assert!(!plausible_image("https://feedburner.com/~ff/x"));
        assert!(plausible_image("https://x.test/photo.jpg"));
    }
}
