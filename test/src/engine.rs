use crate::event::SerialEvent;
use tokio::sync::mpsc;

pub async fn run_engine(mut rx: mpsc::Receiver<SerialEvent>) {
    let mut counter: u32 = 0;
    while let Some(event) = rx.recv().await {
        match event {
            SerialEvent::NewLine { line } => {
                println!("Engine received: {}", line);
                counter += 1;
                println!("Engine counter: {}", counter);
            }
        }
    }
}
