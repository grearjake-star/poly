use anyhow::Result;
use chrono::Utc;
use sqlx::sqlite::SqlitePoolOptions;
use sqlx::SqlitePool;
use tracing::info;

pub const INIT_SQL: &str = include_str!("../../../scripts/init_db.sql");

#[derive(Clone)]
pub struct Store {
    pool: SqlitePool,
}

impl Store {
    pub async fn connect(path: &str) -> Result<Self> {
        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect(path)
            .await?;
        run_init_sql(&pool).await?;
        Ok(Self { pool })
    }

    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    pub async fn insert_run(&self, run_id: &str, git_sha: Option<&str>) -> Result<()> {
        let host = hostname::get()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        let ts_ms = Utc::now().timestamp_millis();
        sqlx::query(
            "INSERT OR REPLACE INTO runs (run_id, started_at_ms, git_sha, host) VALUES (?1, ?2, ?3, ?4)",
        )
        .bind(run_id)
        .bind(ts_ms)
        .bind(git_sha)
        .bind(host)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn log_event(
        &self,
        run_id: &str,
        source: &str,
        topic: &str,
        payload_json: &str,
    ) -> Result<()> {
        let ts_ms = Utc::now().timestamp_millis();
        sqlx::query(
            "INSERT INTO raw_events (run_id, ts_ms, source, topic, payload_json) VALUES (?1, ?2, ?3, ?4, ?5)",
        )
        .bind(run_id)
        .bind(ts_ms)
        .bind(source)
        .bind(topic)
        .bind(payload_json)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn log_incident(
        &self,
        run_id: &str,
        severity: &str,
        kind: &str,
        message: &str,
    ) -> Result<()> {
        let ts_ms = Utc::now().timestamp_millis();
        sqlx::query(
            "INSERT INTO incidents (run_id, ts_ms, severity, kind, message) VALUES (?1, ?2, ?3, ?4, ?5)",
        )
        .bind(run_id)
        .bind(ts_ms)
        .bind(severity)
        .bind(kind)
        .bind(message)
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}

pub async fn init_sqlite(path: &str) -> Result<Store> {
    let store = Store::connect(path).await?;
    info!(path = path, "sqlite initialized");
    Ok(store)
}

async fn run_init_sql(pool: &SqlitePool) -> Result<()> {
    for statement in INIT_SQL.split(';') {
        let trimmed = statement.trim();
        if trimmed.is_empty() {
            continue;
        }
        sqlx::query(trimmed).execute(pool).await?;
    }
    Ok(())
}
