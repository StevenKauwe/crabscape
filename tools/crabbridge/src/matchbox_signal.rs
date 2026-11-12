//! CrabScape's existing Matchbox wire protocol, not a new signaling service.
//! Signaling is untrusted metadata. Application secrets never belong in these types.
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};
use tokio::{
    sync::{mpsc, oneshot},
    time::Instant,
};
use tokio_tungstenite::{
    tungstenite::{protocol::WebSocketConfig, Message},
    MaybeTlsStream, WebSocketStream,
};
use webrtc::peer_connection::sdp::session_description::RTCSessionDescription;
pub const PRODUCTION: &str = "wss://crabscape-introducer.fly.dev";
pub const LOOPBACK: &str = "ws://127.0.0.1:3536";
const PREFIX: &str = "crabscape-crabbridge-";
const PROTOCOL: &str = "crabbridge-v1";
const MAX: usize = 65536;
const ENVELOPE: usize = MAX * 6 + 1024;
#[derive(Clone, Copy)]
pub enum Endpoint {
    Production,
    LoopbackTest,
}
impl Endpoint {
    pub fn url(self) -> &'static str {
        match self {
            Self::Production => PRODUCTION,
            Self::LoopbackTest => LOOPBACK,
        }
    }
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClientSignal {
    pub endpoint: String,
    pub room: String,
}
impl ClientSignal {
    pub fn validate(&self, mode: Endpoint) -> Result<(), String> {
        let suffix = self
            .room
            .strip_prefix(PREFIX)
            .ok_or("Invalid CrabBridge room")?;
        if self.endpoint != mode.url()
            || suffix.len() != 32
            || !suffix
                .bytes()
                .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
        {
            return Err("Invalid fixed Matchbox endpoint or room".into());
        }
        Ok(())
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
enum Event {
    IdAssigned(String),
    NewPeer(String),
    PeerLeft(String),
    Signal { sender: String, data: Payload },
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase", deny_unknown_fields)]
enum Payload {
    Ready { protocol: String },
    Offer { protocol: String, sdp: String },
    Answer { protocol: String, sdp: String },
}
#[derive(Serialize)]
enum Request {
    Signal { receiver: String, data: Payload },
    KeepAlive,
}
fn peer(id: &str) -> bool {
    id.len() == 36
        && id.bytes().enumerate().all(|(i, b)| {
            if [8, 13, 18, 23].contains(&i) {
                b == b'-'
            } else {
                b.is_ascii_hexdigit()
            }
        })
}
#[derive(Default)]
struct State {
    id: Option<String>,
    remote: Option<String>,
    offered: bool,
    events: usize,
}
enum Action {
    None,
    Ready(String),
    Offer(String),
}
impl State {
    fn accept(&mut self, text: &str) -> Result<Action, String> {
        self.events += 1;
        if self.events > 16 || text.len() > ENVELOPE {
            return Err("Signaling event budget exceeded".into());
        }
        let event: Event = serde_json::from_str(text).map_err(|_| "Invalid Matchbox schema")?;
        match event {
            Event::IdAssigned(id) => {
                if self.id.is_some() || !peer(&id) {
                    return Err("Invalid or duplicate assigned peer".into());
                }
                self.id = Some(id);
                Ok(Action::None)
            }
            Event::NewPeer(id) => {
                if self.id.is_none()
                    || self.remote.is_some()
                    || !peer(&id)
                    || self.id.as_ref() == Some(&id)
                {
                    return Err("Unexpected signaling peer".into());
                }
                self.remote = Some(id.clone());
                Ok(Action::Ready(id))
            }
            Event::PeerLeft(id) => {
                let _ = id;
                Err("Peer left; make fresh pairing".into())
            }
            Event::Signal { sender, data } => {
                if self.id.is_none()
                    || !peer(&sender)
                    || self.remote.as_ref() != Some(&sender)
                    || self.offered
                {
                    return Err("Unpinned sender or repeated signal".into());
                }
                let Payload::Offer { protocol, sdp } = data else {
                    return Err("Unexpected signaling role".into());
                };
                if protocol != PROTOCOL
                    || sdp.len() > MAX
                    || !sdp.starts_with("v=0\r\n")
                    || sdp.contains('\0')
                {
                    return Err("Invalid offer".into());
                }
                self.offered = true;
                Ok(Action::Offer(sdp))
            }
        }
    }
}
type Socket = WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>;
type Current = Arc<dyn Fn() -> bool + Send + Sync>;
fn live(expires: u64, deadline: Instant, current: &Current) -> Result<(), String> {
    if crate::unix_now() >= expires || Instant::now() >= deadline || !current() {
        Err("Pairing expired or revoked".into())
    } else {
        Ok(())
    }
}
async fn send(socket: &mut Socket, request: Request) -> Result<(), String> {
    let bytes = serde_json::to_string(&request).map_err(|_| "Signal encoding")?;
    if bytes.len() > ENVELOPE {
        return Err("Signal too large".into());
    }
    tokio::time::timeout(
        Duration::from_secs(3),
        socket.send(Message::Text(bytes.into())),
    )
    .await
    .map_err(|_| "Signal write timeout")?
    .map_err(|_| "Signal write failed".to_owned())
}
struct AnswerCommand {
    answer: RTCSessionDescription,
    result: oneshot::Sender<Result<(), String>>,
}
struct Owner {
    send: mpsc::Sender<AnswerCommand>,
    abort: tokio::task::AbortHandle,
    used: AtomicBool,
}
impl Drop for Owner {
    fn drop(&mut self) {
        self.abort.abort();
    }
}
/// Host joins first; browser waits for the ready Signal, pins that sender, then offers.
/// Endpoint mode is selected by trusted host CLI, never by an invitation alone.
pub async fn receive_offer(
    signal: &ClientSignal,
    mode: Endpoint,
    expires: u64,
    guard: Arc<crate::config_guard::ConfigGuard>,
) -> Result<
    (
        RTCSessionDescription,
        harbor_transport_spike::AnswerPublisher,
    ),
    String,
> {
    receive(signal, mode, expires, Arc::new(move || guard.is_current())).await
}
async fn receive(
    signal: &ClientSignal,
    mode: Endpoint,
    expires: u64,
    current: Current,
) -> Result<
    (
        RTCSessionDescription,
        harbor_transport_spike::AnswerPublisher,
    ),
    String,
> {
    signal.validate(mode)?;
    let remaining = expires
        .checked_sub(crate::unix_now())
        .filter(|n| *n > 0 && *n <= 600)
        .ok_or("Invalid signaling lifetime")?;
    let deadline = Instant::now() + Duration::from_secs(remaining);
    live(expires, deadline, &current)?;
    let config = WebSocketConfig::default()
        .read_buffer_size(4096)
        .write_buffer_size(0)
        .max_write_buffer_size(ENVELOPE + 1024)
        .max_message_size(Some(ENVELOPE))
        .max_frame_size(Some(ENVELOPE));
    // connect_async performs no redirects, proxies or reconnection. Fixed WSS uses WebPKI roots.
    let connecting = tokio_tungstenite::connect_async_with_config(
        format!("{}/{}", mode.url(), signal.room),
        Some(config),
        true,
    );
    let (mut socket, _) = tokio::time::timeout_at(
        deadline.min(Instant::now() + Duration::from_secs(10)),
        connecting,
    )
    .await
    .map_err(|_| "Signaling connection timeout")?
    .map_err(|_| "Signaling connection failed")?;
    live(expires, deadline, &current)?;
    let mut state = State::default();
    let mut tick = tokio::time::interval(Duration::from_millis(500));
    let mut keepalive = tokio::time::interval(Duration::from_secs(10));
    let mut frames = 0;
    let offer = loop {
        live(expires, deadline, &current)?;
        tokio::select! {biased;
         _=tick.tick()=>{live(expires,deadline,&current)?;},
         _=keepalive.tick()=>{send(&mut socket,Request::KeepAlive).await?;},
         message=socket.next()=>{
          frames+=1;if frames>128{return Err("Signaling frame budget exceeded".into());}
          let message=message.ok_or("Signaling closed")?.map_err(|_|"Signaling frame failed")?;
          match message {
           Message::Text(text)=>match state.accept(&text)?{
            Action::None=>{println!("CrabScape rendezvous ready; waiting for browser.");},Action::Ready(receiver)=>send(&mut socket,Request::Signal{receiver,data:Payload::Ready{protocol:PROTOCOL.into()}}).await?,
            Action::Offer(sdp)=>break RTCSessionDescription::offer(sdp).map_err(|_|"Invalid offer SDP")?,
           },Message::Ping(_)=>{tokio::time::timeout(Duration::from_secs(3),socket.flush()).await.map_err(|_|"Signal ping timeout")?.map_err(|_|"Signal ping failed")?;},Message::Pong(_)=>{},_=>return Err("Unsupported or closed signaling frame".into())
          }
         }
        }
    };
    live(expires, deadline, &current)?;
    let receiver = state.remote.ok_or("Missing peer")?;
    let (tx, mut rx) = mpsc::channel::<AnswerCommand>(1);
    let actor = tokio::spawn(async move {
        let mut tick = tokio::time::interval(Duration::from_millis(500));
        let mut frames = 0usize;
        loop {
            if live(expires, deadline, &current).is_err() {
                break;
            }
            tokio::select! {biased;
             _=tick.tick()=>{},
             event=socket.next()=>{frames+=1;if frames>128{break;}match event{Some(Ok(Message::Ping(_)))=>{if !matches!(tokio::time::timeout(Duration::from_secs(3),socket.flush()).await,Ok(Ok(()))){break;}},Some(Ok(Message::Pong(_)))=>{},_=>break}},
             command=rx.recv()=>{
              let Some(command)=command else{break;};
              let result=async{
               live(expires,deadline,&current)?;
               if command.answer.sdp.len()>MAX||command.answer.sdp.contains('\0')||!command.answer.sdp.starts_with("v=0\r\n"){return Err("Invalid answer size or content".into());}
               send(&mut socket,Request::Signal{receiver:receiver.clone(),data:Payload::Answer{protocol:PROTOCOL.into(),sdp:command.answer.sdp}}).await?;
               live(expires,deadline,&current)?;Ok(())
              }.await;
              let _=command.result.send(result);break;
             }
            }
        }
        let _ = tokio::time::timeout(Duration::from_secs(1), socket.close(None)).await;
    });
    let owner = Arc::new(Owner {
        send: tx,
        abort: actor.abort_handle(),
        used: AtomicBool::new(false),
    });
    let publisher: harbor_transport_spike::AnswerPublisher = Arc::new(move |answer| {
        let owner = owner.clone();
        Box::pin(async move {
            if owner.used.swap(true, Ordering::SeqCst) {
                return Err("Answer already submitted; make fresh pairing".into());
            }
            let (result, receive) = oneshot::channel();
            owner
                .send
                .try_send(AnswerCommand { answer, result })
                .map_err(|_| "Signaling session closed")?;
            tokio::time::timeout(Duration::from_secs(4), receive)
                .await
                .map_err(|_| "Answer delivery timeout")?
                .map_err(|_| "Signaling ended before delivery")?
        })
    });
    Ok((offer, publisher))
}
#[cfg(test)]
mod tests {
    use super::*;
    static FIXTURE_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
    const HOST_ID: &str = "11111111-1111-4111-8111-111111111111";
    const CLIENT_ID: &str = "22222222-2222-4222-8222-222222222222";
    const SDP: &str = "v=0\r\no=- 1 1 IN IP4 127.0.0.1\r\ns=-\r\nt=0 0\r\n";
    fn signal() -> ClientSignal {
        ClientSignal {
            endpoint: LOOPBACK.into(),
            room: format!("{PREFIX}{}", "01".repeat(16)),
        }
    }
    fn setup() -> State {
        let mut s = State::default();
        s.accept(&serde_json::json!({"IdAssigned":HOST_ID}).to_string())
            .unwrap();
        s.accept(&serde_json::json!({"NewPeer":CLIENT_ID}).to_string())
            .unwrap();
        s
    }
    fn offer(sender: &str) -> String {
        serde_json::json!({"Signal":{"sender":sender,"data":{"protocol":PROTOCOL,"type":"offer","sdp":SDP}}}).to_string()
    }
    #[test]
    fn endpoint_and_room_are_host_scoped() {
        let s = signal();
        assert!(s.validate(Endpoint::LoopbackTest).is_ok());
        assert!(s.validate(Endpoint::Production).is_err());
        for room in [
            format!("{PREFIX}{}", "a".repeat(64)),
            "crabscape-game".into(),
            format!("{PREFIX}{}", "/".repeat(32)),
        ] {
            assert!(ClientSignal { room, ..signal() }
                .validate(Endpoint::LoopbackTest)
                .is_err());
        }
        for endpoint in [
            "ws://localhost:3536",
            "ws://127.0.0.1:1",
            "https://evil.invalid",
        ] {
            assert!(ClientSignal {
                endpoint: endpoint.into(),
                ..signal()
            }
            .validate(Endpoint::LoopbackTest)
            .is_err());
        }
    }
    #[test]
    fn duplicate_identity_peer_and_offer_fail() {
        let mut s = setup();
        assert!(s
            .accept(&serde_json::json!({"IdAssigned":HOST_ID}).to_string())
            .is_err());
        assert!(s
            .accept(&serde_json::json!({"NewPeer":CLIENT_ID}).to_string())
            .is_err());
        let mut s = setup();
        assert!(matches!(
            s.accept(&offer(CLIENT_ID)).unwrap(),
            Action::Offer(_)
        ));
        assert!(s.accept(&offer(CLIENT_ID)).is_err());
    }
    #[test]
    fn wrong_sender_roles_and_unknown_fields_fail() {
        for text in [offer(HOST_ID),serde_json::json!({"Signal":{"sender":CLIENT_ID,"data":{"protocol":PROTOCOL,"type":"answer","sdp":SDP}}}).to_string(),serde_json::json!({"Signal":{"sender":CLIENT_ID,"data":{"protocol":"other","type":"offer","sdp":SDP}}}).to_string(),serde_json::json!({"Signal":{"sender":CLIENT_ID,"data":{"protocol":PROTOCOL,"type":"offer","sdp":SDP,"password":"notallowed"}}}).to_string(),serde_json::json!({"Signal":{"sender":CLIENT_ID,"data":{"protocol":PROTOCOL,"type":"offer","sdp":SDP},"extra":true}}).to_string()]{assert!(setup().accept(&text).is_err());}
    }
    #[test]
    fn browser_first_unknown_churn_and_size_fail() {
        let mut s = State::default();
        assert!(s.accept(&offer(CLIENT_ID)).is_err());
        assert!(setup()
            .accept(&serde_json::json!({"PeerLeft":CLIENT_ID}).to_string())
            .is_err());
        assert!(setup().accept("{\"Unknown\":true}").is_err());
        let mut s = setup();
        s.events = 16;
        assert!(s.accept(&offer(CLIENT_ID)).is_err());
        let oversized=serde_json::json!({"Signal":{"sender":CLIENT_ID,"data":{"protocol":PROTOCOL,"type":"offer","sdp":format!("v=0\r\n{}","é".repeat(MAX))}}}).to_string();
        assert!(setup().accept(&oversized).is_err());
    }
    #[test]
    fn lifetime_and_revocation_guard_fail_closed() {
        let current: Current = Arc::new(|| false);
        assert!(live(
            crate::unix_now() + 60,
            Instant::now() + Duration::from_secs(60),
            &current
        )
        .is_err());
        let current: Current = Arc::new(|| true);
        assert!(live(
            crate::unix_now(),
            Instant::now() + Duration::from_secs(60),
            &current
        )
        .is_err());
        assert!(live(crate::unix_now() + 60, Instant::now(), &current).is_err());
    }
    #[tokio::test]
    async fn local_matchbox_fixture_roundtrip_and_single_use_answer() {
        let _fixture_guard = FIXTURE_LOCK.lock().await;
        // Explicit loopback test mode only. No production service contact.
        let listener = tokio::net::TcpListener::bind("127.0.0.1:3536")
            .await
            .expect("fixture port occupied: do not kill other listener");
        let fixture = tokio::spawn(async move {
            let (tcp, _) = listener.accept().await.unwrap();
            let mut ws = tokio_tungstenite::accept_async(tcp).await.unwrap();
            ws.send(Message::Text(
                serde_json::json!({"IdAssigned":HOST_ID}).to_string().into(),
            ))
            .await
            .unwrap();
            ws.send(Message::Text(
                serde_json::json!({"NewPeer":CLIENT_ID}).to_string().into(),
            ))
            .await
            .unwrap();
            let mut capture = Vec::new();
            loop {
                let text = ws.next().await.unwrap().unwrap().into_text().unwrap();
                let v: serde_json::Value = serde_json::from_str(&text).unwrap();
                capture.push(v.clone());
                if v.get("Signal").is_some() {
                    assert_eq!(v["Signal"]["receiver"], CLIENT_ID);
                    assert_eq!(v["Signal"]["data"]["type"], "ready");
                    break;
                }
            }
            ws.send(Message::Text(offer(CLIENT_ID).into()))
                .await
                .unwrap();
            loop {
                let message = ws.next().await.unwrap().unwrap();
                if let Message::Text(text) = message {
                    let v: serde_json::Value = serde_json::from_str(&text).unwrap();
                    capture.push(v.clone());
                    if v.get("Signal").is_some() {
                        assert_eq!(v["Signal"]["data"]["type"], "answer");
                        assert_eq!(v["Signal"]["data"]["protocol"], PROTOCOL);
                        break;
                    }
                }
            }
            // Closure after a complete answer does not retract the independently returned SDP.
            let _ = ws.close(None).await;
            capture
        });
        let (received, publisher) = receive(
            &signal(),
            Endpoint::LoopbackTest,
            crate::unix_now() + 60,
            Arc::new(|| true),
        )
        .await
        .unwrap();
        assert_eq!(received.sdp, SDP);
        publisher(RTCSessionDescription::answer(SDP.into()).unwrap())
            .await
            .unwrap();
        assert!(
            publisher(RTCSessionDescription::answer(SDP.into()).unwrap())
                .await
                .is_err()
        );
        let capture = tokio::time::timeout(Duration::from_secs(3), fixture)
            .await
            .unwrap()
            .unwrap();
        let text = serde_json::to_string(&capture).unwrap();
        for forbidden in ["password", "key", "principal", "token", "terminal"] {
            assert!(!text.contains(forbidden));
        }
    }
    #[tokio::test]
    async fn rejected_frames_revocation_and_expiry_close_waiting_session() {
        let _fixture_guard = FIXTURE_LOCK.lock().await;
        for case in ["binary", "oversized", "revoked", "expired"] {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:3536")
                .await
                .expect("fixture port occupied");
            let current = Arc::new(AtomicBool::new(true));
            let server_current = current.clone();
            let fixture = tokio::spawn(async move {
                let (tcp, _) = listener.accept().await.unwrap();
                let mut ws = tokio_tungstenite::accept_async(tcp).await.unwrap();
                ws.send(Message::Text(
                    serde_json::json!({"IdAssigned":HOST_ID}).to_string().into(),
                ))
                .await
                .unwrap();
                match case {
                    "binary" => {
                        let _ = ws.send(Message::Binary(vec![0; 16].into())).await;
                    }
                    "oversized" => {
                        let _ = ws
                            .send(Message::Text("x".repeat(ENVELOPE + 1).into()))
                            .await;
                    }
                    "revoked" => {
                        server_current.store(false, Ordering::SeqCst);
                    }
                    _ => {}
                }
                // Keep the fixture alive so expiry/revocation, not an early close, ends it.
                std::future::pending::<()>().await;
            });
            let expires = crate::unix_now() + if case == "expired" { 1 } else { 60 };
            let result = tokio::time::timeout(
                Duration::from_secs(3),
                receive(
                    &signal(),
                    Endpoint::LoopbackTest,
                    expires,
                    Arc::new(move || current.load(Ordering::SeqCst)),
                ),
            )
            .await;
            fixture.abort();
            let _ = fixture.await;
            assert!(
                result.expect("bounded signaling rejection").is_err(),
                "{case}"
            );
        }
    }
    #[tokio::test]
    async fn post_offer_drop_revoke_and_expiry_close_without_answer() {
        let _fixture_guard = FIXTURE_LOCK.lock().await;
        for case in ["drop", "revoke", "expire"] {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:3536")
                .await
                .expect("fixture port occupied");
            let fixture = tokio::spawn(async move {
                let (tcp, _) = listener.accept().await.unwrap();
                let mut ws = tokio_tungstenite::accept_async(tcp).await.unwrap();
                ws.send(Message::Text(
                    serde_json::json!({"IdAssigned":HOST_ID}).to_string().into(),
                ))
                .await
                .unwrap();
                ws.send(Message::Text(
                    serde_json::json!({"NewPeer":CLIENT_ID}).to_string().into(),
                ))
                .await
                .unwrap();
                loop {
                    let text = ws.next().await.unwrap().unwrap().into_text().unwrap();
                    let value: serde_json::Value = serde_json::from_str(&text).unwrap();
                    if value.get("Signal").is_some() {
                        assert_eq!(value["Signal"]["data"]["type"], "ready");
                        break;
                    }
                }
                ws.send(Message::Text(offer(CLIENT_ID).into()))
                    .await
                    .unwrap();
                let mut answers = 0;
                // EOF, reset, and WebSocket Close all prove the actor released its socket.
                while let Some(Ok(message)) = ws.next().await {
                    match message {
                        Message::Text(text) => {
                            let value: serde_json::Value = serde_json::from_str(&text).unwrap();
                            if value["Signal"]["data"]["type"] == "answer" {
                                answers += 1;
                            }
                        }
                        Message::Close(_) => break,
                        _ => {}
                    }
                }
                answers
            });
            let current = Arc::new(AtomicBool::new(true));
            let check = current.clone();
            let expires = crate::unix_now() + if case == "expire" { 2 } else { 60 };
            let (received, publisher) = receive(
                &signal(),
                Endpoint::LoopbackTest,
                expires,
                Arc::new(move || check.load(Ordering::SeqCst)),
            )
            .await
            .unwrap();
            assert_eq!(received.sdp, SDP);
            if case == "drop" {
                drop(publisher);
            } else {
                if case == "revoke" {
                    current.store(false, Ordering::SeqCst);
                } else {
                    tokio::time::timeout(Duration::from_secs(3), async {
                        while crate::unix_now() < expires {
                            tokio::time::sleep(Duration::from_millis(20)).await;
                        }
                    })
                    .await
                    .unwrap();
                }
                assert!(
                    publisher(RTCSessionDescription::answer(SDP.into()).unwrap())
                        .await
                        .is_err(),
                    "{case}"
                );
                drop(publisher);
            }
            assert_eq!(
                tokio::time::timeout(Duration::from_secs(3), fixture)
                    .await
                    .expect("post-offer socket must close")
                    .unwrap(),
                0,
                "{case}"
            );
        }
    }
}
