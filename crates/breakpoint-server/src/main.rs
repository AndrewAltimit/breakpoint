use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    tracing::info!("Breakpoint server starting");

    // TODO(phase1): Axum router with WSS, REST event ingestion, SSE, and static file serving
}
