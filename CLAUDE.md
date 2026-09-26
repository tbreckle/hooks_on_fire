# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Commands

The Rust version is pinned in `rust-toolchain.toml` (rustup installs it automatically; CI installs it with `rustup toolchain install`). To upgrade Rust, change `channel` there (and the Rust badge in `README.md`) and fix any new clippy lints.

### Build
```bash
cargo build --workspace
```

### Run
```bash
cargo run -p hof-blaze    # background daemon
cargo run -p hof-forge    # config GUI
```

### Test
```bash
cargo test --workspace
cargo test -p hof-common  # single crate
```

### Scripts
```bash
scripts/test.sh               # tests for version.sh and changelog.sh
shellcheck scripts/*.sh       # CI check
scripts/version.sh            # version of the current checkout
```

### Lint & Format
```bash
cargo fmt --all
cargo fmt --all -- --check    # CI check
cargo clippy --workspace -- -D warnings
```

### Linux system dependencies (required to build)
```bash
sudo apt-get install -y libgtk-3-dev libxdo-dev libayatana-appindicator3-dev libudev-dev
```

### Logging
Set `RUST_LOG=debug` (or `trace`, `info`) to control log verbosity. Default is `info`.

hof-blaze logs to the console and to `logs/hof-blaze.log` next to the executable (fallback: `logs/` in the config directory, see `hof_common::paths::log_dirs`). The file is truncated on every start and written via a non-blocking `tracing-appender` worker, so lines reach the file immediately. Panics are logged too. The tray menu entry "Open log file" opens the file written by this process (`logging::log_file()`) with the OS default program (`open` crate). On Windows hof-blaze is built with `windows_subsystem = "windows"` (no console), so the log file is the only output there.

Log level conventions: `info`/`debug` show what is happening; anything that indicates a problem uses `warn`/`error`.

## Documentation

- `README.md` – project overview, logo (`docs/logo.png`, a copy of `assets/logo.png`).
- `docs/user/` – user documentation (installation, network output setup, hof-forge, hof-blaze, game files, device files, troubleshooting).
- `CHANGELOG.md` – Keep a Changelog format. Add user-visible changes under `[Unreleased]`; Release Start turns it into the `[X.Y.Z]` section, which becomes the GitHub Release notes.
- `docs/developer/gitflow.md` – branches, releases, hotfixes, versioning. `.github/WORKFLOWS.md` – CI/CD workflows. The rest of `docs/developer/` and `docs/architecture/` is not written yet.

Keep `docs/user/` in sync when user-visible behavior, file formats, shortcuts or error messages change. In user-facing text, the data source is "MAME-compatible network output" (sent by MAME, Supermodel, TeknoParrot with OutputBlaster, …), not only MAME. The protocol keys `mame_start`/`mame_stop` keep their names.

## Architecture

The project is a Cargo workspace (`crates/*`) with three crates:

- **`hof-common`** - shared types: `HofConfig` (`config.rs`, incl. `MAX_PLAYERS`), events, paths (config dir, log dirs, lock files), instance locking, build info, `data_files` (layered lookup of device/game files, see below), and `usb` (USB serial port enumeration, `UsbId`, resolving USB id + serial number to the current port path). Config is stored at `~/.config/hooks-on-fire/hof-config.yaml` (Linux), `~/Library/Application Support/hooks-on-fire/` (macOS), `%APPDATA%\hooks-on-fire\` (Windows).
- **`hof-blaze`** - the main background daemon (system tray app). Receives MAME-compatible network output (from MAME, Supermodel, TeknoParrot/OutputBlaster, …) via TCP and UDP, processes it, and dispatches hardware commands.
- **`hof-forge`** - a GUI config editor (Slint, UI files in `crates/hof-forge/ui/`) for editing `HofConfig`. Mutually exclusive with `hof-blaze` via instance lock.

### hof-blaze startup and shutdown

`main.rs` builds the Tokio runtime by hand: the main thread is reserved for the UI (startup message box, tray), because macOS only allows UI on the main thread and winit refuses to create its event loop elsewhere on Windows. All other work runs on the runtime's worker threads. There are three phases:

1. `Blaze::start()` (on the runtime) – everything that can fail on start: instance lock, config, device registry (incl. USB port resolution and `max-instances` check), opening serial ports, device `setup` commands, TCP/UDP receivers, engine, action router. An error here is logged and shown in a message box (`rfd`); hof-blaze exits after OK. If it fails after `setup` was sent, the device `teardown` commands are sent first.
2. `tray::run()` (on the main thread) – shows the tray and runs its event loop (GTK on Linux, winit on Windows/macOS) until Exit is chosen in the tray menu. `TrayEvent`s are received by a task on the runtime and passed to the event loop.
3. `Blaze::shutdown()` (on the runtime) – receivers → engine (fires `___teardown` and `leave_game`, logs data statistics) → drain action router → device `teardown`.

The tray is started only after the startup phase because on Linux both the tray and the `rfd` message box use GTK, which must not be driven from two threads.

### hof-blaze data flow

```
TCP (tcp_connector) ──┐
                      ├──► LineEvent ──► line_processor ──► StateEvent ──► engine
UDP (udp_receiver) ───┘                                └──► GameEvent  ──► engine
                                                                              │
                                                                              └──► action_rx ──► action_router
                                                                                     (lightgun / lightcontroller / all devices)
```

1. `tcp_connector` and `udp_receiver` receive newline-delimited `key=value` messages and send them as `LineEvent::NewLine` on a Tokio `mpsc` channel. While nothing is listening on the TCP port (connection refused), the connector retries every second but only logs "Waiting for connection" every 10 seconds. When an established connection ends (closed, reset or aborted by the source), the connector sends `LineEvent::Disconnected` and reconnects; a reset/abort is a normal disconnect, not the faulty tray state.
2. `line_processor` parses lines: special keys (`mame_start`/`game`, `mame_stop`, `pause`) become `StateEvent`; everything else becomes `GameEvent::Data`. `mame_start=___empty` is ignored. `LineEvent::Disconnected` becomes `StateEvent::GameStopped` if a game is running (TeknoParrot/OutputBlaster quits without `mame_stop`).
3. `engine` holds the active `GameConfig` (loaded from a per-game YAML file, e.g. `lostwsga.yaml`) and the current game name. On `StateEvent::NewGame` it ends the previous game, loads/creates the game file and starts the new one. On `GameEvent::Data` it matches against configured signals and emits `GameEvent::Action` events. It also collects data event statistics (`data_stats.rs`) that are logged on shutdown.
   After every `StateEvent` the engine sends `TrayEvent::GameStarted` (game name + the game file's `display-name`) or `TrayEvent::GameEnded` to the tray, which shows the game behind the status (`Status: Healthy (The Lost World: Jurassic Park - lostwsga)`, only the game name without `display-name`).
4. `action_router` dispatches `GameEvent::Action` to light-controller or light-gun handlers, and `GameEvent::DeviceAction` (`enter_game`/`leave_game`) to every device that has the action.

Game start/end order: repeats stopped → `___teardown` (old game) → `leave_game` (old game name) → `enter_game` (new game name) → `___startup` (new game). A game ends on `mame_stop`, on TCP disconnect, on a new game, and on quit.

### Data files (device and game files)

Shipped device and game files live in the repository under `data/devices/` and `data/games/`. `hof_common::data_files` looks them up in two layers, first match wins:

1. **User layer** – `devices/` and `games/` in the config directory (next to `hof-config.yaml`). Own and edited files.
2. **Shipped layer** – `devices/` and `games/` next to the executable (release package), then `data/devices/` and `data/games/` in the working directory (development: `cargo run` from the repository root).

The shipped layer is never written. hof-blaze saves game files only to the user layer (`data_files::user_path`), so a changed shipped game file is copied to the user layer on its first save and overrides the shipped one from then on (copy on write). A game file that exists but fails to parse is loaded as a default config with `read_only = true` and is never saved over. When developing, note that game files changed by hof-blaze end up in the user layer, not in `data/games/`; copy them back to update the shipped files.

The archives built by `build.yml` (CI and releases) contain `devices/` and `games/` (copied from `data/`), `README.md`, `LICENSE` and `LICENSES/` next to the binaries. `crates/hof-blaze/src/data_check.rs` (test only) validates all files in `data/` with the real parsers: device files parse and their `name` matches the file name, game files parse and have a `display-name`, and every command used in a game file is an action of some shipped device file.

### Game configuration files

Per-game YAML files (e.g. `lostwsga.yaml`, `dayto2pe.yaml`, see data files above). The file name is the game name received with `mame_start`/`game`. Schema:
```yaml
display-name: "The Lost World: Jurassic Park"  # optional: title shown in the tray (required for shipped files)
players:
  count: 2             # 1..=MAX_PLAYERS (4)
suppression:           # optional: device types (lightgun | lightcontroller) or device names (e.g. openfire)
  - lightgun           #   that get nothing for this game, not even enter_game/leave_game
signals:
  - signal: ___startup   # fixed signal, fired when the game starts
    commands:
      - enable_autofire
  - signal: ___teardown  # fixed signal, fired when the game ends
    commands: []
  - signal: P1_CtmRecoil
    player: 1          # 1..=players.count or ___all (default ___all)
    repeat: 100        # optional: ms (10..=10000); while held (value not 0) repeat the commands
    commands:
      - recoil          # any string - must match an action defined in a device YAML
      - recoil_value
```

Suppression is carried in `GameEvent::Action`/`DeviceAction` (`suppression`); the `action_router` treats suppressed devices as not configured (`DeviceRegistry::active_by_type`, `light_guns_for_player`, `has_action_for_type` take the list). The engine loads the game file before sending `enter_game`, so suppressed devices do not get it. Device-level `setup`/`teardown` are not affected.

Signals with `repeat` (for games sending `1` while the trigger is held and `0` on release): on a value other than `0`/empty the engine sends the commands and `repeater.rs` schedules them every `repeat` ms (per signal entry index, drift-free, missed repeats skipped); further non-zero values only update `{VALUE}`, `0` stops without sending, game end clears all repeats, pause is ignored. The engine loop has a `select!` branch sleeping until the next due repeat. Repeated sends are logged at debug level. Signals without `repeat` send on every value, including `0`.

If a game file is missing, the engine creates a default stub (incl. the fixed signals) in the user layer. Missing fixed signals are inserted into existing game files on load. Unknown signals received at runtime are appended to the file automatically (saved to the user layer).

### Device configuration files

Device YAML files (e.g. `blast.yaml`, `openfire.yaml`, see data files above; hof-blaze finds them with `data_files::find`, hof-forge lists them with `data_files::list`). They define hardware devices that receive commands from the action router. A YAML file is recognized as a device file if its root element is `device:`. Schema:
```yaml
device:
  type: lightcontroller       # required: lightgun | lightcontroller
  name: blast                 # required: unique identifier, must match the file name
  display-name: B.L.A.S.T.   # required: human-readable label
  connection-type: serial     # required: currently only "serial"
  connection-details:         # required
    speed: 115200             #   baud rate for the serial connection
  command-delay: 1            # optional: ms between commands (default 1)
  max-instances: 1            # optional: how often the device may be added to hof-config.yaml (default unlimited)
  # Additional keys are device actions (name → list of command strings):
  setup:                      # sent on application start
    - B1
  teardown:                   # sent on application shutdown
    - B2
  enter_game:                 # optional: sent to all devices having it when a game starts
    - B8x{GAMENAME}
  leave_game:                 # optional: sent to all devices having it when a game ends (game stop, new game, quit)
    - B8x0
  lamp_start:
    - B3x{PLAYER}x{VALUE}
```

Device action command strings support template placeholders which are substituted at runtime: `{PLAYER}` (player of the signal, `0` for `___all` and for `enter_game`/`leave_game`), `{VALUE}` (value of the received data event, empty for `___startup`/`___teardown`/`enter_game`/`leave_game`) and `{GAMENAME}` (name of the running game as received with `mame_start`/`game`, e.g. `lostwsga`). Placeholders are not substituted in the device-level `setup`/`teardown` commands.

Devices to load are listed in the `devices` field of `hof-config.yaml`. Each entry has `name` (maps to `{name}.yaml`), a unique `instance-name`, `connection-details` and an optional `player` (1-4, light guns only). Example:
```yaml
devices:
  - name: blast
    instance-name: B.L.A.S.T.
    connection-details:
      type: serial
      usb-id: f144:0001
      usb-serial: E6642815E3631E2A
  - name: openfire
    instance-name: OpenFire P1
    player: 1
    connection-details:
      type: serial
      usb-id: f143:0001
      usb-serial: E172B122FF5EA574
```
The `name` field inside the device YAML must match the config entry. The same device file can be used by several instances (e.g. two OpenFire guns), limited by `max-instances` (enforced by hof-forge when adding and by hof-blaze on start). `DeviceRegistry::load()` reads these files at startup and provides lookup by id (index), name, type and player. Log messages identify devices by `instance_name()`.

### Serial connections

Each device's connection is configured in `hof-config.yaml` via `connection-details` with `type: serial` (not in the device YAML file). USB devices are stored by `usb-id` (`vid:pid` in hex) plus `usb-serial` (the USB serial number, if the device has one) instead of the port path, so re-enumeration (`/dev/ttyACM1` → `/dev/ttyACM3`, `COM3` → `COM5`) needs no reconfiguration. On start, `DeviceRegistry::load()` looks up the current port path via `hof_common::usb`; a device that is not connected, or connected more than once without a distinguishing serial number, is a startup error. A fixed `port` path is still accepted if no `usb-id` is set (older configs, non-USB ports); hof-forge converts it to the USB id when the device is saved. The baud rate comes from the device YAML's `connection-details.speed`. `SerialManager` (in `serial.rs`) opens one connection per unique port path at startup; devices sharing the same port reuse the connection (must use the same speed). If a port cannot be opened, startup fails with a message box (see startup phase above).

On start, `setup` commands are sent to each device. On shutdown, `teardown` commands are sent. During runtime the `action_router` sends resolved action commands over each device's serial port. A configurable delay (`command-delay`, default 1ms) is applied between commands per device.

Commands in game config signals are free-form strings (not a fixed enum). The `action_router` in `main.rs` classifies each action dynamically by checking which device type has it configured in the registry, then dispatches accordingly. If any light controller has the action, it goes to light controllers only (light guns with the same action name do not get it); otherwise to light guns:
- Light controllers get every action; the signal's player only fills `{PLAYER}`.
- Light guns are routed by player (`DeviceRegistry::light_guns_for_player`): a signal for player N goes to the guns with `player: N`, or to the unassigned guns if no gun is assigned to N. `player: ___all` goes to all guns.
- For `player: ___all`, `{PLAYER}` is substituted with `0`.

Game files support up to 4 players (`MAX_PLAYERS` in `hof-common`).

### hof-forge

Slint UI (`ui/main.slint`, `devices_tab.slint`, `settings_tab.slint`, `overview_tab.slint`), logic in `src/main.rs`, port dropdown in `src/ports.rs` (built on `hof_common::usb`; entries store the USB id, labels show the current port path). Lists are sorted alphabetically for display only (`instance_order` maps rows to `config.devices`; the order in `hof-config.yaml` is kept). The UI is fully keyboard-operable: key events only reach ancestors of the focused element, so focus is explicitly restored after dialogs close and tabs change. hof-forge uses Slint under the Slint Royalty-free License 2.0 (`LICENSES/LicenseRef-Slint-Royalty-free-2.0.md`), which requires attribution: keep the `AboutSlint` widget on the Overview tab and the #MadeWithSlint badge in `README.md`. The Overview tab can open the hof-blaze log file with the OS default program and the user layer's game files folder in the file manager (`open` crate; `Ctrl+L` / `Ctrl+G`). On Windows hof-forge is built with `windows_subsystem = "windows"` (no console); errors returned from `run()` (e.g. hof-blaze is running, config fails to parse) and failed config saves (`report_save_error`, deferred with a zero timer so no `AppState` borrow is held while the box is open) are shown in an `rfd` message box. Icons are SVGs in `ui/icons/` (Button `icon` + `colorize-icon`), not Unicode glyphs, which Windows fonts may lack.

### Instance locking

`hof-blaze` and `hof-forge` are mutually exclusive: each acquires a lock and blocks the other from starting (lock files in the system temp directory, e.g. `/tmp`). With the command line flag `--wait-for-lock` (`instance_lock::lock_wait_from_args`), a tool waits up to 3 s for the other's lock, checking every 50 ms.

Switching: the hof-blaze tray entry "Open HoF-forge" and the hof-forge Overview button "Switch to hof-blaze" (`Ctrl+B`, saves the Settings tab first) start the other tool from the same folder (`paths::sibling_exe`, `switch::launch`, which passes `--wait-for-lock`). The other tool is started only after the own shutdown is complete and the lock is released: hof-blaze after `Blaze::shutdown()` (`tray::run` returns `TrayExit::SwitchToForge`), hof-forge after `window.run()` returns. In development both binaries must be built (`cargo build --workspace`).

### Build-time metadata

`hof-blaze/build.rs` and `hof-forge/build.rs` inject `HOF_GIT_HASH` and `HOF_BUILD_DATE` env vars from git at compile time, and `HOF_VERSION` from the env var of the same name (set by CI from `scripts/version.sh`; falls back to the `Cargo.toml` version, which stays at `0.0.0`). hof-forge shows them in its window title, hof-blaze in its tray menu and log.

### CI and releases

GitFlow with GitHub Actions, taken over from B.L.A.S.T. (details in `.github/WORKFLOWS.md` and `docs/developer/gitflow.md`). No version is committed: `scripts/version.sh` computes it from branch and `vX.Y.Z` tags (`X.Y.Z` on tagged `main`, `X.Y.Z-rc.N` on `release/*`/`hotfix/*`, `0.0.0+<sha>` otherwise). `ci.yml` runs the reusable `build.yml` (per OS: clippy, tests, release build and archive; plus `cargo fmt` and shellcheck/`scripts/test.sh`). `release-start.yml` (manual) creates `release/X.Y.Z` or `hotfix/X.Y.Z`, updates CHANGELOG.md (`scripts/changelog.sh`) and opens a PR to `main`; merging it runs `release-finish.yml` (build, tag + GitHub Release, back-merge PR into `develop`). Release and back-merge PRs must be merged with a merge commit. Required status checks are listed in `.github/branch-protection.conf`; keep it in sync when job names change.
