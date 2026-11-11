//! The 20-minute loop (`docs/design/core-loop.md`), played headless: a
//! scripted crab pins a want, goes out, crosses, takes the prize, comes home
//! and builds. Each beat is timed and judged against the beat sheet's window.
//!
//! How it is measured (so the numbers can be trusted, and argued with):
//! - Home and Build count a stated UI time (`UI_HOME_SECS`, `UI_BUILD_SECS`:
//!   walking to the bench, reading the board, tapping the buttons) plus the
//!   real ticks the sim commands take.
//! - Way out, Crossing and Way home are route-solver times (the same solver
//!   as `economy_pacing`), cut at landmarks by kind: the way out ends where
//!   the route first enters an open-water or descent landmark, the crossing
//!   ends where it first enters the shelter (refuge, tunnel, mouth, chamber)
//!   nearest the target, and the way home adds the solver's own tired-leg
//!   seconds. The target is found by node kind or creature, never by a fixed
//!   x, so a new map only needs the `ROUTES` re-measured.
//! - Prize and the fight are real ticks: the crab gathers the missing items
//!   at the real nodes, or beats the real creature with a scripted fighter,
//!   walking on the real floor.
//!
//! The solver is slow, so its numbers are cached in `ROUTES`. Re-measure with
//! `cargo test --release -p crabsim --test loop_pacing -- --ignored --nocapture measure_routes`
//! and paste the printed `ROUTES`. The default test only fails on gross
//! breakage: a loop that cannot be completed, or a total far off the scale.

mod common;

use bevy_math::{Rect, Vec2};
use common::route::{self, Kit};
use crabsim::economy::food::Buff;
use crabsim::economy::home::Piece;
use crabsim::economy::place::nodes;
use crabsim::economy::{Item, NodeKind, Tool, ToolKind, xp};
use crabsim::enemy::State;
use crabsim::level::landmarks::Kind as Place;
use crabsim::{Carry, Controls, Kind, Level, PlayerId, Sim, Skill};

const P0: PlayerId = PlayerId(0);
const HZ: f64 = 120.0;

/// Assumed UI seconds at home before leaving: reading the board, pinning the
/// want, walking to the bench and cooking one meal.
const UI_HOME_SECS: f64 = 60.0;
/// Assumed UI seconds at home after returning: opening the Home tab, picking
/// the piece, confirming (and each craft on the way).
const UI_BUILD_SECS: f64 = 45.0;
const UI_CRAFT_SECS: f64 = 15.0;

/// The beat sheet's windows in minutes: (name, from, to).
const WINDOWS: [(&str, f64, f64); 6] = [
    ("Home", 0.0, 2.0),
    ("Way out", 2.0, 6.0),
    ("Crossing", 6.0, 12.0),
    ("Prize", 12.0, 15.0),
    ("Way home", 15.0, 19.0),
    ("Build", 19.0, 20.0),
];

/// A beat is OK between this fraction of its window and its multiple.
const TOO_SHORT: f64 = 0.5;
const TOO_LONG: f64 = 1.5;

// ---- The wants ----

/// What the scripted crab goes for.
#[derive(Clone, Copy, PartialEq)]
enum How {
    /// Cut glass at a tier 3 garden.
    Glass,
    /// Beat a mantis shrimp for its club.
    Club,
    /// Cut kelp beside the door.
    Kelp,
}

struct Want {
    name: &'static str,
    how: How,
    piece: Piece,
    /// The recipe word pinned (None: the piece itself, which cannot be).
    /// Kept as a field so the table can say what the board does.
    levels: &'static [(Skill, u32)],
    /// What is already in the bank when the day starts (earlier trips).
    bank: &'static [(Item, u32)],
    /// What the crab came for.
    needs: (Item, u32),
    /// Cutter / pry tool tier fitted.
    tool_tier: u8,
    /// The kit the route solver is given: what this crab would have.
    kit: Kit,
    /// The least total, in minutes, that is not a soft lock (a near-home
    /// loop is short by design).
    floor_min: f64,
}

const WANTS: [Want; 3] = [
    Want {
        name: "glass lamp (gather, tier 3)",
        how: How::Glass,
        piece: Piece::GlassLamp,
        // Harvesting 30 opens the garden; Building 8 the lamp.
        levels: &[(Skill::Harvesting, 30), (Skill::Building, 8)],
        bank: &[(Item::KelpCord, 1), (Item::Mussel, 2)],
        needs: (Item::Glass, 4),
        tool_tier: 2,
        kit: Kit {
            eggs: 6,
            scissors: true,
            glass: true,
            crusher: true,
            dactyls: true,
        },
        floor_min: 3.0,
    },
    Want {
        name: "bone arch (fight, mantis club)",
        how: How::Club,
        piece: Piece::BoneArch,
        // Fighting stays level 1: the win is on skill.
        levels: &[(Skill::Building, 35)],
        bank: &[(Item::WhaleBone, 2), (Item::Clay, 3), (Item::Mussel, 2)],
        needs: (Item::MantisClub, 1),
        tool_tier: 0,
        kit: Kit {
            eggs: 1,
            scissors: false,
            glass: false,
            crusher: false,
            dactyls: false,
        },
        floor_min: 1.5,
    },
    Want {
        name: "kelp curtain (low attention)",
        how: How::Kelp,
        piece: Piece::KelpCurtain,
        levels: &[(Skill::Building, 3)],
        bank: &[(Item::Mussel, 2)],
        // Six kelp for the curtain and four more for its two cords.
        needs: (Item::Kelp, 10),
        tool_tier: 0,
        kit: Kit {
            eggs: 0,
            scissors: false,
            glass: false,
            crusher: false,
            dactyls: false,
        },
        floor_min: 1.5,
    },
];

// ---- The cached route numbers ----

/// One measured trip: seconds from home to the first open water or descent,
/// to the target's shelter, to the target itself (the approach point), the
/// way back to the home side, the tired-leg seconds on it, and where the
/// target and the home-side arrival were (to notice a moved map).
#[derive(Clone, Copy, Debug)]
struct Route {
    open: f64,
    land: f64,
    target: f64,
    back: f64,
    dry: f64,
    target_x: f32,
    home_x: f32,
}

/// Measured by `measure_routes` (release, a few minutes). Order: `WANTS`.
const ROUTES: [Route; 3] = [
    Route {
        open: 14.48,
        land: 93.12,
        target: 96.42,
        back: 66.89,
        dry: 0.00,
        target_x: 5443.0,
        home_x: 1228.0,
    },
    Route {
        open: 6.77,
        land: 6.77,
        target: 6.77,
        back: 0.00,
        dry: 0.00,
        target_x: 930.0,
        home_x: 894.0,
    },
    Route {
        open: 2.36,
        land: 2.36,
        target: 2.36,
        back: 0.00,
        dry: 0.00,
        target_x: 187.0,
        home_x: 129.0,
    },
];

// ---- Finding things by kind, not by x ----

/// Where the crab goes for the want: the approach point (a standing place on
/// the floor) and the thing's own position.
fn target_of(level: &Level, how: How) -> (Vec2, Vec2) {
    match how {
        How::Glass => {
            let n = nodes(level)
                .into_iter()
                .filter(|n| n.kind == NodeKind::GlassSpongeGarden)
                .min_by(|a, b| a.pos.x.total_cmp(&b.pos.x))
                .expect("a glass sponge garden");
            (n.pos, n.pos)
        }
        How::Club => {
            let (_, at) = level
                .foes
                .iter()
                .filter(|(k, _)| *k == Kind::Mantis)
                .min_by(|a, b| a.1.x.total_cmp(&b.1.x))
                .expect("a mantis shrimp");
            // The scripted fighter starts a short walk from it.
            let stand = at.x - FIGHT_GAP;
            (Vec2::new(stand, level.floor_height(stand) + 2.0), *at)
        }
        How::Kelp => {
            let n = nodes(level)
                .into_iter()
                .filter(|n| n.kind == NodeKind::KelpStand)
                .min_by(|a, b| a.pos.x.total_cmp(&b.pos.x))
                .expect("a kelp stand");
            (n.pos, n.pos)
        }
    }
}

const FIGHT_GAP: f32 = 190.0;

fn is_open(k: Place) -> bool {
    matches!(k, Place::OpenWater | Place::Drop)
}

fn is_shelter(k: Place) -> bool {
    matches!(
        k,
        Place::Refuge | Place::Tunnel | Place::Mouth | Place::Chamber { .. }
    )
}

fn rect_dist(r: &Rect, p: Vec2) -> f32 {
    let dx = (r.min.x - p.x).max(0.0).max(p.x - r.max.x);
    let dy = (r.min.y - p.y).max(0.0).max(p.y - r.max.y);
    dx.hypot(dy)
}

/// Open-water rects and the shelter nearest `target` (not home).
fn landmarks_for(level: &Level, target: Vec2) -> (Vec<Rect>, Option<(String, Rect)>) {
    let marks = level.landmarks();
    let open = marks
        .iter()
        .filter(|l| is_open(l.kind))
        .map(|l| l.rect)
        .collect();
    let shelter = marks
        .iter()
        .filter(|l| is_shelter(l.kind) && l.name != "home")
        .min_by(|a, b| {
            let key = |l: &crabsim::level::landmarks::Landmark| {
                (
                    rect_dist(&l.rect, target),
                    l.rect.size().x * l.rect.size().y,
                )
            };
            let (ka, kb) = (key(a), key(b));
            ka.0.total_cmp(&kb.0).then(ka.1.total_cmp(&kb.1))
        })
        .map(|l| (l.name.clone(), l.rect));
    (open, shelter)
}

fn inside(r: &Rect, p: Vec2) -> bool {
    p.x >= r.min.x && p.x <= r.max.x && p.y >= r.min.y && p.y <= r.max.y
}

// ---- Measuring the routes (slow; ignored) ----

fn measure(w: &Want) -> Route {
    use std::cell::Cell;
    let level = Level::test_level();
    let (goal, thing) = target_of(&level, w.how);
    let kit = w.kit;
    let best = Cell::new(f64::INFINITY);
    let mut s = route::survey_from(
        route::start_node(kit, route::HOME, false),
        kit,
        |n| {
            if n.mover().pos.distance(goal) <= 70.0 {
                best.set(best.get().min(n.time));
            }
            n.time >= best.get() || n.time > 400.0
        },
        route::BUDGET,
    );
    let hit = (0..s.nodes.len())
        .filter(|&j| s.nodes[j].mover().pos.distance(goal) <= 70.0)
        .min_by(|&a, &b| s.nodes[a].time.total_cmp(&s.nodes[b].time))
        .unwrap_or_else(|| {
            panic!(
                "{}: the solver cannot reach {goal:?} with {}",
                w.name,
                kit.name()
            )
        });
    let (open, shelter) = landmarks_for(&level, thing);
    let trace = s.trace(hit);
    let arrive = s.nodes[hit].time;
    let t_open = trace
        .iter()
        .find(|(_, p)| open.iter().any(|r| inside(r, *p)))
        .map(|(t, _)| *t);
    let t_land = match (t_open, &shelter) {
        (Some(o), Some((_, r))) => trace
            .iter()
            .find(|(t, p)| *t >= o && inside(r, *p))
            .map_or(arrive, |(t, _)| *t),
        _ => arrive,
    };
    let (back, dry) = match route::way_home(&s, hit, kit) {
        Some(h) => (h.time - arrive, h.dry - s.nodes[hit].dry),
        None => panic!("{}: no way home from {goal:?}", w.name),
    };
    let home_x = route::way_home(&s, hit, kit).map_or(0.0, |h| h.mover().pos.x);
    // Silence the unused-mut lint on older toolchains.
    s.nodes.shrink_to_fit();
    Route {
        open: t_open.unwrap_or(arrive),
        land: t_land,
        target: arrive,
        back,
        dry,
        target_x: goal.x,
        home_x,
    }
}

#[test]
#[ignore = "route solving, several minutes in release; prints ROUTES"]
fn measure_routes() {
    let routes: Vec<Route> = std::thread::scope(|sc| {
        let jobs: Vec<_> = WANTS
            .iter()
            .map(|w| {
                sc.spawn(move || {
                    let r = measure(w);
                    println!("{}: {r:?}", w.name);
                    r
                })
            })
            .collect();
        jobs.into_iter()
            .map(|j| j.join().expect("a measured route"))
            .collect()
    });
    for (w, r) in WANTS.iter().zip(&routes) {
        println!("{}: {r:?}", w.name);
    }
    println!("ROUTES = [");
    for r in &routes {
        println!(
            "    Route {{ open: {:.2}, land: {:.2}, target: {:.2}, back: {:.2}, dry: {:.2}, target_x: {:.0}, home_x: {:.0} }},",
            r.open, r.land, r.target, r.back, r.dry, r.target_x, r.home_x
        );
    }
    println!("];");
}

// ---- Real ticks ----

fn secs(sim: &Sim) -> f64 {
    sim.tick_count() as f64 / HZ
}

fn level_up(sim: &mut Sim, skill: Skill, level: u32) {
    sim.crab_mut(P0)
        .expect("crab")
        .econ
        .skills
        .set(skill, xp::TABLE[level as usize]);
}

fn shell_count(sim: &Sim, item: Item) -> u32 {
    let m = &sim.crab(P0).expect("crab").mover;
    m.clinging
        .iter()
        .filter(|c| **c == Carry::Item(item))
        .count() as u32
}

fn have(sim: &Sim, item: Item) -> u32 {
    sim.crab(P0).expect("crab").stash.count(item) + shell_count(sim, item)
}

fn step(sim: &mut Sim, c: Controls) {
    sim.set_controls(P0, c);
    sim.tick();
}

fn walk(dir: f32) -> Controls {
    Controls {
        walk: dir,
        ..Controls::default()
    }
}

/// A fresh-ish crab at `at`: the levels, the bank and the tools of the want.
fn crab_for(w: &Want, at: Vec2, foes: bool) -> Sim {
    let mut sim = route::world(w.kit, at, foes);
    for &(skill, level) in w.levels {
        level_up(&mut sim, skill, level);
    }
    let crab = sim.crab_mut(P0).expect("crab");
    crab.stash = crabsim::Stash::of(w.bank);
    if w.tool_tier > 0 {
        for kind in ToolKind::ALL {
            crab.econ.tools[kind.index()] = Tool::new(kind, w.tool_tier);
        }
    }
    sim
}

/// A crab tucked in at home, set up the same way.
fn home_sim(w: &Want) -> Sim {
    let mut sim = Sim::with_one_crab(Vec2::new(180.0, 0.0));
    for &(skill, level) in w.levels {
        level_up(&mut sim, skill, level);
    }
    sim.crab_mut(P0).expect("crab").stash = crabsim::Stash::of(w.bank);
    walk_in(&mut sim);
    sim
}

/// Walk left until tucked in; the ticks it took.
fn walk_in(sim: &mut Sim) -> u64 {
    let from = sim.tick_count();
    for _ in 0..(120 * 60) {
        if sim.crab(P0).expect("crab").mover.tucked_in() {
            break;
        }
        step(sim, walk(-1.0));
    }
    step(sim, Controls::default());
    sim.tick_count() - from
}

/// Beat 0: pin the want, cook one meal. Sim seconds, and whether the board
/// could pin the piece itself.
fn home_beat(w: &Want) -> (f64, bool) {
    let mut sim = home_sim(w);
    let start = secs(&sim);
    let pinned = sim.pin_want(P0, w.piece.word());
    assert_eq!(
        sim.make_econ(P0, "shucked-mussels"),
        Ok(()),
        "{}: cook",
        w.name
    );
    for _ in 0..30 {
        sim.tick();
    }
    (secs(&sim) - start, pinned)
}

/// What the prize beat took, with the pieces it is made of.
struct Prize {
    /// Walking from the approach point to the first node and between nodes.
    walking: f64,
    /// Seconds spent working nodes (or in the fight).
    working: f64,
    /// Nodes (or creatures) worked.
    stops: u32,
    /// Per item seconds, for the reason string.
    each: f64,
    /// The shell when it was done.
    shell: Vec<Carry>,
    won: bool,
}

/// Gather `count` of `item` at the nearest ready nodes of `kind`.
fn gather_prize(w: &Want, kind: NodeKind, at: Vec2) -> Prize {
    let mut sim = crab_for(w, at, false);
    let (item, count) = w.needs;
    let (mut walking, mut working) = (0.0, 0.0);
    let mut worked = std::collections::BTreeSet::new();
    for _ in 0..(HZ as u64 * 900) {
        if shell_count(&sim, item) >= count {
            break;
        }
        let m = &sim.crab(P0).expect("crab").mover;
        let px = m.pos.x;
        let ready = sim
            .placed_nodes()
            .iter()
            .enumerate()
            .filter(|(i, n)| n.kind == kind && sim.shared.nodes[*i].charges > 0)
            .min_by(|a, b| (a.1.pos.x - px).abs().total_cmp(&(b.1.pos.x - px).abs()))
            .map(|(i, n)| (i, n.pos.x));
        let Some((i, x)) = ready else {
            // All spent: wait for them to regrow.
            step(&mut sim, Controls::default());
            walking += 1.0 / HZ;
            continue;
        };
        if (x - px).abs() > 6.0 {
            step(&mut sim, walk((x - px).signum()));
            walking += 1.0 / HZ;
        } else {
            worked.insert(i);
            step(
                &mut sim,
                Controls {
                    gather_held: true,
                    ..Controls::default()
                },
            );
            working += 1.0 / HZ;
        }
    }
    Prize {
        walking,
        working,
        stops: worked.len() as u32,
        each: working / f64::from(count.max(1)),
        shell: sim.crab(P0).expect("crab").mover.clinging.clone(),
        won: shell_count(&sim, item) >= count,
    }
}

/// The scripted level 1 fight (as in `economy_skill_scaling`): read the tell,
/// parry the mantis at the end of it, smash it in its recovery, then eat.
fn fight_prize(w: &Want, at: Vec2, foe_at: Vec2) -> Prize {
    let mut sim = crab_for(w, at, false);
    sim.spawn_enemy(Kind::Mantis, foe_at);
    let mut won = false;
    let mut fight_ticks = 0u64;
    let mut walking = 0.0;
    for _ in 0..(HZ as u64 * 240) {
        let Some(e) = sim.enemies().iter().find(|e| e.kind == Kind::Mantis) else {
            break;
        };
        let me = sim.crab(P0).expect("crab").mover.pos.x;
        let dir = (e.pos.x - me).signum();
        let gap = (e.pos.x - me).abs();
        let gone = e.state == State::Gone;
        let close = gap < 70.0;
        let answer = e.state == State::Tell && e.state_tick + 4 >= e.state_len;
        let punish = matches!(e.state, State::Recover | State::Stagger);
        let tick_in = e.state_tick;
        let c = if gone {
            // Walk onto the body: eating it gives the drop.
            if gap > 4.0 {
                walking += 1.0 / HZ;
                walk(dir)
            } else {
                Controls::default()
            }
        } else {
            Controls {
                walk: if close || answer { 0.0 } else { 0.8 * dir },
                smash_pressed: close && punish && tick_in.is_multiple_of(8),
                jab_pressed: close && !answer && !punish && tick_in.is_multiple_of(30),
                block_held: answer,
                block_pressed: answer,
                ..Controls::default()
            }
        };
        step(&mut sim, c);
        if !gone {
            fight_ticks += 1;
        }
        let crab = sim.crab(P0).expect("crab");
        if crab.fighter.vitals.health <= 0.0 {
            break;
        }
        if shell_count(&sim, w.needs.0) >= w.needs.1 {
            won = true;
            break;
        }
    }
    Prize {
        walking,
        working: fight_ticks as f64 / HZ,
        stops: 1,
        each: fight_ticks as f64 / HZ,
        shell: sim.crab(P0).expect("crab").mover.clinging.clone(),
        won,
    }
}

/// The kelp: cut at the pockets beside the door, home whenever the shell is
/// full, until the bank holds enough. Returns the whole of it in `walking`
/// (the walks), `working` (the cuts) and the home seconds in `stops`.
fn kelp_prize(w: &Want) -> (Prize, Sim) {
    let mut sim = home_sim(w);
    let (item, count) = w.needs;
    let kelp = sim.placed_nodes().to_vec();
    let kinds: Vec<usize> = kelp
        .iter()
        .enumerate()
        .filter(|(_, n)| n.kind == NodeKind::KelpStand && n.pos.x < 600.0)
        .map(|(i, _)| i)
        .collect();
    let start = sim.tick_count();
    let (mut working, mut homeward) = (0.0, false);
    let mut worked = std::collections::BTreeSet::new();
    let cap = sim.crab(P0).expect("crab").mover.carries(&sim.tuning);
    for _ in 0..(HZ as u64 * 900) {
        let c = sim.crab(P0).expect("crab");
        let m = &c.mover;
        if m.tucked_in() {
            let carrying = m.clinging.iter().any(|c| matches!(c, Carry::Item(_)));
            if carrying {
                step(&mut sim, Controls::default());
            } else if have(&sim, item) >= count {
                break;
            } else {
                homeward = false;
                step(&mut sim, walk(1.0));
            }
            continue;
        }
        let full = m.clinging.len() >= cap || have(&sim, item) >= count;
        if full {
            homeward = true;
        }
        if homeward || m.shelter != crabsim::home::Shelter::Out {
            step(&mut sim, walk(if homeward { -1.0 } else { 1.0 }));
            continue;
        }
        let px = m.pos.x;
        let target = kinds
            .iter()
            .copied()
            .filter(|&i| sim.shared.nodes[i].charges > 0)
            .min_by(|&a, &b| {
                (kelp[a].pos.x - px)
                    .abs()
                    .total_cmp(&(kelp[b].pos.x - px).abs())
            });
        let Some(i) = target else {
            step(&mut sim, Controls::default());
            continue;
        };
        let dx = kelp[i].pos.x - px;
        if dx.abs() > 6.0 {
            step(&mut sim, walk(dx.signum()));
        } else {
            worked.insert(i);
            working += 1.0 / HZ;
            step(
                &mut sim,
                Controls {
                    gather_held: true,
                    ..Controls::default()
                },
            );
        }
    }
    let total = (sim.tick_count() - start) as f64 / HZ;
    let won = have(&sim, item) >= count;
    (
        Prize {
            walking: total - working,
            working,
            stops: worked.len() as u32,
            each: working / f64::from(count.max(1)),
            shell: Vec::new(),
            won,
        },
        sim,
    )
}

/// Beat 5: from the home-side arrival `from_x`, walk in and tuck in (real
/// ticks, counted with the way home), then make what is short and build the
/// piece. Returns the walk-in seconds, the build's sim seconds, the UI
/// seconds assumed and whether it was built.
fn build_beat(
    w: &Want,
    mut sim: Sim,
    shell: &[Carry],
    from_x: Option<f32>,
) -> (f64, f64, f64, bool) {
    if !shell.is_empty() {
        sim.crab_mut(P0).expect("crab").mover.clinging = shell.to_vec();
    }
    if let Some(x) = from_x {
        let y = sim.level.floor_height(x) + 2.0;
        let m = &mut sim.crab_mut(P0).expect("crab").mover;
        m.pos = Vec2::new(x, y);
        m.vel = Vec2::ZERO;
        m.shelter = crabsim::home::Shelter::Out;
    }
    let walk_ticks = walk_in(&mut sim);
    let start = secs(&sim);
    // The shell empties into the bank once tucked in.
    for _ in 0..240 {
        step(&mut sim, Controls::default());
    }
    let mut crafts = 0.0;
    while have(&sim, Item::KelpCord) < kelp_cords_needed(w) {
        if sim.make_econ(P0, "kelp-cord").is_err() {
            return (
                walk_ticks as f64 / HZ,
                secs(&sim) - start,
                UI_BUILD_SECS,
                false,
            );
        }
        crafts += UI_CRAFT_SECS;
    }
    let built = sim.build_piece(P0, w.piece).is_ok() && sim.home_pieces().contains(&w.piece);
    (
        walk_ticks as f64 / HZ,
        secs(&sim) - start,
        UI_BUILD_SECS + crafts,
        built,
    )
}

fn kelp_cords_needed(w: &Want) -> u32 {
    w.piece
        .needs()
        .iter()
        .find(|(i, _)| *i == Item::KelpCord)
        .map_or(0, |(_, n)| *n)
}

// ---- The table ----

struct Beat {
    name: &'static str,
    minutes: f64,
    reason: String,
}

fn verdict(i: usize, minutes: f64) -> &'static str {
    let (_, from, to) = WINDOWS[i];
    let len = to - from;
    if minutes < TOO_SHORT * len {
        "too short"
    } else if minutes > TOO_LONG * len {
        "too long"
    } else {
        "OK"
    }
}

struct Run {
    beats: Vec<Beat>,
    total: f64,
    prize_won: bool,
    built: bool,
}

fn walk_home_of(b: &(f64, f64, f64, bool)) -> f64 {
    b.0
}

fn units(x: f32) -> String {
    format!("{:.0}", x)
}

fn play(w: &Want, route: &Route) -> Run {
    let level = Level::test_level();
    let (goal, thing) = target_of(&level, w.how);
    let (open, shelter) = landmarks_for(&level, thing);
    let stale = (route.target_x - goal.x).abs() > 1.0;
    let home = level.home_den.x;
    let (home_sim_secs, pinned_piece) = home_beat(w);
    let home_secs = UI_HOME_SECS + home_sim_secs;

    let m = |s: f64| s / 60.0;
    let mut beats = Vec::new();
    let name = |i: usize| WINDOWS[i].0;
    beats.push(Beat {
        name: name(0),
        minutes: m(home_secs),
        reason: format!(
            "{UI_HOME_SECS:.0} s assumed UI plus {home_sim_secs:.1} s of real commands; the board {} the {} itself",
            if pinned_piece { "pins" } else { "cannot pin" },
            w.piece.name()
        ),
    });

    let (prize, build, near_home) = match w.how {
        How::Glass => {
            let p = gather_prize(w, NodeKind::GlassSpongeGarden, goal);
            let free = p.shell.clone();
            let b = build_beat(w, home_sim(w), &free, Some(route.home_x));
            (p, b, false)
        }
        How::Club => {
            let p = fight_prize(w, goal, thing);
            let free = p.shell.clone();
            let b = build_beat(w, home_sim(w), &free, Some(route.home_x));
            (p, b, false)
        }
        How::Kelp => {
            let (p, sim) = kelp_prize(w);
            let b = build_beat(w, sim, &[], None);
            (p, b, true)
        }
    };

    if near_home {
        // No route: everything is on foot beside the door.
        for (i, label) in [
            (1, "no way out: the kelp is beside the door"),
            (2, "no crossing"),
        ] {
            beats.push(Beat {
                name: name(i),
                minutes: 0.0,
                reason: label.to_string(),
            });
        }
    } else {
        let way_out = route.open;
        let crossing = route.land - route.open;
        let approach = route.target - route.land;
        let first_open = open
            .iter()
            .map(|r| r.min.x)
            .filter(|x| *x < goal.x)
            .fold(f32::INFINITY, f32::min);
        let has_open = route.open + 0.01 < route.target && first_open.is_finite();
        let shelter_name = shelter
            .as_ref()
            .map_or("none".to_string(), |(n, _)| n.clone());
        if has_open {
            beats.push(Beat {
                name: name(1),
                minutes: m(way_out),
                reason: format!(
                    "{way_out:.0} s to the first open water, {} units from the door (the known shallows are that wide)",
                    units(first_open - home)
                ),
            });
            beats.push(Beat {
                name: name(2),
                minutes: m(crossing),
                reason: format!(
                    "{crossing:.0} s from the first open water to the '{shelter_name}' shelter, {} units further on; {approach:.0} s more to the first {}",
                    units(goal.x - first_open),
                    if w.how == How::Club { "fighting stand" } else { "node" }
                ),
            });
        } else {
            beats.push(Beat {
                name: name(1),
                minutes: m(route.target),
                reason: format!(
                    "{:.0} s on foot to the {}, {} units from the door; there is no open water on this way",
                    route.target,
                    if w.how == How::Club { "fighting stand" } else { "node" },
                    units(goal.x - home)
                ),
            });
            beats.push(Beat {
                name: name(2),
                minutes: 0.0,
                reason:
                    "no crossing: nothing between the door and the prize is open water or a descent"
                        .to_string(),
            });
        }
    }

    let gardens = nodes(&level)
        .iter()
        .filter(|n| n.kind == NodeKind::GlassSpongeGarden)
        .count();
    let work_label = match w.how {
        How::Glass => format!(
            "{} glass at {:.1} s each at {} garden(s), plus {:.0} s walking; the map holds {} gardens ({} glass in all); filling half the {:.0} min window would take {:.0} glass",
            w.needs.1,
            prize.each,
            prize.stops,
            prize.walking,
            gardens,
            gardens * usize::from(NodeKind::GlassSpongeGarden.def().charges),
            WINDOWS[3].2 - WINDOWS[3].1,
            0.5 * (WINDOWS[3].2 - WINDOWS[3].1) * 60.0 / prize.each
        ),
        How::Club => format!(
            "the real fight took {:.0} s, then {:.0} s to the body; the mantis is {} units from the door",
            prize.working,
            prize.walking,
            units(thing.x - home)
        ),
        How::Kelp => format!(
            "{} kelp at {:.1} s each over {} stands, {:.0} s of walking and tucking in between loads",
            w.needs.1, prize.each, prize.stops, prize.walking
        ),
    };
    let approach = if near_home {
        0.0
    } else {
        route.target - route.land
    };
    let prize_secs = prize.working + prize.walking + approach;
    beats.push(Beat {
        name: name(3),
        minutes: m(prize_secs),
        reason: work_label,
    });

    let back_secs = if near_home {
        0.0
    } else {
        route.back + walk_home_of(&build)
    };
    beats.push(Beat {
        name: name(4),
        minutes: m(back_secs),
        reason: if near_home {
            "included in the prize beat (loads carried home as the shell filled)".to_string()
        } else {
            format!(
                "{:.0} s of route back to the home side ({:.0} s with tired paddle legs) plus {:.0} s walking in to the door",
                route.back, route.dry, walk_home_of(&build)
            )
        },
    });

    let (_, build_ticks, build_ui, built) = build;
    beats.push(Beat {
        name: name(5),
        minutes: m(build_ticks + build_ui),
        reason: format!(
            "{build_ui:.0} s assumed UI plus {build_ticks:.1} s of tucking in, crafting and building"
        ),
    });

    if stale {
        beats.push(Beat {
            name: "STALE",
            minutes: 0.0,
            reason: format!(
                "the target is now at x {} but ROUTES was measured at x {}: re-measure",
                units(goal.x),
                units(route.target_x)
            ),
        });
    }
    let total = beats
        .iter()
        .filter(|b| b.name != "STALE")
        .map(|b| b.minutes)
        .sum();
    Run {
        beats,
        total,
        prize_won: prize.won,
        built,
    }
}

fn print_table(w: &Want, run: &Run) {
    println!();
    println!("== {} (kit: {}) ==", w.name, w.kit.name());
    println!(
        "{:<9} {:>9} {:>7}  {:<10} reason",
        "beat", "window", "min", "verdict"
    );
    for (i, b) in run.beats.iter().enumerate() {
        if b.name == "STALE" {
            println!("!! {}", b.reason);
            continue;
        }
        let (_, from, to) = WINDOWS[i];
        // A beat that does not exist for a near-home loop has no verdict.
        let v = if b.minutes < 0.005 {
            "n/a"
        } else {
            verdict(i, b.minutes)
        };
        println!(
            "{:<9} {:>4.0}-{:<4.0} {:>7.2}  {:<10} {}",
            b.name, from, to, b.minutes, v, b.reason
        );
    }
    println!("{:<9} {:>9} {:>7.2}", "total", "20", run.total);
}

#[test]
fn the_twenty_minute_loop_completes_for_each_want() {
    println!(
        "beats measured on real ticks (prize, fight, kelp, home, build) and cached solver routes"
    );
    println!(
        "assumed UI: home {UI_HOME_SECS:.0} s, build {UI_BUILD_SECS:.0} s, each craft {UI_CRAFT_SECS:.0} s"
    );
    for (w, route) in WANTS.iter().zip(&ROUTES) {
        let run = play(w, route);
        print_table(w, &run);
        assert!(
            run.prize_won,
            "{}: the prize beat never got {:?}",
            w.name, w.needs
        );
        assert!(run.built, "{}: the piece was never built", w.name);
        assert!(
            run.total >= w.floor_min && run.total <= 45.0,
            "{}: {:.1} min is outside {}..45",
            w.name,
            run.total,
            w.floor_min
        );
    }
    let _ = Buff::Feast;
}
