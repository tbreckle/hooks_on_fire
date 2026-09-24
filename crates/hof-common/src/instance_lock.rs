use std::fs::{File, OpenOptions};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use anyhow::{bail, Context, Result};
use fs2::FileExt;
use tracing::info;

use crate::paths;

/// Command line flag of hof-blaze and hof-forge: wait up to `LOCK_WAIT` for the other tool
/// to release its lock (used when switching from one tool to the other).
pub const WAIT_FOR_LOCK_ARG: &str = "--wait-for-lock";
/// How long to wait for the other tool's lock with `--wait-for-lock`.
pub const LOCK_WAIT: Duration = Duration::from_secs(3);
/// How often the other tool's lock is checked while waiting.
const LOCK_POLL_INTERVAL: Duration = Duration::from_millis(50);

/// Returns how long to wait for the other tool's lock: `LOCK_WAIT` if the program was started
/// with `--wait-for-lock`, otherwise zero.
pub fn lock_wait_from_args() -> Duration {
    if std::env::args().skip(1).any(|arg| arg == WAIT_FOR_LOCK_ARG) {
        LOCK_WAIT
    } else {
        Duration::ZERO
    }
}

/// Holds an advisory lock on a file. The lock is released when this struct is dropped.
pub struct InstanceLock {
    _file: File,
    _path: PathBuf,
}

impl InstanceLock {
    /// Acquires an exclusive lock for `own_name` after verifying `other_name` is not running.
    ///
    /// - Checks the other program's lock file first. While it is locked, checks again every
    ///   50 ms for up to `wait` (`Duration::ZERO`: check once), then aborts.
    /// - Then acquires own lock; if already locked, another instance is running.
    pub fn acquire(own_name: &str, other_name: &str, wait: Duration) -> Result<Self> {
        // Check that the other program is not running
        let other_path = paths::lock_file(other_name);
        let start = Instant::now();
        let mut waiting = false;
        while is_locked(&other_path, other_name)? {
            if start.elapsed() >= wait {
                bail!(
                    "{other_name} is currently running. \
                     Please close it before starting {own_name}."
                );
            }
            if !waiting {
                info!("Waiting up to {wait:?} for {other_name} to exit...");
                waiting = true;
            }
            std::thread::sleep(LOCK_POLL_INTERVAL);
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

/// Returns true if another process holds the lock on `path`.
fn is_locked(path: &Path, name: &str) -> Result<bool> {
    if !path.exists() {
        return Ok(false);
    }
    let file = OpenOptions::new()
        .read(true)
        .open(path)
        .with_context(|| format!("Failed to open lock file for {name}: {}", path.display()))?;
    if file.try_lock_exclusive().is_err() {
        return Ok(true);
    }
    // If we got the lock, the program isn't running. Release it immediately.
    let _ = file.unlock();
    Ok(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Lock names unique to this test run, so tests do not see each other's (or real) locks.
    fn names(test: &str) -> (String, String) {
        let id = std::process::id();
        (
            format!("hof-test-{id}-{test}-own"),
            format!("hof-test-{id}-{test}-other"),
        )
    }

    fn remove_lock_files(names: &[&str]) {
        for name in names {
            let _ = std::fs::remove_file(paths::lock_file(name));
        }
    }

    #[test]
    fn fails_at_once_without_wait() {
        let (own, other) = names("no-wait");
        let _other_lock = InstanceLock::acquire(&other, &own, Duration::ZERO).unwrap();
        let start = Instant::now();
        let err = InstanceLock::acquire(&own, &other, Duration::ZERO)
            .err()
            .unwrap();
        assert!(err.to_string().contains("is currently running"));
        assert!(start.elapsed() < Duration::from_millis(40));
        remove_lock_files(&[&own, &other]);
    }

    #[test]
    fn times_out_while_other_keeps_running() {
        let (own, other) = names("timeout");
        let _other_lock = InstanceLock::acquire(&other, &own, Duration::ZERO).unwrap();
        let start = Instant::now();
        assert!(InstanceLock::acquire(&own, &other, Duration::from_millis(200)).is_err());
        assert!(start.elapsed() >= Duration::from_millis(200));
        remove_lock_files(&[&own, &other]);
    }

    #[test]
    fn waits_until_other_exits() {
        let (own, other) = names("wait");
        let other_lock = InstanceLock::acquire(&other, &own, Duration::ZERO).unwrap();
        let release = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(150));
            drop(other_lock);
        });
        let own_lock = InstanceLock::acquire(&own, &other, LOCK_WAIT);
        release.join().unwrap();
        assert!(own_lock.is_ok());
        drop(own_lock);
        remove_lock_files(&[&own, &other]);
    }
}
