use hof_common::events::{GameEvent, LineEvent, StateEvent};
use tokio::sync::mpsc;
use tracing::{debug, error, info};

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
            LineEvent::Disconnected => {
                // The source closed the connection without sending `mame_stop`.
                if let Some(name) = game_name.take() {
                    info!(
                        "Connection lost while '{}' was running. Stopping game.",
                        name
                    );
                    game_running = false;
                    let _ = state_tx.send(StateEvent::GameStopped).await;
                }
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
            *game_name = None;
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Feeds `events` to the line processor and returns the state events it sent.
    async fn state_events(events: Vec<LineEvent>) -> Vec<StateEvent> {
        let (line_tx, line_rx) = mpsc::channel(32);
        let (state_tx, mut state_rx) = mpsc::channel(32);
        let (game_tx, _game_rx) = mpsc::channel(32);
        for event in events {
            line_tx.send(event).await.unwrap();
        }
        drop(line_tx);
        run_line_processor(line_rx, state_tx, game_tx).await;
        let mut result = Vec::new();
        while let Ok(event) = state_rx.try_recv() {
            result.push(event);
        }
        result
    }

    fn line(line: &str) -> LineEvent {
        LineEvent::NewLine { line: line.into() }
    }

    #[tokio::test]
    async fn disconnect_stops_running_game() {
        let events = state_events(vec![line("mame_start=lostwsga"), LineEvent::Disconnected]).await;
        assert!(matches!(
            events.as_slice(),
            [StateEvent::NewGame { .. }, StateEvent::GameStopped]
        ));
    }

    #[tokio::test]
    async fn disconnect_without_running_game_is_ignored() {
        let events = state_events(vec![
            LineEvent::Disconnected,
            line("mame_start=lostwsga"),
            line("mame_stop=1"),
            LineEvent::Disconnected,
        ])
        .await;
        assert!(matches!(
            events.as_slice(),
            [StateEvent::NewGame { .. }, StateEvent::GameStopped]
        ));
    }
}
