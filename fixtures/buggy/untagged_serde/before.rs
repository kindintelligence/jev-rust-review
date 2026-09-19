use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind")]
pub enum Event {
    Payment { order_id: String, amount: u64 },
    Refund { order_id: String, amount: u64, reason: Option<String> },
}
