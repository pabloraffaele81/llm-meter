use crate::models::TimeWindow;
use std::collections::HashMap;
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct DashboardView {
    pub tokens: u64,
    pub cost: f64,
    pub provider_breakdown: Vec<(String, f64)>,
    pub model_breakdown: Vec<(String, f64)>,
    pub last_refresh: String,
}

impl Default for DashboardView {
    fn default() -> Self {
        Self {
            tokens: 0,
            cost: 0.0,
            provider_breakdown: vec![],
            model_breakdown: vec![],
            last_refresh: "never".into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Screen {
    Dashboard,
    ProviderManager,
    ProviderForm(ProviderFormMode),
    Confirm(ConfirmAction),
    ErrorDialog,
    InfoDialog,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProviderFormMode {
    Add,
    Edit { provider: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfirmAction {
    Quit,
    DeleteProvider { provider: String },
    DeleteKey { provider: String },
}

#[derive(Debug, Clone, Default)]
pub enum ConnectionStatus {
    #[default]
    NotTested,
    Testing,
    Success,
    Failure(String),
}

#[derive(Debug, Clone)]
pub enum LogLevel {
    Info,
    Error,
}

#[derive(Debug, Clone)]
pub struct ProviderLogEntry {
    pub ts: String,
    pub level: LogLevel,
    pub event: String,
    pub detail: String,
    pub http_status: Option<u16>,
    pub duration: Option<Duration>,
}

#[derive(Debug, Clone, Default)]
pub struct ProviderDraft {
    pub name: String,
    pub base_url: String,
    pub organization_id: String,
    pub api_key: String,
    pub enabled: bool,
    pub active_field: usize,
    pub show_advanced: bool,
    pub connection_status: ConnectionStatus,
}

#[derive(Debug, Clone)]
pub struct AppState {
    pub running: bool,
    pub window: TimeWindow,
    pub status: String,
    pub compact_mode: bool,
    pub view: DashboardView,
    pub screen: Screen,
    pub previous_screen: Screen,
    pub action_focused: bool,
    pub action_selected: usize,
    pub provider_selected: usize,
    pub confirm_selected: usize,
    pub provider_draft: ProviderDraft,
    pub provider_test_results: HashMap<String, ConnectionStatus>,
    pub provider_logs: HashMap<String, Vec<ProviderLogEntry>>,
    pub max_provider_logs: usize,
    pub log_scroll: usize,
    pub error_message: String,
    pub info_message: String,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            running: true,
            window: TimeWindow::SevenDays,
            status: "ready".into(),
            compact_mode: false,
            view: DashboardView::default(),
            screen: Screen::Dashboard,
            previous_screen: Screen::Dashboard,
            action_focused: false,
            action_selected: 0,
            provider_selected: 0,
            confirm_selected: 0,
            provider_draft: ProviderDraft::default(),
            provider_test_results: HashMap::new(),
            provider_logs: HashMap::new(),
            max_provider_logs: 100,
            log_scroll: 0,
            error_message: String::new(),
            info_message: String::new(),
        }
    }
}
