use serde::{Deserialize, Serialize};
use strategies::Intent;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Approval {
    pub approved_id: String,
    pub approved: bool,
    pub reason: String,
    pub intent: Intent,
}

pub fn approve(intent: Intent) -> Approval {
    Approval {
        approved_id: Uuid::new_v4().to_string(),
        approved: true,
        reason: "ok".into(),
        intent,
    }
}
