use anyhow::{Context, Result};
use hof_common::events::TrayEvent;
use tracing::{info, warn};
use tray_icon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder};

const ICON_BYTES_HEALTHY: &[u8] = include_bytes!("../../../assets/icon_healthy.png");
const ICON_BYTES_FAULTY: &[u8] = include_bytes!("../../../assets/icon_faulty.png");
const ICON_BYTES_WAITING: &[u8] = include_bytes!("../../../assets/icon_waiting.png");

const EXIT_ID: &str = "exit_id";
const FORGE_ID: &str = "forge_id";

/// Connection status shown by the tray icon and the status menu item.
#[derive(Clone, Copy, Debug)]
enum TrayStatus {
    Healthy,
    Disconnected,
    Faulty,
}

/// Builds a tray `Icon` from the embedded PNG.
fn load_icon(icon_bytes: &[u8]) -> Result<Icon> {
    let img = image::load_from_memory(icon_bytes)
        .context("Failed to decode embedded icon")?
        .into_rgba8();
    let (w, h) = img.dimensions();
    Icon::from_rgba(img.into_raw(), w, h).context("Failed to create tray icon")
}

/// The tray icon and the menu item showing the status.
struct Tray {
    icon: TrayIcon,
    status_item: MenuItem,
}

impl Tray {
    /// Creates the tray icon with a context menu.
    fn build(version_string: &str) -> Result<Self> {
        let icon = load_icon(ICON_BYTES_WAITING)?;
        let menu = Menu::new();

        let top_item: MenuItem =
            MenuItem::new("HoF-blaze ".to_owned() + version_string, false, None);
        let status_item: MenuItem = MenuItem::new("Status: Waiting", false, None);

        let exit_item: MenuItem = MenuItem::with_id(EXIT_ID, "&Exit", true, None);
        let open_forge: MenuItem = MenuItem::with_id(FORGE_ID, "Open HoF-&forge", true, None);

        let separator: PredefinedMenuItem = PredefinedMenuItem::separator();

        menu.append(&top_item)
            .context("Failed to append menu item for app name")?;
        menu.append(&status_item)
            .context("Failed to append menu item for status")?;
        menu.append(&separator)
            .context("Failed to append menu separator.")?;
        menu.append(&open_forge)
            .context("Failed to append menu item for opening forge.")?;
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

        Ok(Self { icon, status_item })
    }

    fn set_status(&self, status: TrayStatus) {
        let (icon_bytes, text) = match status {
            TrayStatus::Healthy => (ICON_BYTES_HEALTHY, "Status: Healthy"),
            TrayStatus::Disconnected => (ICON_BYTES_WAITING, "Status: Disconnected"),
            TrayStatus::Faulty => (ICON_BYTES_FAULTY, "Status: Faulty"),
        };
        match load_icon(icon_bytes).and_then(|icon| {
            self.icon
                .set_icon(Some(icon))
                .context("Failed to set tray icon")
        }) {
            Ok(()) => self.status_item.set_text(text),
            Err(err) => warn!("Failed to update tray icon: {err:#}"),
        }
    }

    /// Handles a click on a menu item. Returns `true` if Exit was chosen.
    fn handle_menu_event(&self, event: &MenuEvent) -> bool {
        if event.id == EXIT_ID {
            info!("Exit requested via tray menu.");
            return true;
        }
        if event.id == FORGE_ID {
            info!("Switching to HoF-Forge.");
        }
        false
    }
}

/// Receives `TrayEvent`s on the Tokio runtime, shows notifications and passes the new status
/// to the tray loop via `set_status`.
fn spawn_event_forwarder(
    runtime: &tokio::runtime::Handle,
    mut rx: tokio::sync::mpsc::Receiver<TrayEvent>,
    set_status: impl Fn(TrayStatus) + Send + 'static,
) {
    runtime.spawn(async move {
        while let Some(event) = rx.recv().await {
            match event {
                TrayEvent::StatusConnected { host, port } => {
                    info!("Remote connected: {}:{}", host, port);
                    set_status(TrayStatus::Healthy);
                    let _ = notify_rust::Notification::new()
                        .summary("HoF-blaze connected")
                        .body(&format!("Connected to: {}:{}", host, port))
                        .show();
                }
                TrayEvent::StatusDisconnected => {
                    info!("Game disconnected.");
                    set_status(TrayStatus::Disconnected);
                    let _ = notify_rust::Notification::new()
                        .summary("HoF-blaze disconnected")
                        .show();
                }
                TrayEvent::StatusFaulty { error } => {
                    warn!("Game status faulty: {}", error);
                    set_status(TrayStatus::Faulty);
                }
            }
        }
    });
}

/// Shows the tray icon and runs its event loop on the current thread until Exit is chosen in
/// the tray menu.
///
/// Must be called on the main thread: macOS only allows the UI on the main thread, and on
/// Linux GTK (also used by the startup message box) must be driven from a single thread.
/// `TrayEvent`s are received by a task on `runtime`.
#[cfg(target_os = "linux")]
pub fn run(
    runtime: &tokio::runtime::Handle,
    rx: tokio::sync::mpsc::Receiver<TrayEvent>,
    version_string: &str,
) -> Result<()> {
    use std::sync::mpsc;
    use std::time::Duration;

    gtk::init().context("Failed to initialize GTK")?;

    let tray = Tray::build(version_string)?;

    let (status_tx, status_rx) = mpsc::channel::<TrayStatus>();
    spawn_event_forwarder(runtime, rx, move |status| {
        let _ = status_tx.send(status);
    });

    // Poll status updates and menu events on the GTK main thread.
    let menu_receiver = MenuEvent::receiver();
    gtk::glib::timeout_add_local(Duration::from_millis(100), move || {
        while let Ok(status) = status_rx.try_recv() {
            tray.set_status(status);
        }
        while let Ok(event) = menu_receiver.try_recv() {
            if tray.handle_menu_event(&event) {
                gtk::main_quit();
                return gtk::glib::ControlFlow::Break;
            }
        }
        gtk::glib::ControlFlow::Continue
    });

    gtk::main();
    Ok(())
}

/// Shows the tray icon and runs its event loop on the current thread until Exit is chosen in
/// the tray menu.
///
/// Must be called on the main thread: macOS only allows the UI on the main thread, and winit
/// refuses to create its event loop anywhere else. `TrayEvent`s are received by a task on
/// `runtime`.
#[cfg(not(target_os = "linux"))]
pub fn run(
    runtime: &tokio::runtime::Handle,
    rx: tokio::sync::mpsc::Receiver<TrayEvent>,
    version_string: &str,
) -> Result<()> {
    use winit::application::ApplicationHandler;
    use winit::event::{StartCause, WindowEvent};
    use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
    use winit::window::WindowId;

    /// Wakes up the event loop from other threads.
    enum UserEvent {
        Status(TrayStatus),
        Menu(MenuEvent),
    }

    struct TrayApp {
        version_string: String,
        tray: Option<Tray>,
        result: Result<()>,
    }

    impl ApplicationHandler<UserEvent> for TrayApp {
        fn new_events(&mut self, event_loop: &ActiveEventLoop, cause: StartCause) {
            // The tray icon can only be created once the event loop runs (macOS).
            if cause == StartCause::Init {
                match Tray::build(&self.version_string) {
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
            let Some(tray) = &self.tray else {
                return;
            };
            match event {
                UserEvent::Status(status) => tray.set_status(status),
                UserEvent::Menu(event) => {
                    if tray.handle_menu_event(&event) {
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
    spawn_event_forwarder(runtime, rx, move |status| {
        let _ = proxy.send_event(UserEvent::Status(status));
    });

    let mut app = TrayApp {
        version_string: version_string.to_owned(),
        tray: None,
        result: Ok(()),
    };
    event_loop.run_app(&mut app).context("Event loop error")?;
    // Stop sending menu events to the closed event loop.
    MenuEvent::set_event_handler(None::<fn(MenuEvent)>);
    app.result
}
