use rusqlite::params;
use serde::{Deserialize, Serialize};

use super::db::SwitchDatabase;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageRecord {
    pub id: Option<i64>,
    pub app_type: String,
    pub provider_id: String,
    pub model_id: String,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cost_usd: f64,
    pub timestamp: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageSummary {
    pub total_requests: u64,
    pub total_input_tokens: u64,
    pub total_output_tokens: u64,
    pub total_cost_usd: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderUsage {
    pub provider_id: String,
    pub request_count: u64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cost_usd: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelUsage {
    pub model_id: String,
    pub request_count: u64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cost_usd: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DailyCost {
    pub date: String,
    pub cost_usd: f64,
    pub request_count: u64,
}

pub fn record_usage(db: &SwitchDatabase, record: &UsageRecord) -> anyhow::Result<()> {
    db.with_conn(|conn| {
        conn.execute(
            "INSERT INTO usage_logs (app_type, provider_id, model_id, input_tokens, output_tokens, cost_usd, timestamp)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                record.app_type,
                record.provider_id,
                record.model_id,
                record.input_tokens,
                record.output_tokens,
                record.cost_usd,
                record.timestamp,
            ],
        )?;
        Ok(())
    })
}

pub fn get_usage_summary(
    db: &SwitchDatabase,
    app_type: &str,
    days: u32,
) -> anyhow::Result<UsageSummary> {
    let since = chrono::Utc::now().timestamp() - (days as i64 * 86400);
    db.with_conn(|conn| {
        let mut stmt = conn.prepare(
            "SELECT COUNT(*), COALESCE(SUM(input_tokens),0), COALESCE(SUM(output_tokens),0), COALESCE(SUM(cost_usd),0)
             FROM usage_logs WHERE app_type = ?1 AND timestamp >= ?2",
        )?;
        let row = stmt.query_row(params![app_type, since], |row| {
            Ok(UsageSummary {
                total_requests: row.get::<_, u64>(0)?,
                total_input_tokens: row.get::<_, u64>(1)?,
                total_output_tokens: row.get::<_, u64>(2)?,
                total_cost_usd: row.get::<_, f64>(3)?,
            })
        })?;
        Ok(row)
    })
}

pub fn get_usage_by_provider(
    db: &SwitchDatabase,
    app_type: &str,
    days: u32,
) -> anyhow::Result<Vec<ProviderUsage>> {
    let since = chrono::Utc::now().timestamp() - (days as i64 * 86400);
    db.with_conn(|conn| {
        let mut stmt = conn.prepare(
            "SELECT provider_id, COUNT(*), COALESCE(SUM(input_tokens),0), COALESCE(SUM(output_tokens),0), COALESCE(SUM(cost_usd),0)
             FROM usage_logs WHERE app_type = ?1 AND timestamp >= ?2
             GROUP BY provider_id ORDER BY SUM(cost_usd) DESC",
        )?;
        let rows = stmt
            .query_map(params![app_type, since], |row| {
                Ok(ProviderUsage {
                    provider_id: row.get(0)?,
                    request_count: row.get(1)?,
                    input_tokens: row.get(2)?,
                    output_tokens: row.get(3)?,
                    cost_usd: row.get(4)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    })
}

pub fn get_usage_by_model(
    db: &SwitchDatabase,
    app_type: &str,
    days: u32,
) -> anyhow::Result<Vec<ModelUsage>> {
    let since = chrono::Utc::now().timestamp() - (days as i64 * 86400);
    db.with_conn(|conn| {
        let mut stmt = conn.prepare(
            "SELECT model_id, COUNT(*), COALESCE(SUM(input_tokens),0), COALESCE(SUM(output_tokens),0), COALESCE(SUM(cost_usd),0)
             FROM usage_logs WHERE app_type = ?1 AND timestamp >= ?2
             GROUP BY model_id ORDER BY SUM(cost_usd) DESC",
        )?;
        let rows = stmt
            .query_map(params![app_type, since], |row| {
                Ok(ModelUsage {
                    model_id: row.get(0)?,
                    request_count: row.get(1)?,
                    input_tokens: row.get(2)?,
                    output_tokens: row.get(3)?,
                    cost_usd: row.get(4)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    })
}

pub fn get_daily_cost(
    db: &SwitchDatabase,
    app_type: &str,
    days: u32,
) -> anyhow::Result<Vec<DailyCost>> {
    let since = chrono::Utc::now().timestamp() - (days as i64 * 86400);
    db.with_conn(|conn| {
        let mut stmt = conn.prepare(
            "SELECT date(timestamp, 'unixepoch') as d, COALESCE(SUM(cost_usd),0), COUNT(*)
             FROM usage_logs WHERE app_type = ?1 AND timestamp >= ?2
             GROUP BY d ORDER BY d ASC",
        )?;
        let rows = stmt
            .query_map(params![app_type, since], |row| {
                Ok(DailyCost {
                    date: row.get(0)?,
                    cost_usd: row.get(1)?,
                    request_count: row.get(2)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    })
}
