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
Set `RUST_LOG=debug` (or `trace`, `info`) to control log verbosity.

## Architecture

The project is a Cargo workspace (`crates/*`) with three crates:

- **`hof-common`** - shared types: `HofConfig`, events, paths, instance locking, build info. Config is stored at `~/.config/hooks-on-fire/hof-config.yaml`.
- **`hof-blaze`** - the main background daemon (system tray app). Receives data from MAME via TCP and UDP, processes it, and dispatches hardware commands.
- **`hof-forge`** - a GUI config editor (egui/eframe) for editing `HofConfig`. Mutually exclusive with `hof-blaze` via instance lock.

### hof-blaze data flow

```
TCP (tcp_connector) ──┐
                      ├──► LineEvent ──► line_processor ──► StateEvent ──► engine
UDP (udp_receiver) ───┘                                └──► GameEvent  ──► engine
                                                                              │
                                                                              └──► action_rx ──► action_router
                                                                                     (lightgun / lightcontroller)
```

1. `tcp_connector` and `udp_receiver` receive newline-delimited `key=value` messages and send them as `LineEvent::NewLine` on a Tokio `mpsc` channel.
2. `line_processor` parses lines: special keys (`mame_start`/`game`, `mame_stop`, `pause`) become `StateEvent`; everything else becomes `GameEvent::Data`.
3. `engine` holds the active `GameConfig` (loaded from a per-game YAML file, e.g. `lightgun.yaml`). On `StateEvent::NewGame` it loads/creates the game file. On `GameEvent::Data` it matches against configured signals and emits `GameEvent::Action` events.
4. `action_router` dispatches actions to light-controller or light-gun handlers.

### Game configuration files

Per-game YAML files (e.g. `lightgun.yaml`, `dayto2pe.yaml`) live next to the binary. Schema:
```yaml
players:
  count: 2
signals:
  - signal: P1_CtmRecoil
    player: 1          # or ___all
    commands:
      - recoil          # any string - must match an action defined in a device YAML
      - recoil_value
```

If a game file is missing, the engine creates a default stub. Unknown signals received at runtime are appended to the file automatically.

### Device configuration files

Device YAML files (e.g. `blast.yaml`) live next to the binary. They define hardware devices that receive commands from the action router. A YAML file is recognized as a device file if its root element is `device:`. Schema:
```yaml
device:
  type: lightcontroller       # required: lightgun | lightcontroller
  name: blast                 # required: unique identifier
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

Device action command strings support template placeholders which are substituted at runtime: `{PLAYER}` (player of the signal, `0` for `___all`), `{VALUE}` (value of the received data event, empty for `___startup`/`___teardown`) and `{GAMENAME}` (name of the running game as received with `mame_start`/`game`, e.g. `lostwsga`). Placeholders are not substituted in the device-level `setup`/`teardown` commands.

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
The `name` field inside the device YAML must match the config entry. The same device file can be used by several instances (e.g. two OpenFire guns). `DeviceRegistry::load()` reads these files at startup and provides lookup by id (index), name, and type.

### Serial connections

Each device's connection is configured in `hof-config.yaml` via `connection-details` with `type: serial` (not in the device YAML file). USB devices are stored by `usb-id` (`vid:pid` in hex) plus `usb-serial` (the USB serial number, if the device has one) instead of the port path, so re-enumeration (`/dev/ttyACM1` → `/dev/ttyACM3`, `COM3` → `COM5`) needs no reconfiguration. On start, `DeviceRegistry::load()` looks up the current port path via `hof_common::usb`; a device that is not connected, or connected more than once without a distinguishing serial number, is a startup error. A fixed `port` path is still accepted if no `usb-id` is set (older configs, non-USB ports); hof-forge converts it to the USB id when the device is saved. The baud rate comes from the device YAML's `connection-details.speed`. `SerialManager` (in `serial.rs`) opens one connection per unique port path at startup; devices sharing the same port reuse the connection (must use the same speed). If a port does not exist or cannot be opened, a tray notification is shown and the application exits.

On start, `setup` commands are sent to each device. On shutdown, `teardown` commands are sent. During runtime the `action_router` sends resolved action commands over each device's serial port. A configurable delay (`command-delay`, default 1ms) is applied between commands per device.

Commands in game config signals are free-form strings (not a fixed enum). The `action_router` in `main.rs` classifies each action dynamically by checking which device type has it configured in the registry, then dispatches accordingly:
- Light controllers get every action; the signal's player only fills `{PLAYER}`.
- Light guns are routed by player (`DeviceRegistry::light_guns_for_player`): a signal for player N goes to the guns with `player: N`, or to the unassigned guns if no gun is assigned to N. `player: ___all` goes to all guns.
- For `player: ___all`, `{PLAYER}` is substituted with `0`.

Game files support up to 4 players (`MAX_PLAYERS` in `hof-common`).

### Instance locking

`hof-blaze` and `hof-forge` are mutually exclusive: each acquires a lock and blocks the other from starting (lock files in `/tmp`).

### Build-time metadata

`hof-blaze/build.rs` and `hof-forge/build.rs` inject `HOF_GIT_HASH` and `HOF_BUILD_DATE` env vars from git at compile time.
