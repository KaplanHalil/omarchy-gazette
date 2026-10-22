//! Builds the JSON document consumed by the QML panel. One call returns
//! everything the UI needs: config, computed tab/feed counts, articles, market
//! quotes, notification queue and paths.

use crate::config::paths;
use crate::model::{market_catalog, Article, Config, Store, TabIndex};
use crate::util::{cache_dir, format_date, now, relative_age};
use serde_json::{json, Value};
use std::collections::HashMap;

pub fn build(config: &Config, store: &Store) -> Value {
    let now_ts = now();
    let tab_index = TabIndex {
        tabs: config.tabs.clone(),
    };

    let feed_titles: HashMap<&str, &str> = config
        .feeds
        .iter()
        .map(|f| (f.id.as_str(), f.title.as_str()))
        .collect();

    // Per-article tab membership is needed by counts, notifications and the UI.
    let mut membership: HashMap<&str, Vec<String>> = HashMap::new();
    for article in &store.articles {
        let haystack = format!("{} {}", article.title, article.summary);
        let tabs = tab_index.tabs_for(&article.feed_id, &haystack);
        membership.insert(article.id.as_str(), tabs);
    }

    let mut tab_counts: HashMap<String, (usize, usize)> = HashMap::new();
    let mut feed_counts: HashMap<String, (usize, usize)> = HashMap::new();
    let mut unread_total = 0usize;
    for article in &store.articles {
        let tabs = membership
            .get(article.id.as_str())
            .cloned()
            .unwrap_or_default();
        for tab in &tabs {
            let entry = tab_counts.entry(tab.clone()).or_insert((0, 0));
            entry.1 += 1;
            if !article.read {
                entry.0 += 1;
            }
        }
        let entry = feed_counts.entry(article.feed_id.clone()).or_insert((0, 0));
        entry.1 += 1;
        if !article.read {
            entry.0 += 1;
            unread_total += 1;
        }
    }

    let tabs: Vec<Value> = config
        .tabs
        .iter()
        .map(|tab| {
            let (unread, total) = tab_counts.get(&tab.id).copied().unwrap_or((0, 0));
            json!({
                "id": tab.id,
                "name": tab.name,
                "glyph": tab.glyph,
                "feeds": tab.feeds,
                "include": tab.include,
                "exclude": tab.exclude,
                "notify": tab.notify,
                "images": tab.images,
                "unread": unread,
                "total": total,
            })
        })
        .collect();

    let feeds: Vec<Value> = config
        .feeds
        .iter()
        .map(|feed| {
            let tab_ids: Vec<String> = config
                .tabs
                .iter()
                .filter(|t| t.feeds.iter().any(|f| f == &feed.id))
                .map(|t| t.id.clone())
                .collect();
            let (unread, total) = feed_counts.get(&feed.id).copied().unwrap_or((0, 0));
            json!({
                "id": feed.id,
                "title": feed.title,
                "url": feed.url,
                "siteUrl": feed.site_url,
                "enabled": feed.enabled,
                "addedAt": feed.added_at,
                "lastFetch": feed.last_fetch,
                "lastError": feed.last_error,
                "tabIds": tab_ids,
                "unread": unread,
                "total": total,
            })
        })
        .collect();

    let mut articles: Vec<&Article> = store.articles.iter().collect();
    articles.sort_by(|a, b| b.published.cmp(&a.published));

    let article_values: Vec<Value> = articles
        .iter()
        .map(|article| {
            let tabs = membership
                .get(article.id.as_str())
                .cloned()
                .unwrap_or_default();
            let image_file = if article.image_path.is_empty() {
                String::new()
            } else {
                url::Url::from_file_path(&article.image_path)
                    .map(|u| u.to_string())
                    .unwrap_or_default()
            };
            json!({
                "id": article.id,
                "feedId": article.feed_id,
                "feedTitle": feed_titles.get(article.feed_id.as_str()).copied().unwrap_or("Feed"),
                "title": article.title,
                "url": article.url,
                "summary": article.summary,
                "content": article.content,
                "author": article.author,
                "published": article.published,
                "age": relative_age(article.published, now_ts),
                "date": format_date(article.published),
                "imageUrl": article.image_url,
                "imagePath": article.image_path,
                "imageFile": image_file,
                "read": article.read,
                "bookmarked": article.bookmarked,
                "tabs": tabs,
            })
        })
        .collect();

    let markets: Vec<Value> = config
        .markets
        .iter()
        .map(|market| {
            let cached = store.markets.quotes.get(&market.id);
            json!({
                "id": market.id,
                "label": market.label,
                "kind": market.kind,
                "symbol": market.symbol,
                "quote": market.quote,
                "enabled": market.enabled,
                "decimals": market.decimals,
                "price": cached.map(|c| c.price).unwrap_or(0.0),
                "changePct": cached.map(|c| c.change_pct).unwrap_or(0.0),
                "currency": cached.map(|c| c.currency.clone()).unwrap_or_else(|| market.quote.clone()),
                "priceText": cached.map(|c| c.price_text.clone()).unwrap_or_else(|| "—".into()),
                "changeText": cached.map(|c| c.change_text.clone()).unwrap_or_else(|| "—".into()),
                "stale": cached.map(|c| c.stale).unwrap_or(true),
                "error": cached.map(|c| c.error.clone()).unwrap_or_default(),
            })
        })
        .collect();

    let ticker = store
        .markets
        .quotes
        .get(&config.settings.bar_ticker_id)
        .filter(|_| {
            config
                .markets
                .iter()
                .any(|m| m.id == config.settings.bar_ticker_id && m.enabled)
        })
        .or_else(|| {
            config
                .markets
                .iter()
                .filter(|m| m.enabled)
                .find_map(|m| store.markets.quotes.get(&m.id))
        });

    let errors = config
        .feeds
        .iter()
        .filter(|f| !f.last_error.is_empty())
        .count();

    let tabs_value = json!(tabs);
    let paths = paths();
    json!({
        "ok": true,
        "version": crate::model::VERSION,
        "generatedAt": now_ts,
        "settings": config.settings,
        "tabs": tabs_value,
        "feeds": feeds,
        "articles": article_values,
        "markets": markets,
        "marketCatalog": market_catalog(),
        "marketUpdated": store.markets.updated,
        "pending": store.meta.pending,
        "stats": {
            "unread": unread_total,
            "total": store.articles.len(),
            "feeds": config.feeds.len(),
            "enabledFeeds": config.feeds.iter().filter(|f| f.enabled).count(),
            "tabs": config.tabs.len(),
            "errors": errors,
            "bookmarked": store.articles.iter().filter(|a| a.bookmarked).count(),
            "lastRefresh": store.meta.last_refresh,
            "lastRefreshAge": if store.meta.last_refresh > 0 { relative_age(store.meta.last_refresh, now_ts) } else { "never".into() },
            "lastNotify": store.meta.last_notify,
        },
        "barTicker": ticker.map(|t| json!({
            "label": t.label,
            "priceText": t.price_text,
            "changeText": t.change_text,
            "changePct": t.change_pct,
            "stale": t.stale,
        })),
        "paths": {
            "config": paths.config.to_string_lossy(),
            "state": paths.state.to_string_lossy(),
            "images": cache_dir().to_string_lossy(),
        }
    })
}

pub fn tab_article_ids(config: &Config, store: &Store, tab_id: &str) -> Vec<String> {
    let tab_index = TabIndex {
        tabs: config.tabs.clone(),
    };
    let Some(tab) = config.tabs.iter().find(|t| t.id == tab_id) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for article in &store.articles {
        let haystack = format!("{} {}", article.title, article.summary);
        let tabs = tab_index.tabs_for(&article.feed_id, &haystack);
        if tabs.iter().any(|t| t == &tab.id) {
            out.push(article.id.clone());
        }
    }
    out
}
