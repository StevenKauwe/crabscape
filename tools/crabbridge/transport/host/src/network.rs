//! Deliberately narrow opt-in for the two specifically authorized LAN test devices.
use std::{
    net::{IpAddr, Ipv4Addr, SocketAddr},
    sync::Arc,
};
use tokio::{net::UdpSocket, sync::watch};
use webrtc::{
    ice::{
        network_type::NetworkType,
        udp_mux::{UDPMux, UDPMuxDefault, UDPMuxParams},
        udp_network::UDPNetwork,
    },
    util::{Conn, Result},
};
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum NetworkPolicy {
    #[default]
    Loopback,
    ApprovedLanPilot,
    /// Explicit public-network opt-in. Standard ICE sockets, NOT an IP firewall.
    DirectStun,
}
pub const DIRECT_STUN_URLS: [&str; 2] = [
    "stun:stun.l.google.com:19302",
    "stun:stun1.l.google.com:19302",
];
fn global_v4(ip: Ipv4Addr) -> bool {
    let [a, b, c, _] = ip.octets();
    !(a == 0
        || a == 10
        || a == 127
        || a >= 224
        || (a == 100 && (64..=127).contains(&b))
        || (a == 169 && b == 254)
        || (a == 172 && (16..=31).contains(&b))
        || (a == 192 && (b == 168 || (b == 0 && (c == 0 || c == 2)) || (b == 88 && c == 99)))
        || (a == 198 && (b == 18 || b == 19 || (b == 51 && c == 100)))
        || (a == 203 && b == 0 && c == 113))
}
fn direct_candidate(line: &str) -> std::result::Result<(), String> {
    if line.len() > 1024 {
        return Err("Candidate too large".into());
    }
    let raw = line.strip_prefix("a=candidate:").ok_or("Not a candidate")?;
    let p: Vec<_> = raw.split_whitespace().collect();
    if p.len() < 8
        || p.len() > 32
        || p[0].is_empty()
        || p[0].len() > 32
        || !p[0].bytes().all(|b| b.is_ascii_alphanumeric())
        || p[1] != "1"
        || !p[2].eq_ignore_ascii_case("udp")
        || p[3].parse::<u32>().is_err()
        || !p[4].parse::<Ipv4Addr>().is_ok_and(global_v4)
        || !p[5].parse::<u16>().is_ok_and(|p| p >= 1024)
        || p[6] != "typ"
        || !matches!(p[7], "host" | "srflx")
        || (p.len() - 8) % 2 != 0
    {
        return Err("Direct STUN requires public numeric UDP4 host/srflx candidates".into());
    }
    let mut seen = std::collections::HashSet::new();
    for pair in p[8..].as_chunks::<2>().0 {
        if !seen.insert(pair[0]) {
            return Err("Duplicate candidate extension".into());
        }
        let valid = match pair[0] {
            "raddr" => pair[1].parse::<Ipv4Addr>().is_ok(),
            "rport" => pair[1].parse::<u16>().is_ok(),
            "generation" | "network-id" | "network-cost" => pair[1].parse::<u32>().is_ok(),
            "ufrag" => {
                pair[1].len() <= 256
                    && pair[1]
                        .bytes()
                        .all(|c| c.is_ascii_alphanumeric() || b"+/".contains(&c))
            }
            _ => false,
        };
        if !valid {
            return Err("Unsupported candidate extension".into());
        }
    }
    Ok(())
}
fn validate_direct_sdp(sdp: &str) -> std::result::Result<(), String> {
    if sdp.len() > 65536 {
        return Err("SDP too large".into());
    }
    let mut count = 0;
    for line in sdp.lines() {
        if line.len() > 2048 {
            return Err("SDP line too large".into());
        }
        if line.starts_with("a=candidate:") {
            direct_candidate(line)?;
            count += 1;
        }
    }
    if count == 0 || count > 16 {
        return Err("Direct STUN requires 1..16 public candidates".into());
    }
    Ok(())
}
const LOCAL: Ipv4Addr = Ipv4Addr::new(192, 168, 1, 147);
const PEER: Ipv4Addr = Ipv4Addr::new(192, 168, 1, 198);
fn permitted(addr: SocketAddr) -> bool {
    addr.ip() == IpAddr::V4(PEER) && addr.port() != 0
}
fn denied() -> webrtc::util::Error {
    std::io::Error::new(
        std::io::ErrorKind::PermissionDenied,
        "Outside approved LAN peer",
    )
    .into()
}
impl NetworkPolicy {
    pub fn validate_context(
        self,
        context: &harbor_noise::SessionContext,
    ) -> std::result::Result<(), String> {
        let expected = if self == Self::DirectStun {
            Some(harbor_noise::NetworkProfile::DirectStunV1)
        } else {
            None
        };
        if context.network_profile != expected {
            return Err("Network policy does not match authenticated context".into());
        }
        Ok(())
    }
    pub fn rtc_configuration(self) -> webrtc::peer_connection::configuration::RTCConfiguration {
        use webrtc::{
            ice_transport::ice_server::RTCIceServer,
            peer_connection::configuration::RTCConfiguration,
        };
        if self == Self::DirectStun {
            RTCConfiguration {
                ice_servers: vec![RTCIceServer {
                    urls: DIRECT_STUN_URLS.iter().map(|s| s.to_string()).collect(),
                    ..Default::default()
                }],
                ..Default::default()
            }
        } else {
            RTCConfiguration::default()
        }
    }
    /// Do not publish private/mDNS/IPv6 candidates in direct Internet mode.
    /// This is SDP hygiene, not a filter on actual inbound ICE packets.
    pub fn public_description(self, sdp: &str) -> std::result::Result<String, String> {
        if self != Self::DirectStun {
            return Ok(sdp.into());
        }
        let filtered = sdp
            .lines()
            .filter(|line| !line.starts_with("a=candidate:") || direct_candidate(line).is_ok())
            .collect::<Vec<_>>()
            .join("\r\n")
            + "\r\n";
        validate_direct_sdp(&filtered)?;
        Ok(filtered)
    }
    fn ice_failed_timeout(self) -> Option<std::time::Duration> {
        match self {
            Self::Loopback | Self::DirectStun => None,
            Self::ApprovedLanPilot => Some(std::time::Duration::from_secs(120)),
        }
    }
    pub fn validate_offer(self, sdp: &str) -> std::result::Result<(), String> {
        if self == Self::Loopback {
            return Ok(());
        }
        if self == Self::DirectStun {
            return validate_direct_sdp(sdp);
        }
        let mut count = 0;
        for line in sdp.lines() {
            if let Some(candidate) = line.strip_prefix("a=candidate:") {
                let p: Vec<_> = candidate.split_whitespace().collect();
                if p.len() < 8
                    || p[1] != "1"
                    || !p[2].eq_ignore_ascii_case("udp")
                    || p[4] != "192.168.1.198"
                    || p[5].parse::<u16>().ok().filter(|p| *p > 0).is_none()
                    || p[6] != "typ"
                    || p[7] != "host"
                    || p[8..]
                        .iter()
                        .any(|p| matches!(*p, "raddr" | "rport" | "tcptype"))
                {
                    return Err("LAN pilot requires only approved peer UDP4 host candidates".into());
                }
                count += 1;
            }
        }
        if count == 0 || count > 16 {
            return Err("LAN pilot requires 1..16 approved candidates".into());
        }
        Ok(())
    }
}
pub(crate) fn diagnostics_enabled(policy: NetworkPolicy) -> bool {
    policy == NetworkPolicy::ApprovedLanPilot
        && std::env::var("HARBOR_LAN_DIAGNOSTICS").as_deref() == Ok("1")
}
struct Diagnostics {
    started: std::time::Instant,
    counts: [std::sync::atomic::AtomicU64; 4],
    reported: std::sync::atomic::AtomicBool,
}
impl Default for Diagnostics {
    fn default() -> Self {
        Self {
            started: std::time::Instant::now(),
            counts: Default::default(),
            reported: Default::default(),
        }
    }
}
impl Diagnostics {
    fn record(&self, index: usize) {
        use std::sync::atomic::Ordering;
        let previous = self.counts[index]
            .try_update(Ordering::Relaxed, Ordering::Relaxed, |n| {
                Some(n.saturating_add(1))
            })
            .unwrap();
        let n = previous.saturating_add(1);
        if n <= 1024 && n.is_power_of_two() {
            eprintln!(
                "harbor-lan t_ms={} io {}={}",
                self.started.elapsed().as_millis(),
                ["sent", "accepted_recv", "rejected_source", "io_error"][index],
                n
            );
        }
    }
    fn summary(&self) {
        use std::sync::atomic::Ordering;
        if !self.reported.swap(true, Ordering::Relaxed) {
            eprintln!(
                "harbor-lan t_ms={} final sent={} accepted_recv={} rejected_source={} io_error={}",
                self.started.elapsed().as_millis(),
                self.counts[0].load(Ordering::Relaxed),
                self.counts[1].load(Ordering::Relaxed),
                self.counts[2].load(Ordering::Relaxed),
                self.counts[3].load(Ordering::Relaxed)
            );
        }
    }
}
type SocketOwner = Arc<std::sync::Mutex<Option<Arc<dyn Conn + Send + Sync>>>>;
struct FilteredSocket {
    diagnostics: Option<Arc<Diagnostics>>,
    socket: SocketOwner,
    closed: watch::Sender<bool>,
}
#[async_trait::async_trait]
impl Conn for FilteredSocket {
    async fn connect(&self, _: SocketAddr) -> Result<()> {
        Err(denied())
    }
    async fn recv(&self, buf: &mut [u8]) -> Result<usize> {
        self.recv_from(buf).await.map(|(n, _)| n)
    }
    async fn recv_from(&self, buf: &mut [u8]) -> Result<(usize, SocketAddr)> {
        let mut closed = self.closed.subscribe();
        let socket = self
            .socket
            .lock()
            .map_err(|_| denied())?
            .clone()
            .ok_or_else(denied)?;
        loop {
            let result = tokio::select! {biased;_=closed.wait_for(|v|*v)=>return Err(denied()),r=socket.recv_from(buf)=>match r {Ok(value)=>value,Err(error)=>{if let Some(d)=&self.diagnostics {d.record(3);}return Err(error);}}};
            if permitted(result.1) {
                if let Some(d) = &self.diagnostics {
                    d.record(1);
                }
                return Ok(result);
            }
            if let Some(d) = &self.diagnostics {
                d.record(2);
            }
            tokio::task::yield_now().await;
        }
    }
    async fn send(&self, _: &[u8]) -> Result<usize> {
        Err(denied())
    }
    async fn send_to(&self, buf: &[u8], target: SocketAddr) -> Result<usize> {
        if !permitted(target) || *self.closed.borrow() {
            return Err(denied());
        }
        let socket = self
            .socket
            .lock()
            .map_err(|_| denied())?
            .clone()
            .ok_or_else(denied)?;
        let mut closed = self.closed.subscribe();
        tokio::select! {biased; _=closed.wait_for(|v|*v)=>Err(denied()), result=socket.send_to(buf,target)=>{if let Some(d)=&self.diagnostics {d.record(if result.is_ok(){0}else{3});}result}}
    }
    fn local_addr(&self) -> Result<SocketAddr> {
        self.socket
            .lock()
            .map_err(|_| denied())?
            .as_ref()
            .ok_or_else(denied)?
            .local_addr()
    }
    fn remote_addr(&self) -> Option<SocketAddr> {
        None
    }
    async fn close(&self) -> Result<()> {
        self.closed.send_replace(true);
        if let Ok(mut socket) = self.socket.lock() {
            socket.take();
        }
        Ok(())
    }
    fn as_any(&self) -> &(dyn std::any::Any + Send + Sync) {
        self
    }
}
pub(crate) struct Cleanup {
    diagnostics: Option<Arc<Diagnostics>>,
    socket: SocketOwner,
    mux: Arc<UDPMuxDefault>,
    closed: watch::Sender<bool>,
}
impl Cleanup {
    pub(crate) async fn finish(&self) {
        if let Some(d) = &self.diagnostics {
            d.summary();
        }
        self.closed.send_replace(true);
        if let Ok(mut socket) = self.socket.lock() {
            socket.take();
        }
        let _ = self.mux.close().await;
    }
}
impl Drop for Cleanup {
    fn drop(&mut self) {
        if let Some(d) = &self.diagnostics {
            d.summary();
        }
        self.closed.send_replace(true);
        if let Ok(mut socket) = self.socket.lock() {
            socket.take();
        }
        let mux = self.mux.clone();
        tokio::spawn(async move {
            let _ = mux.close().await;
        });
    }
}
pub(crate) async fn configure(
    policy: NetworkPolicy,
    settings: &mut webrtc::api::setting_engine::SettingEngine,
) -> Result<Option<Cleanup>> {
    match policy {
        NetworkPolicy::Loopback => {
            settings.set_include_loopback_candidate(true);
            settings.set_ip_filter(Box::new(|ip| ip.is_loopback()));
            Ok(None)
        }
        NetworkPolicy::DirectStun => {
            settings.set_network_types(vec![NetworkType::Udp4]);
            settings.set_include_loopback_candidate(false);
            // Maintained native ICE creates broad ephemeral UDP for srflx.
            // No claim of datagram filtering; permission/limits are separate.
            Ok(None)
        }
        NetworkPolicy::ApprovedLanPilot => {
            // Manual private answer transfer can exceed ICE's default 25s failure stage.
            // Keep disconnected/keepalive defaults; the outer grant deadline still wins.
            settings.set_ice_timeouts(None, policy.ice_failed_timeout(), None);
            let diagnostics = diagnostics_enabled(policy).then(|| Arc::new(Diagnostics::default()));
            let socket = Arc::new(UdpSocket::bind(SocketAddr::new(IpAddr::V4(LOCAL), 0)).await?);
            let socket: SocketOwner = Arc::new(std::sync::Mutex::new(Some(socket)));
            let (closed, _) = watch::channel(false);
            let mux = UDPMuxDefault::new(UDPMuxParams::new(FilteredSocket {
                diagnostics: diagnostics.clone(),
                socket: socket.clone(),
                closed: closed.clone(),
            }));
            settings.set_network_types(vec![NetworkType::Udp4]);
            settings.set_include_loopback_candidate(false);
            settings.set_ip_filter(Box::new(|ip| ip == IpAddr::V4(LOCAL)));
            settings.set_udp_network(UDPNetwork::Muxed(mux.clone()));
            Ok(Some(Cleanup {
                diagnostics,
                mux,
                closed,
                socket,
            }))
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn offer(address: &str, transport: &str, kind: &str) -> String {
        format!("v=0\r\na=candidate:1 1 {transport} 1 {address} 12345 typ {kind}\r\n")
    }
    #[test]
    fn default_remains_loopback() {
        assert_eq!(NetworkPolicy::default(), NetworkPolicy::Loopback);
    }
    #[test]
    fn candidate_scope_is_exact() {
        let p = NetworkPolicy::ApprovedLanPilot;
        assert!(p
            .validate_offer(&offer("192.168.1.198", "UDP", "host"))
            .is_ok());
        for ip in [
            "0.0.0.0",
            "192.168.1.147",
            "192.168.1.199",
            "8.8.8.8",
            "::1",
            "foo.local",
        ] {
            assert!(p.validate_offer(&offer(ip, "UDP", "host")).is_err());
        }
        for (transport, kind) in [("TCP", "host"), ("UDP", "relay"), ("UDP", "srflx")] {
            assert!(p
                .validate_offer(&offer("192.168.1.198", transport, kind))
                .is_err());
        }
        assert!(p.validate_offer("v=0\r\n").is_err());
        assert!(p
            .validate_offer(&format!(
                "{}{}",
                offer("192.168.1.198", "UDP", "host"),
                offer("8.8.8.8", "UDP", "host")
            ))
            .is_err());
    }
    #[test]
    fn datagram_scope_is_exact() {
        assert!(permitted("192.168.1.198:12345".parse().unwrap()));
        for addr in [
            "192.168.1.198:0",
            "192.168.1.199:12345",
            "127.0.0.1:12345",
            "8.8.8.8:12345",
            "[::ffff:192.168.1.198]:12345",
        ] {
            assert!(!permitted(addr.parse().unwrap()));
        }
    }
    struct FakeSocket {
        input: tokio::sync::Mutex<std::collections::VecDeque<SocketAddr>>,
        sent: std::sync::atomic::AtomicUsize,
        stall_send: bool,
    }
    #[async_trait::async_trait]
    impl Conn for FakeSocket {
        async fn connect(&self, _: SocketAddr) -> Result<()> {
            panic!("wrapper must not forward connect")
        }
        async fn recv(&self, _: &mut [u8]) -> Result<usize> {
            panic!("wrapper must use filtered recv_from")
        }
        async fn recv_from(&self, buf: &mut [u8]) -> Result<(usize, SocketAddr)> {
            if let Some(addr) = self.input.lock().await.pop_front() {
                buf[0] = 42;
                return Ok((1, addr));
            }
            std::future::pending().await
        }
        async fn send(&self, _: &[u8]) -> Result<usize> {
            panic!("wrapper must not forward send")
        }
        async fn send_to(&self, buf: &[u8], _: SocketAddr) -> Result<usize> {
            self.sent.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            if self.stall_send {
                std::future::pending::<()>().await;
            }
            Ok(buf.len())
        }
        fn local_addr(&self) -> Result<SocketAddr> {
            Ok("192.168.1.147:12345".parse().unwrap())
        }
        fn remote_addr(&self) -> Option<SocketAddr> {
            None
        }
        async fn close(&self) -> Result<()> {
            Ok(())
        }
        fn as_any(&self) -> &(dyn std::any::Any + Send + Sync) {
            self
        }
    }
    #[tokio::test]
    async fn filter_precedes_ice_and_close_wakes_receive() {
        let fake = Arc::new(FakeSocket {
            input: tokio::sync::Mutex::new(
                [
                    "192.168.1.199:12345".parse().unwrap(),
                    "192.168.1.198:12345".parse().unwrap(),
                ]
                .into(),
            ),
            sent: std::sync::atomic::AtomicUsize::new(0),
            stall_send: false,
        });
        let (closed, _) = watch::channel(false);
        let conn = Arc::new(FilteredSocket {
            diagnostics: None,
            socket: Arc::new(std::sync::Mutex::new(Some(fake.clone()))),
            closed,
        });
        let mut buf = [0; 16];
        assert_eq!(
            conn.recv_from(&mut buf).await.unwrap().1,
            "192.168.1.198:12345".parse().unwrap()
        );
        assert!(conn
            .send_to(b"x", "8.8.8.8:12345".parse().unwrap())
            .await
            .is_err());
        assert!(conn.send(b"x").await.is_err());
        assert!(conn
            .connect("192.168.1.198:12345".parse().unwrap())
            .await
            .is_err());
        assert_eq!(fake.sent.load(std::sync::atomic::Ordering::SeqCst), 0);
        conn.send_to(b"x", "192.168.1.198:12345".parse().unwrap())
            .await
            .unwrap();
        assert_eq!(fake.sent.load(std::sync::atomic::Ordering::SeqCst), 1);
        let receiver = conn.clone();
        let task = tokio::spawn(async move { receiver.recv(&mut [0; 16]).await });
        tokio::task::yield_now().await;
        conn.close().await.unwrap();
        assert!(conn.recv_from(&mut [0; 16]).await.is_err());
        assert!(
            tokio::time::timeout(std::time::Duration::from_secs(1), task)
                .await
                .unwrap()
                .unwrap()
                .is_err()
        );
        assert!(conn
            .send_to(b"x", "192.168.1.198:12345".parse().unwrap())
            .await
            .is_err());
    }
    #[tokio::test]
    async fn cleanup_releases_socket_even_when_mux_is_retained() {
        for cancelled in [false, true] {
            let fake = Arc::new(FakeSocket {
                input: tokio::sync::Mutex::new(Default::default()),
                sent: std::sync::atomic::AtomicUsize::new(0),
                stall_send: false,
            });
            let weak = Arc::downgrade(&fake);
            let socket: SocketOwner = Arc::new(std::sync::Mutex::new(Some(fake)));
            let (closed, _) = watch::channel(false);
            let mux = UDPMuxDefault::new(UDPMuxParams::new(FilteredSocket {
                diagnostics: None,
                socket: socket.clone(),
                closed: closed.clone(),
            }));
            let cleanup = Cleanup {
                diagnostics: None,
                mux: mux.clone(),
                closed,
                socket,
            };
            if cancelled {
                let task = tokio::spawn(async move {
                    let _guard = cleanup;
                    std::future::pending::<()>().await;
                });
                tokio::task::yield_now().await;
                task.abort();
                let _ = task.await;
            } else {
                cleanup.finish().await;
                drop(cleanup);
            }
            tokio::time::timeout(std::time::Duration::from_secs(1), async {
                while weak.upgrade().is_some() {
                    tokio::task::yield_now().await;
                }
            })
            .await
            .unwrap();
            // The retained mux cannot retain the detached socket owner.
            let _ = mux.close().await;
        }
    }
    #[tokio::test]
    async fn close_cancels_pending_send_and_blocks_new_send() {
        use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
        let fake = Arc::new(FakeSocket {
            input: tokio::sync::Mutex::new(Default::default()),
            sent: AtomicUsize::new(0),
            stall_send: true,
        });
        let (closed, _) = watch::channel(false);
        let conn = Arc::new(FilteredSocket {
            diagnostics: None,
            socket: Arc::new(std::sync::Mutex::new(Some(fake.clone()))),
            closed,
        });
        let completed = Arc::new(AtomicBool::new(false));
        let writer = conn.clone();
        let completion = completed.clone();
        let task = tokio::spawn(async move {
            let result = writer
                .send_to(b"pending", "192.168.1.198:12345".parse().unwrap())
                .await;
            if result.is_ok() {
                completion.store(true, Ordering::SeqCst);
            }
            result
        });
        tokio::time::timeout(std::time::Duration::from_secs(1), async {
            while fake.sent.load(Ordering::SeqCst) == 0 {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        conn.close().await.unwrap();
        assert!(
            tokio::time::timeout(std::time::Duration::from_secs(1), task)
                .await
                .unwrap()
                .unwrap()
                .is_err()
        );
        assert!(!completed.load(Ordering::SeqCst));
        assert!(conn
            .send_to(b"after close", "192.168.1.198:12345".parse().unwrap())
            .await
            .is_err());
        assert_eq!(fake.sent.load(Ordering::SeqCst), 1);
    }
    #[test]
    fn diagnostics_are_bounded_counts_and_never_enable_loopback() {
        use std::sync::atomic::Ordering;
        assert!(!diagnostics_enabled(NetworkPolicy::Loopback));
        let d = Diagnostics::default();
        for index in 0..4 {
            d.record(index);
            assert_eq!(d.counts[index].load(Ordering::Relaxed), 1);
        }
        d.counts[0].store(u64::MAX, Ordering::Relaxed);
        d.record(0);
        assert_eq!(d.counts[0].load(Ordering::Relaxed), u64::MAX);
        d.summary();
        d.summary();
        assert!(d.reported.load(Ordering::Relaxed));
    }
    #[test]
    fn only_explicit_lan_pilot_extends_ice_failure_stage() {
        assert_eq!(NetworkPolicy::Loopback.ice_failed_timeout(), None);
        assert_eq!(
            NetworkPolicy::ApprovedLanPilot.ice_failed_timeout(),
            Some(std::time::Duration::from_secs(120))
        );
    }
    #[test]
    fn direct_configuration_is_explicit_fixed_and_closed() {
        assert!(NetworkPolicy::default()
            .rtc_configuration()
            .ice_servers
            .is_empty());
        let c = NetworkPolicy::DirectStun.rtc_configuration();
        assert_eq!(c.ice_servers.len(), 1);
        assert_eq!(c.ice_servers[0].urls, DIRECT_STUN_URLS);
        assert!(c.ice_servers[0].username.is_empty() && c.ice_servers[0].credential.is_empty());
        let mut context = harbor_noise::SessionContext {
            host_id: "a".repeat(64),
            session_id: "b".repeat(64),
            principal: "fixture".into(),
            expires_at: 1,
            network_profile: None,
        };
        assert!(NetworkPolicy::DirectStun
            .validate_context(&context)
            .is_err());
        context.network_profile = Some(harbor_noise::NetworkProfile::DirectStunV1);
        assert!(NetworkPolicy::DirectStun.validate_context(&context).is_ok());
        assert!(NetworkPolicy::Loopback.validate_context(&context).is_err());
        assert!(NetworkPolicy::ApprovedLanPilot
            .validate_context(&context)
            .is_err());
    }
    #[test]
    fn direct_candidate_validation_rejects_unsafe_supplied_targets() {
        let policy = NetworkPolicy::DirectStun;
        assert!(policy
            .validate_offer(&offer("8.8.8.8", "udp", "srflx"))
            .is_ok());
        for ip in [
            "0.0.0.0",
            "10.0.0.1",
            "100.64.0.1",
            "127.0.0.1",
            "169.254.1.1",
            "172.16.0.1",
            "192.0.0.1",
            "192.0.2.1",
            "192.88.99.1",
            "192.168.1.1",
            "198.18.0.1",
            "198.51.100.1",
            "203.0.113.1",
            "224.0.0.1",
            "255.255.255.255",
            "::1",
            "8.8.8.8.local",
        ] {
            assert!(
                policy.validate_offer(&offer(ip, "udp", "srflx")).is_err(),
                "{ip}"
            );
        }
        for (proto, kind) in [("tcp", "host"), ("udp", "relay"), ("udp", "prflx")] {
            assert!(policy
                .validate_offer(&offer("8.8.8.8", proto, kind))
                .is_err());
        }
        assert!(policy
            .validate_offer(&offer("8.8.8.8", "udp", "srflx").repeat(17))
            .is_err());
        let mixed = offer("192.168.1.1", "udp", "host") + &offer("8.8.8.8", "udp", "srflx");
        let clean = policy.public_description(&mixed).unwrap();
        assert!(!clean.contains("192.168.1.1"));
        assert!(policy.validate_offer(&clean).is_ok());
    }
}
