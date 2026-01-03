use serde::{Deserialize, Serialize};

pub const FEATURE_SCHEMA_VERSION: i32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateSnapshot {
    pub snapshot_id: String,
    pub market_id: i64,
    pub ts_ms: i64,
}

pub fn initial_snapshot() -> StateSnapshot {
    StateSnapshot {
        snapshot_id: "bootstrap".into(),
        market_id: 0,
        ts_ms: chrono::Utc::now().timestamp_millis(),
    }
}
