#![forbid(unsafe_code)]
use std::net::SocketAddr;
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Network exposure is an explicit deployment choice, never the development default.
    let address: SocketAddr = std::env::var("CRABBRIDGE_BIND")
        .unwrap_or_else(|_| "127.0.0.1:8080".into())
        .parse()?;
    let listener = tokio::net::TcpListener::bind(address).await?;
    // No request/query/body logging: credentials never belong in telemetry.
    axum::serve(listener, crabbridge_frontend::router())
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await?;
    Ok(())
}
