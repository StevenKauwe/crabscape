//! The common octopus (`docs/design/creatures/common-octopus.md`): the
//! crab's oldest enemy, a boss in its den.
//!
//! - **Hidden:** part of the rock (only its eye shows). It watches a crab it
//!   sees (400 units) and pounces on one that passes close to its front
//!   (140): a dark flush of the skin (30 ticks), then the pounce (10), which
//!   holds on. A claw that finds it first, or a blow at all, gives it away.
//! - **Probe:** found out, it stays in its den and feels for the crab with
//!   its arm tips, at most two at a time. A tip slides in for 60 ticks (the
//!   tell) to where it touches the crab and grabs on contact. Parry it and
//!   it recoils, sore; block it and it is pushed back; cut it and it
//!   withdraws. Each arm has its own health: one at 0 is pulled back for
//!   the rest of the fight.
//!
//! Its state rides on an [`Enemy`] (`Enemy::octo`), so it is saved, sent to
//! co-op friends and hashed like any other; it steps here instead of in
//! [`crate::enemy::step`].

use bevy_math::{Rect, Vec2};

use crate::combat::{self, Grabbed, Hit, Outcome, Source};
use crate::enemy::{self, Enemy, HEAD_BACK_TICKS, MEND_AFTER, Part, State, circle_meets, huntable};
use crate::level::Level;
use crate::tuning::{ArmTuning, Tuning};
use crate::world::{PlayerId, SimCrab};

/// What one arm is doing.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ArmState {
    /// Curled under the body.
    Curled,
    /// The tell: its tip sliding in to where it will touch the crab.
    Slide,
    /// Out, feeling about where the crab was: it grabs what it touches.
    Feel,
    /// Drawing back in.
    Back,
    /// Parried: snapped back and sore for a while.
    Recoil,
    /// Holding a crab.
    Hold,
    /// Hurt to 0: pulled back for the rest of the fight.
    Lost,
}

impl ArmState {
    /// One letter, for `--dump`.
    pub fn letter(self) -> char {
        match self {
            ArmState::Curled => '.',
            ArmState::Slide => 's',
            ArmState::Feel => 'f',
            ArmState::Back => 'b',
            ArmState::Recoil => 'r',
            ArmState::Hold => 'h',
            ArmState::Lost => 'x',
        }
    }
}

/// One arm.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Arm {
    pub health: f32,
    pub state: ArmState,
    /// Ticks in its state (1 on the first).
    pub tick: u32,
    /// Where its tip is.
    pub tip: Vec2,
    /// Where a sliding tip is going...
    pub target: Vec2,
    /// ...and where it set out from (or, drawing back, where it started back).
    pub from: Vec2,
}

impl Arm {
    fn set(&mut self, state: ArmState) {
        self.state = state;
        self.tick = 0;
        self.from = self.tip;
    }

    /// Out of the body, where a claw can reach it.
    pub fn out(&self, a: &ArmTuning) -> bool {
        match self.state {
            ArmState::Slide | ArmState::Feel | ArmState::Back | ArmState::Hold => true,
            ArmState::Recoil => self.tick <= a.snap_ticks,
            ArmState::Curled | ArmState::Lost => false,
        }
    }

    /// Its hurtbox: a chain of circles (centre, radius) from `root` to the
    /// tip, tapering.
    pub fn circles(&self, root: Vec2, a: &ArmTuning) -> impl Iterator<Item = (Vec2, f32)> {
        let (n, tip) = (a.segments.max(1), self.tip);
        let (r0, r1) = a.radius;
        (0..n).map(move |k| {
            let along = (k + 1) as f32 / n as f32;
            let taper = k as f32 / (n - 1).max(1) as f32;
            (root.lerp(tip, along), r0 + (r1 - r0) * taper)
        })
    }
}

/// An octopus's own state, beside its [`Enemy`] one.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Octopus {
    /// Found out: it has pounced or been struck. (It hides again when the
    /// fight is over and it has mended.)
    pub revealed: bool,
    pub arms: Vec<Arm>,
    /// Ticks before another arm may start probing.
    pub next_probe: u32,
    /// The arm whose turn it is to probe next (they take turns).
    pub turn: usize,
    /// The arm holding a crab (None: its body holds it, after a pounce).
    pub holding: Option<usize>,
}

impl Octopus {
    pub fn new(t: &Tuning) -> Self {
        let a = &t.octopus.arms;
        let arm = Arm {
            health: a.health,
            state: ArmState::Curled,
            tick: 0,
            tip: Vec2::ZERO,
            target: Vec2::ZERO,
            from: Vec2::ZERO,
        };
        Self {
            revealed: false,
            arms: vec![arm; a.count],
            next_probe: 0,
            turn: 0,
            holding: None,
        }
    }

    /// Each arm's state as one letter (see [`ArmState::letter`]).
    pub fn arm_letters(&self) -> String {
        self.arms.iter().map(|a| a.state.letter()).collect()
    }
}

/// The finer part of its state, for dumps and the drawing.
pub fn phase(e: &Enemy, o: &Octopus) -> &'static str {
    match e.state {
        State::Idle if !o.revealed => "hidden",
        State::Idle => "den",
        State::Tell => "flush",
        State::Attack => "pounce",
        s => s.name(),
    }
}

/// Where its arms grow from: a ring under the head, in front of its middle.
pub fn root(e: &Enemy, t: &Tuning) -> Vec2 {
    let (ahead, high) = t.octopus.arms.root;
    Vec2::new(e.pos.x + e.facing * (ahead + e.reach), e.pos.y + high)
}

/// Its eye: low while it lies hidden flat on the rock, up once it has risen.
pub fn eye(e: &Enemy, t: &Tuning) -> Vec2 {
    let o = &t.octopus;
    let flat = e.octo.as_ref().is_none_or(|x| !x.revealed) || e.state == State::Tell;
    let (ahead, high) = if flat { o.eye_hidden } else { o.eye_up };
    Vec2::new(e.pos.x + e.facing * (ahead + e.reach), e.pos.y + high)
}

/// The middle of its mantle and head (an ellipse standing on the floor).
pub fn mantle(e: &Enemy, t: &Tuning) -> Vec2 {
    let h = t.octopus.base.body.1;
    Vec2::new(e.pos.x + e.facing * e.reach, e.pos.y + h / 2.0)
}

/// Which part of it a claw's box reaches: an eye first, then the mantle,
/// then an arm that is out.
pub fn meets(e: &Enemy, hitbox: Rect, t: &Tuning) -> Option<Part> {
    let o = e.octo.as_ref()?;
    let ot = &t.octopus;
    if circle_meets(eye(e, t), ot.eye_radius, hitbox) {
        return Some(Part::Eye);
    }
    let (w, h) = ot.base.body;
    if ellipse_meets(mantle(e, t), Vec2::new(w, h) / 2.0, hitbox) {
        return Some(Part::Body);
    }
    let root = root(e, t);
    o.arms
        .iter()
        .enumerate()
        .filter(|(_, arm)| arm.out(&ot.arms))
        .find(|(_, arm)| {
            arm.circles(root, &ot.arms)
                .any(|(c, r)| circle_meets(c, r, hitbox))
        })
        .map(|(i, _)| Part::Arm(i as u8))
}

/// Does an ellipse (centre, half-sizes) touch a box?
fn ellipse_meets(c: Vec2, half: Vec2, b: Rect) -> bool {
    // Squash the picture so the ellipse is a circle of radius `half.x`.
    let squash = half.x / half.y;
    let b = Rect::new(
        b.min.x,
        c.y + (b.min.y - c.y) * squash,
        b.max.x,
        c.y + (b.max.y - c.y) * squash,
    );
    circle_meets(c, half.x, b)
}

/// It was struck (it's alive and on its feet): found out, it gives up
/// hiding, or a pounce it was about to make, and fights from its den.
pub fn struck(e: &mut Enemy) {
    if matches!(e.state, State::Idle | State::Tell) {
        e.enter(State::Probe, 0);
    }
}

/// A blow reaches arm `i`: its own health. Hurt, it withdraws (letting go of
/// what it held); at 0 it is pulled back for the rest of the fight. The
/// blow's poise damage is the whole animal's.
pub fn arm_takes(e: &mut Enemy, i: usize, hit: &Hit, t: &Tuning) -> Option<combat::Took> {
    let k = &t.octopus.base;
    let o = e.octo.as_mut()?;
    let arm = o.arms.get_mut(i).filter(|a| a.state != ArmState::Lost)?;
    arm.health = (arm.health - hit.damage).max(0.0);
    arm.set(if arm.health > 0.0 {
        ArmState::Back
    } else {
        ArmState::Lost
    });
    o.revealed = true;
    let let_go = o.holding == Some(i) && e.state == State::Grab;
    if let_go {
        o.holding = None;
    }
    e.vitals.hitstop = e.vitals.hitstop.max(hit.hitstop);
    let took = e.vitals.take(0.0, hit.poise, &k.vitals);
    if took.broke_poise {
        e.enter(State::Stagger, k.vitals.stagger_ticks);
    } else if let_go {
        e.enter(State::Recover, k.recover_ticks);
    } else {
        struck(e);
    }
    Some(took)
}

/// What a tip's touch did to it, judged on a co-op friend's device (the
/// oldest arm out takes it).
pub fn tip_feels(e: &mut Enemy, outcome: Outcome, t: &Tuning) {
    let Some(o) = e.octo.as_mut() else { return };
    let Some(i) = o
        .arms
        .iter()
        .position(|a| matches!(a.state, ArmState::Slide | ArmState::Feel))
    else {
        return;
    };
    match outcome {
        Outcome::Evaded | Outcome::Ignored => {}
        Outcome::Parried => {
            o.arms[i].set(ArmState::Recoil);
            e.vitals.hitstop = e.vitals.hitstop.max(t.parry_hitstop_ticks);
        }
        Outcome::Blocked(_) => o.arms[i].set(ArmState::Back),
        Outcome::Hit(took) if !took.died => {
            o.arms[i].set(ArmState::Hold);
            o.holding = Some(i);
            e.enter(State::Grab, 0);
        }
        Outcome::Hit(_) => o.arms[i].set(ArmState::Back),
    }
}

/// Advance an octopus by one tick (after hit-stop; see [`enemy::step`]).
pub fn step(
    e: &mut Enemy,
    crabs: &mut [SimCrab],
    t: &Tuning,
    _level: &Level,
) -> Vec<(PlayerId, Outcome)> {
    let mut blows = Vec::new();
    let k = &t.octopus.base;
    let a = &t.octopus.arms;
    e.vitals.tick(&k.vitals);
    // It keeps to its den: a blow doesn't shove it out.
    e.vel_x = 0.0;

    // Timed states end, and the next one starts on this same tick.
    if e.state_len > 0 && e.state_tick >= e.state_len {
        match e.state {
            State::Tell => {
                e.landed = false;
                e.struck.clear();
                e.enter(State::Attack, k.active_ticks);
            }
            State::Attack => e.enter(State::Recover, k.recover_ticks),
            State::Recover | State::Stagger => e.enter(State::Probe, 0),
            _ => {}
        }
    }
    e.state_tick += 1;

    // The body: out with the pounce, held there while it holds on or
    // recovers, then drawn back into the den.
    e.reach = match e.state {
        State::Attack => {
            k.strike.lunge * (e.state_tick as f32 / k.active_ticks.max(1) as f32).min(1.0)
        }
        State::Grab | State::Recover | State::Stagger => e.reach,
        _ => (e.reach - k.strike.lunge / HEAD_BACK_TICKS).max(0.0),
    };

    // Whom it watches: a crab it has seen, until that one is gone or far
    // from its den; else the nearest it sees.
    let home = e.home.x;
    let kept = e
        .target
        .and_then(|id| crabs.iter().find(|c| c.id == id && huntable(c)))
        .filter(|c| (c.mover.pos.x - home).abs() <= k.give_up_range)
        .map(|c| c.id);
    e.target = kept.or_else(|| e.sense(crabs, k.notice_range));
    let target = e
        .target
        .and_then(|id| crabs.iter().find(|c| c.id == id))
        .map(|c| c.mover.pos);

    // Not holding on: anything it held is let go.
    if e.state != State::Grab {
        for c in crabs.iter_mut().filter(|c| !c.remote) {
            if c.fighter.grabbed.is_some_and(|g| g.by == e.id) {
                c.fighter.grabbed = None;
                c.events.freed = true;
            }
        }
    }

    let Some(mut o) = e.octo.take() else {
        return blows;
    };
    let root_now = root(e, t);
    let in_reach = |p: Vec2| (p.x - root_now.x).abs() <= a.length && (p.y - e.pos.y).abs() < 200.0;
    match e.state {
        State::Idle => {
            if !o.revealed {
                // A rock that looks back: it pounces on a crab passing close
                // to its front.
                if let Some(p) = target {
                    let gap = (p.x - e.pos.x).abs() - k.body.0 / 2.0;
                    if gap <= k.strike_range && (p.y - e.pos.y).abs() < k.body.1 {
                        e.facing = (p.x - e.pos.x).signum();
                        o.revealed = true;
                        o.holding = None;
                        e.enter_now(State::Tell, k.tell_ticks);
                    }
                }
            } else if let Some(p) = target.filter(|p| in_reach(*p)) {
                e.facing = (p.x - e.pos.x).signum();
                e.enter_now(State::Probe, 0);
            } else if e.sense(crabs, k.give_up_range).is_none() && e.vitals.unhit > MEND_AFTER {
                // The fight is over: it mends, and is a rock again.
                e.vitals = combat::Vitals::new(&k.vitals);
                o = Octopus::new(t);
            }
        }
        State::Attack => {
            e.octo = Some(o);
            enemy::strike(e, crabs, t, k, &mut blows);
            o = e.octo.take().expect("put back just now");
        }
        State::Probe => match target.filter(|p| in_reach(*p)) {
            None => e.enter_now(State::Idle, 0),
            Some(p) => {
                if p.x != e.pos.x {
                    e.facing = (p.x - e.pos.x).signum();
                }
                probe(e, &mut o, crabs, t);
            }
        },
        State::Grab => {
            let held = crabs
                .iter()
                .any(|c| c.fighter.grabbed.is_some_and(|g| g.by == e.id));
            if !held {
                o.holding = None;
                e.enter_now(State::Recover, k.recover_ticks);
            }
        }
        State::Tell
        | State::Recover
        | State::Stagger
        | State::Gone
        | State::Notice
        | State::Approach
        | State::Flee => {}
    }

    move_arms(e, &mut o, crabs, t);
    if e.state == State::Probe {
        touch(e, &mut o, crabs, t, &mut blows);
    }
    e.octo = Some(o);
    blows
}

/// In its den, the crab in reach: send another arm if fewer than two are
/// out and the last one went a moment ago.
fn probe(e: &Enemy, o: &mut Octopus, crabs: &[SimCrab], t: &Tuning) {
    let a = &t.octopus.arms;
    o.next_probe = o.next_probe.saturating_sub(1);
    let out = o
        .arms
        .iter()
        .filter(|arm| matches!(arm.state, ArmState::Slide | ArmState::Feel))
        .count();
    if out >= a.at_once || o.next_probe > 0 {
        return;
    }
    let Some(crab) = e.target.and_then(|id| crabs.iter().find(|c| c.id == id)) else {
        return;
    };
    let n = o.arms.len();
    let Some(i) = (0..n)
        .map(|k| (o.turn + k) % n)
        .find(|&i| o.arms[i].state == ArmState::Curled)
    else {
        return;
    };
    // To where its tip just touches the crab's near side (so the touch comes
    // at the end of the slide), as far as the arm reaches.
    let root = root(e, t);
    let body = combat::body(&crab.mover, t);
    let side = if crab.mover.pos.x >= root.x {
        1.0
    } else {
        -1.0
    };
    let edge = if side > 0.0 { body.min.x } else { body.max.x };
    let mut target = Vec2::new(
        edge - side * (a.tip_radius - 1.0),
        crab.mover.pos.y + a.height,
    );
    if root.distance(target) > a.length {
        target = root + (target - root).normalize_or_zero() * a.length;
    }
    let arm = &mut o.arms[i];
    arm.tip = root;
    arm.set(ArmState::Slide);
    arm.target = target;
    o.turn = i + 1;
    o.next_probe = a.gap_ticks;
}

/// Each arm's tip moves on: sliding in, feeling about, drawing back, or with
/// the crab it holds.
fn move_arms(e: &Enemy, o: &mut Octopus, crabs: &[SimCrab], t: &Tuning) {
    let a = &t.octopus.arms;
    let root = root(e, t);
    let probing = e.state == State::Probe;
    let held = crabs
        .iter()
        .find(|c| c.fighter.grabbed.is_some_and(|g| g.by == e.id))
        .map(|c| c.mover.pos + Vec2::new(0.0, a.height));
    let frac = |tick: u32, len: u32| (tick as f32 / len.max(1) as f32).min(1.0);
    for arm in &mut o.arms {
        arm.tick += 1;
        match arm.state {
            ArmState::Curled | ArmState::Lost => arm.tip = root,
            ArmState::Slide if !probing => arm.set(ArmState::Back),
            ArmState::Slide => {
                if arm.tick > a.tell_ticks {
                    arm.set(ArmState::Feel);
                    arm.tip = arm.target;
                } else {
                    arm.tip = arm.from.lerp(arm.target, frac(arm.tick, a.tell_ticks));
                }
            }
            ArmState::Feel => {
                if !probing || arm.tick > a.feel_ticks {
                    arm.set(ArmState::Back);
                }
            }
            ArmState::Hold => match held.filter(|_| e.state == State::Grab) {
                Some(p) => arm.tip = p,
                None => arm.set(ArmState::Back),
            },
            ArmState::Back => {
                arm.tip = arm.from.lerp(root, frac(arm.tick, a.back_ticks));
                if arm.tick >= a.back_ticks {
                    arm.set(ArmState::Curled);
                }
            }
            ArmState::Recoil => {
                arm.tip = arm.from.lerp(root, frac(arm.tick, a.snap_ticks));
                if arm.tick >= a.recoil_ticks {
                    arm.set(ArmState::Curled);
                }
            }
        }
    }
}

/// A tip out probing touches a crab: a parry makes it recoil, a block
/// pushes it back, and if it gets through it grabs. (Only crabs run on this
/// device: the defender decides.)
fn touch(
    e: &mut Enemy,
    o: &mut Octopus,
    crabs: &mut [SimCrab],
    t: &Tuning,
    blows: &mut Vec<(PlayerId, Outcome)>,
) {
    let a = &t.octopus.arms;
    let grab = t.octopus.base.strike.grab;
    for i in 0..o.arms.len() {
        if !matches!(o.arms[i].state, ArmState::Slide | ArmState::Feel) {
            continue;
        }
        for crab in crabs.iter_mut() {
            let free = crab.fighter.dead.is_none() && crab.fighter.grabbed.is_none();
            let body = combat::body(&crab.mover, t);
            if crab.remote || !free || !circle_meets(o.arms[i].tip, a.tip_radius, body) {
                continue;
            }
            let hit = Hit {
                damage: a.damage,
                poise: a.poise,
                source: Source::Enemy(e.id),
                hitstop: a.hitstop_ticks,
                parryable: true,
                blockable: true,
                ..Hit::default()
            };
            let outcome = combat::crab_defends(
                &mut crab.fighter,
                &mut crab.mover,
                &hit,
                t,
                &mut crab.events,
            );
            let arm = &mut o.arms[i];
            match outcome {
                // Not yet: it may still touch next tick.
                Outcome::Evaded | Outcome::Ignored => continue,
                Outcome::Parried => {
                    arm.set(ArmState::Recoil);
                    e.vitals.hitstop = e.vitals.hitstop.max(t.parry_hitstop_ticks);
                }
                Outcome::Blocked(_) => arm.set(ArmState::Back),
                Outcome::Hit(took) => match grab.filter(|_| !took.died) {
                    Some(g) => {
                        crab.fighter.grabbed = Some(Grabbed {
                            by: e.id,
                            left: g.ticks,
                            struggle: g.struggle,
                        });
                        crab.mover.vel = Vec2::ZERO;
                        arm.set(ArmState::Hold);
                        o.holding = Some(i);
                        e.enter_now(State::Grab, 0);
                        blows.push((crab.id, outcome));
                        return;
                    }
                    None => arm.set(ArmState::Back),
                },
            }
            blows.push((crab.id, outcome));
            break;
        }
    }
}
