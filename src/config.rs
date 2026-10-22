//! Config/state persistence, normalisation and one-time starter content.

use crate::model::*;
use crate::util::{atomic_write, ensure_dir, now, sha_hex, slugify, state_dir};
use std::collections::HashSet;
use std::fs;

pub struct Paths {
    pub dir: std::path::PathBuf,
    pub config: std::path::PathBuf,
    pub state: std::path::PathBuf,
}

pub fn paths() -> Paths {
    let dir = state_dir();
    Paths {
        config: dir.join("config.json"),
        state: dir.join("state.json"),
        dir,
    }
}

pub fn load_config() -> Config {
    let p = paths();
    let _ = ensure_dir(&p.dir);
    let mut config = match fs::read_to_string(&p.config) {
        Ok(raw) => serde_json::from_str::<Config>(&raw).unwrap_or_default(),
        Err(_) => Config::default(),
    };
    normalize_config(&mut config);
    if !p.config.exists() {
        let _ = save_config(&config);
    }
    config
}

pub fn save_config(config: &Config) -> Result<(), String> {
    let p = paths();
    let body = serde_json::to_vec_pretty(config).map_err(|e| e.to_string())?;
    atomic_write(&p.config, &body)
}

pub fn load_store() -> Store {
    let p = paths();
    match fs::read_to_string(&p.state) {
        Ok(raw) => serde_json::from_str::<Store>(&raw).unwrap_or_default(),
        Err(_) => Store::default(),
    }
}

pub fn save_store(store: &Store) -> Result<(), String> {
    let p = paths();
    let body = serde_json::to_vec(store).map_err(|e| e.to_string())?;
    atomic_write(&p.state, &body)
}

pub fn normalize_config(config: &mut Config) {
    let mut feed_urls = HashSet::new();
    let mut feed_ids = HashSet::new();
    for feed in &mut config.feeds {
        feed.url = feed.url.trim().to_string();
        if feed.url.is_empty() {
            continue;
        }
        if !feed_urls.insert(feed.url.clone()) {
            continue;
        }
        if feed.id.is_empty() || feed_ids.contains(&feed.id) {
            let host = url::Url::parse(&feed.url)
                .ok()
                .and_then(|u| {
                    u.host_str()
                        .map(|h| h.trim_start_matches("www.").to_string())
                })
                .unwrap_or_else(|| "feed".into());
            feed.id = format!("{}-{}", slugify(&host), sha_hex(&feed.url, 4));
        }
        feed_ids.insert(feed.id.clone());
        if feed.title.trim().is_empty() {
            feed.title = url::Url::parse(&feed.url)
                .ok()
                .and_then(|u| {
                    u.host_str()
                        .map(|h| h.trim_start_matches("www.").to_string())
                })
                .unwrap_or_else(|| feed.url.clone());
        }
        if feed.added_at == 0 {
            feed.added_at = now();
        }
    }

    let mut tab_ids = HashSet::new();
    let mut cleaned_tabs: Vec<TabCfg> = Vec::new();
    for mut tab in config.tabs.drain(..) {
        tab.name = tab.name.trim().to_string();
        if tab.name.is_empty() {
            continue;
        }
        if tab.id.is_empty() || tab_ids.contains(&tab.id) {
            let mut base = slugify(&tab.name);
            if base.is_empty() {
                base = "tab".into();
            }
            let mut id = base.clone();
            let mut n = 2;
            while tab_ids.contains(&id) {
                id = format!("{}-{}", base, n);
                n += 1;
            }
            tab.id = id;
        }
        tab.feeds.retain(|f| feed_ids.contains(f));
        tab.include.retain(|k| !k.trim().is_empty());
        tab.exclude.retain(|k| !k.trim().is_empty());
        tab_ids.insert(tab.id.clone());
        cleaned_tabs.push(tab);
    }
    config.tabs = cleaned_tabs;

    let mut market_ids = HashSet::new();
    let mut cleaned_markets: Vec<MarketCfg> = Vec::new();
    for mut market in config.markets.drain(..) {
        market.id = market.id.trim().to_string();
        if market.id.is_empty() || market_ids.contains(&market.id) {
            market.id = format!("{}-{}", slugify(&market.label), sha_hex(&market.symbol, 4));
        }
        if market.symbol.trim().is_empty() && market.kind == "yahoo" {
            continue;
        }
        if market.label.trim().is_empty() {
            market.label = market.symbol.clone();
        }
        if market.decimals > 6 {
            market.decimals = 2;
        }
        market_ids.insert(market.id.clone());
        cleaned_markets.push(market);
    }
    if cleaned_markets.is_empty() {
        cleaned_markets = default_markets();
    }
    config.markets = cleaned_markets;

    if config.settings.refresh_minutes < 1 {
        config.settings.refresh_minutes = 1;
    }
    if config.settings.refresh_minutes > 720 {
        config.settings.refresh_minutes = 720;
    }
    if config.settings.per_feed_limit < 5 {
        config.settings.per_feed_limit = 5;
    }
    if config.settings.per_feed_limit > 300 {
        config.settings.per_feed_limit = 300;
    }
    if config.settings.max_articles < 100 {
        config.settings.max_articles = 100;
    }
    if !["compact", "comfortable"].contains(&config.settings.density.as_str()) {
        config.settings.density = "comfortable".into();
    }
}

pub fn new_feed(url: &str, title: &str) -> FeedCfg {
    let host = url::Url::parse(url)
        .ok()
        .and_then(|u| {
            u.host_str()
                .map(|h| h.trim_start_matches("www.").to_string())
        })
        .unwrap_or_else(|| "feed".into());
    let title = if title.trim().is_empty() {
        host.clone()
    } else {
        title.trim().to_string()
    };
    FeedCfg {
        id: format!("{}-{}", slugify(&host), sha_hex(url, 4)),
        url: url.trim().to_string(),
        title,
        site_url: String::new(),
        enabled: true,
        added_at: now(),
        last_fetch: 0,
        last_error: String::new(),
        etag: String::new(),
        last_modified: String::new(),
    }
}

/// Curated onboarding set. Only applied when the user explicitly asks for it.
pub fn starter(config: &mut Config) -> bool {
    if !config.feeds.is_empty() {
        return false;
    }
    let seeds: Vec<(&str, &str, &str)> = vec![
        ("Gaming", "Polygon", "https://www.polygon.com/feed/"),
        (
            "Gaming",
            "Rock Paper Shotgun",
            "https://feeds.feedburner.com/rockpapershotgun",
        ),
        (
            "Tech",
            "Ars Technica",
            "https://feeds.arstechnica.com/arstechnica/index",
        ),
        (
            "Tech",
            "The Verge",
            "https://www.theverge.com/rss/index.xml",
        ),
        ("Tech", "Phoronix", "https://www.phoronix.com/rss.php"),
        ("AI", "Hugging Face", "https://huggingface.co/blog/feed.xml"),
        (
            "AI",
            "Google DeepMind",
            "https://deepmind.google/blog/rss.xml",
        ),
        (
            "World",
            "BBC World",
            "https://feeds.bbci.co.uk/news/world/rss.xml",
        ),
        (
            "World",
            "Al Jazeera",
            "https://www.aljazeera.com/xml/rss/all.xml",
        ),
    ];

    let mut tab_map: Vec<(String, String)> = Vec::new();
    for (tab_name, _, _) in &seeds {
        if !tab_map.iter().any(|(name, _)| name == tab_name) {
            let id = slugify(tab_name);
            config.tabs.push(TabCfg {
                id: id.clone(),
                name: tab_name.to_string(),
                glyph: String::new(),
                feeds: Vec::new(),
                include: Vec::new(),
                exclude: Vec::new(),
                notify: true,
                images: None,
            });
            tab_map.push((tab_name.to_string(), id));
        }
    }

    for (tab_name, title, url) in seeds {
        if config.feeds.iter().any(|f| f.url == url) {
            continue;
        }
        let feed = new_feed(url, title);
        let tab_id = tab_map
            .iter()
            .find(|(name, _)| name == tab_name)
            .map(|(_, id)| id.clone())
            .unwrap_or_default();
        if let Some(tab) = config.tabs.iter_mut().find(|t| t.id == tab_id) {
            tab.feeds.push(feed.id.clone());
        }
        config.feeds.push(feed);
    }
    normalize_config(config);
    true
}
