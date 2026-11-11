//! Levels and gear: Elden Ring scaling (the owner's rule). They lower the
//! skill a fight needs but never remove it: more shell, harder hits and
//! slightly wider windows are fine, but a fight never becomes click-and-see,
//! and an under-levelled crab can still win on skill.
//!
//! What is enforced here:
//! 1. plating and food together cut damage by at most 40% (factor >= 0.6);
//! 2. claw tips and food together hit at most 1.35 times as hard (the
//!    Fighting level's own cap is separate);
//! 3. no tell, telegraph, parry or dodge window is read from the economy: the
//!    tuning is the same for a bare crab and a geared one, and the enemies'
//!    tells run the same ticks against both;
//! 4. a level 1 crab with no gear beats the snapping shrimp and the mantis
//!    shrimp with a parry and a counter (a scripted crab, below).

use bevy_math::Vec2;
use crabsim::economy::food::Buff;
use crabsim::economy::{Economy, Gear, MAX_CLAW_FACTOR, MAX_DAMAGE_CUT_PERCENT};
use crabsim::enemy::State;
use crabsim::{Controls, Kind, PlayerId, Sim, Skill, Tuning};

const P: PlayerId = PlayerId(0);
const BUFFS: [Buff; 7] = [
    Buff::Gather10,
    Buff::Gather15,
    Buff::ReserveQuick,
    Buff::Pressure30,
    Buff::Claws15,
    Buff::Pressure20,
    Buff::Feast,
];

#[test]
fn plating_and_food_never_cut_damage_below_sixty_percent() {
    assert_eq!(MAX_DAMAGE_CUT_PERCENT, 40);
    let mut worst = 1.0f32;
    for plating in Gear::ALL {
        for buff in std::iter::once(None).chain(BUFFS.map(Some)) {
            let mut e = Economy::default();
            e.gear.body = Some((plating, 100));
            e.buff = buff.map(|b| (b, 1000));
            let f = e.damage_taken_factor();
            assert!(f >= 0.6 - 1e-6, "{plating:?} + {buff:?}: {f}");
            worst = worst.min(f);
        }
    }
    println!("best damage cut: factor {worst}");
    assert!(worst < 1.0, "plating does cut something");
}

#[test]
fn claw_tips_and_food_never_hit_harder_than_one_point_three_five() {
    assert!((MAX_CLAW_FACTOR - 1.35).abs() < 1e-6);
    let mut best = 1.0f32;
    for tips in Gear::ALL {
        for buff in std::iter::once(None).chain(BUFFS.map(Some)) {
            let mut e = Economy::default();
            e.gear.claws = Some(tips);
            e.buff = buff.map(|b| (b, 1000));
            let f = e.claw_bonus();
            assert!(f <= 1.35 + 1e-6, "{tips:?} + {buff:?}: {f}");
            best = best.max(f);
        }
    }
    // The cap bites: the best tips with the grenadier ferment would be 1.357.
    let mut e = Economy::default();
    e.gear.claws = Some(Gear::FangTips);
    e.buff = Some((Buff::Claws15, 1000));
    assert_eq!(e.claw_bonus(), 1.35);
    assert_eq!(best, 1.35);
    // Below the cap nothing changes.
    e.gear.claws = Some(Gear::BeakTips);
    assert!((e.claw_bonus() - 1.1 * 1.15).abs() < 1e-6);
}

/// A crab at the foot of a foe, bare (level 1, no gear) or as strong as the
/// economy can make it.
fn arena(kind: Kind, gap: f32, geared: bool) -> Sim {
    let mut sim = Sim::with_one_crab(Vec2::new(1000.0, 110.0));
    sim.spawn_enemy(kind, Vec2::new(1000.0 + gap, 110.0));
    if geared {
        let econ = &mut sim.crab_mut(P).expect("crab").econ;
        for s in Skill::ALL {
            econ.skills.set(s, 13_034_431);
        }
        econ.gear.body = Some((Gear::ScaleMail, 1000));
        econ.gear.claws = Some(Gear::FangTips);
        econ.buff = Some((Buff::Feast, 1000));
    }
    for _ in 0..30 {
        sim.tick();
    }
    sim
}

fn foe(sim: &Sim, kind: Kind) -> &crabsim::Enemy {
    sim.enemies()
        .iter()
        .find(|e| e.kind == kind)
        .expect("the foe")
}

/// The tick lengths of each tell the foe goes through while the crab stands
/// still, and the ticks between them.
fn tells(sim: &mut Sim, kind: Kind) -> Vec<(u32, u32)> {
    let mut out = Vec::new();
    let (mut last, mut len, mut at) = (State::Idle, 0, 0u32);
    for tick in 0..2400u32 {
        // Walk at the foe for a third of a second so it notices (the snapper
        // feels only steps), then stand and take what comes.
        let walk = if tick < 40 { 0.8 } else { 0.0 };
        sim.set_controls(
            P,
            Controls {
                walk,
                ..Default::default()
            },
        );
        sim.tick();
        let e = foe(sim, kind);
        if e.state == State::Tell && last != State::Tell {
            at = tick;
        }
        if e.state == State::Tell {
            len = e.state_tick;
        }
        if last == State::Tell && e.state != State::Tell {
            out.push((at, len));
        }
        last = e.state;
    }
    out
}

#[test]
fn tells_and_windows_do_not_depend_on_economy_gear_or_levels() {
    // The windows live in `Tuning` (parry, dodge, each foe's tell); the
    // economy only multiplies damage taken and given, so building every
    // kind of crab leaves the tuning the same.
    let base = Tuning::default();
    for geared in [false, true] {
        for kind in [Kind::Snapper, Kind::Mantis] {
            let mut sim = arena(kind, 100.0, geared);
            assert_eq!(sim.tuning, base, "the economy rewrote the tuning");
            assert_eq!(sim.tuning.parry_window_ticks, base.parry_window_ticks);
            assert_eq!(sim.tuning.dodge_invulnerable, base.dodge_invulnerable);
            let _ = tells(&mut sim, kind);
        }
    }
    // The same foe tells the same ticks to a bare crab and a geared one.
    for kind in [Kind::Snapper, Kind::Mantis] {
        let mut bare = arena(kind, 100.0, false);
        let mut geared = arena(kind, 100.0, true);
        let (a, b) = (tells(&mut bare, kind), tells(&mut geared, kind));
        println!("{kind:?} tells (start, ticks): {a:?}");
        assert!(!a.is_empty(), "{kind:?} told nothing in twenty seconds");
        assert_eq!(a, b, "{kind:?}: the economy changed a tell");
    }
}

/// A crab that plays the fight: walks up, reads the tell, answers it (a
/// parry at the end of the mantis's tell, a dodge through the snapper's
/// shock, which a parry cannot stop) and punishes the foe in its recovery or
/// stagger with a smash. Whether it ends the foe, and with what health.
fn skilled_win(kind: Kind, geared: bool) -> (bool, f32) {
    let mut sim = arena(kind, 190.0, geared);
    for _ in 0..9000u32 {
        let e = foe(&sim, kind);
        let me = sim.crab(P).expect("crab").mover.pos.x;
        let dir = (e.pos.x - me).signum();
        let gap = (e.pos.x - me).abs();
        let close = gap < 70.0;
        let answer = e.state == State::Tell && e.state_tick + 4 >= e.state_len;
        let punish = matches!(e.state, State::Recover | State::Stagger);
        let shock = kind == Kind::Snapper;
        sim.set_controls(
            P,
            Controls {
                walk: if answer && shock {
                    -dir
                } else if close || answer {
                    0.0
                } else {
                    0.8 * dir
                },
                dodge_pressed: answer && shock,
                smash_pressed: close && punish && e.state_tick.is_multiple_of(8),
                jab_pressed: !shock
                    && close
                    && !answer
                    && !punish
                    && e.state_tick.is_multiple_of(30),
                block_held: answer && !shock,
                block_pressed: answer && !shock,
                ..Default::default()
            },
        );
        sim.tick();
        if sim.crab(P).expect("crab").fighter.vitals.health <= 0.0 {
            return (false, 0.0);
        }
        if foe(&sim, kind).state == State::Gone {
            break;
        }
    }
    let alive = sim.crab(P).expect("crab").fighter.vitals.health;
    let e = foe(&sim, kind);
    (e.state == State::Gone, alive)
}

#[test]
fn a_level_one_crab_with_no_gear_still_beats_the_snapper_and_the_mantis_on_skill() {
    for kind in [Kind::Snapper, Kind::Mantis] {
        let (won, health) = skilled_win(kind, false);
        println!("{kind:?}: won {won}, {health} health left");
        assert!(won && health > 0.0, "{kind:?}");
    }
}
