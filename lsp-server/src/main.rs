use std::env;
use std::path::PathBuf;
use std::sync::Arc;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

mod ast_cache;
mod backend;
mod bridge;
mod document_cache;
mod semantic_analysis;

use backend::Backend;
use bridge::{BridgeState, RealContextBridge};

fn detect_project_root() -> PathBuf {
    let cwd = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let mut current = Some(cwd.as_path());
    while let Some(dir) = current {
        if dir.join(".git").exists()
            || dir.join("Cargo.toml").exists()
            || dir.join("package.json").exists()
        {
            return dir.to_path_buf();
        }
        current = dir.parent();
    }
    cwd
}

#[tokio::main]
async fn main() {
    let log_dir = env::current_dir()
        .unwrap_or_else(|_| std::path::PathBuf::from("."))
        .join("logs");
    let _ = std::fs::create_dir_all(&log_dir);

    let (non_blocking, _guard) = tracing_appender::non_blocking(
        tracing_appender::rolling::daily(&log_dir, "multilink-lsp.log"),
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

    let project_root = detect_project_root();
    tracing::info!("Project root: {}", project_root.display());

    let index_dir = project_root.join(".multilink").join("index");
    let engine = Arc::new(multilink_core::context_engine::ContextEngineV2::new(
        index_dir,
    )) as Arc<dyn multilink_core::ContextEngine>;

    let real_bridge = RealContextBridge::new(engine, project_root.to_string_lossy().to_string());
    let bridge_state = Arc::new(BridgeState::new(Arc::new(real_bridge)));

    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let (service, socket) = tower_lsp::LspService::new(|client| {
        Backend::with_bridge(client, bridge_state.clone())
    });
    tower_lsp::Server::new(stdin, stdout, socket)
        .serve(service)
        .await;
}
