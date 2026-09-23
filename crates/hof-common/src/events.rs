//! Event definitions.

pub enum LineEvent {
    NewLine { line: String },
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
    },
    /// Device-level action (e.g. `enter_game`), sent to every device that has it configured,
    /// regardless of device type and player.
    DeviceAction {
        action: String,
        game: String,
    },
}

pub enum TrayEvent {
    StatusConnected { host: String, port: u16 },
    StatusDisconnected,
    StatusFaulty { error: String },
}
