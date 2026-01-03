use anyhow::Result;
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use tracing::info;

pub const DEFAULT_SOCKET_PATH: &str = "/tmp/polymarket_bot.sock";

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(tag = "type", content = "payload")]
pub enum AdminRequest {
    Status,
    Pause,
    Resume,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AdminStatus {
    pub run_id: String,
    pub risk_state: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(tag = "type", content = "payload")]
pub enum AdminResponse {
    Status(AdminStatus),
    Ack,
    Error(String),
}

pub async fn run_server<F>(socket_path: &str, handler: F) -> Result<()>
where
    F: Fn(AdminRequest) -> Result<AdminResponse> + Send + Sync + 'static,
{
    let _ = std::fs::remove_file(socket_path);
    let listener = UnixListener::bind(socket_path)?;
    let handler = std::sync::Arc::new(handler);
    info!("admin ipc listening", socket = socket_path);
    loop {
        let (stream, _) = listener.accept().await?;
        let handler = handler.clone();
        tokio::spawn(async move {
            if let Err(err) = handle_stream(stream, handler).await {
                tracing::warn!("admin ipc handler error", error = ?err);
            }
        });
    }
}

async fn handle_stream<F>(stream: UnixStream, handler: std::sync::Arc<F>) -> Result<()>
where
    F: Fn(AdminRequest) -> Result<AdminResponse> + Send + Sync + 'static,
{
    let (read_half, mut write_half) = stream.into_split();
    let mut reader = BufReader::new(read_half);
    let mut buf = String::new();
    let n = reader.read_line(&mut buf).await?;
    if n == 0 {
        return Ok(());
    }
    let req: AdminRequest = serde_json::from_str(buf.trim())?;
    let resp = handler(req)?;
    let line = serde_json::to_string(&resp)? + "\n";
    write_half.write_all(line.as_bytes()).await?;
    Ok(())
}

pub async fn send_request(socket_path: &str, req: &AdminRequest) -> Result<AdminResponse> {
    let mut stream = UnixStream::connect(socket_path).await?;
    let line = serde_json::to_string(req)? + "\n";
    stream.write_all(line.as_bytes()).await?;
    let (read_half, _) = stream.into_split();
    let mut reader = BufReader::new(read_half);
    let mut buf = String::new();
    let _ = reader.read_line(&mut buf).await?;
    let resp: AdminResponse = serde_json::from_str(buf.trim())?;
    Ok(resp)
}
