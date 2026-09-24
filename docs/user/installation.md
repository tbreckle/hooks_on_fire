# Installation

## Download

Download the archive for your system from the GitHub releases page:

| System | Archive |
|---|---|
| Windows (64-bit) | `hooks-on-fire-x86_64-pc-windows-msvc.zip` |
| Linux (64-bit) | `hooks-on-fire-x86_64-unknown-linux-gnu.tar.gz` |
| macOS (Apple Silicon) | `hooks-on-fire-aarch64-apple-darwin.tar.gz` |
| macOS (Intel) | `hooks-on-fire-x86_64-apple-darwin.tar.gz` |

Unpack it into any folder, for example `C:\Games\HooksOnFire` or `~/HooksOnFire`.

## Folder layout

The archive contains the two programs and the shipped [device files](device-files.md) and
[game files](game-files.md):

```
HooksOnFire/
├── hof-blaze(.exe)      background program used while playing
├── hof-forge(.exe)      configuration editor
├── devices/             shipped device files
│   ├── openfire.yaml    OpenFIRE light guns
│   └── blast.yaml       B.L.A.S.T. light controller
├── games/               shipped game files
│   └── lostwsga.yaml    …
└── logs/
    └── hof-blaze.log    log file (created automatically)
```

Do not change the files in `devices/` and `games/`: they are replaced when you install a
new version. Your own and changed files go into your user folder (see below).

## Your files

Everything you configure is stored in your user folder, separate from the program:

| System | User folder |
|---|---|
| Windows | `%APPDATA%\hooks-on-fire\` |
| Linux | `~/.config/hooks-on-fire/` |
| macOS | `~/Library/Application Support/hooks-on-fire/` |

```
hooks-on-fire/
├── hof-config.yaml      your devices and network settings (edited with hof-forge)
├── devices/             your own device files
└── games/               your game files
```

- **Game files:** when hof-blaze changes a game file (a new game, a new signal), it saves
  it to `games/` in your user folder. From then on, your copy is used instead of the
  shipped one. Open the folder with *Open game files folder* in
  [hof-forge](hof-forge.md#overview-tab).
- **Device files:** put device files for other hardware into `devices/` in your user folder.
  A file there with the same name as a shipped one replaces it.

Updating Hooks on Fire never touches your user folder.

## System-specific notes

### Linux

Hooks on Fire needs GTK 3, the Ayatana app indicator library (tray icon), libudev and
libxdo. On Debian/Ubuntu:

```bash
sudo apt-get install -y libgtk-3-0 libayatana-appindicator3-1 libudev1 libxdo3
```

To access serial ports, your user must be allowed to use them. On most distributions this
means being a member of the `dialout` group (log out and in again afterwards):

```bash
sudo usermod -aG dialout "$USER"
```

### macOS

The programs are not signed. If macOS refuses to start them, allow them under
*System Settings → Privacy & Security*.

### Windows

No additional installation is needed. hof-blaze and hof-forge run without a console window.
hof-blaze's messages go to the [log file](hof-blaze.md#log-file); errors that prevent a
program from starting are shown in a message box.
