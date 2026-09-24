use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use anyhow::{Context, Result};
use hof_common::paths;
use tracing::{error, info, warn};
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{fmt, EnvFilter};

/// Path of the log file written by this process (set by `init`).
static LOG_FILE: OnceLock<PathBuf> = OnceLock::new();

/// Returns the log file written by this process, or `None` if logging to a file is disabled.
pub fn log_file() -> Option<&'static Path> {
    LOG_FILE.get().map(PathBuf::as_path)
}

/// Sets up logging to the console and to `logs/hof-blaze.log`.
///
/// The log file is emptied on every start. Lines are handed to a background writer thread
/// that writes them to the file immediately (the file is unbuffered), so logging never blocks
/// the caller and the file is always up to date.
///
/// The returned guard must be kept alive until the application exits; dropping it writes
/// any remaining lines.
pub fn init() -> Option<WorkerGuard> {
    // RUST_LOG controls verbosity, default is info.
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let console_layer = fmt::layer();

    let (file_layer, guard, result) = match open_log_file() {
        Ok((file, path)) => {
            let (writer, guard) = tracing_appender::non_blocking(file);
            let layer = fmt::layer().with_ansi(false).with_writer(writer);
            (Some(layer), Some(guard), Ok(path))
        }
        Err(err) => (None, None, Err(err)),
    };

    tracing_subscriber::registry()
        .with(filter)
        .with(console_layer)
        .with(file_layer)
        .init();

    match result {
        Ok(path) => {
            info!("Logging to {}", path.display());
            let _ = LOG_FILE.set(path);
        }
        Err(err) => warn!("Logging to file disabled: {err:#}"),
    }

    log_panics();
    guard
}

/// Creates (or empties) the log file in the first writable folder of `paths::log_dirs()`.
fn open_log_file() -> Result<(File, PathBuf)> {
    let mut last_err = anyhow::anyhow!("No location for the log file found");
    for dir in paths::log_dirs() {
        let path = dir.join(paths::BLAZE_LOG_FILE);
        let opened = fs::create_dir_all(&dir)
            .and_then(|_| File::create(&path))
            .with_context(|| format!("Failed to create log file {}", path.display()));
        match opened {
            Ok(file) => return Ok((file, path)),
            Err(err) => last_err = err,
        }
    }
    Err(last_err)
}

/// Writes panics to the log as well; without a console (Windows) they would be lost otherwise.
fn log_panics() {
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic_info| {
        error!("Panic: {panic_info}");
        default_hook(panic_info);
    }));
}
