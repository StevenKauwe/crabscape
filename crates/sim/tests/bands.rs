use bevy_math::Vec2;
use crabsim::level::SPAWN;
use crabsim::{Level, Sight, Sim, Tuning};

#[test]
fn bands_cover_the_level_top_to_bottom_without_gaps() {
    let level = Level::test_level();
    let bands = level.bands();
    assert_eq!(bands.len(), 5);
    assert_eq!(bands[0].top_y, f32::INFINITY);
    for (i, band) in bands.iter().enumerate().skip(1) {
        assert!(bands[i - 1].top_y > band.top_y);
        assert_eq!(level.band_at(Vec2::new(0.0, band.top_y)), band);
        assert_eq!(
            level.band_at(Vec2::new(0.0, band.top_y + 0.01)),
            &bands[i - 1]
        );
    }
    for y in -20000..6000 {
        let band = level.band_at(Vec2::new(0.0, y as f32));
        assert!(bands.contains(band));
        assert_eq!(band, level.band_at(Vec2::new(8000.0, y as f32)));
    }
    assert_eq!(level.band_at(Vec2::new(0.0, -20000.0)).name, "Deep");
}

#[test]
fn each_band_is_darker_and_quieter_than_the_one_above() {
    let level = Level::test_level();
    for pair in level.bands().windows(2) {
        let [above, below] = pair else { unreachable!() };
        assert!(above.ambient_light > below.ambient_light);
        assert!(above.ambience > below.ambience);
        assert!(above.lantern_reach_factor >= below.lantern_reach_factor);
        let differences = 2
            + usize::from(above.grain != below.grain)
            + usize::from(above.smell != below.smell)
            + usize::from(above.residents != below.residents);
        assert!(differences >= 4);
    }
    let bands = level.bands();
    for band in bands {
        assert!((0.0..=1.0).contains(&band.ambient_light));
        assert!((0.0..=1.0).contains(&band.ambience));
        assert!((0.0..=1.0).contains(&band.lantern_reach_factor));
        assert!(band.lantern_tint.iter().all(|c| (0.0..=1.0).contains(c)));
    }
    assert!(bands[0].ambient_light >= 0.8);
    assert!(bands[3].ambient_light <= 0.05);
    for (band, range) in bands
        .iter()
        .zip([1.0..=1.0, 0.9..=1.0, 0.75..=0.9, 0.55..=0.75])
    {
        assert!(range.contains(&band.lantern_reach_factor));
    }
}

#[test]
fn shallows_perception_is_unchanged() {
    let sim = Sim::with_one_crab(SPAWN);
    let crab = sim.crab(crabsim::PlayerId(0)).unwrap();
    let eye = crab.mover.pos + Vec2::Y * crab.mover.height(&sim.tuning);
    assert_eq!(Sight::new(eye, 0, &sim.tuning.memory).radii.x, 360.0);
    assert_eq!(Sight::new(eye, 3, &sim.tuning.memory).radii.x, 480.0);
}

#[test]
fn perception_scales_in_every_band_including_cultures() {
    let level = Level::test_level();
    let tuning = Tuning::default();
    for y in [0.0, -260.0, -700.0, -3400.0, -9500.0] {
        let eye = Vec2::new(7000.0, y);
        for cultures in [0, 1, 4] {
            let sight = Sight::new(eye, cultures, &tuning.memory);
            assert_eq!(
                sight.radii.x,
                (360.0 + 40.0 * cultures as f32) * level.band_at(eye).lantern_reach_factor
            );
            assert!(sight.sees(eye + Vec2::X * (sight.radii.x - 0.01)));
            assert!(!sight.sees(eye + Vec2::X * (sight.radii.x + 1.0)));
        }
    }
}

#[test]
fn band_at_is_the_same_in_two_sims() {
    let a = Sim::with_one_crab(SPAWN);
    let mut b = Sim::with_one_crab(SPAWN);
    b.tick();
    for y in -10000..5000 {
        let pos = Vec2::new(6000.0, y as f32);
        assert_eq!(a.level.band_at(pos), b.level.band_at(pos));
    }
}
