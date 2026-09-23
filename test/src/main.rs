mod engine;
mod event;
// mod inventory_consumer;
mod network;
// mod order_consumer;
mod serial;

// use event::{InventoryEvent, OrderEvent};
use tokio::sync::mpsc;

use crate::event::{NetworkEvent, SerialEvent};

#[tokio::main]
async fn main() {
    // 1. Create channels for both consumers
    // let (order_tx, order_rx) = mpsc::channel::<OrderEvent>(32);
    // let (inv_tx, inv_rx) = mpsc::channel::<InventoryEvent>(32);

    // 2. Spawn Inventory Consumer
    // tokio::spawn(inventory_consumer::run_inventory(inv_rx));

    let (engine_tx, engine_rx) = mpsc::channel::<SerialEvent>(32);
    tokio::spawn(engine::run_engine(engine_rx));

    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    println!("Sending");
    tokio::spawn(serial::run_serial(engine_tx));

    // Network: sender runs in its own OS thread, receiver prints in main.
    let (network_tx, mut network_rx) = mpsc::channel::<NetworkEvent>(32);
    network::start_network_sender(network_tx);

    // // 3. Spawn Order Consumer (Give it the Inventory Sender)
    // let order_tx_for_itself = order_tx.clone();
    // tokio::spawn(order_consumer::run_orders(
    //     order_rx,
    //     inv_tx,
    //     order_tx_for_itself,
    // ));

    // // 4. Trigger an event from Main
    // println!("Main: Sending an order...");
    // order_tx
    //     .send(OrderEvent::PlaceOrder { item_id: 1, qty: 5 })
    //     .await
    //     .unwrap();

    // Receive network data in main until the 1-second deadline.
    println!("Quitting in 1 second...");
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(1);
    loop {
        match tokio::time::timeout_at(deadline, network_rx.recv()).await {
            Ok(Some(NetworkEvent::Data { payload })) => {
                println!("Main received network data: {payload}");
            }
            _ => break,
        }
    }
}
