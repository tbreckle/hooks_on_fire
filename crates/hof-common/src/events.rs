//! Event definitions.

pub enum LineEvent {
    NewLine {
        line: String,
    },
    /// The TCP connection to the emulator was lost. Ends the running game, because some
    /// sources close the connection without sending `mame_stop`.
    Disconnected,
}

pub enum StateEvent {
    NewGame { game_name: String },
    GameStopped,
    GamePaused,
}

pub enum GameEvent {
    Data {
        key: String,
        value: String,
    },
    Action {
        action: String,
        value: String,
        player: String,
        /// Name of the running game (for the `{GAMENAME}` placeholder).
        game: String,
        /// Device types and device names the running game suppresses (game file `suppression`).
        suppression: Vec<String>,
    },
    /// Device-level action (e.g. `enter_game`), sent to every device that has it configured,
    /// regardless of device type and player.
    DeviceAction {
        action: String,
        game: String,
        /// Device types and device names the game suppresses (game file `suppression`).
        suppression: Vec<String>,
    },
}

pub enum TrayEvent {
    StatusConnected {
        host: String,
        port: u16,
    },
    StatusDisconnected,
    StatusFaulty {
        error: String,
    },
    /// A game was started: its name (as received with `mame_start`) and the `display-name`
    /// of its game file.
    GameStarted {
        name: String,
        display_name: Option<String>,
    },
    /// No game is running (anymore).
    GameEnded,
}
