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
| TeknoParrot | The outputs are sent by DemulShooter and/or OutputBlaster; see [TeknoParrot](#teknoparrot) below. |

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

## TeknoParrot

TeknoParrot itself does not send any game data. It is provided by
[DemulShooter](#demulshooter) and/or [OutputBlaster](#outputblaster), which run next to the
game, read its lamps, recoil, damage, … and send them as MAME-compatible network output.
Which of them supports a game differs from game to game; check their game lists. Use only one
of them per game, as both provide the TCP server on port `8000`.

### DemulShooter

[DemulShooter](https://github.com/argonlefou/DemulShooter) (Windows) is started next to the
game. It also supports other emulators and PC arcade games (Demul, Sega Model 2, …); the setup
is the same.

1. Download DemulShooter from its
   [releases](https://github.com/argonlefou/DemulShooter/releases) and unpack it.
2. Start `DemulShooter_GUI.exe`, open the *Outputs* tab and select **Network outputs**
   (instead of *Window Message outputs (MameHooker)* or *Disabled*). Save the configuration.
   The recoil and damage pulse lengths on the same tab are used for the outputs DemulShooter
   generates itself.
3. Start DemulShooter together with the game (for example from a batch file or your
   frontend), with the target and rom of the game:

   ```bat
   DemulShooter.exe -target=lindbergh -rom=hotd4
   ```

   Newer games (for example Sega ALLS and Nu) need `DemulShooterX64.exe` instead. The
   `-target` and `-rom` values of each game are listed in the
   [DemulShooter wiki](https://github.com/argonlefou/DemulShooter/wiki/Usage).
   DemulShooter must run as Administrator.

DemulShooter sends the `-rom` value as the game name (`mame_start = hotd4`), so the
[game file](game-files.md) of the game is named after it (`hotd4.yaml`).

DemulShooter provides the TCP server on port `8000` and only accepts connections from the same
computer, so hof-blaze must run on the same PC with the default *Listen on hostname*
`localhost`. More about DemulShooter's outputs:
[DemulShooter wiki: Outputs](https://github.com/argonlefou/DemulShooter/wiki/Outputs).

### OutputBlaster

[OutputBlaster](https://github.com/Boomslangnz/OutputBlaster) is a DLL that is loaded by the
game in TeknoParrot:

1. Place `OutputBlaster.dll` in the game folder and enable *Outputs* in the game's settings
   in TeknoParrot.
2. Switch it to network output in `OutputBlaster.ini` in the game folder:

   ```ini
   [Settings]
   OutputsSystem=1
   ```

   The TCP port is `8000` (`NetOutputsTCPPort`); the outputs are also sent as UDP broadcast
   on port `8001` (`NetOutputsUDPBroadcastPort`).

OutputBlaster uses its own game names in `mame_start`; the game file is named after the game
name hof-blaze receives (see the log or the tray status). More details:
[OutputBlaster on GitHub](https://github.com/Boomslangnz/OutputBlaster).

## Connection settings

By default hof-blaze connects to the emulator at `localhost`, port `8000`. If the emulator
runs on another computer or port, change *Listen on hostname* and *Listen on port* on the
*Settings* tab of [hof-forge](hof-forge.md#settings-tab).

hof-blaze keeps trying to connect while no emulator is running, so the start order of the
emulator and hof-blaze does not matter. The tray icon shows whether it is
[connected](hof-blaze.md#tray-icon).

If the emulator closes the TCP connection while a game is running, the game is ended just
like on `mame_stop`, and hof-blaze waits for the next connection. Some sources quit without
sending `mame_stop`.

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
