//! Small shared helpers: paths, HTTP with hard limits, atomic writes, locking,
//! HTML-to-text, URL normalisation and Turkish-style number formatting.

use sha2::{Digest, Sha256};
use std::fs;
use std::io::{Read, Write};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

static TMP_COUNTER: AtomicU64 = AtomicU64::new(0);

pub const USER_AGENT: &str =
    "Mozilla/5.0 (X11; Linux x86_64) Gazette/1.0 (+https://github.com/KaplanHalil/omarchy-gazette)";
pub const MAX_FEED_BYTES: usize = 6 * 1024 * 1024;
pub const MAX_IMAGE_BYTES: usize = 3 * 1024 * 1024;

pub fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

pub fn home() -> PathBuf {
    PathBuf::from(std::env::var("HOME").unwrap_or_else(|_| "/tmp".into()))
}

pub fn state_dir() -> PathBuf {
    let base = std::env::var("XDG_STATE_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| home().join(".local/state"));
    base.join("omarchy/gazette")
}

pub fn cache_dir() -> PathBuf {
    let base = std::env::var("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| home().join(".cache"));
    base.join("omarchy/gazette/images")
}

/// Create a directory with 0700 permissions, verifying it is not a symlink.
pub fn ensure_dir(path: &Path) -> Result<(), String> {
    if path.exists() {
        let md = fs::symlink_metadata(path).map_err(|e| e.to_string())?;
        if md.file_type().is_symlink() || !md.is_dir() {
            return Err(format!(
                "refusing non-directory state path: {}",
                path.display()
            ));
        }
        return Ok(());
    }
    fs::create_dir_all(path).map_err(|e| e.to_string())?;
    fs::set_permissions(path, fs::Permissions::from_mode(0o700)).map_err(|e| e.to_string())?;
    Ok(())
}

pub fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let parent = path.parent().ok_or("path has no parent")?;
    ensure_dir(parent)?;
    let tmp = parent.join(format!(
        ".{}.{}.{}.tmp",
        path.file_name().and_then(|s| s.to_str()).unwrap_or("state"),
        std::process::id(),
        TMP_COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    {
        let mut f = fs::OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .mode(0o600)
            .open(&tmp)
            .map_err(|e| e.to_string())?;
        f.write_all(bytes).map_err(|e| e.to_string())?;
        f.sync_all().ok();
    }
    fs::rename(&tmp, path).map_err(|e| {
        let _ = fs::remove_file(&tmp);
        e.to_string()
    })?;
    let _ = fs::set_permissions(path, fs::Permissions::from_mode(0o600));
    Ok(())
}

pub fn sha_hex(input: &str, len: usize) -> String {
    let mut hasher = Sha256::new();
    hasher.update(input.as_bytes());
    let hex = format!("{:x}", hasher.finalize());
    hex.chars().take(len).collect()
}

pub fn slugify(input: &str) -> String {
    let mut out = String::new();
    for ch in input.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
        } else if ch.is_whitespace() || ch == '-' || ch == '_' {
            if !out.is_empty() && !out.ends_with('-') {
                out.push('-');
            }
        } else if !ch.is_ascii() {
            let mapped = match ch {
                'ı' | 'İ' => Some('i'),
                'ş' | 'Ş' => Some('s'),
                'ğ' | 'Ğ' => Some('g'),
                'ü' | 'Ü' => Some('u'),
                'ö' | 'Ö' => Some('o'),
                'ç' | 'Ç' => Some('c'),
                _ => None,
            };
            if let Some(mapped) = mapped {
                out.push(mapped);
            }
        }
    }
    let trimmed = out.trim_matches('-').to_string();
    if trimmed.is_empty() {
        "tab".into()
    } else {
        trimmed
    }
}

/// First `<img src>` (or `data-src`) in a fragment of HTML.
pub fn first_img_src(html: &str) -> Option<String> {
    let lower = html.to_ascii_lowercase();
    let mut search = 0usize;
    while let Some(found) = lower[search..].find("<img") {
        let start = search + found;
        let end = match lower[start..].find('>') {
            Some(e) => start + e,
            None => break,
        };
        let tag = &html[start..end];
        for attr in ["data-src", "src"] {
            if let Some(value) = attr_value(tag, attr) {
                let value = decode_entities(&value);
                if !value.is_empty() {
                    return Some(value);
                }
            }
        }
        search = end + 1;
    }
    None
}

pub fn attr_value(tag: &str, wanted: &str) -> Option<String> {
    let inner = tag
        .trim_start_matches('<')
        .trim_start_matches(|c: char| c.is_ascii_alphabetic())
        .trim_end_matches('>')
        .trim_end_matches('/');
    for token in inner.split_whitespace() {
        let Some((name, value)) = token.split_once('=') else {
            continue;
        };
        if name.eq_ignore_ascii_case(wanted) {
            let trimmed = value.trim_matches(|c| c == '"' || c == '\'');
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }
    }
    None
}

pub fn is_http(url: &str) -> bool {
    let lower = url.trim().to_ascii_lowercase();
    lower.starts_with("https://") || lower.starts_with("http://")
}

/// HTTP GET with a hard size cap and timeout. Returns (bytes, etag, last_modified).
pub fn http_get(
    url: &str,
    max_bytes: usize,
    timeout_secs: u64,
    etag: Option<&str>,
    last_modified: Option<&str>,
) -> Result<(Vec<u8>, String, String), String> {
    if !is_http(url) {
        return Err("only http(s) URLs are allowed".into());
    }
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(8))
        .timeout_read(Duration::from_secs(timeout_secs))
        .timeout_write(Duration::from_secs(8))
        .user_agent(USER_AGENT)
        .build();

    let mut req = agent
        .get(url)
        .set("Accept", "application/rss+xml, application/atom+xml, application/xml, text/xml, text/html;q=0.8, */*;q=0.5");
    if let Some(v) = etag {
        if !v.is_empty() {
            req = req.set("If-None-Match", v);
        }
    }
    if let Some(v) = last_modified {
        if !v.is_empty() {
            req = req.set("If-Modified-Since", v);
        }
    }

    let resp = match req.call() {
        Ok(r) => r,
        Err(ureq::Error::Status(304, _)) => return Ok((Vec::new(), String::new(), String::new())),
        Err(ureq::Error::Status(code, _)) => return Err(format!("HTTP {}", code)),
        Err(e) => return Err(e.to_string()),
    };

    let etag = resp.header("etag").unwrap_or("").to_string();
    let lm = resp.header("last-modified").unwrap_or("").to_string();

    let mut bytes = Vec::new();
    resp.into_reader()
        .take(max_bytes as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > max_bytes {
        return Err(format!("response larger than {} bytes", max_bytes));
    }
    Ok((bytes, etag, lm))
}

/// Simple advisory lock so the 5-minute timer, another monitor's widget and a
/// manual refresh cannot write state concurrently. The PID is stored in the
/// file: a lock left behind by a killed shell is reclaimed immediately, and a
/// live holder gets a short grace period instead of an instant error.
pub struct LockGuard {
    path: PathBuf,
    pid: u32,
}

impl LockGuard {
    pub fn acquire() -> Result<LockGuard, String> {
        let dir = state_dir();
        ensure_dir(&dir)?;
        let path = dir.join("engine.lock");
        let pid = std::process::id();
        let deadline = std::time::Instant::now() + Duration::from_secs(10);

        loop {
            match fs::OpenOptions::new()
                .create_new(true)
                .write(true)
                .mode(0o600)
                .open(&path)
            {
                Ok(mut file) => {
                    let _ = write!(file, "{}", pid);
                    return Ok(LockGuard { path, pid });
                }
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                    let owner = fs::read_to_string(&path)
                        .ok()
                        .and_then(|raw| raw.trim().parse::<u32>().ok())
                        .unwrap_or(0);
                    let alive = owner > 0 && Path::new(&format!("/proc/{}", owner)).exists();
                    let age = fs::metadata(&path)
                        .and_then(|md| md.modified())
                        .ok()
                        .and_then(|modified| modified.elapsed().ok())
                        .map(|age| age.as_secs())
                        .unwrap_or(u64::MAX);

                    // Dead owner, or a live process that has been holding the
                    // lock for far longer than any refresh should take.
                    if !alive || age >= 300 {
                        let _ = fs::remove_file(&path);
                        continue;
                    }
                    if std::time::Instant::now() >= deadline {
                        return Err("another gazette-engine operation is running".into());
                    }
                    std::thread::sleep(Duration::from_millis(200));
                }
                Err(error) => return Err(format!("could not acquire lock: {}", error)),
            }
        }
    }
}

impl Drop for LockGuard {
    fn drop(&mut self) {
        // Never delete a lock that another process has taken over.
        let owner = fs::read_to_string(&self.path)
            .ok()
            .and_then(|raw| raw.trim().parse::<u32>().ok());
        if owner == Some(self.pid) {
            let _ = fs::remove_file(&self.path);
        }
    }
}

/// Decode the handful of HTML entities that show up in feeds. Numeric entities
/// are handled generically; unknown named ones are left alone.
pub fn decode_entities(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let chars: Vec<char> = input.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] != '&' {
            out.push(chars[i]);
            i += 1;
            continue;
        }
        let mut j = i + 1;
        while j < chars.len() && j - i < 12 && chars[j] != ';' {
            j += 1;
        }
        if j >= chars.len() || chars[j] != ';' {
            out.push(chars[i]);
            i += 1;
            continue;
        }
        let entity: String = chars[i + 1..j].iter().collect();
        let decoded = if let Some(rest) = entity
            .strip_prefix("#x")
            .or_else(|| entity.strip_prefix("#X"))
        {
            u32::from_str_radix(rest, 16).ok().and_then(char::from_u32)
        } else if let Some(rest) = entity.strip_prefix('#') {
            rest.parse::<u32>().ok().and_then(char::from_u32)
        } else {
            match entity.as_str() {
                "amp" => Some('&'),
                "lt" => Some('<'),
                "gt" => Some('>'),
                "quot" => Some('"'),
                "apos" => Some('\''),
                "nbsp" => Some(' '),
                "hellip" => Some('…'),
                "mdash" => Some('—'),
                "ndash" => Some('–'),
                "lsquo" => Some('‘'),
                "rsquo" => Some('’'),
                "ldquo" => Some('“'),
                "rdquo" => Some('”'),
                "euro" => Some('€'),
                "pound" => Some('£'),
                "copy" => Some('©'),
                "reg" => Some('®'),
                "trade" => Some('™'),
                "middot" => Some('·'),
                "bull" => Some('•'),
                _ => None,
            }
        };
        match decoded {
            Some(ch) => {
                out.push(ch);
                i = j + 1;
            }
            None => {
                out.push('&');
                i += 1;
            }
        }
    }
    out
}

/// Convert feed HTML into readable plain text without executing or rendering
/// anything. Script/style blocks and comments are removed first.
pub fn strip_html(input: &str) -> String {
    let mut text = String::with_capacity(input.len());
    let bytes: Vec<char> = input.chars().collect();
    let mut i = 0;
    let mut skipping: Option<&str> = None;

    while i < bytes.len() {
        if bytes[i] == '<' {
            let mut tag_end = i + 1;
            while tag_end < bytes.len() && bytes[tag_end] != '>' {
                tag_end += 1;
            }
            let tag: String = bytes[i + 1..tag_end.min(bytes.len())].iter().collect();
            let name = tag
                .trim_start_matches('/')
                .split_whitespace()
                .next()
                .unwrap_or("")
                .trim_end_matches('/')
                .to_ascii_lowercase();

            if skipping.is_none() && (name == "script" || name == "style") {
                skipping = Some(if name == "script" { "script" } else { "style" });
            }
            if skipping.is_some()
                && name == skipping.unwrap_or("")
                && tag.trim_start().starts_with('/')
            {
                skipping = None;
            }

            let block_break = matches!(
                name.as_str(),
                "p" | "br" | "div" | "li" | "tr" | "h1" | "h2" | "h3" | "h4" | "h5" | "blockquote"
            );
            if skipping.is_none() && block_break {
                text.push(' ');
            }
            i = (tag_end + 1).min(bytes.len());
            continue;
        }
        if skipping.is_none() {
            text.push(bytes[i]);
        }
        i += 1;
    }

    let decoded = decode_entities(&text);
    collapse_whitespace(&decoded)
}

pub fn collapse_whitespace(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut last_space = false;
    for ch in input.chars() {
        if ch.is_whitespace() {
            if !last_space {
                out.push(' ');
                last_space = true;
            }
        } else {
            out.push(ch);
            last_space = false;
        }
    }
    out.trim().to_string()
}

pub fn truncate_chars(input: &str, max: usize) -> String {
    if input.chars().count() <= max {
        return input.to_string();
    }
    let mut out: String = input.chars().take(max).collect();
    out = out.trim_end().to_string();
    if let Some(pos) = out.rfind(' ') {
        if pos > max / 2 {
            out.truncate(pos);
        }
    }
    out.push('…');
    out
}

/// Resolve a possibly relative URL against a base.
pub fn resolve_url(base: &str, candidate: &str) -> String {
    let c = candidate.trim();
    if c.is_empty() {
        return String::new();
    }
    if is_http(c) {
        return c.to_string();
    }
    if c.starts_with("//") {
        return format!("https:{}", c);
    }
    if c.starts_with("data:") || c.starts_with("javascript:") {
        return String::new();
    }
    if let Ok(base_url) = url::Url::parse(base) {
        if let Ok(joined) = base_url.join(c) {
            let s = joined.to_string();
            if is_http(&s) {
                return s;
            }
        }
    }
    String::new()
}

pub fn format_money(value: f64, currency: &str, decimals: u8) -> String {
    if !value.is_finite() {
        return "—".into();
    }
    let d = decimals as usize;
    let neg = value < 0.0;
    let abs = value.abs();
    let formatted = format!("{:.*}", d, abs);

    // Strip a trailing ",00" for large whole numbers (indices, BTC).
    let formatted = if d > 0 && formatted.ends_with(&format!(".{}", "0".repeat(d))) {
        formatted[..formatted.len() - d - 1].to_string()
    } else {
        formatted
    };

    let (int_part, frac_part) = match formatted.split_once('.') {
        Some((i, f)) => (i.to_string(), Some(f.to_string())),
        None => (formatted, None),
    };

    let mut grouped = String::new();
    for (idx, ch) in int_part.chars().rev().enumerate() {
        if idx > 0 && idx % 3 == 0 {
            grouped.push('.');
        }
        grouped.push(ch);
    }
    let mut int_final: String = grouped.chars().rev().collect();

    if let Some(frac) = frac_part {
        int_final.push(',');
        int_final.push_str(&frac);
    }

    let sign = if neg { "-" } else { "" };
    let symbol = match currency {
        "TRY" => "₺",
        "USD" => "$",
        "EUR" => "€",
        "GBP" => "£",
        _ => "",
    };

    if symbol.is_empty() {
        format!("{}{} {}", sign, int_final, currency)
    } else {
        format!("{}{}{}", sign, symbol, int_final)
    }
}

pub fn format_change(pct: f64) -> String {
    if !pct.is_finite() {
        return "— %0,00".into();
    }
    let rounded = (pct.abs() * 100.0).round() / 100.0;
    let arrow = if pct > 0.0 && rounded >= 0.005 {
        "▲"
    } else if pct < 0.0 && rounded >= 0.005 {
        "▼"
    } else {
        "·"
    };
    let abs_text = format!("{:.2}", rounded).replace('.', ",");
    format!("{} %{}", arrow, abs_text)
}

pub fn relative_age(ts: i64, now_ts: i64) -> String {
    let delta = now_ts - ts;
    if delta < 60 {
        "now".into()
    } else if delta < 3600 {
        format!("{}m", delta / 60)
    } else if delta < 86_400 {
        format!("{}h", delta / 3600)
    } else if delta < 86_400 * 30 {
        format!("{}d", delta / 86_400)
    } else if delta < 86_400 * 365 {
        format!("{}mo", delta / (86_400 * 30))
    } else {
        format!("{}y", delta / (86_400 * 365))
    }
}

pub fn format_date(ts: i64) -> String {
    use chrono::{Local, TimeZone};
    match Local.timestamp_opt(ts, 0) {
        chrono::LocalResult::Single(dt) => dt.format("%d %b %H:%M").to_string(),
        _ => String::new(),
    }
}

pub fn split_list(raw: &str) -> Vec<String> {
    raw.split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

/// Minimum image sanity check: reject responses that are clearly not images even
/// when the server lies about Content-Type.
pub fn sniff_image(bytes: &[u8]) -> Option<&'static str> {
    if bytes.len() < 12 {
        return None;
    }
    if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        return Some("jpg");
    }
    if bytes.starts_with(&[0x89, b'P', b'N', b'G']) {
        return Some("png");
    }
    if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        return Some("gif");
    }
    if bytes.starts_with(b"RIFF") && &bytes[8..12] == b"WEBP" {
        return Some("webp");
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_turkish_money() {
        assert_eq!(format_money(6542.18, "TRY", 2), "₺6.542,18");
        assert_eq!(format_money(49.6218, "TRY", 4), "₺49,6218");
        assert_eq!(format_money(4102.5, "USD", 2), "$4.102,50");
        assert_eq!(format_money(11577.12, "TRY", 2), "₺11.577,12");
    }

    #[test]
    fn formats_change_arrows() {
        assert_eq!(format_change(1.274), "▲ %1,27");
        assert_eq!(format_change(-0.204), "▼ %0,20");
        assert_eq!(format_change(0.0001), "· %0,00");
    }

    #[test]
    fn decodes_entities_and_numerics() {
        assert_eq!(
            decode_entities("A &amp; B &#39;x&#x27; &hellip;"),
            "A & B 'x' …"
        );
    }

    #[test]
    fn strips_tags_and_scripts() {
        let html = "<p>Hello <b>world</b></p><script>alert('x')</script><p>Second&nbsp;line</p>";
        assert_eq!(strip_html(html), "Hello world Second line");
    }

    #[test]
    fn extracts_first_image() {
        let html = "<p>x</p><img alt=\"a\" data-src=\"/a.jpg\"><img src='https://b.test/b.png'>";
        assert_eq!(first_img_src(html).as_deref(), Some("/a.jpg"));
        assert_eq!(
            first_img_src("<img src=\"https://x.test/a.png?a=1&amp;b=2\">").as_deref(),
            Some("https://x.test/a.png?a=1&b=2")
        );
    }

    #[test]
    fn resolves_relative_urls() {
        assert_eq!(
            resolve_url("https://example.com/news/item", "/img/a.jpg"),
            "https://example.com/img/a.jpg"
        );
        assert_eq!(
            resolve_url("https://example.com/", "data:image/png;base64,x"),
            ""
        );
    }

    #[test]
    fn reads_image_magic() {
        assert_eq!(
            sniff_image(&[0xFF, 0xD8, 0xFF, 0, 0, 0, 0, 0, 0, 0, 0, 0]),
            Some("jpg")
        );
        assert_eq!(sniff_image(b"not an image at all"), None);
    }

    #[test]
    fn slugs_unicode() {
        assert_eq!(slugify("Çeyrek Altın"), "ceyrek-altin");
        assert_eq!(slugify("AI & ML"), "ai-ml");
    }
}
