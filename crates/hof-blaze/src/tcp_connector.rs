use anyhow::Context;
use hof_common::events::{LineEvent, TrayEvent};
use std::io::Read;
use std::net::TcpStream;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};
use tracing::{debug, error, info, warn};

/// How often the "waiting for connection" message is logged while nothing is listening.
const WAITING_LOG_INTERVAL: Duration = Duration::from_secs(10);

pub struct TcpConnectorHandle {
    should_stop: Arc<AtomicBool>,
    thread_handle: JoinHandle<anyhow::Result<()>>,
}

impl TcpConnectorHandle {
    pub fn shutdown(self) -> anyhow::Result<()> {
        self.should_stop.store(true, Ordering::Relaxed);
        self.thread_handle
            .join()
            .map_err(|_| anyhow::anyhow!("TCP connector thread panicked."))?
    }
}

pub fn start_tcp_connector(
    tx: tokio::sync::mpsc::Sender<LineEvent>,
    host: String,
    port: u16,
    tray_tx: tokio::sync::mpsc::Sender<TrayEvent>,
) -> anyhow::Result<TcpConnectorHandle> {
    let should_stop = Arc::new(AtomicBool::new(false));
    let should_stop_clone = Arc::clone(&should_stop);

    let thread_handle = std::thread::spawn(move || -> anyhow::Result<()> {
        info!(
            "TCP connector initialized with host: {}, port: {}",
            host, port
        );
        let mut retry_count = 0;
        let mut last_waiting_log: Option<Instant> = None;

        loop {
            if should_stop_clone.load(Ordering::Relaxed) {
                info!("Requested TCP connector stop in unconnected state.");
                break;
            }

            match TcpStream::connect(format!("{}:{}", host, port)) {
                Ok(mut stream) => {
                    info!("TCP connected to {}:{}", host, port);
                    let _ = tray_tx.blocking_send(TrayEvent::StatusConnected {
                        host: host.clone(),
                        port,
                    });
                    retry_count = 0;
                    last_waiting_log = None;

                    stream
                        .set_read_timeout(Some(Duration::from_secs(1)))
                        .context("Failed to set TCP read timeout")?;

                    let mut buffer = [0u8; 4096];
                    let mut accumulated = String::new();

                    loop {
                        if should_stop_clone.load(Ordering::Relaxed) {
                            info!("Requested TCP connector stop.");
                            let _ = tray_tx.blocking_send(TrayEvent::StatusDisconnected {});
                            return Ok(());
                        }

                        match stream.read(&mut buffer) {
                            Ok(0) => {
                                info!("TCP connection closed by server. Try to reconnect.");
                                let _ = tray_tx.blocking_send(TrayEvent::StatusDisconnected {});
                                break;
                            }
                            Ok(size) => {
                                let chunk = String::from_utf8_lossy(&buffer[..size]);
                                accumulated.push_str(&chunk);

                                // Find last line break position.
                                // If found, process all complete lines and keep the remainder for next read.
                                if let Some(last_break) = accumulated.rfind(['\r', '\n']) {
                                    // Process everything up to and including last break.
                                    let to_process = &accumulated[..=last_break];
                                    for line in to_process.split(['\r', '\n']) {
                                        if !line.is_empty() {
                                            debug!("TCP line received: {}", line);
                                            if tx
                                                .blocking_send(LineEvent::NewLine {
                                                    line: line.into(),
                                                })
                                                .is_err()
                                            {
                                                error!("Failed to send line event, receiver may have been dropped. Stopping TCP connector.");
                                                let _ = tray_tx.blocking_send(
                                                    TrayEvent::StatusDisconnected {},
                                                );
                                                return Ok(());
                                            }
                                        }
                                    }
                                    // Keep only incomplete line after last break.
                                    accumulated = accumulated[last_break + 1..].to_string();
                                }
                            }
                            Err(err) => {
                                if err.kind() == std::io::ErrorKind::WouldBlock
                                    || err.kind() == std::io::ErrorKind::TimedOut
                                {
                                    // Timeout occurred, loop again to check shutdown flag.
                                    continue;
                                }
                                error!("TCP read error: {}", err);
                                let _ = tray_tx.blocking_send(TrayEvent::StatusFaulty {
                                    error: err.to_string(),
                                });
                                break;
                            }
                        }
                    }
                }
                Err(err) if err.kind() == std::io::ErrorKind::ConnectionRefused => {
                    // Nothing is listening yet (e.g. MAME not running). This is the normal
                    // waiting state, so only log it every few seconds.
                    if last_waiting_log.is_none_or(|t| t.elapsed() >= WAITING_LOG_INTERVAL) {
                        info!("Waiting for connection to {}:{}...", host, port);
                        last_waiting_log = Some(Instant::now());
                    }
                    std::thread::sleep(Duration::from_secs(1));
                }
                Err(err) => {
                    retry_count += 1;
                    // Log the error and retry after a delay.
                    warn!("TCP connection failed (attempt {}): {}", retry_count, err);
                    std::thread::sleep(Duration::from_secs(1));
                }
            }
        }
        Ok(())
    });

    Ok(TcpConnectorHandle {
        should_stop,
        thread_handle,
    })
}
