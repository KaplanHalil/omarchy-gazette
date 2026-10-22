//! `gazette-engine` — local-first RSS/Atom + markets backend for Gazette.
//!
//! Every command prints JSON on stdout (except `--status`/`--help`), so the
//! Quickshell UI can call one binary and parse the result.

mod config;
mod feeds;
mod images;
mod markets;
mod model;
mod opml;
mod snapshot;
mod util;

use crate::model::{Config, MarketCfg, Pending, Settings, Store, TabCfg};
use crate::util::{atomic_write, home, now, split_list, LockGuard};
use std::io::Write;

#[derive(Default)]
struct Opts {
    command: Cmd,
    tab: Option<String>,
    title: String,
    tabs: Vec<String>,
    json: bool,
}

#[derive(Default, PartialEq)]
enum Cmd {
    #[default]
    Help,
    Snapshot,
    Refresh,
    Markets,
    MarkRead(String),
    MarkUnread(String),
    MarkAllRead,
    Bookmark(String, bool),
    AddFeed(String),
    RemoveFeed(String),
    ToggleFeed(String),
    SaveTab(String),
    DeleteTab(String),
    MoveTab(String, bool),
    SaveSettings(String),
    SaveMarkets(String),
    ImportOpml(String),
    ExportOpml(String),
    SeedStarter,
    AckNotifications,
    ArticleImage(String),
    Status,
    Clean,
    Version,
}

fn main() {
    let code = run();
    std::process::exit(code);
}

fn run() -> i32 {
    let opts = match parse_args() {
        Ok(o) => o,
        Err(error) => return fail(&error),
    };

    let result: Result<serde_json::Value, String> = match opts.command {
        Cmd::Help => {
            print_help();
            return 0;
        }
        Cmd::Version => {
            println!("Gazette {} ({})", model::VERSION, "engine");
            return 0;
        }
        Cmd::Status => return status(&opts),
        Cmd::Snapshot => {
            let config = config::load_config();
            let store = config::load_store();
            Ok(snapshot::build(&config, &store))
        }
        Cmd::Refresh => with_state(|config, store, _lock| {
            let report = feeds::refresh(config, store);
            config::save_config(config)?;
            serde_json::to_value(report).map_err(|e| e.to_string())
        }),
        Cmd::Markets => with_state(|config, store, _lock| {
            markets::refresh(config, &mut store.markets)?;
            config::save_store(store)?;
            let snap = snapshot::build(config, store);
            Ok(serde_json::json!({
                "ok": true,
                "updated": store.markets.updated,
                "markets": snap["markets"],
                "barTicker": snap["barTicker"],
            }))
        }),
        Cmd::MarkRead(id) => with_state(|_, store, _lock| {
            let article = find_article(store, &id)?;
            article.read = true;
            config::save_store(store)?;
            Ok(ok())
        }),
        Cmd::MarkUnread(id) => with_state(|_, store, _lock| {
            let article = find_article(store, &id)?;
            article.read = false;
            config::save_store(store)?;
            Ok(ok())
        }),
        Cmd::MarkAllRead => with_state(|config, store, _lock| {
            let count = if let Some(tab) = &opts.tab {
                let ids = snapshot::tab_article_ids(config, store, tab);
                let mut count = 0;
                for article in store.articles.iter_mut() {
                    if ids.contains(&article.id) && !article.read {
                        article.read = true;
                        count += 1;
                    }
                }
                count
            } else {
                let mut count = 0;
                for article in store.articles.iter_mut() {
                    if !article.read {
                        article.read = true;
                        count += 1;
                    }
                }
                count
            };
            config::save_store(store)?;
            Ok(serde_json::json!({"ok": true, "marked": count}))
        }),
        Cmd::Bookmark(id, value) => with_state(|_, store, _lock| {
            let article = find_article(store, &id)?;
            article.bookmarked = value;
            config::save_store(store)?;
            Ok(ok())
        }),
        Cmd::AddFeed(url) => {
            with_state(|config, _store, _lock| add_feed(config, &url, &opts.title, &opts.tabs))
        }
        Cmd::RemoveFeed(id) => with_state(|config, store, _lock| {
            let before = config.feeds.len();
            config.feeds.retain(|f| f.id != id);
            if config.feeds.len() == before {
                return Err("feed not found".into());
            }
            for tab in config.tabs.iter_mut() {
                tab.feeds.retain(|f| f != &id);
            }
            store.articles.retain(|a| a.feed_id != id);
            config::normalize_config(config);
            config::save_config(config)?;
            config::save_store(store)?;
            Ok(serde_json::json!({"ok": true, "removed": before - config.feeds.len()}))
        }),
        Cmd::ToggleFeed(id) => with_state(|config, _store, _lock| {
            let enabled = {
                let feed = config
                    .feeds
                    .iter_mut()
                    .find(|f| f.id == id)
                    .ok_or_else(|| "feed not found".to_string())?;
                feed.enabled = !feed.enabled;
                feed.enabled
            };
            config::save_config(config)?;
            Ok(serde_json::json!({"ok": true, "enabled": enabled}))
        }),
        Cmd::SaveTab(raw) => with_state(|config, _store, _lock| {
            let mut tab: TabCfg =
                serde_json::from_str(&raw).map_err(|e| format!("invalid tab: {}", e))?;
            tab.name = tab.name.trim().to_string();
            if tab.name.is_empty() {
                return Err("tab name is required".into());
            }
            if tab.name.chars().count() > 40 {
                tab.name = tab.name.chars().take(40).collect();
            }
            tab.include = split_list(&tab.include.join(","));
            tab.exclude = split_list(&tab.exclude.join(","));
            if tab.id.is_empty() {
                tab.id = String::new();
            }
            if let Some(existing) = config
                .tabs
                .iter_mut()
                .find(|t| !tab.id.is_empty() && t.id == tab.id)
            {
                *existing = tab.clone();
            } else {
                config.tabs.push(tab.clone());
            }
            config::normalize_config(config);
            config::save_config(config)?;
            let saved = config.tabs.iter().find(|t| t.name == tab.name).cloned();
            Ok(serde_json::json!({"ok": true, "tab": saved}))
        }),
        Cmd::DeleteTab(id) => with_state(|config, _store, _lock| {
            let before = config.tabs.len();
            config.tabs.retain(|t| t.id != id);
            if before == config.tabs.len() {
                return Err("tab not found".into());
            }
            config::save_config(config)?;
            Ok(ok())
        }),
        Cmd::MoveTab(id, up) => with_state(|config, _store, _lock| {
            let pos = config
                .tabs
                .iter()
                .position(|t| t.id == id)
                .ok_or_else(|| "tab not found".to_string())?;
            if config.tabs.len() < 2 {
                return Ok(ok());
            }
            let target = if up {
                pos.saturating_sub(1)
            } else {
                (pos + 1).min(config.tabs.len() - 1)
            };
            if target != pos {
                config.tabs.swap(pos, target);
                config::save_config(config)?;
            }
            Ok(ok())
        }),
        Cmd::SaveSettings(raw) => with_state(|config, _store, _lock| {
            let patch: serde_json::Value =
                serde_json::from_str(&raw).map_err(|e| format!("invalid settings: {}", e))?;
            let mut current = serde_json::to_value(&config.settings).map_err(|e| e.to_string())?;
            if let (Some(target), Some(source)) = (current.as_object_mut(), patch.as_object()) {
                for (key, value) in source {
                    target.insert(key.clone(), value.clone());
                }
            }
            let settings: Settings =
                serde_json::from_value(current).map_err(|e| format!("invalid settings: {}", e))?;
            config.settings = settings;
            config::normalize_config(config);
            config::save_config(config)?;
            Ok(serde_json::json!({"ok": true, "settings": config.settings}))
        }),
        Cmd::SaveMarkets(raw) => with_state(|config, _store, _lock| {
            let mut list: Vec<MarketCfg> =
                serde_json::from_str(&raw).map_err(|e| format!("invalid markets: {}", e))?;
            if list.len() > 40 {
                return Err("too many tickers".into());
            }
            for market in &mut list {
                market.label = market.label.trim().to_string();
                market.symbol = market.symbol.trim().to_string();
                if market.kind.is_empty() {
                    market.kind = "yahoo".into();
                }
                if market.quote.is_empty() {
                    market.quote = "TRY".into();
                }
            }
            config.markets = list;
            config::normalize_config(config);
            config::save_config(config)?;
            Ok(serde_json::json!({"ok": true, "count": config.markets.len()}))
        }),
        Cmd::ImportOpml(path) => with_state(|config, _store, _lock| {
            let xml = std::fs::read_to_string(&path)
                .map_err(|e| format!("could not read {}: {}", path, e))?;
            let result = opml::import(config, &xml)?;
            config::normalize_config(config);
            config::save_config(config)?;
            Ok(
                serde_json::json!({"ok": true, "added": result.added, "skipped": result.skipped, "tabsCreated": result.tabs_created}),
            )
        }),
        Cmd::ExportOpml(path) => (|| {
            let config = config::load_config();
            let target = if path.trim().is_empty() {
                home().join("gazette-subscriptions.opml")
            } else {
                std::path::PathBuf::from(path)
            };
            let body = opml::export(&config);
            atomic_write(&target, body.as_bytes())?;
            Ok(serde_json::json!({"ok": true, "path": target.to_string_lossy()}))
        })(),
        Cmd::SeedStarter => with_state(|config, _store, _lock| {
            if !config::starter(config) {
                return Ok(
                    serde_json::json!({"ok": true, "seeded": false, "reason": "feeds already exist"}),
                );
            }
            config::save_config(config)?;
            Ok(
                serde_json::json!({"ok": true, "seeded": true, "feeds": config.feeds.len(), "tabs": config.tabs.len()}),
            )
        }),
        Cmd::AckNotifications => with_state(|_config, store, _lock| {
            store.meta.pending = Vec::<Pending>::new();
            config::save_store(store)?;
            Ok(ok())
        }),
        Cmd::ArticleImage(id) => with_state(|_config, store, _lock| {
            let path = feeds::fetch_article_image(store, &id)?;
            config::save_store(store)?;
            Ok(serde_json::json!({"ok": true, "path": path}))
        }),
        Cmd::Clean => with_state(|config, store, _lock| {
            let referenced: std::collections::HashSet<String> = store
                .articles
                .iter()
                .filter(|a| !a.image_path.is_empty())
                .map(|a| a.image_path.clone())
                .collect();
            images::cleanup(referenced);
            store
                .articles
                .retain(|a| a.bookmarked || now() - a.published < 60 * 86_400 || !a.read);
            config::save_store(store)?;
            Ok(
                serde_json::json!({"ok": true, "articles": store.articles.len(), "feeds": config.feeds.len()}),
            )
        }),
    };

    match result {
        Ok(value) => match serde_json::to_string(&value) {
            Ok(text) => {
                println!("{}", text);
                0
            }
            Err(e) => fail(&e.to_string()),
        },
        Err(error) => fail(&error),
    }
}

fn ok() -> serde_json::Value {
    serde_json::json!({"ok": true})
}

fn fail(error: &str) -> i32 {
    let _ = writeln!(
        std::io::stdout(),
        "{}",
        serde_json::json!({"ok": false, "error": error})
    );
    1
}

fn find_article<'a>(store: &'a mut Store, id: &str) -> Result<&'a mut model::Article, String> {
    store
        .articles
        .iter_mut()
        .find(|a| a.id == id)
        .ok_or_else(|| "article not found".to_string())
}

fn with_state<F>(f: F) -> Result<serde_json::Value, String>
where
    F: FnOnce(&mut Config, &mut Store, &LockGuard) -> Result<serde_json::Value, String>,
{
    let lock = LockGuard::acquire()?;
    let mut config = config::load_config();
    let mut store = config::load_store();
    let result = f(&mut config, &mut store, &lock)?;
    Ok(result)
}

fn add_feed(
    config: &mut Config,
    raw_url: &str,
    title: &str,
    tabs: &[String],
) -> Result<serde_json::Value, String> {
    let mut url = raw_url.trim().to_string();
    if url.is_empty() {
        return Err("feed URL is required".into());
    }
    if !url.contains("://") {
        url = format!("https://{}", url);
    }
    if !util::is_http(&url) {
        return Err("only http(s) feed URLs are supported".into());
    }

    let (feed_url, detected_title) = feeds::discover(&url)?;

    if let Some(pos) = config.feeds.iter().position(|f| f.url == feed_url) {
        let id = config.feeds[pos].id.clone();
        config.feeds[pos].enabled = true;
        let existing_title = config.feeds[pos].title.clone();
        for tab_id in tabs {
            if let Some(tab) = config.tabs.iter_mut().find(|t| &t.id == tab_id) {
                if !tab.feeds.iter().any(|f| f == &id) {
                    tab.feeds.push(id.clone());
                }
            }
        }
        config::normalize_config(config);
        config::save_config(config)?;
        return Ok(serde_json::json!({
            "ok": true,
            "existing": true,
            "feed": {"id": id, "url": feed_url, "title": existing_title},
        }));
    }

    let feed = config::new_feed(
        &feed_url,
        if title.trim().is_empty() {
            &detected_title
        } else {
            title
        },
    );
    for tab_id in tabs {
        if let Some(tab) = config.tabs.iter_mut().find(|t| &t.id == tab_id) {
            if !tab.feeds.iter().any(|f| f == &feed.id) {
                tab.feeds.push(feed.id.clone());
            }
        }
    }
    let summary = serde_json::json!({
        "id": feed.id,
        "url": feed.url,
        "title": feed.title,
    });
    config.feeds.push(feed);
    config::normalize_config(config);
    config::save_config(config)?;
    Ok(serde_json::json!({"ok": true, "existing": false, "feed": summary}))
}

fn status(opts: &Opts) -> i32 {
    let config = config::load_config();
    let store = config::load_store();
    let unread = store.articles.iter().filter(|a| !a.read).count();
    let errors = config
        .feeds
        .iter()
        .filter(|f| !f.last_error.is_empty())
        .count();
    if opts.json {
        println!(
            "{}",
            serde_json::json!({
                "ok": true,
                "version": model::VERSION,
                "unread": unread,
                "articles": store.articles.len(),
                "feeds": config.feeds.len(),
                "tabs": config.tabs.len(),
                "errors": errors,
                "lastRefresh": store.meta.last_refresh,
                "marketQuotes": store.markets.quotes.len(),
            })
        );
    } else {
        println!(
            "Gazette {} — unread {}, articles {}, feeds {}, tabs {}",
            model::VERSION,
            unread,
            store.articles.len(),
            config.feeds.len(),
            config.tabs.len()
        );
        if store.meta.last_refresh > 0 {
            println!(
                "Last refresh: {}",
                crate::util::format_date(store.meta.last_refresh)
            );
        } else {
            println!("Last refresh: never");
        }
        if errors > 0 {
            for feed in config.feeds.iter().filter(|f| !f.last_error.is_empty()) {
                println!("  ! {}: {}", feed.title, feed.last_error);
            }
        }
        println!("State: {}", config::paths().state.display());
    }
    0
}

fn parse_args() -> Result<Opts, String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut opts = Opts::default();
    let mut i = 0usize;
    let mut add_url: Option<String> = None;
    let mut add_feed_seen = false;

    while i < args.len() {
        let arg = args[i].as_str();
        match arg {
            "--json" => {
                opts.json = true;
            }
            "--snapshot" | "--state" => opts.command = Cmd::Snapshot,
            "--refresh" => opts.command = Cmd::Refresh,
            "--markets" => opts.command = Cmd::Markets,
            "--mark-read" => {
                i += 1;
                opts.command = Cmd::MarkRead(require(&args, i, "--mark-read")?);
            }
            "--mark-unread" => {
                i += 1;
                opts.command = Cmd::MarkUnread(require(&args, i, "--mark-unread")?);
            }
            "--mark-all-read" => opts.command = Cmd::MarkAllRead,
            "--bookmark" => {
                let id = require(&args, i + 1, "--bookmark")?;
                let value = args
                    .get(i + 2)
                    .map(|v| v != "0" && v != "false")
                    .unwrap_or(true);
                i += 2;
                opts.command = Cmd::Bookmark(id, value);
            }
            "--add-feed" => {
                add_url = Some(require(&args, i + 1, "--add-feed")?);
                add_feed_seen = true;
                i += 1;
            }
            "--remove-feed" => {
                i += 1;
                opts.command = Cmd::RemoveFeed(require(&args, i, "--remove-feed")?);
            }
            "--toggle-feed" => {
                i += 1;
                opts.command = Cmd::ToggleFeed(require(&args, i, "--toggle-feed")?);
            }
            "--save-tab" => {
                i += 1;
                opts.command = Cmd::SaveTab(require(&args, i, "--save-tab")?);
            }
            "--delete-tab" => {
                i += 1;
                opts.command = Cmd::DeleteTab(require(&args, i, "--delete-tab")?);
            }
            "--move-tab" => {
                let id = require(&args, i + 1, "--move-tab")?;
                let direction = args.get(i + 2).cloned().unwrap_or_else(|| "up".into());
                i += 2;
                opts.command = Cmd::MoveTab(id, direction != "down");
            }
            "--save-settings" => {
                i += 1;
                opts.command = Cmd::SaveSettings(require(&args, i, "--save-settings")?);
            }
            "--save-markets" => {
                i += 1;
                opts.command = Cmd::SaveMarkets(require(&args, i, "--save-markets")?);
            }
            "--import-opml" => {
                i += 1;
                opts.command = Cmd::ImportOpml(require(&args, i, "--import-opml")?);
            }
            "--export-opml" => {
                let path = args.get(i + 1).cloned().unwrap_or_default();
                i += 1;
                opts.command = Cmd::ExportOpml(path);
            }
            "--seed-starter" => opts.command = Cmd::SeedStarter,
            "--ack-notifications" => opts.command = Cmd::AckNotifications,
            "--image" => {
                i += 1;
                opts.command = Cmd::ArticleImage(require(&args, i, "--image")?);
            }
            "--clean" => opts.command = Cmd::Clean,
            "--status" => opts.command = Cmd::Status,
            "--version" | "-v" => opts.command = Cmd::Version,
            "--help" | "-h" => opts.command = Cmd::Help,
            "--tab" => {
                i += 1;
                opts.tab = Some(require(&args, i, "--tab")?);
            }
            "--tabs" => {
                i += 1;
                opts.tabs = split_list(&require(&args, i, "--tabs")?);
            }
            "--title" => {
                i += 1;
                opts.title = require(&args, i, "--title")?;
            }
            other => return Err(format!("unknown argument: {}", other)),
        }
        i += 1;
    }

    if add_feed_seen {
        opts.command = Cmd::AddFeed(add_url.unwrap_or_default());
    }
    if opts.command == Cmd::Help && !args.is_empty() {
        return Err("unknown arguments; run gazette-engine --help".into());
    }
    Ok(opts)
}

fn require(args: &[String], index: usize, flag: &str) -> Result<String, String> {
    args.get(index)
        .cloned()
        .ok_or_else(|| format!("{} needs a value", flag))
}

fn print_help() {
    println!(
        "\
Gazette engine {}

Usage:
  gazette-engine --snapshot                 Full JSON snapshot for the UI
  gazette-engine --refresh                  Fetch all enabled feeds
  gazette-engine --markets                  Refresh market quotes
  gazette-engine --status [--json]          Human summary
  gazette-engine --mark-read <id>
  gazette-engine --mark-unread <id>
  gazette-engine --mark-all-read [--tab <id>]
  gazette-engine --bookmark <id> <0|1>
  gazette-engine --image <article-id>       Cache one article image
  gazette-engine --add-feed <url> [--title <name>] [--tabs a,b]
  gazette-engine --remove-feed <id>
  gazette-engine --toggle-feed <id>
  gazette-engine --save-tab '<json>'
  gazette-engine --delete-tab <id>
  gazette-engine --save-settings '<json>'
  gazette-engine --save-markets '<json>'
  gazette-engine --import-opml <path>
  gazette-engine --export-opml [path]
  gazette-engine --seed-starter
  gazette-engine --ack-notifications
  gazette-engine --clean
  gazette-engine --version

State lives in ~/.local/state/omarchy/gazette; images in
~/.cache/omarchy/gazette/images.",
        model::VERSION
    );
}
