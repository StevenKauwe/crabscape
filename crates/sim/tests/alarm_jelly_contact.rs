//! Stings test obstruction at the rendered strand contact, never at the distant bell.
use bevy_math::{Rect, Vec2};
use crabsim::{Kind, PlayerId, Sim};
fn fixture(rock: Option<Rect>, x: f32) -> Sim {
    let mut sim = Sim::with_one_crab(Vec2::new(x, 0.0));
    sim.level.floor = vec![Vec2::ZERO, Vec2::new(20000.0, 0.0)];
    sim.level.rocks = rock.into_iter().collect();
    sim.level.climbs.clear();
    sim.level.currents.clear();
    sim.spawn_enemy(Kind::AlarmJelly, Vec2::new(1000.0, 420.0));
    sim
}
#[test]
fn rock_away_from_touching_strand_must_not_suppress_sting() {
    for rock in [None, Some(Rect::new(1033.0, 150.0, 1037.0, 165.0))] {
        let mut sim = fixture(rock, 1035.0);
        sim.tick();
        let crab = sim.crab(PlayerId(0)).unwrap();
        if let Some(rock) = rock {
            assert!(
                crabsim::alarm_jelly::trail(&sim.enemies()[0])
                    .iter()
                    .all(|p| p.x + crabsim::tuning::JELLY_TRAIL_RADIUS < rock.min.x),
                "the entire strand, including its radius, misses the rock"
            );
        }
        assert_eq!(
            crab.fighter.venom.len(),
            1,
            "nearby strand unobstructed, rock={rock:?}"
        );
    }
}
#[test]
fn wall_between_strand_and_body_blocks_sting() {
    let mut sim = fixture(Some(Rect::new(1011.0, 0.0, 1012.0, 80.0)), 1049.0);
    sim.tick();
    let crab = sim.crab(PlayerId(0)).unwrap();
    assert!(crab.fighter.venom.is_empty());
}
