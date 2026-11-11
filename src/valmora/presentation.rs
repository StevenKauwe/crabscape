//! Display-only cross-connection identity. Never supplies gameplay authority,
//! player state, names, inventory or persistence identities from a payload.
#![cfg_attr(not(target_arch = "wasm32"), allow(dead_code))] // Crab transport is browser-only.
use crabsim::PlayerId;
use serde::{Deserialize, Serialize};

pub const MAX_PEERS: usize = 5;
pub const TTL: f64 = 3.;
pub const PREFIX: &[u8] = b"\xffcrabscape travel ";
const MAX_BYTES: usize = 384;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Scene {
    #[default]
    Castle,
    Travel,
    Landing,
    Flight,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Note {
    pub room: String,
    pub build: String,
    pub sequence: u64,
    pub dragon: Option<String>,
    pub scene: Scene,
    pub x: f32,
}
#[derive(Clone, Debug)]
pub struct Received {
    pub note: Note,
    pub at: f64,
}
pub fn connection(value: &str) -> bool {
    value.len() == 36
        && value.bytes().enumerate().all(|(i, c)| {
            if matches!(i, 8 | 13 | 18 | 23) {
                c == b'-'
            } else {
                c.is_ascii_hexdigit()
            }
        })
}
impl Note {
    pub fn valid(&self, room: &str, build: &str) -> bool {
        !room.is_empty()
            && room.len() <= 16
            && self.room == room
            && self.build == build
            && self.sequence > 0
            && self.dragon.as_deref().is_none_or(connection)
            && self.x.is_finite()
            && (35. ..=1285.).contains(&self.x)
    }
    pub fn encode(&self) -> Option<Vec<u8>> {
        let mut bytes = PREFIX.to_vec();
        bytes.extend(serde_json::to_vec(self).ok()?);
        (bytes.len() <= MAX_BYTES).then_some(bytes)
    }
}
/// Recognized malformed envelopes are consumed, never forwarded to simulation.
pub fn receive(
    previous: &mut Option<Received>,
    bytes: &[u8],
    room: &str,
    build: &str,
    now: f64,
) -> bool {
    let Some(raw) = bytes.strip_prefix(PREFIX) else {
        return false;
    };
    if bytes.len() > MAX_BYTES
        || !now.is_finite()
        || previous.as_ref().is_some_and(|old| now - old.at < 0.05)
    {
        return true;
    }
    if let Ok(note) = serde_json::from_slice::<Note>(raw)
        && note.valid(room, build)
        && previous
            .as_ref()
            .is_none_or(|old| note.sequence > old.note.sequence)
    {
        *previous = Some(Received { note, at: now });
    }
    true
}
#[derive(Clone, Debug)]
pub struct Remote {
    /// Both values are resolved from the admitted Crabscape transport, not Note.
    pub connection: String,
    pub player: PlayerId,
    pub name: String,
    pub received: Received,
}
#[derive(Default)]
pub struct Peers {
    pub now: f64,
    pub local: Option<String>,
    pub room: Option<String>,
    pub remote: Vec<Remote>,
}
impl Peers {
    pub fn live(&self, now: f64) -> impl Iterator<Item = &Remote> {
        self.remote.iter().take(MAX_PEERS).filter(move |p| {
            let age = now - p.received.at;
            now.is_finite()
                && (0. ..=TTL).contains(&age)
                && self.room.as_deref() == Some(p.received.note.room.as_str())
                && self
                    .remote
                    .iter()
                    .filter(|q| q.player == p.player || q.connection == p.connection)
                    .count()
                    == 1
        })
    }
    /// Each side's claim must come from the other's actual live connection.
    /// Conflicts fail closed for display only; gameplay never calls this method.
    pub fn resolve<'a>(
        &'a self,
        dragon: &str,
        crab: &str,
        claims: impl Iterator<Item = (&'a str, &'a str)>,
        now: f64,
    ) -> Option<&'a Remote> {
        let (mut count, mut crab_claims, mut dragon_claims, mut reciprocal) = (0, 0, 0, false);
        for (d, c) in claims.take(MAX_PEERS + 1) {
            count += 1;
            crab_claims += usize::from(c == crab);
            dragon_claims += usize::from(d == dragon);
            reciprocal |= d == dragon && c == crab;
        }
        if !connection(dragon)
            || !connection(crab)
            || count > MAX_PEERS
            || crab_claims != 1
            || dragon_claims != 1
            || !reciprocal
            || self
                .live(now)
                .filter(|p| p.received.note.dragon.as_deref() == Some(dragon))
                .count()
                != 1
        {
            return None;
        }
        self.live(now)
            .find(|p| p.connection == crab && p.received.note.dragon.as_deref() == Some(dragon))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const C: &str = "00000000-0000-0000-0000-000000000001";
    const D: &str = "00000000-0000-0000-0000-000000000002";
    const OTHER: &str = "00000000-0000-0000-0000-000000000003";
    fn note() -> Note {
        Note {
            room: "test".into(),
            build: "build".into(),
            sequence: 1,
            dragon: Some(D.into()),
            scene: Scene::Landing,
            x: 290.,
        }
    }
    fn peers() -> Peers {
        Peers {
            now: 1.,
            local: Some(OTHER.into()),
            room: Some("test".into()),
            remote: vec![Remote {
                connection: C.into(),
                player: PlayerId(7),
                name: "Same name".into(),
                received: Received {
                    note: note(),
                    at: 1.,
                },
            }],
        }
    }
    #[test]
    fn reciprocal_identity_rejects_spoofed_stale_duplicate_and_cross_room_claims() {
        let mut p = peers();
        let resolve =
            |p: &Peers, d, c, now| p.resolve(d, c, [(D, C)].into_iter(), now).map(|p| p.player);
        assert_eq!(resolve(&p, D, C, 2.), Some(PlayerId(7)));
        assert_eq!(resolve(&p, OTHER, C, 2.), None);
        assert_eq!(resolve(&p, D, OTHER, 2.), None);
        assert_eq!(resolve(&p, D, C, 5.), None);
        assert_eq!(resolve(&p, D, C, 0.), None);
        assert!(
            p.resolve(D, C, [(D, C), (OTHER, C)].into_iter(), 2.)
                .is_none()
        );
        assert!(p.resolve(D, C, [(OTHER, C)].into_iter(), 2.).is_none());
        assert!(
            p.resolve(D, C, [(D, C), (D, OTHER)].into_iter(), 2.)
                .is_none()
        );
        p.remote.push(p.remote[0].clone());
        assert_eq!(resolve(&p, D, C, 2.), None);
        p.remote.pop();
        p.room = Some("other".into());
        assert_eq!(resolve(&p, D, C, 2.), None);
        p.room = Some("test".into());
        p.remote[0].connection = OTHER.into();
        assert_eq!(resolve(&p, D, C, 2.), None);
    }
    #[test]
    fn same_names_never_alias_players_and_castle_or_replaced_connections_drop_display() {
        let mut p = peers();
        let mut other = p.remote[0].clone();
        other.player = PlayerId(8);
        other.connection = OTHER.into();
        other.received.note.dragon = Some(C.into());
        p.remote.push(other);
        assert_eq!(p.live(2.).count(), 2);
        let a = p
            .resolve(D, C, [(D, C), (C, OTHER)].into_iter(), 2.)
            .unwrap();
        assert_eq!(a.player, PlayerId(7));
        let b = p
            .resolve(C, OTHER, [(D, C), (C, OTHER)].into_iter(), 2.)
            .unwrap();
        assert_eq!(b.player, PlayerId(8));
        assert_eq!(a.name, b.name);
        p.remote[0].received.note.dragon = None;
        assert!(p.resolve(D, C, [(D, C)].into_iter(), 2.).is_none());
        p.remote.clear();
        assert!(p.resolve(D, C, [(D, C)].into_iter(), 2.).is_none());
    }

    #[test]
    fn envelopes_are_bounded_versioned_and_replay_safe_without_player_or_name_fields() {
        let mut got = None;
        let n = note();
        assert!(receive(&mut got, &n.encode().unwrap(), "test", "build", 1.));
        assert_eq!(got.as_ref().unwrap().at, 1.);
        assert!(receive(&mut got, &n.encode().unwrap(), "test", "build", 2.));
        assert_eq!(got.as_ref().unwrap().at, 1.);
        for change in 0..5 {
            let mut bad = n.clone();
            bad.sequence = 2;
            match change {
                0 => bad.room = "wrong".into(),
                1 => bad.build = "wrong".into(),
                2 => bad.x = f32::INFINITY,
                3 => bad.x = 1286.,
                _ => bad.dragon = Some("Player 7".into()),
            }
            if let Some(bytes) = bad.encode() {
                receive(&mut got, &bytes, "test", "build", 2.);
            }
            assert_eq!(got.as_ref().unwrap().note.sequence, 1);
        }
        let oversized = [PREFIX, &vec![b' '; MAX_BYTES]].concat();
        assert!(receive(&mut got, &oversized, "test", "build", 2.));
        assert_eq!(got.as_ref().unwrap().note.sequence, 1);
        assert!(!receive(&mut got, b"gameplay", "test", "build", 2.));
        let mut changed = n;
        changed.sequence = 2;
        changed.scene = Scene::Castle;
        changed.dragon = None;
        receive(&mut got, &changed.encode().unwrap(), "test", "build", 1.01);
        assert_eq!(got.as_ref().unwrap().note.sequence, 1);
        receive(
            &mut got,
            &changed.encode().unwrap(),
            "test",
            "build",
            f64::NAN,
        );
        assert_eq!(got.as_ref().unwrap().note.sequence, 1);
        receive(&mut got, &changed.encode().unwrap(), "test", "build", 2.);
        assert_eq!(got.unwrap().note, changed);
    }
}
