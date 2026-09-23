# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Commands

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

hof-blaze logs to the console and to `logs/hof-blaze.log` next to the executable (fallback: `logs/` in the config directory, see `hof_common::paths::log_dirs`). The file is truncated on every start and written via a non-blocking `tracing-appender` worker, so lines reach the file immediately. Panics are logged too. On Windows hof-blaze is built with `windows_subsystem = "windows"` (no console), so the log file is the only output there.

Log level conventions: `info`/`debug` show what is happening; anything that indicates a problem uses `warn`/`error`.

## Documentation

- `README.md` – project overview, logo (`docs/logo.png`, a copy of `assets/logo.png`).
- `docs/user/` – user documentation (installation, network output setup, hof-forge, hof-blaze, game files, device files, troubleshooting).
- `docs/developer/`, `docs/architecture/` – not written yet (placeholders).

Keep `docs/user/` in sync when user-visible behavior, file formats, shortcuts or error messages change. In user-facing text, the data source is "MAME-compatible network output" (sent by MAME, Supermodel, TeknoParrot with OutputBlaster, …), not only MAME. The protocol keys `mame_start`/`mame_stop` keep their names.

## Architecture

The project is a Cargo workspace (`crates/*`) with three crates:

- **`hof-common`** - shared types: `HofConfig` (`config.rs`, incl. `MAX_PLAYERS`), events, paths (config dir, log dirs, lock files), instance locking, build info, and `usb` (USB serial port enumeration, `UsbId`, resolving USB id + serial number to the current port path). Config is stored at `~/.config/hooks-on-fire/hof-config.yaml` (Linux), `~/Library/Application Support/hooks-on-fire/` (macOS), `%APPDATA%\hooks-on-fire\` (Windows).
- **`hof-blaze`** - the main background daemon (system tray app). Receives MAME-compatible network output (from MAME, Supermodel, TeknoParrot/OutputBlaster, …) via TCP and UDP, processes it, and dispatches hardware commands.
- **`hof-forge`** - a GUI config editor (Slint, UI files in `crates/hof-forge/ui/`) for editing `HofConfig`. Mutually exclusive with `hof-blaze` via instance lock.

### hof-blaze startup and shutdown

`main.rs` is split into two phases:

1. `Blaze::start()` – everything that can fail on start: instance lock, config, device registry (incl. USB port resolution and `max-instances` check), opening serial ports, device `setup` commands, TCP/UDP receivers, engine, action router. An error here is logged and shown in a message box (`rfd`); hof-blaze exits after OK. If it fails after `setup` was sent, the device `teardown` commands are sent first.
2. `Blaze::run()` – starts the tray, waits for Exit, then shuts down: receivers → engine (fires `___teardown` and `leave_game`, logs data statistics) → drain action router → device `teardown` → tray.

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

1. `tcp_connector` and `udp_receiver` receive newline-delimited `key=value` messages and send them as `LineEvent::NewLine` on a Tokio `mpsc` channel. While nothing is listening on the TCP port (connection refused), the connector retries every second but only logs "Waiting for connection" every 10 seconds.
2. `line_processor` parses lines: special keys (`mame_start`/`game`, `mame_stop`, `pause`) become `StateEvent`; everything else becomes `GameEvent::Data`. `mame_start=___empty` is ignored.
3. `engine` holds the active `GameConfig` (loaded from a per-game YAML file, e.g. `lostwsga.yaml`) and the current game name. On `StateEvent::NewGame` it ends the previous game, loads/creates the game file and starts the new one. On `GameEvent::Data` it matches against configured signals and emits `GameEvent::Action` events. It also collects data event statistics (`data_stats.rs`) that are logged on shutdown.
4. `action_router` dispatches `GameEvent::Action` to light-controller or light-gun handlers, and `GameEvent::DeviceAction` (`enter_game`/`leave_game`) to every device that has the action.

Game start/end order: `___teardown` (old game) → `leave_game` (old game name) → `enter_game` (new game name) → `___startup` (new game). A game ends on `mame_stop`, on a new game, and on quit.

### Game configuration files

Per-game YAML files (e.g. `lostwsga.yaml`, `dayto2pe.yaml`) live next to the binary (read from the working directory). The file name is the game name received with `mame_start`/`game`. Schema:
```yaml
players:
  count: 2             # 1..=MAX_PLAYERS (4)
signals:
  - signal: ___startup   # fixed signal, fired when the game starts
    commands:
      - enable_autofire
  - signal: ___teardown  # fixed signal, fired when the game ends
    commands: []
  - signal: P1_CtmRecoil
    player: 1          # 1..=players.count or ___all (default ___all)
    commands:
      - recoil          # any string - must match an action defined in a device YAML
      - recoil_value
```

If a game file is missing, the engine creates a default stub (incl. the fixed signals). Missing fixed signals are inserted into existing game files on load. Unknown signals received at runtime are appended to the file automatically.

### Device configuration files

Device YAML files (e.g. `blast.yaml`, `openfire.yaml`) live next to the binary (read from the working directory; hof-forge scans the working directory for them). They define hardware devices that receive commands from the action router. A YAML file is recognized as a device file if its root element is `device:`. Schema:
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

Slint UI (`ui/main.slint`, `devices_tab.slint`, `settings_tab.slint`, `overview_tab.slint`), logic in `src/main.rs`, port dropdown in `src/ports.rs` (built on `hof_common::usb`; entries store the USB id, labels show the current port path). Lists are sorted alphabetically for display only (`instance_order` maps rows to `config.devices`; the order in `hof-config.yaml` is kept). The UI is fully keyboard-operable: key events only reach ancestors of the focused element, so focus is explicitly restored after dialogs close and tabs change. The Overview tab can open the hof-blaze log file with the OS default program (`open` crate).

### Instance locking

`hof-blaze` and `hof-forge` are mutually exclusive: each acquires a lock and blocks the other from starting (lock files in the system temp directory, e.g. `/tmp`).

### Build-time metadata

`hof-blaze/build.rs` and `hof-forge/build.rs` inject `HOF_GIT_HASH` and `HOF_BUILD_DATE` env vars from git at compile time. hof-forge shows them in its window title, hof-blaze in its tray menu and log.
