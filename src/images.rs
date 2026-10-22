//! Article image extraction and local cache.
//!
//! Images are downloaded only from http(s), sniffed by magic bytes, capped at
//! 3 MiB, and stored 0600 under `$XDG_CACHE_HOME/omarchy/gazette/images`.
//! Filenames are content-URL hashes so the same image is shared across feeds.

use crate::util::{
    atomic_write, cache_dir, ensure_dir, http_get, is_http, sha_hex, sniff_image, MAX_IMAGE_BYTES,
};
use std::collections::HashSet;
use std::fs;
use std::path::Path;

const MAX_CACHE_BYTES: u64 = 200 * 1024 * 1024;
const EVICT_TO_BYTES: u64 = 160 * 1024 * 1024;

pub fn cached_path(url: &str) -> Option<String> {
    if !is_http(url) {
        return None;
    }
    let dir = cache_dir();
    let key = sha_hex(url, 32);
    for ext in ["jpg", "png", "webp", "gif"] {
        let candidate = dir.join(format!("{}.{}", key, ext));
        if let Ok(md) = fs::metadata(&candidate) {
            if md.len() > 0 {
                return Some(candidate.to_string_lossy().to_string());
            }
        }
    }
    None
}

/// Download an image if needed and return its absolute path.
pub fn ensure_cached(url: &str) -> Result<String, String> {
    if !is_http(url) {
        return Err("not an http(s) image".into());
    }
    if let Some(path) = cached_path(url) {
        return Ok(path);
    }

    let dir = cache_dir();
    ensure_dir(&dir)?;
    let (bytes, _, _) = http_get(url, MAX_IMAGE_BYTES, 12, None, None)?;
    let ext = sniff_image(&bytes).ok_or_else(|| "response is not a supported image".to_string())?;
    let key = sha_hex(url, 32);
    let path = dir.join(format!("{}.{}", key, ext));
    atomic_write(&path, &bytes)?;
    evict_if_needed(&dir);
    Ok(path.to_string_lossy().to_string())
}

/// Drop the oldest files once the cache crosses 200 MiB, down to 160 MiB.
fn evict_if_needed(dir: &Path) {
    let mut files: Vec<(u64, std::time::SystemTime, std::path::PathBuf)> = Vec::new();
    let mut total: u64 = 0;
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            if let Ok(md) = entry.metadata() {
                if md.is_file() {
                    total += md.len();
                    files.push((
                        md.len(),
                        md.modified().unwrap_or(std::time::UNIX_EPOCH),
                        entry.path(),
                    ));
                }
            }
        }
    }
    if total <= MAX_CACHE_BYTES {
        return;
    }
    files.sort_by_key(|(_, modified, _)| *modified);
    for (size, _, path) in files {
        if total <= EVICT_TO_BYTES {
            break;
        }
        if fs::remove_file(&path).is_ok() {
            total = total.saturating_sub(size);
        }
    }
}

/// Remove cached files that no current article references.
pub fn cleanup(referenced: HashSet<String>) {
    let dir = cache_dir();
    let Ok(entries) = fs::read_dir(&dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if name.starts_with('.') {
            continue;
        }
        if !referenced.iter().any(|p| p.ends_with(name)) {
            let _ = fs::remove_file(path);
        }
    }
}
