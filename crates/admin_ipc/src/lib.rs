use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

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

#[cfg(unix)]
mod unix {
    use super::*;
    use tokio::net::unix::{OwnedReadHalf, OwnedWriteHalf};
    use tokio::net::{UnixListener, UnixStream};
    use tracing::info;

    pub async fn run_server<F>(socket_path: &str, handler: F) -> Result<()>
    where
        F: Fn(AdminRequest) -> Result<AdminResponse> + Send + Sync + 'static,
    {
        let _ = std::fs::remove_file(socket_path);
        let listener = UnixListener::bind(socket_path)?;
        let handler = std::sync::Arc::new(handler);
        info!(socket = socket_path, "admin ipc listening");
        loop {
            let (stream, _) = listener.accept().await?;
            let handler = handler.clone();
            tokio::spawn(async move {
                if let Err(err) = handle_stream(stream, handler).await {
                    tracing::warn!(error = ?err, "admin ipc handler error");
                }
            });
        }
    }

    async fn handle_stream<F>(stream: UnixStream, handler: std::sync::Arc<F>) -> Result<()>
    where
        F: Fn(AdminRequest) -> Result<AdminResponse> + Send + Sync + 'static,
    {
        let (read_half, mut write_half): (OwnedReadHalf, OwnedWriteHalf) = stream.into_split();
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
        let mut stream: UnixStream = UnixStream::connect(socket_path).await?;
        let line = serde_json::to_string(req)? + "\n";
        stream.write_all(line.as_bytes()).await?;
        let (read_half, _): (OwnedReadHalf, OwnedWriteHalf) = stream.into_split();
        let mut reader = BufReader::new(read_half);
        let mut buf = String::new();
        let _ = reader.read_line(&mut buf).await?;
        let resp: AdminResponse = serde_json::from_str(buf.trim())?;
        Ok(resp)
    }
}

#[cfg(not(unix))]
mod unix {
    use super::*;

    pub async fn run_server<F>(_socket_path: &str, _handler: F) -> Result<()>
    where
        F: Fn(AdminRequest) -> Result<AdminResponse> + Send + Sync + 'static,
    {
        bail!("admin ipc server is only supported on unix platforms");
    }

    pub async fn send_request(_socket_path: &str, _req: &AdminRequest) -> Result<AdminResponse> {
        bail!("admin ipc client is only supported on unix platforms");
    }
}

pub use unix::{run_server, send_request};
