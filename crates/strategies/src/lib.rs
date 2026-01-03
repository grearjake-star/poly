use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum IntentKind {
    PlaceOrder,
    CancelOrder,
    CancelAll,
    FlattenMarket,
    NoOp,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Intent {
    pub intent_id: String,
    pub market_id: i64,
    pub kind: IntentKind,
    pub expected_value: f64,
}

pub trait Strategy {
    fn propose(&self) -> Intent;
}
