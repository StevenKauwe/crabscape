//! Local-only client for the bounded rendezvous proof. Never sends application secrets.
use serde::{Deserialize, Serialize};
use std::{sync::Arc, time::Duration};
use webrtc::peer_connection::sdp::session_description::RTCSessionDescription;
pub const ENDPOINT: &str = "http://127.0.0.1:7879";
const MAX_SDP: usize = 65_536;
const MAX_ENVELOPE: usize = MAX_SDP * 6 + 128;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Description {
    #[serde(rename = "type")]
    kind: String,
    sdp: String,
}
pub(crate) fn parse_offer(bytes: &[u8]) -> Result<RTCSessionDescription, String> {
    if bytes.len() > MAX_ENVELOPE {
        return Err("Offer envelope too large".into());
    }
    let value: Description = serde_json::from_slice(bytes).map_err(|_| "Invalid offer schema")?;
    if value.kind != "offer"
        || value.sdp.len() > MAX_SDP
        || !value.sdp.starts_with("v=0\r\n")
        || value.sdp.contains('\0')
    {
        return Err("Invalid offer description".into());
    }
    RTCSessionDescription::offer(value.sdp).map_err(|_| "Invalid offer SDP".into())
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ClientSignal {
    pub endpoint: String,
    pub client_token: String,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct HostSignal {
    pub host_token: String,
}
pub fn valid_token(token: &str) -> bool {
    token.len() == 64 && token.bytes().all(|b| b.is_ascii_hexdigit())
}
impl ClientSignal {
    pub fn validate(&self) -> Result<(), String> {
        if self.endpoint != ENDPOINT || !valid_token(&self.client_token) {
            return Err("Only the fixed loopback rendezvous is supported".into());
        }
        Ok(())
    }
}
fn client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(3))
        .build()
        .map_err(|e| e.to_string())
}
pub async fn receive_offer(
    host: &HostSignal,
    expires: u64,
    guard: Arc<crate::config_guard::ConfigGuard>,
) -> Result<RTCSessionDescription, String> {
    if !valid_token(&host.host_token) {
        return Err("Invalid host signaling credential".into());
    }
    let client = client()?;
    loop {
        if crate::unix_now() >= expires || !guard.is_current() {
            return Err("Pairing expired or revoked".into());
        }
        let mut response = client
            .get(format!("{ENDPOINT}/offer"))
            .bearer_auth(&host.host_token)
            .send()
            .await
            .map_err(|_| "Signaling unavailable")?;
        if response.status() == reqwest::StatusCode::NO_CONTENT {
            tokio::time::sleep(Duration::from_millis(500)).await;
            continue;
        }
        if response.status() != reqwest::StatusCode::OK {
            return Err("Signaling offer refused".into());
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|_| "Signaling response failed")?
        {
            if bytes.len() + chunk.len() > MAX_ENVELOPE {
                return Err("Signaling response too large".into());
            }
            bytes.extend_from_slice(&chunk);
        }
        if crate::unix_now() >= expires || !guard.is_current() {
            return Err("Pairing expired or revoked".into());
        }
        return parse_offer(&bytes);
    }
}
pub fn publisher(
    host: HostSignal,
    expires: u64,
    guard: Arc<crate::config_guard::ConfigGuard>,
) -> harbor_transport_spike::AnswerPublisher {
    Arc::new(move |answer| {
        let host = host.clone();
        let guard = guard.clone();
        Box::pin(async move {
            if crate::unix_now() >= expires || !guard.is_current() || !valid_token(&host.host_token)
            {
                return Err("Pairing expired or revoked".into());
            }
            if answer.sdp.len() > MAX_SDP {
                return Err("Answer SDP too large".into());
            }
            let response = client()?
                .post(format!("{ENDPOINT}/answer"))
                .bearer_auth(&host.host_token)
                .json(&answer)
                .send()
                .await
                .map_err(|_| "Answer delivery failed")?;
            if response.status() != reqwest::StatusCode::CREATED {
                return Err("Answer delivery refused; make fresh pairing".into());
            }
            Ok(())
        })
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn untrusted_offer_schema_and_utf8_bounds_are_checked() {
        for value in [
            serde_json::json!({"type":"answer","sdp":"v=0\r\n"}),
            serde_json::json!({"type":"offer","sdp":"v=0\r\n","extra":true}),
            serde_json::json!({"type":"offer","sdp":format!("v=0\r\n{}", "é".repeat(32768))}),
            serde_json::json!({"type":"offer","sdp":"v=0\r\n\0"}),
        ] {
            assert!(parse_offer(&serde_json::to_vec(&value).unwrap()).is_err());
        }
        assert!(parse_offer(br#"{"type":"offer","type":"answer","sdp":"v=0\r\n"}"#).is_err());
    }
    #[test]
    fn invitation_cannot_select_network_destination() {
        for endpoint in [
            "https://example.com",
            "http://localhost:7879",
            "http://127.0.0.1:7879/",
            "http://127.0.0.1:1",
        ] {
            assert!(ClientSignal {
                endpoint: endpoint.into(),
                client_token: "a".repeat(64)
            }
            .validate()
            .is_err());
        }
        assert!(ClientSignal {
            endpoint: ENDPOINT.into(),
            client_token: "a".repeat(64)
        }
        .validate()
        .is_ok());
        assert!(!valid_token("secret"));
    }
}
