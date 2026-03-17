use std::env;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

mod backend;
mod ast_cache;
mod bridge;
mod document_cache;
mod semantic_analysis;

use backend::Backend;

#[tokio::main]
async fn main() {
    let log_dir = env::current_dir()
        .unwrap_or_else(|_| std::path::PathBuf::from("."))
        .join("logs");
    let _ = std::fs::create_dir_all(&log_dir);

    let (non_blocking, _guard) = tracing_appender::non_blocking(
        tracing_appender::rolling::daily(&log_dir, "multilink-lsp.log")
    );

    tracing_subscriber::registry()
        .with(
            tracing_subscriber::fmt::layer()
                .with_writer(non_blocking)
                .with_ansi(false),
        )
        .with(tracing_subscriber::EnvFilter::new(
            env::var("RUST_LOG").unwrap_or_else(|_| "info".to_string()),
        ))
        .init();

    tracing::info!("Starting MultiLink LSP server");

    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let (service, socket) = tower_lsp::LspService::new(Backend::new);
    tower_lsp::Server::new(stdin, stdout, socket)
        .serve(service)
        .await;
}
