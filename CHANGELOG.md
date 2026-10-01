# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- hof-blaze: tray menu entry "Reload game file" reads the running game's file again without restarting the game

### Fixed
- hof-blaze: an error in the running game's file is shown in the tray (faulty icon, "error in game file") and the notification shows the error with line and column
- hof-blaze: startup errors (e.g. an error in a device file) are also shown as notification, in case the message box is hidden behind a fullscreen window
- hof-forge: device files with errors are reported in a message box instead of being left out silently

## [1.0.0] - 2026-09-26

### Added
- hof-blaze: background program (system tray) that receives MAME-compatible network output (MAME, Supermodel, TeknoParrot, …) via TCP and UDP and sends commands to serial devices
- hof-forge: configuration editor for devices, USB ports and settings, fully keyboard-operable
- Device files for OpenFIRE light guns and the B.L.A.S.T. light controller, game files for several games
- Player routing for light guns, repeating signal commands, per-game device suppression
- USB devices are identified by USB id and serial number, so re-enumerated ports need no reconfiguration
- Switching between hof-blaze and hof-forge from the tray menu and the Overview tab
- Log file and error dialogs for troubleshooting
- CI/CD pipeline with GitFlow releases: `release-start.yml` creates release/hotfix branches, `release-finish.yml` tags, builds and publishes GitHub Releases
- SemVer versioning via `scripts/version.sh`, injected into hof-blaze and hof-forge at build time (unofficial builds are `0.0.0+<sha>`)
- macOS universal binaries (Apple Silicon + Intel)

---

## Guide for Updating the Changelog

When making changes to the project:

1. Add your changes to the "Unreleased" section under the appropriate category
2. When a release starts, `release-start.yml` moves the "Unreleased" entries into a new
   `## [X.Y.Z] - date` section (`scripts/changelog.sh release`). That section becomes the
   GitHub Release notes (`scripts/changelog.sh notes`), empty categories are left out.

### Categories

- **Added**: New features
- **Changed**: Changes in existing functionality
- **Fixed**: Bug fixes
- **Deprecated**: Soon-to-be removed features
- **Removed**: Removed features
- **Security**: Security fixes

### Examples

```markdown
### Added
- Game file for Point Blank (#42)
- Keyboard shortcut Ctrl+S for saving the settings

### Fixed
- Serial connection timeout on Windows (#38)

### Changed
- Tray status shows the display name of the running game
```
