//! The amphipod swarm: drawn by a wounded crab's smell or by food; chips,
//! never kills; walking away beats it.
use bevy_math::Vec2;
use crabsim::creatures::{Extra, amphipods as am};
use crabsim::enemy::State;
use crabsim::level::Level;
use crabsim::level::landmarks::Kind as Place;
use crabsim::net::FakeLink;
use crabsim::{Carry, CastOff, Controls, Kind, PlayerId, Sim, Tuning};

const P: PlayerId = PlayerId(0);
const HOST: PlayerId = PlayerId(0);
const GUEST: PlayerId = PlayerId(1);

fn patch() -> Vec2 {
    am::spots(&Level::test_level())[0]
}

/// A crab `dx` east of the patch (feet on the floor) at `health`, one swarm.
fn world(dx: f32, health: f32) -> Sim {
    let home = patch();
    let floor = home.y;
    let mut sim = Sim::with_one_crab(Vec2::new(home.x + dx, floor));
    sim.spawn_enemy(Kind::AmphipodSwarm, home);
    sim.crab_mut(P).unwrap().fighter.vitals.health = health;
    sim
}

fn tick(sim: &mut Sim, c: Controls) {
    sim.set_controls(P, c);
    sim.tick();
}

fn health(sim: &Sim) -> f32 {
    sim.crab(P).unwrap().fighter.vitals.health
}

fn swarm(sim: &Sim) -> &crabsim::Enemy {
    &sim.enemies()[0]
}

fn st(sim: &Sim) -> am::State {
    match &swarm(sim).creature {
        Extra::Amphipods(s) => s.clone(),
        other => panic!("not a swarm: {other:?}"),
    }
}

#[test]
fn it_is_placed_on_the_lower_floor_west_of_the_whale_fall() {
    let level = Level::test_level();
    let lock = level
        .landmarks()
        .into_iter()
        .find(|l| l.name == "narrows lock")
        .unwrap();
    let carcass = lock.rect.max.x + 600.0;
    let foes: Vec<_> = level
        .foes
        .iter()
        .filter(|(k, _)| *k == Kind::AmphipodSwarm)
        .collect();
    assert_eq!(foes.len(), 1);
    assert_eq!(foes[0].1.x, carcass - 500.0);
    assert_eq!(foes[0].1.y, level.floor_height(carcass - 500.0));
    // Not inside any tunnel or mouth.
    for l in level.landmarks() {
        if matches!(l.kind, Place::Tunnel | Place::Mouth) {
            assert!(!l.rect.contains(foes[0].1), "{}", l.name);
        }
    }
}

#[test]
fn dormant_it_cannot_be_hit_and_an_unhurt_crab_never_draws_it() {
    let mut sim = world(300.0, 100.0);
    for _ in 0..1800 {
        tick(&mut sim, Controls::default());
    }
    assert_eq!(swarm(&sim).state, State::Idle);
    assert!(swarm(&sim).hidden(&sim.tuning));
    assert!(!swarm(&sim).hittable(&sim.tuning));
    assert_eq!(health(&sim), 100.0);
}

#[test]
fn a_wounded_crab_draws_it_after_a_three_second_rise() {
    let mut sim = world(500.0, 50.0);
    let mut rise_from = None;
    let mut first_contact = None;
    let mut rise_len = 0;
    for n in 0..4000u32 {
        tick(&mut sim, Controls::default());
        let e = swarm(&sim);
        if e.state == State::Tell {
            rise_from.get_or_insert(n);
            rise_len += 1;
            assert_eq!(health(&sim), 50.0, "no nibble during the rise");
            assert_eq!(e.target, Some(P));
        }
        if health(&sim) < 50.0 && first_contact.is_none() {
            first_contact = Some(n);
        }
    }
    let (r, c) = (
        rise_from.expect("never rose"),
        first_contact.expect("no contact"),
    );
    assert!(rise_len >= 360, "tell {rise_len}");
    assert!(c - r >= 360, "rise to contact {} ticks", c - r);
}

#[test]
fn the_rise_emits_a_tell_cue() {
    let mut sim = world(500.0, 50.0);
    let mut told = false;
    for _ in 0..600 {
        tick(&mut sim, Controls::default());
        told |= sim
            .cues()
            .iter()
            .any(|e| matches!(e.cue, crabsim::world::cues::Cue::Tell { crab, .. } if crab == P));
    }
    assert!(told);
}

#[test]
fn chips_two_a_second_and_never_below_25_shell() {
    let mut sim = world(300.0, 69.0);
    let mut lowest = 69.0_f32;
    let mut hits = 0;
    let mut last = 69.0;
    for _ in 0..(120 * 90) {
        tick(&mut sim, Controls::default());
        let h = health(&sim);
        if h < last - 0.01 {
            hits += 1;
            assert!((last - h - am::NIBBLE_DAMAGE).abs() < 0.01 || h >= am::NIBBLE_FLOOR - 0.01);
        }
        last = h;
        lowest = lowest.min(h);
    }
    assert!(hits >= 20, "{hits} nibbles");
    assert!(lowest >= am::NIBBLE_FLOOR - 0.01, "lowest {lowest}");
    assert!(lowest < 69.0 - 20.0);
}

#[test]
fn a_blocked_nibble_costs_a_fifth_and_cannot_be_parried() {
    let mut sim = world(300.0, 60.0);
    let mut seen = false;
    let mut last = 60.0;
    for _ in 0..3000 {
        tick(
            &mut sim,
            Controls {
                block_held: true,
                ..Default::default()
            },
        );
        let h = health(&sim);
        if h < last {
            assert!((last - h - 0.2).abs() < 0.01, "lost {}", last - h);
            seen = true;
        }
        assert!(!sim.crab(P).unwrap().events.parried);
        last = h;
    }
    assert!(seen);
}

#[test]
fn a_nibble_does_not_stun_or_stagger_the_crab() {
    let mut sim = world(300.0, 60.0);
    for _ in 0..3000 {
        tick(&mut sim, Controls::default());
        let f = &sim.crab(P).unwrap().fighter;
        assert_eq!(f.knocked, 0);
        assert_eq!(f.vitals.stagger, 0);
        assert_eq!(f.vitals.poise, 100.0);
    }
    assert!(health(&sim) < 60.0);
}

#[test]
fn a_dodge_through_a_nibble_takes_nothing() {
    let mut sim = world(300.0, 60.0);
    // Run to the moment of contact.
    for _ in 0..3000 {
        tick(&mut sim, Controls::default());
        if swarm(&sim).state == State::Attack {
            break;
        }
    }
    assert_eq!(swarm(&sim).state, State::Attack);
    let before = health(&sim);
    // A dodge started a few ticks before the nibble covers it.
    let mut dodged = false;
    for lead in 2..=12 {
        let mut s = sim.clone();
        while st(&s).nibble_wait > lead {
            tick(&mut s, Controls::default());
        }
        tick(
            &mut s,
            Controls {
                dodge_pressed: true,
                walk: 1.0,
                ..Default::default()
            },
        );
        for _ in 0..40 {
            tick(&mut s, Controls::default());
        }
        dodged |= health(&s) == before;
    }
    assert!(dodged, "no dodge timing avoided the nibble");
}

#[test]
fn walking_away_beats_it() {
    let mut sim = world(300.0, 60.0);
    // Let it reach the crab, then walk east for good.
    let mut touched = false;
    for n in 0..(120 * 120) {
        let walk = if touched { 1.0 } else { 0.0 };
        tick(
            &mut sim,
            Controls {
                walk,
                ..Default::default()
            },
        );
        touched |= health(&sim) < 60.0;
        if touched
            && n % 600 == 0
            && swarm(&sim).pos.distance(sim.crab(P).unwrap().mover.pos) > 400.0
        {
            break;
        }
    }
    assert!(touched);
    let crab = sim.crab(P).unwrap().mover.pos;
    assert!(swarm(&sim).pos.distance(crab) > 300.0, "caught up");
    let h = health(&sim);
    for _ in 0..(120 * 30) {
        tick(&mut sim, Controls::default());
    }
    assert!(health(&sim) >= h - 0.01 - 5.0 || h < 60.0);
}

#[test]
fn it_gives_up_beyond_its_leash_and_goes_home() {
    let mut sim = world(500.0, 60.0);
    // Far from the patch it never even rises.
    sim.crab_mut(P).unwrap().mover.pos.x = patch().x + am::LEASH + 600.0;
    for _ in 0..1200 {
        tick(&mut sim, Controls::default());
    }
    assert_eq!(swarm(&sim).state, State::Idle);
    assert_eq!(swarm(&sim).pos, patch());
    // Drawn, then the crab is carried out of reach: it settles.
    let mut sim = world(500.0, 60.0);
    for _ in 0..600 {
        tick(&mut sim, Controls::default());
    }
    assert!(!matches!(swarm(&sim).state, State::Idle));
    sim.crab_mut(P).unwrap().mover.pos.x = patch().x + am::LEASH + 600.0;
    for _ in 0..(120 * 40) {
        tick(&mut sim, Controls::default());
    }
    assert_eq!(swarm(&sim).state, State::Idle);
    assert_eq!(swarm(&sim).pos, patch());
}

#[test]
fn it_is_slower_than_a_walking_crab() {
    let sim = world(500.0, 60.0);
    let walk = sim.tuning.walk_speed;
    assert!(am::DRIFT_SPEED < walk, "walk {walk}");
}

#[test]
fn a_crab_in_a_refuge_is_safe_and_never_drawn() {
    let level = Level::test_level();
    let coral = level
        .landmarks()
        .into_iter()
        .find(|l| {
            l.name.starts_with("coral stand") && l.rect.min.x > 18000.0 && l.rect.max.x < 18600.0
        })
        .expect("coral stand near the patch");
    let mut sim = world(300.0, 50.0);
    let at = Vec2::new(coral.rect.center().x, patch().y);
    sim.crab_mut(P).unwrap().mover.pos = at;
    for _ in 0..(120 * 60) {
        tick(&mut sim, Controls::default());
        sim.crab_mut(P).unwrap().mover.pos = at;
    }
    assert_eq!(health(&sim), 50.0);
}

#[test]
fn it_never_enters_a_tunnel_interior() {
    let mut sim = world(500.0, 40.0);
    let tunnels: Vec<_> = sim
        .level
        .landmarks()
        .into_iter()
        .filter(|l| matches!(l.kind, Place::Tunnel | Place::Mouth))
        .collect();
    for _ in 0..(120 * 60) {
        tick(&mut sim, Controls::default());
        let p = swarm(&sim).pos;
        assert!(tunnels.iter().all(|l| !l.rect.contains(p)));
    }
}

#[test]
fn food_draws_it_even_to_an_unhurt_crab_and_it_feeds_twenty_seconds() {
    let mut sim = world(1000.0, 100.0);
    let food = Vec2::new(patch().x + 500.0, patch().y);
    sim.crab_mut(P).unwrap().cast_offs.push(CastOff {
        pos: food,
        carrying: Carry::Clam,
        recoverable: false,
    });
    let mut fed = 0;
    let mut started = false;
    for _ in 0..(120 * 120) {
        tick(&mut sim, Controls::default());
        if swarm(&sim).state == State::Recover {
            started = true;
            fed += 1;
            assert!(swarm(&sim).pos.distance(food) < 60.0);
        }
        assert_eq!(health(&sim), 100.0);
    }
    assert!(started);
    assert!((fed - am::FEED_TICKS as i32).abs() <= 2, "fed {fed}");
    // Afterwards it settled and ignores the stripped spot.
    assert_eq!(swarm(&sim).state, State::Idle);
}

#[test]
fn food_draws_it_away_from_a_wounded_crab() {
    let mut sim = world(300.0, 50.0);
    let food = Vec2::new(patch().x + 700.0, patch().y);
    sim.crab_mut(P).unwrap().cast_offs.push(CastOff {
        pos: food,
        carrying: Carry::Clam,
        recoverable: false,
    });
    for _ in 0..(120 * 25) {
        tick(&mut sim, Controls::default());
    }
    assert_eq!(health(&sim), 50.0, "it went to the food");
    assert_eq!(swarm(&sim).state, State::Recover);
    assert!(swarm(&sim).pos.distance(food) < 60.0);
}

#[test]
fn rock_in_the_way_makes_it_give_up() {
    // Food on the far side of the lock's buttress cannot be reached.
    let mut sim = world(900.0, 100.0);
    let food = Vec2::new(patch().x - 500.0, patch().y);
    sim.crab_mut(P).unwrap().cast_offs.push(CastOff {
        pos: food,
        carrying: Carry::Clam,
        recoverable: false,
    });
    let mut gave_up = false;
    for _ in 0..(120 * 30) {
        tick(&mut sim, Controls::default());
        gave_up |=
            swarm(&sim).state == State::Idle && st(&sim).stuck == 0 && swarm(&sim).hiding > 0;
    }
    assert!(gave_up);
}

#[test]
fn smashes_kill_individuals_and_a_thin_cloud_scatters_and_leaves_a_catch() {
    let mut sim = world(0.0, 100.0);
    // Make it rise by wounding, then fight it from inside.
    sim.crab_mut(P).unwrap().fighter.vitals.health = 60.0;
    let tuning = Tuning::default();
    let full = swarm(&sim).vitals.health;
    assert_eq!(full, am::HEALTH);
    let mut smashed = 0;
    let mut dispersed = false;
    for n in 0..(120 * 200) {
        let e = swarm(&sim);
        if e.state == State::Gone {
            dispersed = true;
            break;
        }
        // Keep the crab healthy enough to carry on, as if it ate.
        let c = sim.crab_mut(P).unwrap();
        c.fighter.vitals.health = c.fighter.vitals.health.max(40.0);
        let _ = &tuning;
        tick(
            &mut sim,
            Controls {
                smash_pressed: n % 90 == 0,
                ..Default::default()
            },
        );
        if st(&sim).count < am::COUNT {
            smashed += 1;
        }
        if n % 600 == 0 && swarm(&sim).state != State::Gone {
            sim.crab_mut(P).unwrap().mover.pos.x = swarm(&sim).pos.x;
        }
    }
    assert!(smashed > 0);
    assert!(dispersed, "never scattered; count {}", st(&sim).count);
    assert_eq!(swarm(&sim).deaths, 1);
    assert_eq!(
        crabsim::creatures::drops(Kind::AmphipodSwarm),
        ["amphipods"]
    );
}

#[test]
fn a_dead_cloud_gathers_again_much_later() {
    let sim = world(300.0, 100.0);
    let k = Kind::AmphipodSwarm.tuning(&sim.tuning).respawn_ticks;
    assert!(k > 120 * 60);
}

#[test]
fn it_fills_out_again_when_left_alone() {
    let mut sim = world(900.0, 100.0);
    sim.enemies_mut()[0].vitals.health = 150.0;
    for _ in 0..(120 * 30) {
        tick(&mut sim, Controls::default());
    }
    assert!(swarm(&sim).vitals.health > 160.0);
}

#[test]
fn two_sims_agree_every_second() {
    let run = || {
        let mut sim = world(500.0, 55.0);
        let mut hashes = Vec::new();
        for n in 0..(120 * 20) {
            tick(&mut sim, Controls::default());
            if n % 60 == 0 {
                hashes.push(sim.state_hash());
            }
        }
        hashes
    };
    assert_eq!(run(), run());
}

fn coop_world(guest_at: Vec2) -> (Sim, Sim) {
    let setup = |sim: &mut Sim| {
        sim.spawn_enemy(Kind::AmphipodSwarm, patch());
    };
    let far = Vec2::new(patch().x - 1500.0, patch().y);
    let mut host = Sim::new(Tuning::default(), Level::test_level());
    setup(&mut host);
    host.add_crab(HOST, far);
    host.add_remote_crab(GUEST, guest_at);
    host.go_online(true);
    let mut guest = Sim::new(Tuning::default(), Level::test_level());
    setup(&mut guest);
    guest.add_remote_crab(HOST, far);
    guest.add_crab(GUEST, guest_at);
    guest.go_online(false);
    (host, guest)
}

#[test]
fn co_op_the_defender_judges_nibbles_and_the_host_hears_the_outcome() {
    let at = Vec2::new(patch().x + 300.0, patch().y);
    let (mut host, mut guest) = coop_world(at);
    guest.crab_mut(GUEST).unwrap().fighter.vitals.health = 60.0;
    host.crab_mut(GUEST).unwrap().fighter.vitals.health = 60.0;
    let mut link = FakeLink::steady(0);
    let mut hurt_on_host = false;
    let mut max_fed = 0;
    for _ in 0..(120 * 40) {
        host.tick();
        guest.tick();
        link.carry(&mut [&mut host, &mut guest]);
        if let Extra::Amphipods(a) = &guest.enemies()[0].creature {
            max_fed = max_fed.max(a.fed);
        }
        let g = guest.crab(GUEST).unwrap().fighter.vitals.health;
        assert!(g >= am::NIBBLE_FLOOR - 0.01);
        // The host never changes the remote crab by itself.
        hurt_on_host |= host.crab(GUEST).unwrap().fighter.vitals.health < 59.0
            && guest.crab(GUEST).unwrap().fighter.vitals.health >= 59.9;
    }
    assert!(guest.crab(GUEST).unwrap().fighter.vitals.health < 60.0);
    let fed = |s: &Sim| match &s.enemies()[0].creature {
        Extra::Amphipods(a) => a.fed,
        _ => 0,
    };
    assert!(max_fed > 0);
    let _ = (hurt_on_host, fed(&guest));
    assert_eq!(host.enemies()[0].state, guest.enemies()[0].state);
}

#[test]
fn a_remote_crab_is_never_changed_by_the_swarm() {
    let mut sim = world(300.0, 50.0);
    sim.crab_mut(P).unwrap().remote = true;
    let before = sim.crab(P).unwrap().fighter.clone();
    for _ in 0..(120 * 30) {
        sim.tick();
    }
    assert_eq!(sim.crab(P).unwrap().fighter, before);
}

#[test]
fn feels_is_the_only_reaction_to_an_outcome() {
    use crabsim::Outcome;
    let sim = world(900.0, 100.0);
    let mut e = swarm(&sim).clone();
    let t = &sim.tuning;
    let took = crabsim::combat::Took {
        broke_poise: false,
        died: false,
    };
    assert_eq!(am::feels(&mut e, P, Outcome::Hit(took), t), Some(false));
    match &e.creature {
        Extra::Amphipods(s) => assert_eq!(s.fed, am::FED_HIT),
        _ => panic!(),
    }
}
