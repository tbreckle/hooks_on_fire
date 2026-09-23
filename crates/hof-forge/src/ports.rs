use hof_common::config::DeviceConnectionDetails;
use hof_common::usb::{self, UsbSerialPort};

/// A serial port entry shown in the port selector.
pub struct PortOption {
    /// Connection details stored in the config when this entry is selected. For connected
    /// USB devices this is the USB id (and serial number), not the current port path.
    pub details: DeviceConnectionDetails,
    /// Human-readable label shown in the dropdown (with the current port path).
    pub label: String,
}

/// Builds the option list for the dropdown from the connected USB serial devices.
/// The currently configured device is always included, even if it is not connected right
/// now, so opening the dialog never loses it. Returns the options and the index of `current`.
pub fn port_options(current: &DeviceConnectionDetails) -> (Vec<PortOption>, usize) {
    let ports = usb::usb_serial_ports();
    let selected = ports.iter().position(|port| is_current(port, current));

    let mut options: Vec<PortOption> = ports
        .iter()
        .map(|port| PortOption {
            details: DeviceConnectionDetails::Serial {
                usb_id: Some(port.id),
                usb_serial: port.serial_number.clone(),
                port: String::new(),
            },
            label: format!("{}  –  {}  [{}]", port.path, port.name(), port.id),
        })
        .collect();

    if let Some(idx) = selected {
        return (options, idx);
    }

    let DeviceConnectionDetails::Serial {
        usb_id,
        usb_serial,
        port,
    } = current;
    let label = match (usb_id, port.is_empty()) {
        (Some(id), _) => match usb_serial {
            Some(serial) => format!("USB device [{id}], serial {serial}  (not connected)"),
            None => format!("USB device [{id}]  (not connected)"),
        },
        (None, false) => format!("{port}  (not connected)"),
        (None, true) if options.is_empty() => "No USB serial devices found".to_string(),
        (None, true) => "Select a port…".to_string(),
    };
    options.insert(
        0,
        PortOption {
            details: current.clone(),
            label,
        },
    );
    (options, 0)
}

/// Whether `port` is the configured device: by USB id (and serial number), or by port path
/// for older configs that only have a path (saving then converts them to the USB id).
fn is_current(port: &UsbSerialPort, current: &DeviceConnectionDetails) -> bool {
    match current {
        DeviceConnectionDetails::Serial {
            usb_id: Some(id),
            usb_serial,
            ..
        } => usb::matches(port, *id, usb_serial.as_deref()),
        DeviceConnectionDetails::Serial { port: path, .. } => {
            !path.is_empty() && port.path == *path
        }
    }
}
