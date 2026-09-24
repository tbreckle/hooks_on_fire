use anyhow::Context;
use hof_common::data_files::{self, DataKind};
use hof_common::events::{GameEvent, StateEvent};
use std::path::PathBuf;
use std::thread::JoinHandle;
use tokio::sync::oneshot;
use tracing::{debug, error, info, warn};

use crate::data_stats::DataStats;
use crate::devices::{ENTER_GAME_ACTION, LEAVE_GAME_ACTION};
use crate::gamefile::{GameConfig, Gamefile, Signal, STARTUP_SIGNAL, TEARDOWN_SIGNAL};

pub struct EngineHandle {
    shutdown_tx: oneshot::Sender<()>,
    thread_handle: JoinHandle<anyhow::Result<()>>,
}

impl EngineHandle {
    /// Stops the engine. The `___teardown` signal of the running game is fired before the
    /// engine thread exits.
    pub fn shutdown(self) -> anyhow::Result<()> {
        let _ = self.shutdown_tx.send(());
        self.thread_handle
            .join()
            .map_err(|_| anyhow::anyhow!("Engine thread panicked."))?
    }
}

pub fn start_engine(
    state_rx: tokio::sync::mpsc::Receiver<StateEvent>,
    game_rx: tokio::sync::mpsc::Receiver<GameEvent>,
    action_tx: tokio::sync::mpsc::Sender<GameEvent>,
) -> anyhow::Result<EngineHandle> {
    let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();

    let thread_handle = std::thread::spawn(move || -> anyhow::Result<()> {
        // Use tokio runtime within the thread for async channel operations
        let rt = tokio::runtime::Runtime::new().context("Failed to create tokio runtime")?;

        rt.block_on(async { run_engine_loop(state_rx, game_rx, action_tx, shutdown_rx).await })
    });

    Ok(EngineHandle {
        shutdown_tx,
        thread_handle,
    })
}

async fn run_engine_loop(
    mut state_rx: tokio::sync::mpsc::Receiver<StateEvent>,
    mut game_rx: tokio::sync::mpsc::Receiver<GameEvent>,
    action_tx: tokio::sync::mpsc::Sender<GameEvent>,
    mut shutdown_rx: oneshot::Receiver<()>,
) -> anyhow::Result<()> {
    let mut current_game = String::new();
    let mut current_config: Option<GameConfig> = None;
    let mut stats = DataStats::default();

    loop {
        tokio::select! {
            _ = &mut shutdown_rx => {
                info!("Engine shutting down.");
                end_current_game(&mut current_config, &current_game, &action_tx).await;
                stats.log();
                break;
            }
            Some(state_event) = state_rx.recv() => {
                match state_event {
                    StateEvent::NewGame { game_name } => {
                        handle_new_game(&game_name, &mut current_game, &mut current_config, &action_tx).await;
                    }
                    StateEvent::GameStopped => {
                        info!("Game stopped.");
                        end_current_game(&mut current_config, &current_game, &action_tx).await;
                    }
                    StateEvent::GamePaused => {
                        info!("Game paused.");
                    }
                }
            }
            Some(game_event) = game_rx.recv() => {
                match game_event {
                    GameEvent::Data { key, value } => {
                        stats.record(&key, &value);
                        handle_data_event(&key, &value, &mut current_config, &current_game, &action_tx).await;
                    }
                    // GameEvent::Action { .. } => {
                    GameEvent::Action { player, value, action, .. } => {
                        warn!("Unexpected action event on game channel: {} with value: {} for player {}", action, value, player);
                        // Actions from game_rx are not expected, but we can ignore them
                    }
                    GameEvent::DeviceAction { action, .. } => {
                        warn!("Unexpected device action event on game channel: {}", action);
                    }
                }
            }
        }
    }

    Ok(())
}

/// Fires the `___teardown` signal of the running game (if any), then the `leave_game` device
/// action, and clears the game.
async fn end_current_game(
    current_config: &mut Option<GameConfig>,
    current_game: &str,
    action_tx: &tokio::sync::mpsc::Sender<GameEvent>,
) {
    if let Some(cfg) = current_config.take() {
        info!("Tearing down current game.");
        send_signal_actions(&cfg, current_game, TEARDOWN_SIGNAL, "", action_tx).await;
        send_device_action(LEAVE_GAME_ACTION, current_game, &cfg.suppression, action_tx).await;
    }
}

async fn handle_new_game(
    game_name: &str,
    current_game: &mut String,
    current_config: &mut Option<GameConfig>,
    action_tx: &tokio::sync::mpsc::Sender<GameEvent>,
) {
    info!("Engine received new game event: {}", game_name);
    if game_name.is_empty() {
        warn!("Game name is empty. Ignoring new game event.");
        return;
    }

    // Tear down the previous game while it is still the current game ({GAMENAME}).
    end_current_game(current_config, current_game, action_tx).await;

    *current_game = game_name.to_string();
    // Load the game file: user layer first, then the shipped game files.
    let config = match data_files::find(DataKind::Games, game_name) {
        Some(path) => {
            info!("Loading game configuration file: {}", path.display());
            match Gamefile::parse_file(&path) {
                Ok(mut config) => {
                    // Add the fixed signals to existing game files so the user can see and edit them.
                    if config.ensure_fixed_signals() {
                        info!("Adding fixed signals to game configuration: {}", game_name);
                        save_game_file_or_notify(&config, game_name);
                    }
                    config
                }
                Err(err) => {
                    error!(
                        "Failed to load game configuration {}: {:#}",
                        path.display(),
                        err
                    );
                    let _ = notify_rust::Notification::new()
                        .summary("Hooks on Fire configuration error.")
                        .body(&format!(
                            "Failed to load game configuration {}.\n\nThe file is left unchanged.",
                            path.display()
                        ))
                        .show();
                    // Keep the broken file as it is: never save the default over it.
                    GameConfig {
                        read_only: true,
                        ..GameConfig::default()
                    }
                }
            }
        }
        None => {
            info!(
                "No game configuration found for '{}'. Creating default configuration.",
                game_name
            );
            let config = GameConfig::default();
            if let Some(path) = save_game_file_or_notify(&config, game_name) {
                let _ = notify_rust::Notification::new()
                    .summary("Hooks on Fire game not found.")
                    .body(&format!(
                        "No game configuration found.\n\nCreated new configuration for game: {}",
                        path.display()
                    ))
                    .show();
            }
            config
        }
    };

    info!(
        "Game configuration loaded: {} players, {} signals",
        config.players.count,
        config.signals.len()
    );
    if !config.suppression.is_empty() {
        info!(
            "Suppressed devices for this game: {}",
            config.suppression.join(", ")
        );
    }
    // The game file is needed first: suppressed devices get no `enter_game` either.
    send_device_action(ENTER_GAME_ACTION, game_name, &config.suppression, action_tx).await;
    send_signal_actions(&config, game_name, STARTUP_SIGNAL, "", action_tx).await;
    *current_config = Some(config);
}

/// Sends a device-level action (`enter_game` / `leave_game`) to all devices having it.
async fn send_device_action(
    action: &str,
    game: &str,
    suppression: &[String],
    action_tx: &tokio::sync::mpsc::Sender<GameEvent>,
) {
    let _ = action_tx
        .send(GameEvent::DeviceAction {
            action: action.to_string(),
            game: game.to_string(),
            suppression: suppression.to_vec(),
        })
        .await;
}

/// Sends an action event for every command of every signal matching `key`.
/// Returns true if at least one signal matched.
async fn send_signal_actions(
    cfg: &GameConfig,
    game: &str,
    key: &str,
    value: &str,
    action_tx: &tokio::sync::mpsc::Sender<GameEvent>,
) -> bool {
    let mut found = false;
    let mut actions: Vec<(String, String, String)> = Vec::new();

    for signal in &cfg.signals {
        if signal.signal == key {
            debug!("Matched signal: {}", signal.signal);
            found = true;
            let player_value = match &signal.player {
                crate::gamefile::PlayerSpec::All(value) => value.clone(),
                crate::gamefile::PlayerSpec::Number(value) => value.to_string(),
            };
            debug!("Associated commands: {:?}", signal.commands);
            debug!("Associated player value: {}", player_value);
            debug!("Signal value: {}", value);
            for command in &signal.commands {
                actions.push((command.clone(), value.to_string(), player_value.clone()));
            }
        }
    }

    // Send action events
    for (action, value, player) in actions {
        info!(
            "Sending action event: {} with value {} for player {}",
            action, value, player
        );
        let _ = action_tx
            .send(GameEvent::Action {
                action,
                value,
                player,
                game: game.to_string(),
                suppression: cfg.suppression.clone(),
            })
            .await;
    }

    found
}

/// Saves a game file to the user layer (it then overrides a shipped game file of the same
/// name). Errors are logged and shown as notification. Returns the saved path.
fn save_game_file_or_notify(config: &GameConfig, game: &str) -> Option<PathBuf> {
    let result = data_files::user_path(DataKind::Games, game)
        .and_then(|path| Gamefile::save_to_file(config, &path).map(|()| path));
    match result {
        Ok(path) => {
            debug!("Saved game configuration {}", path.display());
            Some(path)
        }
        Err(err) => {
            error!(
                "Failed to save game configuration for '{}': {:#}",
                game, err
            );
            let _ = notify_rust::Notification::new()
                .summary("Hooks on Fire configuration error.")
                .body(&format!(
                    "Failed to save game configuration for '{}'.\n\nError: {:#}",
                    game, err
                ))
                .show();
            None
        }
    }
}

async fn handle_data_event(
    key: &str,
    value: &str,
    current_config: &mut Option<GameConfig>,
    current_game: &str,
    action_tx: &tokio::sync::mpsc::Sender<GameEvent>,
) {
    info!("Received data event: {} = {}", key, value);

    let cfg = match current_config.as_ref() {
        Some(cfg) => cfg,
        None => {
            warn!("No active game configuration. Ignoring data event.");
            return;
        }
    };

    if cfg.players.count == 0 {
        warn!("No active game configuration. Ignoring data event.");
        return;
    }

    // Check if received data matches any configured signals
    let found = send_signal_actions(cfg, current_game, key, value, action_tx).await;

    if !found {
        info!("No matching signal found for: {} = {}", key, value);
        // Add signal to config for user to see and edit.
        if let Some(cfg) = current_config.as_mut() {
            cfg.signals.push(Signal {
                signal: key.to_string(),
                commands: vec![],
                player: crate::gamefile::PlayerSpec::All("___all".to_string()),
            });

            info!("Added new signal to configuration: {}", key);
            if cfg.read_only {
                warn!(
                    "Game file of '{}' could not be loaded, not saving the new signal.",
                    current_game
                );
            } else {
                save_game_file_or_notify(cfg, current_game);
            }
        }
    }
}
