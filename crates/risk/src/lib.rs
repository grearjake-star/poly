use std::sync::{Arc, RwLock};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RiskState {
    #[default]
    Active,
    Paused,
}

#[derive(Clone, Default)]
pub struct RiskGate {
    state: Arc<RwLock<RiskState>>,
}

impl RiskGate {
    pub fn new() -> Self {
        Self {
            state: Arc::new(RwLock::new(RiskState::Active)),
        }
    }

    pub fn pause(&self) {
        if let Ok(mut guard) = self.state.write() {
            *guard = RiskState::Paused;
        }
    }

    pub fn resume(&self) {
        if let Ok(mut guard) = self.state.write() {
            *guard = RiskState::Active;
        }
    }

    pub fn status(&self) -> RiskState {
        self.state.read().map(|g| *g).unwrap_or(RiskState::Paused)
    }
}
