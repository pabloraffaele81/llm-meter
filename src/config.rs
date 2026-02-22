use crate::error::AppError;
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

pub const SERVICE_NAME: &str = "llm-meter";

pub fn normalize_provider_name(provider: &str) -> String {
    provider.trim().to_ascii_lowercase()
}

fn app_home_dir() -> Result<PathBuf, AppError> {
    if let Ok(custom) = std::env::var("LLM_METER_HOME") {
        return Ok(PathBuf::from(custom));
    }

    if let Some(dirs) = ProjectDirs::from("com", "neubell", SERVICE_NAME) {
        let candidate = dirs.data_local_dir().to_path_buf();
        if fs::create_dir_all(&candidate).is_ok() {
            return Ok(candidate);
        }
    }

    let cwd = std::env::current_dir()?;
    Ok(cwd.join(".llm-meter"))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub refresh_seconds: u64,
    pub enabled_providers: Vec<String>,
    pub provider_settings: HashMap<String, ProviderSettings>,
    pub pricing_overrides: Vec<PricingOverride>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProviderSettings {
    pub base_url: Option<String>,
    pub organization_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PricingOverride {
    pub provider: String,
    pub model_pattern: String,
    pub input_per_1m: f64,
    pub output_per_1m: f64,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            refresh_seconds: 60,
            enabled_providers: vec![],
            provider_settings: HashMap::new(),
            pricing_overrides: vec![],
        }
    }
}

pub fn config_dir() -> Result<PathBuf, AppError> {
    Ok(app_home_dir()?.join("config"))
}

pub fn data_dir() -> Result<PathBuf, AppError> {
    Ok(app_home_dir()?.join("data"))
}

pub fn config_path() -> Result<PathBuf, AppError> {
    Ok(config_dir()?.join("config.toml"))
}

pub fn db_path() -> Result<PathBuf, AppError> {
    Ok(data_dir()?.join("snapshots.sqlite"))
}

pub fn ensure_dirs() -> Result<(), AppError> {
    fs::create_dir_all(config_dir()?)?;
    fs::create_dir_all(data_dir()?)?;
    Ok(())
}

fn migrate_legacy_api_keys(raw: &mut toml::Value) -> Result<(), AppError> {
    let Some(provider_settings) = raw
        .get_mut("provider_settings")
        .and_then(toml::Value::as_table_mut)
    else {
        return Ok(());
    };

    for (provider, settings) in provider_settings.iter_mut() {
        let Some(settings_table) = settings.as_table_mut() else {
            continue;
        };

        let api_key = settings_table
            .get("api_key")
            .and_then(toml::Value::as_str)
            .map(ToString::to_string);

        if let Some(key) = api_key {
            if !key.is_empty() {
                set_api_key(provider, &key)?;
            }
            settings_table.remove("api_key");
        }
    }

    Ok(())
}

fn normalize_config(config: &mut AppConfig) -> bool {
    let mut changed = false;

    let mut enabled = Vec::new();
    for provider in &config.enabled_providers {
        let normalized = normalize_provider_name(provider);
        if normalized != *provider {
            changed = true;
        }
        if !enabled.iter().any(|p: &String| p == &normalized) {
            enabled.push(normalized);
        } else {
            changed = true;
        }
    }
    config.enabled_providers = enabled;

    let mut normalized_settings: HashMap<String, ProviderSettings> = HashMap::new();
    for (provider, settings) in std::mem::take(&mut config.provider_settings) {
        let normalized = normalize_provider_name(&provider);
        if normalized != provider {
            changed = true;
        }
        normalized_settings.insert(normalized, settings);
    }
    config.provider_settings = normalized_settings;

    for override_row in &mut config.pricing_overrides {
        let normalized = normalize_provider_name(&override_row.provider);
        if normalized != override_row.provider {
            override_row.provider = normalized;
            changed = true;
        }
    }

    changed
}

pub fn load_config() -> Result<AppConfig, AppError> {
    let path = config_path()?;
    if !path.exists() {
        return Ok(AppConfig::default());
    }

    let raw_str = fs::read_to_string(&path)?;
    let mut raw_toml: toml::Value = toml::from_str(&raw_str)?;
    migrate_legacy_api_keys(&mut raw_toml)?;

    let mut parsed: AppConfig = raw_toml.clone().try_into()?;
    let normalized = normalize_config(&mut parsed);

    // Persist migrated config if legacy fields were removed.
    let rewritten = toml::to_string_pretty(&raw_toml)?;
    if rewritten != raw_str || normalized {
        if normalized {
            save_config(&parsed)?;
        } else {
            fs::write(path, rewritten)?;
        }
    }

    Ok(parsed)
}

pub fn save_config(config: &AppConfig) -> Result<(), AppError> {
    ensure_dirs()?;
    let path = config_path()?;
    let raw = toml::to_string_pretty(config)?;
    fs::write(path, raw)?;
    Ok(())
}

pub fn set_api_key(provider: &str, key: &str) -> Result<(), AppError> {
    let normalized = normalize_provider_name(provider);
    let entry = keyring::Entry::new(SERVICE_NAME, &format!("provider:{normalized}"))?;
    entry.set_password(key)?;
    Ok(())
}

pub fn delete_api_key(provider: &str) -> Result<(), AppError> {
    let normalized = normalize_provider_name(provider);
    let entry = keyring::Entry::new(SERVICE_NAME, &format!("provider:{normalized}"))?;
    match entry.delete_credential() {
        Ok(()) => Ok(()),
        Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(AppError::Keyring(e)),
    }
}

pub fn has_api_key(provider: &str) -> Result<bool, AppError> {
    let normalized = normalize_provider_name(provider);
    let entry = keyring::Entry::new(SERVICE_NAME, &format!("provider:{normalized}"))?;
    match entry.get_password() {
        Ok(v) => Ok(!v.is_empty()),
        Err(keyring::Error::NoEntry) => Ok(false),
        Err(e) => Err(AppError::Keyring(e)),
    }
}

pub fn get_api_key(provider: &str) -> Result<String, AppError> {
    let normalized = normalize_provider_name(provider);
    let entry = keyring::Entry::new(SERVICE_NAME, &format!("provider:{normalized}"))?;
    if let Ok(value) = entry.get_password() {
        if !value.is_empty() {
            return Ok(value);
        }
    }

    let env_name = format!(
        "{}_API_KEY",
        normalized.to_ascii_uppercase().replace('-', "_")
    );
    if let Ok(value) = std::env::var(env_name) {
        if !value.is_empty() {
            return Ok(value);
        }
    }

    Err(AppError::Config(format!(
        "No API key found for provider '{normalized}'. Configure in TUI key manager or set env var."
    )))
}

pub fn ensure_initialized() -> Result<(), AppError> {
    ensure_dirs()?;
    let cfg_path = config_path()?;
    if !Path::new(&cfg_path).exists() {
        save_config(&AppConfig::default())?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_provider_name_trims_and_lowercases() {
        assert_eq!(normalize_provider_name(" OpenAI "), "openai");
        assert_eq!(normalize_provider_name("AnThRoPiC"), "anthropic");
    }

    #[test]
    fn normalize_config_dedupes_and_normalizes_keys() {
        let mut cfg = AppConfig {
            refresh_seconds: 60,
            enabled_providers: vec![" OpenAI ".into(), "openai".into(), "ANTHROPIC".into()],
            provider_settings: HashMap::from([
                (
                    " OpenAI ".into(),
                    ProviderSettings {
                        base_url: Some("https://example.com".into()),
                        organization_id: None,
                    },
                ),
                (
                    "ANTHROPIC".into(),
                    ProviderSettings {
                        base_url: None,
                        organization_id: Some("org_1".into()),
                    },
                ),
            ]),
            pricing_overrides: vec![PricingOverride {
                provider: "OpenAI".into(),
                model_pattern: "gpt-4o".into(),
                input_per_1m: 1.0,
                output_per_1m: 2.0,
            }],
        };

        let changed = normalize_config(&mut cfg);
        assert!(changed);
        assert_eq!(
            cfg.enabled_providers,
            vec!["openai".to_string(), "anthropic".to_string()]
        );
        assert!(cfg.provider_settings.contains_key("openai"));
        assert!(cfg.provider_settings.contains_key("anthropic"));
        assert_eq!(cfg.pricing_overrides[0].provider, "openai");
    }
}
