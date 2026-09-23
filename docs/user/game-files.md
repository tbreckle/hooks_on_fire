# Game files

A game file tells Hooks on Fire what to do when a game sends a signal. There is one file per
game, named after the game name the emulator sends with `mame_start` (for MAME the short
name, e.g. `lostwsga.yaml` for *The Lost World*), in the Hooks on Fire folder.

You normally do not create game files yourself: when a game is started for the first time,
hof-blaze creates its file, and every signal the game sends is added to it. You then only
fill in the commands. Edit the files with any text editor; changes take effect the next time
the game is started.

## Example

```yaml
players:
  count: 2
signals:
  - signal: ___startup
    player: ___all
    commands:
      - enable_autofire
  - signal: ___teardown
    player: ___all
    commands:
      - disable_autofire
  - signal: P1_CtmRecoil
    player: 1
    commands:
      - recoil
  - signal: P2_CtmRecoil
    player: 2
    commands:
      - recoil
  - signal: LampStart
    player: 1
    commands:
      - lamp_start
  - signal: LampLeader
    player: ___all
    commands: []
```

## Fields

| Field | Description |
|---|---|
| `players.count` | Number of players of the game (1–4). |
| `signals` | List of signals. |
| `signal` | Name of the signal as sent by the emulator, e.g. `P1_CtmRecoil`. |
| `player` | Player the signal belongs to: `1`–`4`, or `___all` for all players. Default: `___all`. The number must not be higher than `players.count`. |
| `commands` | Names of the commands to send. Each name must be an action of a [device file](device-files.md), e.g. `recoil`. `[]` means: do nothing. |

## Which devices receive a command

A command is sent to every device whose device file defines it:

- **Light guns** receive commands of their own player only (as assigned in
  [hof-forge](hof-forge.md#player-assignment)). Guns without an assigned player receive the
  commands of every player that has no gun of its own. Signals with `player: ___all` go to
  all light guns.
- **Light controllers** receive all commands. The player is passed to the command as
  `{PLAYER}` (`0` for `___all`).

If a light controller and a light gun both define a command with the same name, only the
light controllers receive it. Give such commands different names (e.g. `lamp_recoil` and
`recoil`).

The value of the signal (e.g. `1` in `LampStart = 1`) is passed to the command as `{VALUE}`,
see [placeholders](device-files.md#placeholders).

## Fixed signals

Two signals are not sent by the emulator but by Hooks on Fire itself. They are added to
every game file automatically:

| Signal | Sent |
|---|---|
| `___startup` | when the game is started |
| `___teardown` | when the game ends: the emulator stops, another game is started, or hof-blaze is closed |

Use them for per-game settings, for example switching a light gun to autofire for one game
and back afterwards.
