use anyhow::{bail, Context, Result};
use hof_common::config::MAX_PLAYERS;
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Fixed signal fired by the engine each time a new game is started.
pub const STARTUP_SIGNAL: &str = "___startup";
/// Fixed signal fired by the engine each time a game ends (game stop, new game or quit).
pub const TEARDOWN_SIGNAL: &str = "___teardown";
/// Allowed range of a signal's `repeat` interval in ms.
pub const REPEAT_RANGE_MS: std::ops::RangeInclusive<u64> = 10..=10_000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum PlayerSpec {
    All(String), // "___all"
    Number(u32),
}

impl PlayerSpec {
    pub fn all() -> Self {
        PlayerSpec::All("___all".to_string())
    }

    pub fn validate(&self, player_count: u32) -> Result<()> {
        match self {
            PlayerSpec::All(s) if s == "___all" => Ok(()),
            PlayerSpec::All(s) => bail!(
                "Invalid player spec: '{}'. Expected '___all' or a number",
                s
            ),
            PlayerSpec::Number(n) if *n == 0 => bail!("Player number must be > 0"),
            PlayerSpec::Number(n) if *n > player_count => {
                bail!("Player number {} exceeds player count {}", n, player_count)
            }
            PlayerSpec::Number(_) => Ok(()),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Players {
    pub count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Signal {
    pub signal: String,
    #[serde(default = "PlayerSpec::all")]
    pub player: PlayerSpec,
    /// Optional: while the signal is held (value not `0`), repeat the commands every
    /// `repeat` ms (see `repeater`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repeat: Option<u64>,
    pub commands: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameConfig {
    /// Optional: human-readable title of the game (e.g. "The Lost World: Jurassic Park"), shown
    /// in the tray. The game name itself is the file name.
    #[serde(
        rename = "display-name",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub display_name: Option<String>,
    pub players: Players,
    /// Device types (`lightgun`, `lightcontroller`) and device names (e.g. `openfire`) that
    /// get no commands while this game runs, not even `enter_game` / `leave_game`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub suppression: Vec<String>,
    #[serde(default)]
    pub signals: Vec<Signal>,
    /// The game file exists but could not be loaded: never save this (default)
    /// configuration over it.
    #[serde(skip)]
    pub read_only: bool,
}

impl GameConfig {
    /// Validate the entire configuration
    pub fn validate(&self) -> Result<()> {
        if self.players.count == 0 {
            bail!("Player count must be > 0");
        }
        if self.players.count > u32::from(MAX_PLAYERS) {
            bail!(
                "Player count {} exceeds the maximum of {} players",
                self.players.count,
                MAX_PLAYERS
            );
        }

        if self
            .display_name
            .as_ref()
            .is_some_and(|name| name.trim().is_empty())
        {
            bail!("'display-name' cannot be empty");
        }

        if self.suppression.iter().any(|s| s.trim().is_empty()) {
            bail!("Suppression entries cannot be empty");
        }

        // Validate all signal player specs
        for (idx, signal) in self.signals.iter().enumerate() {
            signal
                .player
                .validate(self.players.count)
                .with_context(|| {
                    format!(
                        "Invalid player spec in signal #{} (signal: {})",
                        idx, signal.signal
                    )
                })?;

            if signal.signal.is_empty() {
                bail!("Signal name cannot be empty in signal #{}", idx);
            }

            if let Some(repeat) = signal.repeat {
                if signal.signal == STARTUP_SIGNAL || signal.signal == TEARDOWN_SIGNAL {
                    bail!("Signal {} cannot use 'repeat'", signal.signal);
                }
                if !REPEAT_RANGE_MS.contains(&repeat) {
                    bail!(
                        "Invalid 'repeat' {} in signal #{} (signal: {}): allowed are {} to {} ms",
                        repeat,
                        idx,
                        signal.signal,
                        REPEAT_RANGE_MS.start(),
                        REPEAT_RANGE_MS.end()
                    );
                }
            }
        }

        Ok(())
    }

    /// Insert the fixed `___startup` / `___teardown` signals at the top if they are missing.
    /// Returns true if the configuration was changed.
    pub fn ensure_fixed_signals(&mut self) -> bool {
        let mut changed = false;
        for (idx, name) in [STARTUP_SIGNAL, TEARDOWN_SIGNAL].into_iter().enumerate() {
            if !self.signals.iter().any(|s| s.signal == name) {
                self.signals.insert(
                    idx,
                    Signal {
                        signal: name.to_string(),
                        player: PlayerSpec::all(),
                        repeat: None,
                        commands: vec![],
                    },
                );
                changed = true;
            }
        }
        changed
    }

    pub(crate) fn default() -> GameConfig {
        let mut config = GameConfig {
            display_name: None,
            players: Players { count: 2 },
            suppression: vec![],
            signals: vec![],
            read_only: false,
        };
        config.ensure_fixed_signals();
        config
    }
}

pub struct Gamefile;

impl Gamefile {
    /// Parse a YAML file and validate its schema
    pub fn parse_file<P: AsRef<Path>>(path: P) -> Result<GameConfig> {
        let path = path.as_ref();
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read file: {}", path.display()))?;

        Self::parse_str(&content)
    }

    /// Parse YAML string and validate its schema
    pub fn parse_str(yaml: &str) -> Result<GameConfig> {
        let config: GameConfig = serde_yaml::from_str(yaml).context("Failed to parse YAML")?;

        config.validate()?;

        Ok(config)
    }

    pub fn save_to_file<P: AsRef<Path>>(config: &GameConfig, path: P) -> Result<()> {
        let yaml = serde_yaml::to_string(config).context("Failed to serialize config to YAML")?;
        std::fs::write(path, yaml).context("Failed to write YAML to file")?;
        Ok(())
    }
}

// #[cfg(test)]
// mod tests {
//     use super::*;

//     fn get_full_error_chain(err: &anyhow::Error) -> String {
//         format!("{:#}", err)
//     }

//     #[test]
//     fn test_parse_valid_config() {
//         let yaml = r#"
// players:
//   count: 2
// signals:
//   - signal: P1_CtmRecoil
//     player: 1
//     commands:
//       - recoil
//       - recoil_value
//   - signal: P2_CtmRecoil
//     player: 2
//     commands:
//       - recoil
//       - recoil_value
//   - signal: LampStart
//     player: ___all
//     commands:
//       - lamp_start
//   - signal: LampView1
//     player: ___all
//     commands:
//       - lamp_coin
// "#;

//         let config = Gamefile::parse_str(yaml).unwrap();
//         assert_eq!(config.players.count, 2);
//         assert_eq!(config.signals.len(), 4);
//     }

//     #[test]
//     fn test_parse_actual_lightgun_yaml() {
//         // Test with the actual lightgun.yaml content
//         let yaml = std::fs::read_to_string("../../lightgun.yaml");
//         if let Ok(content) = yaml {
//             let config = Gamefile::parse_str(&content).unwrap();
//             assert_eq!(config.players.count, 2);
//             assert_eq!(config.signals.len(), 4);
//         }
//         // If file doesn't exist, skip test (not an error)
//     }

//     #[test]
//     fn test_invalid_player_number() {
//         let yaml = r#"
// players:
//   count: 2
// signals:
//   - signal: P3_Test
//     player: 3
//     commands:
//       - recoil
// "#;

//         let result = Gamefile::parse_str(yaml);
//         assert!(result.is_err());
//         let err = result.unwrap_err();
//         let err_msg = get_full_error_chain(&err);
//         assert!(err_msg.contains("exceeds player count"));
//     }

//     #[test]
//     fn test_invalid_player_zero() {
//         let yaml = r#"
// players:
//   count: 2
// signals:
//   - signal: Test
//     player: 0
//     commands:
//       - recoil
// "#;

//         let result = Gamefile::parse_str(yaml);
//         assert!(result.is_err());
//         let err = result.unwrap_err();
//         let err_msg = get_full_error_chain(&err);
//         assert!(err_msg.contains("must be > 0"));
//     }

//     #[test]
//     fn test_invalid_player_spec() {
//         let yaml = r#"
// players:
//   count: 2
// "#;

//         let result = Gamefile::parse_str(yaml);
//         assert!(result.is_err());
//         let err = result.unwrap_err();
//         let err_msg = get_full_error_chain(&err);
//         assert!(err_msg.contains("Invalid player spec"));
//     }

//     #[test]
//     fn test_empty_signal_name() {
//         let yaml = r#"
// players:
//   count: 2
// signals:
//   - signal: ""
//     player: 1
//     commands:
//       - recoil
// "#;

//         let result = Gamefile::parse_str(yaml);
//         assert!(result.is_err());
//         let err = result.unwrap_err();
//         let err_msg = get_full_error_chain(&err);
//         assert!(err_msg.contains("cannot be empty"));
//     }

//     #[test]
//     fn test_zero_player_count() {
//         let yaml = r#"
// players:
//   count: 0
// signals: []
// "#;

//         let result = Gamefile::parse_str(yaml);
//         assert!(result.is_err());
//         let err = result.unwrap_err();
//         let err_msg = get_full_error_chain(&err);
//         assert!(err_msg.contains("Player count must be > 0"));
//     }
// }

#[cfg(test)]
mod fixed_signal_tests {
    use super::*;

    #[test]
    fn fixed_signals_parse_without_player() {
        let config = Gamefile::parse_str(
            "players:\n  count: 2\nsignals:\n  - signal: ___startup\n    commands:\n      - setup_autofire\n",
        )
        .unwrap();
        assert_eq!(config.signals[0].player, PlayerSpec::all());
    }

    #[test]
    fn ensure_fixed_signals_inserts_at_top() {
        let mut config = Gamefile::parse_str(
            "players:\n  count: 2\nsignals:\n  - signal: A\n    player: 1\n    commands: []\n",
        )
        .unwrap();
        assert!(config.ensure_fixed_signals());
        let names: Vec<_> = config.signals.iter().map(|s| s.signal.as_str()).collect();
        assert_eq!(names, [STARTUP_SIGNAL, TEARDOWN_SIGNAL, "A"]);
        assert!(!config.ensure_fixed_signals());
    }

    #[test]
    fn suppression_is_optional_and_not_saved_when_empty() {
        let config = Gamefile::parse_str("players:\n  count: 2\n").unwrap();
        assert!(config.suppression.is_empty());
        let yaml = serde_yaml::to_string(&config).unwrap();
        assert!(!yaml.contains("suppression"));

        let config =
            Gamefile::parse_str("players:\n  count: 2\nsuppression:\n  - lightgun\n").unwrap();
        assert_eq!(config.suppression, ["lightgun"]);
        assert!(Gamefile::parse_str("players:\n  count: 2\nsuppression:\n  - ''\n").is_err());
    }

    fn signal_yaml(signal: &str, repeat: &str) -> String {
        format!("players:\n  count: 2\nsignals:\n  - signal: {signal}\n{repeat}    commands: []\n")
    }

    #[test]
    fn display_name_is_optional() {
        let config = Gamefile::parse_str("players:\n  count: 2\n").unwrap();
        assert_eq!(config.display_name, None);
        assert!(!serde_yaml::to_string(&config)
            .unwrap()
            .contains("display-name"));

        let config =
            Gamefile::parse_str("display-name: Jurassic Park\nplayers:\n  count: 2\n").unwrap();
        assert_eq!(config.display_name.as_deref(), Some("Jurassic Park"));
        let yaml = serde_yaml::to_string(&config).unwrap();
        assert!(yaml.starts_with("display-name: Jurassic Park\n"));

        assert!(Gamefile::parse_str("display-name: ''\nplayers:\n  count: 2\n").is_err());
    }

    #[test]
    fn repeat_is_optional_and_validated() {
        let config = Gamefile::parse_str(&signal_yaml("P1_CtmRecoil", "")).unwrap();
        assert_eq!(config.signals[0].repeat, None);
        assert!(!serde_yaml::to_string(&config).unwrap().contains("repeat"));

        let config =
            Gamefile::parse_str(&signal_yaml("P1_CtmRecoil", "    repeat: 100\n")).unwrap();
        assert_eq!(config.signals[0].repeat, Some(100));

        assert!(Gamefile::parse_str(&signal_yaml("P1_CtmRecoil", "    repeat: 5\n")).is_err());
        assert!(Gamefile::parse_str(&signal_yaml("P1_CtmRecoil", "    repeat: 10001\n")).is_err());
        assert!(Gamefile::parse_str(&signal_yaml(STARTUP_SIGNAL, "    repeat: 100\n")).is_err());
    }
}
