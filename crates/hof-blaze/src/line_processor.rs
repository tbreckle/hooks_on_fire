use hof_common::events::{GameEvent, LineEvent, StateEvent};
use tokio::sync::mpsc;
use tracing::{debug, error};

pub async fn run_line_processor(
    mut rx: mpsc::Receiver<LineEvent>,
    state_tx: mpsc::Sender<StateEvent>,
    game_tx: mpsc::Sender<GameEvent>,
) {
    // let mut stock_count = 100; // Private state, no Mutex!
    let mut game_running: bool = false;
    let mut game_name: Option<String> = None;

    while let Some(event) = rx.recv().await {
        match event {
            LineEvent::NewLine { line } => {
                debug!("Received line: {}", line);
                process_data(
                    &line,
                    &mut game_running,
                    &mut game_name,
                    &state_tx,
                    &game_tx,
                )
                .await;
            }
        }
    }
}

async fn process_data(
    line: &str,
    game_running: &mut bool,
    game_name: &mut Option<String>,
    state_tx: &mpsc::Sender<StateEvent>,
    game_tx: &mpsc::Sender<GameEvent>,
) {
    // Parse the line and update internal state
    debug!("Processing line: {}", line);
    let trimmed = line.trim();
    if trimmed.is_empty() {
        error!("Received empty line. Ignoring.");
        return;
    }

    let (key, value) = match trimmed.split_once('=') {
        Some((key, value)) => (key.trim(), value.trim()),
        None => {
            error!("Received line without '=' character. Ignoring.");
            return;
        }
    };

    if key.is_empty() {
        error!("Received line with empty key. Ignoring.");
        return;
    }

    process_key(key, value, game_running, game_name, state_tx, game_tx).await;
}

async fn process_key(
    key: &str,
    value: &str,
    game_running: &mut bool,
    game_name: &mut Option<String>,
    state_tx: &mpsc::Sender<StateEvent>,
    game_tx: &mpsc::Sender<GameEvent>,
) {
    // Handle specific keys.
    match key {
        "mame_start" | "game" => {
            if value.is_empty() {
                error!("Received start signal with empty game name. Ignoring.");
                return;
            }
            if value == "___empty" {
                debug!("Received mame_start with ___empty (MAME exited game). Ignoring.");
                return;
            }
            debug!("Starting new game: {}", value);
            *game_running = true;
            *game_name = Some(value.to_string());
            let _ = state_tx
                .send(StateEvent::NewGame {
                    game_name: value.to_string(),
                })
                .await;
        }
        "mame_stop" => {
            debug!("Stopping game.");
            *game_running = false;
            let _ = state_tx.send(StateEvent::GameStopped).await;
        }
        "pause" => {
            debug!("Pausing game.");
            *game_running = false;
            let _ = state_tx.send(StateEvent::GamePaused).await;
        }
        "tcp" => {
            debug!("Received TCP port: {}", value);
        }
        _ => {
            debug!("Received data event: {} = {}", key, value);
            let _ = game_tx
                .send(GameEvent::Data {
                    key: key.to_string(),
                    value: value.to_string(),
                })
                .await;
        }
    }
}
