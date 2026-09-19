#![deny(clippy::print_stdout)]

use jev_rust_review::{config::Config, mcp::Server};
use rmcp::{ServiceExt, transport::stdio};

#[tokio::main]
async fn main() -> std::process::ExitCode {
    // `--version` lets the launcher check a cached binary without starting
    // the protocol. It is the only output this binary ever writes to stdout
    // outside the MCP transport.
    if std::env::args().nth(1).as_deref() == Some("--version") {
        use std::io::Write;
        let mut out = std::io::stdout().lock();
        let _ = writeln!(out, "jev-rust-review {}", env!("CARGO_PKG_VERSION"));
        return std::process::ExitCode::SUCCESS;
    }

    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_env("JEV_RUST_REVIEW_LOG")
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn")),
        )
        .init();

    let cfg = Config::from_env();
    tracing::info!(config = ?cfg, "starting jev-rust-review");
    let service = match Server::new(cfg).serve(stdio()).await {
        Ok(s) => s,
        Err(e) => {
            tracing::error!("failed to start MCP server: {e}");
            return std::process::ExitCode::FAILURE;
        }
    };
    if let Err(e) = service.waiting().await {
        tracing::error!("MCP server stopped with an error: {e}");
        return std::process::ExitCode::FAILURE;
    }
    std::process::ExitCode::SUCCESS
}
