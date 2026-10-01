use std::path::PathBuf;

use anyhow::{Context, Result};
use hof_common::events::{StateEvent, TrayEvent};
use hof_common::paths;
use tracing::{info, warn};
use tray_icon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder};

const ICON_BYTES_HEALTHY: &[u8] = include_bytes!("../../../assets/icon_healthy.png");
const ICON_BYTES_FAULTY: &[u8] = include_bytes!("../../../assets/icon_faulty.png");
const ICON_BYTES_WAITING: &[u8] = include_bytes!("../../../assets/icon_waiting.png");

const EXIT_ID: &str = "exit_id";
const FORGE_ID: &str = "forge_id";
const LOG_ID: &str = "log_id";
const RELOAD_ID: &str = "reload_id";

/// How the tray was left.
pub enum TrayExit {
    /// Exit was chosen.
    Exit,
    /// "Open HoF-forge" was chosen: start hof-forge (at this path) after shutting down.
    SwitchToForge(PathBuf),
}

/// Connection status shown by the tray icon and the status menu item.
#[derive(Clone, Copy, Debug)]
enum TrayStatus {
    Waiting,
    Healthy,
    Disconnected,
    Faulty,
}

/// Change passed from the Tokio runtime to the tray loop.
enum TrayUpdate {
    Status(TrayStatus),
    /// The running game (`None`: no game running).
    Game(Option<RunningGame>),
}

/// The running game as shown in the tray.
struct RunningGame {
    /// See `game_label`.
    label: String,
    /// The game file could not be loaded: shown with the faulty icon until the game ends or
    /// its file is reloaded without error.
    file_error: bool,
}

/// Builds a tray `Icon` from the embedded PNG.
fn load_icon(icon_bytes: &[u8]) -> Result<Icon> {
    let img = image::load_from_memory(icon_bytes)
        .context("Failed to decode embedded icon")?
        .into_rgba8();
    let (w, h) = img.dimensions();
    Icon::from_rgba(img.into_raw(), w, h).context("Failed to create tray icon")
}

/// The tray icon and the menu items that change at runtime.
struct Tray {
    icon: TrayIcon,
    status_item: MenuItem,
    /// "Reload game file", enabled while a game is running.
    reload_item: MenuItem,
    /// Sends `StateEvent::ReloadGame` to the engine.
    reload_tx: tokio::sync::mpsc::Sender<StateEvent>,
    status: TrayStatus,
    /// The running game, shown in brackets behind the status.
    game: Option<RunningGame>,
    /// Icon currently shown, to set it only when it changes.
    icon_bytes: &'static [u8],
}

impl Tray {
    /// Creates the tray icon with a context menu.
    fn build(
        version_string: &str,
        reload_tx: tokio::sync::mpsc::Sender<StateEvent>,
    ) -> Result<Self> {
        let icon = load_icon(ICON_BYTES_WAITING)?;
        let menu = Menu::new();

        let top_item: MenuItem =
            MenuItem::new("HoF-blaze ".to_owned() + version_string, false, None);
        let status_item: MenuItem = MenuItem::new("Status: Waiting", false, None);

        let exit_item: MenuItem = MenuItem::with_id(EXIT_ID, "&Exit", true, None);
        let reload_item: MenuItem = MenuItem::with_id(RELOAD_ID, "&Reload game file", false, None);
        let open_forge: MenuItem = MenuItem::with_id(FORGE_ID, "Open HoF-&forge", true, None);
        // Disabled if logging to a file failed on start.
        let open_log: MenuItem = MenuItem::with_id(
            LOG_ID,
            "Open &log file",
            crate::logging::log_file().is_some(),
            None,
        );

        let separator: PredefinedMenuItem = PredefinedMenuItem::separator();

        menu.append(&top_item)
            .context("Failed to append menu item for app name")?;
        menu.append(&status_item)
            .context("Failed to append menu item for status")?;
        menu.append(&separator)
            .context("Failed to append menu separator.")?;
        menu.append(&reload_item)
            .context("Failed to append menu item for reloading the game file.")?;
        menu.append(&open_forge)
            .context("Failed to append menu item for opening forge.")?;
        menu.append(&open_log)
            .context("Failed to append menu item for opening the log file.")?;
        menu.append(&separator)
            .context("Failed to append menu separator.")?;
        menu.append(&exit_item)
            .context("Failed to append menu item for exit.")?;

        let icon = TrayIconBuilder::new()
            .with_menu(Box::new(menu))
            .with_tooltip(format!("Hooks on Fire - {version_string}"))
            .with_icon(icon)
            .build()
            .context("Failed to build tray icon")?;

        Ok(Self {
            icon,
            status_item,
            reload_item,
            reload_tx,
            status: TrayStatus::Waiting,
            game: None,
            icon_bytes: ICON_BYTES_WAITING,
        })
    }

    fn update(&mut self, update: TrayUpdate) {
        match update {
            TrayUpdate::Status(status) => self.status = status,
            TrayUpdate::Game(game) => {
                self.reload_item.set_enabled(game.is_some());
                self.game = game;
            }
        }
        self.update_icon();
        self.status_item.set_text(self.status_text());
    }

    /// Shows the icon for the status; a game file error shows the faulty icon as well.
    fn update_icon(&mut self) {
        let file_error = self.game.as_ref().is_some_and(|game| game.file_error);
        let icon_bytes = match self.status {
            _ if file_error => ICON_BYTES_FAULTY,
            TrayStatus::Waiting | TrayStatus::Disconnected => ICON_BYTES_WAITING,
            TrayStatus::Healthy => ICON_BYTES_HEALTHY,
            TrayStatus::Faulty => ICON_BYTES_FAULTY,
        };
        if std::ptr::eq(icon_bytes, self.icon_bytes) {
            return;
        }
        self.icon_bytes = icon_bytes;
        if let Err(err) = load_icon(icon_bytes).and_then(|icon| {
            self.icon
                .set_icon(Some(icon))
                .context("Failed to set tray icon")
        }) {
            warn!("Failed to update tray icon: {err:#}");
        }
    }

    /// Status menu text, e.g. `Status: Healthy (The Lost World: Jurassic Park - lostwsga)` or
    /// `Status: Healthy (lostwsga - error in game file)`.
    fn status_text(&self) -> String {
        let status = match self.status {
            TrayStatus::Waiting => "Waiting",
            TrayStatus::Healthy => "Healthy",
            TrayStatus::Disconnected => "Disconnected",
            TrayStatus::Faulty => "Faulty",
        };
        match &self.game {
            Some(game) if game.file_error => {
                format!("Status: {status} ({} - error in game file)", game.label)
            }
            Some(game) => format!("Status: {status} ({})", game.label),
            None => format!("Status: {status}"),
        }
    }

    /// Handles a click on a menu item. Returns how to leave the tray, or `None` to keep running.
    fn handle_menu_event(&self, event: &MenuEvent) -> Option<TrayExit> {
        if event.id == EXIT_ID {
            info!("Exit requested via tray menu.");
            return Some(TrayExit::Exit);
        }
        if event.id == FORGE_ID {
            // Only shut down if hof-forge can be started afterwards.
            match paths::sibling_exe("hof-forge") {
                Ok(path) => {
                    info!("Switching to HoF-Forge.");
                    return Some(TrayExit::SwitchToForge(path));
                }
                Err(err) => {
                    warn!("Cannot switch to HoF-Forge: {err:#}");
                    let _ = notify_rust::Notification::new()
                        .summary("HoF-blaze: could not open HoF-forge")
                        .body(&format!("{err:#}"))
                        .show();
                }
            }
        }
        if event.id == LOG_ID {
            open_log_file();
        }
        if event.id == RELOAD_ID {
            info!("Game file reload requested via tray menu.");
            if let Err(err) = self.reload_tx.try_send(StateEvent::ReloadGame) {
                warn!("Could not reload the game file: {err}");
            }
        }
        None
    }
}

/// Opens the log file with the OS default program. Errors are logged and shown as notification.
fn open_log_file() {
    let Some(path) = crate::logging::log_file() else {
        return;
    };
    info!("Opening log file {}", path.display());
    if let Err(err) = open::that_detached(path) {
        warn!("Could not open log file {}: {}", path.display(), err);
        let _ = notify_rust::Notification::new()
            .summary("HoF-blaze: could not open log file")
            .body(&format!("{}\n\nError: {}", path.display(), err))
            .show();
    }
}

/// Label of a running game for the status menu item: the game file's `display-name` followed
/// by the game name received from the emulator, or only the game name.
fn game_label(name: &str, display_name: Option<&str>) -> String {
    match display_name {
        Some(display_name) if display_name != name => format!("{display_name} - {name}"),
        _ => name.to_string(),
    }
}

/// Receives `TrayEvent`s on the Tokio runtime, shows notifications and passes the new status
/// to the tray loop via `update`.
fn spawn_event_forwarder(
    runtime: &tokio::runtime::Handle,
    mut rx: tokio::sync::mpsc::Receiver<TrayEvent>,
    update: impl Fn(TrayUpdate) + Send + 'static,
) {
    runtime.spawn(async move {
        while let Some(event) = rx.recv().await {
            match event {
                TrayEvent::StatusConnected { host, port } => {
                    info!("Remote connected: {}:{}", host, port);
                    update(TrayUpdate::Status(TrayStatus::Healthy));
                    let _ = notify_rust::Notification::new()
                        .summary("HoF-blaze connected")
                        .body(&format!("Connected to: {}:{}", host, port))
                        .show();
                }
                TrayEvent::StatusDisconnected => {
                    info!("Game disconnected.");
                    update(TrayUpdate::Status(TrayStatus::Disconnected));
                    let _ = notify_rust::Notification::new()
                        .summary("HoF-blaze disconnected")
                        .show();
                }
                TrayEvent::StatusFaulty { error } => {
                    warn!("Game status faulty: {}", error);
                    update(TrayUpdate::Status(TrayStatus::Faulty));
                }
                TrayEvent::GameStarted {
                    name,
                    display_name,
                    file_error,
                } => {
                    update(TrayUpdate::Game(Some(RunningGame {
                        label: game_label(&name, display_name.as_deref()),
                        file_error,
                    })));
                }
                TrayEvent::GameEnded => update(TrayUpdate::Game(None)),
            }
        }
    });
}

/// Shows the tray icon and runs its event loop on the current thread until Exit or
/// "Open HoF-forge" is chosen in the tray menu.
///
/// Must be called on the main thread: macOS only allows the UI on the main thread, and on
/// Linux GTK (also used by the startup message box) must be driven from a single thread.
/// `TrayEvent`s are received by a task on `runtime`.
#[cfg(target_os = "linux")]
pub fn run(
    runtime: &tokio::runtime::Handle,
    rx: tokio::sync::mpsc::Receiver<TrayEvent>,
    reload_tx: tokio::sync::mpsc::Sender<StateEvent>,
    version_string: &str,
) -> Result<TrayExit> {
    use std::cell::RefCell;
    use std::rc::Rc;
    use std::sync::mpsc;
    use std::time::Duration;

    gtk::init().context("Failed to initialize GTK")?;

    let mut tray = Tray::build(version_string, reload_tx)?;

    let (update_tx, update_rx) = mpsc::channel::<TrayUpdate>();
    spawn_event_forwarder(runtime, rx, move |update| {
        let _ = update_tx.send(update);
    });

    // Poll status updates and menu events on the GTK main thread.
    let menu_receiver = MenuEvent::receiver();
    let exit = Rc::new(RefCell::new(TrayExit::Exit));
    let exit_clone = Rc::clone(&exit);
    gtk::glib::timeout_add_local(Duration::from_millis(100), move || {
        while let Ok(update) = update_rx.try_recv() {
            tray.update(update);
        }
        while let Ok(event) = menu_receiver.try_recv() {
            if let Some(tray_exit) = tray.handle_menu_event(&event) {
                *exit_clone.borrow_mut() = tray_exit;
                gtk::main_quit();
                return gtk::glib::ControlFlow::Break;
            }
        }
        gtk::glib::ControlFlow::Continue
    });

    gtk::main();
    Ok(exit.replace(TrayExit::Exit))
}

/// Shows the tray icon and runs its event loop on the current thread until Exit or
/// "Open HoF-forge" is chosen in the tray menu.
///
/// Must be called on the main thread: macOS only allows the UI on the main thread, and winit
/// refuses to create its event loop anywhere else. `TrayEvent`s are received by a task on
/// `runtime`.
#[cfg(not(target_os = "linux"))]
pub fn run(
    runtime: &tokio::runtime::Handle,
    rx: tokio::sync::mpsc::Receiver<TrayEvent>,
    reload_tx: tokio::sync::mpsc::Sender<StateEvent>,
    version_string: &str,
) -> Result<TrayExit> {
    use winit::application::ApplicationHandler;
    use winit::event::{StartCause, WindowEvent};
    use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
    use winit::window::WindowId;

    /// Wakes up the event loop from other threads.
    enum UserEvent {
        Update(TrayUpdate),
        Menu(MenuEvent),
    }

    struct TrayApp {
        version_string: String,
        /// Taken when the tray is built.
        reload_tx: Option<tokio::sync::mpsc::Sender<StateEvent>>,
        tray: Option<Tray>,
        result: Result<()>,
        exit: TrayExit,
    }

    impl ApplicationHandler<UserEvent> for TrayApp {
        fn new_events(&mut self, event_loop: &ActiveEventLoop, cause: StartCause) {
            // The tray icon can only be created once the event loop runs (macOS).
            if cause == StartCause::Init {
                let Some(reload_tx) = self.reload_tx.take() else {
                    return;
                };
                match Tray::build(&self.version_string, reload_tx) {
                    Ok(tray) => self.tray = Some(tray),
                    Err(err) => {
                        self.result = Err(err);
                        event_loop.exit();
                    }
                }
            }
        }

        fn resumed(&mut self, _event_loop: &ActiveEventLoop) {}

        fn window_event(
            &mut self,
            _event_loop: &ActiveEventLoop,
            _window_id: WindowId,
            _event: WindowEvent,
        ) {
        }

        fn user_event(&mut self, event_loop: &ActiveEventLoop, event: UserEvent) {
            let Some(tray) = &mut self.tray else {
                return;
            };
            match event {
                UserEvent::Update(update) => tray.update(update),
                UserEvent::Menu(event) => {
                    if let Some(exit) = tray.handle_menu_event(&event) {
                        self.exit = exit;
                        event_loop.exit();
                    }
                }
            }
        }
    }

    let mut builder = EventLoop::<UserEvent>::with_user_event();
    // No Dock icon and no menu bar: hof-blaze only lives in the menu bar extras.
    #[cfg(target_os = "macos")]
    {
        use winit::platform::macos::{ActivationPolicy, EventLoopBuilderExtMacOS};
        builder.with_activation_policy(ActivationPolicy::Accessory);
    }
    let event_loop = builder.build().context("Failed to create event loop")?;
    event_loop.set_control_flow(ControlFlow::Wait);

    let proxy = event_loop.create_proxy();
    MenuEvent::set_event_handler(Some(move |event| {
        let _ = proxy.send_event(UserEvent::Menu(event));
    }));
    let proxy = event_loop.create_proxy();
    spawn_event_forwarder(runtime, rx, move |update| {
        let _ = proxy.send_event(UserEvent::Update(update));
    });

    let mut app = TrayApp {
        version_string: version_string.to_owned(),
        reload_tx: Some(reload_tx),
        tray: None,
        result: Ok(()),
        exit: TrayExit::Exit,
    };
    event_loop.run_app(&mut app).context("Event loop error")?;
    // Stop sending menu events to the closed event loop.
    MenuEvent::set_event_handler(None::<fn(MenuEvent)>);
    app.result.map(|()| app.exit)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn game_label_shows_display_name_and_game_name() {
        assert_eq!(
            game_label("lostwsga", Some("The Lost World: Jurassic Park")),
            "The Lost World: Jurassic Park - lostwsga"
        );
        assert_eq!(game_label("lostwsga", None), "lostwsga");
        assert_eq!(game_label("jp", Some("jp")), "jp");
    }
}
