# Configuring devices with hof-forge

hof-forge edits your device list and network settings (`hof-config.yaml`). Changes are
saved immediately — on the *Devices* tab when you confirm a dialog, on the *Settings* tab
when you press *Save*.

hof-forge cannot run while hof-blaze is running. Exit hof-blaze from its tray menu first.

## Overview tab

Shows a short introduction, license information (hof-forge's user interface is made with
[Slint](https://slint.dev)) and two buttons:

- **Open hof-blaze log** (`Ctrl+L`) opens hof-blaze's [log file](hof-blaze.md#log-file) with
  your system's default program for log files.
- **Open game files folder** (`Ctrl+G`) opens the folder with your
  [game files](game-files.md#where-game-files-are).

## Devices tab

The tab has two lists:

- **Available Devices** (left) lists the device types found in the [device files](device-files.md)
  (shipped and in your user folder), e.g. `OpenFire [lightgun]`. Devices that may only be
  added a limited number of times show how many are in use, e.g.
  `B.L.A.S.T. [lightcontroller]  (1/1)`.
- **Configured Instances** (right) lists the devices you have added, e.g. `OpenFire P1  (Player 1)`.

Both lists are sorted alphabetically.

### Adding a device

1. Select the device type in *Available Devices*.
2. Press **Add** (or `Ins`, `Enter`, double-click).
3. Enter a unique name for this device, e.g. `OpenFire P1`, and press *OK*.

If the device type has reached its limit (for example, only one B.L.A.S.T. is allowed), the
*Add* button is disabled and a message explains why.

### Configuring a device

Select the device in *Configured Instances* and press **Configure** (or `F2`, `Enter`,
double-click). The dialog contains:

| Field | Description |
|---|---|
| Instance Name | Unique name of this device. |
| Connection | Connection type (currently always `serial`). |
| Port | The USB device this instance talks to. The list shows all connected USB serial devices with their current port, name and USB id, e.g. `/dev/ttyACM6 – OpenFIRE FIRECon P1 [f143:0001]` or `COM5 – …`. Press the refresh button (`F5`) after plugging in a device. |
| Player | Light guns only: the player this gun belongs to (1–4), or *Not assigned*. See [player assignment](#player-assignment). |

Press **Save** (`Enter`, `Ctrl+S`) to store the settings.

Hooks on Fire remembers the **USB device**, not the port name. If Windows assigns a new COM
port or Linux a new `/dev/ttyACM…` after re-plugging, the device is found again
automatically. A configured device that is not connected is shown as
`USB device [f143:0001], serial …  (not connected)` and keeps its setting.

### Player assignment

Signals in the [game files](game-files.md) belong to a player (1–4) or to all players.
Commands for a player are sent only to the light guns assigned to that player. Light guns
without a player receive the commands of every player that has no gun of its own — with a
single light gun you do not need to assign a player at all.

Light controllers (e.g. B.L.A.S.T.) always receive all commands; they get the player as a
parameter instead.

### Removing a device

Select the device and press **Remove** (`Del`), then confirm.

## Settings tab

| Setting | Default | Description |
|---|---|---|
| Listen on hostname | `localhost` | Computer the emulator runs on. |
| Listen on port | `8000` | TCP port of the emulator's network output. |
| UDP broadcast listen port | `8001` | Port for UDP broadcast messages. |

Press **Save** (`Ctrl+S`) to store the settings. See [network output setup](network-output.md).

## Keyboard shortcuts

hof-forge can be used completely with the keyboard. `Tab` / `Shift+Tab` move between
controls; on macOS use `⌘` instead of `Ctrl`.

| Where | Keys |
|---|---|
| Anywhere | `Ctrl+1` / `Ctrl+2` / `Ctrl+3` switch tabs, `Ctrl+Tab` / `Ctrl+Shift+Tab` next/previous tab, `Ctrl+L` open log, `Ctrl+G` open game files folder |
| Lists | `↑` `↓`, `Page Up` / `Page Down`, `Home` / `End` |
| Available Devices | `Enter` / `Ins` add |
| Configured Instances | `Enter` / `F2` configure, `Del` remove |
| Dialogs | `Enter` confirm, `Esc` cancel |
| Configure dialog | `F5` rescan ports, `Ctrl+S` save |
| Settings tab | `Ctrl+S` save |
