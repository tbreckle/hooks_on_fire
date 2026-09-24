// Windows: run without a console window (hof-forge is a GUI app).
#![cfg_attr(windows, windows_subsystem = "windows")]

slint::include_modules!();

mod ports;

use std::cell::RefCell;
use std::fs;
use std::rc::Rc;

use anyhow::Context;
use hof_common::build_info;
use hof_common::config::{DeviceConnectionDetails, DeviceEntry, HofConfig, MAX_PLAYERS};
use hof_common::data_files::{self, DataKind};
use hof_common::instance_lock::InstanceLock;
use hof_common::paths;
use serde::Deserialize;
use slint::{ModelRc, SharedString, StandardListViewItem, VecModel};
use tracing::{error, info};

// Minimal structs for detecting and reading device YAML files.
#[derive(Deserialize)]
struct DeviceYamlRoot {
    device: DeviceYamlInfo,
}

#[derive(Deserialize)]
struct DeviceYamlInfo {
    name: String,
    #[serde(rename = "display-name")]
    display_name: String,
    #[serde(rename = "type")]
    device_type: String,
    #[serde(rename = "max-instances", default)]
    max_instances: Option<usize>,
}

struct DeviceFile {
    /// Matches `name` inside the device YAML (used as key in config).
    name: String,
    /// Human-readable label from `display-name`.
    display_name: String,
    /// Device type (e.g. "lightgun", "lightcontroller").
    device_type: String,
    /// Maximum number of instances (`max-instances`), unlimited if `None`.
    max_instances: Option<usize>,
}

/// Reads all device files (user layer and shipped, see `hof_common::data_files`), i.e. YAML
/// files whose root element is `device:`.
fn scan_device_files() -> Vec<DeviceFile> {
    let mut result = Vec::new();
    for path in data_files::list(DataKind::Devices) {
        let Ok(contents) = fs::read_to_string(&path) else {
            continue;
        };
        let Ok(root) = serde_yaml_ng::from_str::<DeviceYamlRoot>(&contents) else {
            continue;
        };
        result.push(DeviceFile {
            name: root.device.name,
            display_name: root.device.display_name,
            device_type: root.device.device_type,
            max_instances: root.device.max_instances,
        });
    }
    result
}

struct AppState {
    config: HofConfig,
    devices: Vec<DeviceFile>,
    /// Index into `devices` of the device currently being added (while name dialog is open).
    pending_device_idx: usize,
    /// Index into `config.devices` of the instance currently being deleted (while confirm dialog is open).
    pending_instance_idx: usize,
    /// Indices into `config.devices` in the (alphabetical) order shown in the instance list.
    instance_order: Vec<usize>,
    /// Connection details (USB id) matching the entries of the port dropdown
    /// (while configure dialog is open).
    port_choices: Vec<DeviceConnectionDetails>,
    /// Available devices list and whether another instance of each may be added.
    device_model: Rc<VecModel<StandardListViewItem>>,
    device_can_add: Rc<VecModel<bool>>,
}

impl AppState {
    /// Number of configured instances of device `idx`.
    fn instance_count(&self, idx: usize) -> usize {
        let name = &self.devices[idx].name;
        self.config
            .devices
            .iter()
            .filter(|d| &d.name == name)
            .count()
    }

    /// Rebuilds the available devices list (labels show the used instances if limited).
    fn refresh_device_list(&self) {
        let mut labels = Vec::new();
        let mut can_add = Vec::new();
        for (idx, device) in self.devices.iter().enumerate() {
            let count = self.instance_count(idx);
            let label = match device.max_instances {
                Some(max) => format!(
                    "{} [{}]  ({count}/{max})",
                    device.display_name, device.device_type
                ),
                None => format!("{} [{}]", device.display_name, device.device_type),
            };
            labels.push(StandardListViewItem::from(SharedString::from(
                label.as_str(),
            )));
            can_add.push(device.max_instances.is_none_or(|max| count < max));
        }
        self.device_model.set_vec(labels);
        self.device_can_add.set_vec(can_add);
    }
}

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    run().inspect_err(|err| {
        error!("hof-forge failed: {err:#}");
        show_error(err);
    })
}

/// Shows an error in a message box and waits until the user presses OK.
/// Without a console (Windows) this is the only way the user learns about it.
fn show_error(err: &anyhow::Error) {
    // Without a display GTK cannot start and the dialog would block forever (e.g. over SSH).
    #[cfg(target_os = "linux")]
    if std::env::var_os("DISPLAY").is_none() && std::env::var_os("WAYLAND_DISPLAY").is_none() {
        return;
    }

    rfd::MessageDialog::new()
        .set_level(rfd::MessageLevel::Error)
        .set_title("Hooks on Fire - Forge")
        .set_description(format!("{err:#}"))
        .set_buttons(rfd::MessageButtons::Ok)
        .show();
}

/// Logs a failed config save and shows it in a message box.
///
/// The box is opened after the current callback has returned: while it is open, the event
/// loop may run other callbacks, which must not find `AppState` still borrowed.
fn report_save_error(err: anyhow::Error) {
    let err = err.context("The configuration could not be saved");
    error!("{err:#}");
    slint::Timer::single_shot(std::time::Duration::ZERO, move || show_error(&err));
}

fn run() -> anyhow::Result<()> {
    let version_string = build_info::format_version(
        env!("CARGO_PKG_VERSION"),
        env!("HOF_GIT_HASH"),
        env!("HOF_BUILD_DATE"),
    );

    info!("hof-forge {version_string}");

    let _lock = InstanceLock::acquire("hof-forge", "hof-blaze")?;

    let config = HofConfig::load()?;
    info!("Config loaded");

    let mut devices = scan_device_files();
    devices.sort_by_cached_key(|d| (d.display_name.to_lowercase(), d.device_type.clone()));
    info!("Found {} device file(s)", devices.len());

    let state = Rc::new(RefCell::new(AppState {
        config,
        devices,
        pending_device_idx: 0,
        pending_instance_idx: 0,
        instance_order: Vec::new(),
        port_choices: Vec::new(),
        device_model: Rc::new(VecModel::default()),
        device_can_add: Rc::new(VecModel::default()),
    }));

    let window = MainWindow::new()?;
    window.set_version(SharedString::from(version_string.as_str()));

    // Player dropdown entries: index 0 = not assigned, index n = player n.
    let player_labels: Vec<SharedString> = std::iter::once("Not assigned".to_string())
        .chain((1..=MAX_PLAYERS).map(|p| format!("Player {p}")))
        .map(|label| SharedString::from(label.as_str()))
        .collect();
    window.set_configure_player_labels(ModelRc::from(Rc::new(VecModel::from(player_labels))));

    // Populate the device list from scanned YAML files.
    {
        let st = state.borrow();
        st.refresh_device_list();
        window.set_device_list(ModelRc::from(st.device_model.clone()));
        window.set_device_can_add(ModelRc::from(st.device_can_add.clone()));
    }

    // Populate the instance list from the loaded config.
    let instance_model = Rc::new(VecModel::default());
    refresh_instance_list(&mut state.borrow_mut(), &instance_model, None);
    window.set_instance_list(ModelRc::from(instance_model.clone()));

    // Populate settings fields from config.
    {
        let st = state.borrow();
        window.set_tcp_host(SharedString::from(st.config.tcp_host.as_str()));
        window.set_tcp_port(st.config.tcp_port as i32);
        window.set_udp_broadcast_port(st.config.udp_broadcast_port as i32);
    }

    // + button: record which device is selected, open name dialog with display name as default.
    {
        let window_weak = window.as_weak();
        let state_clone = state.clone();
        window.on_add_clicked(move |device_idx| {
            let win = window_weak.unwrap();
            let mut st = state_clone.borrow_mut();
            let idx = device_idx as usize;

            // Respect the device's max-instances limit.
            if let Some(max) = st.devices[idx].max_instances {
                if st.instance_count(idx) >= max {
                    let name = &st.devices[idx].display_name;
                    let message = if max == 1 {
                        format!("{name} can only be added once.")
                    } else {
                        format!("{name} can be added at most {max} times.")
                    };
                    win.set_device_message(SharedString::from(message.as_str()));
                    return;
                }
            }

            st.pending_device_idx = idx;
            let default_name = st.devices[idx].display_name.clone();
            drop(st);

            win.set_device_message(SharedString::from(""));
            win.set_name_dialog_value(SharedString::from(default_name.as_str()));
            win.set_name_dialog_error(SharedString::from(""));
            win.set_name_dialog_visible(true);
        });
    }

    // Name dialog OK: validate uniqueness, add instance to config and list model.
    {
        let window_weak = window.as_weak();
        let state_clone = state.clone();
        let instance_model_clone = instance_model.clone();
        window.on_name_confirmed(move |name| {
            let name = name.to_string();
            let win = window_weak.unwrap();

            let st = state_clone.borrow();
            if st.config.devices.iter().any(|d| d.instance_name == name) {
                win.set_name_dialog_error(SharedString::from("Name already in use."));
                return;
            }
            let device_idx = st.pending_device_idx;
            let device_name = st.devices[device_idx].name.clone();
            drop(st);

            let mut st = state_clone.borrow_mut();
            st.config.devices.push(DeviceEntry {
                name: device_name,
                instance_name: name.clone(),
                connection_details: DeviceConnectionDetails::empty_serial(),
                player: None,
            });
            if let Err(err) = st.config.save() {
                report_save_error(err);
            }
            let new_idx = st.config.devices.len() - 1;
            let row = refresh_instance_list(&mut st, &instance_model_clone, Some(new_idx));
            st.refresh_device_list();
            drop(st);

            win.set_selected_instance(row);
            win.set_name_dialog_visible(false);
            win.set_name_dialog_error(SharedString::from(""));
        });
    }

    // Name dialog Cancel.
    {
        let window_weak = window.as_weak();
        window.on_name_cancelled(move || {
            let win = window_weak.unwrap();
            win.set_name_dialog_visible(false);
            win.set_name_dialog_error(SharedString::from(""));
        });
    }

    // - button: record which instance is selected, open confirm dialog.
    {
        let window_weak = window.as_weak();
        let state_clone = state.clone();
        window.on_remove_clicked(move |instance_idx| {
            let mut st = state_clone.borrow_mut();
            let idx = st.instance_order[instance_idx as usize];
            st.pending_instance_idx = idx;
            let name = st.config.devices[idx].instance_name.clone();
            drop(st);

            let win = window_weak.unwrap();
            win.set_confirm_dialog_instance_name(SharedString::from(name.as_str()));
            win.set_confirm_dialog_visible(true);
        });
    }

    // Confirm dialog Remove: delete from config and list model.
    {
        let window_weak = window.as_weak();
        let state_clone = state.clone();
        let instance_model_clone = instance_model.clone();
        window.on_delete_confirmed(move || {
            let mut st = state_clone.borrow_mut();
            let idx = st.pending_instance_idx;
            st.config.devices.remove(idx);
            if let Err(err) = st.config.save() {
                report_save_error(err);
            }
            refresh_instance_list(&mut st, &instance_model_clone, None);
            st.refresh_device_list();
            let remaining = st.instance_order.len() as i32;
            drop(st);

            // Keep the selection on the row that moved into place (handy for repeated Del).
            let win = window_weak.unwrap();
            let row = win.get_selected_instance().min(remaining - 1);
            win.set_selected_instance(row);
            win.set_confirm_dialog_visible(false);
        });
    }

    // Confirm dialog Cancel.
    {
        let window_weak = window.as_weak();
        window.on_delete_cancelled(move || {
            window_weak.unwrap().set_confirm_dialog_visible(false);
        });
    }

    // Configure button: populate dialog fields from config, then show dialog.
    {
        let window_weak = window.as_weak();
        let state_clone = state.clone();
        window.on_configure_clicked(move |instance_idx| {
            let mut st = state_clone.borrow_mut();
            let idx = st.instance_order[instance_idx as usize];
            st.pending_instance_idx = idx;
            let entry = &st.config.devices[idx];
            let instance_name = entry.instance_name.clone();
            let connection = entry.connection_details.clone();
            let conn_type = match &connection {
                DeviceConnectionDetails::Serial { .. } => "serial",
            };
            // Only light guns are routed by player. Show it too if the device file is missing.
            let player_visible = st
                .devices
                .iter()
                .find(|d| d.name == entry.name)
                .is_none_or(|d| d.device_type == "lightgun");
            let player_index = entry.player.map_or(0, i32::from);
            drop(st);

            let win = window_weak.unwrap();
            fill_port_selector(&win, &state_clone, &connection);
            win.set_configure_instance_name(SharedString::from(instance_name.as_str()));
            win.set_configure_connection_type(SharedString::from(conn_type));
            win.set_configure_player_visible(player_visible);
            win.set_configure_player_index(player_index);
            win.set_configure_error(SharedString::from(""));
            win.set_configure_dialog_visible(true);
        });
    }

    // Configure dialog Save: validate, update config, save, refresh instance list.
    {
        let window_weak = window.as_weak();
        let state_clone = state.clone();
        let instance_model_clone = instance_model.clone();
        window.on_configure_saved(move || {
            let win = window_weak.unwrap();
            let new_name = win.get_configure_instance_name().to_string();
            let conn_type = win.get_configure_connection_type().to_string();

            let st = state_clone.borrow();
            let idx = st.pending_instance_idx;
            let old_name = &st.config.devices[idx].instance_name;

            // Validate: name must be non-empty and unique (ignoring self).
            if new_name.is_empty() {
                win.set_configure_error(SharedString::from("Instance name cannot be empty."));
                return;
            }
            if new_name != *old_name
                && st
                    .config
                    .devices
                    .iter()
                    .any(|d| d.instance_name == new_name)
            {
                win.set_configure_error(SharedString::from("Name already in use."));
                return;
            }
            drop(st);

            let mut st = state_clone.borrow_mut();
            let entry = &mut st.config.devices[idx];
            entry.instance_name = new_name.clone();

            // Update connection details based on type.
            if conn_type == "serial" {
                let selected = usize::try_from(win.get_configure_port_index())
                    .ok()
                    .and_then(|i| st.port_choices.get(i).cloned());
                if let Some(details) = selected {
                    st.config.devices[idx].connection_details = details;
                }
            }

            // Player assignment (index 0 = not assigned). Untouched if the field is hidden.
            if win.get_configure_player_visible() {
                st.config.devices[idx].player = u8::try_from(win.get_configure_player_index())
                    .ok()
                    .filter(|p| (1..=MAX_PLAYERS).contains(p));
            }

            if let Err(err) = st.config.save() {
                report_save_error(err);
            }
            // Update instance list display (a rename may change the sort position).
            let row = refresh_instance_list(&mut st, &instance_model_clone, Some(idx));
            drop(st);

            win.set_selected_instance(row);
            win.set_configure_error(SharedString::from(""));
            win.set_configure_dialog_visible(false);
        });
    }

    // Configure dialog refresh button: rescan ports, keeping the current selection.
    {
        let window_weak = window.as_weak();
        let state_clone = state.clone();
        window.on_configure_refresh_ports(move || {
            let win = window_weak.unwrap();
            let current = usize::try_from(win.get_configure_port_index())
                .ok()
                .and_then(|i| state_clone.borrow().port_choices.get(i).cloned())
                .unwrap_or_else(DeviceConnectionDetails::empty_serial);
            fill_port_selector(&win, &state_clone, &current);
        });
    }

    // Configure dialog Cancel.
    {
        let window_weak = window.as_weak();
        window.on_configure_cancelled(move || {
            window_weak.unwrap().set_configure_dialog_visible(false);
        });
    }

    // Settings Save: validate and persist network settings.
    {
        let window_weak = window.as_weak();
        let state_clone = state.clone();
        window.on_settings_saved(move || {
            let win = window_weak.unwrap();
            let tcp_host = win.get_tcp_host().to_string();
            let tcp_port = win.get_tcp_port() as u16;
            let udp_port = win.get_udp_broadcast_port() as u16;

            let mut st = state_clone.borrow_mut();
            st.config.tcp_host = tcp_host;
            st.config.tcp_port = tcp_port;
            st.config.udp_broadcast_port = udp_port;
            if let Err(err) = st.config.save() {
                drop(st);
                win.set_settings_error(SharedString::from(
                    format!("Save failed: {err:#}").as_str(),
                ));
                report_save_error(err);
                return;
            }
            drop(st);

            win.set_settings_error(SharedString::from(""));
        });
    }

    // Open log button: open the hof-blaze log file with the OS default program.
    {
        let window_weak = window.as_weak();
        window.on_open_log(move || {
            let win = window_weak.unwrap();
            let message = match open_blaze_log() {
                Ok(()) => String::new(),
                Err(e) => {
                    tracing::warn!("Failed to open log file: {e:#}");
                    format!("{e:#}")
                }
            };
            win.set_overview_error(SharedString::from(message.as_str()));
        });
    }

    // Open game files folder button: the user layer, where hof-blaze saves game files.
    {
        let window_weak = window.as_weak();
        window.on_open_games_folder(move || {
            let win = window_weak.unwrap();
            let message = match open_games_folder() {
                Ok(()) => String::new(),
                Err(e) => {
                    tracing::warn!("Failed to open game files folder: {e:#}");
                    format!("{e:#}")
                }
            };
            win.set_overview_error(SharedString::from(message.as_str()));
        });
    }

    window.run()?;
    Ok(())
}

/// Opens the user game files folder (created if missing) in the OS file manager.
fn open_games_folder() -> anyhow::Result<()> {
    let dir = data_files::user_dir(DataKind::Games)?;
    fs::create_dir_all(&dir).with_context(|| format!("Could not create {}", dir.display()))?;
    info!("Opening game files folder {}", dir.display());
    open::that_detached(&dir).with_context(|| format!("Could not open {}", dir.display()))
}

/// Opens the most recently written hof-blaze log file with the OS default program.
fn open_blaze_log() -> anyhow::Result<()> {
    let path = paths::log_dirs()
        .into_iter()
        .map(|dir| dir.join(paths::BLAZE_LOG_FILE))
        .filter_map(|path| {
            let modified = fs::metadata(&path).and_then(|m| m.modified()).ok()?;
            Some((modified, path))
        })
        .max()
        .map(|(_, path)| path)
        .ok_or_else(|| anyhow::anyhow!("No log file found. Start hof-blaze first."))?;

    info!("Opening log file {}", path.display());
    open::that_detached(&path).with_context(|| format!("Could not open {}", path.display()))
}

/// Scans the connected USB serial devices and fills the port dropdown, selecting `current`.
/// Port paths in the labels are always the current ones; the selection follows the USB id.
fn fill_port_selector(
    win: &MainWindow,
    state: &Rc<RefCell<AppState>>,
    current: &DeviceConnectionDetails,
) {
    let (options, selected) = ports::port_options(current);
    let labels: Vec<SharedString> = options
        .iter()
        .map(|o| SharedString::from(o.label.as_str()))
        .collect();
    state.borrow_mut().port_choices = options.into_iter().map(|o| o.details).collect();
    win.set_configure_port_labels(ModelRc::from(Rc::new(VecModel::from(labels))));
    win.set_configure_port_index(selected as i32);
}

/// Rebuilds the instance list sorted alphabetically by instance name (the order in the
/// config file is kept). Returns the list row of `config_idx`, or -1.
fn refresh_instance_list(
    st: &mut AppState,
    model: &VecModel<StandardListViewItem>,
    config_idx: Option<usize>,
) -> i32 {
    let devices = &st.config.devices;
    let mut order: Vec<usize> = (0..devices.len()).collect();
    order.sort_by_cached_key(|&i| {
        let name = &devices[i].instance_name;
        (name.to_lowercase(), name.clone())
    });

    let items: Vec<StandardListViewItem> = order
        .iter()
        .map(|&i| {
            let label = match devices[i].player {
                Some(p) => format!("{}  (Player {p})", devices[i].instance_name),
                None => devices[i].instance_name.clone(),
            };
            StandardListViewItem::from(SharedString::from(label.as_str()))
        })
        .collect();
    model.set_vec(items);

    let row = config_idx
        .and_then(|idx| order.iter().position(|&i| i == idx))
        .map_or(-1, |row| row as i32);
    st.instance_order = order;
    row
}
