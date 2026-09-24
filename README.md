<p align="center">
  <img src="docs/logo.png" alt="Hooks on Fire" width="600">
</p>

# Hooks on Fire

Hooks on Fire connects emulators to your light guns and arcade lighting. It listens to the
**MAME-compatible network output** that emulators send while a game is running (lamps,
recoil, …) and turns it into commands for serial devices such as OpenFIRE light guns or
light controllers — solenoid recoil, rumble, LEDs and cabinet lamps react to the game.

Supported sources include MAME, Supermodel and TeknoParrot with OutputBlaster, or any other
program that sends MAME-compatible network output.

It runs on Windows, Linux and macOS and consists of two programs:

- **hof-blaze** – runs in the background (system tray) while you play and drives your devices.
- **hof-forge** – configuration editor for your devices, serial ports, players and network settings.

## Made with Slint
<a href="https://slint.dev">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="https://slint.dev/logo/MadeWithSlint-logo-dark.svg">
    <img alt="#MadeWithSlint" src="https://slint.dev/logo/MadeWithSlint-logo-light.svg" height="60">
  </picture>
</a>

The user interface of hof-forge is made with the excellent [Slint UI toolkit](https://slint.dev).

## Features

- Per-game configuration: map any output signal to device commands, per player.
- Player-aware routing for up to 4 players: player 1's recoil only fires player 1's gun.
- USB devices are recognised by their USB id and serial number, so re-plugging a device
  (e.g. `COM3` → `COM5`) needs no reconfiguration.
- New games and unknown signals are added to the game files automatically, ready to edit.
- Commands on game start/end, player and game name placeholders in commands.
- Log file and error dialogs for easy troubleshooting.

## Quick start

1. Download the archive for your system from the
   [releases](../../releases) and unpack it into any folder. It includes device files for
   OpenFIRE light guns and the B.L.A.S.T. light controller, and game files for several games.
2. For other hardware, put its device file into your user folder (see the
   [installation guide](docs/user/installation.md#your-files)).
3. Start **hof-forge**, add your devices and select their USB ports.
4. Enable the MAME-compatible network output of your emulator (MAME: `-output network`).
5. Close hof-forge, start **hof-blaze** and play.

The [user documentation](docs/user/README.md) explains every step in detail.

## Documentation

- [User documentation](docs/user/README.md) – installation, setup and configuration
- [Developer documentation](docs/developer/README.md)
- [Architecture](docs/architecture/README.md)

## Building from source

Hooks on Fire is written in Rust. On Linux, install the build dependencies first:

```bash
sudo apt-get install -y libgtk-3-dev libxdo-dev libayatana-appindicator3-dev libudev-dev
```

The Rust version is pinned in `rust-toolchain.toml`; [rustup](https://rustup.rs) installs it
automatically. Then build both programs:

```bash
cargo build --workspace --release
```

## License

Hooks on Fire is released under the [MIT License](LICENSE).

hof-forge uses [Slint](https://slint.dev) under the
[Slint Royalty-free Desktop, Mobile, and Web Applications License 2.0](LICENSES/LicenseRef-Slint-Royalty-free-2.0.md).
