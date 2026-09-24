# Running hof-blaze

hof-blaze is the program that runs while you play. It places an icon in the system tray and
shows a notification *Blaze started*.

hof-blaze cannot run while hof-forge is running. To switch from hof-forge to hof-blaze, use
*Switch to hof-blaze* on the [Overview tab](hof-forge.md#overview-tab) of hof-forge.

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

While a game is running, it is shown in brackets behind the status: the `display-name` from
its [game file](game-files.md) and the game name sent by the emulator, for example
`Status: Healthy (The Lost World: Jurassic Park - lostwsga)`. Without `display-name`, only the
game name is shown.

A notification is also shown when the connection to the emulator is established or lost.

The tray menu also has:

- **Open HoF-forge**: exits hof-blaze (see [what happens on exit](#what-happens-on-exit)) and
  starts [hof-forge](hof-forge.md) to change the configuration.
- **Open log file**: opens the [log file](#log-file) with your system's default program.

To quit, choose **Exit** in the tray menu.

hof-blaze and hof-forge must be in the same folder for switching between them. When one is
started by the other, it waits up to 3 seconds for the other one to exit (command line option
`--wait-for-lock`; you can use it yourself as well, for example in a start script).

## During a game

- When the emulator starts a game, hof-blaze loads its [game file](game-files.md) `<game>.yaml`
  (your own first, then the shipped one). If there is none yet, it creates one in your
  [user folder](installation.md#your-files) and shows a notification.
- Every signal the emulator sends is looked up in the game file, and the configured commands are
  sent to the devices. Signals that are not in the game file yet are added automatically
  (without commands), so you can assign commands to them later. Changed game files are always
  saved to your user folder.
- If a game file cannot be loaded (e.g. a typing error in it), a notification tells you so.
  The game runs without commands and the file is left unchanged.
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

hof-blaze writes everything it does to `logs/hof-blaze.log` in the program folder. If
that folder is not writable, the log is written to a `logs` folder next to the
[user folder](installation.md#your-files) instead. The log file is emptied every time
hof-blaze starts.

While hof-blaze is running, open it with **Open log file** in the tray menu. Otherwise use the
*Open hof-blaze log* button in [hof-forge](hof-forge.md#overview-tab) (`Ctrl+L`).

The log shows every signal received and every command sent to a device. For even more
detail, start hof-blaze with the environment variable `RUST_LOG=debug`.
