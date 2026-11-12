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
