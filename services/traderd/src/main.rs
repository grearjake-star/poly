use std::{future, net::SocketAddr, time::Duration};

use admin_ipc::{run_server, AdminRequest, AdminResponse, DEFAULT_SOCKET_PATH};
use clap::Parser;
use metrics::MetricsHandle;
use risk::RiskGate;
use storage::init_sqlite;
use tokio::task;
use tokio::time;
use tracing::{info, Level};
use uuid::Uuid;

#[derive(Parser, Debug)]
struct Args {
    #[arg(long, env = "SQLITE_PATH", default_value = "bot.db")]
    sqlite_path: String,

    #[arg(long, env = "ADMIN_SOCKET", default_value = DEFAULT_SOCKET_PATH)]
    admin_socket: String,

    #[arg(long, env = "METRICS_ADDR", default_value = "127.0.0.1:9109")]
    metrics_addr: SocketAddr,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_max_level(Level::INFO)
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let args = Args::parse();
    info!(
        sqlite = %args.sqlite_path,
        socket = %args.admin_socket,
        "booting traderd"
    );

    let run_id = Uuid::new_v4().to_string();
    let store = init_sqlite(&args.sqlite_path).await?;
    store.insert_run(&run_id, None).await?;

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
    let metrics_addr = args.metrics_addr;
    let metrics_task = metrics.clone();
    task::spawn(async move {
        if let Err(err) = metrics_task.serve(metrics_addr).await {
            tracing::error!(error = ?err, "metrics server error");
        }
    });

    info!(%run_id, "started");

    let store_clone = store.clone();
    let run_id_clone2 = run_id.clone();
    task::spawn(async move {
        let mut ticker = time::interval(Duration::from_secs(1));
        let mut tick: u64 = 0;
        loop {
            ticker.tick().await;
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
