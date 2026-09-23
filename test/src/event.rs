// use tokio::sync::oneshot;

pub enum SerialEvent {
    NewLine { line: String },
}

pub enum NetworkEvent {
    Data { payload: String },
}

// // Messages for the Order Consumer
// pub enum OrderEvent {
//     PlaceOrder { item_id: u32, qty: u32 },
//     UpdateStatus { item_id: u32, success: bool },
// }

// // Messages for the Inventory Consumer
// pub enum InventoryEvent {
//     CheckStock {
//         item_id: u32,
//         qty: u32,
//         respond_to: tokio::sync::mpsc::Sender<OrderEvent> // Callback channel!
//     },
// }
