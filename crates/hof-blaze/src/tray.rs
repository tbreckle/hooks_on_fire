use anyhow::{Context, Result};
use hof_common::events::TrayEvent;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};
use std::thread::JoinHandle;
use tracing::{info, warn};
use tray_icon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, TrayIconBuilder};

const ICON_BYTES_HEALTHY: &[u8] = include_bytes!("../../../assets/icon_healthy.png");
const ICON_BYTES_FAULTY: &[u8] = include_bytes!("../../../assets/icon_faulty.png");
const ICON_BYTES_WAITING: &[u8] = include_bytes!("../../../assets/icon_waiting.png");

/// Builds a tray `Icon` from the embedded PNG.
fn load_icon(icon_bytes: &[u8]) -> Result<Icon> {
    let img = image::load_from_memory(icon_bytes)
        .context("Failed to decode embedded icon")?
        .into_rgba8();
    let (w, h) = img.dimensions();
    Icon::from_rgba(img.into_raw(), w, h).context("Failed to create tray icon")
}

/// Creates the tray icon with a context menu.
fn build_tray(version_string: &str) -> Result<(tray_icon::TrayIcon, MenuItem)> {
    let icon = load_icon(ICON_BYTES_WAITING)?;
    let menu = Menu::new();

    let top_item: MenuItem = MenuItem::new("HoF-blaze ".to_owned() + version_string, false, None);
    let status_item: MenuItem = MenuItem::new("Status: Waiting", false, None);

    let exit_item: MenuItem = MenuItem::with_id("exit_id", "&Exit", true, None);
    let open_forge: MenuItem = MenuItem::with_id("forge_id", "Open HoF-&forge", true, None);

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

    let tray = TrayIconBuilder::new()
        .with_menu(Box::new(menu))
        .with_tooltip(format!("Hooks on Fire - {version_string}"))
        .with_icon(icon)
        .build()
        .context("Failed to build tray icon")?;

    Ok((tray, status_item))
}

pub struct TrayHandle {
    should_stop: Arc<AtomicBool>,
    thread_handle: JoinHandle<anyhow::Result<()>>,
    shutdown_rx: tokio::sync::mpsc::Receiver<()>,
}

impl TrayHandle {
    pub fn shutdown(self) -> anyhow::Result<()> {
        self.should_stop.store(true, Ordering::Relaxed);
        self.thread_handle
            .join()
            .map_err(|_| anyhow::anyhow!("Tray thread panicked."))?
    }

    pub async fn wait_for_exit(&mut self) {
        let _ = self.shutdown_rx.recv().await;
    }
}

/// Starts the tray icon on a background thread (non-blocking).
/// Returns immediately, spawning the event loop on a separate thread.
pub async fn start_tray(
    mut rx: tokio::sync::mpsc::Receiver<TrayEvent>,
    version_string: String,
) -> anyhow::Result<TrayHandle> {
    let should_stop = Arc::new(AtomicBool::new(false));
    let running_from_should_stop = Arc::new(AtomicBool::new(!should_stop.load(Ordering::Relaxed)));

    // Create channel for cross-thread icon updates.
    let (tx_icon, rx_icon) = mpsc::channel::<u8>();
    let tx_icon_clone = tx_icon.clone();

    // Create channel for shutdown signaling
    let (shutdown_tx, shutdown_rx) = tokio::sync::mpsc::channel::<()>(1);

    let thread_handle = std::thread::spawn(move || -> anyhow::Result<()> {
        run_tray_loop(
            version_string,
            running_from_should_stop,
            tx_icon,
            rx_icon,
            shutdown_tx,
        )
    });

    // Spawn async task to handle TrayEvents
    tokio::spawn(async move {
        while let Some(event) = rx.recv().await {
            match event {
                TrayEvent::StatusConnected { host, port } => {
                    info!("Remote connected: {}:{}", host, port);
                    let _ = tx_icon_clone.send(1);
                    let _ = notify_rust::Notification::new()
                        .summary("HoF-blaze connected")
                        .body(&format!("Connected to: {}:{}", host, port))
                        .show();
                }
                TrayEvent::StatusDisconnected => {
                    info!("Game disconnected.");
                    let _ = tx_icon_clone.send(0);
                    let _ = notify_rust::Notification::new()
                        .summary("HoF-blaze disconnected")
                        // .body("Disconnected.")
                        .show();
                }
                TrayEvent::StatusFaulty { error } => {
                    warn!("Game status faulty: {}", error);
                    let _ = tx_icon_clone.send(255);
                }
            }
        }
    });

    Ok(TrayHandle {
        should_stop,
        thread_handle,
        shutdown_rx,
    })
}

/// Runs the platform-specific tray event loop.
#[cfg(target_os = "linux")]
fn run_tray_loop(
    version_string: String,
    running: Arc<AtomicBool>,
    _tx: mpsc::Sender<u8>,
    rx: mpsc::Receiver<u8>,
    shutdown_tx: tokio::sync::mpsc::Sender<()>,
) -> Result<()> {
    gtk::init().context("Failed to initialize GTK")?;

    let (_tray, status_item) = build_tray(&version_string)?;

    // Handle icon updates on GTK main thread.
    let tray_clone = _tray.clone();
    let status_item_clone = status_item.clone();
    gtk::glib::idle_add_local(move || {
        if let Ok(status) = rx.try_recv() {
            if status == 1 {
                let new_icon = load_icon(ICON_BYTES_HEALTHY).unwrap();
                tray_clone.set_icon(Some(new_icon)).unwrap();
                status_item_clone.set_text("Status: Healthy");
            } else if status == 0 {
                let new_icon = load_icon(ICON_BYTES_WAITING).unwrap();
                tray_clone.set_icon(Some(new_icon)).unwrap();
                status_item_clone.set_text("Status: Disconnected");
            } else if status == 255 {
                let new_icon = load_icon(ICON_BYTES_FAULTY).unwrap();
                tray_clone.set_icon(Some(new_icon)).unwrap();
                status_item_clone.set_text("Status: Faulty");
            }
        }
        gtk::glib::ControlFlow::Continue
    });

    // Run GTK main loop, polling for menu events.
    let menu_receiver = MenuEvent::receiver();
    gtk::glib::timeout_add_local(std::time::Duration::from_millis(100), move || {
        if !running.load(Ordering::Relaxed) {
            tracing::info!("Exit requested via signal.");
            gtk::main_quit();
            return gtk::glib::ControlFlow::Break;
        }
        if let Ok(event) = menu_receiver.try_recv() {
            if event.id == "exit_id" {
                tracing::info!("Exit requested via tray menu.");
                running.store(false, Ordering::Relaxed);
                let _ = shutdown_tx.blocking_send(());
                gtk::main_quit();
                return gtk::glib::ControlFlow::Break;
            } else if event.id == "forge_id" {
                tracing::info!("Switching to HoF-Forge.");
                let new_icon = load_icon(ICON_BYTES_HEALTHY).unwrap();
                _tray.set_icon(Some(new_icon)).unwrap();
            }
        }
        gtk::glib::ControlFlow::Continue
    });

    gtk::main();
    Ok(())
}

/// Runs the tray event loop for Windows / macOS using winit.
#[cfg(not(target_os = "linux"))]
fn run_tray_loop(
    version_string: String,
    running: Arc<AtomicBool>,
    _tx: mpsc::Sender<u8>,
    rx: mpsc::Receiver<u8>,
    shutdown_tx: tokio::sync::mpsc::Sender<()>,
) -> Result<()> {
    use winit::application::ApplicationHandler;
    use winit::event::WindowEvent;
    use winit::event_loop::{ActiveEventLoop, EventLoop};
    use winit::window::WindowId;

    struct TrayApp {
        version_string: String,
        _tray: Option<tray_icon::TrayIcon>,
        status_item: Option<MenuItem>,
        running: Arc<AtomicBool>,
        rx: mpsc::Receiver<u8>,
        shutdown_tx: tokio::sync::mpsc::Sender<()>,
    }

    impl ApplicationHandler for TrayApp {
        fn resumed(&mut self, _event_loop: &ActiveEventLoop) {}

        fn window_event(
            &mut self,
            _event_loop: &ActiveEventLoop,
            _window_id: WindowId,
            _event: WindowEvent,
        ) {
        }

        fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
            if self._tray.is_none() {
                match build_tray(&self.version_string) {
                    Ok((tray, status_item)) => {
                        self._tray = Some(tray);
                        self.status_item = Some(status_item);
                    }
                    Err(e) => {
                        tracing::error!("Failed to create tray icon: {e:#}");
                        event_loop.exit();
                        return;
                    }
                }
            }

            // Handle icon updates from channel
            if let Ok(status) = self.rx.try_recv() {
                if let (Some(tray), Some(status_item)) = (&self._tray, &self.status_item) {
                    if status == 1 {
                        let new_icon = load_icon(ICON_BYTES_HEALTHY).unwrap();
                        tray.set_icon(Some(new_icon)).unwrap();
                        status_item.set_text("Status: Healthy");
                    } else if status == 0 {
                        let new_icon = load_icon(ICON_BYTES_WAITING).unwrap();
                        tray.set_icon(Some(new_icon)).unwrap();
                        status_item.set_text("Status: Disconnected");
                    } else if status == 255 {
                        let new_icon = load_icon(ICON_BYTES_FAULTY).unwrap();
                        tray.set_icon(Some(new_icon)).unwrap();
                        status_item.set_text("Status: Faulty");
                    }
                }
            }

            let menu_receiver = MenuEvent::receiver();
            if let Ok(event) = menu_receiver.try_recv() {
                if event.id == "exit_id" {
                    tracing::info!("Exit requested via tray menu.");
                    self.running.store(false, Ordering::Relaxed);
                    let _ = self.shutdown_tx.blocking_send(());
                    event_loop.exit();
                } else if event.id == "forge_id" {
                    tracing::info!("Switching to HoF-Forge.");
                }
            }

            if !self.running.load(Ordering::Relaxed) {
                tracing::info!("Exit requested via signal.");
                event_loop.exit();
            }
        }
    }

    let event_loop = EventLoop::new().context("Failed to create event loop")?;
    event_loop.set_control_flow(winit::event_loop::ControlFlow::Wait);

    let mut app = TrayApp {
        version_string,
        _tray: None,
        status_item: None,
        running,
        rx,
        shutdown_tx,
    };

    event_loop.run_app(&mut app).context("Event loop error")?;
    Ok(())
}
