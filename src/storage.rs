use crate::error::AppError;
use crate::models::{CostRecord, UsageRecord};
use chrono::{DateTime, Utc};
use rusqlite::{params, types::Type, Connection};
use std::path::Path;

pub struct Storage {
    conn: Connection,
}

pub type AggregateSummary = (u64, f64, Vec<(String, f64)>, Vec<(String, f64)>);

impl Storage {
    pub fn open(path: &Path) -> Result<Self, AppError> {
        let conn = Connection::open(path)?;
        let this = Self { conn };
        this.init()?;
        Ok(this)
    }

    fn init(&self) -> Result<(), AppError> {
        self.conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS usage_records (
                id INTEGER PRIMARY KEY,
                provider TEXT NOT NULL,
                model TEXT NOT NULL,
                input_tokens INTEGER NOT NULL,
                output_tokens INTEGER NOT NULL,
                cached_tokens INTEGER NOT NULL,
                timestamp TEXT NOT NULL
            );

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
        )?;
        Ok(())
    }

    pub fn replace_snapshot(
        &mut self,
        since: DateTime<Utc>,
        providers: &[String],
        usage: &[UsageRecord],
        cost: &[CostRecord],
    ) -> Result<(), AppError> {
        let tx = self.conn.transaction()?;
        let since_str = since.to_rfc3339();

        if !providers.is_empty() {
            let mut delete_usage =
                tx.prepare("DELETE FROM usage_records WHERE provider = ? AND timestamp >= ?")?;
            let mut delete_cost =
                tx.prepare("DELETE FROM cost_records WHERE provider = ? AND timestamp >= ?")?;
            for provider in providers {
                delete_usage.execute(params![provider, since_str.clone()])?;
                delete_cost.execute(params![provider, since_str.clone()])?;
            }
        }

        let mut insert_usage = tx.prepare(
            "INSERT INTO usage_records (provider, model, input_tokens, output_tokens, cached_tokens, timestamp)
             VALUES (?, ?, ?, ?, ?, ?)",
        )?;
        for r in usage {
            insert_usage.execute(params![
                r.provider,
                r.model,
                r.input_tokens,
                r.output_tokens,
                r.cached_tokens,
                r.timestamp.to_rfc3339(),
            ])?;
        }

        let mut insert_cost = tx.prepare(
            "INSERT INTO cost_records (provider, model, input_cost, output_cost, total_cost, currency, timestamp)
             VALUES (?, ?, ?, ?, ?, ?, ?)",
        )?;
        for r in cost {
            insert_cost.execute(params![
                r.provider,
                r.model,
                r.input_cost,
                r.output_cost,
                r.total_cost,
                r.currency,
                r.timestamp.to_rfc3339(),
            ])?;
        }

        drop(insert_usage);
        drop(insert_cost);
        tx.commit()?;
        Ok(())
    }

    pub fn aggregate_since(&self, since: DateTime<Utc>) -> Result<AggregateSummary, AppError> {
        let since_str = since.to_rfc3339();

        let token_total_raw: i64 = self.conn.query_row(
            "SELECT COALESCE(SUM(input_tokens + output_tokens + cached_tokens), 0) FROM usage_records WHERE timestamp >= ?",
            [since_str.clone()],
            |row| row.get(0),
        )?;
        let token_total = token_total_raw.max(0) as u64;

        let cost_total: f64 = self.conn.query_row(
            "SELECT COALESCE(SUM(total_cost), 0.0) FROM cost_records WHERE timestamp >= ?",
            [since_str.clone()],
            |row| row.get(0),
        )?;

        let mut by_provider_stmt = self.conn.prepare(
            "SELECT provider, COALESCE(SUM(total_cost), 0.0) AS c
             FROM cost_records WHERE timestamp >= ?
             GROUP BY provider ORDER BY c DESC",
        )?;
        let by_provider = by_provider_stmt
            .query_map([since_str.clone()], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<Result<Vec<_>, _>>()?;

        let mut by_model_stmt = self.conn.prepare(
            "SELECT model, COALESCE(SUM(total_cost), 0.0) AS c
             FROM cost_records WHERE timestamp >= ?
             GROUP BY model ORDER BY c DESC LIMIT 10",
        )?;
        let by_model = by_model_stmt
            .query_map([since_str], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<Result<Vec<_>, _>>()?;

        Ok((token_total, cost_total, by_provider, by_model))
    }

    pub fn export_cost_json(&self) -> Result<String, AppError> {
        let mut stmt = self.conn.prepare(
            "SELECT provider, model, input_cost, output_cost, total_cost, currency, timestamp FROM cost_records ORDER BY timestamp DESC",
        )?;

        let rows = stmt
            .query_map([], |r| {
                Ok(CostRecord {
                    provider: r.get(0)?,
                    model: r.get(1)?,
                    input_cost: r.get(2)?,
                    output_cost: r.get(3)?,
                    total_cost: r.get(4)?,
                    currency: r.get(5)?,
                    timestamp: chrono::DateTime::parse_from_rfc3339(&r.get::<_, String>(6)?)
                        .map(|d| d.with_timezone(&Utc))
                        .map_err(|e| {
                            rusqlite::Error::FromSqlConversionFailure(6, Type::Text, Box::new(e))
                        })?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(serde_json::to_string_pretty(&rows)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, TimeZone};
    use tempfile::TempDir;

    fn sample_usage(provider: &str, model: &str, ts: DateTime<Utc>, tokens: u64) -> UsageRecord {
        UsageRecord {
            provider: provider.to_string(),
            model: model.to_string(),
            input_tokens: tokens,
            output_tokens: 0,
            cached_tokens: 0,
            timestamp: ts,
        }
    }

    fn sample_cost(provider: &str, model: &str, ts: DateTime<Utc>, total_cost: f64) -> CostRecord {
        CostRecord {
            provider: provider.to_string(),
            model: model.to_string(),
            input_cost: total_cost,
            output_cost: 0.0,
            total_cost,
            currency: "USD".to_string(),
            timestamp: ts,
        }
    }

    fn fixed_ts(hour: i64) -> DateTime<Utc> {
        Utc.timestamp_opt(1_700_000_000 + (hour * 3600), 0)
            .single()
            .expect("valid fixed timestamp")
    }

    #[test]
    fn replace_snapshot_replaces_rows_without_double_counting() {
        let tmp = TempDir::new().expect("tempdir");
        let db = tmp.path().join("snapshots.sqlite");
        let mut storage = Storage::open(&db).expect("open storage");
        let since = fixed_ts(0);

        storage
            .replace_snapshot(
                since,
                &["openai".to_string()],
                &[sample_usage("openai", "gpt-4o", fixed_ts(1), 100)],
                &[sample_cost("openai", "gpt-4o", fixed_ts(1), 1.0)],
            )
            .expect("first snapshot");

        storage
            .replace_snapshot(
                since,
                &["openai".to_string()],
                &[sample_usage("openai", "gpt-4o", fixed_ts(2), 250)],
                &[sample_cost("openai", "gpt-4o", fixed_ts(2), 2.5)],
            )
            .expect("second snapshot");

        let (tokens, cost, by_provider, by_model) = storage
            .aggregate_since(since - Duration::hours(1))
            .expect("aggregate");
        assert_eq!(tokens, 250);
        assert!((cost - 2.5).abs() < f64::EPSILON);
        assert_eq!(by_provider, vec![("openai".to_string(), 2.5)]);
        assert_eq!(by_model, vec![("gpt-4o".to_string(), 2.5)]);
    }

    #[test]
    fn replace_snapshot_only_affects_targeted_providers() {
        let tmp = TempDir::new().expect("tempdir");
        let db = tmp.path().join("snapshots.sqlite");
        let mut storage = Storage::open(&db).expect("open storage");
        let since = fixed_ts(0);

        storage
            .replace_snapshot(
                since,
                &["openai".to_string(), "anthropic".to_string()],
                &[
                    sample_usage("openai", "gpt-4o", fixed_ts(1), 100),
                    sample_usage("anthropic", "claude-3-5-sonnet", fixed_ts(1), 80),
                ],
                &[
                    sample_cost("openai", "gpt-4o", fixed_ts(1), 1.0),
                    sample_cost("anthropic", "claude-3-5-sonnet", fixed_ts(1), 0.8),
                ],
            )
            .expect("seed two providers");

        storage
            .replace_snapshot(
                since,
                &["openai".to_string()],
                &[sample_usage("openai", "gpt-4o", fixed_ts(2), 40)],
                &[sample_cost("openai", "gpt-4o", fixed_ts(2), 0.4)],
            )
            .expect("replace openai");

        let (tokens, cost, by_provider, _) = storage
            .aggregate_since(since - Duration::hours(1))
            .expect("aggregate");
        assert_eq!(tokens, 120);
        assert!((cost - 1.2).abs() < 1e-9);
        assert_eq!(by_provider.len(), 2);
        assert_eq!(by_provider[0], ("anthropic".to_string(), 0.8));
        assert_eq!(by_provider[1], ("openai".to_string(), 0.4));
    }

    #[test]
    fn export_cost_json_serializes_inserted_rows() {
        let tmp = TempDir::new().expect("tempdir");
        let db = tmp.path().join("snapshots.sqlite");
        let mut storage = Storage::open(&db).expect("open storage");
        let since = fixed_ts(0);

        storage
            .replace_snapshot(
                since,
                &["openai".to_string()],
                &[sample_usage("openai", "gpt-4o", fixed_ts(1), 50)],
                &[sample_cost("openai", "gpt-4o", fixed_ts(1), 0.5)],
            )
            .expect("replace snapshot");

        let json = storage.export_cost_json().expect("export json");
        let rows: Vec<CostRecord> = serde_json::from_str(&json).expect("parse exported json");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].provider, "openai");
        assert_eq!(rows[0].model, "gpt-4o");
        assert!((rows[0].total_cost - 0.5).abs() < f64::EPSILON);
    }
}
