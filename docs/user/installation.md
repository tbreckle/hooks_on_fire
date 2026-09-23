# Installation

## Download

Download the archive for your system from the GitHub releases page:

| System | Archive |
|---|---|
| Windows (64-bit) | `hooks-on-fire-x86_64-pc-windows-msvc.zip` |
| Linux (64-bit) | `hooks-on-fire-x86_64-unknown-linux-gnu.tar.gz` |
| macOS (Apple Silicon) | `hooks-on-fire-aarch64-apple-darwin.tar.gz` |
| macOS (Intel) | `hooks-on-fire-x86_64-apple-darwin.tar.gz` |

Unpack it into a folder **you can write to**, for example `C:\Games\HooksOnFire` or
`~/HooksOnFire`. Avoid `C:\Program Files`: Hooks on Fire creates game files and its log
file in this folder.

## Folder layout

The archive contains the two programs. Add the [device files](device-files.md) for your
hardware (for example `openfire.yaml` for OpenFIRE light guns, `blast.yaml` for the
B.L.A.S.T. light controller). They are available in the Hooks on Fire repository.

```
HooksOnFire/
├── hof-blaze(.exe)      background program used while playing
├── hof-forge(.exe)      configuration editor
├── openfire.yaml        device file
├── blast.yaml           device file
├── lostwsga.yaml        game file (created automatically)
└── logs/
    └── hof-blaze.log    log file (created automatically)
```

Device files and game files are read from the folder the programs are started from.
Start the programs from this folder: double-click them, or when using a terminal, change
into the folder first (`cd ~/HooksOnFire`, then `./hof-blaze`).

## Settings file

Your device list and network settings are stored in `hof-config.yaml`, which hof-forge
creates and edits for you:

| System | Location |
|---|---|
| Windows | `%APPDATA%\hooks-on-fire\hof-config.yaml` |
| Linux | `~/.config/hooks-on-fire/hof-config.yaml` |
| macOS | `~/Library/Application Support/hooks-on-fire/hof-config.yaml` |

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

No additional installation is needed. hof-blaze runs without a console window; its
messages go to the [log file](hof-blaze.md#log-file).
