use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "loopback-mcp-proxy", version, about = "Loopback MCP proxy")]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Serve the MCP proxy, admin API, and web UI
    Serve,
    /// Admin console attached to a running proxy
    Tui {
        #[arg(long, env = "PROXY_URL", default_value = "http://127.0.0.1:8080")]
        url: String,
        #[arg(long, env = "ADMIN_TOKEN")]
        token: Option<String>,
    },
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    let result = match cli.command.unwrap_or(Command::Serve) {
        Command::Serve => {
            tracing_subscriber::fmt()
                .with_env_filter(
                    tracing_subscriber::EnvFilter::try_from_default_env()
                        .unwrap_or_else(|_| "info".into()),
                )
                .init();
            match loopback_mcp_proxy::config::Config::from_env() {
                Ok(config) => loopback_mcp_proxy::serve(config).await,
                Err(err) => Err(err),
            }
        }
        Command::Tui { url, token } => loopback_mcp_proxy::tui::run(&url, token.as_deref()).await,
    };
    if let Err(err) = result {
        eprintln!("error: {err}");
        std::process::exit(1);
    }
}
