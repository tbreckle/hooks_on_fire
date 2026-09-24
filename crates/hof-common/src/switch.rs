//! Switching between hof-blaze and hof-forge.

use std::path::Path;
use std::process::{Command, Stdio};

use anyhow::{Context, Result};
use tracing::info;

use crate::instance_lock::WAIT_FOR_LOCK_ARG;

/// Starts the other tool at `path` (see `paths::sibling_exe`) with `--wait-for-lock`, so it
/// waits for the calling tool to exit and release its lock.
pub fn launch(path: &Path) -> Result<()> {
    info!("Starting {}", path.display());
    Command::new(path)
        .arg(WAIT_FOR_LOCK_ARG)
        .stdin(Stdio::null())
        .spawn()
        .with_context(|| format!("Could not start {}", path.display()))?;
    Ok(())
}
