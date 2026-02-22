use crate::config::{
    db_path, delete_api_key, get_api_key, has_api_key, load_config, normalize_provider_name,
    save_config, set_api_key, AppConfig, ProviderSettings,
};
use crate::error::AppError;
use crate::models::TimeWindow;
use crate::service::{MeterService, ProviderTestReport};
use crate::storage::Storage;
use crate::ui::app::{
    AppState, ConfirmAction, ConnectionStatus, LogLevel, ProviderDraft, ProviderFormMode,
    ProviderLogEntry, Screen,
};
use chrono::{Duration, Utc};
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use crossterm::ExecutableCommand;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Cell, Clear, Paragraph, Row, Table, Wrap};
use ratatui::Terminal;
use std::io;
use std::time::{Duration as StdDuration, Instant};
use tokio::task::JoinHandle;
use url::Url;

const ACTIONS: [(&str, &str); 3] = [
    ("Refresh now", "r/Enter"),
    ("Manage providers/keys", "Enter"),
    ("Quit application", "q/Enter"),
];

const COLOR_ACCENT: Color = Color::Cyan;
const COLOR_INFO: Color = Color::Green;
const COLOR_MUTED: Color = Color::DarkGray;
const COLOR_HEADER: Color = Color::White;

#[derive(Debug, Clone)]
enum ProviderTestOrigin {
    Manager,
    Form { mode: ProviderFormMode },
}

struct ProviderTestJob {
    provider: String,
    origin: ProviderTestOrigin,
    started_at: Instant,
    handle: JoinHandle<Result<ProviderTestReport, AppError>>,
}

pub async fn run_tui() -> Result<(), AppError> {
    let mut cfg = load_config()?;
    let db = db_path()?;
    let mut storage = Storage::open(&db)?;
    let service = MeterService::new()?;

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    stdout.execute(EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let loop_result = run_loop(&mut terminal, &mut cfg, &mut storage, &service).await;

    disable_raw_mode()?;
    terminal.backend_mut().execute(LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    loop_result
}

async fn run_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    cfg: &mut AppConfig,
    storage: &mut Storage,
    service: &MeterService,
) -> Result<(), AppError> {
    let mut state = AppState::default();
    let mut provider_test_job: Option<ProviderTestJob> = None;
    let mut last_tick = Instant::now();
    let tick_rate = StdDuration::from_secs(cfg.refresh_seconds.max(10));

    refresh_dashboard(&mut state, cfg, storage, service).await;

    while state.running {
        if provider_test_job
            .as_ref()
            .is_some_and(|job| job.handle.is_finished())
        {
            process_provider_test_job(&mut state, &mut provider_test_job).await;
        }

        terminal.draw(|f| render(f, cfg, &state))?;

        let timeout = tick_rate
            .checked_sub(last_tick.elapsed())
            .unwrap_or_else(|| StdDuration::from_millis(0));

        if event::poll(timeout)? {
            if let Event::Key(key) = event::read()? {
                if key.kind != KeyEventKind::Press {
                    continue;
                }
                handle_key(
                    key.code,
                    key.modifiers,
                    &mut state,
                    cfg,
                    storage,
                    service,
                    &mut provider_test_job,
                )
                .await;
            }
        }

        if state.screen == Screen::Dashboard && last_tick.elapsed() >= tick_rate {
            refresh_dashboard(&mut state, cfg, storage, service).await;
            last_tick = Instant::now();
        }
    }

    Ok(())
}

async fn handle_key(
    code: KeyCode,
    modifiers: KeyModifiers,
    state: &mut AppState,
    cfg: &mut AppConfig,
    storage: &mut Storage,
    service: &MeterService,
    provider_test_job: &mut Option<ProviderTestJob>,
) {
    if modifiers.contains(KeyModifiers::CONTROL) && code == KeyCode::Char('c') {
        state.previous_screen = state.screen.clone();
        state.screen = Screen::Confirm(ConfirmAction::Quit);
        state.confirm_selected = 0;
        state.action_focused = false;
        return;
    }

    if code == KeyCode::Char('z') {
        state.compact_mode = !state.compact_mode;
        state.status = if state.compact_mode {
            "compact mode enabled".into()
        } else {
            "compact mode disabled".into()
        };
        return;
    }

    if code == KeyCode::Char('a')
        && matches!(state.screen, Screen::Dashboard | Screen::ProviderManager)
    {
        state.action_focused = true;
        return;
    }

    if code == KeyCode::Esc
        && state.action_focused
        && matches!(state.screen, Screen::Dashboard | Screen::ProviderManager)
    {
        state.action_focused = false;
        return;
    }

    if state.action_focused && matches!(state.screen, Screen::Dashboard | Screen::ProviderManager) {
        match code {
            KeyCode::Up => {
                if state.action_selected > 0 {
                    state.action_selected -= 1;
                }
            }
            KeyCode::Down => {
                if state.action_selected + 1 < ACTIONS.len() {
                    state.action_selected += 1;
                }
            }
            KeyCode::Enter => match state.action_selected {
                0 => {
                    refresh_dashboard(state, cfg, storage, service).await;
                    state.action_focused = false;
                }
                1 => {
                    state.screen = Screen::ProviderManager;
                    state.action_focused = false;
                }
                2 => {
                    state.previous_screen = state.screen.clone();
                    state.screen = Screen::Confirm(ConfirmAction::Quit);
                    state.confirm_selected = 0;
                    state.action_focused = false;
                }
                _ => {}
            },
            _ => {}
        }
        return;
    }

    match state.screen.clone() {
        Screen::Dashboard => match code {
            KeyCode::Char('q') => {
                state.previous_screen = state.screen.clone();
                state.screen = Screen::Confirm(ConfirmAction::Quit);
                state.confirm_selected = 0;
                state.action_focused = false;
            }
            KeyCode::Char('1') => state.window = TimeWindow::OneDay,
            KeyCode::Char('7') => state.window = TimeWindow::SevenDays,
            KeyCode::Char('3') => state.window = TimeWindow::ThirtyDays,
            KeyCode::Char('r') => refresh_dashboard(state, cfg, storage, service).await,
            _ => {}
        },
        Screen::ProviderManager => {
            let providers = provider_list(cfg);
            let provider_count = providers.len();
            if provider_count == 0 {
                state.provider_selected = 0;
            } else if state.provider_selected >= provider_count {
                state.provider_selected = provider_count - 1;
            }

            match code {
                KeyCode::Esc => state.screen = Screen::Dashboard,
                KeyCode::Char('q') => {
                    state.previous_screen = state.screen.clone();
                    state.screen = Screen::Confirm(ConfirmAction::Quit);
                    state.confirm_selected = 0;
                    state.action_focused = false;
                }
                KeyCode::Char('n') => {
                    state.provider_draft = ProviderDraft {
                        show_advanced: false,
                        connection_status: ConnectionStatus::NotTested,
                        ..ProviderDraft::default()
                    };
                    state.screen = Screen::ProviderForm(ProviderFormMode::Add);
                    state.action_focused = false;
                }
                KeyCode::Up => {
                    if state.provider_selected > 0 {
                        state.provider_selected -= 1;
                    }
                }
                KeyCode::Down => {
                    if state.provider_selected + 1 < provider_count {
                        state.provider_selected += 1;
                    }
                }
                KeyCode::Char('t') => {
                    if let Some(provider) = providers.get(state.provider_selected) {
                        if provider_test_job.is_some() {
                            state.status = "Another provider connection test is running.".into();
                            return;
                        }
                        match build_manager_test_target(cfg, provider) {
                            Ok((name, api_key, settings)) => {
                                state.status = format!("Testing '{name}' connection...");
                                append_provider_log(
                                    state,
                                    &name,
                                    LogLevel::Info,
                                    "test_started",
                                    "Connection test queued from Provider Manager.",
                                    None,
                                    None,
                                );
                                queue_provider_test_job(
                                    provider_test_job,
                                    name,
                                    api_key,
                                    settings,
                                    ProviderTestOrigin::Manager,
                                );
                            }
                            Err(message) => show_error(state, message),
                        }
                    }
                }
                KeyCode::Char('e') => {
                    if let Some(provider) = providers.get(state.provider_selected) {
                        let normalized = normalize_provider_name(provider);
                        if cfg
                            .enabled_providers
                            .iter()
                            .any(|p| p.eq_ignore_ascii_case(&normalized))
                        {
                            cfg.enabled_providers
                                .retain(|p| !p.eq_ignore_ascii_case(&normalized));
                            if let Err(e) = save_config(cfg) {
                                show_error(state, format!("Failed to save config: {e}"));
                            } else {
                                state.status = format!("Provider '{normalized}' disabled");
                            }
                        } else if !matches!(
                            state.provider_test_results.get(&normalized),
                            Some(ConnectionStatus::Success)
                        ) {
                            state.status = format!(
                                "Run test first for '{normalized}' (press 't'), then enable with 'e'."
                            );
                        } else {
                            match has_api_key(&normalized) {
                                Ok(true) => {
                                    cfg.enabled_providers.push(normalized.clone());
                                    if let Err(e) = save_config(cfg) {
                                        show_error(state, format!("Failed to save config: {e}"));
                                    } else {
                                        state.status = format!("Provider '{normalized}' enabled");
                                    }
                                }
                                Ok(false) => show_error(
                                    state,
                                    format!("Provider '{normalized}' has no key. Set key first."),
                                ),
                                Err(e) => {
                                    show_error(state, format!("Failed reading keychain: {e}"))
                                }
                            }
                        }
                    }
                }
                KeyCode::Enter => {
                    if let Some(provider) = providers.get(state.provider_selected) {
                        let settings = cfg
                            .provider_settings
                            .get(provider)
                            .cloned()
                            .unwrap_or_default();
                        let normalized = normalize_provider_name(provider);
                        let is_enabled = cfg
                            .enabled_providers
                            .iter()
                            .any(|p| p.eq_ignore_ascii_case(provider));
                        state.provider_draft = ProviderDraft {
                            name: provider.clone(),
                            base_url: settings.base_url.unwrap_or_default(),
                            organization_id: settings.organization_id.unwrap_or_default(),
                            api_key: String::new(),
                            enabled: is_enabled,
                            active_field: 0,
                            show_advanced: false,
                            connection_status: state
                                .provider_test_results
                                .get(&normalized)
                                .cloned()
                                .unwrap_or({
                                    if is_enabled {
                                        ConnectionStatus::Success
                                    } else {
                                        ConnectionStatus::NotTested
                                    }
                                }),
                        };
                        state.screen = Screen::ProviderForm(ProviderFormMode::Edit {
                            provider: provider.clone(),
                        });
                        state.action_focused = false;
                    }
                }
                KeyCode::Char('d') => {
                    if let Some(provider) = providers.get(state.provider_selected) {
                        state.previous_screen = Screen::ProviderManager;
                        state.screen = Screen::Confirm(ConfirmAction::DeleteProvider {
                            provider: provider.clone(),
                        });
                        state.confirm_selected = 0;
                        state.action_focused = false;
                    }
                }
                KeyCode::Char('k') => {
                    if let Some(provider) = providers.get(state.provider_selected) {
                        state.previous_screen = Screen::ProviderManager;
                        state.screen = Screen::Confirm(ConfirmAction::DeleteKey {
                            provider: provider.clone(),
                        });
                        state.confirm_selected = 0;
                        state.action_focused = false;
                    }
                }
                _ => {}
            }
        }
        Screen::ProviderForm(mode) => {
            let field_count = visible_form_fields(&mode, state.provider_draft.show_advanced).len();
            match code {
                KeyCode::Esc => state.screen = Screen::ProviderManager,
                KeyCode::Tab => {
                    state.provider_draft.active_field =
                        (state.provider_draft.active_field + 1) % field_count;
                }
                KeyCode::BackTab => {
                    if state.provider_draft.active_field == 0 {
                        state.provider_draft.active_field = field_count - 1;
                    } else {
                        state.provider_draft.active_field -= 1;
                    }
                }
                KeyCode::Char('v') => {
                    state.provider_draft.show_advanced = !state.provider_draft.show_advanced;
                    let new_count =
                        visible_form_fields(&mode, state.provider_draft.show_advanced).len();
                    if state.provider_draft.active_field >= new_count {
                        state.provider_draft.active_field = new_count.saturating_sub(1);
                    }
                }
                KeyCode::Char('t') => {
                    if provider_test_job.is_some() {
                        state.status = "Another provider connection test is running.".into();
                    } else {
                        match build_form_test_target(state, cfg, &mode) {
                            Ok((provider, api_key, settings)) => {
                                state.provider_draft.connection_status = ConnectionStatus::Testing;
                                state.status = format!("Testing '{provider}' connection...");
                                append_provider_log(
                                    state,
                                    &provider,
                                    LogLevel::Info,
                                    "test_started",
                                    "Connection test queued from Provider Form.",
                                    None,
                                    None,
                                );
                                queue_provider_test_job(
                                    provider_test_job,
                                    provider,
                                    api_key,
                                    settings,
                                    ProviderTestOrigin::Form { mode: mode.clone() },
                                );
                            }
                            Err(message) => show_error(state, message),
                        }
                    }
                }
                KeyCode::Enter => submit_provider_form(state, cfg, mode),
                KeyCode::Char('i') => {
                    if let ConnectionStatus::Failure(message) =
                        &state.provider_draft.connection_status
                    {
                        show_info(state, message.clone());
                    } else {
                        state.status = "No detailed connection error to show.".into();
                    }
                }
                KeyCode::Char('x') => {
                    if let Some(provider) = form_provider_name(state, &mode) {
                        state.provider_logs.remove(&provider);
                        state.log_scroll = 0;
                        state.status = format!("Cleared test logs for '{provider}'.");
                    } else {
                        state.status =
                            "Set provider name first to target logs, then press 'x'.".into();
                    }
                }
                KeyCode::Char('e') => {
                    if active_form_field(state, &mode) == ProviderFormField::Enabled {
                        if matches!(
                            state.provider_draft.connection_status,
                            ConnectionStatus::Success
                        ) {
                            state.provider_draft.enabled = !state.provider_draft.enabled;
                        } else {
                            state.status = "Run connection test before enabling provider.".into();
                        }
                    } else {
                        input_char(state, mode, 'e');
                    }
                }
                KeyCode::Char(' ') => {
                    if active_form_field(state, &mode) != ProviderFormField::Enabled {
                        input_char(state, mode, ' ');
                    }
                }
                KeyCode::Backspace => backspace_char(state, mode),
                KeyCode::Char(c) => input_char(state, mode, c),
                _ => {}
            }
        }
        Screen::Confirm(action) => match code {
            KeyCode::Esc => {
                state.screen = state.previous_screen.clone();
                state.action_focused = false;
            }
            KeyCode::Left => {
                if state.confirm_selected > 0 {
                    state.confirm_selected -= 1;
                }
            }
            KeyCode::Right => {
                if state.confirm_selected < 1 {
                    state.confirm_selected += 1;
                }
            }
            KeyCode::Enter => {
                if state.confirm_selected == 0 {
                    state.screen = state.previous_screen.clone();
                    state.action_focused = false;
                    return;
                }

                match action {
                    ConfirmAction::Quit => state.running = false,
                    ConfirmAction::DeleteProvider { provider } => {
                        let normalized = normalize_provider_name(&provider);
                        cfg.provider_settings.remove(&provider);
                        cfg.enabled_providers
                            .retain(|p| !p.eq_ignore_ascii_case(&provider));
                        state.provider_test_results.remove(&normalized);
                        state.provider_logs.remove(&normalized);
                        if let Err(e) = delete_api_key(&provider) {
                            show_error(state, format!("Failed to delete key: {e}"));
                            return;
                        }
                        if let Err(e) = save_config(cfg) {
                            show_error(state, format!("Failed to save config: {e}"));
                            return;
                        }
                        state.status = format!("Provider '{provider}' removed");
                        state.screen = Screen::ProviderManager;
                    }
                    ConfirmAction::DeleteKey { provider } => {
                        let normalized = normalize_provider_name(&provider);
                        if let Err(e) = delete_api_key(&provider) {
                            show_error(state, format!("Failed to delete key: {e}"));
                            return;
                        }
                        cfg.enabled_providers
                            .retain(|p| !p.eq_ignore_ascii_case(&provider));
                        state.provider_test_results.remove(&normalized);
                        state.provider_logs.remove(&normalized);
                        if let Err(e) = save_config(cfg) {
                            show_error(state, format!("Failed to save config: {e}"));
                            return;
                        }
                        state.status = format!("Key removed for '{provider}'");
                        state.screen = Screen::ProviderManager;
                    }
                }
            }
            _ => {}
        },
        Screen::ErrorDialog => {
            if matches!(code, KeyCode::Enter | KeyCode::Esc) {
                state.screen = state.previous_screen.clone();
            }
        }
        Screen::InfoDialog => {
            if matches!(code, KeyCode::Enter | KeyCode::Esc) {
                state.screen = state.previous_screen.clone();
            }
        }
    }
}

fn submit_provider_form(state: &mut AppState, cfg: &mut AppConfig, mode: ProviderFormMode) {
    let provider_name = match &mode {
        ProviderFormMode::Add => normalize_provider_name(&state.provider_draft.name),
        ProviderFormMode::Edit { provider } => normalize_provider_name(provider),
    };

    if provider_name.is_empty() {
        show_error(state, "Provider name is required".to_string());
        return;
    }

    if matches!(mode, ProviderFormMode::Add) && state.provider_draft.api_key.trim().is_empty() {
        show_error(state, "API key is required for new providers.".to_string());
        return;
    }

    if !state.provider_draft.base_url.trim().is_empty()
        && Url::parse(state.provider_draft.base_url.trim()).is_err()
    {
        show_error(state, "Base URL is not valid".to_string());
        return;
    }

    let settings = ProviderSettings {
        base_url: if state.provider_draft.base_url.trim().is_empty() {
            None
        } else {
            Some(state.provider_draft.base_url.trim().to_string())
        },
        organization_id: if state.provider_draft.organization_id.trim().is_empty() {
            None
        } else {
            Some(state.provider_draft.organization_id.trim().to_string())
        },
    };

    cfg.provider_settings
        .insert(provider_name.clone(), settings);

    if !state.provider_draft.api_key.trim().is_empty() {
        if let Err(e) = set_api_key(&provider_name, state.provider_draft.api_key.trim()) {
            show_error(state, format!("Failed to save key: {e}"));
            return;
        }
    }

    let mut blocked_enable_without_test = false;
    if state.provider_draft.enabled {
        if !matches!(
            state.provider_draft.connection_status,
            ConnectionStatus::Success
        ) {
            cfg.enabled_providers
                .retain(|p| !p.eq_ignore_ascii_case(&provider_name));
            state.provider_draft.enabled = false;
            blocked_enable_without_test = true;
        } else {
            match has_api_key(&provider_name) {
                Ok(true) => {
                    if !cfg
                        .enabled_providers
                        .iter()
                        .any(|p| p.eq_ignore_ascii_case(&provider_name))
                    {
                        cfg.enabled_providers.push(provider_name.clone());
                    }
                }
                Ok(false) => {
                    show_error(
                        state,
                        "Cannot enable provider without key. Add API key first.".to_string(),
                    );
                    return;
                }
                Err(e) => {
                    show_error(state, format!("Failed reading keychain: {e}"));
                    return;
                }
            }
        }
    } else {
        cfg.enabled_providers
            .retain(|p| !p.eq_ignore_ascii_case(&provider_name));
    }

    if let Err(e) = save_config(cfg) {
        show_error(state, format!("Failed to save config: {e}"));
        return;
    }

    state.status = if blocked_enable_without_test {
        format!("Provider '{provider_name}' saved disabled: run connection test first.")
    } else {
        match mode {
            ProviderFormMode::Add => format!("Provider '{}' added", provider_name),
            ProviderFormMode::Edit { .. } => format!("Provider '{}' updated", provider_name),
        }
    };
    if matches!(
        state.provider_draft.connection_status,
        ConnectionStatus::Success
    ) {
        state
            .provider_test_results
            .insert(provider_name.clone(), ConnectionStatus::Success);
    } else if let ConnectionStatus::Failure(message) = &state.provider_draft.connection_status {
        state.provider_test_results.insert(
            provider_name.clone(),
            ConnectionStatus::Failure(message.clone()),
        );
    }
    state.provider_draft = ProviderDraft::default();
    state.screen = Screen::ProviderManager;
}

fn build_manager_test_target(
    cfg: &AppConfig,
    provider: &str,
) -> Result<(String, String, ProviderSettings), String> {
    let provider_name = normalize_provider_name(provider);
    let api_key = get_api_key(&provider_name).map_err(|_| {
        format!("Provider '{provider_name}' has no key. Set key first before testing.")
    })?;
    let settings = cfg
        .provider_settings
        .get(&provider_name)
        .cloned()
        .unwrap_or_default();
    Ok((provider_name, api_key, settings))
}

fn build_form_test_target(
    state: &AppState,
    cfg: &AppConfig,
    mode: &ProviderFormMode,
) -> Result<(String, String, ProviderSettings), String> {
    let provider_name = match mode {
        ProviderFormMode::Add => normalize_provider_name(&state.provider_draft.name),
        ProviderFormMode::Edit { provider } => normalize_provider_name(provider),
    };
    if provider_name.is_empty() {
        return Err("Provider name is required before testing.".to_string());
    }

    let api_key = if !state.provider_draft.api_key.trim().is_empty() {
        state.provider_draft.api_key.trim().to_string()
    } else {
        get_api_key(&provider_name)
            .map_err(|_| "API key is required to run a connection test.".to_string())?
    };

    if !state.provider_draft.base_url.trim().is_empty()
        && Url::parse(state.provider_draft.base_url.trim()).is_err()
    {
        return Err("Base URL is not valid".to_string());
    }

    let existing = cfg
        .provider_settings
        .get(&provider_name)
        .cloned()
        .unwrap_or_default();
    let settings = ProviderSettings {
        base_url: if !state.provider_draft.base_url.trim().is_empty() {
            Some(state.provider_draft.base_url.trim().to_string())
        } else {
            existing.base_url
        },
        organization_id: if !state.provider_draft.organization_id.trim().is_empty() {
            Some(state.provider_draft.organization_id.trim().to_string())
        } else {
            existing.organization_id
        },
    };
    Ok((provider_name, api_key, settings))
}

fn queue_provider_test_job(
    provider_test_job: &mut Option<ProviderTestJob>,
    provider: String,
    api_key: String,
    settings: ProviderSettings,
    origin: ProviderTestOrigin,
) {
    let provider_for_task = provider.clone();
    let started_at = Instant::now();
    let handle = tokio::spawn(async move {
        let svc = MeterService::new()?;
        svc.test_provider_connection(&provider_for_task, api_key, settings)
            .await
    });
    *provider_test_job = Some(ProviderTestJob {
        provider,
        origin,
        started_at,
        handle,
    });
}

async fn process_provider_test_job(
    state: &mut AppState,
    provider_test_job: &mut Option<ProviderTestJob>,
) {
    let Some(job) = provider_test_job.take() else {
        return;
    };
    let provider = normalize_provider_name(&job.provider);
    let fallback_duration = job.started_at.elapsed();
    match job.handle.await {
        Ok(Ok(report)) => {
            let duration = StdDuration::from_millis(report.duration_ms as u64);
            append_provider_log(
                state,
                &provider,
                LogLevel::Info,
                "response_received",
                "Provider responded to connection test request.",
                report.status_code,
                Some(duration),
            );
            append_provider_log(
                state,
                &provider,
                LogLevel::Info,
                "test_succeeded",
                "Connection test completed successfully.",
                report.status_code,
                Some(duration),
            );
            state
                .provider_test_results
                .insert(provider.clone(), ConnectionStatus::Success);
            state.status = format!("Connection test succeeded for '{provider}'.");
            if let ProviderTestOrigin::Form { mode } = &job.origin {
                if form_job_matches_current(state, mode, &provider) {
                    state.provider_draft.connection_status = ConnectionStatus::Success;
                }
            }
        }
        Ok(Err(message)) => {
            let message = message.to_string();
            append_provider_log(
                state,
                &provider,
                LogLevel::Error,
                "test_failed",
                &message,
                None,
                Some(fallback_duration),
            );
            let status = ConnectionStatus::Failure(message.clone());
            state
                .provider_test_results
                .insert(provider.clone(), status.clone());
            state.status = format!("Connection test failed for '{provider}': {message}");
            if let ProviderTestOrigin::Form { mode } = &job.origin {
                if form_job_matches_current(state, mode, &provider) {
                    state.provider_draft.connection_status = status;
                    state.provider_draft.enabled = false;
                }
            }
        }
        Err(e) => {
            let message = format!("Background test task failed: {e}");
            append_provider_log(
                state,
                &provider,
                LogLevel::Error,
                "test_failed",
                &message,
                None,
                Some(fallback_duration),
            );
            let status = ConnectionStatus::Failure(message.clone());
            state
                .provider_test_results
                .insert(provider.clone(), status.clone());
            state.status = format!("Connection test failed for '{provider}': {message}");
            if let ProviderTestOrigin::Form { mode } = &job.origin {
                if form_job_matches_current(state, mode, &provider) {
                    state.provider_draft.connection_status = status;
                    state.provider_draft.enabled = false;
                }
            }
        }
    }
}

fn form_job_matches_current(state: &AppState, mode: &ProviderFormMode, provider: &str) -> bool {
    let Screen::ProviderForm(current_mode) = &state.screen else {
        return false;
    };
    if current_mode != mode {
        return false;
    }
    match mode {
        ProviderFormMode::Add => normalize_provider_name(&state.provider_draft.name) == provider,
        ProviderFormMode::Edit { .. } => true,
    }
}

fn form_provider_name(state: &AppState, mode: &ProviderFormMode) -> Option<String> {
    let provider = match mode {
        ProviderFormMode::Add => normalize_provider_name(&state.provider_draft.name),
        ProviderFormMode::Edit { provider } => normalize_provider_name(provider),
    };
    if provider.is_empty() {
        None
    } else {
        Some(provider)
    }
}

fn append_provider_log(
    state: &mut AppState,
    provider: &str,
    level: LogLevel,
    event: &str,
    detail: &str,
    http_status: Option<u16>,
    duration: Option<StdDuration>,
) {
    let key = normalize_provider_name(provider);
    if key.is_empty() {
        return;
    }

    let ts = chrono::Local::now().format("%H:%M:%S").to_string();
    let entry = ProviderLogEntry {
        ts,
        level,
        event: event.to_string(),
        detail: detail.to_string(),
        http_status,
        duration,
    };

    let logs = state.provider_logs.entry(key).or_default();
    logs.push(entry);
    if logs.len() > state.max_provider_logs {
        let trim = logs.len() - state.max_provider_logs;
        logs.drain(0..trim);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ProviderFormField {
    Name,
    ApiKey,
    BaseUrl,
    OrganizationId,
    Enabled,
}

fn visible_form_fields(mode: &ProviderFormMode, show_advanced: bool) -> Vec<ProviderFormField> {
    let mut fields = Vec::new();
    if matches!(mode, ProviderFormMode::Add) {
        fields.push(ProviderFormField::Name);
    }
    fields.push(ProviderFormField::ApiKey);
    if show_advanced {
        fields.push(ProviderFormField::BaseUrl);
        fields.push(ProviderFormField::OrganizationId);
    }
    fields.push(ProviderFormField::Enabled);
    fields
}

fn active_form_field(state: &AppState, mode: &ProviderFormMode) -> ProviderFormField {
    let fields = visible_form_fields(mode, state.provider_draft.show_advanced);
    let index = state
        .provider_draft
        .active_field
        .min(fields.len().saturating_sub(1));
    fields[index]
}

fn reset_connection_status_after_edit(state: &mut AppState) {
    if !matches!(
        state.provider_draft.connection_status,
        ConnectionStatus::NotTested
    ) {
        state.provider_draft.connection_status = ConnectionStatus::NotTested;
    }
}

fn input_char(state: &mut AppState, mode: ProviderFormMode, ch: char) {
    match active_form_field(state, &mode) {
        ProviderFormField::Name => state.provider_draft.name.push(ch),
        ProviderFormField::ApiKey => state.provider_draft.api_key.push(ch),
        ProviderFormField::BaseUrl => state.provider_draft.base_url.push(ch),
        ProviderFormField::OrganizationId => state.provider_draft.organization_id.push(ch),
        ProviderFormField::Enabled => {}
    }
    reset_connection_status_after_edit(state);
}

fn backspace_char(state: &mut AppState, mode: ProviderFormMode) {
    match active_form_field(state, &mode) {
        ProviderFormField::Name => {
            state.provider_draft.name.pop();
        }
        ProviderFormField::ApiKey => {
            state.provider_draft.api_key.pop();
        }
        ProviderFormField::BaseUrl => {
            state.provider_draft.base_url.pop();
        }
        ProviderFormField::OrganizationId => {
            state.provider_draft.organization_id.pop();
        }
        ProviderFormField::Enabled => {}
    }
    reset_connection_status_after_edit(state);
}

fn show_error(state: &mut AppState, message: String) {
    state.error_message = message;
    state.previous_screen = state.screen.clone();
    state.screen = Screen::ErrorDialog;
}

fn show_info(state: &mut AppState, message: String) {
    state.info_message = message;
    state.previous_screen = state.screen.clone();
    state.screen = Screen::InfoDialog;
}

async fn refresh_dashboard(
    state: &mut AppState,
    cfg: &AppConfig,
    storage: &mut Storage,
    service: &MeterService,
) {
    state.status = "refreshing...".into();
    match service.refresh(cfg, state.window, storage).await {
        Ok(_) => {
            let since = Utc::now() - Duration::hours(state.window.as_hours());
            if let Ok((tokens, cost, providers, models)) = storage.aggregate_since(since) {
                state.view.tokens = tokens;
                state.view.cost = cost;
                state.view.provider_breakdown = providers;
                state.view.model_breakdown = models;
            }
            state.view.last_refresh = Utc::now().to_rfc3339();
            state.status = "ok".into();
        }
        Err(err) => {
            state.status = format!("refresh failed: {err}");
        }
    }
}

fn provider_list(cfg: &AppConfig) -> Vec<String> {
    let mut providers: Vec<String> = cfg.provider_settings.keys().cloned().collect();
    for p in &cfg.enabled_providers {
        if !providers.iter().any(|x| x.eq_ignore_ascii_case(p)) {
            providers.push(p.clone());
        }
    }
    for p in ["openai", "anthropic"] {
        if !providers.iter().any(|x| x.eq_ignore_ascii_case(p)) {
            providers.push(p.to_string());
        }
    }
    providers.sort();
    providers
}

fn render(f: &mut ratatui::Frame, cfg: &AppConfig, state: &AppState) {
    let size = f.area();
    let compact = state.compact_mode || size.width < 120;

    let root = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(5),
            Constraint::Min(6),
            Constraint::Length(2),
        ])
        .split(size);

    let header = Paragraph::new(format!(
        " llm-meter  ·  {}  ·  {}  ·  {} ",
        state.window.as_label(),
        state.status,
        state.view.last_refresh
    ))
    .block(Block::default().borders(Borders::ALL).title(" Session "))
    .style(Style::default().fg(COLOR_HEADER));
    f.render_widget(header, root[0]);

    let kpis = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(root[1]);

    let cost = Paragraph::new(format!("${:.4}", state.view.cost))
        .block(Block::default().borders(Borders::ALL).title(" Cost "))
        .style(
            Style::default()
                .fg(COLOR_ACCENT)
                .add_modifier(Modifier::BOLD),
        );
    let tokens = Paragraph::new(format!("{}", state.view.tokens))
        .block(Block::default().borders(Borders::ALL).title(" Tokens "))
        .style(Style::default().fg(COLOR_INFO).add_modifier(Modifier::BOLD));

    f.render_widget(cost, kpis[0]);
    f.render_widget(tokens, kpis[1]);

    let body = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(if compact {
            [
                Constraint::Percentage(33),
                Constraint::Percentage(33),
                Constraint::Percentage(34),
            ]
        } else {
            [
                Constraint::Percentage(36),
                Constraint::Percentage(36),
                Constraint::Percentage(28),
            ]
        })
        .split(root[2]);

    let provider_rows = state
        .view
        .provider_breakdown
        .iter()
        .map(|(p, c)| {
            Row::new(vec![
                Cell::from(p.clone()),
                Cell::from(format!("${:.4}", c)),
            ])
        })
        .collect::<Vec<_>>();
    let provider_table = Table::new(
        provider_rows,
        [Constraint::Percentage(70), Constraint::Percentage(30)],
    )
    .header(
        Row::new(vec!["Provider", "Cost"]).style(
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ),
    )
    .block(Block::default().borders(Borders::ALL).title(if compact {
        " Providers "
    } else {
        " Cost By Provider "
    }));
    f.render_widget(provider_table, body[0]);

    let model_rows = state
        .view
        .model_breakdown
        .iter()
        .map(|(m, c)| {
            Row::new(vec![
                Cell::from(m.clone()),
                Cell::from(format!("${:.4}", c)),
            ])
        })
        .collect::<Vec<_>>();
    let model_table = Table::new(
        model_rows,
        [Constraint::Percentage(70), Constraint::Percentage(30)],
    )
    .header(
        Row::new(vec!["Model", "Cost"]).style(
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ),
    )
    .block(Block::default().borders(Borders::ALL).title(if compact {
        " Models "
    } else {
        " Top Models "
    }));
    f.render_widget(model_table, body[1]);

    render_action_panel(f, body[2], state, compact);

    let footer = Paragraph::new(footer_text(state))
        .block(Block::default().borders(Borders::ALL))
        .style(Style::default().fg(COLOR_MUTED));
    f.render_widget(footer, root[3]);

    match &state.screen {
        Screen::Dashboard => {}
        Screen::ProviderManager => render_provider_manager(f, cfg, state),
        Screen::ProviderForm(mode) => render_provider_form(f, state, mode),
        Screen::Confirm(action) => render_confirm(f, state, action),
        Screen::ErrorDialog => render_error(f, state),
        Screen::InfoDialog => render_info(f, state),
    }
}

fn footer_text(state: &AppState) -> &'static str {
    match state.screen {
        Screen::Dashboard => "a focus actions | r refresh | 1/7/3 window | z compact | q quit | Esc unfocus actions",
        Screen::ProviderManager => {
            "n add | Enter edit | t test | e enable/disable | k del key | d remove | a actions | z compact | Esc back"
        }
        Screen::ProviderForm(_) => {
            "Tab next | Shift+Tab prev | t test | x clear logs | e toggle enabled | v advanced | i details | Enter save | Esc cancel"
        }
        Screen::Confirm(_) => "Left/Right choose | Enter confirm | Esc cancel",
        Screen::ErrorDialog => "Enter/Esc close",
        Screen::InfoDialog => "Enter/Esc close",
    }
}

fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}

fn render_action_panel(f: &mut ratatui::Frame, area: Rect, state: &AppState, compact: bool) {
    let mut lines = Vec::new();
    for (idx, (label, hint)) in ACTIONS.iter().enumerate() {
        let selected = idx == state.action_selected;
        let focused = state.action_focused
            && matches!(state.screen, Screen::Dashboard | Screen::ProviderManager);
        let marker = if selected { ">" } else { " " };
        let style = if selected && focused {
            Style::default()
                .fg(Color::Black)
                .bg(COLOR_ACCENT)
                .add_modifier(Modifier::BOLD)
        } else if selected {
            Style::default()
                .fg(COLOR_ACCENT)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(COLOR_HEADER)
        };

        lines.push(Line::from(vec![
            Span::styled(format!("{marker} {label}"), style),
            Span::styled(
                if compact {
                    format!(" [{hint}]")
                } else {
                    format!("  [{hint}]")
                },
                Style::default().fg(COLOR_MUTED),
            ),
        ]));
    }

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        if state.action_focused {
            if compact {
                "Focused: Up/Down, Enter, Esc"
            } else {
                "Focused: Up/Down + Enter · Esc to return"
            }
        } else if compact {
            "Press 'a' to focus"
        } else {
            "Press 'a' to focus actions"
        },
        Style::default().fg(COLOR_MUTED),
    )));

    let panel =
        Paragraph::new(lines).block(Block::default().borders(Borders::ALL).title(" Actions "));
    f.render_widget(panel, area);
}

fn render_provider_manager(f: &mut ratatui::Frame, cfg: &AppConfig, state: &AppState) {
    let area = centered_rect(90, 80, f.area());
    f.render_widget(Clear, area);

    let providers = provider_list(cfg);
    let mut rows = Vec::new();

    for (idx, provider) in providers.iter().enumerate() {
        let enabled = cfg
            .enabled_providers
            .iter()
            .any(|p| p.eq_ignore_ascii_case(provider));
        let key_status = match has_api_key(provider) {
            Ok(true) => "present",
            Ok(false) => "missing",
            Err(_) => "error",
        };

        let style = if idx == state.provider_selected {
            Style::default()
                .bg(Color::DarkGray)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };

        rows.push(
            Row::new(vec![
                Cell::from(provider.clone()),
                Cell::from(if enabled { "enabled" } else { "disabled" }),
                Cell::from(key_status),
            ])
            .style(style),
        );
    }

    let table = Table::new(
        rows,
        [
            Constraint::Percentage(40),
            Constraint::Percentage(30),
            Constraint::Percentage(30),
        ],
    )
    .header(
        Row::new(vec!["Provider", "State", "Key"]).style(
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ),
    )
    .block(
        Block::default()
            .borders(Borders::ALL)
            .title(" Provider Manager "),
    );

    f.render_widget(table, area);
}

fn render_provider_form(f: &mut ratatui::Frame, state: &AppState, mode: &ProviderFormMode) {
    let area = centered_rect(80, 70, f.area());
    f.render_widget(Clear, area);
    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(12), Constraint::Min(8)])
        .split(area);

    let title = match mode {
        ProviderFormMode::Add => "Add Provider",
        ProviderFormMode::Edit { .. } => "Edit Provider",
    };

    let mut lines = Vec::new();
    let active_field = active_form_field(state, mode);

    match mode {
        ProviderFormMode::Add => {
            lines.push(form_line(
                "Name",
                &state.provider_draft.name,
                active_field == ProviderFormField::Name,
                false,
            ));
            lines.push(form_line(
                "API Key",
                &state.provider_draft.api_key,
                active_field == ProviderFormField::ApiKey,
                true,
            ));
            lines.push(Line::from(format!(
                "Advanced: {} (press 'v' to toggle)",
                if state.provider_draft.show_advanced {
                    "visible"
                } else {
                    "hidden"
                }
            )));
            if state.provider_draft.show_advanced {
                lines.push(form_line(
                    "Base URL (optional)",
                    &state.provider_draft.base_url,
                    active_field == ProviderFormField::BaseUrl,
                    false,
                ));
                lines.push(form_line(
                    "Organization ID (optional)",
                    &state.provider_draft.organization_id,
                    active_field == ProviderFormField::OrganizationId,
                    false,
                ));
            }
            lines.push(form_line(
                "Enabled",
                if state.provider_draft.enabled {
                    "yes"
                } else {
                    "no"
                },
                active_field == ProviderFormField::Enabled,
                false,
            ));
        }
        ProviderFormMode::Edit { provider } => {
            lines.push(Line::from(format!("Provider: {provider}")));
            lines.push(form_line(
                "New API Key (optional)",
                &state.provider_draft.api_key,
                active_field == ProviderFormField::ApiKey,
                true,
            ));
            lines.push(Line::from(format!(
                "Advanced: {} (press 'v' to toggle)",
                if state.provider_draft.show_advanced {
                    "visible"
                } else {
                    "hidden"
                }
            )));
            if state.provider_draft.show_advanced {
                lines.push(form_line(
                    "Base URL (optional)",
                    &state.provider_draft.base_url,
                    active_field == ProviderFormField::BaseUrl,
                    false,
                ));
                lines.push(form_line(
                    "Organization ID (optional)",
                    &state.provider_draft.organization_id,
                    active_field == ProviderFormField::OrganizationId,
                    false,
                ));
            }
            lines.push(form_line(
                "Enabled",
                if state.provider_draft.enabled {
                    "yes"
                } else {
                    "no"
                },
                active_field == ProviderFormField::Enabled,
                false,
            ));
        }
    }

    lines.push(Line::from(format!(
        "Connection: {}",
        connection_status_label(&state.provider_draft.connection_status)
    )));
    lines.push(Line::from(""));
    lines.push(Line::from(
        "Tab/Shift+Tab switch field | t test | x clear logs | e toggle enabled | v advanced | i details | Enter save | Esc cancel",
    ));

    let content = Paragraph::new(lines)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(format!(" {} ", title)),
        )
        .style(Style::default().fg(COLOR_HEADER));
    f.render_widget(content, sections[0]);

    let provider_logs = form_provider_name(state, mode)
        .and_then(|provider| state.provider_logs.get(&provider).cloned())
        .unwrap_or_default();
    let visible_lines = sections[1].height.saturating_sub(2) as usize;
    let visible_lines = visible_lines.max(1);
    let start = provider_logs.len().saturating_sub(visible_lines);

    let mut log_lines: Vec<Line<'static>> = provider_logs[start..]
        .iter()
        .map(format_provider_log_line)
        .collect();
    if log_lines.is_empty() {
        log_lines.push(Line::from(
            "No test logs yet. Press 't' to run a connection test.",
        ));
    }

    let log_panel = Paragraph::new(log_lines)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Test Logs (Edit Provider) "),
        )
        .style(Style::default().fg(COLOR_HEADER))
        .wrap(Wrap { trim: true });
    f.render_widget(log_panel, sections[1]);
}

fn format_provider_log_line(entry: &ProviderLogEntry) -> Line<'static> {
    let level = match entry.level {
        LogLevel::Info => "INFO",
        LogLevel::Error => "ERROR",
    };
    let mut suffix = String::new();
    if let Some(status) = entry.http_status {
        suffix.push_str(&format!(" status={status}"));
    }
    if let Some(duration) = entry.duration {
        suffix.push_str(&format!(" dur={}ms", duration.as_millis()));
    }

    Line::from(format!(
        "[{}] {} {} - {}{}",
        entry.ts, level, entry.event, entry.detail, suffix
    ))
}

fn connection_status_label(status: &ConnectionStatus) -> String {
    match status {
        ConnectionStatus::NotTested => "not tested".to_string(),
        ConnectionStatus::Testing => "testing...".to_string(),
        ConnectionStatus::Success => "ok".to_string(),
        ConnectionStatus::Failure(_) => "failed (press 'i' for details)".to_string(),
    }
}

fn form_line(label: &str, value: &str, active: bool, masked: bool) -> Line<'static> {
    let display = if masked {
        if value.is_empty() {
            "".to_string()
        } else {
            "*".repeat(value.chars().count())
        }
    } else {
        value.to_string()
    };

    let prefix = if active { "> " } else { "  " };
    let style = if active {
        Style::default()
            .fg(COLOR_ACCENT)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default()
    };

    Line::from(vec![
        Span::styled(prefix.to_string(), style),
        Span::styled(format!("{label}: {display}"), style),
    ])
}

fn render_confirm(f: &mut ratatui::Frame, state: &AppState, action: &ConfirmAction) {
    let area = centered_rect(56, 34, f.area());
    f.render_widget(Clear, area);

    let (title, message, target, consequence): (&str, &str, String, String) = match action {
        ConfirmAction::Quit => (
            "Confirm Quit",
            "Do you want to exit llm-meter?",
            "Target: application session".to_string(),
            "Consequence: closes TUI and returns to shell.".to_string(),
        ),
        ConfirmAction::DeleteProvider { provider } => (
            "Confirm Provider Removal",
            "Remove this provider configuration?",
            format!("Provider: {provider}"),
            "Consequence: removes provider config and stored API key.".to_string(),
        ),
        ConfirmAction::DeleteKey { provider } => (
            "Confirm Key Deletion",
            "Delete the stored key for this provider?",
            format!("Provider: {provider}"),
            "Consequence: provider key is deleted and provider is disabled.".to_string(),
        ),
    };

    let cancel_style = if state.confirm_selected == 0 {
        Style::default()
            .fg(Color::Black)
            .bg(Color::Yellow)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default()
    };
    let confirm_style = if state.confirm_selected == 1 {
        Style::default()
            .fg(Color::Black)
            .bg(Color::Red)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default()
    };

    let content = Paragraph::new(vec![
        Line::from(message),
        Line::from(target),
        Line::from(Span::styled(consequence, Style::default().fg(COLOR_MUTED))),
        Line::from(""),
        Line::from(vec![
            Span::styled("[Cancel (Esc)]", cancel_style),
            Span::raw("   "),
            Span::styled("[Confirm (Enter)]", confirm_style),
        ]),
        Line::from("Use Left/Right to choose"),
    ])
    .block(
        Block::default()
            .borders(Borders::ALL)
            .title(format!(" {} ", title)),
    )
    .alignment(Alignment::Center);

    f.render_widget(content, area);
}

fn render_error(f: &mut ratatui::Frame, state: &AppState) {
    let area = centered_rect(60, 30, f.area());
    f.render_widget(Clear, area);
    let content = Paragraph::new(vec![
        Line::from(state.error_message.clone()),
        Line::from(""),
        Line::from("Press Enter or Esc"),
    ])
    .block(Block::default().borders(Borders::ALL).title(" Error "))
    .style(Style::default().fg(Color::Red));
    f.render_widget(content, area);
}

fn render_info(f: &mut ratatui::Frame, state: &AppState) {
    let area = centered_rect(70, 38, f.area());
    f.render_widget(Clear, area);
    let content = Paragraph::new(vec![
        Line::from(state.info_message.clone()),
        Line::from(""),
        Line::from("Press Enter or Esc"),
    ])
    .block(Block::default().borders(Borders::ALL).title(" Details "))
    .style(Style::default().fg(Color::Yellow));
    f.render_widget(content, area);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration as StdDuration;

    #[test]
    fn visible_form_fields_for_add_defaults_to_minimal_inputs() {
        let fields = visible_form_fields(&ProviderFormMode::Add, false);
        assert_eq!(
            fields,
            vec![
                ProviderFormField::Name,
                ProviderFormField::ApiKey,
                ProviderFormField::Enabled,
            ]
        );
    }

    #[test]
    fn visible_form_fields_include_advanced_when_enabled() {
        let fields = visible_form_fields(
            &ProviderFormMode::Edit {
                provider: "openai".into(),
            },
            true,
        );
        assert_eq!(
            fields,
            vec![
                ProviderFormField::ApiKey,
                ProviderFormField::BaseUrl,
                ProviderFormField::OrganizationId,
                ProviderFormField::Enabled,
            ]
        );
    }

    #[test]
    fn connection_status_label_hides_full_error_text() {
        let label = connection_status_label(&ConnectionStatus::Failure(
            "very long internal error".into(),
        ));
        assert_eq!(label, "failed (press 'i' for details)");
    }

    #[test]
    fn provider_logs_are_truncated_to_max_size() {
        let mut state = AppState {
            max_provider_logs: 2,
            ..AppState::default()
        };
        append_provider_log(
            &mut state,
            "openai",
            LogLevel::Info,
            "first",
            "first detail",
            None,
            None,
        );
        append_provider_log(
            &mut state,
            "openai",
            LogLevel::Info,
            "second",
            "second detail",
            None,
            None,
        );
        append_provider_log(
            &mut state,
            "openai",
            LogLevel::Info,
            "third",
            "third detail",
            Some(200),
            Some(StdDuration::from_millis(20)),
        );

        let logs = state.provider_logs.get("openai").expect("logs for openai");
        assert_eq!(logs.len(), 2);
        assert_eq!(logs[0].event, "second");
        assert_eq!(logs[1].event, "third");
    }

    #[test]
    fn provider_logs_are_isolated_per_provider() {
        let mut state = AppState::default();
        append_provider_log(
            &mut state,
            "openai",
            LogLevel::Info,
            "openai_event",
            "ok",
            Some(200),
            None,
        );
        append_provider_log(
            &mut state,
            "anthropic",
            LogLevel::Error,
            "anthropic_event",
            "failed",
            None,
            Some(StdDuration::from_millis(11)),
        );

        let openai_logs = state.provider_logs.get("openai").expect("openai logs");
        let anthropic_logs = state
            .provider_logs
            .get("anthropic")
            .expect("anthropic logs");
        assert_eq!(openai_logs.len(), 1);
        assert_eq!(anthropic_logs.len(), 1);
        assert_eq!(openai_logs[0].event, "openai_event");
        assert_eq!(anthropic_logs[0].event, "anthropic_event");
    }
}
