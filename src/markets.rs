//! Market data. Yahoo Finance chart quotes are the primary source (no API key),
//! TCMB and open.er-api.com are fallbacks for USD/TRY. Metals are converted to
//! Turkish grams locally so no Turkish "altın" vendor is scraped.
//!
//! All prices are cached in state.json; when the network fails the previous
//! quote is returned with `stale: true`.

use crate::model::{BaseQuote, CachedQuote, Config, MarketCache, MarketCfg};
use crate::util::{format_change, format_money, http_get, now};
use std::collections::HashMap;
use std::sync::Mutex;

const GRAMS_PER_OUNCE: f64 = 31.103_476_8;
/// 22k çeyrek coin is 1.75 g; expressed as 24k-equivalent grams.
const QUARTER_GOLD_GRAMS: f64 = 1.6;

pub fn refresh(config: &Config, cache: &mut MarketCache) -> Result<(), String> {
    let enabled: Vec<&MarketCfg> = config.markets.iter().filter(|m| m.enabled).collect();
    if enabled.is_empty() {
        cache.quotes.clear();
        cache.updated = now();
        return Ok(());
    }

    let mut symbols: Vec<String> = Vec::new();
    let mut need_usdtry = false;
    let mut need_eurtry = false;

    for market in &enabled {
        match market.kind.as_str() {
            "gold_gram" | "gold_quarter" | "silver_gram" => {
                if !market.symbol.is_empty() {
                    push_unique(&mut symbols, market.symbol.clone());
                }
                need_usdtry = true;
            }
            "gold_ounce" => {
                if !market.symbol.is_empty() {
                    push_unique(&mut symbols, market.symbol.clone());
                }
            }
            _ => {
                if !market.symbol.is_empty() {
                    push_unique(&mut symbols, market.symbol.clone());
                }
                if market.quote == "TRY" {
                    need_usdtry = true;
                }
            }
        }
        if market.id == "eurtry" || market.symbol == "EURTRY=X" {
            need_eurtry = true;
        }
    }
    if need_usdtry {
        push_unique(&mut symbols, "USDTRY=X".into());
    }
    if need_eurtry && !need_usdtry {
        push_unique(&mut symbols, "EURTRY=X".into());
    }

    let fetched: Mutex<HashMap<String, Result<BaseQuote, String>>> = Mutex::new(HashMap::new());
    let queue = Mutex::new(symbols.clone());
    let worker_count = symbols.len().min(6).max(1);
    std::thread::scope(|scope| {
        for _ in 0..worker_count {
            scope.spawn(|| loop {
                let symbol = {
                    let mut q = queue.lock().unwrap();
                    match q.pop() {
                        Some(s) => s,
                        None => break,
                    }
                };
                let result = fetch_yahoo(&symbol).or_else(|first_err| {
                    if symbol == "USDTRY=X" {
                        fetch_usdtry_fallback()
                            .map_err(|second| format!("{}; {}", first_err, second))
                    } else if symbol == "EURTRY=X" {
                        fetch_eurtry_fallback()
                            .map_err(|second| format!("{}; {}", first_err, second))
                    } else {
                        Err(first_err)
                    }
                });
                fetched.lock().unwrap().insert(symbol, result);
            });
        }
    });

    let results = fetched.into_inner().unwrap();
    for (symbol, result) in results {
        match result {
            Ok(quote) => {
                cache.bases.insert(symbol, quote);
            }
            Err(_) => { /* keep the previous base for stale rendering */ }
        }
    }

    let usdtry = cache
        .bases
        .get("USDTRY=X")
        .map(|b| b.price)
        .filter(|p| *p > 0.0);

    let updated = now();
    let mut quotes: HashMap<String, CachedQuote> = HashMap::new();
    for market in &enabled {
        let old = cache.quotes.get(&market.id).cloned().unwrap_or_default();
        let quote = build_quote(market, &cache.bases, usdtry, &old, updated);
        quotes.insert(market.id.clone(), quote);
    }
    cache.quotes = quotes;
    cache.updated = updated;
    Ok(())
}

fn push_unique(list: &mut Vec<String>, value: String) {
    if !list.contains(&value) {
        list.push(value);
    }
}

fn build_quote(
    market: &MarketCfg,
    bases: &HashMap<String, BaseQuote>,
    usdtry: Option<f64>,
    old: &CachedQuote,
    updated: i64,
) -> CachedQuote {
    let currency = if market.quote.is_empty() {
        "TRY"
    } else {
        market.quote.as_str()
    };

    let (value, change) = match market.kind.as_str() {
        "gold_gram" | "gold_quarter" | "silver_gram" => {
            let base = bases.get(&market.symbol);
            match (base, usdtry) {
                (Some(base), Some(fx)) => {
                    let grams = match market.kind.as_str() {
                        "gold_gram" => base.price / GRAMS_PER_OUNCE,
                        "gold_quarter" => base.price / GRAMS_PER_OUNCE * QUARTER_GOLD_GRAMS,
                        _ => base.price / GRAMS_PER_OUNCE,
                    };
                    (
                        grams * fx,
                        base.change_pct
                            + bases.get("USDTRY=X").map(|b| b.change_pct).unwrap_or(0.0),
                    )
                }
                _ => return stale_or_error(market, old, updated),
            }
        }
        "gold_ounce" => match bases.get(&market.symbol) {
            Some(base) => (base.price, base.change_pct),
            None => return stale_or_error(market, old, updated),
        },
        _ => {
            let Some(base) = bases.get(&market.symbol) else {
                return stale_or_error(market, old, updated);
            };
            let mut price = base.price;
            let mut change = base.change_pct;
            if currency == "TRY" && base.currency == "USD" {
                match usdtry {
                    Some(fx) => {
                        price *= fx;
                        change += bases.get("USDTRY=X").map(|b| b.change_pct).unwrap_or(0.0);
                    }
                    None => return stale_or_error(market, old, updated),
                }
            } else if currency == "USD" && base.currency == "TRY" {
                match usdtry {
                    Some(fx) if fx > 0.0 => {
                        price /= fx;
                        change -= bases.get("USDTRY=X").map(|b| b.change_pct).unwrap_or(0.0);
                    }
                    _ => return stale_or_error(market, old, updated),
                }
            }
            (price, change)
        }
    };

    CachedQuote {
        label: market.label.clone(),
        price: value,
        change_pct: change,
        currency: currency.to_string(),
        price_text: format_money(value, currency, market.decimals),
        change_text: format_change(change),
        updated,
        stale: false,
        error: String::new(),
    }
}

fn stale_or_error(market: &MarketCfg, old: &CachedQuote, updated: i64) -> CachedQuote {
    if old.price > 0.0 {
        CachedQuote {
            label: market.label.clone(),
            stale: true,
            updated,
            ..old.clone()
        }
    } else {
        CachedQuote {
            label: market.label.clone(),
            price: 0.0,
            change_pct: 0.0,
            currency: market.quote.clone(),
            price_text: "—".into(),
            change_text: "—".into(),
            updated,
            stale: true,
            error: "price unavailable".into(),
        }
    }
}

/// Yahoo's undocumented chart endpoint. `regularMarketPrice` is enough for the
/// ticker; `regularMarketChangePercent` gives the daily move.
fn fetch_yahoo(symbol: &str) -> Result<BaseQuote, String> {
    let encoded = urlencode(symbol);
    let mut last_error = String::new();
    for host in ["query1.finance.yahoo.com", "query2.finance.yahoo.com"] {
        let url = format!(
            "https://{}/v8/finance/chart/{}?interval=1d&range=1d",
            host, encoded
        );
        match http_get(&url, 512 * 1024, 10, None, None) {
            Ok((bytes, _, _)) => {
                let text = String::from_utf8_lossy(&bytes);
                match parse_yahoo(&text) {
                    Ok(quote) => return Ok(quote),
                    Err(e) => last_error = e,
                }
            }
            Err(e) => last_error = e,
        }
    }
    Err(last_error)
}

fn parse_yahoo(text: &str) -> Result<BaseQuote, String> {
    let root: serde_json::Value = serde_json::from_str(text).map_err(|e| e.to_string())?;
    let meta = &root["chart"]["result"][0]["meta"];
    let price = meta["regularMarketPrice"]
        .as_f64()
        .or_else(|| {
            root["chart"]["result"][0]["indicators"]["quote"][0]["close"]
                .as_array()
                .and_then(|arr| arr.iter().rev().find_map(|v| v.as_f64()))
        })
        .filter(|p| p.is_finite() && *p > 0.0)
        .ok_or_else(|| "no price in Yahoo response".to_string())?;

    let change = meta["regularMarketChangePercent"]
        .as_f64()
        .or_else(|| {
            let prev = meta["chartPreviousClose"].as_f64()?;
            if prev > 0.0 {
                Some((price - prev) / prev * 100.0)
            } else {
                None
            }
        })
        .unwrap_or(0.0);

    Ok(BaseQuote {
        price,
        change_pct: change,
        currency: meta["currency"].as_str().unwrap_or("USD").to_string(),
        updated: now(),
    })
}

fn fetch_usdtry_fallback() -> Result<BaseQuote, String> {
    let mut errors = Vec::new();
    match fetch_tcmb() {
        Ok(rate) => return Ok(rate),
        Err(e) => errors.push(e),
    }
    match fetch_erapi() {
        Ok(rate) => return Ok(rate),
        Err(e) => errors.push(e),
    }
    Err(errors.join("; "))
}

fn fetch_eurtry_fallback() -> Result<BaseQuote, String> {
    let xml = http_get(
        "https://www.tcmb.gov.tr/kurlar/today.xml",
        1024 * 1024,
        15,
        None,
        None,
    )?;
    let text = String::from_utf8_lossy(&xml.0);
    let rate = tcmb_rate(&text, "EUR").ok_or_else(|| "TCMB EUR missing".to_string())?;
    Ok(BaseQuote {
        price: rate,
        change_pct: 0.0,
        currency: "TRY".into(),
        updated: now(),
    })
}

fn fetch_tcmb() -> Result<BaseQuote, String> {
    let (bytes, _, _) = http_get(
        "https://www.tcmb.gov.tr/kurlar/today.xml",
        1024 * 1024,
        15,
        None,
        None,
    )?;
    let text = String::from_utf8_lossy(&bytes);
    let rate = tcmb_rate(&text, "USD").ok_or_else(|| "TCMB USD missing".to_string())?;
    Ok(BaseQuote {
        price: rate,
        change_pct: 0.0,
        currency: "TRY".into(),
        updated: now(),
    })
}

fn tcmb_rate(xml: &str, code: &str) -> Option<f64> {
    for block in xml.split("<Currency ").skip(1) {
        let head = block.split('>').next().unwrap_or("");
        if !head.contains(&format!("Kod=\"{}\"", code)) {
            continue;
        }
        // Prefer selling; fall back to buying.
        for tag in ["ForexSelling", "ForexBuying"] {
            if let Some(start) = block.find(&format!("<{}>", tag)) {
                let rest = &block[start + tag.len() + 2..];
                if let Some(end) = rest.find(&format!("</{}>", tag)) {
                    let raw = rest[..end].trim().replace(',', ".");
                    if let Ok(value) = raw.parse::<f64>() {
                        if value > 0.0 {
                            return Some(value);
                        }
                    }
                }
            }
        }
    }
    None
}

fn fetch_erapi() -> Result<BaseQuote, String> {
    let (bytes, _, _) = http_get(
        "https://open.er-api.com/v6/latest/USD",
        512 * 1024,
        12,
        None,
        None,
    )?;
    let root: serde_json::Value = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
    let rate = root["rates"]["TRY"]
        .as_f64()
        .filter(|v| *v > 0.0)
        .ok_or_else(|| "er-api TRY missing".to_string())?;
    Ok(BaseQuote {
        price: rate,
        change_pct: 0.0,
        currency: "TRY".into(),
        updated: now(),
    })
}

fn urlencode(symbol: &str) -> String {
    let mut out = String::new();
    for byte in symbol.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char)
            }
            _ => out.push_str(&format!("%{:02X}", byte)),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base(price: f64, currency: &str) -> BaseQuote {
        BaseQuote {
            price,
            change_pct: 1.0,
            currency: currency.into(),
            updated: 0,
        }
    }

    fn market(kind: &str, quote: &str, decimals: u8) -> MarketCfg {
        MarketCfg {
            id: "x".into(),
            label: "X".into(),
            kind: kind.into(),
            symbol: if kind == "silver_gram" {
                "SI=F".into()
            } else {
                "GC=F".into()
            },
            quote: quote.into(),
            enabled: true,
            decimals,
        }
    }

    #[test]
    fn computes_gram_and_quarter_gold() {
        let mut bases = HashMap::new();
        bases.insert("GC=F".to_string(), base(4102.5, "USD"));
        bases.insert("USDTRY=X".to_string(), base(49.62, "TRY"));
        let usd_try = Some(49.62);
        let old = CachedQuote::default();

        let gram = build_quote(&market("gold_gram", "TRY", 2), &bases, usd_try, &old, 0);
        let expected = 4102.5 / GRAMS_PER_OUNCE * 49.62;
        assert!((gram.price - expected).abs() < 0.001);
        assert!(gram.price_text.starts_with("₺6."));

        let quarter = build_quote(&market("gold_quarter", "TRY", 2), &bases, usd_try, &old, 0);
        assert!((quarter.price - expected * QUARTER_GOLD_GRAMS).abs() < 0.001);
    }

    #[test]
    fn converts_usd_symbols_to_try() {
        let mut bases = HashMap::new();
        bases.insert("BTC-USD".to_string(), base(100_000.0, "USD"));
        bases.insert("USDTRY=X".to_string(), base(50.0, "TRY"));
        let mut cfg = market("yahoo", "TRY", 0);
        cfg.symbol = "BTC-USD".into();
        let quote = build_quote(&cfg, &bases, Some(50.0), &CachedQuote::default(), 0);
        assert!((quote.price - 5_000_000.0).abs() < 0.001);
    }

    #[test]
    fn falls_back_to_stale_quote() {
        let old = CachedQuote {
            price: 12.5,
            price_text: "₺12,50".into(),
            ..CachedQuote::default()
        };
        let quote = build_quote(&market("yahoo", "TRY", 2), &HashMap::new(), None, &old, 7);
        assert!(quote.stale);
        assert_eq!(quote.price, 12.5);
        assert_eq!(quote.updated, 7);
    }

    #[test]
    fn parses_tcmb_xml() {
        let xml = r#"<Tarih_Date><Currency Kod="USD"><Unit>1</Unit><ForexBuying>49.1000</ForexBuying><ForexSelling>49.2000</ForexSelling></Currency>
        <Currency Kod="EUR"><ForexBuying>54.1</ForexBuying><ForexSelling>54.2</ForexSelling></Currency></Tarih_Date>"#;
        assert_eq!(tcmb_rate(xml, "USD"), Some(49.2));
        assert_eq!(tcmb_rate(xml, "EUR"), Some(54.2));
        assert_eq!(tcmb_rate(xml, "GBP"), None);
    }
}
