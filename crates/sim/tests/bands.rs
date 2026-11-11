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

use crabsim::level::bands::{BANDS, Declared, POCKET_MUTE, Smell, declared_bed};

/// Where a landmark's bed is sampled: a pocket at its middle, anything else at
/// its lowest edge (where the crab stands).
fn sample(level: &Level, name: &str, rect: bevy_math::Rect) -> Vec2 {
    if level.pockets().iter().any(|p| p.name == name) {
        rect.center()
    } else {
        Vec2::new(rect.center().x, rect.min.y)
    }
}

#[test]
fn every_named_place_has_a_band() {
    let level = Level::test_level();
    let marks = level.landmarks();
    assert!(marks.len() > 100, "the whole map: {}", marks.len());
    let mut fixed = 0;
    for mark in &marks {
        let declared = declared_bed(&mark.name)
            .unwrap_or_else(|| panic!("{:?} has no bed: add it to declared_bed", mark.name));
        if let Declared::Bed(i) = declared {
            fixed += 1;
            let at = sample(&level, &mark.name, mark.rect);
            assert_eq!(
                level.bed_at(at).name,
                BANDS[i].name,
                "{:?} at {at:?} belongs to {}",
                mark.name,
                BANDS[i].name
            );
        }
    }
    for pocket in level.pockets() {
        assert!(declared_bed(pocket.name).is_some(), "{}", pocket.name);
    }
    println!("{} places, {fixed} with a fixed bed", marks.len());
    assert_eq!(declared_bed("a place nobody named"), None);
}

#[test]
fn the_bibles_table_holds_place_by_place() {
    let level = Level::test_level();
    let bed_of = |name: &str| {
        let mark = level
            .landmarks()
            .into_iter()
            .find(|l| l.name == name)
            .unwrap_or_else(|| panic!("no {name}"));
        let at = sample(&level, name, mark.rect);
        BANDS
            .iter()
            .position(|b| b.name == level.bed_at(at).name)
            .unwrap()
    };
    for (names, bed) in [
        (&["home", "home burrow"][..], 0),
        (
            &["basin", "basin floor", "far basin", "mantis mouth"][..],
            1,
        ),
        (
            &["clay tunnel", "bone hall", "canyon doorstep crossing"][..],
            2,
        ),
        (
            &[
                "canyon deep water",
                "octopus den",
                "pebble warren",
                "clay cathedral",
                "hanging garden",
            ][..],
            3,
        ),
        (
            &[
                "vent field",
                "seep garden",
                "crust vault",
                "warm seam",
                "chimney hall",
                "angler stage",
            ][..],
            4,
        ),
    ] {
        for name in names {
            assert_eq!(bed_of(name), bed, "{name}");
        }
    }
}

#[test]
fn bands_get_deeper_in_every_sense() {
    let level = Level::test_level();
    let blue = |t: [f32; 3]| t[2] / (t[0] + t[1] + t[2]);
    let smell_rank = |s: Smell| match s {
        Smell::Weed => 0,
        Smell::Clay => 1,
        Smell::Sulfur => 2,
        Smell::Metallic => 3,
    };
    for pair in BANDS.windows(2) {
        assert!(pair[1].ambient_light < pair[0].ambient_light);
        assert!(pair[1].lantern_reach_factor <= pair[0].lantern_reach_factor);
        assert!(
            blue(pair[1].lantern_tint) > blue(pair[0].lantern_tint),
            "bluer"
        );
        assert!(smell_rank(pair[1].smell) >= smell_rank(pair[0].smell));
        assert!(pair[1].ambience < pair[0].ambience);
        assert!(pair[1].sound != pair[0].sound);
        assert!(pair[1].hum >= pair[0].hum);
    }
    assert_eq!(BANDS[0].hum, 0.0);
    assert!(BANDS[4].hum > 0.0, "the deepest ground hums");
    // The five beds of the bible, as places walked in order.
    let marks = level.landmarks();
    let mut last: Option<(usize, f32)> = None;
    for name in [
        "home burrow",
        "mantis mouth",
        "bone hall",
        "clay cathedral",
        "seep garden",
    ] {
        let mark = marks.iter().find(|l| l.name == name).unwrap();
        let at = mark.rect.center();
        let bed = level.bed_at(at);
        let i = BANDS.iter().position(|b| b.name == bed.name).unwrap();
        if let Some((j, y)) = last {
            assert_eq!(i, j + usize::from(i != j), "{name}");
            assert!(
                i > j && at.y < y,
                "{name} is not deeper than the one before"
            );
            assert!(bed.lantern_reach_factor < BANDS[j].lantern_reach_factor);
        }
        println!("{name}: bed {i} ({}), y {:.0}", bed.name, at.y);
        last = Some((i, at.y));
    }
    // The wisp's tint follows the bed: weed in the home burrow, metal below.
    let burrow = marks.iter().find(|l| l.name == "home burrow").unwrap();
    assert_eq!(level.bed_at(burrow.rect.center()).smell, Smell::Weed);
    let garden = marks.iter().find(|l| l.name == "seep garden").unwrap();
    assert_eq!(level.bed_at(garden.rect.center()).smell, Smell::Metallic);
}

#[test]
fn pockets_mute_the_outside() {
    let level = Level::test_level();
    assert!(
        (0.0..1.0).contains(&POCKET_MUTE) && POCKET_MUTE > 0.2,
        "muted, never silent"
    );
    let pockets = level.pockets();
    assert_eq!(pockets.len(), 15);
    for pocket in &pockets {
        let middle = pocket.frame.center();
        assert_eq!(level.pocket_mute(middle), POCKET_MUTE, "{}", pocket.name);
        // The door's outside is the open world again, a step above the lid.
        let door = pocket.mouths[0];
        let outside = Vec2::new(door.center().x, door.max.y + 400.0);
        if level.pocket_at(outside).is_none() {
            assert_eq!(level.pocket_mute(outside), 1.0, "{}", pocket.name);
        }
    }
    // The home burrow's shafts reach the floor, and are muted all the way up.
    let burrow = level
        .landmarks()
        .into_iter()
        .find(|l| l.name == "home burrow mouth 0")
        .unwrap();
    assert_eq!(
        level.pocket_mute(burrow.rect.center() - Vec2::Y * 100.0),
        POCKET_MUTE
    );
    for open in [
        Vec2::new(1500.0, 5.0),
        Vec2::new(3000.0, -690.0),
        Vec2::new(7000.0, -4000.0),
    ] {
        assert_eq!(level.pocket_mute(open), 1.0, "{open:?}");
    }
}
