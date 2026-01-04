use anyhow::{bail, Result};
use chrono::Utc;
use sqlx::migrate::Migrator;
#[cfg(feature = "postgres")]
use sqlx::postgres::PgPoolOptions;
#[cfg(feature = "sqlite")]
use sqlx::sqlite::SqlitePoolOptions;
#[cfg(feature = "postgres")]
use sqlx::PgPool;
#[cfg(feature = "sqlite")]
use sqlx::SqlitePool;
use tracing::info;

const REQUIRED_TABLES: &[&str] = &["runs", "raw_events", "incidents"];

#[cfg(feature = "sqlite")]
static SQLITE_MIGRATOR: Migrator = sqlx::migrate!("../../migrations/sqlite");

#[cfg(feature = "postgres")]
static POSTGRES_MIGRATOR: Migrator = sqlx::migrate!("../../migrations/postgres");

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DatabaseBackend {
    Sqlite,
    Postgres,
}

impl DatabaseBackend {
    pub fn from_url(url: &str) -> Result<Self> {
        if url.starts_with("sqlite://") || url.starts_with("sqlite::memory:") {
            Ok(DatabaseBackend::Sqlite)
        } else if url.starts_with("postgres://") || url.starts_with("postgresql://") {
            Ok(DatabaseBackend::Postgres)
        } else {
            bail!("database url must start with sqlite://, sqlite::memory:, postgres://, or postgresql://");
        }
    }
}

#[derive(Clone)]
enum StorePool {
    #[cfg(feature = "sqlite")]
    Sqlite(SqlitePool),
    #[cfg(feature = "postgres")]
    Postgres(PgPool),
}

#[derive(Clone)]
pub struct Store {
    pool: StorePool,
    backend: DatabaseBackend,
}

impl Store {
    pub async fn connect(url: &str) -> Result<Self> {
        let backend = DatabaseBackend::from_url(url)?;

        #[cfg(all(not(feature = "sqlite"), feature = "postgres"))]
        if matches!(backend, DatabaseBackend::Sqlite) {
            bail!("sqlite backend is not enabled");
        }

        #[cfg(all(feature = "sqlite", not(feature = "postgres")))]
        if matches!(backend, DatabaseBackend::Postgres) {
            bail!("postgres backend is not enabled");
        }

        let pool = match backend {
            #[cfg(feature = "sqlite")]
            DatabaseBackend::Sqlite => {
                let pool = SqlitePoolOptions::new()
                    .max_connections(5)
                    .connect(url)
                    .await?;
                SQLITE_MIGRATOR.run(&pool).await?;
                StorePool::Sqlite(pool)
            }
            #[cfg(feature = "postgres")]
            DatabaseBackend::Postgres => {
                let pool = PgPoolOptions::new().max_connections(5).connect(url).await?;
                POSTGRES_MIGRATOR.run(&pool).await?;
                StorePool::Postgres(pool)
            }
            #[allow(unreachable_patterns)]
            _ => bail!("unsupported backend"),
        };

        Ok(Self { pool, backend })
    }

    pub fn backend(&self) -> DatabaseBackend {
        self.backend
    }

    pub async fn insert_run(&self, run_id: &str, git_sha: Option<&str>) -> Result<()> {
        let host = hostname::get()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        let ts_ms = Utc::now().timestamp_millis();

        match &self.pool {
            #[cfg(feature = "sqlite")]
            StorePool::Sqlite(pool) => {
                sqlx::query(
                    "INSERT INTO runs (run_id, started_at_ms, git_sha, host) VALUES (?1, ?2, ?3, ?4)
                     ON CONFLICT(run_id) DO UPDATE SET started_at_ms = excluded.started_at_ms, git_sha = excluded.git_sha, host = excluded.host",
                )
                .bind(run_id)
                .bind(ts_ms)
                .bind(git_sha)
                .bind(host)
                .execute(pool)
                .await?;
            }
            #[cfg(feature = "postgres")]
            StorePool::Postgres(pool) => {
                sqlx::query(
                    "INSERT INTO runs (run_id, started_at_ms, git_sha, host) VALUES ($1, $2, $3, $4)
                     ON CONFLICT(run_id) DO UPDATE SET started_at_ms = excluded.started_at_ms, git_sha = excluded.git_sha, host = excluded.host",
                )
                .bind(run_id)
                .bind(ts_ms)
                .bind(git_sha)
                .bind(host)
                .execute(pool)
                .await?;
            }
        }
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

        match &self.pool {
            #[cfg(feature = "sqlite")]
            StorePool::Sqlite(pool) => {
                sqlx::query(
                    "INSERT INTO raw_events (run_id, ts_ms, source, topic, payload_json) VALUES (?1, ?2, ?3, ?4, ?5)",
                )
                .bind(run_id)
                .bind(ts_ms)
                .bind(source)
                .bind(topic)
                .bind(payload_json)
                .execute(pool)
                .await?;
            }
            #[cfg(feature = "postgres")]
            StorePool::Postgres(pool) => {
                sqlx::query(
                    "INSERT INTO raw_events (run_id, ts_ms, source, topic, payload_json) VALUES ($1, $2, $3, $4, $5)",
                )
                .bind(run_id)
                .bind(ts_ms)
                .bind(source)
                .bind(topic)
                .bind(payload_json)
                .execute(pool)
                .await?;
            }
        }
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

        match &self.pool {
            #[cfg(feature = "sqlite")]
            StorePool::Sqlite(pool) => {
                sqlx::query(
                    "INSERT INTO incidents (run_id, ts_ms, severity, kind, message) VALUES (?1, ?2, ?3, ?4, ?5)",
                )
                .bind(run_id)
                .bind(ts_ms)
                .bind(severity)
                .bind(kind)
                .bind(message)
                .execute(pool)
                .await?;
            }
            #[cfg(feature = "postgres")]
            StorePool::Postgres(pool) => {
                sqlx::query(
                    "INSERT INTO incidents (run_id, ts_ms, severity, kind, message) VALUES ($1, $2, $3, $4, $5)",
                )
                .bind(run_id)
                .bind(ts_ms)
                .bind(severity)
                .bind(kind)
                .bind(message)
                .execute(pool)
                .await?;
            }
        }
        Ok(())
    }

    pub async fn validate_required_tables(&self) -> Result<Vec<String>> {
        let mut missing = Vec::new();

        for table in REQUIRED_TABLES {
            let exists = match &self.pool {
                #[cfg(feature = "sqlite")]
                StorePool::Sqlite(pool) => {
                    sqlx::query_scalar::<_, Option<String>>(
                        "SELECT name FROM sqlite_master WHERE type='table' AND name = ?1",
                    )
                    .bind(table)
                    .fetch_optional(pool)
                    .await?
                    .is_some()
                }
                #[cfg(feature = "postgres")]
                StorePool::Postgres(pool) => {
                    sqlx::query_scalar::<_, bool>(
                        "SELECT EXISTS (SELECT 1 FROM information_schema.tables WHERE table_schema = current_schema() AND table_name = $1)",
                    )
                    .bind(table)
                    .fetch_one(pool)
                    .await?
                }
            };

            if !exists {
                missing.push((*table).to_string());
            }
        }

        Ok(missing)
    }
}

pub async fn init_sqlite(path: &str) -> Result<Store> {
    let store = Store::connect(path).await?;
    info!(path = %path, "sqlite initialized");
    Ok(store)
}

pub async fn init_postgres(url: &str) -> Result<Store> {
    let store = Store::connect(url).await?;
    info!(url = %url, "postgres initialized");
    Ok(store)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn init_and_validate_required_tables() -> Result<()> {
        let store = init_sqlite("sqlite::memory:?cache=shared").await?;
        let missing_tables = store.validate_required_tables().await?;

        assert!(
            missing_tables.is_empty(),
            "missing tables: {:?}",
            missing_tables
        );

        Ok(())
    }

    #[test]
    fn detects_backends_from_url() {
        assert_eq!(
            DatabaseBackend::from_url("sqlite://bot.db").unwrap(),
            DatabaseBackend::Sqlite
        );
        assert_eq!(
            DatabaseBackend::from_url("sqlite::memory:?cache=shared").unwrap(),
            DatabaseBackend::Sqlite
        );
        assert_eq!(
            DatabaseBackend::from_url("postgres://localhost/poly").unwrap(),
            DatabaseBackend::Postgres
        );
        assert_eq!(
            DatabaseBackend::from_url("postgresql://localhost/poly").unwrap(),
            DatabaseBackend::Postgres
        );
    }

    #[cfg(feature = "postgres")]
    #[tokio::test]
    async fn runs_migrations_on_postgres_when_available() -> Result<()> {
        let url = match std::env::var("TEST_POSTGRES_URL") {
            Ok(url) => url,
            Err(_) => {
                eprintln!("skipping postgres migration test; TEST_POSTGRES_URL not set");
                return Ok(());
            }
        };

        let store = Store::connect(&url).await?;
        let missing_tables = store.validate_required_tables().await?;
        assert!(
            missing_tables.is_empty(),
            "missing tables: {:?}",
            missing_tables
        );
        Ok(())
    }
}
