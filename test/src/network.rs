use crate::event::NetworkEvent;
use std::thread;
use std::time::Duration;
use tokio::sync::mpsc;

pub fn start_network_sender(tx: mpsc::Sender<NetworkEvent>) {
    thread::spawn(move || {
        let mut seq = 0u32;
        loop {
            let payload = format!("packet #{seq}");
            if tx.blocking_send(NetworkEvent::Data { payload }).is_err() {
                break;
            }
            seq += 1;
            thread::sleep(Duration::from_millis(200));
        }
    });
}
