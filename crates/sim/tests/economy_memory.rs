//! The crab remembers the gathering nodes it has seen, and only those.

use bevy_math::Vec2;
use crabsim::economy::NodeKind;
use crabsim::{Level, Memory, PlayerId, Sim, Tuning};

const ME: PlayerId = PlayerId(0);
const FRIEND: PlayerId = PlayerId(1);

fn sim() -> Sim {
    let mut level = Level::test_level();
    level.currents.clear();
    Sim::new(Tuning::default(), level)
}

fn put(sim: &mut Sim, id: PlayerId, x: f32) {
    let y = sim.level.floor_height(x);
    let c = sim.crab_mut(id).unwrap();
    c.mover.pos = Vec2::new(x, y);
    c.mover.vel = Vec2::ZERO;
}

fn run(sim: &mut Sim, ticks: u32) {
    for _ in 0..ticks {
        sim.tick();
    }
}

fn start() -> Sim {
    let mut s = sim();
    s.add_crab(ME, Vec2::new(200.0, s.level.floor_height(200.0)));
    s
}

fn mem(sim: &Sim, id: PlayerId) -> &Memory {
    &sim.crab(id).unwrap().memory
}

/// The first node of `kind`.
fn node(sim: &Sim, kind: NodeKind) -> usize {
    sim.placed_nodes()
        .iter()
        .position(|n| n.kind == kind)
        .expect("kind placed")
}

#[test]
fn a_far_node_is_not_remembered() {
    let mut s = start();
    run(&mut s, 120);
    let far = node(&s, NodeKind::BambooCoral) as u32;
    assert!(mem(&s, ME).nodes.iter().all(|n| n.index != far));
    // Nearby ones are known, and none farther than the crab could see.
    assert!(!mem(&s, ME).nodes.is_empty());
    for n in &mem(&s, ME).nodes {
        let p = s.placed_nodes()[n.index as usize].pos;
        assert!((p.x - 200.0).abs() < 800.0, "node {} too far", n.index);
    }
}

#[test]
fn walking_past_a_node_remembers_it() {
    let mut s = start();
    let i = node(&s, NodeKind::WormMud);
    let x = s.placed_nodes()[i].pos.x;
    assert!(mem(&s, ME).nodes.iter().all(|n| n.index as usize != i));
    // Walk along the floor, a few units a step, from well short to past it.
    let mut at = x - 700.0;
    while at < x + 100.0 {
        put(&mut s, ME, at);
        run(&mut s, 12);
        at += 20.0;
    }
    let seen = mem(&s, ME)
        .nodes
        .iter()
        .find(|n| n.index as usize == i)
        .expect("remembered");
    assert_eq!(seen.kind, NodeKind::WormMud);
    assert!(seen.ready);
}

#[test]
fn walking_a_long_way_remembers_the_nodes_on_the_way() {
    let mut s = start();
    let i = node(&s, NodeKind::PitGlowSite);
    assert!(mem(&s, ME).nodes.iter().all(|n| n.index as usize != i));
    let x = s.placed_nodes()[i].pos.x;
    put(&mut s, ME, x - 150.0);
    run(&mut s, 60);
    assert!(mem(&s, ME).nodes.iter().any(|n| n.index as usize == i));
}

#[test]
fn spent_when_seen_is_recorded_and_updated_on_a_later_look() {
    let mut s = start();
    let i = node(&s, NodeKind::MusselClump);
    let x = s.placed_nodes()[i].pos.x;
    s.shared.nodes[i].charges = 0;
    put(&mut s, ME, x - 100.0);
    run(&mut s, 60);
    let see = |s: &Sim| {
        mem(s, ME)
            .nodes
            .iter()
            .find(|n| n.index as usize == i)
            .copied()
            .expect("seen")
    };
    assert!(!see(&s).ready, "spent when seen");
    // It regrows while the crab is away: the memory still says spent.
    put(&mut s, ME, 4000.0);
    run(&mut s, 60);
    s.shared.nodes[i].charges = 3;
    run(&mut s, 60);
    assert!(!see(&s).ready, "not seen, so not updated");
    put(&mut s, ME, x - 100.0);
    run(&mut s, 60);
    assert!(see(&s).ready, "updated on a later look");
}

#[test]
fn remote_crabs_never_record() {
    let mut s = start();
    let x = s.placed_nodes()[node(&s, NodeKind::MusselClump)].pos.x;
    s.add_remote_crab(FRIEND, Vec2::new(x, s.level.floor_height(x)));
    run(&mut s, 120);
    assert!(mem(&s, FRIEND).nodes.is_empty());
    assert!(!mem(&s, ME).nodes.is_empty());
}

#[test]
fn a_save_round_trip_keeps_the_nodes() {
    let mut s = start();
    run(&mut s, 120);
    let m = mem(&s, ME);
    assert!(!m.nodes.is_empty());
    let text = serde_json::to_string(&m.to_save()).unwrap();
    let back = Memory::from_save(
        &serde_json::from_str(&text).unwrap(),
        &s.level,
        &s.tuning.memory,
    )
    .expect("loads");
    assert_eq!(back.nodes, m.nodes);
    assert_eq!(back.fingerprint(), m.fingerprint());
}

#[test]
fn same_inputs_same_memory() {
    let go = || {
        let mut s = start();
        run(&mut s, 60);
        put(&mut s, ME, 2900.0);
        run(&mut s, 120);
        (mem(&s, ME).fingerprint(), mem(&s, ME).nodes.clone())
    };
    assert_eq!(go(), go());
}
