//! Source Party pick/Ready/countdown rules. No clocks, network or UI are hidden here.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Party {
    pub mission: Option<String>,
    pub picked_at: f64,
    pub ready: Option<String>,
    pub countdown: f64,
}
pub struct Peer<'a> {
    pub state: &'a str,
    pub mission: Option<&'a str>,
    pub ready: Option<&'a str>,
}
impl Party {
    pub fn enter(&mut self) {
        self.ready = None;
        self.countdown = 0.;
    }
    pub fn pick(&mut self, mission: &str, now_ms: f64) -> bool {
        if !crate::game::Game::known_mission(mission) || !now_ms.is_finite() {
            return false;
        }
        self.mission = Some(mission.into());
        self.picked_at = now_ms;
        self.ready = self.mission.clone();
        self.countdown = 0.;
        true
    }
    pub fn receive_pick(&mut self, mission: &str, at: f64, state: &str) -> bool {
        if state != "party"
            || !at.is_finite()
            || at <= self.picked_at
            || !crate::game::Game::known_mission(mission)
        {
            return false;
        }
        self.mission = Some(mission.into());
        self.picked_at = at;
        if self.countdown > 0.5 {
            self.countdown = 0.;
        }
        true
    }
    pub fn toggle_ready(&mut self) {
        if self.mission.is_some() {
            self.ready = if self.ready == self.mission {
                None
            } else {
                self.mission.clone()
            };
        }
    }
    pub fn all_ready(&self, peers: &[Peer<'_>]) -> bool {
        let Some(mission) = self.mission.as_deref() else {
            return false;
        };
        self.ready.as_deref() == Some(mission)
            && !peers.is_empty()
            && peers.iter().all(|peer| {
                (peer.state == "party" && peer.ready == Some(mission))
                    || (peer.state == "play" && peer.mission == Some(mission))
            })
    }
    /// The first ready frame starts at exactly three. The final half-second is
    /// committed, so a faster partner entering play cannot cancel this launch.
    pub fn tick(&mut self, dt: f64, state: &str, ready: bool) -> Option<String> {
        if state != "party" || !dt.is_finite() || dt < 0. {
            return None;
        }
        if !ready && self.countdown > 0.5 {
            self.countdown = 0.;
            return None;
        }
        if ready && self.countdown <= 0. {
            self.countdown = 3.;
            return None;
        }
        if self.countdown > 0. {
            self.countdown -= dt;
            if self.countdown <= 0. {
                let mission = self.mission.clone();
                self.enter();
                return mission;
            }
        }
        None
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn newest_pick_keeps_readiness_and_half_second_commit_matches_source() {
        let mut p = Party::default();
        assert!(p.pick("m1", 100.));
        assert!(!p.receive_pick("m2", 100., "party"));
        p.countdown = 2.;
        assert!(p.receive_pick("m2", 101., "party"));
        assert_eq!(p.countdown, 0.);
        assert_eq!(p.ready.as_deref(), Some("m1"));
        p.toggle_ready();
        assert_eq!(p.ready.as_deref(), Some("m2"));
        assert_eq!(p.tick(0.05, "party", true), None);
        assert_eq!(p.countdown, 3.);
        p.tick(0.05, "party", false);
        assert_eq!(p.countdown, 0.);
        p.countdown = 0.5;
        assert_eq!(p.tick(0.25, "party", false), None);
        assert_eq!(p.tick(0.25, "party", false), Some("m2".into()));
        assert!(p.ready.is_none());
    }
    #[test]
    fn all_connected_riders_must_be_ready_or_already_flying_same_mission() {
        let mut p = Party::default();
        p.pick("m1", 1.);
        assert!(!p.all_ready(&[]));
        assert!(p.all_ready(&[Peer {
            state: "party",
            mission: None,
            ready: Some("m1")
        }]));
        assert!(p.all_ready(&[Peer {
            state: "play",
            mission: Some("m1"),
            ready: None
        }]));
        assert!(!p.all_ready(&[Peer {
            state: "roost",
            mission: None,
            ready: Some("m1")
        }]));
        assert!(!p.all_ready(&[Peer {
            state: "pause",
            mission: Some("m1"),
            ready: None
        }]));
    }
}
