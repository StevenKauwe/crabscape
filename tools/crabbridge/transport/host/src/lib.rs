#![forbid(unsafe_code)]
mod network;
use bytes::Bytes;
use harbor_noise::{Channel, SessionContext};
pub use network::NetworkPolicy;
use std::{
    future::Future, io::Write, os::unix::fs::OpenOptionsExt, path::Path, pin::Pin, sync::Arc,
    time::Duration,
};
use tokio::sync::Mutex;
use webrtc::{
    api::{setting_engine::SettingEngine, APIBuilder},
    data_channel::{data_channel_message::DataChannelMessage, RTCDataChannel},
    peer_connection::{
        configuration::RTCConfiguration, sdp::session_description::RTCSessionDescription,
    },
};
pub type ReplyFuture = Pin<Box<dyn Future<Output = Result<Vec<u8>, String>> + Send>>;
pub type Handler = Arc<dyn Fn(Vec<u8>) -> ReplyFuture + Send + Sync>;
async fn dispatch(
    handler: Handler,
    plain: Vec<u8>,
    mut cancelled: tokio::sync::watch::Receiver<bool>,
    deadline: tokio::time::Instant,
) -> Result<Vec<u8>, String> {
    // A Tokio timer can be pending for its first poll even at an elapsed deadline.
    // Never allow an immediately-ready handler to win that expired race.
    if tokio::time::Instant::now() >= deadline {
        return Err("Expired".into());
    }
    tokio::select! {biased;_=cancelled.wait_for(|closed|*closed)=>Err("Disconnected".into()),_=tokio::time::sleep_until(deadline)=>Err("Expired".into()),result=async {handler(plain).await}=>result}
}

fn write_private(
    path: &Path,
    bytes: &[u8],
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)?;
    f.write_all(bytes)?;
    Ok(())
}
/// One loopback-only peer. The handler receives only successfully authenticated/decrypted
/// frames. It must authenticate the application user and strictly authorize every operation.
/// It must not interpret possession of the Noise capability as tool authorization.
/// Replies are limited to 16 KiB. Caller owns logout/cleanup after this function returns.
pub async fn serve_offer(
    key: [u8; 32],
    context: SessionContext,
    offer: RTCSessionDescription,
    answer_path: &Path,
    lifetime: Duration,
    handler: Handler,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let path = answer_path.to_owned();
    let publish: AnswerPublisher = Arc::new(move |answer| {
        let path = path.clone();
        Box::pin(async move {
            let bytes = serde_json::to_vec_pretty(&answer).map_err(|e| e.to_string())?;
            write_private(&path, &bytes).map_err(|e| e.to_string())
        })
    });
    serve_offer_published(key, context, offer, lifetime, handler, publish).await
}
pub type AnswerPublisher = Arc<
    dyn Fn(RTCSessionDescription) -> Pin<Box<dyn Future<Output = Result<(), String>> + Send>>
        + Send
        + Sync,
>;
/// Same authenticated loopback transport, with a caller-owned bounded signaling publisher.
pub async fn serve_offer_published(
    key: [u8; 32],
    context: SessionContext,
    offer: RTCSessionDescription,
    lifetime: Duration,
    handler: Handler,
    publish: AnswerPublisher,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    serve_offer_published_with_policy(
        key,
        context,
        offer,
        lifetime,
        handler,
        publish,
        NetworkPolicy::Loopback,
    )
    .await
}
/// Explicit two-device LAN pilot opt-in; never enables public interfaces or ICE servers.
pub async fn serve_offer_published_with_policy(
    key: [u8; 32],
    context: SessionContext,
    offer: RTCSessionDescription,
    lifetime: Duration,
    handler: Handler,
    publish: AnswerPublisher,
    policy: NetworkPolicy,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    context.validate()?;
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)?;
    let remaining = Duration::from_secs(context.expires_at)
        .checked_sub(now)
        .filter(|d| !d.is_zero())
        .ok_or("Invitation expired")?;
    let deadline = tokio::time::Instant::now() + lifetime.min(remaining);
    if offer.sdp.len() > 65536 {
        return Err("Offer too large".into());
    }
    policy.validate_offer(&offer.sdp)?;
    // No ICE servers or TURN, and mDNS remains disabled for either policy.
    let mut settings = SettingEngine::default();
    settings.set_ice_multicast_dns_mode(webrtc::ice::mdns::MulticastDnsMode::Disabled);
    let network_cleanup = network::configure(policy, &mut settings).await?;
    let api = APIBuilder::new().with_setting_engine(settings).build();
    let pc = Arc::new(api.new_peer_connection(RTCConfiguration::default()).await?);
    let (ended, mut end_rx) = tokio::sync::watch::channel(false);
    let diagnostics = network::diagnostics_enabled(policy);
    let diagnostics_started = std::time::Instant::now();
    let state_budget = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let ice_budget = state_budget.clone();
    pc.on_ice_connection_state_change(Box::new(move |state| {
        let budget = ice_budget.clone();
        Box::pin(async move {
            if diagnostics && budget.fetch_add(1, std::sync::atomic::Ordering::Relaxed) < 32 {
                eprintln!(
                    "harbor-lan t_ms={} ICE state={state:?}",
                    diagnostics_started.elapsed().as_millis()
                );
            }
        })
    }));
    let state_ended = ended.clone();
    pc.on_peer_connection_state_change(Box::new(move |state|{let ended=state_ended.clone();let budget=state_budget.clone();Box::pin(async move{if diagnostics && budget.fetch_add(1,std::sync::atomic::Ordering::Relaxed)<32 {eprintln!("harbor-lan t_ms={} PC state={state:?}", diagnostics_started.elapsed().as_millis());}if matches!(state,webrtc::peer_connection::peer_connection_state::RTCPeerConnectionState::Closed|webrtc::peer_connection::peer_connection_state::RTCPeerConnectionState::Failed|webrtc::peer_connection::peer_connection_state::RTCPeerConnectionState::Disconnected){ended.send_replace(true);}})}));
    let accepted = Arc::new(Mutex::new(false));
    let ended_signal = ended.clone();
    pc.on_data_channel(Box::new(move |dc: Arc<RTCDataChannel>| {
        let accepted = accepted.clone();
        let ended = ended.clone();
        let handler = handler.clone();
        let context = context.clone();
        Box::pin(async move {
            let mut used = accepted.lock().await;
            if *used
                || dc.label() != "harbor-v1"
                || !dc.ordered()
                || dc.max_packet_lifetime().is_some()
                || dc.max_retransmits().is_some()
            {
                let _ = dc.close().await;
                return;
            }
            *used = true;
            drop(used);
            let Ok(noise) = Channel::new(&key, false, &context) else {
                let _ = dc.close().await;
                return;
            };
            let close_end = ended.clone();
            dc.on_close(Box::new(move || {
                let end = close_end.clone();
                Box::pin(async move {
                    end.send_replace(true);
                })
            }));
            let channel = Arc::new(Mutex::new(noise));
            let writer = dc.clone();
            dc.on_message(Box::new(move |message: DataChannelMessage| {
                let channel = channel.clone();
                let writer = writer.clone();
                let handler = handler.clone();
                let ended = ended.clone();
                Box::pin(async move {
                    if *ended.borrow() || tokio::time::Instant::now() >= deadline {
                        ended.send_replace(true);
                        return;
                    }
                    if message.is_string || message.data.len() > 16_400 {
                        ended.send_replace(true);
                        let _ = writer.close().await;
                        return;
                    }
                    let mut noise = channel.lock().await;
                    if *ended.borrow() || tokio::time::Instant::now() >= deadline {
                        ended.send_replace(true);
                        return;
                    }
                    let result = if !noise.ready() {
                        noise
                            .handshake_read(&message.data)
                            .and_then(|_| noise.handshake_write())
                    } else {
                        match noise.decrypt(&message.data) {
                            Ok(plain) => {
                                match dispatch(handler, plain, ended.subscribe(), deadline).await {
                                    Ok(response) => noise.encrypt(&response),
                                    Err(error) => Err(error),
                                }
                            }
                            Err(error) => Err(error),
                        }
                    };
                    drop(noise);
                    match result {
                        Ok(bytes) => {
                            if writer.buffered_amount().await > 65536 {
                                ended.send_replace(true);
                                let _ = writer.close().await;
                            } else {
                                if writer.send(&Bytes::from(bytes)).await.is_err() {
                                    ended.send_replace(true);
                                }
                            }
                        }
                        Err(_) => {
                            ended.send_replace(true);
                            let _ = writer.close().await;
                        }
                    }
                })
            }));
        })
    }));
    let setup = async {
        pc.set_remote_description(offer).await?;
        let answer = pc.create_answer(None).await?;
        let mut complete = pc.gathering_complete_promise().await;
        pc.set_local_description(answer).await?;
        tokio::time::timeout_at(
            deadline.min(tokio::time::Instant::now() + Duration::from_secs(15)),
            complete.recv(),
        )
        .await?;
        let answer = pc.local_description().await.ok_or("Missing answer")?;
        tokio::time::timeout_at(deadline, publish(answer))
            .await?
            .map_err(std::io::Error::other)?;

        Ok::<(), Box<dyn std::error::Error + Send + Sync>>(())
    }
    .await;
    if let Err(error) = setup {
        ended_signal.send_replace(true);
        if let Some(cleanup) = &network_cleanup {
            cleanup.finish().await;
        }
        let _ = pc.close().await;
        return Err(error);
    }

    tokio::select! { _=end_rx.changed()=>{},_=tokio::signal::ctrl_c()=>{},_=tokio::time::sleep_until(deadline)=>{}}
    ended_signal.send_replace(true);
    if let Some(cleanup) = &network_cleanup {
        cleanup.finish().await;
    }
    pc.close().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};
    #[tokio::test]
    async fn already_closed_never_polls_handler() {
        let (tx, rx) = tokio::sync::watch::channel(false);
        tx.send_replace(true);
        let called = Arc::new(AtomicBool::new(false));
        let flag = called.clone();
        let handler: Handler = Arc::new(move |_| {
            flag.store(true, Ordering::SeqCst);
            Box::pin(async move { Ok(vec![]) })
        });
        assert!(dispatch(
            handler,
            vec![],
            rx,
            tokio::time::Instant::now() + Duration::from_secs(1)
        )
        .await
        .is_err());
        assert!(!called.load(Ordering::SeqCst));
    }
    #[tokio::test]
    async fn disconnect_cancels_pending_handler() {
        let (tx, rx) = tokio::sync::watch::channel(false);
        let finished = Arc::new(AtomicBool::new(false));
        let flag = finished.clone();
        let handler: Handler = Arc::new(move |_| {
            let flag = flag.clone();
            Box::pin(async move {
                tokio::time::sleep(Duration::from_secs(60)).await;
                flag.store(true, Ordering::SeqCst);
                Ok(vec![])
            })
        });
        let task = tokio::spawn(dispatch(
            handler,
            vec![],
            rx,
            tokio::time::Instant::now() + Duration::from_secs(60),
        ));
        tokio::task::yield_now().await;
        tx.send_replace(true);
        assert!(tokio::time::timeout(Duration::from_secs(1), task)
            .await
            .unwrap()
            .unwrap()
            .is_err());
        assert!(!finished.load(Ordering::SeqCst));
    }
    #[tokio::test]
    async fn expired_deadline_never_invokes_handler() {
        let (_tx, rx) = tokio::sync::watch::channel(false);
        let called = Arc::new(AtomicBool::new(false));
        let flag = called.clone();
        let handler: Handler = Arc::new(move |_| {
            flag.store(true, Ordering::SeqCst);
            Box::pin(async { Ok(vec![]) })
        });
        assert!(dispatch(handler, vec![], rx, tokio::time::Instant::now())
            .await
            .is_err());
        assert!(!called.load(Ordering::SeqCst));
    }
}
