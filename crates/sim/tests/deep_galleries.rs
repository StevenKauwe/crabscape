//! The Drop and the Deep Galleries (region 4): a stairwell of ledges from the
//! Silt Galleries' drop cavern down to the octopus lair, three water-filled
//! halls off it, and the breach to the long chimney's return duct. A crab walks
//! down the Drop and climbs back up using only the ledges; nothing that hunts
//! reaches the Drop or the halls; every place is named; no cavity is sealed
//! off; and the baked cavities stay open water.
mod common;
use bevy_math::{Rect, Vec2};
use common::route::{self, Kit};
use crabsim::enemy::Kind as Foe;
use crabsim::level::deep_galleries as dg;
use crabsim::level::landmarks::{Kind, Landmark};
use crabsim::level::silt_galleries as sg;
use crabsim::underwater::{Cell, GAME_BOTTOM, GAME_CELL, initial_terrain_cell};
use crabsim::{Controls, Level, Sim, Tuning};
use std::collections::VecDeque;

const KIT: Kit = Kit::eggs(4);

fn marks() -> Vec<Landmark> {
    Level::test_level().landmarks()
}

fn named(marks: &[Landmark], name: &str) -> Landmark {
    marks
        .iter()
        .find(|l| l.name == name)
        .unwrap_or_else(|| panic!("no place called {name}"))
        .clone()
}

fn crab(sim: &Sim) -> &crabsim::Mover {
    &sim.crab(route::ME).expect("the crab").mover
}

/// Run `controls` until `done` or `seconds` pass.
fn run(
    sim: &mut Sim,
    seconds: f32,
    mut controls: impl FnMut(&Sim, u32) -> Controls,
    mut done: impl FnMut(&Sim) -> bool,
) -> bool {
    for tick in 0..(seconds * 120.0) as u32 {
        let c = controls(sim, tick);
        sim.set_controls(route::ME, c);
        sim.tick();
        if done(sim) {
            return true;
        }
    }
    false
}

fn standing_at(sim: &Sim, y: f32) -> bool {
    let m = crab(sim);
    m.grounded && (m.pos.y - y).abs() <= 8.0 && m.vel.length() < 8.0
}

// --- The shape ------------------------------------------------------------

#[test]
fn the_drop_has_a_standing_place_every_500_units_and_a_stalk_between() {
    let level = Level::test_level();
    let corals = dg::corals();
    assert_eq!(corals.len() as i32, dg::SLABS + 1);
    // From the approach's floor to the foot, every climb is one stalk, from a
    // standing place to the next, none longer than the ledge rule.
    let mut worst: f32 = 0.0;
    for c in corals {
        assert!(level.climbs.contains(c), "{c:?} is in the level");
        worst = worst.max(c.top - c.bottom);
    }
    println!("longest climb between ledges: {worst} units");
    assert!(worst <= dg::LEDGE_MAX_GAP, "{worst}");
    let slabs = dg::slabs();
    assert_eq!(slabs.len() as i32, dg::SLABS);
    let mut tops = vec![dg::TOP_FLOOR];
    tops.extend(slabs.iter().map(|s| s.max.y));
    tops.push(dg::FOOT);
    let gaps: Vec<f32> = tops.windows(2).map(|w| w[0] - w[1]).collect();
    println!("{} standing places, gaps {:?}", tops.len(), &gaps[..3]);
    assert!(gaps.iter().all(|g| *g > 0.0 && *g <= dg::LEDGE_MAX_GAP));
    // Each slab leaves a gap a crab and its stalk fit through, on alternate sides.
    for (k, s) in slabs.iter().enumerate() {
        assert!(dg::SHAFT.width() - s.width() >= 110.0, "slab {k} gap");
        assert!(s.width() >= 250.0, "a ledge a crab can stand on");
    }
    // The Drop falls from near the top of the Silt Galleries to the foot.
    const { assert!(dg::TOP_FLOOR > -2000.0 && dg::FOOT < -10000.0) };
}

#[test]
fn the_plug_shaft_climb_has_a_standing_place_within_600_units_of_either_end() {
    let level = Level::test_level();
    let stalk = sg::CORAL[1];
    let ledge = sg::PLUG_LEDGE;
    let floor = ledge.min.y;
    println!(
        "plug shaft climb {} to {}, ledge at {floor}",
        stalk.bottom, stalk.top
    );
    assert!(floor - stalk.bottom <= 600.0, "from the foot");
    assert!(stalk.top - floor <= 600.0, "from the top");
    // The ledge is a named refuge beside the stalk, within a stroke's reach of
    // it, and its floor is solid.
    let marks = level.landmarks();
    let place = named(&marks, "plug shaft ledge");
    assert_eq!(place.kind, Kind::Refuge);
    assert!(place.rect.min.x - stalk.x <= 100.0);
    assert!(level.solid_at(Vec2::new(place.rect.center().x, floor - 10.0)));
    assert!(!level.solid_at(Vec2::new(place.rect.center().x, floor + 40.0)));
}

#[test]
fn every_new_place_is_named() {
    let marks = marks();
    for name in [
        "drop approach",
        "drop shaft",
        "drop top ledge",
        "lanternfish hall",
        "nodule hall",
        "whale-fall hall",
        "whale fall",
        "octopus lair",
        "octopus lair door",
        "lair passage",
        "breach shaft",
        "plug shaft ledge",
    ] {
        let place = named(&marks, name);
        assert!(!place.rect.is_empty(), "{name}");
    }
    // The lair's door is a refuge outside it, the arena a place of its own,
    // and one of the shaft's ledges is named for each slab.
    assert_eq!(named(&marks, "octopus lair door").kind, Kind::Refuge);
    assert_eq!(named(&marks, "octopus lair").kind, Kind::Frame);
    let ledges = marks
        .iter()
        .filter(|l| l.name.starts_with("drop ledge "))
        .count();
    assert_eq!(ledges as i32, dg::SLABS);
    let stalks = marks
        .iter()
        .filter(|l| l.name.starts_with("drop coral "))
        .count();
    assert_eq!(
        stalks as i32,
        dg::SLABS + 2,
        "the Drop's and the far door's"
    );
    // The breach's cover is a gate (a crusher's), and sits in the lair
    // passage's floor.
    let cover = marks
        .iter()
        .find(|l| matches!(l.kind, Kind::Gate { .. }) && l.rect == dg::BREACH_COVER)
        .expect("the breach's brittle cover");
    assert!(
        named(&marks, "lair passage")
            .rect
            .contains(cover.rect.center() + Vec2::Y * 60.0)
    );
}

// --- The walk -------------------------------------------------------------

/// Walk down one level: off the free end of the slab we stand on.
fn step_down(sim: &mut Sim, k: i32) {
    // Slab k's free end: its gap is east when k is even (k = 0 is the
    // approach: west, into the shaft).
    let dir = if k == 0 || k % 2 == 1 { -1.0 } else { 1.0 };
    let want = dg::level_of(k + 1);
    let ok = run(
        sim,
        20.0,
        // Walk to the free end and let the drop take the crab: no steering
        // in the air (a held key would carry it past the next ledge).
        |s, _| Controls {
            walk: if crab(s).grounded { dir } else { 0.0 },
            ..Default::default()
        },
        |s| standing_at(s, want),
    );
    assert!(ok, "level {k}: still at {:?}", crab(sim).pos);
}

/// Climb one level: stand on level `k + 1`, grip the stalk through slab `k`'s
/// gap and climb to its lip.
fn step_up(sim: &mut Sim, k: i32) -> f32 {
    let stalk = dg::stalk_x(k);
    let stand = stalk + if k % 2 == 1 { 70.0 } else { -70.0 };
    let rim = if k == 0 || k % 2 == 1 { 1.0 } else { -1.0 };
    let want = dg::level_of(k);
    let low = crab(sim).reserve;
    let mut least = low;
    let gripped = std::cell::Cell::new(false);
    let ok = run(
        sim,
        40.0,
        |s, _| {
            let m = crab(s);
            if m.climbing {
                return Controls {
                    jump_held: true,
                    ..Default::default()
                };
            }
            // Stand 70 units off the stalk (just out of its reach), then
            // jump at it, walking in: the grip catches the crab on the way.
            let toward = (stalk - stand).signum();
            if m.grounded && (m.pos.x - stand).abs() > 6.0 && !gripped.get() {
                return Controls {
                    walk: (stand - m.pos.x).signum(),
                    ..Default::default()
                };
            }
            if m.grounded && gripped.get() {
                return Controls::default();
            }
            if m.grounded {
                return Controls {
                    walk: toward,
                    jump_pressed: true,
                    jump_held: true,
                    ..Default::default()
                };
            }
            // Off the floor and beside the stalk: a second press grips it
            // (in the water a press beside a stalk is a grip, not a stroke).
            // Past the stalk's top: over the lip.
            Controls {
                walk: if m.pos.y > want - 30.0 { rim } else { toward },
                jump_held: m.pos.y <= want + 20.0,
                ..Default::default()
            }
        },
        |s| {
            let m = crab(s);
            gripped.set(gripped.get() || m.climbing);
            least = least.min(m.reserve);
            m.grounded && gripped.get() && (m.pos.y - want).abs() <= 8.0 && m.vel.length() < 8.0
        },
    );
    assert!(
        ok,
        "level {k}: climbing from {:?} (gripped: {})",
        crab(sim).pos,
        gripped.get()
    );
    low - least
}

fn rest(sim: &mut Sim) {
    run(
        sim,
        30.0,
        |_, _| Controls::default(),
        |s| {
            let m = crab(s);
            m.reserve >= m.strokes(&s.tuning) as f32 - 0.01
        },
    );
}

#[test]
fn a_crab_walks_down_the_drop_to_the_lair_door_and_climbs_back_up_by_the_ledges() {
    let mut sim = route::world(KIT, Vec2::new(1700.0, dg::TOP_FLOOR), false);
    // Settle on the approach's floor, then west to the top ledge.
    assert!(run(
        &mut sim,
        5.0,
        |_, _| Controls::default(),
        |s| standing_at(s, dg::TOP_FLOOR)
    ));
    // Down: every level lands on the next ledge.
    for k in 0..=dg::SLABS {
        step_down(&mut sim, k);
    }
    let foot = crab(&sim).pos;
    println!("foot of the Drop: {foot:?}");
    assert!((foot.y - dg::FOOT).abs() <= 8.0);
    // The lair's door is a walk east from the foot.
    let door = dg::lair_door();
    assert!(run(
        &mut sim,
        8.0,
        |_, _| Controls {
            walk: 1.0,
            ..Default::default()
        },
        |s| crab(s).pos.x > door.min.x + 40.0
    ));
    // And back: a crab climbs by the ledges only (every stalk starts from a
    // standing place and ends on one), each climb paid from the reserve and
    // rested off on the ledge above.
    run(
        &mut sim,
        8.0,
        |_, _| Controls {
            walk: -1.0,
            ..Default::default()
        },
        |s| crab(s).pos.x < dg::stalk_x(dg::SLABS) + 4.0,
    );
    let mut worst: f32 = 0.0;
    let mut total = 0.0;
    for k in (0..=dg::SLABS).rev() {
        let spent = step_up(&mut sim, k);
        worst = worst.max(spent);
        total += spent;
        rest(&mut sim);
    }
    println!("climb: {total:.1} strokes of reserve in all, at most {worst:.2} between ledges");
    assert!(worst > 0.1, "a climb costs reserve");
    assert!(
        worst < sim.tuning.stamina_reserve as f32,
        "and never the whole reserve"
    );
    assert!(
        (crab(&sim).pos.y - dg::TOP_FLOOR).abs() <= 8.0,
        "back at the top ledge"
    );
}

/// The jump across the drop cavern from the Silt Galleries' ledge to the
/// approach's floor (the ledge itself needs the paddle strokes).
#[test]
fn from_the_silt_galleries_ledge_a_jump_west_lands_on_the_approach() {
    let ledge = named(&marks(), "drop ledge").rect;
    let mut sim = route::world(KIT, Vec2::new(ledge.center().x, ledge.min.y), false);
    assert!(run(
        &mut sim,
        5.0,
        |_, _| Controls::default(),
        |s| standing_at(s, ledge.min.y)
    ));
    let landed = run(
        &mut sim,
        4.0,
        |_, tick| Controls {
            walk: -1.0,
            jump_pressed: tick == 0,
            jump_held: tick < 40,
            ..Default::default()
        },
        |s| {
            let m = crab(s);
            m.grounded && m.pos.x < 2290.0 && (m.pos.y - dg::TOP_FLOOR).abs() <= 8.0
        },
    );
    assert!(landed, "ended at {:?}", crab(&sim).pos);
}

// --- Who is where -----------------------------------------------------------

#[test]
fn no_hunters_reach_touches_the_drop_or_the_halls_and_the_lair_door_is_far_from_the_octopus() {
    let level = Level::test_level();
    let tuning = Tuning::default();
    let marks = marks();
    let mut places: Vec<(String, Rect)> = vec![
        ("the drop".into(), dg::SHAFT),
        ("the approach".into(), named(&marks, "drop approach").rect),
    ];
    for name in ["lanternfish hall", "nodule hall", "whale-fall hall"] {
        places.push((name.into(), named(&marks, name).rect));
    }
    for (hall, _, at) in dg::NODE_SITES {
        places.push((
            format!("a gather spot in the {hall}"),
            Rect::from_center_size(at, Vec2::new(80.0, 80.0)),
        ));
    }
    for (kind, home) in level.foes.iter().copied() {
        let k = kind.tuning(&tuning);
        let leash = crabsim::enemy::Enemy::new(0, kind, home, &tuning).home_leash(&tuning);
        let chase = if kind == Foe::GoliathGrouper {
            0.0
        } else {
            leash + k.approach_speed * k.tell_ticks as f32 * route::DT as f32
        };
        let reach = chase + k.body.0.max(k.body.1) + 60.0 + 400.0;
        let seabed = level
            .floor
            .iter()
            .filter(|p| (p.x - home.x).abs() <= chase)
            .map(|p| p.y)
            .chain([
                home.y,
                level.floor_height(home.x - chase),
                level.floor_height(home.x + chase),
            ])
            .fold(f32::INFINITY, f32::min);
        let bottom = if kind.swims() {
            (home.y - chase).max(seabed.min(home.y))
        } else {
            seabed
        };
        let envelope = Rect::new(
            home.x - reach,
            bottom - 60.0,
            home.x + reach,
            home.y + reach,
        );
        for (name, r) in &places {
            assert!(
                envelope.intersect(*r).is_empty(),
                "{kind:?} at {home:?} (reach {reach:.0}) touches {name}"
            );
        }
    }
    // The lair's octopus is the second of its kind (the far rim's den keeps the
    // first), and lives at the lair's far end, facing the door, with the walk
    // in far out of its sight.
    let octopi: Vec<Vec2> = level
        .foes
        .iter()
        .filter(|(k, _)| *k == Foe::Octopus)
        .map(|(_, at)| *at)
        .collect();
    assert_eq!(octopi.len(), 2);
    assert_eq!(octopi[1], dg::OCTOPUS_DEN);
    let lair = named(&marks, "octopus lair").rect;
    assert!(lair.contains(dg::OCTOPUS_DEN + Vec2::Y * 10.0));
    let door = dg::lair_door();
    let sight = tuning
        .octopus
        .base
        .notice_range
        .max(tuning.octopus.base.strike_range);
    let far = (dg::OCTOPUS_DEN.x - door.max.x).min(dg::OCTOPUS_DEN.x - dg::SHAFT.max.x);
    println!("the lair door is {far} from the octopus (its sight is {sight})");
    assert!(
        far > 2.0 * sight,
        "the walk into the lair is out of its reach"
    );
    // Room for its webover and an arena long enough for its jet.
    assert!(lair.height() >= tuning.octopus.webover.headroom + 40.0);
    assert!(lair.width() >= tuning.octopus.ink.jet + tuning.octopus.webover.width);
}

#[test]
fn the_lair_octopus_lives_hidden_in_the_lair_on_its_floor() {
    let level = Level::test_level();
    let mut sim = Sim::new(Tuning::default(), level);
    sim.populate();
    let lair = dg::OCTOPUS_DEN;
    let e = sim
        .enemies()
        .iter()
        .find(|e| e.kind == Foe::Octopus && (e.pos.x - lair.x).abs() < 1.0)
        .expect("the lair's octopus");
    assert!(
        (e.pos.y - dg::FOOT).abs() < 1.0,
        "on the lair's floor: {:?}",
        e.pos
    );
    assert!(e.facing < 0.0, "faces the door");
    assert!(e.hidden(&sim.tuning));
}

// --- The cells ---------------------------------------------------------------

/// Water, in the cell grid of the region: flooded from the approach, every
/// cavity of the deep galleries is part of one connected space.
#[test]
fn no_sealed_cavity_in_the_deep_galleries() {
    let level = Level::test_level();
    let x0 = (900.0 / GAME_CELL) as i32;
    let x1 = (5900.0 / GAME_CELL) as i32;
    let y0 = ((-12700.0 - GAME_BOTTOM) / GAME_CELL) as i32;
    let y1 = ((-1500.0 - GAME_BOTTOM) / GAME_CELL) as i32;
    let (w, h) = ((x1 - x0) as usize, (y1 - y0) as usize);
    let mut water = vec![false; w * h];
    for dx in 0..w {
        for dy in 0..h {
            water[dy * w + dx] =
                initial_terrain_cell(&level, x0 + dx as i32, y0 + dy as i32) == Cell::Water;
        }
    }
    let at = |p: Vec2| -> (usize, usize) {
        (
            ((p.x / GAME_CELL) as i32 - x0) as usize,
            (((p.y - GAME_BOTTOM) / GAME_CELL) as i32 - y0) as usize,
        )
    };
    let start = at(Vec2::new(1800.0, dg::TOP_FLOOR + 60.0));
    assert!(water[start.1 * w + start.0], "the approach is open water");
    let mut seen = vec![false; w * h];
    let mut queue = VecDeque::from([start]);
    seen[start.1 * w + start.0] = true;
    while let Some((x, y)) = queue.pop_front() {
        for (nx, ny) in [
            (x + 1, y),
            (x.wrapping_sub(1), y),
            (x, y + 1),
            (x, y.wrapping_sub(1)),
        ] {
            if nx < w && ny < h && water[ny * w + nx] && !seen[ny * w + nx] {
                seen[ny * w + nx] = true;
                queue.push_back((nx, ny));
            }
        }
    }
    let rooms: Vec<Rect> = dg::rooms().iter().map(|r| r.rect).collect();
    for room in dg::rooms() {
        let c = room.rect.center();
        let (gx, gy) = at(c);
        let mut found = false;
        'room: for dy in 0..(room.rect.height() / GAME_CELL) as usize {
            for dx in 0..(room.rect.width().min(400.0) / GAME_CELL) as usize {
                let (x, y) = (gx.saturating_sub(10) + dx, gy.saturating_sub(10) + dy);
                if x < w && y < h && seen[y * w + x] {
                    found = true;
                    break 'room;
                }
            }
        }
        assert!(found, "{} is cut off", room.name);
    }
    let mut sealed = 0;
    for dx in 0..w {
        for dy in 0..h {
            if water[dy * w + dx] && !seen[dy * w + dx] {
                let p = Vec2::new(
                    (x0 as f32 + dx as f32 + 0.5) * GAME_CELL,
                    GAME_BOTTOM + ((y0 + dy as i32) as f32 + 0.5) * GAME_CELL,
                );
                // (The long chimney's return duct and the silt galleries'
                // other rooms are sealed frames of their own, or reached from
                // elsewhere: only the deep galleries are counted.)
                if rooms.iter().any(|r| r.contains(p)) {
                    sealed += 1;
                }
            }
        }
    }
    assert!(
        sealed <= 16,
        "{sealed} cells of water in a room that no flood reaches"
    );
}

/// The five-minute settling must leave the deep galleries' cavities open
/// water, as it does the Silt Galleries'.
#[test]
fn the_baked_cavities_stay_mostly_open_water() {
    let level = Level::test_level();
    let mut worst = (1.0f32, "");
    for room in dg::rooms() {
        let r = room.rect;
        let (x0, x1) = ((r.min.x / GAME_CELL) as i32, (r.max.x / GAME_CELL) as i32);
        let (y0, y1) = (
            ((r.min.y - GAME_BOTTOM) / GAME_CELL) as i32,
            ((r.max.y - GAME_BOTTOM) / GAME_CELL) as i32,
        );
        let (mut open, mut total) = (0u32, 0u32);
        let mut lowest_column = f32::MAX;
        for gx in x0..x1 {
            let mut column_open = 0;
            for gy in y0..y1 {
                total += 1;
                let water = initial_terrain_cell(&level, gx, gy) == Cell::Water;
                open += u32::from(water);
                column_open += i32::from(water);
            }
            lowest_column = lowest_column.min(column_open as f32 * GAME_CELL);
        }
        let kept = open as f32 / total as f32;
        println!(
            "{}: {:.1}% open water; least open column {lowest_column}",
            room.name,
            kept * 100.0
        );
        if r.height() <= 300.0 {
            assert!(lowest_column >= 90.0, "{} is nearly blocked", room.name);
        }
        if kept < worst.0 {
            worst = (kept, room.name);
        }
    }
    assert!(worst.0 > 0.7, "{} slumped: {:.3}", worst.1, worst.0);
}

#[test]
fn the_deep_galleries_keep_clear_of_the_bone_hall_and_the_other_frames() {
    let level = Level::test_level();
    let marks = level.landmarks();
    let bone = named(&marks, "bone hall").rect;
    let chimney = named(&marks, "long chimney").rect;
    let ret = named(&marks, "chimney return").rect;
    for room in dg::rooms() {
        let fat = room.rect.inflate(EDGE_CLEAR);
        if room.name == "breach shaft" {
            // It ends at the return duct's roof, on purpose.
            assert!(fat.intersect(bone).is_empty());
            continue;
        }
        if room.name.starts_with("bone far") {
            // Its shaft and niche run beside the hall's west wall, which is
            // the hall's own twenty units of bedrock.
            assert!(
                room.rect.intersect(bone).is_empty(),
                "{} is in the bone hall",
                room.name
            );
            continue;
        }
        assert!(
            fat.intersect(bone).is_empty(),
            "{} touches the bone hall",
            room.name
        );
        assert!(
            fat.intersect(chimney).is_empty(),
            "{} touches the long chimney",
            room.name
        );
        assert!(
            fat.intersect(ret).is_empty(),
            "{} touches the return duct",
            room.name
        );
    }
    // The breach comes down through the duct's roof, under a brittle cover.
    assert!(dg::BREACH.min.y + 20.0 >= ret.max.y);
    assert!(
        level
            .brittle
            .iter()
            .any(|b| b.rect == dg::BREACH_COVER && !b.cracked)
    );
}

const EDGE_CLEAR: f32 = 30.0;

#[test]
fn every_gather_spot_is_on_a_hall_floor() {
    let level = Level::test_level();
    for (hall, kind, at) in dg::NODE_SITES {
        let room = dg::hall_rect(hall);
        assert!(
            room.contains(at + Vec2::Y * 10.0),
            "{kind} at {at:?} is in the {hall}"
        );
        let floor = level.ground_below(at + Vec2::Y * 4.0).expect("a floor");
        assert!(
            (floor - at.y).abs() <= 12.0,
            "{kind} at {at:?} lies {floor} above its floor"
        );
    }
    let kinds: std::collections::BTreeSet<_> = dg::NODE_SITES.iter().map(|s| s.1).collect();
    assert_eq!(kinds.len(), 3);
}

/// Hurt, the lair's octopus inks and jets, and fights on from another spot
/// of the lair's floor (not from the sea floor far above).
#[test]
fn the_lair_octopus_jets_along_the_lairs_own_floor() {
    use crabsim::enemy::{Part, State};
    use crabsim::{Hit, Source};
    let mut level = Level::test_level();
    level
        .foes
        .retain(|(k, at)| *k == Foe::Octopus && *at == dg::OCTOPUS_DEN);
    let mut sim = Sim::new(Tuning::default(), level);
    let start = Vec2::new(dg::OCTOPUS_DEN.x - 300.0, dg::FOOT);
    sim.add_crab(route::ME, start);
    sim.populate();
    let t = sim.tuning.clone();
    sim.crab_mut(route::ME).unwrap().fighter.vitals.health = 100_000.0;
    sim.enemies_mut()[0].octopus_mut().unwrap().revealed = true;
    let lair = dg::hall_rect("octopus lair");
    let mut jetted = false;
    let mut before = State::Idle;
    for i in 0..9000u64 {
        if i % 20 == 0 {
            let e = &mut sim.enemies_mut()[0];
            if e.hittable(&t) {
                e.takes_at(Part::Body, &Hit::new(10.0, 0.0, Source::World), 200.0, &t);
            }
        }
        sim.tick();
        let e = &sim.enemies()[0];
        if before == State::Flee && e.state != State::Flee && e.vitals.health > 0.0 {
            jetted = true;
            println!("jetted to {:?}", e.pos);
            assert!(
                (e.pos.y - dg::FOOT).abs() < 8.0,
                "on the lair's floor: {:?}",
                e.pos
            );
            assert!(
                lair.contains(e.pos + Vec2::Y * 20.0),
                "inside the lair: {:?}",
                e.pos
            );
            break;
        }
        before = e.state;
    }
    assert!(jetted, "it inked and jetted");
}

// --- The far door from the bone hall ------------------------------------------

/// The hall's far door is one of the hall's own mouths (a gap in its west
/// wall, clay to be dug through), well clear of the haze, which still exists
/// and still keeps off every mouth.
#[test]
fn the_far_door_is_clear_of_the_haze() {
    use crabsim::bone_hall;
    let level = Level::test_level();
    let marks = level.landmarks();
    let door = marks
        .iter()
        .find(|l| l.rect == dg::FAR_GAP)
        .expect("the far door is a mouth of the bone hall")
        .clone();
    assert_eq!(door.kind, Kind::Mouth);
    assert!(door.name.starts_with("bone hall mouth"), "{}", door.name);
    // The room outside it ends at the gap.
    assert_eq!(named(&marks, "bone far door").rect.max.x, dg::FAR_GAP.min.x);
    let real = bone_hall::find(&level).expect("the hall");
    let haze = bone_hall::haze(&level, &real).expect("the hall still has its haze");
    println!("haze {haze:?}, far door {:?}", door.rect);
    let gap = haze.min.x - door.rect.max.x;
    println!("the haze keeps {gap} from the far door");
    assert!(gap >= 80.0, "the haze keeps {gap} from the far door");
    for l in &marks {
        if matches!(l.kind, Kind::Tunnel | Kind::Mouth) {
            let apart = l.rect.intersect(haze);
            assert!(apart.is_empty(), "{} is in the haze", l.name);
        }
    }
}

/// Down the far door's shaft and west along the passage to the Drop's third
/// ledge; back by the stalk, resting in the niche halfway.
#[test]
fn from_the_bone_hall_a_crab_walks_the_far_door_to_the_drop_and_climbs_back() {
    let marks = marks();
    let door = named(&marks, "bone far door").rect;
    let passage = named(&marks, "bone far passage").rect;
    let shaft = named(&marks, "bone far shaft").rect;
    let mut sim = route::world(KIT, Vec2::new(door.max.x - 30.0, door.min.y), false);
    assert!(run(
        &mut sim,
        5.0,
        |_, _| Controls::default(),
        |s| standing_at(s, door.min.y)
    ));
    // Out of the hall and down the shaft (no steering in the air).
    let dropped = run(
        &mut sim,
        20.0,
        |s, _| Controls {
            walk: if crab(s).grounded { -1.0 } else { 0.0 },
            ..Default::default()
        },
        |s| standing_at(s, passage.min.y) && crab(s).pos.x > shaft.min.x,
    );
    assert!(dropped, "down the shaft: {:?}", crab(&sim).pos);
    // West along the passage and up the step onto the Drop's third ledge.
    let step = named(&marks, "bone far step").rect;
    let there = run(
        &mut sim,
        40.0,
        |s, tick| {
            let m = crab(s);
            let at_step = m.pos.x < step.max.x + 90.0;
            Controls {
                walk: if m.pos.x > step.max.x - 60.0 {
                    -1.0
                } else {
                    0.0
                },
                jump_pressed: at_step
                    && m.grounded
                    && m.pos.y < step.min.y - 40.0
                    && tick.is_multiple_of(30),
                jump_held: at_step && m.pos.y < step.min.y - 10.0,
                ..Default::default()
            }
        },
        |s| standing_at(s, step.min.y) && crab(s).pos.x < step.max.x,
    );
    assert!(there, "on the Drop's third ledge: {:?}", crab(&sim).pos);
}

/// The shaft up from the passage to the bone hall (770 units): the stalk, a
/// kick off it onto the niche halfway, and on up to the hall's floor.
#[test]
fn the_far_door_shaft_climbs_back_with_a_niche_to_rest_in() {
    let marks = marks();
    let door = named(&marks, "bone far door").rect;
    let niche = named(&marks, "bone far ledge").rect;
    let stalk = dg::FAR_CORAL;
    assert!(niche.min.y - stalk.bottom <= 600.0 && stalk.top - niche.min.y <= 600.0);
    let mut sim = route::world(KIT, Vec2::new(stalk.x - 70.0, stalk.bottom), false);
    assert!(run(
        &mut sim,
        5.0,
        |_, _| Controls::default(),
        |s| standing_at(s, stalk.bottom)
    ));
    // Jump at the stalk and climb to just over the niche's floor.
    let gripped = std::cell::Cell::new(false);
    let climbed = run(
        &mut sim,
        30.0,
        |s, _| {
            let m = crab(s);
            if m.climbing {
                return Controls {
                    jump_held: true,
                    ..Default::default()
                };
            }
            Controls {
                walk: 1.0,
                jump_pressed: m.grounded && !gripped.get(),
                jump_held: true,
                ..Default::default()
            }
        },
        |s| {
            gripped.set(gripped.get() || crab(s).climbing);
            crab(s).climbing && crab(s).pos.y > niche.min.y + 20.0
        },
    );
    assert!(
        climbed,
        "gripped: {}, at {:?}",
        gripped.get(),
        crab(&sim).pos
    );
    // Kick off east onto the niche.
    let landed = run(
        &mut sim,
        6.0,
        |s, tick| Controls {
            walk: 1.0,
            jump_pressed: tick == 0 && crab(s).climbing,
            jump_held: tick < 30,
            ..Default::default()
        },
        |s| standing_at(s, niche.min.y),
    );
    assert!(landed, "on the niche: {:?}", crab(&sim).pos);
    rest(&mut sim);
    // And back across onto the stalk and on up to the hall's floor.
    let up = std::cell::Cell::new(false);
    let out = run(
        &mut sim,
        40.0,
        |s, _| {
            let m = crab(s);
            if m.climbing {
                up.set(true);
                return Controls {
                    jump_held: true,
                    ..Default::default()
                };
            }
            if m.grounded && up.get() {
                return Controls::default();
            }
            Controls {
                walk: if m.pos.y > door.min.y - 30.0 {
                    1.0
                } else {
                    -1.0
                },
                jump_pressed: m.grounded,
                jump_held: true,
                ..Default::default()
            }
        },
        |s| up.get() && standing_at(s, door.min.y),
    );
    assert!(
        out,
        "out into the hall: {:?} (climbed again: {})",
        crab(&sim).pos,
        up.get()
    );
}

/// The plug shaft's ledge can be used: up the stalk, a kick off it onto the
/// niche, a rest, a jump back onto the stalk, and on up to the plug.
#[test]
fn the_plug_shafts_ledge_is_somewhere_a_climber_can_stand_and_leave_again() {
    let stalk = sg::CORAL[1];
    let ledge = sg::PLUG_LEDGE;
    let mut sim = route::world(KIT, Vec2::new(stalk.x + 70.0, stalk.bottom), false);
    assert!(run(
        &mut sim,
        5.0,
        |_, _| Controls::default(),
        |s| standing_at(s, stalk.bottom)
    ));
    let gripped = std::cell::Cell::new(false);
    let climbed = run(
        &mut sim,
        30.0,
        |s, _| {
            let m = crab(s);
            if m.climbing {
                return Controls {
                    jump_held: true,
                    ..Default::default()
                };
            }
            Controls {
                walk: -1.0,
                jump_pressed: m.grounded && !gripped.get(),
                jump_held: true,
                ..Default::default()
            }
        },
        |s| {
            gripped.set(gripped.get() || crab(s).climbing);
            crab(s).climbing && crab(s).pos.y > ledge.min.y + 20.0
        },
    );
    assert!(
        climbed,
        "gripped: {}, at {:?}",
        gripped.get(),
        crab(&sim).pos
    );
    let landed = run(
        &mut sim,
        6.0,
        |s, tick| Controls {
            walk: 1.0,
            jump_pressed: tick == 0 && crab(s).climbing,
            jump_held: tick < 30,
            ..Default::default()
        },
        |s| standing_at(s, ledge.min.y),
    );
    assert!(landed, "on the ledge: {:?}", crab(&sim).pos);
    rest(&mut sim);
    let up = std::cell::Cell::new(false);
    let top = sg::PLUG.min.y - 100.0;
    let out = run(
        &mut sim,
        40.0,
        |s, _| {
            let m = crab(s);
            if m.climbing {
                up.set(true);
                return Controls {
                    jump_held: true,
                    ..Default::default()
                };
            }
            Controls {
                walk: -1.0,
                jump_pressed: m.grounded,
                jump_held: true,
                ..Default::default()
            }
        },
        |s| up.get() && crab(s).pos.y > top,
    );
    assert!(
        out,
        "back on the stalk and up to the plug: {:?}",
        crab(&sim).pos
    );
}
