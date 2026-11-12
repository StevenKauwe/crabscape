#![forbid(unsafe_code)]
use rand::{rngs::OsRng, RngCore};
use std::{io::Write, os::unix::fs::OpenOptionsExt, path::Path, sync::Arc, time::Duration};
use webrtc::peer_connection::sdp::session_description::RTCSessionDescription;
fn write_private(path: &Path, bytes: &[u8]) -> Result<(), Box<dyn std::error::Error>> {
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)?;
    f.write_all(bytes)?;
    Ok(())
}
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = std::env::args().collect::<Vec<_>>();
    if args.get(1).map(String::as_str) == Some("keygen") {
        let mut key = [0; 32];
        OsRng.fill_bytes(&mut key);
        let mut id = [0; 32];
        OsRng.fill_bytes(&mut id);
        let host_id = hex::encode(id);
        OsRng.fill_bytes(&mut id);
        let context = harbor_noise::SessionContext {
            network_profile: None,
            host_id,
            principal: "echo".into(),
            session_id: hex::encode(id),
            expires_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?
                .as_secs()
                + 600,
        };
        write_private(
            Path::new(args.get(2).ok_or("keygen KEYFILE")?),
            &serde_json::to_vec_pretty(
                &serde_json::json!({"key":hex::encode(key),"context":context}),
            )?,
        )?;
        println!("Pairing capability saved locally. Treat it like a password; never publish it.");
        return Ok(());
    }
    if args.len() != 5 || args[1] != "answer" {
        return Err(
            "Usage: harbor-transport-spike keygen KEYFILE | answer KEYFILE OFFER_JSON ANSWER_JSON"
                .into(),
        );
    }
    let invitation: serde_json::Value = serde_json::from_slice(&std::fs::read(&args[2])?)?;
    let key: [u8; 32] = hex::decode(invitation["key"].as_str().ok_or("Missing key")?)?
        .try_into()
        .map_err(|_| "Invalid pairing capability")?;
    let context: harbor_noise::SessionContext =
        serde_json::from_value(invitation["context"].clone())?;
    let offer: RTCSessionDescription = serde_json::from_slice(&std::fs::read(&args[3])?)?;
    if offer.sdp.len() > 65_536 {
        return Err("Offer too large".into());
    }
    let handler: harbor_transport_spike::Handler = Arc::new(|plain| {
        Box::pin(async move {
            let v: serde_json::Value =
                serde_json::from_slice(&plain).map_err(|_| "Invalid JSON")?;
            if v.as_object().is_none_or(|m| m.len() != 2)
                || v["type"] != "echo"
                || v["text"].as_str().is_none_or(|s| s.len() > 4096)
            {
                return Err("Only bounded echo allowed".into());
            }
            serde_json::to_vec(&v).map_err(|_| "Encode failed".into())
        })
    });
    println!("Loopback-only authenticated echo; no CLI access. Exits after 120 seconds or Ctrl-C.");
    harbor_transport_spike::serve_offer(
        key,
        context,
        offer,
        Path::new(&args[4]),
        Duration::from_secs(120),
        handler,
    )
    .await
    .map_err(|e| e.to_string())?;
    Ok(())
}
