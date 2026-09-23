use anyhow::{bail, Context, Result};
use hof_common::config::{DeviceEntry, MAX_PLAYERS};
use hof_common::usb;
use std::collections::HashMap;
use std::path::Path;
use tracing::info;

pub const DEFAULT_COMMAND_DELAY_MS: u64 = 1;

/// Device action sent to all devices when a game is started.
pub const ENTER_GAME_ACTION: &str = "enter_game";
/// Device action sent to all devices when a game is ended (game stop, new game or quit).
pub const LEAVE_GAME_ACTION: &str = "leave_game";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeviceType {
    LightGun,
    LightController,
}

impl std::fmt::Display for DeviceType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DeviceType::LightGun => write!(f, "lightgun"),
            DeviceType::LightController => write!(f, "lightcontroller"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConnectionType {
    Serial,
}

impl std::fmt::Display for ConnectionType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConnectionType::Serial => write!(f, "serial"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ConnectionDetails {
    pub speed: u32,
}

#[derive(Debug, Clone)]
pub struct Device {
    device_type: DeviceType,
    name: String,
    display_name: String,
    /// Unique name of this device instance from hof-config.yaml (e.g. "OpenFire P1").
    instance_name: String,
    /// Player this device is assigned to (light guns), from hof-config.yaml.
    player: Option<u8>,
    serial_port: String,
    connection_type: ConnectionType,
    connection_details: ConnectionDetails,
    command_delay_ms: u64,
    /// Maximum number of instances of this device in hof-config.yaml (`max-instances`).
    max_instances: Option<u64>,
    /// Map of action name → list of device commands
    actions: HashMap<String, Vec<String>>,
}

#[allow(dead_code)]
impl Device {
    pub fn device_type(&self) -> &DeviceType {
        &self.device_type
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn display_name(&self) -> &str {
        &self.display_name
    }

    pub fn instance_name(&self) -> &str {
        &self.instance_name
    }

    pub fn player(&self) -> Option<u8> {
        self.player
    }

    pub fn serial_port(&self) -> &str {
        &self.serial_port
    }

    pub fn connection_type(&self) -> &ConnectionType {
        &self.connection_type
    }

    pub fn connection_details(&self) -> &ConnectionDetails {
        &self.connection_details
    }

    pub fn command_delay_ms(&self) -> u64 {
        self.command_delay_ms
    }

    pub fn actions(&self) -> &HashMap<String, Vec<String>> {
        &self.actions
    }

    pub fn action(&self, name: &str) -> Option<&Vec<String>> {
        self.actions.get(name)
    }
}

#[derive(Debug)]
pub struct DeviceRegistry {
    devices: Vec<Device>,
}

#[allow(dead_code)]
impl DeviceRegistry {
    /// Load devices from config entries. Each entry maps to a file `{name}.yaml`
    /// in the current working directory. The `name` field inside the YAML must match.
    pub fn load(entries: &[DeviceEntry]) -> Result<Self> {
        let mut devices: Vec<Device> = Vec::new();
        // Current port paths of the connected USB devices (resolved by USB id below).
        let usb_ports = usb::usb_serial_ports();

        for entry in entries {
            let filename = format!("{}.yaml", entry.name);
            let path = Path::new(&filename);

            let content = std::fs::read_to_string(path)
                .with_context(|| format!("Failed to read device file: {}", filename))?;

            let raw: serde_yaml::Value = serde_yaml::from_str(&content)
                .with_context(|| format!("Failed to parse YAML in {}", filename))?;

            let device_value = raw
                .get("device")
                .with_context(|| format!("Missing root 'device' key in {}", filename))?;

            let mut device = parse_device(device_value, &filename)?;

            if device.name != entry.name {
                bail!(
                    "Device name mismatch in {}: config lists '{}' but file contains name '{}'",
                    filename,
                    entry.name,
                    device.name
                );
            }

            device.instance_name = entry.instance_name.clone();
            if let Some(player) = entry.player {
                if !(1..=MAX_PLAYERS).contains(&player) {
                    bail!(
                        "Invalid player {} for device '{}' in hof-config.yaml (allowed: 1-{})",
                        player,
                        entry.instance_name,
                        MAX_PLAYERS
                    );
                }
            }
            device.player = entry.player;

            device.serial_port = entry
                .connection_details
                .resolve_serial_port(&usb_ports)
                .with_context(|| format!("Device '{}' is not available", entry.instance_name))?;

            devices.push(device);
        }

        check_max_instances(&devices)?;

        info!("Loaded {} device(s):", devices.len());
        for device in &devices {
            let action_names: Vec<&str> = {
                let mut keys: Vec<&str> = device.actions.keys().map(|s| s.as_str()).collect();
                keys.sort();
                keys
            };
            let player = device
                .player
                .map_or("unassigned".to_string(), |p| format!("player {p}"));
            info!(
                "  [{}] '{}' ({}, {}), connection: {} on '{}' @ {} baud, delay: {}ms - actions: [{}]",
                device.device_type,
                device.instance_name,
                device.name,
                player,
                device.connection_type,
                device.serial_port,
                device.connection_details.speed,
                device.command_delay_ms,
                action_names.join(", ")
            );
            for action_name in &action_names {
                if let Some(cmds) = device.actions.get(*action_name) {
                    info!("    {}: {:?}", action_name, cmds);
                }
            }
        }

        Ok(DeviceRegistry { devices })
    }

    pub fn get_by_id(&self, id: usize) -> Option<&Device> {
        self.devices.get(id)
    }

    pub fn get_by_name(&self, name: &str) -> Option<&Device> {
        self.devices.iter().find(|d| d.name == name)
    }

    pub fn get_by_type(&self, device_type: &DeviceType) -> Vec<&Device> {
        self.devices
            .iter()
            .filter(|d| &d.device_type == device_type)
            .collect()
    }

    pub fn devices(&self) -> &[Device] {
        &self.devices
    }

    pub fn len(&self) -> usize {
        self.devices.len()
    }

    pub fn is_empty(&self) -> bool {
        self.devices.is_empty()
    }

    /// Light guns a command for `player` is sent to:
    /// - `None` (all players): all light guns.
    /// - `Some(n)`: the light guns assigned to player n, or the unassigned light guns if no
    ///   light gun is assigned to player n.
    pub fn light_guns_for_player(&self, player: Option<u8>) -> Vec<&Device> {
        let guns = self.get_by_type(&DeviceType::LightGun);
        let Some(player) = player else {
            return guns;
        };
        if guns.iter().any(|d| d.player == Some(player)) {
            guns.into_iter()
                .filter(|d| d.player == Some(player))
                .collect()
        } else {
            guns.into_iter().filter(|d| d.player.is_none()).collect()
        }
    }

    /// Returns true if any device of the given type has the specified action configured.
    pub fn has_action_for_type(&self, action: &str, device_type: &DeviceType) -> bool {
        self.devices
            .iter()
            .any(|d| &d.device_type == device_type && d.actions.contains_key(action))
    }
}

/// Fails if a device is configured more often than its `max-instances` allows.
fn check_max_instances(devices: &[Device]) -> Result<()> {
    for device in devices {
        let Some(max) = device.max_instances else {
            continue;
        };
        let count = devices.iter().filter(|d| d.name == device.name).count() as u64;
        if count > max {
            bail!(
                "Device '{}' is configured {} times in hof-config.yaml, but at most {} instance(s) are allowed.",
                device.display_name,
                count,
                max
            );
        }
    }
    Ok(())
}

const RESERVED_KEYS: &[&str] = &[
    "type",
    "name",
    "display-name",
    "connection-type",
    "connection-details",
    "command-delay",
    "max-instances",
];

fn parse_device(value: &serde_yaml::Value, source: &str) -> Result<Device> {
    let map = value
        .as_mapping()
        .with_context(|| format!("'device' in {} is not a YAML mapping", source))?;

    let type_str = map
        .get("type")
        .and_then(|v| v.as_str())
        .with_context(|| format!("Missing or invalid 'type' field in {}", source))?;

    let device_type = match type_str {
        "lightgun" => DeviceType::LightGun,
        "lightcontroller" => DeviceType::LightController,
        other => bail!(
            "Invalid device type '{}' in {}. Allowed values: lightgun, lightcontroller",
            other,
            source
        ),
    };

    let name = map
        .get("name")
        .and_then(|v| v.as_str())
        .with_context(|| format!("Missing or invalid 'name' field in {}", source))?
        .to_string();

    if name.is_empty() {
        bail!("Device 'name' cannot be empty in {}", source);
    }

    let display_name = map
        .get("display-name")
        .and_then(|v| v.as_str())
        .with_context(|| format!("Missing or invalid 'display-name' field in {}", source))?
        .to_string();

    // connection-type (mandatory)
    let conn_type_str = map
        .get("connection-type")
        .and_then(|v| v.as_str())
        .with_context(|| format!("Missing or invalid 'connection-type' field in {}", source))?;

    let connection_type = match conn_type_str {
        "serial" => ConnectionType::Serial,
        other => bail!(
            "Invalid connection-type '{}' in {}. Allowed values: serial",
            other,
            source
        ),
    };

    // connection-details (mandatory, must contain speed)
    let conn_details_value = map
        .get("connection-details")
        .with_context(|| format!("Missing 'connection-details' field in {}", source))?;

    let conn_details_map = conn_details_value
        .as_mapping()
        .with_context(|| format!("'connection-details' in {} must be a mapping", source))?;

    let speed = conn_details_map
        .get("speed")
        .and_then(|v| v.as_u64())
        .with_context(|| {
            format!(
                "Missing or invalid 'speed' in connection-details in {}",
                source
            )
        })? as u32;

    let connection_details = ConnectionDetails { speed };

    // command-delay (optional, defaults to DEFAULT_COMMAND_DELAY_MS)
    let command_delay_ms = map
        .get("command-delay")
        .and_then(|v| v.as_u64())
        .unwrap_or(DEFAULT_COMMAND_DELAY_MS);

    // max-instances (optional, unlimited if missing)
    let max_instances = match map.get("max-instances") {
        None => None,
        Some(v) => match v.as_u64() {
            Some(n) if n > 0 => Some(n),
            _ => bail!("'max-instances' in {} must be a number > 0", source),
        },
    };

    // Collect remaining keys as actions
    let mut actions: HashMap<String, Vec<String>> = HashMap::new();

    for (key, val) in map {
        let key_str = match key.as_str() {
            Some(s) => s,
            None => continue,
        };
        if RESERVED_KEYS.contains(&key_str) {
            continue;
        }

        let commands: Vec<String> = val
            .as_sequence()
            .with_context(|| {
                format!(
                    "Action '{}' in {} must be a list of strings",
                    key_str, source
                )
            })?
            .iter()
            .enumerate()
            .map(|(i, v)| {
                v.as_str()
                    .with_context(|| {
                        format!(
                            "Action '{}' entry {} in {} is not a string",
                            key_str, i, source
                        )
                    })
                    .map(|s| s.to_string())
            })
            .collect::<Result<_>>()?;

        actions.insert(key_str.to_string(), commands);
    }

    Ok(Device {
        device_type,
        name,
        display_name,
        // populated from config entry in load()
        instance_name: String::new(),
        player: None,
        serial_port: String::new(),
        connection_type,
        connection_details,
        command_delay_ms,
        max_instances,
        actions,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn device(device_type: DeviceType, instance_name: &str, player: Option<u8>) -> Device {
        Device {
            device_type,
            name: "test".to_string(),
            display_name: "Test".to_string(),
            instance_name: instance_name.to_string(),
            player,
            serial_port: String::new(),
            connection_type: ConnectionType::Serial,
            connection_details: ConnectionDetails { speed: 115200 },
            command_delay_ms: DEFAULT_COMMAND_DELAY_MS,
            max_instances: None,
            actions: HashMap::new(),
        }
    }

    fn names(devices: Vec<&Device>) -> Vec<&str> {
        devices.iter().map(|d| d.instance_name()).collect()
    }

    #[test]
    fn rejects_more_instances_than_allowed() {
        let mut blast = device(DeviceType::LightController, "BLAST", None);
        blast.max_instances = Some(1);

        assert!(check_max_instances(&[blast.clone()]).is_ok());
        let err = check_max_instances(&[blast.clone(), blast]).unwrap_err();
        assert!(err.to_string().contains("at most 1 instance"));
    }

    #[test]
    fn routes_player_commands_to_assigned_guns() {
        let registry = DeviceRegistry {
            devices: vec![
                device(DeviceType::LightController, "BLAST", None),
                device(DeviceType::LightGun, "Gun P1", Some(1)),
                device(DeviceType::LightGun, "Gun P2", Some(2)),
                device(DeviceType::LightGun, "Gun free", None),
            ],
        };

        assert_eq!(names(registry.light_guns_for_player(Some(1))), ["Gun P1"]);
        assert_eq!(names(registry.light_guns_for_player(Some(2))), ["Gun P2"]);
        // Nobody is player 3: unassigned guns take it.
        assert_eq!(names(registry.light_guns_for_player(Some(3))), ["Gun free"]);
        // All players: every gun, but no light controller.
        assert_eq!(
            names(registry.light_guns_for_player(None)),
            ["Gun P1", "Gun P2", "Gun free"]
        );
    }
}
