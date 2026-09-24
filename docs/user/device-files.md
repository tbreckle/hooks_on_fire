# Device files

A device file describes a type of device — which connection it uses and which commands it
understands. Device files are YAML files whose first entry is `device:`. hof-forge lists every
device file it finds under *Available Devices*.

Device files for supported hardware (`openfire.yaml`, `blast.yaml`) are shipped in the
`devices/` folder of the program. To add a device or change commands, put a device file into
`devices/` in your [user folder](installation.md#your-files). A file there replaces a shipped
file with the same name — copy the shipped file first if you only want to change it.

## Example

```yaml
device:
  type: lightgun
  name: openfire
  display-name: OpenFire
  connection-type: serial
  connection-details:
    speed: 115200
  max-instances: 4
  setup:
    - S6            # start with everything enabled
  teardown:
    - E
  enter_game:
    - F3x2x3        # pulse LED green three times
  recoil:
    - F0x2x1        # pulse solenoid once
  enable_autofire:
    - M8x1
  disable_autofire:
    - M8x0
```

Text after `#` is a comment.

## Fields

| Field | Required | Description |
|---|---|---|
| `type` | yes | `lightgun` or `lightcontroller`. Decides how commands are routed to players, see [game files](game-files.md#which-devices-receive-a-command). |
| `name` | yes | Unique identifier; must match the file name (`openfire` → `openfire.yaml`). |
| `display-name` | yes | Name shown in hof-forge. |
| `connection-type` | yes | Currently always `serial`. |
| `connection-details.speed` | yes | Baud rate of the serial connection, e.g. `115200`. |
| `command-delay` | no | Pause in milliseconds between two commands to this device. Default: `1`. |
| `max-instances` | no | How often this device may be added in hof-forge, e.g. `1` for a single light controller, `4` for light guns. Default: unlimited. |

Every other entry is an **action**: a name and the list of commands sent to the device, one
line each. The action names are what you use in the `commands` of the
[game files](game-files.md).

## Special actions

These actions are sent by Hooks on Fire itself. All of them are optional.

| Action | Sent |
|---|---|
| `setup` | when hof-blaze starts |
| `teardown` | when hof-blaze exits |
| `enter_game` | when a game is started |
| `leave_game` | when a game ends: the emulator stops, another game is started, or hof-blaze exits |

`setup` and `teardown` are sent to every device when hof-blaze starts and exits.
`enter_game` and `leave_game` are sent to every device that defines them, regardless of its
type and player.

## Placeholders

Commands can contain placeholders that are replaced when the command is sent:

| Placeholder | Replaced with |
|---|---|
| `{VALUE}` | Value of the signal, e.g. `1` for `LampStart = 1`. Empty for `___startup`, `___teardown`, `enter_game` and `leave_game`. |
| `{PLAYER}` | Player of the signal (`1`–`4`), `0` for signals of all players (`___all`), `enter_game` and `leave_game`. |
| `{GAMENAME}` | Name of the running game as sent by the emulator with `mame_start`, e.g. `lostwsga`. For `leave_game`, the game that is ending. |

Example for a light controller that switches the start lamp of the signal's player:

```yaml
  lamp_start:
    - B3x{PLAYER}x{VALUE}
```

Placeholders are not replaced in `setup` and `teardown`.
