use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageRecord {
    pub provider: String,
    pub model: String,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cached_tokens: u64,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CostRecord {
    pub provider: String,
    pub model: String,
    pub input_cost: f64,
    pub output_cost: f64,
    pub total_cost: f64,
    pub currency: String,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Snapshot {
    pub usage: Vec<UsageRecord>,
    pub cost: Vec<CostRecord>,
    pub fetched_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum TimeWindow {
    OneDay,
    SevenDays,
    ThirtyDays,
}

impl TimeWindow {
    pub fn as_label(self) -> &'static str {
        match self {
            TimeWindow::OneDay => "1d",
            TimeWindow::SevenDays => "7d",
            TimeWindow::ThirtyDays => "30d",
        }
    }

    pub fn as_hours(self) -> i64 {
        match self {
            TimeWindow::OneDay => 24,
            TimeWindow::SevenDays => 24 * 7,
            TimeWindow::ThirtyDays => 24 * 30,
        }
    }
}
