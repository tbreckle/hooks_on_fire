//! Location of device files and game files.
//!
//! Files are looked up in two layers, the first match wins:
//!
//! 1. **User** – `devices/` and `games/` in the config directory. Own and edited files.
//!    hof-blaze writes game files only here.
//! 2. **Shipped** – `devices/` and `games/` next to the executable (release package), or
//!    `data/devices/` and `data/games/` in the working directory (development, `cargo run`
//!    from the repository root). Never written; replaced by updates.
//!
//! A game file that hof-blaze changes is saved to the user layer, so it then overrides the
//! shipped version (copy on write).

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result};

use crate::paths;

/// Kind of data file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DataKind {
    Devices,
    Games,
}

impl DataKind {
    fn dir_name(self) -> &'static str {
        match self {
            DataKind::Devices => "devices",
            DataKind::Games => "games",
        }
    }
}

/// User layer folder for `kind` (may not exist yet).
pub fn user_dir(kind: DataKind) -> Result<PathBuf> {
    Ok(paths::config_dir()?.join(kind.dir_name()))
}

/// Shipped layer folders for `kind`: next to the executable, then `data/` in the working
/// directory (development).
pub fn shipped_dirs(kind: DataKind) -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(exe_dir) = paths::exe_dir() {
        dirs.push(exe_dir.join(kind.dir_name()));
    }
    dirs.push(PathBuf::from("data").join(kind.dir_name()));
    dirs
}

/// All folders searched for `kind`, highest priority first.
pub fn search_dirs(kind: DataKind) -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Ok(user) = user_dir(kind) {
        dirs.push(user);
    }
    dirs.extend(shipped_dirs(kind));
    dirs
}

/// Finds `<name>.yaml` of `kind`, user layer first.
pub fn find(kind: DataKind, name: &str) -> Option<PathBuf> {
    let file_name = format!("{name}.yaml");
    search_dirs(kind)
        .into_iter()
        .map(|dir| dir.join(&file_name))
        .find(|path| path.is_file())
}

/// All `*.yaml` files of `kind`, one per file name (user layer wins), sorted by name.
pub fn list(kind: DataKind) -> Vec<PathBuf> {
    let mut files: BTreeMap<String, PathBuf> = BTreeMap::new();
    for dir in search_dirs(kind) {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("yaml") {
                continue;
            }
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                files.entry(name.to_string()).or_insert(path);
            }
        }
    }
    files.into_values().collect()
}

/// Path of `<name>.yaml` in the user layer, for writing. Creates the folder if needed.
pub fn user_path(kind: DataKind, name: &str) -> Result<PathBuf> {
    let dir = user_dir(kind)?;
    fs::create_dir_all(&dir).with_context(|| format!("Failed to create {}", dir.display()))?;
    Ok(dir.join(format!("{name}.yaml")))
}

/// Human-readable list of the searched folders, for error messages.
pub fn describe_search_dirs(kind: DataKind) -> String {
    search_dirs(kind)
        .iter()
        .map(|dir| dir.display().to_string())
        .collect::<Vec<_>>()
        .join(", ")
}
