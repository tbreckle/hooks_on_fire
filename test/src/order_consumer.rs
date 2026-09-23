use crate::event::{OrderEvent, InventoryEvent};
use tokio::sync::mpsc;

pub async fn run_orders(
    mut rx: mpsc::Receiver<OrderEvent>, 
    inventory_tx: mpsc::Sender<InventoryEvent>,
    self_tx: mpsc::Sender<OrderEvent> // We need our own sender to give to others
) {
    while let Some(event) = rx.recv().await {
        match event {
            OrderEvent::PlaceOrder { item_id, qty } => {
                println!("Orders: Placing order for item {}", item_id);
                // Ask Inventory to check stock
                let _ = inventory_tx.send(InventoryEvent::CheckStock { 
                    item_id, 
                    qty, 
                    respond_to: self_tx.clone() 
                }).await;
            }
            OrderEvent::UpdateStatus { item_id, success } => {
                println!("Orders: Order for {} was finalized. Success: {}", item_id, success);
            }
        }
    }
}
