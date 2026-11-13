//! The shallows near home (`level/shallows.rs`): two rock pools a crab walks or
//! jumps out of, a lookout reached by kelp, the trench hollow, and the sites the
//! economy places its first nodes from.
mod common;
use bevy_math::Vec2;
use common::route::{self, Kit};
use crabsim::level::landmarks::Kind;
use crabsim::level::shallows as sh;
use crabsim::{Controls, Level, Sim, Tuning};
use std::collections::BTreeSet;

const KIT: Kit = Kit::eggs(0);

fn crab(sim: &Sim) -> crabsim::Mover {
    sim.crab(route::ME).expect("the crab").mover.clone()
}

/// Hold `controls` for up to `seconds`; true once `done` holds.
fn hold(
    sim: &mut Sim,
    mut controls: impl FnMut(&crabsim::Mover, u32) -> Controls,
    seconds: f32,
    mut done: impl FnMut(&crabsim::Mover) -> bool,
) -> bool {
    for tick in 0..(seconds * 120.0) as u32 {
        let c = controls(&crab(sim), tick);
        sim.set_controls(route::ME, c);
        sim.tick();
        if done(&crab(sim)) {
            return true;
        }
    }
    false
}

/// Walk toward `dir`, hopping now and then.
fn hopping(dir: f32) -> impl Fn(&crabsim::Mover, u32) -> Controls {
    move |m, tick| Controls {
        walk: dir,
        jump_pressed: m.grounded && tick.is_multiple_of(60),
        jump_held: tick % 60 < 45,
        ..Default::default()
    }
}

fn named(name: &str) -> Vec2 {
    let l = Level::test_level()
        .landmarks()
        .into_iter()
        .find(|l| l.name == name)
        .unwrap_or_else(|| panic!("no place called {name}"));
    l.rect.center()
}

#[test]
fn every_new_place_has_a_name() {
    let marks = Level::test_level().landmarks();
    for n in [
        "shrimp row",
        "the shelf",
        "the trench",
        "far ledge",
        "the crest",
        "the lookout",
        "trench hollow",
        "home",
        "silt mouth",
        "basin",
    ] {
        assert!(marks.iter().any(|l| l.name == n), "no place called {n}");
    }
}

#[test]
fn a_crab_dropped_in_each_pool_gets_out_unaided_in_both_directions() {
    for pool in &sh::POOLS {
        let first = pool.profile[0];
        let last = pool.profile[pool.profile.len() - 1];
        let mid = (first.x + last.x) / 2.0;
        for dir in [-1.0f32, 1.0] {
            // Dropped in from above the middle.
            let mut sim = route::world(KIT, Vec2::new(mid, pool.bottom + 150.0), false);
            for _ in 0..240 {
                sim.tick();
            }
            let m = crab(&sim);
            assert!(
                m.grounded && (m.pos.y - pool.bottom).abs() < 3.0,
                "{} holds a crab: {:?}",
                pool.name,
                m.pos
            );
            let (rim, beyond) = if dir < 0.0 {
                (first, first.x - 30.0)
            } else {
                (last, last.x + 30.0)
            };
            let out = hold(&mut sim, hopping(dir), 12.0, |m| {
                (m.pos.x - beyond) * dir > 0.0 && m.pos.y > rim.y - 8.0
            });
            let m = crab(&sim);
            println!("{} {dir}: out={out} at {:?}", pool.name, m.pos);
            assert!(out, "{} {dir}: stuck at {:?}", pool.name, m.pos);
        }
    }
}

#[test]
fn the_lookout_is_climbed_by_kelp_and_its_top_holds_a_crab() {
    let top = sh::LOOKOUT_TOP;
    let level = Level::test_level();
    // Out of every hunter's reach, the turtle's too.
    let t = Tuning::default();
    for &(kind, at) in &foes(&level) {
        let site = Vec2::new(sh::LOOKOUT.center().x, top);
        assert!(site.distance(at) > danger(kind, &t), "{kind:?} at {at}");
    }
    // A crab with no upgrade: from the east of the stalk a jump west at it,
    // up the stalk, and over its top onto the ledge.
    let mut sim = route::world(KIT, Vec2::new(335.0, 0.0), false);
    let mut tick = 0;
    let up = hold(
        &mut sim,
        |m, _| {
            tick += 1;
            Controls {
                walk: if m.climbing && m.pos.y < top + 10.0 {
                    0.0
                } else if m.climbing {
                    1.0
                } else {
                    -1.0
                },
                jump_pressed: tick == 1,
                jump_held: !(m.climbing && m.pos.y >= top + 10.0),
                ..Default::default()
            }
        },
        30.0,
        |m| m.grounded && m.pos.y > top - 5.0,
    );
    let m = crab(&sim);
    println!("lookout: up={up} at {:?}", m.pos);
    assert!(up, "the crab never stands on the lookout: {:?}", m.pos);
    assert!(m.pos.x >= sh::LOOKOUT.min.x - 50.0 && m.pos.x <= sh::LOOKOUT.max.x);
    sim.set_controls(route::ME, Controls::default());
    for _ in 0..240 {
        sim.tick();
    }
    let m = crab(&sim);
    assert!(m.grounded && (m.pos.y - top).abs() < 2.0, "{:?}", m.pos);
    // Its sites lie on it.
    for &(place, _, at) in sh::SITES.iter().filter(|s| s.0 == "the lookout") {
        assert!(
            at.x >= sh::LOOKOUT.min.x && at.x <= sh::LOOKOUT.max.x,
            "{place}"
        );
        assert_eq!(at.y, top);
    }
}

#[test]
fn the_trench_hollow_is_reached_from_the_trench_and_left_again() {
    let level = Level::test_level();
    let inside = |p: Vec2| sh::HOLLOW.contains(p + Vec2::Y * 10.0);
    // From the trench floor, beside the mouth shaft.
    let start = route::start_node(KIT, Vec2::new(1500.0, -260.0), false);
    let s = route::survey_from(start, KIT, |n| inside(n.mover().pos), 3000);
    let in_node = s
        .nodes
        .iter()
        .filter(|n| inside(n.mover().pos))
        .min_by(|a, b| a.time.total_cmp(&b.time))
        .expect("a crab with no upgrade reaches the hollow from the trench");
    println!(
        "hollow reached in {:.0} s at {:?}",
        in_node.time,
        in_node.mover().pos
    );
    let i = s
        .nodes
        .iter()
        .position(|n| std::ptr::eq(n, in_node))
        .unwrap();
    println!("{}", s.describe(i));
    // And out again, to the trench floor: walk out of the door, kick off the
    // ledge's edge at the shaft's coral, climb.
    let mut sim = in_node.sim.clone();
    let out = hold(
        &mut sim,
        |m, _| Controls {
            walk: 1.0,
            jump_pressed: m.grounded && m.pos.x > 1500.0,
            jump_held: m.pos.x > 1500.0 || m.climbing,
            ..Default::default()
        },
        8.0,
        |m| m.pos.y > -250.0,
    );
    assert!(out, "no way out of the hollow: {:?}", crab(&sim).pos);
    let _ = level;
}

fn foes(level: &Level) -> Vec<(crabsim::Kind, Vec2)> {
    let mut v = level.foes.clone();
    v.extend(crabsim::creatures::spots(level));
    v.extend(level.crevices.iter().map(|c| (crabsim::Kind::Moray, c.den)));
    v
}

/// The economy test's danger distances.
fn danger(kind: crabsim::Kind, t: &Tuning) -> f32 {
    use crabsim::Kind;
    let e = kind.tuning(t);
    match kind {
        Kind::Octopus => e.notice_range + 20.0,
        Kind::HermitCrab | Kind::GiantIsopod | Kind::Lobster => {
            e.give_up_range + e.strike_range + 20.0
        }
        Kind::Hagfish => e.give_up_range + 140.0,
        Kind::GoliathGrouper | Kind::LoggerheadTurtle | Kind::HumboldtSquid => 600.0,
        _ if e.approach_speed == 0.0 => e.strike_range + 20.0,
        _ => 600.0,
    }
}

fn tier_of(kind: &str) -> u8 {
    crabsim::economy::NODES
        .iter()
        .find(|n| format!("{:?}", n.kind) == kind)
        .unwrap_or_else(|| panic!("no node kind {kind}"))
        .tier
}

#[test]
fn every_site_is_on_a_floor_in_its_band_and_out_of_every_foes_reach() {
    let level = Level::test_level();
    let t = Tuning::default();
    let foes = foes(&level);
    for &(place, kind, at) in sh::SITES {
        let ground = level
            .ground_below(at + Vec2::Y * 16.0)
            .unwrap_or_else(|| panic!("{place} {at}: no ground"));
        assert!(
            (ground - at.y).abs() <= 12.0,
            "{place} {kind} {at}: ground {ground}"
        );
        let tier = tier_of(kind);
        if at.y > -260.0 {
            assert_eq!(tier, 1, "{place} {kind} is above -260");
        } else {
            assert_eq!(tier, 2, "{place} {kind} is in the tier-2 band");
            assert!(at.y > -700.0);
        }
        assert!(level.place_at(at).is_none(), "{place} {at} is in a doorway");
        for &(fk, fp) in &foes {
            assert!(
                at.distance(fp) >= danger(fk, &t),
                "{place} {kind} {at} is {} from a {fk:?} at {fp} (keep {})",
                at.distance(fp),
                danger(fk, &t)
            );
        }
    }
}

#[test]
fn the_sites_make_groups_of_different_skills_along_the_way() {
    type Group<'a> = (&'a str, Vec<(&'a str, f32, f32)>);
    let mut groups: Vec<Group> = Vec::new();
    for &(place, kind, at) in sh::SITES {
        match groups.iter_mut().find(|g| g.0 == place) {
            Some(g) => g.1.push((kind, at.x, at.y)),
            None => groups.push((place, vec![(kind, at.x, at.y)])),
        }
    }
    let tier1: Vec<_> = groups.iter().filter(|g| g.1[0].2 > -260.0).collect();
    assert!(tier1.len() >= 6, "{} tier-1 groups", tier1.len());
    let skill = |k: &str| {
        let def = crabsim::economy::NODES
            .iter()
            .find(|n| format!("{:?}", n.kind) == k)
            .expect("kind");
        format!("{:?}", def.skill)
    };
    let mut centres = Vec::new();
    for g in &tier1 {
        let n = g.1.len();
        assert!((2..=4).contains(&n), "{} has {n} sites", g.0);
        let skills: BTreeSet<_> = g.1.iter().map(|s| skill(s.0)).collect();
        assert_eq!(skills.len(), n, "{} repeats a skill", g.0);
        centres.push(g.1.iter().map(|s| s.1).sum::<f32>() / n as f32);
    }
    centres.sort_by(f32::total_cmp);
    let mut worst: f32 = 0.0;
    for w in centres.windows(2) {
        // The trench (x 1,300 to 1,820) is the one gap the ground cannot fill.
        if w[0] < 1300.0 && w[1] > 1800.0 {
            assert!(w[1] - w[0] <= 650.0, "across the trench: {}", w[1] - w[0]);
            continue;
        }
        worst = worst.max(w[1] - w[0]);
    }
    println!("groups at {centres:?}, widest gap off the trench {worst}");
    assert!(worst <= 420.0, "{worst}");
    let hollow: Vec<_> = groups.iter().filter(|g| g.0 == "trench hollow").collect();
    assert_eq!(hollow.len(), 1);
    assert!(hollow[0].1.len() >= 3);
}

#[test]
fn finds_lie_in_the_water_or_the_sediment_out_of_reach_of_foes() {
    let level = Level::test_level();
    let t = Tuning::default();
    let foes = foes(&level);
    assert!(sh::FINDS.len() >= 6);
    for &f in sh::FINDS {
        let ground = level
            .ground_below(f)
            .unwrap_or_else(|| panic!("{f}: no ground"));
        let height = f.y - ground;
        assert!(
            (-40.0..=120.0).contains(&height) || height > 0.0,
            "{f}: {height} over ground"
        );
        for &(fk, fp) in &foes {
            assert!(
                f.distance(fp) >= danger(fk, &t),
                "{f} is near a {fk:?} at {fp}"
            );
        }
    }
    let _ = (named("the shelf"), Kind::Frame);
}

/// The top of the baked ground at world x, searching down from `from`.
fn baked_top(level: &Level, x: f32, from: f32) -> f32 {
    use crabsim::underwater::{Cell, GAME_BOTTOM, GAME_CELL, initial_terrain_cell};
    let gx = (x / GAME_CELL).floor() as i32;
    let mut gy = ((from - GAME_BOTTOM) / GAME_CELL) as i32;
    while initial_terrain_cell(level, gx, gy) == Cell::Water {
        gy -= 1;
    }
    GAME_BOTTOM + (gy + 1) as f32 * GAME_CELL
}

#[test]
fn the_baked_pools_and_hollow_keep_their_shape() {
    use crabsim::underwater::{Cell, GAME_BOTTOM, GAME_CELL, initial_terrain_cell};
    let level = Level::test_level();
    for (pool, from) in sh::POOLS.iter().zip([200.0; 0]) {
        let (a, b) = (pool.profile[0].x, pool.profile[pool.profile.len() - 1].x);
        let mid = baked_top(&level, (a + b) / 2.0, from);
        let rim = baked_top(&level, a - 12.0, from).min(baked_top(&level, b + 12.0, from));
        println!("{}: baked floor {mid}, rims {rim}", pool.name);
        assert!(rim - mid >= 15.0, "{} bakes {} deep", pool.name, rim - mid);
    }
    // The hollow stays open: a crab-high column of water over its floor.
    let (mut open, mut total) = (0, 0);
    for gx in (sh::HOLLOW.min.x / GAME_CELL) as i32..(sh::HOLLOW.max.x / GAME_CELL) as i32 {
        for gy in ((sh::HOLLOW.min.y - GAME_BOTTOM) / GAME_CELL) as i32
            ..((sh::HOLLOW.max.y - GAME_BOTTOM) / GAME_CELL) as i32
        {
            total += 1;
            open += i32::from(initial_terrain_cell(&level, gx, gy) == Cell::Water);
        }
    }
    println!("hollow {open}/{total} open");
    assert!(open * 10 >= total * 9);
}

// --- First hour, part 2: weak creatures, short climbs and a paddle gap --------

/// The home door, the rest points, and the co-op walks' stretches.
#[test]
fn the_new_shrimp_keep_clear_of_nodes_homes_rests_and_the_co_op_walks() {
    let level = Level::test_level();
    let t = Tuning::default();
    assert!(sh::FIGHTS.len() >= 2, "{}", sh::FIGHTS.len());
    let strike = danger(crabsim::Kind::Snapper, &t);
    let nodes = crabsim::economy::place::nodes(&level);
    let rests = level.rest_points();
    for &(name, kind, at) in sh::FIGHTS {
        assert_eq!(kind, crabsim::Kind::Snapper, "{name}");
        assert!(
            level.foes.contains(&(kind, at)),
            "{name} is not in the level"
        );
        assert!((0.0..2700.0).contains(&at.x), "{name}");
        assert!(level.place_at(at).is_none(), "{name} is in a doorway");
        // Standing on something: the floor, or a rock's top within a hair.
        let floor = level.floor_height(at.x);
        let on_rock = level
            .rocks
            .iter()
            .any(|r| r.min.x <= at.x && at.x <= r.max.x && (r.max.y - at.y).abs() < 1.0);
        assert!(
            (floor - at.y).abs() < 8.0 || on_rock,
            "{name} floats at {at}"
        );
        for n in &nodes {
            assert!(at.distance(n.pos) >= strike, "{name} near a {:?}", n.kind);
        }
        for &(place, kind, s) in sh::SITES {
            assert!(at.distance(s) >= strike, "{name} near {place} {kind}");
        }
        for &f in sh::FINDS {
            assert!(at.distance(f) >= strike, "{name} near a find at {f}");
        }
        for r in &rests {
            assert!(
                at.distance(r.at) > 72.0 + 40.0,
                "{name} within reach of {}",
                r.name
            );
        }
        let door = level.home;
        let (dx, dy) = (
            (door.min.x - at.x).max(0.0).max(at.x - door.max.x),
            (door.min.y - at.y).max(0.0).max(at.y - door.max.y),
        );
        assert!(dx.hypot(dy) > 72.0 + 40.0, "{name} reaches the home door");
        // The co-op walks: strike reach clear of x 100..240 and 2,400..2,540.
        for (a, b) in [(100.0, 240.0), (2400.0, 2540.0)] {
            assert!(
                at.x + 72.0 < a || at.x - 72.0 > b,
                "{name} reaches {a}..{b}"
            );
        }
        // Nor a climb's foot.
        for (ledge, climb, _) in sh::KELP_LEDGES {
            let foot = Vec2::new(climb.x, climb.bottom);
            assert!(at.distance(foot) >= strike, "{name} reaches {ledge}");
        }
    }
}

/// The existing fair snapper bot (`economy_skill_scaling.rs`): walk up, read
/// the tell, dodge through the shock, smash the recovery. Fights every shrimp
/// in `spots` (nearest live one first); whether all are beaten, and the health
/// left.
fn bare_crab_beats(start: Vec2, spots: &[Vec2]) -> (bool, f32) {
    use crabsim::enemy::State;
    let mut sim = route::world(KIT, start, false);
    for &s in spots {
        sim.spawn_enemy(crabsim::Kind::Snapper, s);
    }
    for _ in 0..30 {
        sim.tick();
    }
    let live = |sim: &Sim| {
        let me = sim.crab(route::ME).expect("crab").mover.pos.x;
        sim.enemies()
            .iter()
            .filter(|e| e.state != State::Gone)
            .min_by(|a, b| (a.pos.x - me).abs().total_cmp(&(b.pos.x - me).abs()))
            .cloned()
    };
    for tick in 0..18000u32 {
        let Some(e) = live(&sim) else { break };
        let me = sim.crab(route::ME).expect("crab").mover.pos.x;
        let dir = (e.pos.x - me).signum();
        let close = (e.pos.x - me).abs() < 70.0;
        let answer = e.state == State::Tell && e.state_tick + 4 >= e.state_len;
        let punish = matches!(e.state, State::Recover | State::Stagger);
        sim.set_controls(
            route::ME,
            Controls {
                walk: if answer {
                    -dir
                } else if close && e.state == State::Idle {
                    // Close, but not noticed: shuffle (it feels steps).
                    if tick % 40 < 20 {
                        0.4 * dir
                    } else {
                        -0.4 * dir
                    }
                } else if close {
                    0.0
                } else {
                    0.8 * dir
                },
                // Hop now and then: the boulder and the slope are in the way.
                jump_pressed: !close && tick % 50 == 0,
                jump_held: tick % 50 < 30,
                dodge_pressed: answer,
                smash_pressed: close && punish && e.state_tick.is_multiple_of(8),
                ..Default::default()
            },
        );
        sim.tick();
        if sim.crab(route::ME).expect("crab").fighter.vitals.health <= 0.0 {
            return (false, 0.0);
        }
    }
    let health = sim.crab(route::ME).expect("crab").fighter.vitals.health;
    (live(&sim).is_none(), health)
}

#[test]
fn a_level_one_crab_with_no_kit_beats_each_new_shrimp_with_its_colony_beside_it() {
    let level = Level::test_level();
    for &(name, _, spot) in sh::FIGHTS {
        // The old shrimp beside it fights too.
        let mut spots = vec![spot];
        spots.extend(
            level
                .foes
                .iter()
                .filter(|(k, p)| *k == crabsim::Kind::Snapper && (p.x - spot.x).abs() < 80.0)
                .map(|(_, p)| *p)
                .filter(|p| *p != spot),
        );
        // Walked up to from 190 off on the same ground (the far ledge's from
        // the east: the block and the trench are in the way from the west).
        let x = if spot.x > 1900.0 {
            spots.iter().map(|p| p.x).fold(f32::MIN, f32::max) + 190.0
        } else {
            spots.iter().map(|p| p.x).fold(f32::MAX, f32::min) - 190.0
        };
        let start = Vec2::new(x, level.floor_height(x));
        let (won, health) = bare_crab_beats(start, &spots);
        println!(
            "{name} with {} beside it: won {won}, {health:.0} health left",
            spots.len() - 1
        );
        assert!(spots.len() >= 2, "{name} has no colony");
        assert!(won && health > 0.0, "{name}");
    }
}

#[test]
fn each_kelp_ledge_is_climbed_from_the_floor_by_a_crab_with_no_upgrade() {
    let level = Level::test_level();
    let t = Tuning::default();
    for (name, climb, ledge) in sh::KELP_LEDGES {
        let height = climb.top - climb.bottom;
        assert!((200.0..=500.0).contains(&height), "{name}: {height} tall");
        assert_eq!(climb.kind, crabsim::level::Growth::Kelp);
        assert!(level.climbs.contains(climb), "{name} is not in the level");
        assert!(level.rocks.contains(ledge), "{name} has no ledge");
        let top = ledge.max.y;
        assert!(
            climb.top > top + 10.0,
            "{name}: the stalk ends under its ledge"
        );
        // Out of every creature's reach: the stalk's foot and the ledge top.
        for at in [
            Vec2::new(climb.x, climb.bottom),
            Vec2::new(ledge.center().x, top),
        ] {
            for &(kind, f) in &foes(&level) {
                assert!(at.distance(f) > danger(kind, &t), "{name}: {kind:?} at {f}");
            }
        }
        // Up from the side the ledge lies on, stepping off onto it.
        let dir = (ledge.center().x - climb.x).signum();
        // (From the floor beside it: the block's stalk grows on the block, a
        // hop up.)
        let sx = climb.x + dir * 55.0;
        let mut sim = route::world(KIT, Vec2::new(sx, level.floor_height(sx)), false);
        let mut tick = 0;
        let up = hold(
            &mut sim,
            |m, _| {
                tick += 1;
                Controls {
                    walk: if m.pos.y >= top + 10.0 {
                        // Steer onto the ledge's middle and sink.
                        ((ledge.center().x - m.pos.x) / 40.0).clamp(-1.0, 1.0)
                    } else if m.climbing {
                        0.0
                    } else {
                        -dir
                    },
                    jump_pressed: tick % 40 == 1 && !m.climbing,
                    jump_held: m.pos.y < top + 10.0,
                    ..Default::default()
                }
            },
            40.0,
            |m| m.grounded && m.pos.y > top - 5.0,
        );
        let m = crab(&sim);
        println!("{name}: up={up} at {:?}, climb xp {}", m.pos, m.climb_xp);
        assert!(
            up,
            "{name}: the crab never stands on its ledge: {:?}",
            m.pos
        );
        assert!(m.climb_xp > 0.0, "{name} pays no Climbing XP");
        assert!(m.pos.x >= ledge.min.x - 45.0 && m.pos.x <= ledge.max.x + 45.0);
        sim.set_controls(route::ME, Controls::default());
        for _ in 0..240 {
            sim.tick();
        }
        let m = crab(&sim);
        assert!(
            m.grounded && (m.pos.y - top).abs() < 2.0,
            "{name}: {:?}",
            m.pos
        );
    }
}

/// Off the lookout's east edge: run, jump, then strokes `n` times.
fn leap(eggs: u32, n: u32) -> (Sim, bool) {
    let top = sh::LOOKOUT_TOP;
    let mut sim = route::world(
        Kit::eggs(eggs),
        Vec2::new(sh::LOOKOUT.max.x - 30.0, top),
        false,
    );
    for _ in 0..60 {
        sim.tick();
    }
    let mut presses = 0;
    let mut since = 0u32;
    let mut jumped = false;
    let mut across = false;
    for _ in 0..(8.0 * 120.0) as u32 {
        let m = crab(&sim);
        let edge = m.pos.x >= sh::LOOKOUT.max.x - 8.0;
        let mut c = Controls {
            walk: 1.0,
            ..Default::default()
        };
        since += 1;
        if m.grounded && edge && !jumped {
            c.jump_pressed = true;
            c.jump_held = true;
            jumped = true;
            since = 0;
        } else if jumped && !m.grounded && presses < n && since >= 24 {
            c.jump_pressed = true;
            presses += 1;
            since = 0;
        }
        c.jump_held |= jumped && !m.grounded && m.vel.y > 0.0;
        sim.set_controls(route::ME, c);
        sim.tick();
        let m = crab(&sim);
        if m.grounded && m.pos.x > sh::FAR_LOOKOUT.min.x && m.pos.y > top - 5.0 {
            across = true;
            break;
        }
        if jumped && m.grounded && m.pos.y < top - 5.0 {
            break;
        }
    }
    (sim, across)
}

#[test]
fn the_paddle_gap_is_crossed_with_a_stroke_or_two_and_a_miss_lands_on_the_floor() {
    let level = Level::test_level();
    let gap = sh::FAR_LOOKOUT.min.x - sh::LOOKOUT.max.x;
    assert!((200.0..=300.0).contains(&gap), "gap {gap}");
    let floor_under = level.floor_height(sh::LOOKOUT.max.x + gap / 2.0);
    assert!(sh::LOOKOUT_TOP - floor_under >= 150.0);
    // The far lookout holds a site worth crossing for.
    assert!(sh::SITES.iter().filter(|s| s.0 == "far lookout").count() >= 2);
    // A jump alone falls short, and the crab lands on the floor, unhurt.
    let (sim, across) = leap(0, 0);
    let m = crab(&sim);
    println!("no strokes: across={across}, landed {:?}", m.pos);
    assert!(!across);
    let mut sim = sim;
    for _ in 0..(6.0 * 120.0) as u32 {
        sim.set_controls(route::ME, Controls::default());
        sim.tick();
    }
    let m = crab(&sim);
    let hp = sim.crab(route::ME).expect("crab").fighter.vitals.health;
    println!("missed: {:?}, health {hp}", m.pos);
    assert!(
        m.grounded && m.pos.y < sh::LOOKOUT_TOP - 100.0,
        "{:?}",
        m.pos
    );
    assert!(hp > 0.0);
    // Stepping off the edge without a jump lands on the floor, unhurt.
    let mut sim = route::world(
        KIT,
        Vec2::new(sh::LOOKOUT.max.x - 20.0, sh::LOOKOUT_TOP),
        false,
    );
    for _ in 0..(10.0 * 120.0) as u32 {
        sim.set_controls(
            route::ME,
            Controls {
                walk: 0.3,
                ..Default::default()
            },
        );
        sim.tick();
    }
    let m = crab(&sim);
    let hp = sim.crab(route::ME).expect("crab").fighter.vitals.health;
    println!("stepped off: {:?}, health {hp}", m.pos);
    assert!(m.grounded && (m.pos.y - level.floor_height(m.pos.x)).abs() < 3.0);
    assert!(hp >= 100.0, "{hp}");
    // One or two strokes carry across.
    let mut best = None;
    for (eggs, strokes) in [(1, 1), (2, 2), (2, 1)] {
        let (sim, across) = leap(eggs, strokes);
        println!(
            "{eggs} eggs, {strokes} strokes: across={across} at {:?}",
            crab(&sim).pos
        );
        if across {
            best = Some((eggs, strokes));
            break;
        }
    }
    assert!(best.is_some(), "no stroke or two crosses the gap");
}
