use crate::config::PricingOverride;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelPricing {
    pub provider: String,
    pub model_pattern: String,
    pub input_per_1m: f64,
    pub output_per_1m: f64,
}

pub fn built_in_pricing() -> Vec<ModelPricing> {
    vec![
        ModelPricing {
            provider: "openai".into(),
            model_pattern: "gpt-4o".into(),
            input_per_1m: 5.0,
            output_per_1m: 15.0,
        },
        ModelPricing {
            provider: "openai".into(),
            model_pattern: "gpt-4o-mini".into(),
            input_per_1m: 0.15,
            output_per_1m: 0.60,
        },
        ModelPricing {
            provider: "anthropic".into(),
            model_pattern: "claude-3-5-sonnet".into(),
            input_per_1m: 3.0,
            output_per_1m: 15.0,
        },
        ModelPricing {
            provider: "anthropic".into(),
            model_pattern: "claude-3-5-haiku".into(),
            input_per_1m: 0.80,
            output_per_1m: 4.0,
        },
    ]
}

pub fn resolve_pricing(
    provider: &str,
    model: &str,
    overrides: &[PricingOverride],
) -> Option<ModelPricing> {
    if let Some(ov) = overrides
        .iter()
        .find(|ov| ov.provider.eq_ignore_ascii_case(provider) && model.contains(&ov.model_pattern))
    {
        return Some(ModelPricing {
            provider: provider.to_string(),
            model_pattern: ov.model_pattern.clone(),
            input_per_1m: ov.input_per_1m,
            output_per_1m: ov.output_per_1m,
        });
    }

    built_in_pricing()
        .into_iter()
        .find(|p| p.provider.eq_ignore_ascii_case(provider) && model.contains(&p.model_pattern))
}
