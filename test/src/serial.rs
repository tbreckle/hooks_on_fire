use crate::event::SerialEvent;
use tokio::sync::mpsc;

pub async fn run_serial(tx: mpsc::Sender<SerialEvent>) {
    tx.send(SerialEvent::NewLine {
        line: "Hello from Serial!".to_string(),
    })
    .await
    .unwrap();

    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    tx.send(SerialEvent::NewLine {
        line: "Another line from Serial!".to_string(),
    })
    .await
    .unwrap();
}
