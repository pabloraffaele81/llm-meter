use crate::config::ProviderSettings;
use crate::error::AppError;
use crate::models::{CostRecord, TimeWindow, UsageRecord};
use crate::pricing::resolve_pricing;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use reqwest::Client;

pub mod anthropic;
pub mod openai;

#[derive(Debug, Clone)]
pub struct ProviderContext {
    pub api_key: String,
    pub settings: ProviderSettings,
    pub window: TimeWindow,
    pub refresh_end: DateTime<Utc>,
}

#[async_trait]
pub trait ProviderAdapter {
    fn name(&self) -> &'static str;

    async fn fetch_usage(
        &self,
        client: &Client,
        ctx: &ProviderContext,
    ) -> Result<Vec<UsageRecord>, AppError>;

    async fn test_connection(
        &self,
        client: &Client,
        ctx: &ProviderContext,
    ) -> Result<Option<u16>, AppError> {
        self.fetch_usage(client, ctx).await.map(|_| None)
    }

    fn derive_costs(
        &self,
        usage: &[UsageRecord],
        overrides: &[crate::config::PricingOverride],
    ) -> Vec<CostRecord> {
        usage
            .iter()
            .filter_map(|u| {
                let pricing = resolve_pricing(self.name(), &u.model, overrides)?;
                let input_cost = (u.input_tokens as f64 / 1_000_000.0) * pricing.input_per_1m;
                let output_cost = (u.output_tokens as f64 / 1_000_000.0) * pricing.output_per_1m;
                Some(CostRecord {
                    provider: u.provider.clone(),
                    model: u.model.clone(),
                    input_cost,
                    output_cost,
                    total_cost: input_cost + output_cost,
                    currency: "USD".into(),
                    timestamp: u.timestamp,
                })
            })
            .collect()
    }
}
