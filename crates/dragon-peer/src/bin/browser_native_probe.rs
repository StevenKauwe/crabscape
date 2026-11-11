//! Explicit host-only interoperability probe; never contacts public services.
#[cfg(not(all(feature = "loopback-test", not(target_arch = "wasm32"))))]
fn main() {}
#[cfg(all(feature = "loopback-test", not(target_arch = "wasm32")))]
fn main() {
    use dragon_peer::{Delivery, Event, NativePeer};
    use std::io::Write;
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        let mut server = matchbox_signaling::SignalingServer::full_mesh_builder(
            std::net::SocketAddr::from(([127, 0, 0, 1], 0)),
        )
        .build();
        let addr = server.bind().unwrap();
        tokio::spawn(async move {
            server.serve().await.unwrap();
        });
        let url = format!("ws://{addr}");
        let (mut peer, pump) = NativePeer::connect_host_only(&url, "interop", "synthetic").unwrap();
        tokio::spawn(async move {
            let _ = pump.await;
        });
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(180);
        let mut announced = false;
        loop {
            let poll = peer.poll();
            assert!(!poll.ended, "native pump ended");
            if !announced && poll.id.is_some() {
                println!("{url}");
                std::io::stdout().flush().unwrap();
                announced = true;
            }
            for event in poll.events {
                if let Event::Message(message) = event {
                    if message.topic == "hello" && message.data["fixture"] == "browser" {
                        assert!(peer.send(
                            "world",
                            serde_json::json!({"fixture":"native","echo":message.data["nonce"]}),
                            Delivery::Reliable
                        ));
                    }
                    if message.topic == "hit" && message.data["fixture"] == "ack" {
                        println!("PASS browser/native host-only reliable exchange");
                        return;
                    }
                }
            }
            assert!(
                std::time::Instant::now() < deadline,
                "browser exchange timed out"
            );
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    });
}
