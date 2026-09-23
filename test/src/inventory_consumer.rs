use crate::event::{InventoryEvent, OrderEvent};
use tokio::sync::mpsc;

pub async fn run_inventory(mut rx: mpsc::Receiver<InventoryEvent>) {
    let mut stock_count = 100; // Private state, no Mutex!

    while let Some(event) = rx.recv().await {
        match event {
            InventoryEvent::CheckStock { item_id, qty, respond_to } => {
                let success = stock_count >= qty;
                if success { stock_count -= qty; }
                
                println!("Inventory: Item {} check. Success: {}. Stock left: {}", item_id, success, stock_count);
                
                // Communication: Send a message back to the Order Consumer
                let _ = respond_to.send(OrderEvent::UpdateStatus { item_id, success }).await;
            }
        }
    }
}
