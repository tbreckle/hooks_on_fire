use std::fs;
use std::path::Path;

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use tracing::info;

use crate::paths;
use crate::usb::{self, UsbId, UsbSerialPort};

/// Highest supported player number (players are numbered 1..=MAX_PLAYERS).
pub const MAX_PLAYERS: u8 = 4;

/// Connection details for a device in hof-config.yaml.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum DeviceConnectionDetails {
    /// Serial device. USB devices are identified by `usb-id` (and `usb-serial` if the device
    /// has a serial number); the port path is looked up on start, so re-enumeration of USB
    /// devices does not require reconfiguration. `port` is a fixed port path, only used if no
    /// `usb-id` is configured (older configs, non-USB ports).
    #[serde(rename = "serial")]
    Serial {
        #[serde(rename = "usb-id", default, skip_serializing_if = "Option::is_none")]
        usb_id: Option<UsbId>,
        #[serde(
            rename = "usb-serial",
            default,
            skip_serializing_if = "Option::is_none"
        )]
        usb_serial: Option<String>,
        #[serde(default, skip_serializing_if = "String::is_empty")]
        port: String,
    },
}

impl DeviceConnectionDetails {
    /// Not configured yet.
    pub fn empty_serial() -> Self {
        Self::Serial {
            usb_id: None,
            usb_serial: None,
            port: String::new(),
        }
    }

    /// Resolves the current serial port path (looks up USB devices in `ports`).
    pub fn resolve_serial_port(&self, ports: &[UsbSerialPort]) -> Result<String> {
        match self {
            Self::Serial {
                usb_id: Some(id),
                usb_serial,
                ..
            } => usb::resolve_port(ports, *id, usb_serial.as_deref()),
            Self::Serial { port, .. } if !port.is_empty() => Ok(port.clone()),
            Self::Serial { .. } => bail!("No serial port configured."),
        }
    }
}

/// Per-device entry in hof-config.yaml.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceEntry {
    pub name: String,
    #[serde(rename = "instance-name", alias = "instance_name")]
    pub instance_name: String,
    #[serde(rename = "connection-details", alias = "connection_details")]
    pub connection_details: DeviceConnectionDetails,
    /// Player (1..=MAX_PLAYERS) a light gun belongs to. Player-specific commands are only
    /// sent to the guns of that player; unassigned guns get them if no gun is assigned.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub player: Option<u8>,
}

/// Top-level configuration for Hooks on Fire.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HofConfig {
    #[serde(rename = "config-version", alias = "config_version")]
    pub config_version: u32,
    #[serde(default)]
    pub devices: Vec<DeviceEntry>,
    #[serde(rename = "tcp-host", alias = "tcp_host")]
    pub tcp_host: String,
    #[serde(rename = "tcp-port", alias = "tcp_port")]
    pub tcp_port: u16,
    #[serde(rename = "udp-broadcast-port", alias = "udp_broadcast_port")]
    pub udp_broadcast_port: u16,
}

impl Default for HofConfig {
    fn default() -> Self {
        Self {
            config_version: 1,
            devices: Vec::new(),
            tcp_host: "localhost".to_string(),
            tcp_port: 8000,
            udp_broadcast_port: 8001,
        }
    }
}

impl HofConfig {
    /// Loads the config from the default path, creating it with defaults if missing.
    pub fn load() -> Result<Self> {
        let path = paths::config_file()?;
        if path.exists() {
            Self::load_from(&path)
        } else {
            let config = Self::default();
            config.save()?;
            Ok(config)
        }
    }

    /// Loads the config from a specific path.
    pub fn load_from(path: &Path) -> Result<Self> {
        info!("Loading configuration from {}", path.display());
        let contents = fs::read_to_string(path)
            .with_context(|| format!("Failed to read config from {}", path.display()))?;
        let config: Self = serde_yaml_ng::from_str(&contents)
            .with_context(|| format!("Failed to parse config from {}", path.display()))?;
        Ok(config)
    }

    /// Saves the config to the default path.
    pub fn save(&self) -> Result<()> {
        let path = paths::config_file()?;
        self.save_to(&path)
    }

    /// Saves the config to a specific path, creating parent directories as needed.
    pub fn save_to(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).with_context(|| {
                format!("Failed to create config directory {}", parent.display())
            })?;
        }
        let yaml = serde_yaml_ng::to_string(self).context("Failed to serialize config")?;
        fs::write(path, yaml)
            .with_context(|| format!("Failed to write config to {}", path.display()))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serial_connection_yaml_round_trip() {
        let yaml = "type: serial\nusb-id: f143:0001\nusb-serial: E172B122FF5EA574\n";
        let details: DeviceConnectionDetails = serde_yaml_ng::from_str(yaml).unwrap();
        assert_eq!(serde_yaml_ng::to_string(&details).unwrap(), yaml);

        // Older configs with only a port path still load.
        let legacy: DeviceConnectionDetails =
            serde_yaml_ng::from_str("type: serial\nport: /dev/ttyACM6\n").unwrap();
        assert_eq!(legacy.resolve_serial_port(&[]).unwrap(), "/dev/ttyACM6");

        // An all-digit id must stay a string (no YAML 1.1 sexagesimal number).
        let digits: DeviceConnectionDetails =
            serde_yaml_ng::from_str("type: serial\nusb-id: 1234:5678\n").unwrap();
        let written = serde_yaml_ng::to_string(&digits).unwrap();
        let reread: DeviceConnectionDetails = serde_yaml_ng::from_str(&written).unwrap();
        assert_eq!(reread, digits);
    }
}
