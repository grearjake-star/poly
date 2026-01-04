#[cfg(test)]
use std::sync::{Mutex, OnceLock};
use std::{env, fs, future, net::SocketAddr, path::PathBuf, time::Duration};

use admin_ipc::{run_server, AdminRequest, AdminResponse, DEFAULT_SOCKET_PATH};
use anyhow::bail;
use clap::Parser;
use metrics::MetricsHandle;
use risk::RiskGate;
use storage::{DatabaseBackend, Store};
use tokio::task;
use tokio::time;
use tracing::{info, warn, Level};
use uuid::Uuid;

#[derive(Parser, Debug)]
struct Args {
    #[arg(
        long,
        env = "DB_URL",
        default_value = "sqlite://bot.db",
        alias = "sqlite-path"
    )]
    db_url: String,

    #[arg(long, env = "ADMIN_SOCKET", default_value = DEFAULT_SOCKET_PATH)]
    admin_socket: String,

    #[arg(long, env = "METRICS_ADDR", default_value = "127.0.0.1:9109")]
    metrics_addr: SocketAddr,
}

fn log_startup(args: &Args, backend: DatabaseBackend, run_id: &str) {
    info!(
        backend = ?backend,
        url = %args.db_url,
        "database backend configured"
    );
    info!(socket = %args.admin_socket, "admin socket bind planned");
    info!(addr = %args.metrics_addr, "metrics bind planned");
    info!(%run_id, "run initialized");
}

fn ensure_sqlite_parent_dir(path: &str) -> anyhow::Result<()> {
    const MEMORY_PREFIX: &str = "sqlite::memory:";
    const URL_PREFIX: &str = "sqlite://";

    if path.starts_with(MEMORY_PREFIX) {
        return Ok(());
    }

    if let Some(rest) = path.strip_prefix(URL_PREFIX) {
        let path_part = rest.split_once('?').map(|(path, _)| path).unwrap_or(rest);
        let fs_path = normalize_windows_style_sqlite_path(path_part);
        if let Some(parent) = fs_path.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent)?;
            }
        }
    }

    Ok(())
}
use anyhow::bail;
use std::path::PathBuf;

fn parse_sqlite_file_path(db_url: &str) -> anyhow::Result<Option<PathBuf>> {
    const MEMORY_PREFIX: &str = "sqlite::memory:";
    const URL_PREFIX: &str = "sqlite://";

    let s = db_url.trim();

    if s.starts_with(MEMORY_PREFIX) || s == ":memory:" {
        return Ok(None);
    }

    if !s.starts_with(URL_PREFIX) {
        bail!("sqlite path must start with `sqlite://` or use `sqlite::memory:`");
    }

    // strip scheme and any query params
    let rest = s.trim_start_matches(URL_PREFIX);
    let (path_part, _) = rest.split_once('?').unwrap_or((rest, ""));
    if path_part.is_empty() {
        bail!("sqlite path is missing a filesystem component after `sqlite://`");
    }

    // Windows: sqlite:///C:/... becomes "/C:/..." after stripping "sqlite://"
    // We want "C:/..." (a real absolute path).
    let mut p = path_part.to_string();

    #[cfg(windows)]
    {
        let b = p.as_bytes();
        if b.len() >= 4 && b[0] == b'/' && b[2] == b':' {
            // "/C:/..." -> "C:/..."
            p.remove(0);
        }

        // Reject drive-relative "C:foo" because it’s ambiguous and causes pain.
        let b = p.as_bytes();
        if b.len() >= 3 && b[1] == b':' && b[2] != b'\\' && b[2] != b'/' {
            bail!(
                "windows sqlite path must be absolute like `sqlite:///C:/...` (got `{}`)",
                path_part
            );
        }
    }

    Ok(Some(PathBuf::from(p)))
}

fn validate_sqlite_path(db_url: &str) -> anyhow::Result<()> {
    parse_sqlite_file_path(db_url).map(|_| ())
}

fn ensure_sqlite_parent_dir(db_url: &str) -> anyhow::Result<()> {
    if let Some(path) = parse_sqlite_file_path(db_url)? {
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)?;
            }
        }
    }
    Ok(())
}



#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_max_level(Level::INFO)
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let mut args = Args::parse();
    if args.db_url == "sqlite://bot.db" {
        if let Ok(sqlite_url) = env::var("SQLITE_PATH") {
            args.db_url = sqlite_url;
        }
    }
    let backend = DatabaseBackend::from_url(&args.db_url)?;

    if matches!(backend, DatabaseBackend::Sqlite) {
        validate_sqlite_path(&args.db_url)?;
        ensure_sqlite_parent_dir(&args.db_url)?;
    }

    info!(
        db_url = %args.db_url,
        backend = ?backend,
        socket = %args.admin_socket,
        "booting traderd"
    );

    let run_id = Uuid::new_v4().to_string();
    let store = Store::connect(&args.db_url).await?;
    store.insert_run(&run_id, None).await?;
    log_startup(&args, backend, &run_id);

    let missing_tables = store.validate_required_tables().await?;
    if !missing_tables.is_empty() {
        warn!(
            backend = ?backend,
            tables = ?missing_tables,
            "database missing required tables"
        );
        if let Err(err) = store
            .log_incident(
                &run_id,
                "warning",
                "db_schema_missing",
                &format!(
                    "database missing required tables: {}",
                    missing_tables.join(", ")
                ),
            )
            .await
        {
            warn!(error = ?err, "failed to log missing schema incident");
        }
    }

    let risk_gate = RiskGate::new();
    let run_id_clone = run_id.clone();
    let gate_clone = risk_gate.clone();
    let socket_path = args.admin_socket.clone();

    task::spawn(async move {
        let handler = move |req: AdminRequest| -> anyhow::Result<AdminResponse> {
            match req {
                AdminRequest::Status => Ok(AdminResponse::Status(admin_ipc::AdminStatus {
                    run_id: run_id_clone.clone(),
                    risk_state: format!("{:?}", gate_clone.status()),
                })),
                AdminRequest::Pause => {
                    gate_clone.pause();
                    Ok(AdminResponse::Ack)
                }
                AdminRequest::Resume => {
                    gate_clone.resume();
                    Ok(AdminResponse::Ack)
                }
            }
        };
        if let Err(err) = run_server(&socket_path, handler).await {
            tracing::error!(error = ?err, "admin ipc server failed");
        }
    });

    let metrics = MetricsHandle::new();
    let heartbeat_counter = metrics.heartbeat_counter();
    let metrics_addr = args.metrics_addr;
    let metrics_task = metrics.clone();
    task::spawn(async move {
        if let Err(err) = metrics_task.serve(metrics_addr).await {
            tracing::error!(error = ?err, "metrics server error");
        }
    });

    info!(
        run_id = %run_id,
        db_url = %args.db_url,
        backend = ?backend,
        admin_socket = %args.admin_socket,
        metrics_addr = %args.metrics_addr,
        "ready"
    );
    if let Err(err) = store
        .log_incident(&run_id, "info", "ready", "traderd booted and ready")
        .await
    {
        tracing::warn!(error = ?err, "failed to record ready incident");
    }

    info!(run_id = %run_id, "started");

    let store_clone = store.clone();
    let run_id_clone2 = run_id.clone();
    task::spawn(async move {
        let mut ticker = time::interval(Duration::from_secs(1));
        let mut tick: u64 = 0;
        loop {
            ticker.tick().await;
            heartbeat_counter.inc();
            tick += 1;
            let payload = serde_json::json!({"tick": tick});
            if let Err(err) = store_clone
                .log_event(&run_id_clone2, "internal", "tick", &payload.to_string())
                .await
            {
                tracing::warn!(error = ?err, "failed to log tick");
            }
        }
    });

    // keep running
    future::pending::<()>().await;
    Ok(())
}

#[cfg(test)]
pub(crate) fn cwd_guard() -> &'static Mutex<()> {
    static GUARD: OnceLock<Mutex<()>> = OnceLock::new();
    GUARD.get_or_init(|| Mutex::new(()))
}

#[cfg(test)]
mod sqlite_paths_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};
    use tracing_subscriber::fmt::MakeWriter;

    #[derive(Clone)]
    struct VecWriter(Arc<Mutex<Vec<u8>>>);

    impl std::io::Write for VecWriter {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            let mut guard = self.0.lock().unwrap();
            guard.extend_from_slice(buf);
            Ok(buf.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    impl<'a> MakeWriter<'a> for VecWriter {
        type Writer = VecWriter;

        fn make_writer(&'a self) -> Self::Writer {
            self.clone()
        }
    }

    #[test]
    fn startup_logs_include_configuration() {
        let args = Args::parse_from([
            "traderd",
            "--db-url",
            "sqlite:///tmp/test.db",
            "--admin-socket",
            "/tmp/test.sock",
            "--metrics-addr",
            "127.0.0.1:9000",
        ]);
        let backend = DatabaseBackend::Sqlite;
        let run_id = Uuid::nil().to_string();
        let buffer = Arc::new(Mutex::new(Vec::new()));
        let writer = VecWriter(buffer.clone());
        let subscriber = tracing_subscriber::fmt()
            .with_max_level(Level::INFO)
            .with_writer(writer)
            .finish();

        tracing::subscriber::with_default(subscriber, || {
            log_startup(&args, backend, &run_id);
        });

        let output =
            String::from_utf8(buffer.lock().unwrap().clone()).expect("log output should be utf8");
        assert!(output.contains("database backend configured"));
        assert!(output.contains("admin socket bind planned"));
        assert!(output.contains("metrics bind planned"));
        assert!(output.contains("run initialized"));
        assert!(output.contains(&args.db_url));
        assert!(output.contains(&args.admin_socket));
        assert!(output.contains(&args.metrics_addr.to_string()));
        assert!(output.contains("Sqlite"));
        assert!(output.contains(&run_id));
    }

    #[test]
    fn creates_parent_directory_for_windows_style_sqlite_url() {
        let _guard = crate::cwd_guard().lock().expect("cwd guard should lock");
        let tmp_dir = env::temp_dir().join(format!("poly_traderd_{}", Uuid::new_v4()));
        let db_path = tmp_dir.join("data").join("traderd.sqlite");
        let url = format!(
            "sqlite:///{}",
            db_path.display().to_string().replace('\\', "/")
        );
        
        ensure_sqlite_parent_dir(url).expect("should be able to create parent directories");

        let expected_parent = db_path.parent().unwrap();
        assert!(
            expected_parent.is_dir(),
            "expected parent directory {:?} to exist",
            expected_parent
        );
    }

    #[test]
    fn normalizes_drive_letter_with_leading_slash() {
        let path = parse_sqlite_file_path("sqlite:///C:/poly/data/bot.db")
            .expect("should parse")
            .expect("should be file");
        assert_eq!(path, PathBuf::from("C:/poly/data/bot.db"));
    }

    #[test]
    fn validates_memory_and_file_urls() {
        validate_sqlite_path("sqlite::memory:?cache=shared").expect("memory dsn should validate");
        validate_sqlite_path("sqlite://bot.db").expect("relative file url should validate");
        validate_sqlite_path("sqlite:///C:/poly/data/bot.db")
            .expect("absolute windows file url should validate");
    
    }
    #[test]
    fn rejects_missing_or_invalid_urls() {
        let err = validate_sqlite_path("bot.db").expect_err("should reject plain filename");
        assert!(err
            .to_string()
            .contains("must start with `sqlite://` or use `sqlite::memory:`"));

        let err = validate_sqlite_path("sqlite://").expect_err("should reject empty path");
        assert!(err
            .to_string()
            .contains("missing a filesystem component after `sqlite://`"));
    }
