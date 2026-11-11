//! Static-room authority election, matching `reconcileSkyHost` without network IO.
//! Peer IDs are transport identities, not rider-supplied payload fields.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Candidate {
    pub peer: String,
    pub rider: usize,
    pub host: bool,
    pub epoch: u64,
    pub paused: bool,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Authority {
    pub guest: bool,
    pub host_peer: Option<String>,
    pub host_rider: usize,
    pub epoch: u64,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Election {
    pub refresh_world_time: bool,
    pub become_host: bool,
    pub reset_presence: bool,
}
impl Authority {
    /// Candidates must already be filtered to the same mission/run, alive, playing
    /// or paused. Include the local candidate iff the local player is not out.
    /// Matchbox UUID strings have ASCII ordering equivalent to the JS locale order.
    /// Becoming host is returned as an action: the caller must restore queue and
    /// enemy cooldowns synchronously before advertising the new authority.
    pub fn reconcile(&mut self, mine: &str, candidates: &[Candidate], timed_out: bool) -> Election {
        let mut result = Election::default();
        let winner = candidates
            .iter()
            .filter(|c| c.host && c.epoch >= self.epoch)
            .min_by(|a, b| b.epoch.cmp(&a.epoch).then_with(|| a.peer.cmp(&b.peer)));
        if let Some(winner) = winner
            && winner.peer != mine
            && (winner.epoch > self.epoch
                || !self.guest
                || self.host_peer.as_deref() != Some(&winner.peer))
        {
            self.guest = true;
            self.host_peer = Some(winner.peer.clone());
            self.host_rider = winner.rider;
            self.epoch = winner.epoch;
            result.refresh_world_time = true;
        }
        if !timed_out || !self.guest {
            return result;
        }
        if candidates
            .iter()
            .any(|c| Some(c.peer.as_str()) == self.host_peer.as_deref() && c.paused)
        {
            result.refresh_world_time = true;
            return result;
        }
        let next = candidates
            .iter()
            .filter(|c| Some(c.peer.as_str()) != self.host_peer.as_deref())
            .min_by(|a, b| a.peer.cmp(&b.peer));
        if let Some(next) = next.filter(|next| next.peer != mine) {
            self.host_peer = Some(next.peer.clone());
            self.host_rider = next.rider;
            result.refresh_world_time = true;
        } else {
            result.become_host = true;
        }
        result.reset_presence = true;
        result
    }
}
