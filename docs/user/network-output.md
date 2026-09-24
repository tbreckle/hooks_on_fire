# Network output setup

Hooks on Fire gets its information from **MAME-compatible network output**: a TCP server
(port 8000 by default) that sends one line per event while a game runs, for example:

```
mame_start = lostwsga
LampStart = 1
P1_CtmRecoil = 1
mame_stop = 1
```

This format was introduced by MAME and is also provided by other emulators and tools, for
example:

| Source | Notes |
|---|---|
| MAME | Built in, see [MAME](#mame) below. |
| Supermodel (Sega Model 3) | Enable its network output; see the Supermodel documentation. |
| TeknoParrot with OutputBlaster | Enable OutputBlaster's network output; see the OutputBlaster documentation. |

Any other program that sends the same format works as well. In this documentation, the
program sending the data is called the **emulator**.

## MAME

Start MAME with the `-output network` option:

```bash
mame -output network lostwsga
```

or enable it permanently in `mame.ini`:

```ini
output                    network
```

## Connection settings

By default hof-blaze connects to the emulator at `localhost`, port `8000`. If the emulator
runs on another computer or port, change *Listen on hostname* and *Listen on port* on the
*Settings* tab of [hof-forge](hof-forge.md#settings-tab).

hof-blaze keeps trying to connect while no emulator is running, so the start order of the
emulator and hof-blaze does not matter. The tray icon shows whether it is
[connected](hof-blaze.md#tray-icon).

If the emulator closes the TCP connection while a game is running, the game is ended just
like on `mame_stop`, and hof-blaze waits for the next connection. Some sources (for example
TeknoParrot with OutputBlaster) quit without sending `mame_stop`.

## UDP broadcast

In addition to the TCP connection, hof-blaze listens for the same `name = value` messages
as UDP broadcasts (default port `8001`, *UDP broadcast listen port* in hof-forge). Use it
for sources that send their output as broadcasts instead of providing a TCP server.

## Messages

| Message | Meaning in Hooks on Fire |
|---|---|
| `mame_start = <game>` (or `game = <game>`) | A game was started. Its [game file](game-files.md) is loaded. |
| `mame_stop = …` | The emulator stopped. The game is ended. |
| `pause = …` | The game was paused (logged only). |
| `<signal> = <value>` | An output of the game changed. Commands are sent according to the game file. |

The message names `mame_start` and `mame_stop` are part of the format and are used by all
sources, not only by MAME.

The signal names depend on the game and the emulator. hof-blaze adds every signal a game
sends to its game file automatically, so you can see which signals exist.
