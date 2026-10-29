//! Co-op: what one device tells the others, and a fake connection to test it.
//!
//! Every device runs a whole [`Sim`], but not every part of it:
//! - **Its own crab** runs here, exactly as in solo, so it never feels the
//!   lag. Other players' crabs are *remote*: never stepped here, only set from
//!   their updates ([`Msg::Crab`]).
//! - **An enemy** runs on the device of the crab it hunts, so it moves and
//!   strikes on that crab's real position, as in solo; one hunting nobody
//!   runs on the host's. When its prey changes, its runner hands it over
//!   ([`Msg::Handover`]). Everyone else steps a copy between updates
//!   ([`Msg::Enemy`]), so it moves and strikes on time.
//! - **The defender decides.** An enemy's blow is judged on the device of the
//!   crab it reaches, against that device's copy of the enemy: a parry is as
//!   crisp as in solo, whatever the lag. What happened goes to the enemy's
//!   runner ([`Msg::Blow`]), which staggers it, starts its combo, and so on.
//! - **The attacker feels it at once.** A claw that reaches an enemy run
//!   elsewhere freezes, sparks and pushes the copy here at once; the runner
//!   applies the same hit when [`Msg::HitEnemy`] arrives. Until the runner's
//!   updates show it has heard every such message, this device keeps its own
//!   copy rather than flicking back to an older one.
//! - Anything that changes a crab happens only on that crab's device;
//!   anything that changes an enemy happens on its runner. Shared world bits
//!   (a cut ghost net) are announced to everyone.
//! - **The host referees shared things** (a clam, shell grit: see
//!   [`crate::shared`]). A guest's crab asks for one ([`Msg::Take`]) and gets
//!   it when the host says it's theirs ([`Msg::Taken`]); the host's own crab
//!   takes at once. What unlocks progression (eggs and the like) is each
//!   crab's own and never sent.
//!
//! Friends are trusted *in the game*: nothing here checks that a device tells
//! the truth about a hit. The bytes are never trusted: [`Msg::from_bytes`]
//! drops anything too long, cut short, malformed or holding a number that
//! isn't finite, and never panics.

use serde::{Deserialize, Serialize};

use crate::combat::{Fighter, Hit, Outcome};
use crate::enemy::{Enemy, Part};
use crate::physics::{Mover, StepEvents};
use crate::rng::Rng;
use crate::shared::{OwnWorld, Thing};
use crate::world::{PlayerId, Sim};

/// How often each device sends the state of what it runs: every 4 ticks
/// (30 times a second).
pub const SEND_EVERY: u64 = 4;

/// The longest a device keeps its own copy of an enemy while waiting for the
/// runner to hear its messages (1 s): a lost hand-over can't freeze it.
pub const HOLD_AT_MOST: u64 = 120;

/// Who a message is for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum To {
    Everyone,
    /// The device that runs the world.
    Host,
    /// The device whose crab this is.
    Player(PlayerId),
}

/// A message on its way out of a device.
#[derive(Clone, Debug, PartialEq)]
pub struct Outgoing {
    pub to: To,
    pub msg: Msg,
}

/// The most bytes one message may take. The biggest, an enemy's state, is a
/// few hundred; anything longer is dropped unread.
pub const MAX_BYTES: usize = 4096;

/// What one device tells another.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Msg {
    /// A crab's state, from its own device (often; a newer one replaces it).
    /// `at` is the sender's tick.
    Crab { at: u64, crab: CrabState },
    /// An enemy's state, from the device that runs it (often).
    Enemy { at: u64, enemy: Enemy },
    /// The device that ran an enemy hands it over (to the device of the crab
    /// it now hunts, or the host's): its whole state, with the new
    /// [`Enemy::runner`]. It must arrive.
    Handover { at: u64, enemy: Enemy },
    /// `by`'s claw reached `part` of `enemy` (run elsewhere), from `from_x`.
    HitEnemy {
        by: PlayerId,
        enemy: u32,
        part: Part,
        hit: Hit,
        from_x: f32,
    },
    /// `enemy`'s blow reached `crab`, on the crab's device: what happened.
    Blow {
        enemy: u32,
        crab: PlayerId,
        outcome: Outcome,
    },
    /// A hit for a crab that runs on another device.
    HitCrab { crab: PlayerId, hit: Hit },
    /// A ghost net (its index in the level) was cut.
    NetCut(usize),
    /// A lump of brittle rock (its index in the level) was cracked.
    RockCracked(usize),
    /// The host tells a device that just joined how the world stands: which
    /// ghost nets are cut and which brittle rocks cracked, which clam beds
    /// are bare, which shell grit and midden pieces are taken and which
    /// bodies eaten. (Crabs and enemies arrive in their own updates.)
    World {
        nets_cut: Vec<bool>,
        rocks_cracked: Vec<bool>,
        clams_bare: Vec<bool>,
        grit_taken: Vec<bool>,
        midden_taken: Vec<bool>,
        bodies_eaten: Vec<(u32, u32)>,
    },
    /// A guest's crab `by` reached a shared thing and asks the host for it.
    Take { by: PlayerId, thing: Thing },
    /// The host's word on a shared thing: gone, and crab `by` has it (to
    /// everyone), or it was gone already (`None`, to the crab that asked).
    Taken { by: Option<PlayerId>, thing: Thing },
    /// A clam bed (its index in the level) grew a new clam, on the host.
    Regrown(usize),
}

impl Msg {
    /// Must it arrive (and in order)? States can be lost: the next one
    /// replaces them.
    pub fn reliable(&self) -> bool {
        !matches!(self, Msg::Crab { .. } | Msg::Enemy { .. })
    }

    /// The message as bytes, to send.
    pub fn to_bytes(&self) -> Vec<u8> {
        postcard::to_allocvec(self).expect("a message always encodes")
    }

    /// A message from bytes another device sent, or None if they aren't a
    /// sound one: too long, cut short, malformed, or holding a number that
    /// isn't finite (a NaN position would freeze the physics). Lists inside
    /// can't be longer than the bytes, so nothing big is ever allocated.
    pub fn from_bytes(bytes: &[u8]) -> Option<Msg> {
        if bytes.len() > MAX_BYTES {
            return None;
        }
        let msg: Msg = postcard::from_bytes(bytes).ok()?;
        // Every number in it, printed: `{:?}` writes a non-finite one as NaN
        // or inf, and no name in a message contains either (a test checks).
        let text = format!("{msg:?}");
        (!text.contains("NaN") && !text.contains("inf")).then_some(msg)
    }
}

/// What other devices need of a crab: where it is and what it's doing, to
/// draw it and for enemies to hunt it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CrabState {
    pub id: PlayerId,
    pub mover: Mover,
    pub fighter: Fighter,
    /// What happened since its last update.
    pub events: StepEvents,
}

/// A device's part in a co-op game (the [`Sim`] has none in solo play).
#[derive(Clone, Debug, Default)]
pub struct Net {
    /// Runs the world: the enemies that hunt nobody.
    pub host: bool,
    pub(crate) outbox: Vec<Outgoing>,
    /// Events of this device's crabs since their last update.
    pub(crate) unsent: Vec<(PlayerId, StepEvents)>,
    /// Messages sent about each enemy run elsewhere, per crab of ours: (enemy,
    /// crab, how many, tick of the latest).
    pub(crate) told: Vec<(u32, PlayerId, u32, u64)>,
    /// The sender's tick of the newest update applied, per crab, and per
    /// enemy and hand-over (each runner counts its own ticks).
    pub(crate) crab_at: Vec<(PlayerId, u64)>,
    pub(crate) enemy_at: Vec<((u32, u32), u64)>,
    /// Shared things our crabs asked the host for, not yet answered.
    pub(crate) asked: Vec<(PlayerId, Thing)>,
    /// A guest's own world, put aside while it plays in the host's.
    pub(crate) own: Option<OwnWorld>,
}

impl Net {
    pub(crate) fn send(&mut self, to: To, msg: Msg) {
        self.outbox.push(Outgoing { to, msg });
    }

    /// One more message from `crab` about `enemy`, at `tick`.
    pub(crate) fn tell(&mut self, enemy: u32, crab: PlayerId, tick: u64) {
        match self.told.iter_mut().find(|t| t.0 == enemy && t.1 == crab) {
            Some(t) => {
                t.2 += 1;
                t.3 = tick;
            }
            None => self.told.push((enemy, crab, 1, tick)),
        }
    }

    /// Is an update of `enemy` from its runner too old to show yet: it
    /// hasn't heard every message our crabs sent it about the enemy?
    pub(crate) fn holding(&self, enemy: &Enemy, now: u64) -> bool {
        self.told.iter().any(|&(id, crab, count, at)| {
            id == enemy.id && enemy.heard_from(crab) < count && now < at + HOLD_AT_MOST
        })
    }

    /// Is `at` newer than the last update applied for this key? Notes it if so.
    pub(crate) fn newer<K: PartialEq + Copy>(list: &mut Vec<(K, u64)>, key: K, at: u64) -> bool {
        match list.iter_mut().find(|e| e.0 == key) {
            Some(e) if e.1 >= at => false,
            Some(e) => {
                e.1 = at;
                true
            }
            None => {
                list.push((key, at));
                true
            }
        }
    }
}

/// A pretend connection between devices in one process, for tests and for
/// trying co-op with a bot: each message takes `delay` ticks (plus up to
/// `jitter` more), and updates that may be lost are lost with chance `loss`.
/// Reliable messages always arrive, in the order sent. Its randomness is
/// seeded, so a test plays the same every time.
#[derive(Clone, Debug)]
pub struct FakeLink {
    pub delay: u32,
    pub jitter: u32,
    pub loss: f32,
    rng: Rng,
    now: u64,
    /// (due tick, to device, message), in the order sent.
    queue: Vec<(u64, usize, Msg)>,
    /// When the last reliable message from one device to another is due
    /// (from, to, tick): nothing reliable overtakes it.
    last_reliable: Vec<(usize, usize, u64)>,
}

impl FakeLink {
    pub fn new(delay: u32, jitter: u32, loss: f32, seed: u64) -> Self {
        Self {
            delay,
            jitter,
            loss,
            rng: Rng::new(seed),
            now: 0,
            queue: Vec::new(),
            last_reliable: Vec::new(),
        }
    }

    /// A perfect connection with a fixed delay.
    pub fn steady(delay: u32) -> Self {
        Self::new(delay, 0, 0.0, 1)
    }

    /// One tick of the connection: take what every device sent, and hand
    /// over what is due. Call after every device has ticked.
    pub fn carry(&mut self, sims: &mut [&mut Sim]) {
        for from in 0..sims.len() {
            let outbox = sims[from].take_outbox();
            for out in outbox {
                for (to, sim) in sims.iter().enumerate() {
                    if to != from && Self::addressed(sim, out.to) {
                        self.post(from, to, out.msg.clone());
                    }
                }
            }
        }
        self.now += 1;
        let now = self.now;
        let mut due = Vec::new();
        self.queue.retain(|(at, to, msg)| {
            if *at <= now {
                due.push((*to, msg.clone()));
                false
            } else {
                true
            }
        });
        for (to, msg) in due {
            sims[to].receive(msg);
        }
    }

    fn addressed(sim: &Sim, to: To) -> bool {
        match to {
            To::Everyone => true,
            To::Host => sim.is_host(),
            To::Player(id) => sim.crab(id).is_some_and(|c| !c.remote),
        }
    }

    fn post(&mut self, from: usize, to: usize, msg: Msg) {
        let lost = self.loss > 0.0
            && !msg.reliable()
            && (self.rng.next_u64() % 1_000_000) as f32 / 1_000_000.0 < self.loss;
        if lost {
            return;
        }
        let mut at =
            self.now + 1 + u64::from(self.delay) + u64::from(self.rng.range(0, self.jitter));
        if msg.reliable() {
            match self
                .last_reliable
                .iter_mut()
                .find(|l| l.0 == from && l.1 == to)
            {
                Some(l) => {
                    at = at.max(l.2);
                    l.2 = at;
                }
                None => self.last_reliable.push((from, to, at)),
            }
        }
        self.queue.push((at, to, msg));
    }
}
