use std::net::SocketAddr;

use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .init();

    let token = std::env::var("NOT_TOKEN").unwrap_or_default();
    if token.len() < 32 {
        eprintln!("NOT_TOKEN must be set to a random secret of at least 32 characters");
        std::process::exit(2);
    }
    let db_path = std::env::var("NOT_DB").unwrap_or_else(|_| "/data/not.db".into());
    let addr: SocketAddr = std::env::var("NOT_ADDR")
        .unwrap_or_else(|_| "0.0.0.0:8080".into())
        .parse()
        .expect("NOT_ADDR must be host:port");

    let state = not_server::AppState::open(&db_path, &token).expect("open database");
    let listener = tokio::net::TcpListener::bind(addr).await.expect("bind");
    tracing::info!(%addr, db = %db_path, "not-server listening");
    axum::serve(listener, not_server::router(state))
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await
        .expect("serve");
}
