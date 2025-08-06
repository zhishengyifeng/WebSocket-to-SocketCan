mod messages;
mod server;
mod client;

use std::{sync::LazyLock, time::Instant};
use clap::{Parser, Subcommand};
use env_logger::Builder;
use log::LevelFilter;

#[derive(Parser)]
#[command(name = "ws2scan")]
#[command(about = "WebSocket server/client", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
    bus:String,
}

#[derive(Subcommand)]
enum Commands {
    /// Start WebSocket server
    Server {
        #[arg(short, long, default_value = "127.0.0.1:8080")]
        addr: String,
    },
    /// Run WebSocket client
    Client {
        #[command(flatten)]
        args: client::ClientArgs,
    },
}

static START_TIME: LazyLock<Instant> = LazyLock::new(Instant::now);

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    Builder::new().filter_level(LevelFilter::Info).init();
    
    let cli = Cli::parse();
    
    match cli.command {
        Commands::Server { addr } => {
            server::run_server(&addr,cli.bus).await
        }
        Commands::Client { args } => {
            client::run_client(args).await
        }
    }
}