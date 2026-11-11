//! Play the first queen-triggerfish slice with ordinary controls.
use bevy_math::{Rect, Vec2};
use crabsim::enemy::State;
use crabsim::{Controls, Kind, PlayerId, Sim};
const P: PlayerId = PlayerId(0);
fn encounter() -> Sim {
    let mut sim = Sim::with_one_crab(Vec2::new(11770.0, -4700.0));
    sim.enable_underwater_cells();
    sim.spawn_enemy(Kind::Triggerfish, Vec2::new(11600.0, -4700.0));
    sim
}
fn tick(sim: &mut Sim, controls: Controls) {
    sim.set_controls(P, controls);
    sim.tick();
}
#[test]
fn jet_warns_without_damage_and_bracing_reduces_push_and_poise_strain() {
    let mut results = Vec::new();
    for brace in [false, true] {
        let mut sim = encounter();
        let health = sim.crab(P).unwrap().fighter.vitals.health;
        let mut tell = 0;
        for _ in 0..700 {
            let e = &sim.enemies()[0];
            if e.state == State::Tell && e.combo == 0 {
                tell += 1;
            }
            tick(
                &mut sim,
                Controls {
                    block_held: brace,
                    ..Default::default()
                },
            );
            if sim.enemies()[0].combo == 0 && sim.enemies()[0].struck.contains(&P) {
                let c = sim.crab(P).unwrap();
                assert_eq!(c.fighter.vitals.health, health);
                assert!(tell >= 70);
                results.push((c.mover.vel.x.abs(), c.fighter.vitals.poise));
                break;
            }
        }
    }
    assert_eq!(results.len(), 2, "jet never connected");
    assert!(results[1].0 < results[0].0);
    assert!(results[1].1 > results[0].1);
}
#[test]
fn the_bite_has_its_own_warning_and_can_be_parried() {
    let mut sim = encounter();
    let health = sim.crab(P).unwrap().fighter.vitals.health;
    let mut bite_tell = 0;
    for _ in 0..1000 {
        let e = &sim.enemies()[0];
        let parry = e.combo == 1 && e.state == State::Tell && e.state_tick + 2 >= e.state_len;
        if e.combo == 1 && e.state == State::Tell {
            bite_tell += 1;
        }
        let brace = e.combo == 0;
        tick(
            &mut sim,
            Controls {
                block_held: brace || parry,
                block_pressed: parry,
                ..Default::default()
            },
        );
        if sim.crab(P).unwrap().events.parried {
            assert!(bite_tell >= 28);
            assert_eq!(sim.crab(P).unwrap().fighter.vitals.health, health);
            assert_eq!(sim.enemies()[0].state, State::Stagger);
            return;
        }
    }
    panic!("no parry: {:?}", sim.enemies()[0]);
}
#[test]
fn an_unanswered_bite_hurts_then_recovers() {
    let mut sim = encounter();
    let health = sim.crab(P).unwrap().fighter.vitals.health;
    let mut bitten = false;
    for _ in 0..1000 {
        tick(&mut sim, Controls::default());
        bitten |= sim.crab(P).unwrap().fighter.vitals.health < health;
        if bitten && sim.enemies()[0].state == State::Recover {
            return;
        }
    }
    panic!("bite/recovery missing: {:?}", sim.enemies()[0]);
}
#[test]
fn full_diamond_does_not_fit_a_slender_animals_gap() {
    let mut sim = encounter();
    sim.level
        .rocks
        .push(Rect::new(11400.0, -4500.0, 11950.0, -4450.0));
    let initial = sim.enemies()[0].pos;
    for _ in 0..500 {
        tick(&mut sim, Controls::default());
    }
    assert!(sim.enemies()[0].pos.distance(initial) < 3.0);
    assert_eq!(
        sim.crab(P).unwrap().fighter.vitals.health,
        sim.tuning.crab.health
    );
}
#[test]
fn a_remote_crab_is_never_changed_by_the_fish() {
    let mut sim = encounter();
    sim.crab_mut(P).unwrap().remote = true;
    let before = sim.crab(P).unwrap().fighter.clone();
    for _ in 0..800 {
        sim.tick();
    }
    assert_eq!(sim.crab(P).unwrap().fighter, before);
}
#[test]
fn the_optional_habitat_has_an_unhurt_four_stroke_upper_crossing() {
    for direction in [-1.0, 1.0] {
        let (start, end) = if direction > 0.0 {
            (11100.0, 12100.0)
        } else {
            (12100.0, 11100.0)
        };
        let mut sim = Sim::with_one_crab(Vec2::new(start, -4320.0));
        sim.enable_underwater_cells();
        sim.spawn_enemy(Kind::Triggerfish, Vec2::new(11600.0, -4700.0));
        let c = sim.crab_mut(P).unwrap();
        c.mover.upgrades = 4;
        c.mover.stamina = 4.0;
        let health = c.fighter.vitals.health;
        let mut crossed = false;
        for frame in 0..1000 {
            tick(
                &mut sim,
                Controls {
                    walk: direction,
                    jump_held: true,
                    jump_pressed: frame % 90 == 0,
                    ..Default::default()
                },
            );
            assert_eq!(sim.crab(P).unwrap().fighter.vitals.health, health);
            if (sim.crab(P).unwrap().mover.pos.x - end) * direction > 0.0 {
                crossed = true;
                break;
            }
        }
        assert!(
            crossed,
            "bypass failed: {:?}",
            sim.crab(P).unwrap().mover.pos
        );
    }
}
#[test]
fn triggerfish_inputs_replay_to_the_same_final_hash() {
    let mut sim = Sim::with_one_crab(Vec2::new(11770.0, -4700.0));
    sim.enable_underwater_cells();
    sim.start_recording();
    sim.spawn_enemy(Kind::Triggerfish, Vec2::new(11600.0, -4700.0));
    for _ in 0..650 {
        tick(
            &mut sim,
            Controls {
                block_held: true,
                ..Default::default()
            },
        );
    }
    let replay = crabsim::Replay::parse(&sim.recording().unwrap().to_text()).unwrap();
    let mut restored = Sim::with_one_crab(replay.start);
    let mut playback = replay.playback();
    for _ in 0..650 {
        replay.apply_at(&mut restored);
        tick(&mut restored, playback.next_controls());
    }
    assert_eq!(sim.state_hash(), restored.state_hash());
}

#[test]
fn dodging_the_locked_jet_preserves_poise() {
    let mut sim = encounter();
    let poise = sim.crab(P).unwrap().fighter.vitals.poise;
    let mut dodged = false;
    for _ in 0..600 {
        let e = &sim.enemies()[0];
        let dodge =
            !dodged && e.combo == 0 && e.state == State::Tell && e.state_tick + 3 >= e.state_len;
        dodged |= dodge;
        tick(
            &mut sim,
            Controls {
                walk: if dodged { 1.0 } else { 0.0 },
                dodge_pressed: dodge,
                ..Default::default()
            },
        );
        if sim.enemies()[0].combo == 1 {
            assert!(dodged);
            assert_eq!(sim.crab(P).unwrap().fighter.vitals.poise, poise);
            return;
        }
    }
    panic!("jet never completed");
}

#[test]
fn rock_stops_both_the_jet_hit_and_its_visible_reach() {
    let mut sim = encounter();
    sim.level
        .rocks
        .push(Rect::new(11650.0, -4640.0, 11900.0, -4630.0));
    let aim = sim.crab(P).unwrap().mover.pos + Vec2::Y * 24.0;
    let e = &mut sim.enemies_mut()[0];
    e.pos = Vec2::new(11600.0, -4600.0);
    e.facing = 1.0;
    e.target = Some(P);
    e.state = State::Attack;
    e.state_len = 12;
    e.combo = 0;
    e.swim_goal = aim;
    let length = crabsim::triggerfish::jet_reach(
        &sim.enemies()[0],
        &sim.level,
        crabsim::triggerfish::jet_direction(&sim.enemies()[0]),
    );
    assert!(length > 40.0 && length < 120.0, "{length}");
    let poise = sim.crab(P).unwrap().fighter.vitals.poise;
    for _ in 0..12 {
        tick(&mut sim, Controls::default());
    }
    assert_eq!(sim.crab(P).unwrap().fighter.vitals.poise, poise);
}
