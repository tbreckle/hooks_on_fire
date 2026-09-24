//! Validates the shipped device and game files in `data/` with the real parsers, so broken
//! files are caught by `cargo test` (and CI) before they are released.

use std::path::{Path, PathBuf};

use crate::devices::{load_device_file, Device};
use crate::gamefile::Gamefile;

fn data_files(kind: &str) -> Vec<PathBuf> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../data")
        .join(kind);
    let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap_or_else(|err| panic!("Cannot read {}: {err}", dir.display()))
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().and_then(|e| e.to_str()) == Some("yaml"))
        .collect();
    files.sort();
    files
}

fn file_stem(path: &Path) -> &str {
    path.file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or_default()
}

fn load_devices() -> (Vec<Device>, Vec<String>) {
    let mut devices = Vec::new();
    let mut problems = Vec::new();
    for path in data_files("devices") {
        match load_device_file(&path) {
            Ok(device) if device.name() != file_stem(&path) => problems.push(format!(
                "{}: name '{}' does not match the file name",
                path.display(),
                device.name()
            )),
            Ok(device) => devices.push(device),
            Err(err) => problems.push(format!("{err:#}")),
        }
    }
    (devices, problems)
}

#[test]
fn shipped_device_files_are_valid() {
    let (devices, problems) = load_devices();
    assert!(
        problems.is_empty(),
        "Invalid device files:\n{}",
        problems.join("\n")
    );
    assert!(!devices.is_empty(), "No device files found in data/devices");
}

#[test]
fn shipped_game_files_are_valid() {
    let (devices, _) = load_devices();
    let mut problems = Vec::new();

    for path in data_files("games") {
        let config = match Gamefile::parse_file(&path) {
            Ok(config) => config,
            Err(err) => {
                problems.push(format!("{}: {err:#}", path.display()));
                continue;
            }
        };
        // Every command must be an action of at least one shipped device.
        for signal in &config.signals {
            for command in &signal.commands {
                if !devices.iter().any(|d| d.action(command).is_some()) {
                    problems.push(format!(
                        "{}: signal '{}' uses command '{}', which no device file defines",
                        path.display(),
                        signal.signal,
                        command
                    ));
                }
            }
        }
    }

    assert!(
        problems.is_empty(),
        "Invalid game files:\n{}",
        problems.join("\n")
    );
}
