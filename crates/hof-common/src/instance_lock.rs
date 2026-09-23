use std::fs::{File, OpenOptions};
use std::path::PathBuf;

use anyhow::{bail, Context, Result};
use fs2::FileExt;
use tracing::info;

use crate::paths;

/// Holds an advisory lock on a file. The lock is released when this struct is dropped.
pub struct InstanceLock {
    _file: File,
    _path: PathBuf,
}

impl InstanceLock {
    /// Acquires an exclusive lock for `own_name` after verifying `other_name` is not running.
    ///
    /// - Checks the other program's lock file first; if locked, aborts.
    /// - Then acquires own lock; if already locked, another instance is running.
    pub fn acquire(own_name: &str, other_name: &str) -> Result<Self> {
        // Check that the other program is not running
        let other_path = paths::lock_file(other_name);
        if other_path.exists() {
            let other_file = OpenOptions::new()
                .read(true)
                .open(&other_path)
                .with_context(|| {
                    format!(
                        "Failed to open lock file for {other_name}: {}",
                        other_path.display()
                    )
                })?;
            if other_file.try_lock_exclusive().is_err() {
                bail!(
                    "{other_name} is currently running. \
                     Please close it before starting {own_name}."
                );
            }
            // If we got the lock, the other program isn't running. Release it immediately.
            let _ = other_file.unlock();
        }

        // Acquire our own lock
        let own_path = paths::lock_file(own_name);
        let own_file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&own_path)
            .with_context(|| {
                format!(
                    "Failed to create lock file for {own_name}: {}",
                    own_path.display()
                )
            })?;

        own_file.try_lock_exclusive().map_err(|_| {
            anyhow::anyhow!(
                "Another instance of {own_name} is already running. \
                 Only one instance is allowed at a time."
            )
        })?;

        info!("Acquired instance lock: {}", own_path.display());

        Ok(Self {
            _file: own_file,
            _path: own_path,
        })
    }
}
