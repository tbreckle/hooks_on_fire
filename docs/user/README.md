# User documentation

Hooks on Fire connects emulators to your light guns and arcade lighting. While a game runs,
the emulator sends output signals — for example `LampStart=1` or `P1_CtmRecoil=1` — as
**MAME-compatible network output**. MAME, Supermodel and TeknoParrot can
send it, as can any other program using the same format. Hooks on Fire receives the
signals, looks up what should happen for this game and sends the matching commands to your
devices over their (USB) serial ports.

```
emulator ──(MAME-compatible network output)──► hof-blaze ──(serial / USB)──► devices
                                                   ▲
                                 game files, device files, hof-config.yaml
                                                   ▲
                                        hof-forge (configuration)
```

Hooks on Fire consists of two programs:

- **hof-blaze** runs in the background (system tray) while you play.
- **hof-forge** is the configuration editor.

Only one of the two can run at a time: close hof-blaze before you configure, and close
hof-forge before you play.

## Contents

1. [Installation](installation.md) – download, folder layout, system requirements
2. [Network output setup](network-output.md) – enabling the emulator's MAME-compatible network output
3. [Configuring devices with hof-forge](hof-forge.md)
4. [Running hof-blaze](hof-blaze.md)
5. [Game files](game-files.md) – which signal triggers which command, per game
6. [Device files](device-files.md) – the commands a device understands
7. [Troubleshooting](troubleshooting.md)

## Quick start

1. [Install](installation.md) Hooks on Fire. Device files for OpenFIRE light guns and the
   B.L.A.S.T. light controller and game files for several games are included.
2. Start **hof-forge**, add your devices on the *Devices* tab, select their USB port and
   (for light guns) the player. See [hof-forge](hof-forge.md).
3. Enable the [network output](network-output.md) of your emulator.
4. Close hof-forge and start **hof-blaze**.
5. Start a game in the emulator. Hof-blaze creates a [game file](game-files.md) for it and adds
   every signal the game sends. Assign commands to the signals you want, e.g. `recoil`
   for the recoil signal of player 1.
