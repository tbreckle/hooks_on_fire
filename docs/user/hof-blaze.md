# Running hof-blaze

hof-blaze is the program that runs while you play. Start it from the Hooks on Fire folder
(see [installation](installation.md#folder-layout)); it places an icon in the system tray
and shows a notification *Blaze started*.

hof-blaze cannot run while hof-forge is running. Close hof-forge first.

## What happens on start

1. The settings and the [device files](device-files.md) of all configured devices are loaded.
2. Every configured USB device is looked up by its USB id and serial number, and its serial
   port is opened.
3. The `setup` commands of each device are sent.
4. hof-blaze connects to the emulator's [network output](network-output.md) and waits for games.

If anything fails — for example a device is not connected — a message box explains the
problem and hof-blaze exits when you press *OK*. See [troubleshooting](troubleshooting.md).

## Tray icon

| Icon state | Menu status | Meaning |
|---|---|---|
| Waiting | `Status: Waiting` / `Status: Disconnected` | Not connected (emulator not running or its network output disabled). |
| Healthy | `Status: Healthy` | Connected to the emulator. |
| Faulty | `Status: Faulty` | The connection to the emulator failed with an error. |

A notification is also shown when the connection to the emulator is established or lost.

To quit, choose **Exit** in the tray menu.

## During a game

- When the emulator starts a game, hof-blaze loads its [game file](game-files.md) `<game>.yaml`.
  If there is none yet, it creates one and shows a notification.
- Every signal the emulator sends is looked up in the game file, and the configured commands are
  sent to the devices. Signals that are not in the game file yet are added automatically
  (without commands), so you can assign commands to them later.
- When a game starts or ends, the `enter_game` / `leave_game` commands of the devices and
  the `___startup` / `___teardown` commands of the game file are sent.

Changes to a game file take effect the next time the game is started.

## On exit

When you choose *Exit*, hof-blaze

1. ends the running game (`___teardown` of the game file, then `leave_game` of the devices),
2. writes statistics about the received signals to the log, for example
   `LampView1 (0, 1, 255): 23` — the signal, all values seen, and how often it was received.
   This helps to find out which signals a game uses,
3. sends the `teardown` commands of each device.

## Log file

hof-blaze writes everything it does to `logs/hof-blaze.log` in the Hooks on Fire folder. If
that folder is not writable, the log is written to a `logs` folder next to the
[settings file](installation.md#settings-file) instead. The log file is emptied every time
hof-blaze starts.

Open it with the *Open hof-blaze log* button in [hof-forge](hof-forge.md#overview-tab)
(`Ctrl+L`).

The log shows every signal received and every command sent to a device. For even more
detail, start hof-blaze with the environment variable `RUST_LOG=debug`.
