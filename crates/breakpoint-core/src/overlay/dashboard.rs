use serde::{Deserialize, Serialize};

/// Agent session status shown in the dashboard.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AgentStatus {
    Working,
    Waiting,
    Blocked,
    Idle,
}

/// Aggregate statistics for the dashboard view.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DashboardStats {
    pub events_last_hour: u32,
    pub events_last_minute: u32,
    pub agents_active: u32,
    pub agents_blocked: u32,
}
