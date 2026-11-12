//! The Silt Galleries (region 2): the first polished stretch. A crab with no
//! upgrade at all walks home -> the trench's mouth -> each sponge gallery ->
//! back to the shaft that climbs to the shelf -> through the opened plug ->
//! home; no hunter reaches the calm gallery; both rest points are named; no
//! cavity is sealed off; and the plug opens from below, never from above.
mod common;
use bevy_math::{Rect, Vec2};
use common::route::{self, Kit, Node};
use crabsim::level::landmarks::{Kind, Landmark};
use crabsim::level::silt_galleries as sg;
use crabsim::underwater::{Cell, GAME_BOTTOM, GAME_CELL, initial_terrain_cell};
use crabsim::{Controls, Level, PlayerId, Sim, Tuning};
use std::collections::VecDeque;

const KIT: Kit = Kit::eggs(0);

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

/// The quickest node at rest from `start` that satisfies `goal`.
fn reach(start: &Node, goal: impl Fn(Vec2) -> bool, what: &str) -> Node {
    let survey = route::survey_from(start.restart(), KIT, |n| goal(n.mover().pos), route::BUDGET);
    let best = survey
        .nodes
        .iter()
        .filter(|n| goal(n.mover().pos))
        .min_by(|a, b| a.time.total_cmp(&b.time))
        .unwrap_or_else(|| panic!("a crab with no upgrade never reaches {what}"));
    println!(
        "{what}: {:.0} s from the last stop, at {:?}",
        best.time - start.time,
        best.mover().pos
    );
    if std::env::var("SILT_DESCRIBE").is_ok() {
        let i = survey
            .nodes
            .iter()
            .position(|n| std::ptr::eq(n, best))
            .unwrap();
        println!("   {}", survey.describe(i));
    }
    best.restart()
}

/// Scripted walks (fast); the route surveys further down are the exhaustive
/// version of the same promises.
fn crab_pos(sim: &Sim) -> Vec2 {
    sim.crab(route::ME).expect("the crab").mover.pos
}

/// Hold `controls` until `done` or `seconds` pass; the places it was grounded.
fn hold(
    sim: &mut Sim,
    controls: impl Fn(&Sim, u32) -> Controls,
    seconds: f32,
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

fn walking(dir: f32) -> impl Fn(&Sim, u32) -> Controls {
    move |_, _| Controls {
        walk: dir,
        ..Default::default()
    }
}

#[test]
fn a_crab_with_no_upgrade_walks_from_home_to_the_mouth_and_drops_into_the_gallery() {
    let marks = marks();
    let hall = named(&marks, "upper gallery").rect;
    let mut sim = route::world(KIT, route::HOME, false);
    let mut shaft_bottom = false;
    // Walking east, hopping the boulder and the ledges as they come, then
    // creeping in short steps over the trench's edge and along its floor (a
    // run off the plateau flies clear of the stone and the mouth both).
    let hopping = |sim: &Sim, tick: u32| {
        let m = &sim.crab(route::ME).unwrap().mover;
        if m.pos.x > 1250.0 {
            return Controls {
                walk: if tick % 40 < 10 { 1.0 } else { 0.0 },
                ..Default::default()
            };
        }
        Controls {
            walk: 1.0,
            jump_pressed: m.grounded && tick.is_multiple_of(90),
            jump_held: tick % 90 < 20,
            ..Default::default()
        }
    };
    let arrived = hold(&mut sim, hopping, 90.0, |sim| {
        let p = crab_pos(sim);
        shaft_bottom |= p.y < sg::UPPER_FLOOR + 5.0 && hall.contains(p + Vec2::Y);
        shaft_bottom
    });
    assert!(arrived, "stopped at {:?}", crab_pos(&sim));
    assert!(crab_pos(&sim).x > sg::MOUTH.min.x - 100.0);
}

#[test]
fn the_walk_along_the_upper_gallery_passes_each_sponge_and_ends_at_the_drop() {
    let marks = marks();
    let rooms: Vec<Rect> = (1..=3)
        .map(|n| named(&marks, &format!("sponge gallery {n}")).rect)
        .collect();
    let mut sim = route::world(KIT, Vec2::new(sg::MOUTH.center().x, sg::UPPER_FLOOR), false);
    let mut seen = [false; 3];
    let fell = hold(&mut sim, walking(1.0), 40.0, |sim| {
        let m = &sim.crab(route::ME).unwrap().mover;
        if m.grounded {
            for (k, room) in rooms.iter().enumerate() {
                seen[k] |= room.contains(m.pos + Vec2::Y);
            }
        }
        // Over the last sponge, the walk ends in the way down.
        m.pos.y < sg::UPPER_FLOOR - 200.0
    });
    assert!(fell, "the walk ends at the drop, {:?}", crab_pos(&sim));
    assert_eq!(seen, [true; 3], "every sponge gallery is walked through");
    assert!(crab_pos(&sim).x > sg::SPONGES[2].x);
}

#[test]
fn back_along_the_walk_up_the_shelf_shaft_the_plug_blocks_until_it_is_dug_open() {
    let shaft = named(&marks(), "plug shaft").rect;
    let top_of_the_climb = sg::PLUG.min.y - 30.0;
    let start = Vec2::new(sg::SPONGES[2].x, sg::UPPER_FLOOR);
    // Walk west to the shaft's foot, then jump at its stalk and climb.
    let climb = |dig: bool| {
        let mut sim = route::world(KIT, start, false);
        let walked = hold(&mut sim, walking(-1.0), 40.0, |sim| {
            crab_pos(sim).x < shaft.center().x + 90.0
        });
        assert!(walked, "walks back to the shaft: {:?}", crab_pos(&sim));
        // Jump at the stalk, holding west, then climb (holding the dig, if
        // asked to, once under the cover).
        let mut highest = f32::MIN;
        hold(
            &mut sim,
            |sim, tick| {
                let m = &sim.crab(route::ME).unwrap().mover;
                Controls {
                    walk: if m.climbing { 0.0 } else { -1.0 },
                    jump_pressed: tick == 0,
                    jump_held: tick < 40 || m.climbing || m.pos.y > sg::PLUG.min.y - 80.0,
                    dig_held: dig && m.pos.y > sg::PLUG.min.y - 120.0,
                    ..Default::default()
                }
            },
            45.0,
            |sim| {
                highest = highest.max(crab_pos(sim).y);
                highest > 60.0
            },
        );
        (
            highest,
            sim.level
                .nets
                .iter()
                .find(|n| n.rect == sg::PLUG)
                .map(|n| n.cut),
        )
    };
    let (stuck, cut) = climb(false);
    assert_eq!(cut, Some(false));
    assert!(
        stuck < top_of_the_climb + 40.0 && stuck > sg::UPPER_FLOOR + 500.0,
        "a climb without a dig stops under the cover: {stuck}"
    );
    let (out, cut) = climb(true);
    assert_eq!(cut, Some(true), "a held dig opens the plug from below");
    assert!(
        out > 60.0,
        "and the climb goes on out onto the shelf: {out}"
    );
}

/// The gallery walk, from inside (the way down from the mouth is proved by
/// `home_to_the_mouth_and_down_into_the_upper_gallery`).
fn inside_the_upper_gallery() -> Node {
    let mouth = named(&marks(), "silt mouth").rect;
    let at = Vec2::new(mouth.center().x, sg::UPPER_FLOOR);
    let mut start = route::start_node(KIT, at, false);
    // Let it settle on the floor.
    for _ in 0..240 {
        start.sim.tick();
    }
    reach(&start, |p| p.y < sg::UPPER_FLOOR + 5.0, "the gallery floor")
}

#[test]
#[ignore = "route survey, about a minute: run by hand with --ignored --nocapture"]
fn survey_home_to_the_mouth_and_down_into_the_upper_gallery() {
    let marks = marks();
    let mouth = named(&marks, "silt mouth");
    let hall = named(&marks, "upper gallery");
    assert!(mouth.kind == Kind::Mouth && hall.kind == Kind::Tunnel);
    let start = route::start_node(KIT, route::HOME, false);
    // On the trench floor beside the mouth (its rest point's door is here).
    let door = sg::REST_POINTS[0].0;
    let at_mouth = reach(
        &start,
        |p| p.y < -200.0 && p.y > -300.0 && p.x >= door.min.x && p.x <= mouth.rect.min.x - 10.0,
        "the trench floor beside the mouth",
    );
    // Down the shaft into the upper gallery.
    let inside = reach(
        &at_mouth,
        |p| p.y < sg::UPPER_FLOOR + 5.0 && hall.rect.contains(p + Vec2::Y),
        "the upper gallery",
    );
    assert!(inside.mover().pos.x > mouth.rect.min.x - 100.0);
}

#[test]
#[ignore = "route survey, about four minutes: run by hand with --ignored --nocapture"]
fn survey_each_sponge_gallery_and_the_shelf_shaft_through_the_opened_plug() {
    let marks = marks();
    let shaft = named(&marks, "plug shaft").rect;
    // Each sponge gallery, one after the other along the walk.
    let mut at = inside_the_upper_gallery();
    for n in 1..=3 {
        let room = named(&marks, &format!("sponge gallery {n}")).rect;
        at = reach(
            &at,
            |p| p.y < sg::UPPER_FLOOR + 5.0 && p.x > room.min.x + 20.0 && p.x < room.max.x - 20.0,
            &format!("sponge gallery {n}"),
        );
    }
    // Back west along the walk to the foot of the shaft that climbs to the
    // shelf.
    let base = reach(
        &at,
        |p| p.y < sg::UPPER_FLOOR + 5.0 && p.x >= shaft.min.x && p.x <= shaft.max.x,
        "the plug shaft's foot",
    );
    // The plug, dug open from below, stays open: up the stalk and out.
    let mut opened = base.restart();
    let plug = opened
        .sim
        .level
        .nets
        .iter()
        .position(|n| n.rect == sg::PLUG)
        .expect("the plug");
    opened.sim.level.nets[plug].cut = true;
    let home = reach(
        &opened,
        |p| p.x < 1300.0 && p.y > -10.0 && p.y < 1000.0,
        "home, through the opened plug",
    );
    assert!(home.mover().pos.x > shaft.min.x - 400.0);
}

#[test]
fn each_sponge_lies_on_the_walk_and_a_crab_walking_over_it_picks_it_up() {
    let marks = marks();
    let rooms: Vec<Rect> = (1..=3)
        .map(|n| named(&marks, &format!("sponge gallery {n}")).rect)
        .collect();
    for (sponge, room) in sg::SPONGES.iter().zip(&rooms) {
        assert!(
            room.contains(*sponge + Vec2::Y),
            "{sponge:?} is in its gallery"
        );
        // Lying on the floor, in plain view of the walk (no wall between the
        // room's door and the sponge: the floor is one clear run).
        assert_eq!(sponge.y, sg::UPPER_FLOOR + 10.0);
    }
    let mut level = Level::test_level();
    level.foes.clear();
    let mut sim = Sim::new(Tuning::default(), level);
    let first = sg::SPONGES[0];
    sim.add_crab(P0, Vec2::new(first.x - 200.0, sg::UPPER_FLOOR));
    let mut got = false;
    for _ in 0..120 * 6 {
        sim.set_controls(
            P0,
            Controls {
                walk: 1.0,
                ..Default::default()
            },
        );
        sim.tick();
        got |= sim.crab(P0).unwrap().mover.carrying == Some(crabsim::Carry::Glass);
        if got {
            break;
        }
    }
    assert!(
        got,
        "walking over a glass sponge puts it on the crab's back"
    );
}

#[test]
fn no_hunters_leash_touches_the_calm_gallery() {
    let level = Level::test_level();
    let tuning = Tuning::default();
    // Every calm room, the plug shaft up to the shelf, and the gallery's whole
    // walk: nothing that hunts may reach any of them. (The mouth's shaft is a
    // burrow mouth: the trench's moray may lunge at its lip, as at any mouth,
    // and never into it.)
    let calm: Vec<(String, Rect)> = sg::rooms()
        .iter()
        .filter(|r| r.part == sg::Part::Calm || r.name == "plug shaft")
        .map(|r| {
            let mut rect = r.rect;
            // The plug shaft's open top is the shelf's own water; the calm
            // place ends under the cover.
            rect.max.y = rect.max.y.min(sg::PLUG.min.y - 40.0);
            (r.name.to_string(), rect)
        })
        .collect();
    assert!(calm.len() >= 8, "{} calm places", calm.len());
    let walk = named(&marks(), "upper gallery");
    assert_eq!(walk.kind, Kind::Tunnel);
    let calm: Vec<(String, Rect)> = calm
        .into_iter()
        .chain(std::iter::once((walk.name, walk.rect)))
        .collect();
    let mut checked = 0;
    for &(kind, home) in &level.foes {
        let k = kind.tuning(&tuning);
        let leash = crabsim::enemy::Enemy::new(0, kind, home, &tuning).home_leash(&tuning);
        let chase = if kind == crabsim::enemy::Kind::GoliathGrouper {
            0.0
        } else {
            leash + k.approach_speed * k.tell_ticks as f32 * route::DT as f32
        };
        // The same envelope as `world_shape.rs`: a box round the hunter's whole
        // reach, body and lunge included. A walker cannot leave the sea floor
        // (its box stops at the lowest ground within its leash), a swimmer
        // cannot go through the seabed; under rock only a door lets it in.
        let reach = chase + k.body.0.max(k.body.1) + 60.0;
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
        for (name, r) in &calm {
            assert!(
                envelope.intersect(*r).is_empty(),
                "{kind:?} at {home:?} (reach {reach:.0}, box {envelope:?}) touches {name}"
            );
        }
        checked += 1;
    }
    assert!(checked > 5);
    // The moray of the hall is one of them, and far below the walk.
    assert!(
        level
            .foes
            .iter()
            .any(|&(k, at)| k == crabsim::enemy::Kind::Moray && at == sg::MORAY)
    );
}

#[test]
fn both_rest_points_exist_and_are_named() {
    let marks = marks();
    let level = Level::test_level();
    for (name, (door, den)) in ["silt mouth rest", "moray hall door rest"]
        .into_iter()
        .zip(sg::REST_POINTS)
    {
        let place = named(&marks, name);
        assert_eq!(place.kind, Kind::Refuge);
        assert_eq!(place.rect, door);
        let i = level
            .crevices
            .iter()
            .position(|c| c.door == door && c.den == den)
            .unwrap_or_else(|| panic!("{name} is a crevice"));
        // Death wakes a crab in the last crevice it rested in.
        assert_eq!(
            level.place_at(door.center()),
            Some(crabsim::level::Place::Crevice(i))
        );
    }
    // The mouth's is on the trench floor, the hall's at the moray hall's door.
    let (a, b) = (sg::REST_POINTS[0].0, sg::REST_POINTS[1].0);
    let (left, right) = level.trench();
    assert!(
        a.min.x > left && a.max.x < right && (a.min.y - level.floor_height(a.min.x)).abs() < 2.0
    );
    let hall_shaft = named(&marks, "moray hall").rect;
    assert!((b.center().x - hall_shaft.center().x).abs() < 700.0 && b.min.y > hall_shaft.max.y);
}

/// Water cells (the whole raster of the galleries' neighbourhood), flooded
/// from the mouth: every cavity of the galleries is part of one connected
/// space, with the sea above and nothing sealed away.
#[test]
fn no_dead_rock_every_cavity_is_connected() {
    let level = Level::test_level();
    let (x0, x1) = (800 / 4, 3300 / 4);
    let (y0, y1) = (
        ((-3300.0 - GAME_BOTTOM) / GAME_CELL) as i32,
        ((-200.0 - GAME_BOTTOM) / GAME_CELL) as i32,
    );
    let (w, h) = ((x1 - x0) as usize, (y1 - y0) as usize);
    let cell = |gx: i32, gy: i32| initial_terrain_cell(&level, gx, gy);
    let mut water = vec![false; w * h];
    for dx in 0..w {
        for dy in 0..h {
            water[dy * w + dx] = cell(x0 + dx as i32, y0 + dy as i32) == Cell::Water;
        }
    }
    // Start in the mouth shaft, just under the trench floor.
    let m = sg::MOUTH.center();
    let (sx, sy) = (
        ((m.x / GAME_CELL) as i32 - x0) as usize,
        (((-300.0 - GAME_BOTTOM) / GAME_CELL) as i32 - y0) as usize,
    );
    assert!(water[sy * w + sx], "the mouth is open water");
    let mut seen = vec![false; w * h];
    let mut queue = VecDeque::from([(sx, sy)]);
    seen[sy * w + sx] = true;
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
    // Every room's middle is reached, and no gallery water is left over:
    // water below the trench's own floor that the flood never found.
    for room in sg::rooms() {
        let c = room.rect.center();
        let (gx, gy) = (
            ((c.x / GAME_CELL) as i32 - x0) as usize,
            (((c.y - GAME_BOTTOM) / GAME_CELL) as i32 - y0) as usize,
        );
        let mut found = false;
        // The middle may be a settled grain: any water in the room will do.
        'room: for dy in 0..(room.rect.height() / GAME_CELL) as usize {
            for dx in 0..(room.rect.width() / GAME_CELL) as usize {
                let (x, y) = (
                    gx.saturating_sub((room.rect.width() / GAME_CELL / 2.0) as usize) + dx,
                    gy.saturating_sub((room.rect.height() / GAME_CELL / 2.0) as usize) + dy,
                );
                if x < w && y < h && seen[y * w + x] {
                    found = true;
                    break 'room;
                }
            }
        }
        assert!(found, "{} is cut off", room.name);
    }
    let rooms: Vec<Rect> = sg::rooms().iter().map(|r| r.rect).collect();
    let mut sealed = 0;
    for dx in 0..w {
        for dy in 0..h {
            if water[dy * w + dx] && !seen[dy * w + dx] {
                let p = Vec2::new(
                    (x0 as f32 + dx as f32 + 0.5) * GAME_CELL,
                    GAME_BOTTOM + ((y0 + dy as i32) as f32 + 0.5) * GAME_CELL,
                );
                if rooms.iter().any(|r| r.contains(p)) {
                    sealed += 1;
                }
            }
        }
    }
    // (A single grain-sized gap the bake left in a floor is not a cavity.)
    assert!(
        sealed <= 16,
        "{sealed} cells of water in a room that no flood reaches"
    );
}

const P0: PlayerId = PlayerId(0);

/// A cellular world with one crab at `at`.
fn world_at(at: Vec2) -> Sim {
    let mut level = Level::test_level();
    level.foes.clear();
    let mut sim = Sim::new(Tuning::default(), level);
    sim.add_crab(P0, at);
    sim.enable_underwater_cells();
    sim
}

fn plug_index(sim: &Sim) -> usize {
    sim.level
        .nets
        .iter()
        .position(|n| n.rect == sg::PLUG)
        .expect("the plug")
}

#[test]
fn the_plug_cannot_be_dug_from_above_but_opens_from_below_and_stays_open() {
    let plug = sg::PLUG;
    let dig = Controls {
        dig_held: true,
        ..Default::default()
    };
    let x = plug.center().x;
    let hold = |sim: &mut Sim, at: Vec2, controls: Controls, ticks: u32| {
        for _ in 0..ticks {
            let m = &mut sim.crab_mut(P0).unwrap().mover;
            (m.pos, m.vel) = (at, Vec2::ZERO);
            sim.set_controls(P0, controls);
            sim.tick();
        }
    };
    // From above: a crab with its feet in the cover (it has dug down to it),
    // digging down for a long while. Nothing gives way.
    let mut above = world_at(Vec2::new(x, plug.max.y + 30.0));
    let i = plug_index(&above);
    let height = above.tuning.body_height;
    hold(
        &mut above,
        Vec2::new(x, plug.max.y - 5.0),
        Controls { down: true, ..dig },
        120 * 20,
    );
    assert!(!above.level.nets[i].cut, "dug open from above");
    // From below: a crab clinging under it (its head at the cover), digging up.
    let mut below = world_at(Vec2::new(x, plug.min.y - 200.0));
    let j = plug_index(&below);
    let under = Vec2::new(x, plug.min.y - height + 1.0);
    hold(
        &mut below,
        under,
        Controls {
            dig_up: true,
            jump_held: true,
            ..dig
        },
        120 * 4,
    );
    assert!(below.level.nets[j].cut, "dug open from below");
    // And it stays open after a save.
    let saved = below.progress(P0).unwrap();
    let text = serde_json::to_string(&saved).unwrap();
    let saved: crabsim::Progress = serde_json::from_str(&text).unwrap();
    let mut fresh = world_at(Vec2::new(x, plug.min.y - 200.0));
    assert!(!fresh.level.nets[plug_index(&fresh)].cut);
    fresh.restore_progress(P0, &saved);
    let k = plug_index(&fresh);
    assert!(
        fresh.level.nets[k].cut,
        "the plug is still open after a save"
    );
}

/// Walk into each rest point's den and settle; die; wake there.
#[test]
fn a_crab_rests_in_each_rest_point_and_wakes_there_after_dying() {
    for (i, (door, den)) in sg::REST_POINTS.into_iter().enumerate() {
        let mut sim = route::world(KIT, Vec2::new(door.max.x - 5.0, den.y), false);
        let index = sim
            .level
            .crevices
            .iter()
            .position(|c| c.door == door)
            .expect("the crevice");
        let tucked = hold(&mut sim, walking(-1.0), 10.0, |sim| {
            sim.crab(route::ME).unwrap().mover.tucked_in()
        });
        assert!(
            tucked,
            "rest point {i} takes a crab in: {:?}",
            crab_pos(&sim)
        );
        hold(&mut sim, |_, _| Controls::default(), 2.0, |_| false);
        assert_eq!(sim.crab(route::ME).unwrap().wakes_at, Some(index));
        // Out of it and dead somewhere else, it wakes here.
        sim.crab_mut(route::ME).unwrap().mover.shelter = crabsim::Shelter::Out;
        for _ in 0..4 {
            sim.hit_crab(
                route::ME,
                &crabsim::Hit {
                    damage: 60.0,
                    ..Default::default()
                },
            );
            sim.tick();
        }
        assert!(sim.crab(route::ME).unwrap().fighter.dead.is_some());
        hold(&mut sim, |_, _| Controls::default(), 4.0, |_| false);
        let crab = sim.crab(route::ME).unwrap();
        assert_eq!(crab.mover.pos, den, "woke in rest point {i}");
    }
}

/// The two old holes. The crust vault had one door, on the vent field's floor
/// (and the warm seam, a chamber behind the heavy crusher's crust, only that
/// vault's); the vault now has a second, a duct and a shaft to the floor. The
/// seam stays behind its crust: no door of its own, so the crusher is still
/// the only way in.
#[test]
fn the_crust_vault_has_a_second_door_and_the_warm_seam_is_still_behind_its_crust() {
    let level = Level::test_level();
    let pockets = level.pockets();
    let get = |name: &str| {
        pockets
            .iter()
            .find(|p| p.name == name)
            .unwrap_or_else(|| panic!("no pocket {name}"))
    };
    let (vault, duct, shaft, seam) = (
        get("crust vault"),
        get("vault duct"),
        get("vault shaft"),
        get("warm seam"),
    );
    // Top door, the crust below, and the new east door into the duct (east,
    // clear of the vent field's gathering spots).
    assert_eq!(vault.mouths.len(), 3);
    let east = vault
        .mouths
        .iter()
        .max_by(|a, b| a.max.x.total_cmp(&b.max.x))
        .unwrap();
    assert!(east.min.x >= vault.frame.max.x && east.max.x <= duct.frame.min.x);
    // The shaft's top door is on the vent field's floor, like the vault's own.
    let floor = level.floor_height(shaft.frame.center().x);
    let door = shaft
        .mouths
        .iter()
        .max_by(|a, b| a.max.y.total_cmp(&b.max.y))
        .unwrap();
    assert!(
        (door.max.y - floor).abs() <= 30.0,
        "{door:?} vs the floor {floor}"
    );
    // The seam's only door is the one under the crust: the doorway is plugged
    // by brittle rock that a full heavy-crusher charge cracks.
    assert_eq!(seam.mouths.len(), 1);
    let doorway = seam.mouths[0];
    assert!(
        level.brittle.iter().any(|b| {
            b.rect.min.x < doorway.max.x
                && b.rect.max.x > doorway.min.x
                && b.rect.min.y < doorway.max.y + 40.0
                && b.rect.max.y > doorway.min.y - 40.0
        }),
        "the seam's doorway has its crust"
    );
    // The new frames keep their walls clear of the others (forty units, as
    // two frames sharing a doorway), and stand clear of the seep garden.
    for p in [duct, shaft] {
        for q in pockets.iter().filter(|q| {
            q.name != p.name && !["crust vault", "vault duct", "vault shaft"].contains(&q.name)
        }) {
            let gap = (p.frame.min.x - q.frame.max.x)
                .max(q.frame.min.x - p.frame.max.x)
                .max(p.frame.min.y - q.frame.max.y)
                .max(q.frame.min.y - p.frame.max.y);
            assert!(gap >= 40.0, "{} is {gap} from {}", p.name, q.name);
        }
    }
}

/// The ledge over the Drop is in plain sight from the lower gallery's floor
/// and out of reach of a single jump: the paddle strokes (eggs) are what get
/// a crab up to it. (It leads to a sealed stub, the Drop's future shaft.)
#[test]
fn the_drop_ledge_is_in_sight_and_out_of_reach_without_the_strokes() {
    let marks = marks();
    let ledge = named(&marks, "drop ledge").rect;
    assert_eq!(named(&marks, "drop cavern").kind, Kind::Frame);
    let climbs = |eggs: u32| {
        let kit = Kit::eggs(eggs);
        let mut best = f32::MIN;
        // A jump from the foot of the ledge, strokes on the rhythm of the
        // route survey's moves, holding towards it; the highest the crab gets.
        for strokes in 0..=eggs.min(6) {
            let mut sim = route::world(kit, Vec2::new(ledge.min.x - 130.0, sg::LOWER_FLOOR), false);
            for tick in 0..120 * 8 {
                let t = tick as f32 / 120.0;
                let press = |at: f32| t >= at && t < at + 0.008;
                let mut pressed = press(0.0);
                for k in 0..strokes {
                    pressed |= press(0.45 + 0.31 * k as f32);
                }
                // Straight up first; then, once above the ledge, over it.
                let above = sim.crab(route::ME).unwrap().mover.pos.y > ledge.min.y + 5.0;
                sim.set_controls(
                    route::ME,
                    Controls {
                        walk: if above { 1.0 } else { 0.0 },
                        jump_pressed: pressed,
                        jump_held: t < 0.45 + 0.31 * strokes as f32,
                        ..Default::default()
                    },
                );
                sim.tick();
                let m = &sim.crab(route::ME).unwrap().mover;
                if m.grounded {
                    best = best.max(m.pos.y);
                }
            }
        }
        best
    };
    let on_the_ledge = |y: f32| (y - ledge.min.y).abs() < 30.0;
    assert!(
        !on_the_ledge(climbs(0)),
        "one jump does not reach the ledge"
    );
    assert!(on_the_ledge(climbs(4)), "the strokes do");
}
