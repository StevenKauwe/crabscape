//! The hagfish knot: slime only when struck or touched, with a tell; slime
//! clogs gills but never kills; a crab that stays in the knot gets rasped.
use bevy_math::Vec2;
use crabsim::creatures::hagfish;
use crabsim::enemy::{self, State};
use crabsim::world::cues::Cue;
use crabsim::{Controls, Kind, Outcome, PlayerId, Sim};

const P: PlayerId = PlayerId(0);
const KNOT: f32 = 1000.0;

fn flat(crab_x: f32) -> Sim {
    let mut s = Sim::with_one_crab(Vec2::new(crab_x, 0.0));
    s.level.floor = vec![Vec2::ZERO, Vec2::new(20000.0, 0.0)];
    s.level.rocks.clear();
    s.level.climbs.clear();
    s.level.currents.clear();
    s.spawn_enemy(Kind::Hagfish, Vec2::new(KNOT, 0.0));
    s
}
fn tick(s: &mut Sim, c: Controls) {
    s.set_controls(P, c);
    s.tick();
}
fn smash() -> Controls {
    Controls {
        smash_pressed: true,
        ..Default::default()
    }
}
/// Walk back to `x` (smashes carry the crab forward).
fn hold(s: &Sim, x: f32, mut c: Controls) -> Controls {
    c.walk = ((x - s.crab(P).unwrap().mover.pos.x) / 20.0).clamp(-1.0, 1.0);
    c
}
fn knot(s: &Sim) -> &enemy::Enemy {
    &s.enemies()[0]
}
fn cloud_left(s: &Sim) -> u32 {
    hagfish::state(knot(s)).map_or(0, |h| h.cloud_left)
}
/// Strike once from just outside the knot.
fn struck_sim() -> Sim {
    let mut s = flat(KNOT - 131.0);
    for i in 0..110 {
        let c = hold(
            &s,
            KNOT - 131.0,
            if i == 0 { smash() } else { Controls::default() },
        );
        tick(&mut s, c);
    }
    s
}

#[test]
fn it_is_placed_on_the_carcass_in_the_test_level() {
    let level = crabsim::level::Level::test_level();
    let c = hagfish::carcass(&level).expect("narrows lock exists");
    let lock = level
        .landmarks()
        .into_iter()
        .find(|l| l.name == "narrows lock")
        .unwrap();
    assert_eq!(c.x, lock.rect.max.x + 600.0);
    assert!(
        level
            .foes
            .iter()
            .any(|&(k, p)| k == Kind::Hagfish && p == c)
    );
    assert!(hagfish::prize(&level).unwrap().x > c.x);
}

#[test]
fn no_slime_unless_struck_or_touched() {
    let mut s = flat(KNOT - 200.0);
    let hp = s.crab(P).unwrap().fighter.vitals.health;
    for i in 0..1200 {
        // Edge up to just short of touching, then stand.
        let walk = if s.crab(P).unwrap().mover.pos.x < KNOT - 150.0 && i < 300 {
            1.0
        } else {
            0.0
        };
        tick(
            &mut s,
            Controls {
                walk,
                ..Default::default()
            },
        );
        assert_eq!(knot(&s).state, State::Idle);
        assert_eq!(cloud_left(&s), 0);
    }
    assert_eq!(s.crab(P).unwrap().fighter.vitals.health, hp);
    assert_eq!(knot(&s).pos, knot(&s).home);
}

#[test]
fn a_strike_gets_a_slime_tell_then_a_cloud() {
    let mut s = flat(KNOT - 131.0);
    let mut tell = 0;
    let mut told = false;
    let mut cloud_started_after = 0;
    for i in 0..200 {
        tick(&mut s, if i == 0 { smash() } else { Controls::default() });
        told |= s
            .cues()
            .iter()
            .any(|e| matches!(e.cue, Cue::Tell { crab, .. } if crab == P));
        if knot(&s).state == State::Tell {
            assert_eq!(hagfish::phase(knot(&s)), Some("slime-tell"));
            tell += 1;
            assert_eq!(cloud_left(&s), 0, "no cloud before the tell ends");
        } else if cloud_left(&s) > 0 && cloud_started_after == 0 {
            cloud_started_after = i;
        }
    }
    assert!(told, "a Tell cue");
    assert!(tell as u32 >= hagfish::SLIME_TELL, "tell ticks {tell}");
    assert!(cloud_started_after > 0);
    assert!(knot(&s).vitals.health < hagfish::HEALTH);
}

#[test]
fn slime_clogs_gills_but_never_kills_and_ends() {
    let mut s = struck_sim();
    let (hp, reserve) = {
        let c = s.crab(P).unwrap();
        (c.fighter.vitals.health, c.mover.reserve)
    };
    assert!(cloud_left(&s) > 0);
    let mut low = hp;
    let mut least = reserve;
    for _ in 0..1600 {
        tick(&mut s, Controls::default());
        least = least.min(s.crab(P).unwrap().mover.reserve);
        low = low.min(s.crab(P).unwrap().fighter.vitals.health);
    }
    let c = s.crab(P).unwrap();
    assert!(least < reserve - 0.1, "{reserve} -> {least}");
    assert!(low > hp - 8.0 && low >= 1.0, "chip only: {hp} -> {low}");
    assert_eq!(cloud_left(&s), 0, "the cloud ends");
    assert!(c.fighter.vitals.poise >= hagfish::POISE_FLOOR - 0.01);
}

#[test]
fn leaving_the_cloud_ends_the_drain() {
    let mut s = struck_sim();
    let mut out_since = 0;
    let mut last = s.crab(P).unwrap().mover.reserve;
    for _ in 0..500 {
        tick(
            &mut s,
            Controls {
                walk: -1.0,
                ..Default::default()
            },
        );
        let c = s.crab(P).unwrap();
        let (at, r) = hagfish::cloud(knot(&s)).unwrap_or((Vec2::ZERO, 0.0));
        if r > 0.0 && c.mover.pos.distance(at) > r + 80.0 {
            out_since += 1;
            assert!(c.mover.reserve >= last - 1e-6, "drained outside the cloud");
        }
        last = c.mover.reserve;
    }
    assert!(out_since > 100, "never got out ({out_since})");
}

#[test]
fn staying_in_the_knot_gets_rasped_after_a_tell_and_never_kills() {
    let mut s = flat(KNOT);
    let hp = s.crab(P).unwrap().fighter.vitals.health;
    let mut rasp_tell = 0;
    let mut hit = false;
    let mut worst_tick = 0.0_f32;
    let mut before = hp;
    for _ in 0..3000 {
        tick(&mut s, Controls::default());
        let now = s.crab(P).unwrap().fighter.vitals.health;
        worst_tick = worst_tick.max(before - now);
        before = now;
        match hagfish::phase(knot(&s)) {
            Some("rasp-tell") => rasp_tell += 1,
            Some("rasp") if !hit => {
                assert!(rasp_tell >= hagfish::RASP_TELL - 1, "tell {rasp_tell}");
                hit = true;
            }
            _ => {}
        }
        assert!(now > 0.0);
    }
    assert!(hit, "never rasped");
    assert!(worst_tick <= 5.5, "one blow {worst_tick}");
    assert!(s.crab(P).unwrap().fighter.vitals.health > hp - 45.0);
}

#[test]
fn a_parry_in_the_window_takes_nothing_and_throws_the_knot_back() {
    let mut s = flat(KNOT);
    for _ in 0..2000 {
        let e = knot(&s);
        let parry = e.state == State::Tell
            && hagfish::phase(e) == Some("rasp-tell")
            && e.state_tick + 3 >= e.state_len;
        let knot_tick = e.state_len.saturating_sub(e.state_tick);
        let held = parry || e.state == State::Attack;
        let hp = s.crab(P).unwrap().fighter.vitals.health;
        tick(
            &mut s,
            Controls {
                block_held: held,
                block_pressed: parry && knot_tick == 3,
                ..Default::default()
            },
        );
        if s.crab(P).unwrap().events.parried {
            assert_eq!(knot(&s).state, State::Stagger);
            assert!(s.crab(P).unwrap().fighter.vitals.health >= hp - 0.5);
            return;
        }
    }
    panic!("no parry");
}

#[test]
fn it_can_be_beaten_fish_leave_and_the_knot_disperses() {
    let mut s = flat(KNOT - 131.0);
    let mut counts = vec![hagfish::fish(knot(&s))];
    for i in 0..4000 {
        let c = hold(
            &s,
            KNOT - 131.0,
            if i % 45 == 0 {
                smash()
            } else {
                Controls::default()
            },
        );
        tick(&mut s, c);
        let n = hagfish::fish(knot(&s));
        if *counts.last().unwrap() != n {
            counts.push(n);
        }
        assert_ne!(knot(&s).state, State::Stagger, "it has no poise to break");
        if knot(&s).state == State::Gone {
            break;
        }
    }
    assert_eq!(knot(&s).state, State::Gone, "{counts:?}");
    assert_eq!(counts[0], 6);
    assert!(counts.windows(2).all(|w| w[0] > w[1]) && counts.contains(&5));
    assert!(s.crab(P).unwrap().fighter.dead.is_none());
    assert_eq!(hagfish::cloud(knot(&s)), None);
}

#[test]
fn a_remote_crab_is_never_changed_and_a_parry_result_reaches_the_same_state() {
    let mut host = flat(KNOT);
    host.crab_mut(P).unwrap().remote = true;
    let before = host.crab(P).unwrap().fighter.clone();
    let reserve = host.crab(P).unwrap().mover.reserve;
    for _ in 0..1500 {
        host.tick();
    }
    assert_eq!(host.crab(P).unwrap().fighter, before);
    assert_eq!(host.crab(P).unwrap().mover.reserve, reserve);
    // The host learns the guest's judged outcome through `feels`, as co-op does.
    let mut host = flat(KNOT);
    host.crab_mut(P).unwrap().remote = true;
    let mut guest = flat(KNOT);
    for _ in 0..3000 {
        let e = &guest.enemies()[0];
        let parry = e.state == State::Tell
            && hagfish::phase(e) == Some("rasp-tell")
            && e.state_tick + 3 >= e.state_len;
        let knot_tick = e.state_len.saturating_sub(e.state_tick);
        let held = parry || e.state == State::Attack;
        tick(
            &mut guest,
            Controls {
                block_held: held,
                block_pressed: parry && knot_tick == 3,
                ..Default::default()
            },
        );
        host.tick();
        if guest.crab(P).unwrap().events.parried {
            let tuning = host.tuning.clone();
            enemy::feels(&mut host.enemies_mut()[0], P, Outcome::Parried, &tuning);
            assert_eq!(host.enemies()[0].state, guest.enemies()[0].state);
            assert_eq!(host.enemies()[0].state, State::Stagger);
            return;
        }
    }
    panic!("no parry");
}

#[test]
fn two_sims_agree_every_second_for_twenty() {
    let run = || {
        let mut s = flat(KNOT - 131.0);
        let mut hashes = Vec::new();
        for i in 0..2400 {
            tick(
                &mut s,
                if i % 200 == 0 {
                    smash()
                } else {
                    Controls::default()
                },
            );
            if i % 60 == 0 {
                hashes.push(s.state_hash());
            }
        }
        hashes
    };
    assert_eq!(run(), run());
}

#[test]
fn feel_numbers() {
    let mut s = struck_sim();
    let r0 = s.crab(P).unwrap().mover.reserve;
    for _ in 0..720 {
        tick(&mut s, Controls::default());
    }
    println!(
        "slime tell {} ticks, cloud {} ticks, reserve {:.2} -> {:.2}, rasp tell {}",
        hagfish::SLIME_TELL,
        hagfish::CLOUD_TICKS,
        r0,
        s.crab(P).unwrap().mover.reserve,
        hagfish::RASP_TELL
    );
}
