//! Source majority/120s vote rules with a serialized transport coordinator.
//! The room owns identities. No storage, timers or networking are hidden here.
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "item", rename_all = "snake_case")]
pub enum Purchase {
    Craft(String),
    Build(String),
}
impl Purchase {
    pub fn command(&self) -> crate::progression::Command {
        match self {
            Self::Craft(id) => crate::progression::Command::Craft(id.clone()),
            Self::Build(id) => crate::progression::Command::Build(id.clone()),
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    Voting,
    Granted,
    Passed,
    Failed,
    Expired,
    Cancelled,
    Blocked,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct View {
    pub round: u64,
    pub revision: u64,
    pub proposer: String,
    pub purchase: Purchase,
    pub cost: u32,
    pub votes: BTreeMap<String, bool>,
    pub phase: Phase,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum Message {
    Request {
        sequence: u64,
        purchase: Purchase,
        cost: u32,
    },
    Cast {
        round: u64,
        yes: bool,
    },
    Snapshot {
        view: View,
    },
    Receipt {
        round: u64,
        applied: bool,
    },
}
#[derive(Default)]
pub struct Vote {
    pub view: Option<View>,
    coordinator: Option<String>,
    members: BTreeSet<String>,
    requests: BTreeMap<String, u64>,
    sequence: u64,
    round: u64,
    revision: u64,
    started: f64,
    blocked: bool,
    applied: BTreeSet<u64>,
    outbox: Vec<Message>,
}
impl Vote {
    pub fn suspend(&mut self) {
        if self.coordinator.is_some()
            && self
                .view
                .as_ref()
                .is_none_or(|v| matches!(v.phase, Phase::Voting | Phase::Granted))
        {
            self.blocked = true;
            if let Some(v) = &mut self.view {
                v.phase = Phase::Blocked;
            }
        }
        self.outbox.clear();
    }
    pub fn blocked(&self) -> bool {
        self.blocked
    }
    /// Membership changes abort votes before grant; after grant the outcome is uncertain.
    /// Coordinator loss never elects a replacement that might repeat a purchase.
    pub fn sync(&mut self, mine: &str, members: BTreeSet<String>, now: f64) {
        if !now.is_finite() || members.len() > 6 || !members.contains(mine) {
            return;
        }
        if let Some(coordinator) = &self.coordinator {
            if members.first() != Some(coordinator) {
                self.blocked = true;
                if let Some(v) = &mut self.view {
                    v.phase = Phase::Blocked;
                }
                return;
            }
            if self.members != members
                && self
                    .view
                    .as_ref()
                    .is_some_and(|v| matches!(v.phase, Phase::Voting | Phase::Granted))
            {
                if self.view.as_ref().unwrap().phase == Phase::Granted {
                    self.blocked = true;
                    self.view.as_mut().unwrap().phase = Phase::Blocked;
                } else if coordinator == mine {
                    self.finish(Phase::Cancelled);
                }
            }
        }
        self.members = members;
        if self.coordinator.as_deref() == Some(mine)
            && !self.blocked
            && self.view.as_ref().is_some_and(|v| v.phase == Phase::Voting)
            && now - self.started > 120.
        {
            self.finish(Phase::Expired);
        }
    }
    fn pin(&mut self) {
        if self.coordinator.is_none() {
            self.coordinator = self.members.first().cloned();
        }
    }
    pub fn propose(&mut self, purchase: Purchase, cost: u32) -> Result<(), &'static str> {
        if self.blocked
            || self
                .view
                .as_ref()
                .is_some_and(|v| matches!(v.phase, Phase::Voting | Phase::Granted))
        {
            return Err("A team purchase is already pending or needs a room reconnect");
        }
        if cost > 10000 || self.members.is_empty() {
            return Err("Invalid team purchase");
        }
        self.pin();
        self.sequence = self
            .sequence
            .checked_add(1)
            .ok_or("Vote sequence exhausted")?;
        self.outbox.push(Message::Request {
            sequence: self.sequence,
            purchase,
            cost,
        });
        Ok(())
    }
    pub fn cast(&mut self, yes: bool) -> Result<(), &'static str> {
        if self.blocked {
            return Err("Reconnect before another team purchase");
        }
        let view = self
            .view
            .as_ref()
            .filter(|v| v.phase == Phase::Voting)
            .ok_or("No open vote")?;
        self.outbox.push(Message::Cast {
            round: view.round,
            yes,
        });
        Ok(())
    }
    /// Only coordinator consumes requests/votes. `valid` recomputes current source cost.
    pub fn receive(
        &mut self,
        mine: &str,
        sender: &str,
        message: Message,
        now: f64,
        valid: impl Fn(&Purchase, u32) -> bool,
    ) {
        if self.blocked || !now.is_finite() || !self.members.contains(sender) {
            return;
        }
        self.pin();
        let coordinator = self.coordinator.as_deref().unwrap_or("").to_owned();
        match message {
            Message::Request {
                sequence,
                purchase,
                cost,
            } if coordinator == mine => {
                if sequence == 0 || sequence <= *self.requests.get(sender).unwrap_or(&0) {
                    return;
                }
                if !self.requests.contains_key(sender) && self.requests.len() >= 128 {
                    self.blocked = true;
                    return;
                }
                self.requests.insert(sender.into(), sequence);
                if self
                    .view
                    .as_ref()
                    .is_some_and(|v| matches!(v.phase, Phase::Voting | Phase::Granted))
                    || !valid(&purchase, cost)
                {
                    return;
                }
                if self.round >= 128 {
                    self.blocked = true;
                    self.finish(Phase::Blocked);
                    return;
                }
                self.round += 1;
                self.revision = 0;
                self.started = now;
                self.view = Some(View {
                    round: self.round,
                    revision: 0,
                    proposer: sender.into(),
                    purchase,
                    cost,
                    votes: BTreeMap::from([(sender.into(), true)]),
                    phase: Phase::Voting,
                });
                self.publish();
                self.decide();
            }
            Message::Cast { round, yes } if coordinator == mine => {
                if let Some(v) = &mut self.view {
                    if v.round != round || v.phase != Phase::Voting {
                        return;
                    }
                    v.votes.insert(sender.into(), yes);
                } else {
                    return;
                }
                self.publish();
                self.decide();
            }
            Message::Snapshot { view } if sender == coordinator && sender != mine => {
                if view.round == 0
                    || view.round > 128
                    || view.cost > 10000
                    || (matches!(view.phase, Phase::Voting | Phase::Granted)
                        && !self.members.contains(&view.proposer))
                    || view.votes.len() > 6
                    || (matches!(view.phase, Phase::Voting | Phase::Granted)
                        && view.votes.keys().any(|p| !self.members.contains(p)))
                {
                    return;
                }
                if self
                    .view
                    .as_ref()
                    .is_some_and(|v| (view.round, view.revision) <= (v.round, v.revision))
                {
                    return;
                }
                if self.view.as_ref().is_some_and(|old| {
                    old.round == view.round
                        && (old.purchase != view.purchase
                            || old.cost != view.cost
                            || old.proposer != view.proposer)
                }) {
                    return;
                }
                if view.phase == Phase::Blocked {
                    self.blocked = true;
                }
                self.view = Some(view);
            }
            Message::Receipt { round, applied }
                if coordinator == mine
                    && self.view.as_ref().is_some_and(|v| {
                        v.round == round && v.proposer == sender && v.phase == Phase::Granted
                    }) =>
            {
                self.finish(if applied {
                    Phase::Passed
                } else {
                    Phase::Failed
                });
            }
            _ => {}
        }
    }
    fn decide(&mut self) {
        let Some(v) = &self.view else {
            return;
        };
        let yes = v.votes.values().filter(|v| **v).count();
        let no = v.votes.len() - yes;
        let n = self.members.len().max(v.votes.len());
        if yes > n / 2 || (v.votes.len() >= n && yes > no) {
            self.finish(Phase::Granted);
        } else if no * 2 >= n || (v.votes.len() >= n && no >= yes) {
            self.finish(Phase::Failed);
        }
    }
    fn publish(&mut self) {
        self.revision += 1;
        if let Some(v) = &mut self.view {
            v.revision = self.revision;
            self.outbox.push(Message::Snapshot { view: v.clone() });
        }
    }
    fn finish(&mut self, phase: Phase) {
        if let Some(v) = &mut self.view {
            v.phase = phase;
        }
        self.publish();
    }
    /// Claim locally before mutation; duplicate/replayed grants can never apply twice.
    pub fn take_grant(&mut self, mine: &str) -> Option<(u64, Purchase, u32)> {
        if self.blocked {
            return None;
        }
        let v = self.view.as_ref()?;
        if v.phase != Phase::Granted || v.proposer != mine || !self.applied.insert(v.round) {
            return None;
        }
        Some((v.round, v.purchase.clone(), v.cost))
    }
    pub fn receipt(&mut self, round: u64, applied: bool) {
        self.outbox.push(Message::Receipt { round, applied });
    }
    pub fn drain(&mut self) -> Vec<Message> {
        std::mem::take(&mut self.outbox)
    }
}
