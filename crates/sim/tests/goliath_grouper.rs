//! The goliath grouper: a boom, a slow tell, a suction gulp. Headless scenarios.
use bevy_math::Vec2;
use crabsim::combat::{self, Hit, Source};
use crabsim::creatures::{Extra, grouper};
use crabsim::enemy::State;
use crabsim::level::landmarks::Kind as Place;
use crabsim::net::FakeLink;
use crabsim::world::cues::Cue;
use crabsim::{Controls, Kind, Level, PlayerId, Sim, Tuning};

const P: PlayerId = PlayerId(0);
const HOST: PlayerId = PlayerId(0);
const GUEST: PlayerId = PlayerId(1);

/// Its home with the mouth where a crab on the floor meets it.
fn meeting_home() -> Vec2 {
    let level = Level::test_level();
    let h = home();
    Vec2::new(h.x, grouper::meeting_height(&level, h.x))
}

fn home() -> Vec2 {
    grouper::spots(&Level::test_level())[0]
}

/// A crab `ahead` units west of the mouth, standing on the floor, with the
/// grouper at its real place.
fn encounter(ahead: f32) -> Sim {
    let level = Level::test_level();
    let h = home();
    let x = h.x - grouper::MOUTH_X - ahead;
    let mut sim = Sim::with_one_crab(Vec2::new(x, level.floor_height(x)));
    // The meeting height puts the mouth where a crab on the floor meets it.
    let at = Vec2::new(h.x, grouper::meeting_height(&level, h.x));
    sim.spawn_enemy(Kind::GoliathGrouper, at);
    sim
}

fn tick(sim: &mut Sim, controls: Controls) {
    sim.set_controls(P, controls);
    sim.tick();
}

fn health(sim: &Sim) -> f32 {
    sim.crab(P).unwrap().fighter.vitals.health
}

fn middle(sim: &Sim) -> Vec2 {
    combat::body(&sim.crab(P).unwrap().mover, &sim.tuning).center()
}

fn mouth(sim: &Sim) -> Vec2 {
    grouper::mouth(&sim.enemies()[0])
}

/// A grouper whose mouth hangs just over flat floor, for the close-in
/// mechanics, with a crab `ahead` units in front of the mouth.
fn close(ahead: f32) -> Sim {
    let level = Level::test_level();
    let mouth_x = 2200.0;
    let floor = level.floor_height(mouth_x - ahead);
    let home = Vec2::new(mouth_x + grouper::MOUTH_X, floor + 30.0 - grouper::MOUTH_Y);
    let mut sim = Sim::with_one_crab(Vec2::new(mouth_x - ahead, floor));
    sim.spawn_enemy(Kind::GoliathGrouper, home);
    sim
}

fn grouper_state(sim: &Sim) -> grouper::State {
    match &sim.enemies()[0].creature {
        Extra::Grouper(s) => s.clone(),
        other => panic!("not a grouper: {other:?}"),
    }
}

#[test]
fn it_lives_at_its_place_in_the_authored_level() {
    let level = Level::test_level();
    let spots = grouper::spots(&level);
    assert_eq!(spots.len(), 1);
    assert!(level.foes.contains(&(Kind::GoliathGrouper, spots[0])));
    // Not inside any refuge, tunnel, mouth or chamber.
    let body_middle = spots[0] + Vec2::new(0.0, grouper::BODY.1 * 0.5);
    for l in level.landmarks() {
        if matches!(
            l.kind,
            Place::Refuge | Place::Tunnel | Place::Mouth | Place::Chamber { .. }
        ) {
            assert!(!l.rect.contains(body_middle), "{}", l.name);
            assert!(!l.rect.contains(spots[0] + Vec2::Y * 100.0), "{}", l.name);
        }
    }
    // No other resident within its cone of the mouth.
    let mouth = spots[0] + Vec2::new(-grouper::MOUTH_X, grouper::MOUTH_Y);
    for (kind, at) in &level.foes {
        if *kind != Kind::GoliathGrouper {
            assert!(at.distance(mouth) > grouper::CONE_LEN, "{kind:?} at {at:?}");
        }
    }
    assert!(spots[0].y >= level.floor_height(spots[0].x) + grouper::ABOVE_FLOOR);
    let prize = grouper::prize(&level, spots[0]);
    assert_eq!(prize.y, level.floor_height(prize.x));
    assert!(prize.distance(spots[0]) < 800.0);
}

#[test]
fn the_boom_comes_before_any_gulp_and_hurts_nothing() {
    let mut sim = encounter(300.0);
    let start = health(&sim);
    let mut boom = 0;
    for _ in 0..400 {
        tick(&mut sim, Controls::default());
        let e = &sim.enemies()[0];
        match e.state {
            State::Notice => {
                assert_eq!(e.phase(&sim.tuning), "boom");
                boom += 1;
            }
            State::Tell | State::Attack | State::Grab => {
                assert_eq!(boom, grouper::BOOM_TICKS, "the boom is the first thing");
                return;
            }
            State::Idle => assert_eq!(boom, 0),
            _ => {}
        }
        assert_eq!(health(&sim), start, "the boom does no damage");
    }
    panic!("no gulp came");
}

#[test]
fn the_tell_lasts_1_2_seconds_swells_and_the_gulp_is_12_ticks() {
    let mut sim = encounter(300.0);
    let (mut tell, mut gulp) = (0, 0);
    let mut swell = None;
    let start = health(&sim);
    for _ in 0..700 {
        // Backing away through the tell, out of the cone: the gulp then takes
        // all its 12 ticks (a crab it engulfed would end it in the tenth).
        let back = sim.enemies()[0].state == State::Tell;
        tick(
            &mut sim,
            Controls {
                walk: if back { -1.0 } else { 0.0 },
                ..Default::default()
            },
        );
        match sim.enemies()[0].state {
            State::Tell => {
                tell += 1;
                assert_eq!(health(&sim), start, "no blow in the tell");
            }
            State::Attack => gulp += 1,
            _ => {}
        }
        for event in sim.cues() {
            if let Cue::Tell { crab, strike_tick } = event.cue {
                swell = Some((crab, strike_tick));
            }
        }
        if gulp > 0 && sim.enemies()[0].state != State::Attack {
            break;
        }
    }
    assert_eq!(tell, grouper::TELL_TICKS);
    assert_eq!(gulp, grouper::GULP_TICKS);
    assert!(swell.is_some_and(|(crab, _)| crab == P), "{swell:?}");
}

#[test]
fn each_boom_cues_one_deep_thump_before_the_tell() {
    let mut sim = encounter(300.0);
    let (mut booms, mut boom_tick, mut tell_tick) = (0, None, None);
    for _ in 0..600 {
        tick(&mut sim, Controls::default());
        let now = sim.tick_count();
        for event in sim.cues().iter().filter(|e| e.tick == now) {
            match event.cue {
                Cue::Boom { at } => {
                    booms += 1;
                    boom_tick = Some(now);
                    assert_eq!(at, sim.enemies()[0].pos);
                }
                Cue::Tell { .. } => tell_tick = tell_tick.or(Some(now)),
                _ => {}
            }
        }
        if tell_tick.is_some() {
            break;
        }
    }
    assert_eq!(booms, 1, "exactly one Boom per boom");
    assert!(boom_tick < tell_tick, "{boom_tick:?} before {tell_tick:?}");
}

/// Put it mid-gulp aiming straight at the crab (or `turn` radians away).
fn force_gulp(sim: &mut Sim, turn: f32) {
    let at = middle(sim);
    let e = &mut sim.enemies_mut()[0];
    let dir = (at - grouper::mouth(e)).normalize();
    let (s, c) = turn.sin_cos();
    let aim = Vec2::new(dir.x * c - dir.y * s, dir.x * s + dir.y * c);
    e.target = Some(P);
    e.state = State::Attack;
    e.state_tick = 0;
    e.state_len = grouper::GULP_TICKS;
    e.creature = Extra::Grouper(grouper::State {
        out: -1.0,
        aim,
        ..Default::default()
    });
}

fn pulled_by(mut sim: Sim, turn: f32, block: bool) -> f32 {
    force_gulp(&mut sim, turn);
    let before = middle(&sim).distance(mouth(&sim));
    for _ in 0..6 {
        tick(
            &mut sim,
            Controls {
                block_held: block,
                ..Default::default()
            },
        );
    }
    before - middle(&sim).distance(mouth(&sim))
}

#[test]
fn the_pull_happens_only_in_the_cone_and_a_brace_cuts_it() {
    let open = pulled_by(encounter(250.0), 0.0, false);
    let braced = pulled_by(encounter(250.0), 0.0, true);
    let outside = pulled_by(encounter(250.0), 0.8, false);
    assert!(open > 60.0, "pulled {open}");
    assert!(
        braced < open * 0.5 && braced > 0.0,
        "braced {braced} vs open {open}"
    );
    assert!(outside.abs() < 5.0, "outside the cone: {outside}");
}

#[test]
fn an_engulfed_crab_is_held_bitten_to_its_cap_and_spat_out() {
    let mut sim = encounter(220.0);
    let start = health(&sim);
    let mut held = 0;
    let mut spat = false;
    let mut before_grab = start;
    for _ in 0..1500 {
        let state = sim.enemies()[0].state;
        if state != State::Grab {
            before_grab = health(&sim);
        }
        tick(&mut sim, Controls::default());
        let e = &sim.enemies()[0];
        if e.state == State::Grab {
            held += 1;
            assert!(sim.crab(P).unwrap().fighter.grabbed.is_some());
            assert_eq!(e.phase(&sim.tuning), "engulf");
        }
        if state == State::Grab && e.state == State::Recover {
            spat = true;
            let cost = before_grab - health(&sim);
            assert!(
                (grouper::ENGULF_DAMAGE..=grouper::ENGULF_MAX).contains(&cost),
                "an engulf cost {cost}"
            );
            assert!(sim.crab(P).unwrap().fighter.grabbed.is_none());
            assert!(sim.crab(P).unwrap().mover.vel.x < 0.0, "spat out ahead");
            break;
        }
    }
    assert!(held > 100 && spat, "held {held} ticks, spat {spat}");
}

#[test]
fn struggling_frees_the_crab_sooner() {
    let mut sim = encounter(220.0);
    let mut held = 0;
    for i in 0..1500 {
        let grabbed = sim.crab(P).unwrap().fighter.grabbed.is_some();
        tick(
            &mut sim,
            Controls {
                dodge_pressed: grabbed && i % 6 == 0,
                ..Default::default()
            },
        );
        held += u32::from(sim.enemies()[0].state == State::Grab);
        if sim.enemies()[0].state == State::Recover {
            break;
        }
    }
    assert!(held > 0 && held < 60, "struggled free after {held}");
    assert!(health(&sim) >= 100.0 - grouper::ENGULF_MAX);
}

#[test]
fn walking_out_of_the_cone_takes_nothing() {
    let mut sim = encounter(300.0);
    let start = health(&sim);
    let mut gulped = false;
    for _ in 0..800 {
        let e = &sim.enemies()[0];
        // Back away during the tell: the cone reaches 520 units, so it takes
        // a few steps, well before the lock.
        let go = e.state == State::Tell;
        tick(
            &mut sim,
            Controls {
                walk: if go || gulped { -1.0 } else { 0.0 },
                ..Default::default()
            },
        );
        gulped |= sim.enemies()[0].state == State::Attack;
        assert!(sim.crab(P).unwrap().fighter.grabbed.is_none());
    }
    assert!(gulped);
    assert_eq!(health(&sim), start);
}

#[test]
fn a_dodge_through_the_gulp_takes_nothing() {
    let mut sim = encounter(220.0);
    let start = health(&sim);
    let mut recovered = false;
    for _ in 0..800 {
        let e = &sim.enemies()[0];
        let dodge = e.state == State::Tell && e.state_tick + 1 == e.state_len;
        tick(
            &mut sim,
            Controls {
                dodge_pressed: dodge,
                ..Default::default()
            },
        );
        assert!(sim.crab(P).unwrap().fighter.grabbed.is_none());
        if sim.enemies()[0].state == State::Recover {
            recovered = true;
            break;
        }
    }
    assert!(recovered);
    assert_eq!(health(&sim), start);
}

#[test]
fn a_braced_crab_at_the_mouth_is_not_swallowed() {
    let mut sim = encounter(220.0);
    let start = health(&sim);
    let mut recovered = false;
    for _ in 0..800 {
        tick(
            &mut sim,
            Controls {
                block_held: true,
                ..Default::default()
            },
        );
        assert!(sim.crab(P).unwrap().fighter.grabbed.is_none());
        if sim.enemies()[0].state == State::Recover {
            recovered = true;
            break;
        }
    }
    assert!(recovered);
    assert!(health(&sim) > start - 2.0, "a brace costs a chip at most");
}

#[test]
fn a_smash_to_the_lip_during_the_pull_staggers_it() {
    let mut staggered = false;
    // The smash has a 14-tick windup: find the press that lands in the pull.
    for early in 4..24u32 {
        let mut sim = close(90.0);
        let start = health(&sim);
        let mut grabbed = false;
        for _ in 0..600 {
            let e = &sim.enemies()[0];
            let press = e.state == State::Tell && e.state_tick + early == e.state_len;
            tick(
                &mut sim,
                Controls {
                    smash_pressed: press,
                    ..Default::default()
                },
            );
            grabbed |= sim.crab(P).unwrap().fighter.grabbed.is_some();
            let e = &sim.enemies()[0];
            if e.state == State::Stagger && e.state_len == grouper::LIP_STAGGER {
                staggered = !grabbed && health(&sim) == start;
                break;
            }
            if matches!(e.state, State::Recover | State::Grab) {
                break;
            }
        }
        if staggered {
            break;
        }
    }
    assert!(staggered, "no smash staggered it");
}

#[test]
fn its_poise_breaks_under_the_crabs_blows() {
    let mut sim = encounter(300.0);
    let t = sim.tuning.clone();
    let hit = Hit::new(t.crusher.damage, t.crusher.poise, Source::Crab(P));
    let x = home().x - 200.0;
    let mut broke = false;
    for _ in 0..12 {
        let took = sim.enemies_mut()[0].takes(&hit, x, &t).unwrap();
        if took.broke_poise {
            broke = true;
            break;
        }
    }
    assert!(broke);
    let e = &sim.enemies()[0];
    assert_eq!(e.state, State::Stagger);
    assert_eq!(e.state_len, t.fixture.vitals.stagger_ticks.max(120));
    assert!(
        e.vitals.health < 400.0 && e.vitals.health > 250.0,
        "slow to kill"
    );
    for _ in 0..e.state_len + 2 {
        tick(&mut sim, Controls::default());
    }
    assert_ne!(sim.enemies()[0].state, State::Stagger);
}

#[test]
fn head_hits_count_more_while_it_recovers() {
    let t = Tuning::default();
    let mut sim = encounter(300.0);
    let hit = Hit::new(12.0, 0.0, Source::Crab(P));
    let in_front = home().x - 900.0;
    let e = &mut sim.enemies_mut()[0];
    let h0 = e.vitals.health;
    e.takes(&hit, in_front, &t);
    let plain = h0 - e.vitals.health;
    e.state = State::Recover;
    let h1 = e.vitals.health;
    e.takes(&hit, in_front, &t);
    assert!(h1 - e.vitals.health > plain * 1.4);
}

/// A crab at the fish's flank (behind the mouth, inside the body's reach)
/// smashing away drives it off in the end.
#[test]
fn a_crab_can_drive_it_off_and_it_never_strays_from_its_cave() {
    let level = Level::test_level();
    let h = home();
    let x = h.x - 330.0;
    let mut sim = Sim::with_one_crab(Vec2::new(x, level.floor_height(x)));
    sim.spawn_enemy(Kind::GoliathGrouper, meeting_home());
    sim.crab_mut(P).unwrap().fighter.facing = 1.0;
    let mut backed = false;
    let mut left = false;
    let stand = Vec2::new(x, level.floor_height(x));
    for i in 0..9000 {
        // Planted at the flank.
        if i % 4 == 0 {
            let c = sim.crab_mut(P).unwrap();
            c.mover.pos = stand;
            c.mover.vel = Vec2::ZERO;
        }
        tick(
            &mut sim,
            Controls {
                smash_pressed: i % 50 == 0,
                walk: 0.0,
                ..Default::default()
            },
        );
        let e = &sim.enemies()[0];
        let st = grouper_state(&sim);
        if i % 100 == 0 && i < 2000 && std::env::var("DBG").is_ok() {
            println!(
                "i={i} fish={:?} hp={} st={:?} pos={:?}",
                e.state, e.vitals.health, st, e.pos
            );
        }
        if st.retreat || st.leaving {
            backed = true;
        }
        if !st.leaving {
            assert!(
                e.pos.distance(e.home) <= grouper::BACK_DIST + 1.0,
                "strayed {}",
                e.pos.distance(e.home)
            );
        }
        if e.state == State::Gone {
            left = true;
            break;
        }
    }
    assert!(backed, "it never backed out at half health");
    assert!(left, "it never left: {:?}", sim.enemies()[0]);
    assert!(grouper_state(&sim).leaving);
    assert!(health(&sim) > 0.0);
}

#[test]
fn hurt_past_half_it_backs_into_its_cave_and_stops_guarding_for_a_minute() {
    let mut sim = encounter(300.0);
    let t = sim.tuning.clone();
    let hit = Hit::new(40.0, 0.0, Source::Crab(P));
    for _ in 0..6 {
        sim.enemies_mut()[0].takes(&hit, home().x - 900.0, &t);
    }
    assert_eq!(sim.enemies()[0].state, State::Flee);
    // Walk the crab to where it would be noticed, and wait out the retreat.
    let mut rested = 0;
    let mut health_at_rest = None;
    for _ in 0..(grouper::REST_TICKS + 900) {
        tick(&mut sim, Controls::default());
        let e = &sim.enemies()[0];
        if grouper_state(&sim).rest_left > 0 {
            rested += 1;
            assert_eq!(e.state, State::Idle, "it does not guard while backed in");
            assert!(e.target.is_none());
            // Nothing hurts the crab while it rests.
            assert_eq!(health(&sim), *health_at_rest.get_or_insert(health(&sim)));
        }
    }
    assert!(rested >= grouper::REST_TICKS - 5, "rested {rested}");
}

#[test]
fn a_hit_while_it_rests_wakes_it() {
    let mut sim = encounter(300.0);
    let t = sim.tuning.clone();
    let hit = Hit::new(40.0, 0.0, Source::Crab(P));
    for _ in 0..6 {
        sim.enemies_mut()[0].takes(&hit, home().x - 900.0, &t);
    }
    for _ in 0..900 {
        tick(&mut sim, Controls::default());
    }
    assert!(grouper_state(&sim).rest_left > 0);
    sim.enemies_mut()[0].takes(&Hit::new(5.0, 0.0, Source::Crab(P)), home().x - 900.0, &t);
    tick(&mut sim, Controls::default());
    tick(&mut sim, Controls::default());
    assert_eq!(grouper_state(&sim).rest_left, 0);
    assert_eq!(sim.enemies()[0].state, State::Notice);
}

#[test]
fn walking_away_ends_it() {
    let mut sim = encounter(300.0);
    let start = health(&sim);
    for _ in 0..2400 {
        tick(
            &mut sim,
            Controls {
                walk: -1.0,
                ..Default::default()
            },
        );
    }
    let e = &sim.enemies()[0];
    assert_eq!(e.state, State::Idle, "{:?}", e.state);
    assert!(e.target.is_none());
    assert_eq!(health(&sim), start);
    assert!(e.pos.distance(e.home) < 3.0);
}

#[test]
fn it_breaks_off_for_a_crab_holding_coral() {
    let level = Level::test_level();
    let stand = level
        .landmarks()
        .into_iter()
        .filter(|l| l.name.starts_with("coral stand"))
        .min_by(|a, b| {
            (a.rect.max.x - 16_340.0)
                .abs()
                .total_cmp(&(b.rect.max.x - 16_340.0).abs())
        })
        .unwrap();
    let at = Vec2::new(stand.rect.center().x, stand.rect.min.y + 400.0);
    let mut sim = Sim::with_one_crab(at);
    sim.spawn_enemy(Kind::GoliathGrouper, meeting_home());
    // Without holding on, the same crab in the same spot is noticed.
    tick(&mut sim, Controls::default());
    assert_ne!(sim.enemies()[0].state, State::Idle);
    let mut sim = Sim::with_one_crab(at);
    sim.spawn_enemy(Kind::GoliathGrouper, meeting_home());
    let start = health(&sim);
    for _ in 0..600 {
        // Holding on to the coral.
        sim.crab_mut(P).unwrap().mover.climbing = true;
        tick(&mut sim, Controls::default());
        assert!(!matches!(
            sim.enemies()[0].state,
            State::Tell | State::Attack | State::Grab
        ));
    }
    assert_eq!(health(&sim), start);
}

#[test]
fn it_never_follows_into_a_tunnel() {
    let level = Level::test_level();
    let tunnel = level
        .landmarks()
        .into_iter()
        .find(|l| l.name == "crack chimney")
        .unwrap()
        .rect;
    let inside = tunnel.center() - Vec2::new(0.0, 40.0);
    let mut sim = Sim::with_one_crab(inside);
    // Its mouth right at the tunnel's door.
    sim.spawn_enemy(
        Kind::GoliathGrouper,
        Vec2::new(
            tunnel.min.x - 150.0 + grouper::MOUTH_X,
            tunnel.min.y - 160.0,
        ),
    );
    let start = health(&sim);
    for _ in 0..1200 {
        // The crab stays in the tunnel.
        let c = sim.crab_mut(P).unwrap();
        c.mover.pos = inside;
        c.mover.vel = Vec2::ZERO;
        tick(&mut sim, Controls::default());
        let e = &sim.enemies()[0];
        assert!(!tunnel.contains(e.pos + Vec2::new(0.0, 100.0)));
        assert!(e.target.is_none());
    }
    assert_eq!(health(&sim), start);
    // The same crab just outside the door is noticed at once.
    let c = sim.crab_mut(P).unwrap();
    c.mover.pos = Vec2::new(tunnel.min.x - 250.0, tunnel.min.y - 160.0);
    tick(&mut sim, Controls::default());
    assert!(sim.enemies()[0].target.is_some());
}

#[test]
fn no_blow_or_undefended_cycle_kills_a_full_shell() {
    let mut sim = encounter(220.0);
    let mut worst = 0.0f32;
    let mut at_gulp = health(&sim);
    for _ in 0..4000 {
        let before = sim.enemies()[0].state;
        tick(&mut sim, Controls::default());
        let e = &sim.enemies()[0];
        if before == State::Tell && e.state == State::Attack {
            at_gulp = health(&sim);
        }
        if matches!(e.state, State::Recover) {
            worst = worst.max(at_gulp - health(&sim));
        }
        if sim.crab(P).unwrap().fighter.vitals.health <= 40.0 {
            break;
        }
    }
    assert!(worst > 0.0 && worst <= grouper::ENGULF_MAX, "worst {worst}");
}

fn script(i: u32) -> Controls {
    Controls {
        walk: match (i / 240) % 4 {
            0 => 1.0,
            1 => 0.0,
            2 => -1.0,
            _ => 0.0,
        },
        block_held: (i / 100).is_multiple_of(5),
        dodge_pressed: i % 400 == 399,
        smash_pressed: i.is_multiple_of(170),
        ..Default::default()
    }
}

#[test]
fn two_sims_with_the_same_inputs_agree_every_second() {
    let (mut a, mut b) = (encounter(520.0), encounter(520.0));
    for i in 0..2400u32 {
        let c = script(i);
        tick(&mut a, c);
        tick(&mut b, c);
        if i % 60 == 59 {
            assert_eq!(a.state_hash(), b.state_hash(), "tick {i}");
        }
    }
}

#[test]
fn a_remote_crab_is_never_changed_by_the_fish() {
    let mut sim = encounter(220.0);
    sim.crab_mut(P).unwrap().remote = true;
    let before = sim.crab(P).unwrap().fighter.clone();
    let pos = sim.crab(P).unwrap().mover.pos;
    for _ in 0..1200 {
        sim.tick();
    }
    assert_eq!(sim.crab(P).unwrap().fighter, before);
    assert_eq!(sim.crab(P).unwrap().mover.vel, Vec2::ZERO);
    assert!(sim.crab(P).unwrap().mover.pos.distance(pos) < 2.0);
}

fn world(at: Vec2, remote: bool, who: PlayerId, other: PlayerId, other_at: Vec2) -> Sim {
    let mut sim = Sim::new(Tuning::default(), Level::test_level());
    sim.spawn_enemy(Kind::GoliathGrouper, meeting_home());
    if remote {
        sim.add_crab(other, other_at);
        sim.add_remote_crab(who, at);
        sim.go_online(true);
    } else {
        sim.add_remote_crab(other, other_at);
        sim.add_crab(who, at);
        sim.go_online(false);
    }
    sim
}

#[test]
fn co_op_the_guest_defends_once_and_both_devices_agree() {
    let level = Level::test_level();
    let x = home().x - grouper::MOUTH_X - 220.0;
    let at = Vec2::new(x, level.floor_height(x));
    let far = Vec2::new(1150.0, 110.0);
    let mut host = world(at, true, GUEST, HOST, far);
    let mut guest = world(at, false, GUEST, HOST, far);
    let mut link = FakeLink::steady(0);
    let full = guest.crab(GUEST).unwrap().fighter.vitals.health;
    let mut engulfed = false;
    let mut host_saw = false;
    let mut spat = false;
    for _ in 0..1500 {
        host.set_controls(HOST, Controls::default());
        guest.set_controls(GUEST, Controls::default());
        host.tick();
        guest.tick();
        link.carry(&mut [&mut host, &mut guest]);
        let local = guest.crab(GUEST).unwrap();
        engulfed |= local.fighter.grabbed.is_some();
        host_saw |= host.crab(GUEST).unwrap().fighter.grabbed.is_some();
        assert_eq!(host.crab(HOST).unwrap().fighter.vitals.health, full);
        // The first engulf is over: judged once, by the guest, at most its cap.
        if engulfed && local.fighter.grabbed.is_none() {
            spat = true;
            break;
        }
    }
    assert!(engulfed && host_saw && spat);
    let cost = full - guest.crab(GUEST).unwrap().fighter.vitals.health;
    assert!(
        (grouper::ENGULF_DAMAGE..=grouper::ENGULF_MAX).contains(&cost),
        "{cost}"
    );
    for _ in 0..30 {
        host.tick();
        guest.tick();
        link.carry(&mut [&mut host, &mut guest]);
    }
    assert_eq!(
        host.crab(GUEST).unwrap().fighter.vitals.health,
        guest.crab(GUEST).unwrap().fighter.vitals.health
    );
    let find = |s: &Sim| {
        s.enemies()
            .iter()
            .find(|e| e.kind == Kind::GoliathGrouper)
            .map(|e| (e.state, grouper_state_of(e).engulf_damage))
    };
    assert_eq!(find(&host), find(&guest));
}

fn grouper_state_of(e: &crabsim::Enemy) -> grouper::State {
    match &e.creature {
        Extra::Grouper(s) => s.clone(),
        _ => panic!("not a grouper"),
    }
}

#[test]
fn the_dumps_name_its_phases() {
    let mut sim = encounter(300.0);
    let mut seen = std::collections::BTreeSet::new();
    for _ in 0..900 {
        tick(&mut sim, Controls::default());
        seen.insert(sim.enemies()[0].phase(&sim.tuning));
    }
    for want in ["boom", "gape", "locked", "gulp", "recover"] {
        assert!(seen.contains(want), "no {want} in {seen:?}");
    }
}

#[test]
fn the_recovery_can_be_punished_head_first() {
    let mut sim = encounter(220.0);
    let mut punished = 0.0;
    let mut presses = 0;
    for _ in 0..1200 {
        let e = &sim.enemies()[0];
        let dodge = e.state == State::Tell && e.state_tick + 1 == e.state_len;
        let smash = e.state == State::Recover && e.state_tick % 40 == 5;
        presses += u32::from(smash);
        let before = e.vitals.health;
        tick(
            &mut sim,
            Controls {
                dodge_pressed: dodge,
                smash_pressed: smash,
                ..Default::default()
            },
        );
        let e = &sim.enemies()[0];
        if e.vitals.health < before && e.state == State::Recover {
            punished = before - e.vitals.health;
            break;
        }
    }
    assert!(presses > 0);
    assert!(punished > 12.0, "head hit for {punished}");
}
