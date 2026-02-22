use crate::error::AppError;
use crate::models::UsageRecord;
use crate::providers::{ProviderAdapter, ProviderContext};
use async_trait::async_trait;
use chrono::{Duration, TimeZone, Utc};
use reqwest::Client;
use serde_json::Value;

pub struct AnthropicAdapter;

impl AnthropicAdapter {
    fn usage_endpoint(hours: i64) -> String {
        let end = Utc::now();
        let start = end - Duration::hours(hours);
        format!(
            "https://api.anthropic.com/v1/organizations/usage_report/messages?starting_at={}&ending_at={}",
            start.to_rfc3339(),
            end.to_rfc3339()
        )
    }

    fn parse_item_timestamp(item: &Value) -> Option<chrono::DateTime<Utc>> {
        if let Some(raw) = item.get("starting_at").and_then(Value::as_str) {
            if let Ok(parsed) = chrono::DateTime::parse_from_rfc3339(raw) {
                return Some(parsed.with_timezone(&Utc));
            }
        }
        if let Some(raw) = item.get("ending_at").and_then(Value::as_str) {
            if let Ok(parsed) = chrono::DateTime::parse_from_rfc3339(raw) {
                return Some(parsed.with_timezone(&Utc));
            }
        }
        if let Some(secs) = item.get("timestamp").and_then(Value::as_i64) {
            return Utc.timestamp_opt(secs, 0).single();
        }
        None
    }

    fn test_endpoint() -> &'static str {
        "https://api.anthropic.com/v1/models"
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
impl ProviderAdapter for AnthropicAdapter {
    fn name(&self) -> &'static str {
        "anthropic"
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
            .unwrap_or_else(|| Self::usage_endpoint(ctx.window.as_hours()));

        let body: Value = client
            .get(url)
            .header("x-api-key", &ctx.api_key)
            .header("anthropic-version", "2023-06-01")
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;

        let mut out = Vec::new();
        let items = body
            .get("data")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();

        for item in items {
            let model = item
                .get("model")
                .and_then(Value::as_str)
                .unwrap_or("unknown")
                .to_string();
            let input_tokens = item
                .get("input_tokens")
                .and_then(Value::as_u64)
                .or_else(|| item.get("tokens_in").and_then(Value::as_u64))
                .unwrap_or(0);
            let output_tokens = item
                .get("output_tokens")
                .and_then(Value::as_u64)
                .or_else(|| item.get("tokens_out").and_then(Value::as_u64))
                .unwrap_or(0);

            out.push(UsageRecord {
                provider: self.name().to_string(),
                model,
                input_tokens,
                output_tokens,
                cached_tokens: 0,
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

        let response = client
            .get(url)
            .header("x-api-key", &ctx.api_key)
            .header("anthropic-version", "2023-06-01")
            .send()
            .await?;

        let status = response.status();
        if status.is_success() {
            return Ok(Some(status.as_u16()));
        }
        if status.as_u16() == 401 || status.as_u16() == 403 {
            return Err(AppError::Config(
                "Anthropic rejected credentials (unauthorized).".into(),
            ));
        }

        Err(AppError::Config(format!(
            "Anthropic connection failed with HTTP status {}.",
            status
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parse_item_timestamp_prefers_rfc3339_fields() {
        let ts = AnthropicAdapter::parse_item_timestamp(
            &json!({ "starting_at": "2024-01-01T00:00:00Z" }),
        )
        .expect("timestamp should parse");
        assert_eq!(ts.timestamp(), 1_704_067_200);
    }

    #[test]
    fn parse_item_timestamp_supports_epoch_timestamp() {
        let ts = AnthropicAdapter::parse_item_timestamp(&json!({ "timestamp": 1_700_000_000 }))
            .expect("timestamp should parse");
        assert_eq!(ts.timestamp(), 1_700_000_000);
    }

    #[test]
    fn parse_item_timestamp_returns_none_for_invalid_payload() {
        assert!(AnthropicAdapter::parse_item_timestamp(&json!({ "starting_at": "bad" })).is_none());
        assert!(AnthropicAdapter::parse_item_timestamp(&json!({})).is_none());
    }
}
