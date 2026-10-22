//! Cached full article bodies and inline image blocks.
//!
//! One JSON file per article under `<state>/texts/`, written atomically. This
//! keeps read flags in `state.json` tiny and lets a text fetch run without the
//! engine lock or a feed refresh.

use crate::article::Block;
use crate::util::{atomic_write, ensure_dir, state_dir};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct CachedText {
    #[serde(default)]
    pub v: u32,
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub blocks: Vec<Block>,
}

fn dir() -> PathBuf {
    state_dir().join("texts")
}

fn file_for(id: &str) -> PathBuf {
    let safe: String = id
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
        .collect();
    dir().join(format!("{}.json", safe))
}

pub fn get(id: &str) -> Option<CachedText> {
    let raw = fs::read_to_string(file_for(id)).ok()?;
    let mut cached: CachedText = serde_json::from_str(&raw).ok()?;
    if cached.v < 3 {
        let _ = fs::remove_file(file_for(id));
        return None;
    }
    if cached.blocks.is_empty() && !cached.text.trim().is_empty() {
        cached.blocks.push(Block::text(cached.text.clone()));
    }
    // Image cache eviction must not leave broken <img> blocks behind.
    cached
        .blocks
        .retain(|block| block.kind != "image" || Path::new(&block.file).is_file());
    if cached.text.trim().is_empty() {
        let text = cached
            .blocks
            .iter()
            .filter(|block| block.kind == "text")
            .map(|block| block.text.as_str())
            .collect::<Vec<_>>()
            .join("\n\n");
        cached.text = text;
    }
    if cached.text.trim().is_empty() {
        None
    } else {
        Some(cached)
    }
}

pub fn put(id: &str, cached: &CachedText) -> Result<(), String> {
    if cached.text.trim().is_empty() {
        return Err("empty article text".into());
    }
    ensure_dir(&dir())?;
    let body = serde_json::to_vec(cached).map_err(|e| e.to_string())?;
    atomic_write(&file_for(id), &body)
}

/// Drop cached bodies for articles that no longer exist, plus legacy `.txt`
/// entries from Gazette 1.1.
pub fn retain(valid: &HashSet<String>) -> Result<(), String> {
    let Ok(entries) = fs::read_dir(dir()) else {
        return Ok(());
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let is_json = path.extension().and_then(|ext| ext.to_str()) == Some("json");
        let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
        if !is_json || !valid.contains(stem) {
            let _ = fs::remove_file(path);
        }
    }
    Ok(())
}
