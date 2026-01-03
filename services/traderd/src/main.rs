use std::{env, fs, future, net::SocketAddr, path::Path, time::Duration};

use admin_ipc::{run_server, AdminRequest, AdminResponse, DEFAULT_SOCKET_PATH};
use anyhow::bail;
use clap::Parser;
use metrics::MetricsHandle;
use risk::RiskGate;
use storage::init_sqlite;
use tokio::task;
use tokio::time;
use tracing::{info, warn, Level};
use uuid::Uuid;

#[derive(Parser, Debug)]
struct Args {
    #[arg(long, env = "SQLITE_PATH", default_value = "sqlite://bot.db")]
    sqlite_path: String,

    #[arg(long, env = "ADMIN_SOCKET", default_value = DEFAULT_SOCKET_PATH)]
    admin_socket: String,

    #[arg(long, env = "METRICS_ADDR", default_value = "127.0.0.1:9109")]
    metrics_addr: SocketAddr,
}

fn log_startup(args: &Args, run_id: &str) {
    info!(path = %args.sqlite_path, "sqlite path configured");
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
        let fs_path = std::path::Path::new(path_part);
        if let Some(parent) = fs_path.parent() {
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

    let args = Args::parse();
    validate_sqlite_path(&args.sqlite_path)?;
    info!(
        sqlite = %args.sqlite_path,
        socket = %args.admin_socket,
        "booting traderd"
    );

    ensure_sqlite_parent_dir(&args.sqlite_path)?;

    let run_id = Uuid::new_v4().to_string();
    let store = init_sqlite(&args.sqlite_path).await?;
    store.insert_run(&run_id, None).await?;
    log_startup(&args, &run_id);

    let missing_tables = store.validate_required_tables().await?;
    if !missing_tables.is_empty() {
        warn!(tables = ?missing_tables, "sqlite missing required tables");
        if let Err(err) = store
            .log_incident(
                &run_id,
                "warning",
                "db_schema_missing",
                &format!(
                    "sqlite missing required tables: {}",
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
        sqlite = %args.sqlite_path,
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
            "--sqlite-path",
            "sqlite:///tmp/test.db",
            "--admin-socket",
            "/tmp/test.sock",
            "--metrics-addr",
            "127.0.0.1:9000",
        ]);
        let run_id = Uuid::nil().to_string();
        let buffer = Arc::new(Mutex::new(Vec::new()));
        let writer = VecWriter(buffer.clone());
        let subscriber = tracing_subscriber::fmt()
            .with_max_level(Level::INFO)
            .with_writer(writer)
            .finish();

        tracing::subscriber::with_default(subscriber, || {
            log_startup(&args, &run_id);
        });

        let output =
            String::from_utf8(buffer.lock().unwrap().clone()).expect("log output should be utf8");
        assert!(output.contains("sqlite path configured"));
        assert!(output.contains("admin socket bind planned"));
        assert!(output.contains("metrics bind planned"));
        assert!(output.contains("run initialized"));
        assert!(output.contains(&args.sqlite_path));
        assert!(output.contains(&args.admin_socket));
        assert!(output.contains(&args.metrics_addr.to_string()));
        assert!(output.contains(&run_id));
    }

    #[test]
    fn creates_parent_directory_for_windows_style_sqlite_url() {
        let tmp_dir = std::env::temp_dir().join(format!("poly_traderd_{}", Uuid::new_v4()));
        std::fs::create_dir_all(&tmp_dir).expect("temp dir should be creatable");

        let original_dir = std::env::current_dir().expect("current dir should be readable");
        std::env::set_current_dir(&tmp_dir).expect("should be able to change to temp dir");

        ensure_sqlite_parent_dir("sqlite://C:/poly/data/bot.db")
            .expect("should be able to create parent directories");

        let expected_parent = tmp_dir.join("C:").join("poly").join("data");
        assert!(
            expected_parent.is_dir(),
            "expected parent directory {:?} to exist",
            expected_parent
        );

        std::env::set_current_dir(original_dir).expect("should be able to restore cwd");
    }
}
