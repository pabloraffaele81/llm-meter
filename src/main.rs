mod config;
mod error;
mod models;
mod pricing;
mod providers;
mod service;
mod storage;
mod ui;

use clap::{Parser, Subcommand};
use config::{
    db_path, ensure_initialized, load_config, normalize_provider_name, save_config, set_api_key,
};
use error::AppError;
use models::TimeWindow;
use service::MeterService;
use storage::Storage;
use ui::run::run_tui;

#[derive(Debug, Parser)]
#[command(name = "llm-meter")]
#[command(about = "Online LLM token and cost monitor")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    Init,
    AddProvider {
        provider: String,
        #[arg(long)]
        api_key: String,
        #[arg(long)]
        base_url: Option<String>,
        #[arg(long)]
        organization_id: Option<String>,
    },
    Tui,
    Refresh {
        #[arg(long, default_value = "7d")]
        window: String,
    },
    Export {
        #[arg(long, default_value = "json")]
        format: String,
    },
}

fn parse_window(input: &str) -> TimeWindow {
    match input {
        "1d" => TimeWindow::OneDay,
        "7d" => TimeWindow::SevenDays,
        "30d" => TimeWindow::ThirtyDays,
        _ => TimeWindow::SevenDays,
    }
}

fn validate_window(input: &str) -> Result<TimeWindow, AppError> {
    match input {
        "1d" | "7d" | "30d" => Ok(parse_window(input)),
        _ => Err(AppError::Config(
            "Unsupported window. Use 1d, 7d, or 30d.".into(),
        )),
    }
}

fn csv_field(raw: &str) -> String {
    if raw.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", raw.replace('"', "\"\""))
    } else {
        raw.to_string()
    }
}

#[tokio::main]
async fn main() -> Result<(), AppError> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Init => {
            ensure_initialized()?;
            println!("Initialized llm-meter config and data directories.");
        }
        Commands::AddProvider {
            provider,
            api_key,
            base_url,
            organization_id,
        } => {
            ensure_initialized()?;
            let mut cfg = load_config()?;
            let provider = normalize_provider_name(&provider);

            if !cfg
                .enabled_providers
                .iter()
                .any(|p| p.eq_ignore_ascii_case(&provider))
            {
                cfg.enabled_providers.push(provider.clone());
            }

            cfg.provider_settings.insert(
                provider.clone(),
                config::ProviderSettings {
                    base_url,
                    organization_id,
                },
            );

            set_api_key(&provider, &api_key)?;
            save_config(&cfg)?;
            println!("Provider '{}' configured.", provider);
        }
        Commands::Tui => {
            ensure_initialized()?;
            run_tui().await?;
        }
        Commands::Refresh { window } => {
            ensure_initialized()?;
            let cfg = load_config()?;
            let db = db_path()?;
            let mut storage = Storage::open(&db)?;
            let svc = MeterService::new()?;
            let snap = svc
                .refresh(&cfg, validate_window(&window)?, &mut storage)
                .await?;
            println!(
                "Fetched {} usage records and {} cost rows at {}",
                snap.usage.len(),
                snap.cost.len(),
                snap.fetched_at
            );
        }
        Commands::Export { format } => {
            ensure_initialized()?;
            let db = db_path()?;
            let storage = Storage::open(&db)?;
            if format.eq_ignore_ascii_case("json") {
                println!("{}", storage.export_cost_json()?);
            } else if format.eq_ignore_ascii_case("csv") {
                let json = storage.export_cost_json()?;
                let rows: Vec<models::CostRecord> = serde_json::from_str(&json)?;
                println!("provider,model,input_cost,output_cost,total_cost,currency,timestamp");
                for r in rows {
                    println!(
                        "{},{},{:.8},{:.8},{:.8},{},{}",
                        csv_field(&r.provider),
                        csv_field(&r.model),
                        r.input_cost,
                        r.output_cost,
                        r.total_cost,
                        csv_field(&r.currency),
                        csv_field(&r.timestamp.to_rfc3339()),
                    );
                }
            } else {
                return Err(AppError::Config(
                    "Unsupported export format. Use json or csv".into(),
                ));
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_window_accepts_known_values() {
        assert_eq!(parse_window("1d"), TimeWindow::OneDay);
        assert_eq!(parse_window("7d"), TimeWindow::SevenDays);
        assert_eq!(parse_window("30d"), TimeWindow::ThirtyDays);
    }

    #[test]
    fn parse_window_defaults_to_seven_days_for_unknown() {
        assert_eq!(parse_window("weird"), TimeWindow::SevenDays);
    }

    #[test]
    fn validate_window_rejects_unknown_values() {
        let err = validate_window("2d").expect_err("expected validation error");
        assert!(err.to_string().contains("Unsupported window"));
    }

    #[test]
    fn csv_field_escapes_special_characters() {
        assert_eq!(csv_field("plain"), "plain");
        assert_eq!(csv_field("a,b"), "\"a,b\"");
        assert_eq!(csv_field("a\"b"), "\"a\"\"b\"");
        assert_eq!(csv_field("a\nb"), "\"a\nb\"");
    }
}
