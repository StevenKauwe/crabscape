//! The giant isopod: armour on top, a belly bared when it rears, a shove and
//! a bite each with their own tell, and a flip when it is parried.
use bevy_math::Vec2;
use crabsim::creatures::Extra;
use crabsim::creatures::isopod::{self, ARMOUR, BELLY, FLIPPED, ROLLED};
use crabsim::enemy::State;
use crabsim::net::FakeLink;
use crabsim::{Controls, Hit, Kind, Level, PlayerId, Sim, Source, Tuning};

const P: PlayerId = PlayerId(0);
const G: PlayerId = PlayerId(1);
const X: f32 = 19180.0;

/// A crab 120 units west of an isopod on the plain floor.
fn arena() -> Sim {
    let mut sim = Sim::with_one_crab(Vec2::new(X - 120.0, -10000.0));
    let y = sim.level.floor_height(X);
    sim.spawn_enemy(Kind::GiantIsopod, Vec2::new(X, y));
    sim
}

fn tick(sim: &mut Sim, c: Controls) {
    sim.set_controls(P, c);
    sim.tick();
}

fn phase(sim: &Sim) -> &'static str {
    sim.enemies()[0].phase(&sim.tuning)
}

fn health(sim: &Sim) -> f32 {
    sim.crab(P).unwrap().fighter.vitals.health
}

fn iso(sim: &Sim) -> isopod::State {
    match &sim.enemies()[0].creature {
        Extra::Isopod(s) => s.clone(),
        other => panic!("{other:?}"),
    }
}

/// Tick with `c` until `f` holds; the tick count, or a panic.
fn until(sim: &mut Sim, c: Controls, max: u32, f: impl Fn(&Sim) -> bool) -> u32 {
    for n in 0..max {
        if f(sim) {
            return n;
        }
        tick(sim, c);
    }
    panic!("never happened: {:?}", sim.enemies()[0]);
}

fn set_health(sim: &mut Sim, h: f32) {
    let e = &mut sim.enemies_mut()[0];
    e.vitals.health = h;
}

/// A smash-sized blow straight onto the enemy, then one tick to settle.
fn blow(sim: &mut Sim, damage: f32, poise: f32) {
    let t = sim.tuning.clone();
    let x = sim.crab(P).unwrap().mover.pos.x;
    let hit = Hit::new(damage, poise, Source::Crab(P));
    sim.enemies_mut()[0].takes(&hit, x, &t);
    tick(sim, Controls::default());
}

/// Hold still until it has come and is telling a bite (the second attack).
fn to_bite_tell(sim: &mut Sim, c: Controls) {
    until(sim, c, 3000, |s| phase(s) == "rear");
}

#[test]
fn it_lives_at_the_whale_fall_500_east_of_the_carcass() {
    let level = Level::test_level();
    let lock = level
        .landmarks()
        .into_iter()
        .find(|l| l.name == "narrows lock")
        .unwrap();
    let carcass = isopod::carcass(&level).unwrap();
    assert_eq!(carcass.x, lock.rect.max.x + 600.0);
    let home = level
        .foes
        .iter()
        .find(|(k, _)| *k == Kind::GiantIsopod)
        .expect("placed")
        .1;
    assert_eq!(home.x, carcass.x + 500.0);
    assert_eq!(home.y, level.floor_height(home.x));
    assert_eq!(isopod::spots(&level), vec![home]);
}

#[test]
fn the_shove_is_told_for_0_6_s_pushes_and_does_no_damage() {
    let mut sim = arena();
    let hp = health(&sim);
    let poise = sim.crab(P).unwrap().fighter.vitals.poise;
    let mut tell = 0;
    let mut seen_cue = false;
    for _ in 0..600 {
        tick(&mut sim, Controls::default());
        if phase(&sim) == "brace" {
            tell += 1;
            seen_cue |= sim
                .cues()
                .iter()
                .any(|e| matches!(e.cue, crabsim::world::cues::Cue::Tell { .. }));
        }
        if phase(&sim) == "shove" && sim.crab(P).unwrap().fighter.vitals.poise < poise {
            break;
        }
    }
    assert_eq!(tell, isopod::SHOVE_TELL, "tell ticks");
    assert!(seen_cue, "a Cue::Tell swell");
    assert_eq!(health(&sim), hp, "no damage");
    assert!(sim.crab(P).unwrap().fighter.vitals.poise <= poise - isopod::SHOVE_POISE + 1.0);
    assert!(sim.crab(P).unwrap().mover.vel.x < 0.0, "pushed back");
}

#[test]
fn the_bite_is_told_for_0_9_s_then_hurts_14_once() {
    let mut sim = arena();
    let hp = health(&sim);
    let mut tell = 0;
    for _ in 0..3000 {
        tick(&mut sim, Controls::default());
        if matches!(phase(&sim), "rear" | "cocked") {
            tell += 1;
        }
        if health(&sim) < hp {
            break;
        }
    }
    assert_eq!(tell, isopod::BITE_TELL);
    assert_eq!(health(&sim), hp - isopod::BITE_DAMAGE);
    assert_eq!(sim.enemies()[0].state, State::Attack);
    until(&mut sim, Controls::default(), 20, |s| {
        s.enemies()[0].state == State::Recover
    });
    for _ in 0..200 {
        tick(&mut sim, Controls::default());
    }
    assert!(phase(&sim) != "bite");
    assert!(
        health(&sim) >= hp - isopod::BITE_DAMAGE,
        "one bite per blow"
    );
}

#[test]
fn no_blow_or_undefended_pair_kills_a_full_crab() {
    let mut sim = arena();
    for _ in 0..3000 {
        tick(&mut sim, Controls::default());
        assert!(sim.crab(P).unwrap().fighter.dead.is_none());
    }
    assert!(isopod::BITE_DAMAGE + isopod::BITE_DAMAGE < sim.tuning.crab.health);
}

#[test]
fn a_parry_takes_no_damage_and_turns_it_on_its_back() {
    let mut sim = arena();
    let hp = health(&sim);
    // Brace through the shove, then parry the bite at the end of its tell.
    to_bite_tell(
        &mut sim,
        Controls {
            block_held: true,
            ..Default::default()
        },
    );
    let mut parried = false;
    for _ in 0..200 {
        let e = &sim.enemies()[0];
        let press = e.state == State::Tell && e.state_tick + 5 >= e.state_len;
        let holding = e.state != State::Attack;
        tick(
            &mut sim,
            Controls {
                block_held: press || holding,
                block_pressed: press,
                ..Default::default()
            },
        );
        if sim.crab(P).unwrap().events.parried {
            parried = true;
            break;
        }
    }
    assert!(parried);
    assert_eq!(health(&sim), hp);
    assert_eq!(sim.enemies()[0].state, State::Stagger);
    assert_eq!(phase(&sim), "flipped");
    assert_eq!(sim.enemies()[0].state_len, isopod::FLIP_TICKS);
    assert_eq!(isopod::armour(&sim.enemies()[0]), FLIPPED);
    // It rights itself afterwards.
    until(&mut sim, Controls::default(), 400, |s| {
        phase(s) == "righting"
    });
    until(&mut sim, Controls::default(), 100, |s| {
        phase(s) != "righting"
    });
}

#[test]
fn a_dodge_through_the_shove_takes_nothing() {
    let mut sim = arena();
    let poise = sim.crab(P).unwrap().fighter.vitals.poise;
    let mut dodged = false;
    for _ in 0..400 {
        let e = &sim.enemies()[0];
        let go = !dodged && e.state == State::Tell && e.state_tick + 3 >= e.state_len;
        dodged |= go;
        tick(
            &mut sim,
            Controls {
                dodge_pressed: go,
                walk: if go { -1.0 } else { 0.0 },
                ..Default::default()
            },
        );
        if dodged && sim.enemies()[0].state == State::Recover {
            break;
        }
    }
    assert!(dodged);
    assert_eq!(sim.crab(P).unwrap().fighter.vitals.poise, poise);
}

#[test]
fn backing_off_during_the_tell_makes_it_miss() {
    let mut sim = arena();
    let hp = health(&sim);
    until(&mut sim, Controls::default(), 600, |s| phase(s) == "brace");
    for _ in 0..300 {
        tick(
            &mut sim,
            Controls {
                walk: -1.0,
                ..Default::default()
            },
        );
    }
    assert_eq!(health(&sim), hp);
    assert_eq!(sim.crab(P).unwrap().fighter.vitals.poise, 100.0);
}

#[test]
fn its_back_takes_a_quarter_its_bared_belly_double_and_a_flipped_one_double() {
    // On its feet.
    let mut sim = arena();
    blow(&mut sim, 12.0, 0.0);
    assert_eq!(sim.enemies()[0].vitals.health, 90.0 - 12.0 * ARMOUR);
    // Reared (belly bared, first half of the bite's tell).
    let mut sim = arena();
    to_bite_tell(&mut sim, Controls::default());
    assert_eq!(isopod::armour(&sim.enemies()[0]), BELLY);
    let before = sim.enemies()[0].vitals.health;
    blow(&mut sim, 12.0, 0.0);
    assert_eq!(sim.enemies()[0].vitals.health, before - 12.0 * BELLY);
    // The cocked second half covers it again.
    until(&mut sim, Controls::default(), 100, |s| phase(s) == "cocked");
    tick(&mut sim, Controls::default());
    assert_eq!(isopod::armour(&sim.enemies()[0]), ARMOUR);
    // The settle after the bite bares it once more.
    until(&mut sim, Controls::default(), 200, |s| phase(s) == "settle");
    assert_eq!(isopod::armour(&sim.enemies()[0]), BELLY);
}

#[test]
fn a_broken_poise_flips_it_and_a_flipped_isopod_takes_double() {
    let mut sim = arena();
    // The shove first: hold off with the block so the crab stays standing.
    for _ in 0..3 {
        blow(&mut sim, 12.0, 25.0);
    }
    assert_eq!(sim.enemies()[0].state, State::Stagger);
    assert_eq!(phase(&sim), "flipped");
    let before = sim.enemies()[0].vitals.health;
    blow(&mut sim, 12.0, 25.0);
    assert_eq!(sim.enemies()[0].vitals.health, before - 12.0 * FLIPPED);
    // 2.5 s later it has righted.
    until(&mut sim, Controls::default(), 400, |s| {
        phase(s) == "righting"
    });
}

#[test]
fn hurt_to_30_percent_it_rolls_up_then_crawls_away() {
    let mut sim = arena();
    let home = sim.enemies()[0].home;
    set_health(&mut sim, 27.0);
    tick(&mut sim, Controls::default());
    assert_eq!(phase(&sim), "rolled");
    assert_eq!(isopod::armour(&sim.enemies()[0]), ROLLED);
    let at = sim.enemies()[0].pos;
    let before = sim.enemies()[0].vitals.health;
    blow(&mut sim, 12.0, 0.0);
    assert!((before - sim.enemies()[0].vitals.health - 12.0 * ROLLED).abs() < 1e-3);
    for _ in 0..isopod::CURL_TICKS - 5 {
        tick(&mut sim, Controls::default());
    }
    assert!(sim.enemies()[0].pos.distance(at) < 60.0, "stays curled");
    until(&mut sim, Controls::default(), 100, |s| phase(s) == "crawl");
    for _ in 0..isopod::CRAWL_TICKS + 5 {
        tick(&mut sim, Controls::default());
    }
    let e = &sim.enemies()[0];
    assert!(e.pos.x > at.x, "crawled away from the crab");
    assert!((e.pos.x - home.x).abs() <= isopod::CRAWL_MAX + 10.0);
    assert_ne!(e.state, State::Flee);
    assert!(e.fled);
}

#[test]
fn the_crab_walks_away_and_it_gives_up() {
    let mut sim = arena();
    let hp = health(&sim);
    until(&mut sim, Controls::default(), 600, |s| {
        s.enemies()[0].target.is_some()
    });
    for _ in 0..2400 {
        tick(
            &mut sim,
            Controls {
                walk: -1.0,
                ..Default::default()
            },
        );
    }
    assert_eq!(health(&sim), hp);
    let e = &sim.enemies()[0];
    assert!(e.target.is_none());
    assert!(matches!(phase(&sim), "graze" | "wander"));
    assert!((e.pos.x - e.home.x).abs() <= isopod::GIVE_UP);
}

#[test]
fn a_crab_in_a_refuge_is_left_alone() {
    let level = Level::test_level();
    let door = level.crevices[0].door;
    let at = door.center();
    let mut sim = Sim::with_one_crab(at);
    let x = at.x + 150.0;
    sim.spawn_enemy(Kind::GiantIsopod, Vec2::new(x, sim.level.floor_height(x)));
    for _ in 0..1500 {
        tick(&mut sim, Controls::default());
        assert!(sim.enemies()[0].target.is_none());
    }
}

#[test]
fn it_can_be_beaten_by_a_crab_that_waits_for_the_belly_and_parries() {
    let mut sim = arena();
    let mut flipped = false;
    for _ in 0..12_000 {
        let e = &sim.enemies()[0];
        let crab = sim.crab(P).unwrap();
        let dx = e.pos.x - crab.mover.pos.x;
        let (state, len, at) = (e.state, e.state_len, e.state_tick);
        let biting = iso(&sim).biting;
        let bare = matches!(phase(&sim), "rear" | "settle" | "flipped");
        let hit_range = dx.abs() < 150.0;
        let c = Controls {
            walk: if dx.abs() > 135.0 {
                dx.signum()
            } else if dx.abs() < 115.0 {
                -dx.signum()
            } else {
                0.0
            },
            block_pressed: state == State::Tell && biting && at + 5 >= len,
            block_held: state == State::Tell && at + 12 >= len,
            smash_pressed: hit_range && bare && crab.fighter.swing.is_none(),
            ..Default::default()
        };
        tick(&mut sim, c);
        flipped |= phase(&sim) == "flipped";
        let e = &sim.enemies()[0];
        if e.state == State::Gone || e.fled {
            assert!(flipped, "the way to win is to turn it over");
            return;
        }
        assert!(sim.crab(P).unwrap().fighter.dead.is_none());
    }
    panic!("not beaten: {:?}", sim.enemies()[0]);
}

fn run(seed_walk: f32) -> Vec<u64> {
    let mut sim = arena();
    let mut hashes = Vec::new();
    for i in 0..2400 {
        tick(
            &mut sim,
            Controls {
                walk: if (i / 400) % 2 == 0 { seed_walk } else { 0.0 },
                block_held: i % 700 > 600,
                ..Default::default()
            },
        );
        if i % 60 == 0 {
            hashes.push(sim.state_hash());
        }
    }
    hashes
}

#[test]
fn two_sims_with_the_same_inputs_agree_every_second() {
    assert_eq!(run(0.5), run(0.5));
    assert_ne!(run(0.5), run(-0.5));
}

fn world() -> Sim {
    let mut sim = Sim::new(Tuning::default(), Level::test_level());
    let y = sim.level.floor_height(X);
    sim.spawn_enemy(Kind::GiantIsopod, Vec2::new(X, y));
    sim
}

#[test]
fn a_bite_on_a_guest_is_judged_by_the_guest_and_both_devices_agree() {
    let far = Vec2::new(X - 900.0, -10000.0);
    let near = Vec2::new(X - 120.0, -10000.0);
    let mut host = world();
    host.add_crab(P, far);
    host.add_remote_crab(G, near);
    host.go_online(true);
    let mut guest = world();
    guest.add_remote_crab(P, far);
    guest.add_crab(G, near);
    guest.go_online(false);
    let mut link = FakeLink::new(3, 0, 0.0, 7);
    let start = guest.crab(G).unwrap().fighter.vitals.health;
    let mut bitten = false;
    let mut reflected = false;
    for _ in 0..1500 {
        host.set_controls(P, Controls::default());
        guest.set_controls(G, Controls::default());
        host.tick();
        guest.tick();
        link.carry(&mut [&mut host, &mut guest]);
        let local = guest.crab(G).unwrap();
        bitten |= local.fighter.vitals.health < start;
        reflected |=
            bitten && host.crab(G).unwrap().fighter.vitals.health == local.fighter.vitals.health;
        assert_eq!(host.crab(P).unwrap().fighter.vitals.health, start);
        assert!(local.fighter.vitals.health >= start - 4.0 * isopod::BITE_DAMAGE);
    }
    assert!(bitten && reflected);
    let (h, g) = (&host.enemies()[0], &guest.enemies()[0]);
    assert_eq!(h.kind, g.kind);
    assert_eq!(h.state, g.state);
    assert_eq!(h.creature, g.creature);
}

#[test]
fn a_remote_crab_is_never_changed_by_the_isopod() {
    let mut sim = arena();
    sim.crab_mut(P).unwrap().remote = true;
    let before = sim.crab(P).unwrap().fighter.clone();
    for _ in 0..1500 {
        sim.tick();
    }
    assert_eq!(sim.crab(P).unwrap().fighter, before);
}
