# Troubleshooting

Most problems are explained in the [log file](hof-blaze.md#log-file). Open it from
hof-forge with *Open hof-blaze log* (`Ctrl+L`).

## hof-blaze does not start

hof-blaze shows a message box with the reason and exits when you press *OK*.

| Message | Solution |
|---|---|
| *hof-forge is currently running* | Close hof-forge. The two programs cannot run at the same time. |
| *No devices configured* | Add your devices in [hof-forge](hof-forge.md#adding-a-device). |
| *Failed to read device file: …* | The device file named in the message is missing in the Hooks on Fire folder, or hof-blaze was not started from that folder. See [folder layout](installation.md#folder-layout). |
| *Device '…' is not available: USB device … is not connected* | Plug in the device. If it is connected, open [Configure](hof-forge.md#configuring-a-device) in hof-forge and select it again. |
| *… is connected more than once and cannot be told apart* | Two identical devices without a USB serial number are connected. Hooks on Fire cannot tell which one is which; connect only one of them. |
| *Device '…' is configured N times …, but at most M instance(s) are allowed* | Remove the extra devices in hof-forge. |
| *Failed to open serial port* | The port is in use by another program, or (Linux) you lack permission — see [Linux notes](installation.md#linux). |

## Nothing happens during a game

- Check the tray icon: if it does not show *Status: Healthy*, hof-blaze is not connected to
  the emulator. Make sure the emulator's [network output](network-output.md) is enabled and
  that host and port in hof-forge's *Settings* tab match.
- Open the game's [game file](game-files.md) and check that the signals have commands.
  Signals without commands (`commands: []`) do nothing.
- Check that the command names in the game file exist as actions in the
  [device file](device-files.md). The log shows *Unknown action command* otherwise.
- Changes to a game file take effect the next time the game is started.

## A command reaches the wrong light gun, or all of them

Assign a player to each light gun in hof-forge
([player assignment](hof-forge.md#player-assignment)) and check the `player` of the signal
in the game file. Guns without a player receive the commands of every player that has no
gun of its own.

## The COM port / `/dev/ttyACM…` of a device changed

Nothing to do: Hooks on Fire finds USB devices by their USB id. If you configured the
device with an older version (stored as a port name), open *Configure* in hof-forge once
and press *Save*.
