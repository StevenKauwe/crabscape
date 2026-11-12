#![forbid(unsafe_code)]
use harbor_rendezvous::{Broker, Config, HOST};
use std::os::unix::fs::MetadataExt;
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args()
        .nth(1)
        .ok_or("Usage: harbor-rendezvous PRIVATE_CONFIG_JSON")?;
    let metadata = std::fs::symlink_metadata(&path)?;
    if !metadata.is_file() || metadata.mode() & 0o077 != 0 || metadata.len() > 4096 {
        return Err("Config must be a private regular file <=4KiB".into());
    }
    let config: Config = serde_json::from_slice(&std::fs::read(path)?)?;
    let expiry = config.expires_at;
    let broker = Broker::new(config)?;
    let listener = tokio::net::TcpListener::bind(HOST).await?;
    eprintln!("Loopback signaling ready at {HOST}; no tool execution or credential endpoints");
    axum::serve(listener,broker.router()).with_graceful_shutdown(async move{
        tokio::select!{_=tokio::signal::ctrl_c()=>{},_=tokio::time::sleep(std::time::Duration::from_secs(expiry.saturating_sub(harbor_rendezvous::now())))=>{}}
    }).await?;
    Ok(())
}
