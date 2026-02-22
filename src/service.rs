use crate::config::{normalize_provider_name, AppConfig, ProviderSettings};
use crate::error::AppError;
use crate::models::{Snapshot, TimeWindow};
use crate::providers::anthropic::AnthropicAdapter;
use crate::providers::openai::OpenAiAdapter;
use crate::providers::{ProviderAdapter, ProviderContext};
use crate::storage::Storage;
use chrono::{Duration, Utc};
use reqwest::Client;
use std::time::Instant;

pub struct ProviderTestReport {
    pub status_code: Option<u16>,
    pub duration_ms: u128,
}

pub struct MeterService {
    client: Client,
}

impl MeterService {
    pub fn new() -> Result<Self, AppError> {
        let client = Client::builder()
            .connect_timeout(std::time::Duration::from_secs(10))
            .timeout(std::time::Duration::from_secs(30))
            .build()?;
        Ok(Self { client })
    }

    pub async fn test_provider_connection(
        &self,
        provider: &str,
        api_key: String,
        settings: ProviderSettings,
    ) -> Result<ProviderTestReport, AppError> {
        let provider = normalize_provider_name(provider);
        let ctx = ProviderContext {
            api_key,
            settings,
            window: TimeWindow::SevenDays,
            refresh_end: Utc::now(),
        };
        let started = Instant::now();
        let status_code = match provider.as_str() {
            "openai" => OpenAiAdapter.test_connection(&self.client, &ctx).await?,
            "anthropic" => AnthropicAdapter.test_connection(&self.client, &ctx).await?,
            _ => {
                return Err(AppError::Config(format!(
                    "Unsupported provider '{provider}'."
                )));
            }
        };
        Ok(ProviderTestReport {
            status_code,
            duration_ms: started.elapsed().as_millis(),
        })
    }

    pub async fn refresh(
        &self,
        cfg: &AppConfig,
        window: TimeWindow,
        storage: &mut Storage,
    ) -> Result<Snapshot, AppError> {
        let refresh_end = Utc::now();
        let since = refresh_end - Duration::hours(window.as_hours());
        let mut usage = Vec::new();
        let mut cost = Vec::new();
        let mut refreshed_providers = Vec::new();

        let adapters: Vec<Box<dyn ProviderAdapter>> =
            vec![Box::new(OpenAiAdapter), Box::new(AnthropicAdapter)];

        for adapter in adapters {
            if !cfg
                .enabled_providers
                .iter()
                .any(|p| p.eq_ignore_ascii_case(adapter.name()))
            {
                continue;
            }

            let settings = cfg
                .provider_settings
                .get(adapter.name())
                .cloned()
                .unwrap_or_default();
            let api_key = crate::config::get_api_key(adapter.name())?;

            let ctx = ProviderContext {
                api_key,
                settings,
                window,
                refresh_end,
            };

            let rows = adapter.fetch_usage(&self.client, &ctx).await?;
            let rows_cost = adapter.derive_costs(&rows, &cfg.pricing_overrides);

            usage.extend(rows);
            cost.extend(rows_cost);
            refreshed_providers.push(adapter.name().to_string());
        }

        storage.replace_snapshot(since, &refreshed_providers, &usage, &cost)?;

        Ok(Snapshot {
            usage,
            cost,
            fetched_at: refresh_end,
        })
    }
}
