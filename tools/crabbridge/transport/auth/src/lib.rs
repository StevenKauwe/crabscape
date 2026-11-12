#![forbid(unsafe_code)]
use wasm_bindgen::prelude::*;
const PROLOGUE: &[u8] =
    b"codex-harbor/transport/v1;role=browser-to-host;purpose=encrypted-application-gateway";
const MAX: usize = 16_384;
const PATTERN: &str = "Noise_NNpsk0_25519_ChaChaPoly_BLAKE2s";

#[derive(Clone, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SessionContext {
    pub host_id: String,
    pub principal: String,
    pub session_id: String,
    pub expires_at: u64,
}
impl SessionContext {
    pub fn validate(&self) -> Result<(), String> {
        if [&self.host_id, &self.session_id]
            .iter()
            .any(|id| id.len() != 64 || !id.bytes().all(|b| b.is_ascii_hexdigit()))
        {
            return Err("Invalid context identity".into());
        }
        if self.principal.is_empty()
            || self.principal.len() > 128
            || !self
                .principal
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"_@.-".contains(&b))
        {
            return Err("Invalid principal".into());
        }
        Ok(())
    }
}
enum State {
    Handshake(snow::HandshakeState),
    Transport(snow::TransportState),
    Closed,
}
pub struct Channel {
    state: State,
}
impl Channel {
    pub fn new(key: &[u8; 32], initiator: bool, context: &SessionContext) -> Result<Self, String> {
        context.validate()?;
        let mut prologue = PROLOGUE.to_vec();
        prologue.extend_from_slice(
            &serde_json::to_vec(context).map_err(|_| "Context encoding failed")?,
        );
        let builder =
            snow::Builder::new(PATTERN.parse().map_err(|_| "Invalid fixed Noise pattern")?)
                .psk(0, key)
                .map_err(|_| "Invalid PSK")?
                .prologue(&prologue)
                .map_err(|_| "Invalid prologue")?;
        let state = if initiator {
            builder.build_initiator()
        } else {
            builder.build_responder()
        }
        .map_err(|_| "Noise initialization failed")?;
        Ok(Self {
            state: State::Handshake(state),
        })
    }
    pub fn handshake_write(&mut self) -> Result<Vec<u8>, String> {
        let mut bytes = vec![0; 1024];
        let State::Handshake(h) = &mut self.state else {
            return Err("Not handshaking".into());
        };
        match h.write_message(&[], &mut bytes) {
            Ok(n) => {
                bytes.truncate(n);
                self.promote()?;
                Ok(bytes)
            }
            Err(_) => {
                self.state = State::Closed;
                Err("Handshake failed".into())
            }
        }
    }
    pub fn handshake_read(&mut self, bytes: &[u8]) -> Result<(), String> {
        if bytes.len() > 1024 {
            self.state = State::Closed;
            return Err("Handshake too large".into());
        }
        let State::Handshake(h) = &mut self.state else {
            return Err("Not handshaking".into());
        };
        let mut payload = [0; 1024];
        match h.read_message(bytes, &mut payload) {
            Ok(0) => self.promote(),
            _ => {
                self.state = State::Closed;
                Err("Handshake failed".into())
            }
        }
    }
    fn promote(&mut self) -> Result<(), String> {
        if matches!(&self.state,State::Handshake(h) if h.is_handshake_finished()) {
            let State::Handshake(h) = std::mem::replace(&mut self.state, State::Closed) else {
                unreachable!()
            };
            self.state = State::Transport(
                h.into_transport_mode()
                    .map_err(|_| "Transport transition failed")?,
            );
        }
        Ok(())
    }
    pub fn ready(&self) -> bool {
        matches!(self.state, State::Transport(_))
    }
    pub fn encrypt(&mut self, plaintext: &[u8]) -> Result<Vec<u8>, String> {
        if plaintext.len() > MAX {
            return Err("Message too large".into());
        }
        let State::Transport(t) = &mut self.state else {
            return Err("Not authenticated".into());
        };
        let mut bytes = vec![0; plaintext.len() + 16];
        match t.write_message(plaintext, &mut bytes) {
            Ok(n) => {
                bytes.truncate(n);
                Ok(bytes)
            }
            Err(_) => {
                self.state = State::Closed;
                Err("Encryption failed".into())
            }
        }
    }
    pub fn decrypt(&mut self, bytes: &[u8]) -> Result<Vec<u8>, String> {
        if bytes.len() > MAX + 16 {
            self.state = State::Closed;
            return Err("Message too large".into());
        }
        let State::Transport(t) = &mut self.state else {
            return Err("Not authenticated".into());
        };
        let mut plain = vec![0; bytes.len()];
        match t.read_message(bytes, &mut plain) {
            Ok(n) => {
                plain.truncate(n);
                Ok(plain)
            }
            Err(_) => {
                self.state = State::Closed;
                Err("Invalid, tampered or replayed message".into())
            }
        }
    }
}
#[wasm_bindgen]
pub struct BrowserChannel {
    inner: Channel,
}
#[wasm_bindgen]
impl BrowserChannel {
    #[wasm_bindgen(constructor)]
    pub fn new(key_hex: &str, context_json: &str) -> Result<BrowserChannel, JsValue> {
        if context_json.len() > 1024 {
            return Err(JsValue::from_str("Context too large"));
        }
        let context: SessionContext =
            serde_json::from_str(context_json).map_err(|_| JsValue::from_str("Invalid context"))?;
        let bytes = hex::decode(key_hex).map_err(|_| JsValue::from_str("Invalid pairing key"))?;
        let key: [u8; 32] = bytes
            .try_into()
            .map_err(|_| JsValue::from_str("Pairing key must be 256 bits"))?;
        Ok(Self {
            inner: Channel::new(&key, true, &context).map_err(|s| JsValue::from_str(&s))?,
        })
    }
    pub fn begin(&mut self) -> Result<Vec<u8>, JsValue> {
        self.inner
            .handshake_write()
            .map_err(|s| JsValue::from_str(&s))
    }
    pub fn finish(&mut self, bytes: &[u8]) -> Result<(), JsValue> {
        self.inner
            .handshake_read(bytes)
            .map_err(|s| JsValue::from_str(&s))
    }
    pub fn ready(&self) -> bool {
        self.inner.ready()
    }
    pub fn encrypt(&mut self, plaintext: &[u8]) -> Result<Vec<u8>, JsValue> {
        self.inner
            .encrypt(plaintext)
            .map_err(|s| JsValue::from_str(&s))
    }
    pub fn decrypt(&mut self, bytes: &[u8]) -> Result<Vec<u8>, JsValue> {
        self.inner.decrypt(bytes).map_err(|s| JsValue::from_str(&s))
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn context() -> SessionContext {
        SessionContext {
            host_id: "01".repeat(32),
            principal: "test".into(),
            session_id: "02".repeat(32),
            expires_at: 2_000_000_000,
        }
    }
    fn pair() -> (Channel, Channel) {
        let context = context();
        let key = [7; 32];
        let mut a = Channel::new(&key, true, &context).unwrap();
        let mut b = Channel::new(&key, false, &context).unwrap();
        b.handshake_read(&a.handshake_write().unwrap()).unwrap();
        a.handshake_read(&b.handshake_write().unwrap()).unwrap();
        (a, b)
    }
    #[test]
    fn both_directions() {
        let (mut a, mut b) = pair();
        assert_eq!(b.decrypt(&a.encrypt(b"hello").unwrap()).unwrap(), b"hello");
        assert_eq!(a.decrypt(&b.encrypt(b"world").unwrap()).unwrap(), b"world");
    }
    #[test]
    fn wrong_key() {
        let mut a = Channel::new(&[1; 32], true, &context()).unwrap();
        let mut b = Channel::new(&[2; 32], false, &context()).unwrap();
        assert!(b.handshake_read(&a.handshake_write().unwrap()).is_err());
        assert!(!b.ready());
    }
    #[test]
    fn replay_closes() {
        let (mut a, mut b) = pair();
        let c = a.encrypt(b"once").unwrap();
        b.decrypt(&c).unwrap();
        assert!(b.decrypt(&c).is_err());
        assert!(!b.ready());
    }
    #[test]
    fn tamper_closes() {
        let (mut a, mut b) = pair();
        let mut c = a.encrypt(b"safe").unwrap();
        c[0] ^= 1;
        assert!(b.decrypt(&c).is_err());
        assert!(!b.ready());
    }
    #[test]
    fn pre_auth_denied() {
        let mut a = Channel::new(&[1; 32], true, &context()).unwrap();
        assert!(a.encrypt(b"command").is_err());
    }
    #[test]
    fn bounds() {
        let (mut a, mut b) = pair();
        assert!(a.encrypt(&vec![0; MAX + 1]).is_err());
        assert!(b.decrypt(&vec![0; MAX + 17]).is_err());
        assert!(!b.ready());
    }
    #[test]
    fn wrong_context_denied() {
        let mut context2 = context();
        context2.principal = "other".into();
        let mut a = Channel::new(&[1; 32], true, &context()).unwrap();
        let mut b = Channel::new(&[1; 32], false, &context2).unwrap();
        assert!(b.handshake_read(&a.handshake_write().unwrap()).is_err());
    }
    #[test]
    fn every_context_field_is_authenticated() {
        for field in ["host_id", "principal", "session_id", "expires_at"] {
            let mut changed = context();
            match field {
                "host_id" => changed.host_id = "03".repeat(32),
                "principal" => changed.principal = "another-user".into(),
                "session_id" => changed.session_id = "04".repeat(32),
                "expires_at" => changed.expires_at += 1,
                _ => unreachable!(),
            }
            let mut a = Channel::new(&[7; 32], true, &context()).unwrap();
            let mut b = Channel::new(&[7; 32], false, &changed).unwrap();
            assert!(
                b.handshake_read(&a.handshake_write().unwrap()).is_err(),
                "{field}"
            );
            assert!(matches!(b.state, State::Closed), "{field}");
            assert!(b.encrypt(b"no access").is_err());
        }
    }
    #[test]
    fn different_connection_rejects_ciphertext_with_same_key_and_context() {
        let (mut a, mut b) = pair();
        let (_, mut other_b) = pair();
        let frame = a.encrypt(b"connection-bound").unwrap();
        assert!(other_b.decrypt(&frame).is_err());
        assert!(matches!(other_b.state, State::Closed));
        assert_eq!(b.decrypt(&frame).unwrap(), b"connection-bound");
    }
    #[test]
    fn reordered_record_closes_and_cannot_resume() {
        let (mut a, mut b) = pair();
        let first = a.encrypt(b"first").unwrap();
        let second = a.encrypt(b"second").unwrap();
        assert!(b.decrypt(&second).is_err());
        assert!(matches!(b.state, State::Closed));
        assert!(b.decrypt(&first).is_err());
        assert!(b.encrypt(b"reply").is_err());
    }
    #[test]
    fn truncated_ciphertext_closes() {
        for removed in [1, 16, 20] {
            let (mut a, mut b) = pair();
            let frame = a.encrypt(b"data").unwrap();
            assert!(b.decrypt(&frame[..frame.len() - removed]).is_err());
            assert!(matches!(b.state, State::Closed));
            assert!(b.decrypt(&frame).is_err());
        }
    }
    #[test]
    fn truncated_handshake_closes_at_either_step() {
        for responder_step in [true, false] {
            let mut a = Channel::new(&[7; 32], true, &context()).unwrap();
            let mut b = Channel::new(&[7; 32], false, &context()).unwrap();
            let first = a.handshake_write().unwrap();
            let (target, frame) = if responder_step {
                (&mut b, first)
            } else {
                b.handshake_read(&first).unwrap();
                (&mut a, b.handshake_write().unwrap())
            };
            assert!(target.handshake_read(&frame[..frame.len() - 1]).is_err());
            assert!(matches!(target.state, State::Closed));
            assert!(target.handshake_read(&frame).is_err());
        }
    }
    #[test]
    fn unauthenticated_decrypt_and_invalid_handshake_order_are_rejected() {
        let mut a = Channel::new(&[7; 32], true, &context()).unwrap();
        assert!(a.decrypt(&[0; 16]).is_err());
        assert!(!a.ready());
        let mut b = Channel::new(&[7; 32], false, &context()).unwrap();
        assert!(b.handshake_write().is_err());
        assert!(matches!(b.state, State::Closed));
        let (mut a, mut b) = pair();
        assert!(a.handshake_write().is_err());
        assert!(b.handshake_read(&[]).is_err());
        // Misusing the handshake API must not advance transport counters.
        assert_eq!(
            b.decrypt(&a.encrypt(b"still ordered").unwrap()).unwrap(),
            b"still ordered"
        );
    }
}
