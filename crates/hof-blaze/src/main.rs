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
use hof_common::instance_lock::InstanceLock;
use hof_common::{build_info, events::TrayEvent};
use tracing::{debug, error, info, warn};

use crate::devices::{DeviceRegistry, DeviceType};
use crate::engine::{start_engine, EngineHandle};
use crate::serial::SerialManager;
use crate::tcp_connector::{start_tcp_connector, TcpConnectorHandle};
use crate::tray::TrayHandle;
use crate::udp_receiver::{start_udp_receiver, UdpReceiverHandle};
use hof_common::events::{GameEvent, StateEvent};
use tokio::sync::mpsc;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Keep the guard until the end of main so the last log lines are written.
    let _log_guard = logging::init();

    let blaze = match Blaze::start().await {
        Ok(blaze) => blaze,
        Err(err) => {
            error!("hof-blaze failed to start: {err:#}");
            show_startup_error(&err);
            return Err(err);
        }
    };

    let result = blaze.run().await;
    if let Err(err) = &result {
        error!("hof-blaze stopped with an error: {err:#}");
    }
    result
}

/// Shows a startup error in a message box and waits until the user presses OK.
///
/// Must only be called before the tray is started: on Linux both use GTK, which must not be
/// driven from two threads.
fn show_startup_error(err: &anyhow::Error) {
    // Without a display GTK cannot start and the dialog would block forever (e.g. over SSH).
    #[cfg(target_os = "linux")]
    if std::env::var_os("DISPLAY").is_none() && std::env::var_os("WAYLAND_DISPLAY").is_none() {
        return;
    }

    rfd::MessageDialog::new()
        .set_level(rfd::MessageLevel::Error)
        .set_title("Hooks on Fire - Blaze could not start")
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
    tray_rx: mpsc::Receiver<TrayEvent>,
    keep_alive_tx: mpsc::Sender<GameEvent>,
    udp_handle: UdpReceiverHandle,
    tcp_handle: TcpConnectorHandle,
    engine_handle: EngineHandle,
    action_router_handle: tokio::task::JoinHandle<()>,
}

impl Blaze {
    /// Startup phase: everything that can fail on start. Errors returned here are shown to
    /// the user in a message box, so the tray (GTK on Linux) is only started afterwards.
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
        let lock = InstanceLock::acquire("hof-blaze", "hof-forge")?;

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
            tray_rx,
            keep_alive_tx,
            udp_handle,
            tcp_handle,
            engine_handle,
            action_router_handle,
        })
    }

    /// Starts the tray and runs until exit is requested, then shuts everything down.
    async fn run(self) -> anyhow::Result<()> {
        let mut tray_handle: TrayHandle =
            tray::start_tray(self.tray_rx, self.version_string.clone()).await?;
        // Send startup notification.
        let _ = notify_rust::Notification::new()
            .summary(&format!("Hooks on Fire {}", self.version_string))
            .body("Blaze started.\n\nWaiting for connections...")
            .show();

        // Wait for exit signal from tray menu
        info!("Application running. Click Exit in the tray menu to quit.");
        tray_handle.wait_for_exit().await;

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

        tray_handle.shutdown()?;

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
            } => {
                // "___all" (not a number) addresses all players; {PLAYER} then becomes 0.
                let target_player: Option<u8> = player.parse().ok();
                let player_param = target_player.map_or("0".to_string(), |p| p.to_string());

                if devices.has_action_for_type(&action, &DeviceType::LightController) {
                    handle_lightcontroller_action(
                        &devices,
                        &serial,
                        &action,
                        &value,
                        &player_param,
                        &game,
                    );
                } else if devices.has_action_for_type(&action, &DeviceType::LightGun) {
                    handle_lightgun_action(
                        &devices,
                        &serial,
                        &action,
                        &value,
                        target_player,
                        &player_param,
                        &game,
                    );
                } else {
                    warn!(
                        "Unknown action command: '{}' - not configured in any device",
                        action
                    );
                }
            }
            GameEvent::DeviceAction { action, game } => {
                handle_device_action(&devices, &serial, &action, &game);
            }
            GameEvent::Data { .. } => {}
        }
    }

    info!("Action router stopped");
}

/// Sends a device-level action (e.g. `enter_game`) to every device that has it configured.
/// `{PLAYER}` is 0 (all players) and `{VALUE}` is empty.
fn handle_device_action(
    devices: &DeviceRegistry,
    serial: &SerialManager,
    action: &str,
    game: &str,
) {
    for device in devices.devices() {
        if device.action(action).is_none() {
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
) {
    let controllers = devices.get_by_type(&DeviceType::LightController);
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

/// Handles lightgun actions by dispatching to the light guns of the addressed player
/// (see `DeviceRegistry::light_guns_for_player`).
fn handle_lightgun_action(
    devices: &DeviceRegistry,
    serial: &SerialManager,
    action: &str,
    value: &str,
    target_player: Option<u8>,
    player_param: &str,
    game: &str,
) {
    let guns = devices.light_guns_for_player(target_player);
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
