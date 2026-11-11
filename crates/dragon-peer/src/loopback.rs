//! Real host-only WebRTC regression. No public signaling, STUN/TURN, microphone or saves.
use super::*;
use matchbox_signaling::SignalingServer;
use std::{
    net::{Ipv4Addr, SocketAddr},
    time::Duration,
};
use tokio::{
    task::JoinSet,
    time::{sleep, timeout},
};

#[test]
#[ignore = "explicit local network integration; run with --features loopback-test -- --ignored"]
fn two_native_clients_exchange_over_host_only_webrtc() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        timeout(Duration::from_secs(30), exchange())
            .await
            .expect("loopback integration exceeded 30 seconds");
    });
}

// Matchbox requires a complete HTTP response as its connection-rejection error.
#[allow(clippy::result_large_err)]
async fn exchange() {
    let mut server = SignalingServer::full_mesh_builder(SocketAddr::from((Ipv4Addr::LOCALHOST, 0)))
        .on_connection_request(|meta| {
            Ok(meta
                .path
                .as_deref()
                .is_some_and(|p| p.starts_with("crabscape-dragon-loopback-")))
        })
        .build();
    let address = server
        .bind()
        .expect("bind ephemeral loopback signaling listener");
    assert!(address.ip().is_loopback());
    let mut tasks = JoinSet::new(); // Aborts every server/pump on panic, timeout or scope exit.
    tasks.spawn(async move {
        server.serve().await.expect("local signaling server");
    });
    let introducer = format!("ws://{address}");
    let host_only = || RtcIceServerConfig {
        urls: Vec::new(),
        username: None,
        credential: None,
    };
    let (mut alice, alice_pump) =
        NativePeer::connect_with_ice(&introducer, "loopback", "synthetic", host_only()).unwrap();
    let (mut bob, bob_pump) =
        NativePeer::connect_with_ice(&introducer, "loopback", "synthetic", host_only()).unwrap();
    tasks.spawn(async move {
        let _ = alice_pump.await;
    });
    let bob_abort = tasks.spawn(async move {
        let _ = bob_pump.await;
    });

    let (alice_id, bob_id) = timeout(Duration::from_secs(15), async {
        loop {
            let a = alice.poll();
            let b = bob.poll();
            assert!(!a.ended && !b.ended, "pump ended before peers connected");
            if alice.session.peers().count() == 1 && bob.session.peers().count() == 1 {
                break (a.id.unwrap(), b.id.unwrap());
            }
            sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("both native clients establish all three channels");
    assert_ne!(alice_id, bob_id);
    assert!(alice.send(
        "world",
        serde_json::json!({"order":1, "peer":"forged"}),
        Delivery::Reliable
    ));
    assert!(alice.send("world", serde_json::json!({"order":2}), Delivery::Reliable));
    // Inject the exact same wire packet twice. The receiver must emit it only once.
    let replay = alice
        .session
        .encode("world", serde_json::json!({"order":3}))
        .unwrap();
    for _ in 0..2 {
        alice
            .socket
            .channel_mut(0)
            .try_send(replay.clone().into_boxed_slice(), bob_id)
            .unwrap();
    }
    // A later valid marker fences the duplicate on the ordered reliable channel.
    assert!(alice.send("world", serde_json::json!({"order":4}), Delivery::Reliable));
    let mut orders = Vec::new();
    timeout(Duration::from_secs(5), async {
        while orders.last() != Some(&4) {
            for event in bob.poll().events {
                if let Event::Message(message) = event {
                    assert_eq!(message.peer, alice_id);
                    assert_eq!(message.topic, "world");
                    orders.push(message.data["order"].as_u64().unwrap());
                }
            }
            sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("ordered reliable messages arrive");
    assert_eq!(
        orders,
        [1, 2, 3, 4],
        "duplicate wire sequence must be dropped"
    );

    assert!(bob.send(
        "presence",
        serde_json::json!({"ack":true}),
        Delivery::Unreliable
    ));
    timeout(Duration::from_secs(5), async {
        loop {
            if alice.poll().events.into_iter().any(|event| matches!(event,
                Event::Message(Message { peer, topic, data }) if peer == bob_id && topic == "presence" && data["ack"] == true)) { break; }
            sleep(Duration::from_millis(10)).await;
        }
    }).await.expect("reverse unreliable message arrives");
    alice.close();
    assert!(alice.poll().ended);
    timeout(Duration::from_secs(5), async {
        loop {
            if bob
                .poll()
                .events
                .into_iter()
                .any(|event| matches!(event, Event::Left(peer) if peer == alice_id))
            {
                break;
            }
            sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("remaining client sees disconnect");
    bob_abort.abort();
    timeout(Duration::from_secs(2), async {
        while !bob.poll().ended {
            sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("cancelled pump marks peer ended");
    assert!(!bob.send("hello", serde_json::json!({}), Delivery::Reliable));
    drop(alice);
    drop(bob);
    tasks.abort_all();
    while tasks.join_next().await.is_some() {}
    eprintln!(
        "Two native peers: connected, ordered/replay-safe reliable exchange, reverse unreliable exchange, disconnect and cancellation passed; all tasks stopped."
    );
}
