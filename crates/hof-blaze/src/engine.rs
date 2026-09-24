use anyhow::Context;
use hof_common::data_files::{self, DataKind};
use hof_common::events::{GameEvent, StateEvent, TrayEvent};
use std::path::PathBuf;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};
use tokio::sync::oneshot;
use tracing::{debug, error, info, warn};

use crate::data_stats::DataStats;
use crate::devices::{ENTER_GAME_ACTION, LEAVE_GAME_ACTION};
use crate::gamefile::{GameConfig, Gamefile, Signal, STARTUP_SIGNAL, TEARDOWN_SIGNAL};
use crate::repeater::{Change, Repeater};

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
    tray_tx: tokio::sync::mpsc::Sender<TrayEvent>,
) -> anyhow::Result<EngineHandle> {
    let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();

    let thread_handle = std::thread::spawn(move || -> anyhow::Result<()> {
        // Use tokio runtime within the thread for async channel operations
        let rt = tokio::runtime::Runtime::new().context("Failed to create tokio runtime")?;

        rt.block_on(async {
            run_engine_loop(state_rx, game_rx, action_tx, tray_tx, shutdown_rx).await
        })
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
    tray_tx: tokio::sync::mpsc::Sender<TrayEvent>,
    mut shutdown_rx: oneshot::Receiver<()>,
) -> anyhow::Result<()> {
    let mut current_game = String::new();
    let mut current_config: Option<GameConfig> = None;
    let mut repeater = Repeater::default();
    let mut stats = DataStats::default();

    loop {
        // The branch below is disabled while nothing is held; the sleep is then never polled.
        let next_repeat = repeater.next_deadline();
        let repeat_sleep = tokio::time::sleep_until(tokio::time::Instant::from_std(
            next_repeat.unwrap_or_else(Instant::now),
        ));

        tokio::select! {
            _ = &mut shutdown_rx => {
                info!("Engine shutting down.");
                end_current_game(&mut current_config, &current_game, &mut repeater, &action_tx).await;
                stats.log();
                break;
            }
            _ = repeat_sleep, if next_repeat.is_some() => {
                send_repeats(current_config.as_ref(), &current_game, &mut repeater, &action_tx).await;
            }
            Some(state_event) = state_rx.recv() => {
                match state_event {
                    StateEvent::NewGame { game_name } => {
                        handle_new_game(&game_name, &mut current_game, &mut current_config, &mut repeater, &action_tx).await;
                    }
                    StateEvent::GameStopped => {
                        info!("Game stopped.");
                        end_current_game(&mut current_config, &current_game, &mut repeater, &action_tx).await;
                    }
                    StateEvent::GamePaused => {
                        info!("Game paused.");
                    }
                }
                // Show the running game in the tray status.
                let tray_event = match &current_config {
                    Some(cfg) => TrayEvent::GameStarted {
                        name: current_game.clone(),
                        display_name: cfg.display_name.clone(),
                    },
                    None => TrayEvent::GameEnded,
                };
                let _ = tray_tx.send(tray_event).await;
            }
            Some(game_event) = game_rx.recv() => {
                match game_event {
                    GameEvent::Data { key, value } => {
                        stats.record(&key, &value);
                        handle_data_event(&key, &value, &mut current_config, &current_game, &mut repeater, &action_tx).await;
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

/// Stops all repeats, fires the `___teardown` signal of the running game (if any), then the
/// `leave_game` device action, and clears the game.
async fn end_current_game(
    current_config: &mut Option<GameConfig>,
    current_game: &str,
    repeater: &mut Repeater,
    action_tx: &tokio::sync::mpsc::Sender<GameEvent>,
) {
    repeater.clear();
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
    repeater: &mut Repeater,
    action_tx: &tokio::sync::mpsc::Sender<GameEvent>,
) {
    info!("Engine received new game event: {}", game_name);
    if game_name.is_empty() {
        warn!("Game name is empty. Ignoring new game event.");
        return;
    }

    // Tear down the previous game while it is still the current game ({GAMENAME}).
    end_current_game(current_config, current_game, repeater, action_tx).await;

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

/// Sends an action event for every command of every signal matching `key` (used for the
/// fixed signals). Returns true if at least one signal matched.
async fn send_signal_actions(
    cfg: &GameConfig,
    game: &str,
    key: &str,
    value: &str,
    action_tx: &tokio::sync::mpsc::Sender<GameEvent>,
) -> bool {
    let mut found = false;
    for signal in cfg.signals.iter().filter(|s| s.signal == key) {
        found = true;
        send_entry_actions(cfg, signal, game, value, false, action_tx).await;
    }
    found
}

/// Sends an action event for every command of one signal entry. Repeated sends are logged at
/// debug level only, as they can come many times per second.
async fn send_entry_actions(
    cfg: &GameConfig,
    signal: &Signal,
    game: &str,
    value: &str,
    repeated: bool,
    action_tx: &tokio::sync::mpsc::Sender<GameEvent>,
) {
    debug!("Matched signal: {}", signal.signal);
    let player = match &signal.player {
        crate::gamefile::PlayerSpec::All(value) => value.clone(),
        crate::gamefile::PlayerSpec::Number(value) => value.to_string(),
    };
    debug!("Associated commands: {:?}", signal.commands);
    debug!("Associated player value: {}", player);
    debug!("Signal value: {}", value);

    for action in &signal.commands {
        if repeated {
            debug!(
                "Repeating action event: {} with value {} for player {}",
                action, value, player
            );
        } else {
            info!(
                "Sending action event: {} with value {} for player {}",
                action, value, player
            );
        }
        let _ = action_tx
            .send(GameEvent::Action {
                action: action.clone(),
                value: value.to_string(),
                player: player.clone(),
                game: game.to_string(),
                suppression: cfg.suppression.clone(),
            })
            .await;
    }
}

/// Sends the commands of all held repeating signals that are due.
async fn send_repeats(
    current_config: Option<&GameConfig>,
    current_game: &str,
    repeater: &mut Repeater,
    action_tx: &tokio::sync::mpsc::Sender<GameEvent>,
) {
    let Some(cfg) = current_config else {
        repeater.clear();
        return;
    };
    for (idx, value) in repeater.take_due(Instant::now()) {
        if let Some(signal) = cfg.signals.get(idx) {
            send_entry_actions(cfg, signal, current_game, &value, true, action_tx).await;
        }
    }
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
    repeater: &mut Repeater,
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
    let now = Instant::now();
    let mut found = false;
    for (idx, signal) in cfg.signals.iter().enumerate() {
        if signal.signal != key {
            continue;
        }
        found = true;
        let Some(repeat_ms) = signal.repeat else {
            send_entry_actions(cfg, signal, current_game, value, false, action_tx).await;
            continue;
        };
        // Repeating signal: send on press, then every `repeat` ms until released.
        match repeater.update(idx, Duration::from_millis(repeat_ms), value, now) {
            Change::Pressed => {
                info!(
                    "Signal {} held: repeating its commands every {} ms.",
                    key, repeat_ms
                );
                send_entry_actions(cfg, signal, current_game, value, false, action_tx).await;
            }
            Change::Released => info!("Signal {} released: stopped repeating.", key),
            Change::StillHeld | Change::NotHeld => {}
        }
    }

    if !found {
        info!("No matching signal found for: {} = {}", key, value);
        // Add signal to config for user to see and edit.
        if let Some(cfg) = current_config.as_mut() {
            cfg.signals.push(Signal {
                signal: key.to_string(),
                commands: vec![],
                player: crate::gamefile::PlayerSpec::All("___all".to_string()),
                repeat: None,
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

#[cfg(test)]
mod tests {
    use super::*;

    fn received_actions(rx: &mut tokio::sync::mpsc::Receiver<GameEvent>) -> Vec<String> {
        let mut actions = Vec::new();
        while let Ok(event) = rx.try_recv() {
            if let GameEvent::Action { action, value, .. } = event {
                actions.push(format!("{action}={value}"));
            }
        }
        actions
    }

    #[tokio::test]
    async fn repeating_signal_sends_on_press_and_repeats_until_release() {
        let config = Gamefile::parse_str(
            "players:\n  count: 2\nsignals:\n  - signal: P1_CtmRecoil\n    player: 1\n    repeat: 20\n    commands:\n      - recoil\n  - signal: Lamp\n    commands:\n      - lamp\n",
        )
        .unwrap();
        let mut current_config = Some(config);
        let mut repeater = Repeater::default();
        let (tx, mut rx) = tokio::sync::mpsc::channel(32);

        handle_data_event(
            "P1_CtmRecoil",
            "1",
            &mut current_config,
            "test",
            &mut repeater,
            &tx,
        )
        .await;
        assert_eq!(received_actions(&mut rx), ["recoil=1"]);
        // Held: a second 1 does not send again.
        handle_data_event(
            "P1_CtmRecoil",
            "1",
            &mut current_config,
            "test",
            &mut repeater,
            &tx,
        )
        .await;
        assert!(received_actions(&mut rx).is_empty());

        tokio::time::sleep(Duration::from_millis(25)).await;
        send_repeats(current_config.as_ref(), "test", &mut repeater, &tx).await;
        assert_eq!(received_actions(&mut rx), ["recoil=1"]);

        // Release: nothing is sent, repeats stop.
        handle_data_event(
            "P1_CtmRecoil",
            "0",
            &mut current_config,
            "test",
            &mut repeater,
            &tx,
        )
        .await;
        assert!(received_actions(&mut rx).is_empty());
        assert_eq!(repeater.next_deadline(), None);

        // Signals without `repeat` still send on every value.
        handle_data_event("Lamp", "0", &mut current_config, "test", &mut repeater, &tx).await;
        assert_eq!(received_actions(&mut rx), ["lamp=0"]);
    }
}
