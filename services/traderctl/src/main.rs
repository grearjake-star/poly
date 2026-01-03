use admin_ipc::{send_request, AdminRequest, DEFAULT_SOCKET_PATH};
use anyhow::Result;
use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
struct Cli {
    #[arg(long, env = "ADMIN_SOCKET", default_value = DEFAULT_SOCKET_PATH)]
    socket: String,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    Status,
    Pause,
    Resume,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let req = match cli.command {
        Command::Status => AdminRequest::Status,
        Command::Pause => AdminRequest::Pause,
        Command::Resume => AdminRequest::Resume,
    };

    let resp = send_request(&cli.socket, &req).await?;
    println!("{}", serde_json::to_string(&resp)?);
    Ok(())
}
