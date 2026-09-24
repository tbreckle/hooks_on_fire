// Windows: run without a console window (hof-blaze is a tray app; output goes to the log file).
#![cfg_attr(windows, windows_subsystem = "windows")]

#[cfg(test)]
mod data_check;
mod data_stats;
mod devices;
mod engine;
mod gamefile;
mod line_processor;
mod logging;
mod serial;
mod tcp_connector;
mod tray;
mod udp_receiver;

use std::sync::Arc;

use anyhow::Context;

use hof_common::config::HofConfig;
use hof_common::events::LineEvent;
use hof_common::instance_lock::{self, InstanceLock};
use hof_common::{build_info, events::TrayEvent, switch};
use tracing::{debug, error, info, warn};

use crate::devices::{Device, DeviceRegistry, DeviceType};
use crate::engine::{start_engine, EngineHandle};
use crate::serial::SerialManager;
use crate::tcp_connector::{start_tcp_connector, TcpConnectorHandle};
use crate::tray::TrayExit;
use crate::udp_receiver::{start_udp_receiver, UdpReceiverHandle};
use hof_common::events::{GameEvent, StateEvent};
use tokio::sync::mpsc;

fn main() -> anyhow::Result<()> {
    // Keep the guard until the end of main so the last log lines are written.
    let _log_guard = logging::init();

    // The main thread is reserved for the UI (startup message box, tray): macOS only allows it
    // there. Everything else runs on the runtime's worker threads.
    let runtime = match tokio::runtime::Runtime::new().context("Failed to start the Tokio runtime")
    {
        Ok(runtime) => runtime,
        Err(err) => {
            error!("hof-blaze failed to start: {err:#}");
            show_error("Hooks on Fire - Blaze could not start", &err);
            return Err(err);
        }
    };

    let mut blaze = match runtime.block_on(Blaze::start()) {
        Ok(blaze) => blaze,
        Err(err) => {
            error!("hof-blaze failed to start: {err:#}");
            show_error("Hooks on Fire - Blaze could not start", &err);
            return Err(err);
        }
    };

    // Send startup notification.
    let _ = notify_rust::Notification::new()
        .summary(&format!("Hooks on Fire {}", blaze.version_string))
        .body("Blaze started.\n\nWaiting for connections...")
        .show();

    info!("Application running. Click Exit in the tray menu to quit.");
    let tray_rx = blaze
        .tray_rx
        .take()
        .expect("tray receiver is taken only once");
    let tray_result = tray::run(runtime.handle(), tray_rx, &blaze.version_string);
    if let Err(err) = &tray_result {
        error!("Tray failed: {err:#}");
    }

    // Shutting down releases the instance lock, so hof-forge is started only afterwards.
    let result = runtime.block_on(blaze.shutdown());
    if let Ok(TrayExit::SwitchToForge(path)) = &tray_result {
        if let Err(err) = switch::launch(path) {
            error!("Failed to start hof-forge: {err:#}");
            show_error("Hooks on Fire - Forge could not start", &err);
        }
    }
    let result = result.and(tray_result.map(|_| ()));
    if let Err(err) = &result {
        error!("hof-blaze stopped with an error: {err:#}");
    }
    result
}

/// Shows an error in a message box and waits until the user presses OK.
///
/// Must not be called while the tray is running: on Linux both use GTK, which must not be
/// driven from two threads.
fn show_error(title: &str, err: &anyhow::Error) {
    // Without a display GTK cannot start and the dialog would block forever (e.g. over SSH).
    #[cfg(target_os = "linux")]
    if std::env::var_os("DISPLAY").is_none() && std::env::var_os("WAYLAND_DISPLAY").is_none() {
        return;
    }

    rfd::MessageDialog::new()
        .set_level(rfd::MessageLevel::Error)
        .set_title(title)
        .set_description(format!("{err:#}"))
        .set_buttons(rfd::MessageButtons::Ok)
        .show();
}

/// Running application: everything that has been started successfully.
struct Blaze {
    _lock: InstanceLock,
    version_string: String,
    devices: Arc<DeviceRegistry>,
    serial: Arc<SerialManager>,
    /// Taken by the tray when it starts.
    tray_rx: Option<mpsc::Receiver<TrayEvent>>,
    keep_alive_tx: mpsc::Sender<GameEvent>,
    udp_handle: UdpReceiverHandle,
    tcp_handle: TcpConnectorHandle,
    engine_handle: EngineHandle,
    action_router_handle: tokio::task::JoinHandle<()>,
}

impl Blaze {
    /// Startup phase: everything that can fail on start. Errors returned here are shown to
    /// the user in a message box, so the tray (GTK on Linux) is only started afterwards.
    /// Runs on the Tokio runtime, while `main` shows the message box on the main thread.
    async fn start() -> anyhow::Result<Self> {
        let version_string = build_info::format_version(
            env!("CARGO_PKG_VERSION"),
            env!("HOF_GIT_HASH"),
            env!("HOF_BUILD_DATE"),
        );

        println!(
            r#"
  _    _             _                        ______ _
 | |  | |           | |                      |  ____(_)
 | |__| | ___   ___ | | _____    ___  _ __   | |__   _ _ __ ___
 |  __  |/ _ \ / _ \| |/ / __|  / _ \| '_ \  |  __| | | '__/ _ \
 | |  | | (_) | (_) |   <\__ \ | (_) | | | | | |    | | | |  __/
 |_|  |_|\___/ \___/|_|\_\___/  \___/|_| |_| |_|    |_|_|  \___|
                                                          v{}
"#,
            env!("CARGO_PKG_VERSION")
        );

        info!("hof-blaze {version_string}");

        // Make sure only one instance of the application is running.
        let lock = InstanceLock::acquire(
            "hof-blaze",
            "hof-forge",
            instance_lock::lock_wait_from_args(),
        )?;

        let config = HofConfig::load()?;
        info!("Configuration loaded.");

        if config.devices.is_empty() {
            anyhow::bail!("No devices configured.\n\nAdd devices with hof-forge and restart.");
        }

        let devices = Arc::new(DeviceRegistry::load(&config.devices)?);

        // Open serial connections for all devices.
        let serial = Arc::new(SerialManager::open(&devices).context("Failed to open serial port")?);

        // Send setup commands to all devices.
        if let Err(err) = serial.send_setup(&devices) {
            warn!("Error during device setup: {:#}", err);
        }

        // The devices are set up from here on: if anything below fails, tear them down again.
        let services = (|| -> anyhow::Result<_> {
            let (line_tx, line_rx) = mpsc::channel::<LineEvent>(32);
            let (tray_tx, tray_rx) = mpsc::channel::<TrayEvent>(32);
            let (state_tx, state_rx) = mpsc::channel::<StateEvent>(32);
            let (game_tx, game_rx) = mpsc::channel::<GameEvent>(32);
            let (action_tx, action_rx) = mpsc::channel::<GameEvent>(32);
            let keep_alive_tx = action_tx.clone(); // Keep a sender alive for the engine to prevent it from exiting.

            tokio::spawn(line_processor::run_line_processor(
                line_rx, state_tx, game_tx,
            ));

            let line_tx_clone = line_tx.clone();
            let udp_handle = start_udp_receiver(line_tx, config.udp_broadcast_port)?;
            let tcp_handle = start_tcp_connector(
                line_tx_clone,
                config.tcp_host.clone(),
                config.tcp_port,
                tray_tx,
            )?;
            let engine_handle = start_engine(state_rx, game_rx, action_tx)?;
            Ok((
                tray_rx,
                action_rx,
                keep_alive_tx,
                udp_handle,
                tcp_handle,
                engine_handle,
            ))
        })();
        let (tray_rx, action_rx, keep_alive_tx, udp_handle, tcp_handle, engine_handle) =
            match services {
                Ok(services) => services,
                Err(err) => {
                    if let Err(teardown_err) = serial.send_teardown(&devices) {
                        warn!("Error during device teardown: {:#}", teardown_err);
                    }
                    return Err(err);
                }
            };

        // Spawn action receivers
        let action_router_handle = tokio::spawn(run_action_router(
            action_rx,
            Arc::clone(&devices),
            Arc::clone(&serial),
        ));

        Ok(Self {
            _lock: lock,
            version_string,
            devices,
            serial,
            tray_rx: Some(tray_rx),
            keep_alive_tx,
            udp_handle,
            tcp_handle,
            engine_handle,
            action_router_handle,
        })
    }

    /// Shuts everything down after the tray has exited.
    async fn shutdown(self) -> anyhow::Result<()> {
        info!("Exit requested, shutting down...");

        self.udp_handle.shutdown()?;
        self.tcp_handle.shutdown()?;
        // Stopping the engine fires the ___teardown signal of the running game.
        self.engine_handle.shutdown()?;

        // Let the action router deliver the remaining actions before tearing down the devices.
        drop(self.keep_alive_tx);
        if let Err(err) = self.action_router_handle.await {
            warn!("Action router task failed: {:#}", err);
        }

        // Send teardown commands before closing.
        if let Err(err) = self.serial.send_teardown(&self.devices) {
            warn!("Error during device teardown: {:#}", err);
        }

        info!("All threads exited. Goodbye!");

        Ok(())
    }
}

/// Routes action events to appropriate receivers based on command type
async fn run_action_router(
    mut action_rx: mpsc::Receiver<GameEvent>,
    devices: Arc<DeviceRegistry>,
    serial: Arc<SerialManager>,
) {
    info!("Action router started");

    while let Some(event) = action_rx.recv().await {
        match event {
            GameEvent::Action {
                action,
                value,
                player,
                game,
                suppression,
            } => {
                // "___all" (not a number) addresses all players; {PLAYER} then becomes 0.
                let target_player: Option<u8> = player.parse().ok();
                let player_param = target_player.map_or("0".to_string(), |p| p.to_string());

                // Devices suppressed by the game count as not configured.
                if devices.has_action_for_type(&action, &DeviceType::LightController, &suppression)
                {
                    handle_lightcontroller_action(
                        &devices,
                        &serial,
                        &action,
                        &value,
                        &player_param,
                        &game,
                        &suppression,
                    );
                } else if devices.has_action_for_type(&action, &DeviceType::LightGun, &suppression)
                {
                    handle_lightgun_action(
                        devices.light_guns_for_player(target_player, &suppression),
                        &serial,
                        &action,
                        &value,
                        &player_param,
                        &game,
                    );
                } else if devices.has_action(&action) {
                    debug!(
                        "Action '{}' is only configured in devices suppressed by the game, skipping.",
                        action
                    );
                } else {
                    warn!(
                        "Unknown action command: '{}' - not configured in any device",
                        action
                    );
                }
            }
            GameEvent::DeviceAction {
                action,
                game,
                suppression,
            } => {
                handle_device_action(&devices, &serial, &action, &game, &suppression);
            }
            GameEvent::Data { .. } => {}
        }
    }

    info!("Action router stopped");
}

/// Sends a device-level action (e.g. `enter_game`) to every device that has it configured
/// and is not suppressed by the game. `{PLAYER}` is 0 (all players) and `{VALUE}` is empty.
fn handle_device_action(
    devices: &DeviceRegistry,
    serial: &SerialManager,
    action: &str,
    game: &str,
    suppression: &[String],
) {
    for device in devices.devices() {
        if device.action(action).is_none() {
            continue;
        }
        if device.is_suppressed(suppression) {
            info!(
                "Device '{}': suppressed by game '{}', not sending '{}'",
                device.instance_name(),
                game,
                action
            );
            continue;
        }
        info!(
            "Device '{}': sending '{}' for game '{}'",
            device.instance_name(),
            action,
            game
        );
        if let Err(err) = serial.send_device_action(device, action, "", "0", game) {
            warn!(
                "Device '{}': failed to send action '{}': {:#}",
                device.instance_name(),
                action,
                err
            );
        }
    }
}

/// Handles lightcontroller actions by dispatching to all lightcontroller devices.
/// The player is only passed on as `{PLAYER}` parameter.
fn handle_lightcontroller_action(
    devices: &DeviceRegistry,
    serial: &SerialManager,
    action: &str,
    value: &str,
    player_param: &str,
    game: &str,
    suppression: &[String],
) {
    let controllers = devices.active_by_type(&DeviceType::LightController, suppression);
    if controllers.is_empty() {
        debug!("[LIGHTCONTROLLER] No lightcontroller devices configured, skipping.");
        return;
    }

    for device in controllers {
        match device.action(action) {
            Some(_) => {
                info!(
                    "[LIGHTCONTROLLER] Device '{}': processing action '{}'",
                    device.instance_name(),
                    action
                );
                if let Err(err) =
                    serial.send_device_action(device, action, value, player_param, game)
                {
                    warn!(
                        "[LIGHTCONTROLLER] Device '{}': failed to send action '{}': {:#}",
                        device.instance_name(),
                        action,
                        err
                    );
                }
            }
            None => {
                debug!(
                    "[LIGHTCONTROLLER] Device '{}': action '{}' not configured, skipping.",
                    device.instance_name(),
                    action
                );
            }
        }
    }
}

/// Handles lightgun actions by dispatching to `guns`, the light guns of the addressed player
/// (see `DeviceRegistry::light_guns_for_player`).
fn handle_lightgun_action(
    guns: Vec<&Device>,
    serial: &SerialManager,
    action: &str,
    value: &str,
    player_param: &str,
    game: &str,
) {
    if guns.is_empty() {
        debug!(
            "[LIGHTGUN] No lightgun for player {}, skipping action '{}'.",
            player_param, action
        );
        return;
    }

    for device in guns {
        match device.action(action) {
            Some(_) => {
                debug!(
                    "[LIGHTGUN] Device '{}': processing action '{}'",
                    device.instance_name(),
                    action
                );
                if let Err(err) =
                    serial.send_device_action(device, action, value, player_param, game)
                {
                    warn!(
                        "[LIGHTGUN] Device '{}': failed to send action '{}': {:#}",
                        device.instance_name(),
                        action,
                        err
                    );
                }
            }
            None => {
                debug!(
                    "[LIGHTGUN] Device '{}': action '{}' not configured, skipping.",
                    device.instance_name(),
                    action
                );
            }
        }
    }
}
