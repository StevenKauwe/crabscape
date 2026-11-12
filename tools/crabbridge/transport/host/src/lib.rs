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
    peer_connection::sdp::session_description::RTCSessionDescription,
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
/// Explicit network policy. Loopback and the narrow LAN pilot use no ICE servers;
/// DirectStun separately opts into fixed public STUN and standard broad UDP sockets.
pub async fn serve_offer_published_with_policy(
    key: [u8; 32],
    context: SessionContext,
    offer: RTCSessionDescription,
    lifetime: Duration,
    handler: Handler,
    publish: AnswerPublisher,
    policy: NetworkPolicy,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    serve_offer_published_with_shutdown(
        key,
        context,
        offer,
        lifetime,
        handler,
        publish,
        policy,
        Arc::new(std::sync::atomic::AtomicBool::new(false)),
    )
    .await
}
/// Caller requests shutdown by setting the flag, then MUST await this function.
/// Unlike dropping its future, this executes the peer's graceful close path.
#[allow(clippy::too_many_arguments)]
pub async fn serve_offer_published_with_shutdown(
    key: [u8; 32],
    context: SessionContext,
    offer: RTCSessionDescription,
    lifetime: Duration,
    handler: Handler,
    publish: AnswerPublisher,
    policy: NetworkPolicy,
    shutdown: Arc<std::sync::atomic::AtomicBool>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    if shutdown.load(std::sync::atomic::Ordering::Acquire) {
        return Err("Shutdown requested".into());
    }
    context.validate()?;
    policy.validate_context(&context)?;
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
    // mDNS remains disabled for every policy. Only explicit DirectStun enables
    // the fixed STUN servers; no policy configures TURN.
    let mut settings = SettingEngine::default();
    settings.set_ice_multicast_dns_mode(webrtc::ice::mdns::MulticastDnsMode::Disabled);
    let network_cleanup = network::configure(policy, &mut settings).await?;
    let api = APIBuilder::new().with_setting_engine(settings).build();
    let pc = Arc::new(api.new_peer_connection(policy.rtc_configuration()).await?);
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
    pc.on_peer_connection_state_change(Box::new(move |state|{let ended=state_ended.clone();let budget=state_budget.clone();Box::pin(async move{if diagnostics && budget.fetch_add(1,std::sync::atomic::Ordering::Relaxed)<32 {eprintln!("harbor-lan t_ms={} PC state={state:?}", diagnostics_started.elapsed().as_millis());}
if matches!(state,webrtc::peer_connection::peer_connection_state::RTCPeerConnectionState::Closed|webrtc::peer_connection::peer_connection_state::RTCPeerConnectionState::Failed|webrtc::peer_connection::peer_connection_state::RTCPeerConnectionState::Disconnected){ended.send_replace(true);}})}));
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
        let mut answer = pc.local_description().await.ok_or("Missing answer")?;
        answer.sdp = policy.public_description(&answer.sdp)?;
        tokio::time::timeout_at(deadline, publish(answer))
            .await?
            .map_err(std::io::Error::other)?;

        Ok::<(), Box<dyn std::error::Error + Send + Sync>>(())
    };
    let setup = tokio::select! {biased;
        _=wait_shutdown(&shutdown)=>Err("Shutdown requested".into()),
        _=end_rx.wait_for(|closed|*closed)=>Err("Disconnected during setup".into()),
        _=tokio::time::sleep_until(deadline)=>Err("Invitation expired".into()),
        result=setup=>result,
    };
    if let Err(error) = setup {
        ended_signal.send_replace(true);
        shutdown.store(true, std::sync::atomic::Ordering::Release);
        let _ = close_then_cleanup(
            pc.close(),
            async {
                if let Some(cleanup) = &network_cleanup {
                    cleanup.finish().await;
                }
            },
            Duration::from_secs(5),
        )
        .await;
        return Err(error);
    }

    tokio::select! {biased; _=wait_shutdown(&shutdown)=>{},_=end_rx.wait_for(|closed|*closed)=>{},_=tokio::signal::ctrl_c()=>{},_=tokio::time::sleep_until(deadline)=>{}}
    ended_signal.send_replace(true);
    shutdown.store(true, std::sync::atomic::Ordering::Release);
    let close_result = close_then_cleanup(
        pc.close(),
        async {
            if let Some(cleanup) = &network_cleanup {
                cleanup.finish().await;
            }
        },
        Duration::from_secs(5),
    )
    .await;
    close_result??;
    Ok(())
}

async fn close_then_cleanup<C, F, T>(
    close: C,
    cleanup: F,
    bound: Duration,
) -> Result<T, tokio::time::error::Elapsed>
where
    C: Future<Output = T>,
    F: Future<Output = ()>,
{
    let result = tokio::time::timeout(bound, close).await;
    cleanup.await;
    result
}

async fn wait_shutdown(flag: &std::sync::atomic::AtomicBool) {
    loop {
        if flag.load(std::sync::atomic::Ordering::Acquire) {
            return;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
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
    #[tokio::test]
    async fn explicit_shutdown_runs_owned_cleanup_not_future_abort() {
        let flag = Arc::new(AtomicBool::new(false));
        let taskflag = flag.clone();
        let cleaned = Arc::new(AtomicBool::new(false));
        let result = cleaned.clone();
        let task = tokio::spawn(async move {
            tokio::select! {biased;_=wait_shutdown(&taskflag)=>{},_=std::future::pending::<()>()=>{}}
            // This continuation is exactly what external dropping would skip.
            result.store(true, Ordering::Release);
        });
        flag.store(true, Ordering::Release);
        tokio::time::timeout(Duration::from_millis(300), task)
            .await
            .unwrap()
            .unwrap();
        assert!(cleaned.load(Ordering::Acquire));
        tokio::time::timeout(Duration::from_millis(20), wait_shutdown(&flag))
            .await
            .unwrap();
    }
    #[tokio::test]
    async fn graceful_close_precedes_socket_cleanup_even_on_timeout() {
        let order = Arc::new(std::sync::Mutex::new(Vec::new()));
        let a = order.clone();
        let b = order.clone();
        close_then_cleanup(
            async move {
                a.lock().unwrap().push("close");
            },
            async move {
                b.lock().unwrap().push("cleanup");
            },
            Duration::from_millis(50),
        )
        .await
        .unwrap();
        assert_eq!(*order.lock().unwrap(), vec!["close", "cleanup"]);
        let cleaned = Arc::new(AtomicBool::new(false));
        let result = cleaned.clone();
        assert!(close_then_cleanup(
            std::future::pending::<()>(),
            async move {
                result.store(true, Ordering::Release);
            },
            Duration::from_millis(1)
        )
        .await
        .is_err());
        assert!(cleaned.load(Ordering::Acquire));
    }
}
