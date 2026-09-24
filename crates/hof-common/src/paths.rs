use std::path::PathBuf;

use anyhow::{Context, Result};

/// Returns the configuration directory for Hooks on Fire.
///
/// - Linux: `~/.config/hooks-on-fire/`
/// - macOS: `~/Library/Application Support/hooks-on-fire/`
/// - Windows: `%APPDATA%\hooks-on-fire\`
pub fn config_dir() -> Result<PathBuf> {
    let base = dirs::config_dir().context("Could not determine config directory")?;
    Ok(base.join("hooks-on-fire"))
}

/// Returns the full path to the config file.
pub fn config_file() -> Result<PathBuf> {
    Ok(config_dir()?.join("hof-config.yaml"))
}

/// Folder containing the running executable.
pub fn exe_dir() -> Option<PathBuf> {
    std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|dir| dir.to_path_buf()))
}

/// Path of the executable `name` (e.g. `hof-forge`) in the folder of the running executable,
/// if it exists there.
pub fn sibling_exe(name: &str) -> Result<PathBuf> {
    let dir = exe_dir().context("Could not determine the program folder")?;
    let path = dir.join(format!("{name}{}", std::env::consts::EXE_SUFFIX));
    if !path.is_file() {
        anyhow::bail!("{name} not found: {}", path.display());
    }
    Ok(path)
}

/// File name of the hof-blaze log file.
pub const BLAZE_LOG_FILE: &str = "hof-blaze.log";

/// Folders a log file may be written to, in order of preference:
/// `logs` next to the executable, then `logs` in the config directory
/// (fallback if the executable's folder is not writable, e.g. Program Files).
pub fn log_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(exe_dir) = exe_dir() {
        dirs.push(exe_dir.join("logs"));
    }
    if let Ok(config_dir) = config_dir() {
        dirs.push(config_dir.join("logs"));
    }
    dirs
}

/// Returns the path for a lock file in the system temp directory.
pub fn lock_file(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("{name}.lock"))
}
