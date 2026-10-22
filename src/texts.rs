//! Full article text cache.
//!
//! One file per article under `<state>/texts/`, written atomically. Keeping
//! bodies out of `state.json` means reading flags stay tiny and a text fetch
//! never contends with a feed refresh. File names are article IDs (hex), which
//! are already filesystem-safe.

use crate::util::{atomic_write, ensure_dir, state_dir};
use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;

fn dir() -> PathBuf {
    state_dir().join("texts")
}

fn file_for(id: &str) -> PathBuf {
    let safe: String = id
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
        .collect();
    dir().join(format!("{}.txt", safe))
}

pub fn get(id: &str) -> Option<String> {
    let text = fs::read_to_string(file_for(id)).ok()?;
    if text.trim().is_empty() {
        None
    } else {
        Some(text)
    }
}

pub fn put(id: &str, text: &str) -> Result<(), String> {
    if text.trim().is_empty() {
        return Err("empty article text".into());
    }
    ensure_dir(&dir())?;
    atomic_write(&file_for(id), text.as_bytes())
}

/// Drop cached bodies for articles that no longer exist.
pub fn retain(valid: &HashSet<String>) -> Result<(), String> {
    let Ok(entries) = fs::read_dir(dir()) else {
        return Ok(());
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
            if !valid.contains(stem) {
                let _ = fs::remove_file(path);
            }
        }
    }
    Ok(())
}
