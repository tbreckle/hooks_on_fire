use anyhow::Context;
use hof_common::events::LineEvent;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;
use tracing::{debug, error, info};

pub struct UdpReceiverHandle {
    should_stop: Arc<AtomicBool>,
    thread_handle: JoinHandle<anyhow::Result<()>>,
}

impl UdpReceiverHandle {
    pub fn shutdown(self) -> anyhow::Result<()> {
        self.should_stop.store(true, Ordering::Relaxed);
        self.thread_handle
            .join()
            .map_err(|_| anyhow::anyhow!("UDP receiver thread panicked."))?
    }
}

pub fn start_udp_receiver(
    tx: tokio::sync::mpsc::Sender<LineEvent>,
    port: u16,
) -> anyhow::Result<UdpReceiverHandle> {
    let should_stop = Arc::new(AtomicBool::new(false));
    let should_stop_clone = Arc::clone(&should_stop);

    let thread_handle = std::thread::spawn(move || -> anyhow::Result<()> {
        let port: u16 = port;
        if port <= 1024 {
            error!(
                "Invalid UDP port specified: {}. Use a port between 1025 and 65535.",
                port
            );
            return Err(anyhow::anyhow!("Invalid UDP port specified: {}.", port));
        }

        info!("Waiting for UDP broadcast on port: {}", port);

        let bind_addr = format!("0.0.0.0:{}", port);
        let socket = std::net::UdpSocket::bind(&bind_addr)
            .with_context(|| format!("Failed to bind UDP socket to {bind_addr}"))?;
        socket
            .set_read_timeout(Some(std::time::Duration::from_millis(250)))
            .context("Failed to set UDP socket read timeout")?;
        socket
            .set_broadcast(true)
            .context("Failed to enable UDP broadcast")?;

        let mut buffer = [0u8; 1500];

        while !should_stop_clone.load(Ordering::Relaxed) {
            match socket.recv_from(&mut buffer) {
                Ok((size, peer)) => {
                    debug!("Received UDP broadcast from {peer} ({size} bytes)");
                    let content = String::from_utf8_lossy(&buffer[..size]);
                    for line in content.split(['\r', '\n']) {
                        if !line.is_empty() {
                            info!("UDP line received: {line}");
                            if tx
                                .blocking_send(LineEvent::NewLine { line: line.into() })
                                .is_err()
                            {
                                error!("Failed to send line event, receiver may have been dropped. Stopping UDP receiver.");
                                break;
                            }
                        }
                    }
                }
                Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                    // Timeout reached; loop again to check shutdown flag.
                }
                Err(err) if err.kind() == std::io::ErrorKind::TimedOut => {
                    // Timeout reached; loop again to check shutdown flag.
                }
                Err(err) => {
                    return Err(err).context("UDP receive failed.");
                }
            }
        }
        Ok(())
    });

    Ok(UdpReceiverHandle {
        should_stop,
        thread_handle,
    })
}
