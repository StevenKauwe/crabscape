//! The far rim opens into a connected expedition, rather than ending at
//! the octopus. Existing pickup indices and the opening geometry stay put.
use super::{Climb, Crevice, Growth, Level};
use crate::enemy::Kind;
use bevy_math::{Rect, Vec2};

pub(super) fn extend(level: &mut Level) {
    level.max_x = 17400.0;
    level.floor.extend([
        Vec2::new(9200.0, -1900.0),
        Vec2::new(9800.0, -2200.0),
        Vec2::new(10400.0, -2200.0),
        Vec2::new(11200.0, -2700.0),
        Vec2::new(11900.0, -2700.0),
        Vec2::new(12700.0, -3250.0),
        Vec2::new(13400.0, -3250.0),
        Vec2::new(14300.0, -3952.0),
        Vec2::new(14600.0, -3952.0),
        Vec2::new(15800.0, -3000.0),
        Vec2::new(16200.0, -3000.0),
        Vec2::new(17200.0, -2200.0),
        Vec2::new(17400.0, -2200.0),
    ]);
    level.rocks.extend([
        // Rubble bank: walk/climb over the cap, or excavate the sand beneath.
        Rect::new(9940.0, -2224.0, 10240.0, -2080.0),
        // A terraced upper route over the silt hollow. These are shelves.
        // Coral stays beside each west edge: a shelf across its stalk
        // would block the ascent, especially after the crab grows.
        Rect::new(11160.0, -2400.0, 11360.0, -2360.0),
        Rect::new(12060.0, -2580.0, 12260.0, -2540.0),
        Rect::new(13060.0, -2840.0, 13260.0, -2800.0),
        // A roofed gallery inset below the surrounding bank. The mouths
        // retain clearance above the continuous, gently sloping floor.
        Rect::new(14000.0, -3600.0, 14800.0, -3300.0),
        Rect::new(14060.0, -3650.0, 14100.0, -3600.0),
        Rect::new(14620.0, -3730.0, 14700.0, -3600.0),
        // The discovery sits off the lower through-route, behind the lip.
        Rect::new(14260.0, -3780.0, 14460.0, -3740.0),
        Rect::new(14960.0, -2600.0, 15220.0, -2560.0),
        // Anchor the haven roof in the cap, leaving full jump headroom
        // at the excavated mouth. A low roof traps the crab behind the
        // loosened-sand lip even after it has cleared the rock cap.
        Rect::new(10200.0, -2100.0, 10240.0, -1880.0),
        Rect::new(10220.0, -1920.0, 10390.0, -1880.0),
        // The mature sand bank can erode below its old authored surface.
        // Anchor the side haven on the cap itself, leaving the excavation
        // mouth below this shallow overhang open even for a grown body.
        Rect::new(10200.0, -2048.0, 10400.0, -2016.0),
        // A rock buttress at the end anchors the last coral and discovery.
        Rect::new(17330.0, -2300.0, 17400.0, -1800.0),
    ]);
    // Uneven, seabed-rooted transfers connect the shelves to the gallery
    // roof. No extra enemies or swim upgrades are needed for this branch.
    for (x, top) in [
        (9870.0, -2030.0),
        (11100.0, -2320.0),
        (11580.0, -2450.0),
        (12000.0, -2500.0),
        (12520.0, -2620.0),
        (13000.0, -2760.0),
        (13500.0, -2990.0),
        (13900.0, -3240.0),
        (14200.0, -3680.0),
        (14900.0, -3240.0),
        (15040.0, -2520.0),
        (16280.0, -2350.0),
        (17280.0, -1780.0),
    ] {
        level.climbs.push(Climb {
            x,
            bottom: level.floor_height(x),
            top,
            kind: Growth::Coral,
        });
    }
    // A short tidal lane rises beside the Silt shelf. Its lower edge is
    // above even a grown walking crab; jumping in is an optional choice.
    // The neighbouring coral at 13500 remains outside the reversing flow.
    level
        .currents
        .push(Rect::new(13320.0, -3120.0, 13450.0, -2860.0));
    // Retain haven indices: one beside the cap, one on the gallery ledge.
    // Neither interrupts the continuous lower route east of the cap.
    for (x, y) in [(10255.0, -2016.0), (14280.0, -3740.0)] {
        level.crevices.push(Crevice {
            door: Rect::new(x, y - 1.0, x + 90.0, y + 40.0),
            den: Vec2::new(x + 40.0, y),
        });
    }
    // Resource beds are clustered in sheltered pockets. Long stretches of
    // empty transit separate them; combat is avoidable on the upper route.
    for x in [
        9350.0, 10480.0, 11640.0, 11750.0, 12800.0, 13280.0, 14300.0, 15880.0,
    ] {
        level.clams.push(Vec2::new(x, level.floor_height(x) + 4.0));
    }
    for x in [9540.0, 10320.0, 11840.0, 13100.0, 14680.0, 16080.0, 17100.0] {
        level.grit.push(Vec2::new(x, level.floor_height(x) + 4.0));
    }
    level.teeth.push(Vec2::new(10090.0, -2350.0));
    level
        .sponges
        .extend([Vec2::new(13130.0, -2770.0), Vec2::new(14400.0, -3740.0)]);
    level
        .cultures
        .extend([Vec2::new(14360.0, -3710.0), Vec2::new(17280.0, -1790.0)]);
    // Don't add more permanent swim strokes: this is a choice of routes
    // using existing movement, not another stat gate.
    // Keep resident burrows more than 400 units from both rest niches.
    for (kind, x) in [
        (Kind::Snapper, 9430.0),
        (Kind::Mantis, 10720.0),
        (Kind::Snapper, 11900.0),
        (Kind::Snapper, 12140.0),
        (Kind::Moray, 9880.0),
        (Kind::Snapper, 13200.0),
        (Kind::Snapper, 13820.0),
        (Kind::Snapper, 13580.0),
        (Kind::Moray, 15220.0),
        (Kind::Mantis, 16900.0),
        // Append the new habitat so existing creature IDs keep their slots.
        (Kind::Triggerfish, 11600.0),
    ] {
        level.foes.push((kind, Vec2::new(x, level.floor_height(x))));
    }
    // Optional midwater hazard above the lower return; append to retain IDs.
    level
        .foes
        .push((Kind::AlarmJelly, Vec2::new(15180.0, -2830.0)));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::underwater::{Cell, GAME_BOTTOM, GAME_CELL, initial_terrain_cell};

    fn cell(level: &Level, x: f32, y: f32) -> Cell {
        initial_terrain_cell(
            level,
            (x / GAME_CELL).floor() as i32,
            ((y - GAME_BOTTOM) / GAME_CELL).floor() as i32,
        )
    }

    #[test]
    fn expedition_has_a_walkable_return_and_optional_upper_route() {
        let level = Level::test_level();
        assert!(level.max_x > 17000.0);
        // Every slope has a continuous walking route; the cap is the one
        // optional dig/climb obstacle, not a one-way cliff out of the world.
        for pair in level.floor.windows(2).filter(|p| p[0].x >= 8800.0) {
            assert!((pair[1].y - pair[0].y).abs() / (pair[1].x - pair[0].x) <= 0.8);
        }
        for climb in level.climbs.iter().filter(|c| c.x > 8800.0) {
            assert_eq!(climb.bottom, level.floor_height(climb.x));
            assert!(climb.top > climb.bottom);
        }
        assert_eq!(cell(&level, 10080.0, -2180.0), Cell::Rock);
        assert_eq!(cell(&level, 10080.0, -2350.0), Cell::Sand);
        assert_eq!(cell(&level, 10080.0, -2470.0), Cell::Rock);
        assert_eq!(cell(&level, 13200.0, -3270.0), Cell::Mud);
    }

    #[test]
    fn rest_stops_and_surface_resources_are_clear_of_rock() {
        let level = Level::test_level();
        for stop in level.crevices.iter().filter(|c| c.den.x > 8800.0) {
            assert!(stop.den.y >= level.floor_height(stop.den.x));
            assert_eq!(cell(&level, stop.den.x, stop.den.y + 40.0), Cell::Water);
        }
        for at in level
            .clams
            .iter()
            .chain(&level.grit)
            .filter(|p| p.x > 8800.0)
        {
            assert_ne!(
                cell(&level, at.x, at.y),
                Cell::Rock,
                "buried in hard rock: {at:?}"
            );
        }
    }

    #[test]
    fn a_crab_can_walk_the_lower_route_in_both_directions() {
        use crate::{Controls, Mover, STEP, Tuning, physics};
        let level = Level::test_level();
        let tuning = Tuning::default();
        for (start, target, walk) in [(10480.0, 17200.0, 1.0), (17200.0, 10480.0, -1.0)] {
            let mut mover = Mover::new(Vec2::new(start, level.floor_height(start)));
            let controls = Controls {
                walk,
                ..Default::default()
            };
            for _ in 0..120 * 120 {
                physics::step(&mut mover, &controls, &tuning, &level, STEP);
                if (mover.pos.x - target) * walk >= 0.0 {
                    break;
                }
            }
            assert!(
                (mover.pos.x - target) * walk >= -5.0,
                "stopped at {:?}, target {target}",
                mover.pos
            );
        }
    }
}
