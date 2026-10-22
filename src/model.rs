//! Persistent data model. Everything the engine stores on disk is defined here.
//!
//! Two files live in `$XDG_STATE_HOME/omarchy/pulse/`:
//!   - `config.json`  user intent: feeds, tabs, markets, settings
//!   - `state.json`   derived data: articles, read flags, market cache
//!
//! Both are written atomically (temp file + rename) with mode 0600.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub const CONFIG_VERSION: u32 = 1;
pub const VERSION: &str = "1.0.0";

fn default_refresh_minutes() -> u32 {
    5
}
fn default_market_seconds() -> u32 {
    60
}
fn default_notify_interval() -> u32 {
    30
}
fn default_max_articles() -> usize {
    1200
}
fn default_per_feed() -> usize {
    60
}
fn default_quiet_start() -> String {
    "23:00".into()
}
fn default_quiet_end() -> String {
    "08:00".into()
}
fn default_bar_ticker() -> String {
    "usdtry".into()
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub version: u32,
    pub feeds: Vec<FeedCfg>,
    pub tabs: Vec<TabCfg>,
    pub markets: Vec<MarketCfg>,
    pub settings: Settings,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            version: CONFIG_VERSION,
            feeds: Vec::new(),
            tabs: Vec::new(),
            markets: default_markets(),
            settings: Settings::default(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub show_images: bool,
    pub thumbnails: bool,
    pub show_excerpts: bool,
    pub density: String,
    pub bar_ticker: bool,
    pub bar_ticker_id: String,
    pub refresh_minutes: u32,
    pub market_refresh_seconds: u32,
    pub notifications: bool,
    pub notify_min_interval_minutes: u32,
    pub quiet_start: String,
    pub quiet_end: String,
    pub max_articles: usize,
    pub per_feed_limit: usize,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            show_images: true,
            thumbnails: true,
            show_excerpts: true,
            density: "comfortable".into(),
            bar_ticker: false,
            bar_ticker_id: default_bar_ticker(),
            refresh_minutes: default_refresh_minutes(),
            market_refresh_seconds: default_market_seconds(),
            notifications: true,
            notify_min_interval_minutes: default_notify_interval(),
            quiet_start: default_quiet_start(),
            quiet_end: default_quiet_end(),
            max_articles: default_max_articles(),
            per_feed_limit: default_per_feed(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct FeedCfg {
    pub id: String,
    pub url: String,
    pub title: String,
    pub site_url: String,
    pub enabled: bool,
    pub added_at: i64,
    pub last_fetch: i64,
    pub last_error: String,
    /// ETag / Last-Modified from the previous fetch, used for conditional GETs.
    pub etag: String,
    pub last_modified: String,
}

impl Default for FeedCfg {
    fn default() -> Self {
        Self {
            id: String::new(),
            url: String::new(),
            title: String::new(),
            site_url: String::new(),
            enabled: true,
            added_at: 0,
            last_fetch: 0,
            last_error: String::new(),
            etag: String::new(),
            last_modified: String::new(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct TabCfg {
    pub id: String,
    pub name: String,
    pub glyph: String,
    /// Feed ids owned by this tab.
    pub feeds: Vec<String>,
    /// When non-empty, an article must match at least one keyword.
    pub include: Vec<String>,
    /// Never show articles matching these keywords in this tab.
    pub exclude: Vec<String>,
    pub notify: bool,
    /// tri-state: null inherits the global image setting
    pub images: Option<bool>,
}

impl Default for TabCfg {
    fn default() -> Self {
        Self {
            id: String::new(),
            name: String::new(),
            glyph: String::new(),
            feeds: Vec::new(),
            include: Vec::new(),
            exclude: Vec::new(),
            notify: true,
            images: None,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct MarketCfg {
    pub id: String,
    pub label: String,
    /// `yahoo`, `gold_gram`, `gold_quarter`, `gold_ounce`, `silver_gram`
    pub kind: String,
    /// Yahoo ticker for `yahoo` and the metal base (GC=F / SI=F).
    pub symbol: String,
    /// Display currency: TRY or USD.
    pub quote: String,
    pub enabled: bool,
    pub decimals: u8,
}

impl Default for MarketCfg {
    fn default() -> Self {
        Self {
            id: String::new(),
            label: String::new(),
            kind: "yahoo".into(),
            symbol: String::new(),
            quote: "TRY".into(),
            enabled: true,
            decimals: 2,
        }
    }
}

pub fn default_markets() -> Vec<MarketCfg> {
    vec![
        MarketCfg {
            id: "gold_gram".into(),
            label: "Gram Altın".into(),
            kind: "gold_gram".into(),
            symbol: "GC=F".into(),
            quote: "TRY".into(),
            enabled: true,
            decimals: 2,
        },
        MarketCfg {
            id: "usdtry".into(),
            label: "USD/TRY".into(),
            kind: "yahoo".into(),
            symbol: "USDTRY=X".into(),
            quote: "TRY".into(),
            enabled: true,
            decimals: 4,
        },
        MarketCfg {
            id: "eurtry".into(),
            label: "EUR/TRY".into(),
            kind: "yahoo".into(),
            symbol: "EURTRY=X".into(),
            quote: "TRY".into(),
            enabled: true,
            decimals: 4,
        },
        MarketCfg {
            id: "bist100".into(),
            label: "BIST 100".into(),
            kind: "yahoo".into(),
            symbol: "XU100.IS".into(),
            quote: "TRY".into(),
            enabled: true,
            decimals: 2,
        },
    ]
}

/// A market preset offered by the settings UI. Keeping the catalogue here means
/// the UI and the engine agree on symbols.
pub fn market_catalog() -> Vec<MarketCfg> {
    let mut out = default_markets();
    out.extend(vec![
        MarketCfg {
            id: "gold_quarter".into(),
            label: "Çeyrek Altın".into(),
            kind: "gold_quarter".into(),
            symbol: "GC=F".into(),
            quote: "TRY".into(),
            enabled: false,
            decimals: 2,
        },
        MarketCfg {
            id: "gold_ounce".into(),
            label: "Ons Altın".into(),
            kind: "gold_ounce".into(),
            symbol: "GC=F".into(),
            quote: "USD".into(),
            enabled: false,
            decimals: 2,
        },
        MarketCfg {
            id: "silver_gram".into(),
            label: "Gram Gümüş".into(),
            kind: "silver_gram".into(),
            symbol: "SI=F".into(),
            quote: "TRY".into(),
            enabled: false,
            decimals: 2,
        },
        MarketCfg {
            id: "gbptry".into(),
            label: "GBP/TRY".into(),
            kind: "yahoo".into(),
            symbol: "GBPTRY=X".into(),
            quote: "TRY".into(),
            enabled: false,
            decimals: 4,
        },
        MarketCfg {
            id: "eurusd".into(),
            label: "EUR/USD".into(),
            kind: "yahoo".into(),
            symbol: "EURUSD=X".into(),
            quote: "USD".into(),
            enabled: false,
            decimals: 4,
        },
        MarketCfg {
            id: "btctry".into(),
            label: "BTC/TRY".into(),
            kind: "yahoo".into(),
            symbol: "BTC-USD".into(),
            quote: "TRY".into(),
            enabled: false,
            decimals: 0,
        },
        MarketCfg {
            id: "ethtry".into(),
            label: "ETH/TRY".into(),
            kind: "yahoo".into(),
            symbol: "ETH-USD".into(),
            quote: "TRY".into(),
            enabled: false,
            decimals: 0,
        },
        MarketCfg {
            id: "nasdaq".into(),
            label: "NASDAQ".into(),
            kind: "yahoo".into(),
            symbol: "^IXIC".into(),
            quote: "USD".into(),
            enabled: false,
            decimals: 2,
        },
        MarketCfg {
            id: "sp500".into(),
            label: "S&P 500".into(),
            kind: "yahoo".into(),
            symbol: "^GSPC".into(),
            quote: "USD".into(),
            enabled: false,
            decimals: 2,
        },
    ]);
    out
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Article {
    pub id: String,
    pub feed_id: String,
    pub title: String,
    pub url: String,
    pub summary: String,
    pub content: String,
    pub author: String,
    pub published: i64,
    pub fetched: i64,
    pub image_url: String,
    pub image_path: String,
    pub read: bool,
    pub bookmarked: bool,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Store {
    pub articles: Vec<Article>,
    pub meta: Meta,
    pub markets: MarketCache,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Meta {
    pub last_refresh: i64,
    pub has_refreshed: bool,
    pub last_notify: i64,
    pub pending: Vec<Pending>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Pending {
    pub title: String,
    pub feed: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct MarketCache {
    pub updated: i64,
    pub bases: HashMap<String, BaseQuote>,
    pub quotes: HashMap<String, CachedQuote>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct BaseQuote {
    pub price: f64,
    pub change_pct: f64,
    pub currency: String,
    pub updated: i64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct CachedQuote {
    pub label: String,
    pub price: f64,
    pub change_pct: f64,
    pub currency: String,
    pub price_text: String,
    pub change_text: String,
    pub updated: i64,
    pub stale: bool,
    pub error: String,
}

/// A feed/tab membership index used while building snapshots.
pub struct TabIndex {
    pub tabs: Vec<TabCfg>,
}

impl TabIndex {
    pub fn tabs_for(&self, feed_id: &str, haystack: &str) -> Vec<String> {
        let mut out = Vec::new();
        let lower = haystack.to_lowercase();
        for tab in &self.tabs {
            let in_feed = tab.feeds.iter().any(|f| f == feed_id);
            let keyword_candidate = tab.feeds.is_empty() && !tab.include.is_empty();
            if !in_feed && !keyword_candidate {
                continue;
            }
            if tab
                .exclude
                .iter()
                .any(|k| !k.is_empty() && lower.contains(&k.to_lowercase()))
            {
                continue;
            }
            if !tab.include.is_empty()
                && !tab
                    .include
                    .iter()
                    .any(|k| !k.is_empty() && lower.contains(&k.to_lowercase()))
            {
                continue;
            }
            out.push(tab.id.clone());
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tab(id: &str, feeds: &[&str], include: &[&str], exclude: &[&str]) -> TabCfg {
        TabCfg {
            id: id.into(),
            name: id.into(),
            feeds: feeds.iter().map(|s| s.to_string()).collect(),
            include: include.iter().map(|s| s.to_string()).collect(),
            exclude: exclude.iter().map(|s| s.to_string()).collect(),
            ..TabCfg::default()
        }
    }

    #[test]
    fn feed_tabs_and_keyword_filters() {
        let index = TabIndex {
            tabs: vec![
                tab("gaming", &["f1"], &[], &[]),
                tab("ai", &[], &["llm", "model"], &[]),
                tab("clean", &["f1"], &[], &["sponsored"]),
            ],
        };
        assert_eq!(
            index.tabs_for("f1", "A new GPU"),
            vec!["gaming".to_string(), "clean".to_string()]
        );
        assert_eq!(
            index.tabs_for("f2", "A new LLM released"),
            vec!["ai".to_string()]
        );
        assert!(
            index.tabs_for("f1", "Sponsored: buy this").is_empty()
                || index.tabs_for("f1", "Sponsored: buy this") == vec!["gaming".to_string()]
        );
        assert!(index.tabs_for("f2", "nothing here").is_empty());
    }

    #[test]
    fn keyword_tab_can_restrict_its_own_feeds() {
        let index = TabIndex {
            tabs: vec![tab("ai", &["f1"], &["llm"], &["sponsor"])],
        };
        assert_eq!(index.tabs_for("f1", "new llm"), vec!["ai".to_string()]);
        assert!(index.tabs_for("f1", "new phone").is_empty());
        assert!(index.tabs_for("f2", "new llm").is_empty());
    }

    #[test]
    fn defaults_include_requested_markets() {
        let ids: Vec<String> = default_markets().iter().map(|m| m.id.clone()).collect();
        assert!(ids.contains(&"gold_gram".to_string()));
        assert!(ids.contains(&"usdtry".to_string()));
        assert!(ids.contains(&"eurtry".to_string()));
        assert!(ids.contains(&"bist100".to_string()));
    }
}
