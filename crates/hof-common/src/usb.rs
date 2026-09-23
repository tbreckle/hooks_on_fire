//! USB serial device identification. Devices are stored by USB vendor/product id (plus serial
//! number if available) instead of the port path, which can change when USB devices are
//! re-enumerated (e.g. `/dev/ttyACM1` → `/dev/ttyACM3`, `COM3` → `COM5`).

use std::fmt;
use std::str::FromStr;

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use serialport::{SerialPortInfo, SerialPortType};
use tracing::warn;

/// USB vendor and product id, written as `vvvv:pppp` in hex (like `lsusb`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct UsbId {
    pub vid: u16,
    pub pid: u16,
}

impl fmt::Display for UsbId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:04x}:{:04x}", self.vid, self.pid)
    }
}

impl FromStr for UsbId {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self> {
        let (vid, pid) = s
            .split_once(':')
            .with_context(|| format!("Invalid USB id '{s}', expected 'vvvv:pppp' (hex)"))?;
        let parse = |part: &str| {
            u16::from_str_radix(part.trim(), 16)
                .with_context(|| format!("Invalid USB id '{s}', expected 'vvvv:pppp' (hex)"))
        };
        Ok(Self {
            vid: parse(vid)?,
            pid: parse(pid)?,
        })
    }
}

impl TryFrom<String> for UsbId {
    type Error = anyhow::Error;

    fn try_from(s: String) -> Result<Self> {
        s.parse()
    }
}

impl From<UsbId> for String {
    fn from(id: UsbId) -> Self {
        id.to_string()
    }
}

/// A connected USB serial device.
#[derive(Debug, Clone)]
pub struct UsbSerialPort {
    /// Current OS port path (e.g. `/dev/ttyACM0`, `COM3`, `/dev/cu.usbmodem1101`).
    pub path: String,
    pub id: UsbId,
    pub serial_number: Option<String>,
    pub manufacturer: Option<String>,
    pub product: Option<String>,
}

impl UsbSerialPort {
    /// Human-readable device name from manufacturer and product.
    pub fn name(&self) -> String {
        let name = match (self.manufacturer.as_deref(), self.product.as_deref()) {
            (Some(m), Some(p)) if p.starts_with(m) => p.to_string(),
            (Some(m), Some(p)) => format!("{m} {p}"),
            (None, Some(p)) => p.to_string(),
            (Some(m), None) => m.to_string(),
            (None, None) => "USB serial device".to_string(),
        };
        name.trim().to_string()
    }
}

/// Lists all currently connected USB serial devices (Windows, Linux and macOS),
/// sorted by port path.
pub fn usb_serial_ports() -> Vec<UsbSerialPort> {
    let ports = serialport::available_ports().unwrap_or_else(|err| {
        warn!("Failed to enumerate serial ports: {err}");
        Vec::new()
    });

    let mut result: Vec<UsbSerialPort> = ports
        .into_iter()
        .filter(is_preferred_path)
        .filter_map(|port| match port.port_type {
            SerialPortType::UsbPort(usb) => Some(UsbSerialPort {
                path: port.port_name,
                id: UsbId {
                    vid: usb.vid,
                    pid: usb.pid,
                },
                serial_number: usb.serial_number.filter(|s| !s.trim().is_empty()),
                manufacturer: usb.manufacturer,
                product: usb.product,
            }),
            _ => None,
        })
        .collect();

    result.sort_by(|a, b| natural_key(&a.path).cmp(&natural_key(&b.path)));
    result.dedup_by(|a, b| a.path == b.path);
    result
}

/// Whether `port` is the device identified by `id` and (if given) `serial_number`.
pub fn matches(port: &UsbSerialPort, id: UsbId, serial_number: Option<&str>) -> bool {
    port.id == id && serial_number.is_none_or(|s| port.serial_number.as_deref() == Some(s))
}

/// Finds the current port path of the device identified by `id` and (if given) `serial_number`.
pub fn resolve_port(
    ports: &[UsbSerialPort],
    id: UsbId,
    serial_number: Option<&str>,
) -> Result<String> {
    let found: Vec<&UsbSerialPort> = ports
        .iter()
        .filter(|p| matches(p, id, serial_number))
        .collect();
    let device = match serial_number {
        Some(serial) => format!("USB device {id} (serial number {serial})"),
        None => format!("USB device {id}"),
    };
    match found.as_slice() {
        [] => bail!("{device} is not connected."),
        [port] => Ok(port.path.clone()),
        _ => {
            let paths: Vec<&str> = found.iter().map(|p| p.path.as_str()).collect();
            bail!(
                "{device} is connected more than once ({}) and cannot be told apart.",
                paths.join(", ")
            )
        }
    }
}

/// macOS exposes every device twice: `/dev/tty.*` (dial-in) and `/dev/cu.*` (call-out).
/// Only the call-out device is useful for talking to a device, so hide the other one.
#[cfg(target_os = "macos")]
fn is_preferred_path(port: &SerialPortInfo) -> bool {
    !port.port_name.starts_with("/dev/tty.")
}

#[cfg(not(target_os = "macos"))]
fn is_preferred_path(_port: &SerialPortInfo) -> bool {
    true
}

/// Sort key so that `COM2` comes before `COM10` and `ttyACM2` before `ttyACM10`.
fn natural_key(path: &str) -> (String, u64) {
    let digits = path.len() - path.trim_end_matches(|c: char| c.is_ascii_digit()).len();
    let (prefix, number) = path.split_at(path.len() - digits);
    (prefix.to_string(), number.parse().unwrap_or(0))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn port(path: &str, id: &str, serial: Option<&str>) -> UsbSerialPort {
        UsbSerialPort {
            path: path.to_string(),
            id: id.parse().unwrap(),
            serial_number: serial.map(str::to_string),
            manufacturer: None,
            product: None,
        }
    }

    #[test]
    fn parses_and_formats_usb_id() {
        let id: UsbId = "F143:0002".parse().unwrap();
        assert_eq!((id.vid, id.pid), (0xf143, 0x0002));
        assert_eq!(id.to_string(), "f143:0002");
        assert!("f143".parse::<UsbId>().is_err());
        assert!("xyz:0002".parse::<UsbId>().is_err());
    }

    #[test]
    fn resolves_by_id_and_serial_number() {
        let gun: UsbId = "f143:0002".parse().unwrap();
        let ports = [
            port("/dev/ttyACM3", "2e8a:f00b", Some("PICO")),
            port("/dev/ttyACM5", "f143:0002", Some("GUN-A")),
            port("/dev/ttyACM6", "f143:0002", Some("GUN-B")),
        ];

        assert_eq!(
            resolve_port(&ports, gun, Some("GUN-B")).unwrap(),
            "/dev/ttyACM6"
        );
        let pico: UsbId = "2e8a:f00b".parse().unwrap();
        assert_eq!(resolve_port(&ports, pico, None).unwrap(), "/dev/ttyACM3");
        // Same VID/PID twice without a serial number: ambiguous.
        assert!(resolve_port(&ports, gun, None).is_err());
        assert!(resolve_port(&ports, gun, Some("GUN-C")).is_err());
    }
}
