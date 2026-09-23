use std::collections::HashMap;
use std::io::Write;
use std::sync::Mutex;
use std::time::Duration;

use anyhow::{Context, Result};
use tracing::info;

use crate::devices::{Device, DeviceRegistry};

/// A single open serial port.
struct SerialPort {
    port: Box<dyn serialport::SerialPort>,
}

/// Manages serial connections for all devices.
/// Devices sharing the same port path reuse a single connection.
pub struct SerialManager {
    /// port path → open serial port
    connections: HashMap<String, Mutex<SerialPort>>,
}

impl SerialManager {
    /// Open serial connections for all devices in the registry.
    /// Devices on the same port path share a connection (must use the same speed).
    pub fn open(devices: &DeviceRegistry) -> Result<Self> {
        let mut connections: HashMap<String, Mutex<SerialPort>> = HashMap::new();
        // Track port → speed to detect conflicts.
        let mut port_speeds: HashMap<String, (u32, String)> = HashMap::new();

        for device in devices.devices() {
            let port_path = device.serial_port();
            let speed = device.connection_details().speed;

            if let Some((existing_speed, first_device)) = port_speeds.get(port_path) {
                if *existing_speed != speed {
                    anyhow::bail!(
                        "Speed conflict on serial port '{}': device '{}' wants {} baud, \
                         but device '{}' already configured {} baud",
                        port_path,
                        device.instance_name(),
                        speed,
                        first_device,
                        existing_speed
                    );
                }
                // Already open, skip.
                continue;
            }

            info!(
                "Opening serial port '{}' at {} baud (device: '{}')",
                port_path,
                speed,
                device.instance_name()
            );

            // Check if the serial port device exists on the filesystem (Linux/macOS).
            #[cfg(unix)]
            {
                let path = std::path::Path::new(port_path);
                if !path.exists() {
                    anyhow::bail!(
                        "Serial port '{}' does not exist (device: '{}'). Check your configuration.",
                        port_path,
                        device.instance_name()
                    );
                }
            }

            let port = serialport::new(port_path, speed)
                .timeout(Duration::from_secs(1))
                .open()
                .with_context(|| {
                    format!(
                        "Failed to open serial port '{}' at {} baud (device: '{}')",
                        port_path,
                        speed,
                        device.instance_name()
                    )
                })?;

            info!("Serial port '{}' opened successfully.", port_path);

            port_speeds.insert(
                port_path.to_string(),
                (speed, device.instance_name().to_string()),
            );
            connections.insert(port_path.to_string(), Mutex::new(SerialPort { port }));
        }

        Ok(SerialManager { connections })
    }

    /// Send a list of commands to a specific serial port with a delay between each.
    fn send_commands(&self, port_path: &str, commands: &[String], delay_ms: u64) -> Result<()> {
        let port_mutex = self
            .connections
            .get(port_path)
            .with_context(|| format!("No open connection for serial port '{}'", port_path))?;

        let mut port = port_mutex
            .lock()
            .map_err(|e| anyhow::anyhow!("Serial port mutex poisoned: {}", e))?;

        for cmd in commands {
            info!("Serial TX [{}]: {}", port_path, cmd);
            port.port
                .write_all(cmd.as_bytes())
                .with_context(|| format!("Failed to write command '{}' to '{}'", cmd, port_path))?;
            port.port
                .write_all(b"\n")
                .with_context(|| format!("Failed to write newline to '{}'", port_path))?;
            port.port
                .flush()
                .with_context(|| format!("Failed to flush '{}'", port_path))?;

            if delay_ms > 0 && commands.len() > 1 {
                std::thread::sleep(Duration::from_millis(delay_ms));
            }
        }

        Ok(())
    }

    /// Send setup commands for all devices that have them configured.
    pub fn send_setup(&self, devices: &DeviceRegistry) -> Result<()> {
        for device in devices.devices() {
            if let Some(commands) = device.action("setup") {
                if commands.is_empty() {
                    continue;
                }
                info!(
                    "Sending setup commands for device '{}' ({})",
                    device.instance_name(),
                    device.display_name()
                );
                self.send_commands(device.serial_port(), commands, device.command_delay_ms())?;
            }
        }
        Ok(())
    }

    /// Send teardown commands for all devices that have them configured.
    pub fn send_teardown(&self, devices: &DeviceRegistry) -> Result<()> {
        for device in devices.devices() {
            if let Some(commands) = device.action("teardown") {
                if commands.is_empty() {
                    continue;
                }
                info!(
                    "Sending teardown commands for device '{}' ({})",
                    device.instance_name(),
                    device.display_name()
                );
                self.send_commands(device.serial_port(), commands, device.command_delay_ms())?;
            }
        }
        Ok(())
    }

    /// Send an action's commands for a specific device, with placeholder substitution.
    pub fn send_device_action(
        &self,
        device: &Device,
        action: &str,
        value: &str,
        player: &str,
        game: &str,
    ) -> Result<()> {
        if let Some(cmd_templates) = device.action(action) {
            let commands: Vec<String> = cmd_templates
                .iter()
                .map(|t| {
                    t.replace("{PLAYER}", player)
                        .replace("{VALUE}", value)
                        .replace("{GAMENAME}", game)
                })
                .collect();
            self.send_commands(device.serial_port(), &commands, device.command_delay_ms())?;
        }
        Ok(())
    }
}
