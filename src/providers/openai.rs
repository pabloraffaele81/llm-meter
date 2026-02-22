use crate::error::AppError;
use crate::models::{TimeWindow, UsageRecord};
use crate::providers::{ProviderAdapter, ProviderContext};
use async_trait::async_trait;
use chrono::{Duration, TimeZone, Utc};
use reqwest::Client;
use serde_json::Value;

pub struct OpenAiAdapter;

impl OpenAiAdapter {
    fn usage_endpoint(window: TimeWindow) -> String {
        let end = Utc::now();
        let start = end - Duration::hours(window.as_hours());
        format!(
            "https://api.openai.com/v1/organization/usage/completions?start_time={}&end_time={}",
            start.timestamp(),
            end.timestamp()
        )
    }

    fn parse_item_timestamp(item: &Value) -> Option<chrono::DateTime<Utc>> {
        if let Some(secs) = item.get("start_time").and_then(Value::as_i64) {
            return Utc.timestamp_opt(secs, 0).single();
        }
        if let Some(secs) = item.get("timestamp").and_then(Value::as_i64) {
            return Utc.timestamp_opt(secs, 0).single();
        }
        if let Some(raw) = item.get("start_time").and_then(Value::as_str) {
            if let Ok(parsed) = chrono::DateTime::parse_from_rfc3339(raw) {
                return Some(parsed.with_timezone(&Utc));
            }
        }
        if let Some(raw) = item.get("timestamp").and_then(Value::as_str) {
            if let Ok(parsed) = chrono::DateTime::parse_from_rfc3339(raw) {
                return Some(parsed.with_timezone(&Utc));
            }
        }
        None
    }

    fn test_endpoint() -> &'static str {
        "https://api.openai.com/v1/models"
    }

    fn resolve_test_url(base_url: Option<String>) -> String {
        let Some(base) = base_url else {
            return Self::test_endpoint().to_string();
        };

        if let Ok(mut parsed) = url::Url::parse(&base) {
            let path = parsed.path().to_string();
            if path.is_empty() || path == "/" || path == "/v1" || path == "/v1/" {
                parsed.set_path("/v1/models");
                return parsed.to_string();
            }
            if path.ends_with("/v1/models") {
                return parsed.to_string();
            }
        }
        base
    }
}

#[async_trait]
impl ProviderAdapter for OpenAiAdapter {
    fn name(&self) -> &'static str {
        "openai"
    }

    async fn fetch_usage(
        &self,
        client: &Client,
        ctx: &ProviderContext,
    ) -> Result<Vec<UsageRecord>, AppError> {
        let url = ctx
            .settings
            .base_url
            .clone()
            .unwrap_or_else(|| Self::usage_endpoint(ctx.window));

        let mut req = client.get(url).bearer_auth(&ctx.api_key);
        if let Some(org) = &ctx.settings.organization_id {
            req = req.header("OpenAI-Organization", org);
        }

        let body: Value = req.send().await?.error_for_status()?.json().await?;
        let items = body
            .get("data")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();

        let mut out = Vec::with_capacity(items.len());
        for item in items {
            let model = item
                .get("model")
                .and_then(Value::as_str)
                .unwrap_or("unknown")
                .to_string();
            let input_tokens = item
                .get("input_tokens")
                .and_then(Value::as_u64)
                .unwrap_or(0);
            let output_tokens = item
                .get("output_tokens")
                .and_then(Value::as_u64)
                .unwrap_or(0);
            let cached_tokens = item
                .get("input_cached_tokens")
                .and_then(Value::as_u64)
                .unwrap_or(0);
            out.push(UsageRecord {
                provider: self.name().to_string(),
                model,
                input_tokens,
                output_tokens,
                cached_tokens,
                timestamp: Self::parse_item_timestamp(&item).unwrap_or(ctx.refresh_end),
            });
        }

        Ok(out)
    }

    async fn test_connection(
        &self,
        client: &Client,
        ctx: &ProviderContext,
    ) -> Result<Option<u16>, AppError> {
        let url = Self::resolve_test_url(ctx.settings.base_url.clone());

        let mut req = client.get(url).bearer_auth(&ctx.api_key);
        if let Some(org) = &ctx.settings.organization_id {
            req = req.header("OpenAI-Organization", org);
        }

        let response = req.send().await?;
        let status = response.status();
        if status.is_success() {
            return Ok(Some(status.as_u16()));
        }
        if status.as_u16() == 401 || status.as_u16() == 403 {
            return Err(AppError::Config(
                "OpenAI rejected credentials (unauthorized).".into(),
            ));
        }

        Err(AppError::Config(format!(
            "OpenAI connection failed with HTTP status {}.",
            status
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parse_item_timestamp_supports_epoch_seconds() {
        let ts = OpenAiAdapter::parse_item_timestamp(&json!({ "start_time": 1_700_000_000 }))
            .expect("timestamp should parse");
        assert_eq!(ts.timestamp(), 1_700_000_000);
    }

    #[test]
    fn parse_item_timestamp_supports_rfc3339() {
        let ts =
            OpenAiAdapter::parse_item_timestamp(&json!({ "timestamp": "2024-01-01T00:00:00Z" }))
                .expect("timestamp should parse");
        assert_eq!(ts.timestamp(), 1_704_067_200);
    }

    #[test]
    fn parse_item_timestamp_returns_none_for_invalid_payload() {
        assert!(OpenAiAdapter::parse_item_timestamp(&json!({ "start_time": "nope" })).is_none());
        assert!(OpenAiAdapter::parse_item_timestamp(&json!({})).is_none());
    }
}
