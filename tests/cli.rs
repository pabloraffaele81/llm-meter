use rusqlite::Connection;
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use tempfile::TempDir;

fn home_path(home: &TempDir) -> &Path {
    home.path()
}

fn bin_path() -> &'static str {
    env!("CARGO_BIN_EXE_llm-meter")
}

fn run_cmd(home: &TempDir, args: &[&str]) -> Output {
    Command::new(bin_path())
        .args(args)
        .env("LLM_METER_HOME", home_path(home))
        .output()
        .expect("run llm-meter command")
}

fn db_path(home: &TempDir) -> PathBuf {
    home.path().join("data").join("snapshots.sqlite")
}

fn seed_cost_row(home: &TempDir, provider: &str, model: &str, total: f64) {
    let db = db_path(home);
    let conn = Connection::open(db).expect("open sqlite");
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS cost_records (
            id INTEGER PRIMARY KEY,
            provider TEXT NOT NULL,
            model TEXT NOT NULL,
            input_cost REAL NOT NULL,
            output_cost REAL NOT NULL,
            total_cost REAL NOT NULL,
            currency TEXT NOT NULL,
            timestamp TEXT NOT NULL
        );
        "#,
    )
    .expect("create cost table");

    conn.execute(
        "INSERT INTO cost_records (provider, model, input_cost, output_cost, total_cost, currency, timestamp)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        rusqlite::params![provider, model, total, 0.0_f64, total, "USD", "2024-01-01T00:00:00Z"],
    )
    .expect("insert cost row");
}

#[test]
fn init_creates_config_and_data_paths() {
    let home = TempDir::new().expect("temp home");
    let output = run_cmd(&home, &["init"]);
    assert!(output.status.success());

    assert!(home.path().join("config").exists());
    assert!(home.path().join("data").exists());
    assert!(home.path().join("config").join("config.toml").exists());
}

#[test]
fn refresh_rejects_invalid_window() {
    let home = TempDir::new().expect("temp home");
    assert!(run_cmd(&home, &["init"]).status.success());

    let output = run_cmd(&home, &["refresh", "--window", "2d"]);
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("Unsupported window. Use 1d, 7d, or 30d"));
}

#[test]
fn export_csv_outputs_header_and_escaped_fields() {
    let home = TempDir::new().expect("temp home");
    assert!(run_cmd(&home, &["init"]).status.success());
    seed_cost_row(&home, "open,ai", "gpt\"4o", 1.25);

    let output = run_cmd(&home, &["export", "--format", "csv"]);
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("provider,model,input_cost,output_cost,total_cost,currency,timestamp"));
    assert!(stdout.contains("\"open,ai\",\"gpt\"\"4o\""));
}

#[test]
fn export_json_outputs_valid_array() {
    let home = TempDir::new().expect("temp home");
    assert!(run_cmd(&home, &["init"]).status.success());
    seed_cost_row(&home, "openai", "gpt-4o", 2.5);

    let output = run_cmd(&home, &["export", "--format", "json"]);
    assert!(output.status.success());

    let parsed: Value = serde_json::from_slice(&output.stdout).expect("valid json output");
    let arr = parsed.as_array().expect("json array");
    assert_eq!(arr.len(), 1);
    assert_eq!(arr[0]["provider"], "openai");
    assert_eq!(arr[0]["model"], "gpt-4o");
}

#[test]
fn init_is_idempotent() {
    let home = TempDir::new().expect("temp home");

    assert!(run_cmd(&home, &["init"]).status.success());
    let first = fs::read_to_string(home.path().join("config").join("config.toml"))
        .expect("read config after first init");

    assert!(run_cmd(&home, &["init"]).status.success());
    let second = fs::read_to_string(home.path().join("config").join("config.toml"))
        .expect("read config after second init");

    assert_eq!(first, second);
}
