//! Test-only use of the actual same Matchbox 0.14 full-mesh service implementation.
//! No reports, production credentials, deployment, or nonloopback bind.
use matchbox_signaling::SignalingServer;
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut server =
        SignalingServer::full_mesh_builder("127.0.0.1:3536".parse::<std::net::SocketAddr>()?)
            .on_connection_request(|meta| {
                Ok(meta
                    .path
                    .as_deref()
                    .is_some_and(|room| room.starts_with("crabscape-") && room.len() <= 80))
            })
            .build();
    server.bind()?;
    println!("Matchbox loopback fixture ready");
    server.serve().await?;
    Ok(())
}
