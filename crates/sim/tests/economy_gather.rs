//! Gathering: the hold at a node, what it asks for, what it gives, the shared
//! charges and their regrowth (alone and in co-op), the vent pot, Survival
//! and the rare finds.

use bevy_math::Vec2;
use crabsim::economy::gather::{self, GatherBlocked};
use crabsim::economy::place::{band_index, nodes};
use crabsim::economy::{Food, Item, NODES, NodeKind, Tool, ToolKind, roll, xp};
use crabsim::net::FakeLink;
use crabsim::prepared_food::PortionKind;
use crabsim::world::cues::Cue;
use crabsim::{Carry, Controls, Feel, Level, PlayerId, Replay, Sim, Skill, Tuning};

const P0: PlayerId = PlayerId(0);
const HOST: PlayerId = PlayerId(0);
const GUEST: PlayerId = PlayerId(1);

fn holding() -> Controls {
    Controls {
        gather_held: true,
        ..Controls::default()
    }
}

/// The test level with its currents still: the vent field's plumes would
/// carry a crab off the pot.
fn long_level() -> Level {
    let mut level = Level::test_level();
    level.currents.clear();
    level
}

fn node_of(sim: &Sim, kind: NodeKind) -> usize {
    sim.placed_nodes()
        .iter()
        .position(|n| n.kind == kind)
        .unwrap_or_else(|| panic!("no {kind:?} in this level"))
}

/// A world with one crab standing at the first node of `kind`.
fn at_node(level: Level, kind: NodeKind) -> (Sim, usize) {
    let probe = Sim::new(Tuning::default(), level.clone());
    let i = node_of(&probe, kind);
    let at = probe.placed_nodes()[i].pos;
    let mut sim = Sim::new(Tuning::default(), level);
    if sim.level.pocket_at(at).is_some() {
        // A node in a sealed pocket (the burrow's clam beds) is worked from
        // inside it, on the cell floor, as the game does.
        sim.add_crab(P0, at);
        sim.enable_underwater_cells();
        sim.crab_mut(P0).expect("crab").mover.pos = at;
    } else {
        sim.add_crab(P0, Vec2::new(at.x, sim.level.floor_height(at.x)));
    }
    // These tests are about gathering, not the first-hour guide: with it on,
    // a hold takes the guide's kind (kelp) from the next node over.
    assert!(sim.guide_skip(P0));
    for _ in 0..5 {
        sim.tick();
    }
    (sim, i)
}

/// Every other node spent, so the neighbours (60 units away) are not
/// worked instead.
fn isolate(sim: &mut Sim, keep: usize) {
    for (j, n) in sim.shared.nodes.iter_mut().enumerate() {
        if j != keep {
            n.charges = 0;
        }
    }
}

fn mussel() -> (Sim, usize) {
    at_node(Level::test_level(), NodeKind::MusselClump)
}

fn shell(sim: &Sim, id: PlayerId) -> Vec<Carry> {
    sim.crab(id).unwrap().mover.clinging.clone()
}

fn count(sim: &Sim, id: PlayerId, item: Item) -> usize {
    shell(sim, id)
        .iter()
        .filter(|c| **c == Carry::Item(item))
        .count()
}

fn xp_of(sim: &Sim, skill: Skill) -> u32 {
    sim.crab(P0).unwrap().econ.skills.get(skill)
}

fn skill_at(sim: &mut Sim, id: PlayerId, skill: Skill, level: u32) {
    sim.crab_mut(id)
        .unwrap()
        .econ
        .skills
        .set(skill, xp::xp_for_level(level));
}

/// Hold gather for `n` ticks; how many ticks said they were blocked, and
/// the first reason.
fn hold(sim: &mut Sim, id: PlayerId, n: u64) -> (u32, Option<GatherBlocked>) {
    sim.set_controls(id, holding());
    let (mut said, mut first) = (0, None);
    for _ in 0..n {
        sim.tick();
        if let Some(why) = sim.crab(id).unwrap().events.gather_blocked {
            said += 1;
            first = first.or(Some(why));
        }
    }
    (said, first)
}

#[test]
fn holding_gather_gives_the_item_the_xp_and_wears_the_tool() {
    let (mut sim, i) = mussel();
    sim.crab_mut(P0).unwrap().econ.tools[ToolKind::Pry.index()] = Tool::new(ToolKind::Pry, 1);
    let def = NodeKind::MusselClump.def();
    // A shell tip on a tier 1 node is a tier faster.
    let need = def.gather_ticks(1, 1, 0);
    assert!(need < 300);
    sim.set_controls(P0, holding());
    let mut done_at = None;
    let mut gathered = None;
    let mut cues = 0;
    for t in 1..=need + 40 {
        sim.tick();
        let crab = sim.crab(P0).unwrap();
        if crab.events.gathered.is_some() {
            gathered = crab.events.gathered;
            done_at.get_or_insert(t);
        }
        if t == need / 2 {
            let s = sim.gather_state(P0).expect("working");
            assert_eq!(s.node, i);
            assert!(s.progress > 0.3 && s.progress < 0.7, "{}", s.progress);
        }
        cues += sim
            .cues()
            .iter()
            .filter(|e| matches!(e.cue, Cue::Gathered { .. }))
            .count();
        sim.set_controls(P0, holding());
    }
    let done_at = done_at.expect("it finished");
    assert!((need..=need + 2).contains(&done_at), "{done_at} for {need}");
    // Tier 1 pays 130% below level 15 (the first hours).
    assert_eq!(gathered, Some((Item::Mussel, 13)));
    assert_eq!(count(&sim, P0, Item::Mussel), 1);
    assert_eq!(xp_of(&sim, Skill::Prying), 13);
    let crab = sim.crab(P0).unwrap();
    assert_eq!(crab.econ.tool(ToolKind::Pry).uses_left, 59);
    assert_eq!(crab.econ.gathers_done, 1);
    assert_eq!(sim.shared.nodes[i].charges, def.charges - 1);
    assert!(cues >= 1);
    // The next gather started by itself while holding.
    assert!(sim.gather_state(P0).is_some());
}

#[test]
fn walking_jumping_and_letting_go_empty_the_bar() {
    let (mut sim, _) = mussel();
    hold(&mut sim, P0, 100);
    let before = sim.gather_state(P0).expect("working").progress;
    assert!(before > 0.2);
    // A step aside.
    sim.set_controls(
        P0,
        Controls {
            gather_held: true,
            walk: 0.8,
            ..Controls::default()
        },
    );
    sim.tick();
    assert_eq!(sim.gather_state(P0), None);
    hold(&mut sim, P0, 5);
    assert!(sim.gather_state(P0).expect("again").progress < 0.1);
    // A jump.
    hold(&mut sim, P0, 50);
    sim.set_controls(
        P0,
        Controls {
            gather_held: true,
            jump_held: true,
            jump_pressed: true,
            ..Controls::default()
        },
    );
    sim.tick();
    assert_eq!(sim.gather_state(P0), None);
    // Letting go.
    sim.set_controls(P0, Controls::default());
    hold(&mut sim, P0, 5);
    sim.set_controls(P0, Controls::default());
    sim.tick();
    assert_eq!(sim.gather_state(P0), None);
    assert_eq!(count(&sim, P0, Item::Mussel), 0);
}

#[test]
fn level_tool_room_and_spent_nodes_say_why_once_per_press() {
    // Level: clams open at Prying 15.
    let (mut sim, _) = at_node(Level::test_level(), NodeKind::ClamBed);
    let (said, why) = hold(&mut sim, P0, 60);
    assert_eq!(
        (said, why),
        (1, Some(GatherBlocked::Level(Skill::Prying, 15)))
    );
    assert_eq!(sim.gather_state(P0), None);
    // Pressed again: said again.
    sim.set_controls(P0, Controls::default());
    sim.tick();
    assert_eq!(hold(&mut sim, P0, 30).0, 1);
    // Tool: a shell pry tip is needed at tier 2.
    skill_at(&mut sim, P0, Skill::Prying, 15);
    sim.set_controls(P0, Controls::default());
    sim.tick();
    let (said, why) = hold(&mut sim, P0, 30);
    assert_eq!(
        (said, why),
        (1, Some(GatherBlocked::Tool(ToolKind::Pry, 1)))
    );
    // A cracked tool counts as none.
    let mut cracked = Tool::new(ToolKind::Pry, 1);
    cracked.uses_left = 0;
    sim.crab_mut(P0).unwrap().econ.tools[0] = cracked;
    sim.set_controls(P0, Controls::default());
    sim.tick();
    assert_eq!(
        hold(&mut sim, P0, 30).1,
        Some(GatherBlocked::Tool(ToolKind::Pry, 1))
    );
    // With the tip fitted it works.
    sim.crab_mut(P0).unwrap().econ.tools[0] = Tool::new(ToolKind::Pry, 1);
    sim.set_controls(P0, Controls::default());
    sim.tick();
    assert_eq!(hold(&mut sim, P0, 30).0, 0);
    assert!(sim.gather_state(P0).is_some());
    // Spent: no ready node in reach, but a spent one.
    isolate(&mut sim, usize::MAX);
    sim.set_controls(P0, Controls::default());
    sim.tick();
    assert_eq!(hold(&mut sim, P0, 30), (1, Some(GatherBlocked::Spent)));

    // Room: a full shell.
    let (mut sim, _) = mussel();
    let slots = {
        let t = sim.tuning.clone();
        sim.crab(P0).unwrap().mover.carries(&t)
    };
    sim.crab_mut(P0).unwrap().mover.clinging = vec![Carry::Grit; slots];
    assert_eq!(hold(&mut sim, P0, 30), (1, Some(GatherBlocked::NoRoom)));
}

#[test]
fn a_big_find_is_blocked_by_a_full_back() {
    let (mut sim, _) = at_node(Level::test_level(), NodeKind::GrenadierCarcass);
    skill_at(&mut sim, P0, Skill::Foraging, 45);
    sim.crab_mut(P0).unwrap().mover.carrying = Some(Carry::Culture);
    assert_eq!(hold(&mut sim, P0, 30), (1, Some(GatherBlocked::NoRoom)));
    sim.crab_mut(P0).unwrap().mover.carrying = None;
    sim.set_controls(P0, Controls::default());
    sim.tick();
    assert_eq!(hold(&mut sim, P0, 30).0, 0);
    assert!(sim.gather_state(P0).is_some());
    let def = NodeKind::GrenadierCarcass.def();
    hold(&mut sim, P0, u64::from(def.gather_ticks(45, 3, 0)));
    assert_eq!(
        sim.crab(P0).unwrap().mover.carrying,
        Some(Carry::Item(Item::Grenadier))
    );
}

#[test]
fn charges_run_out_and_regrow_after_exactly_the_respawn_time() {
    let (mut sim, i) = mussel();
    isolate(&mut sim, i);
    skill_at(&mut sim, P0, Skill::Prying, 50);
    let def = NodeKind::MusselClump.def();
    sim.set_controls(P0, holding());
    let mut items = 0;
    let mut spent_at = None;
    for t in 0..2000u64 {
        sim.tick();
        // Empty the shell so there is always room.
        let crab = sim.crab_mut(P0).unwrap();
        // Only the mussels: a rare find (a hash of the crab's own counters,
        // so it moves with the node's place) may ride along.
        items += crab
            .mover
            .clinging
            .iter()
            .filter(|c| matches!(c, Carry::Item(Item::Mussel)))
            .count();
        crab.mover.clinging.clear();
        if sim.shared.nodes[i].charges == 0 {
            spent_at = Some(t);
            break;
        }
    }
    assert!(spent_at.is_some(), "spent");
    assert_eq!(items, usize::from(def.charges));
    let node = sim.shared.nodes[i];
    assert_eq!(node.regrow, (def.respawn_secs * 120.0) as u32);
    // Holding on does nothing now, and regrowth takes exactly that long.
    let mut waited = 0;
    while sim.shared.nodes[i].charges == 0 {
        sim.tick();
        waited += 1;
        assert!(waited <= node.regrow, "too slow");
        assert_eq!(sim.crab(P0).unwrap().mover.clinging.len(), 0);
    }
    assert_eq!(waited, node.regrow);
    assert_eq!(sim.shared.nodes[i].charges, def.charges);
    // And it can be worked again.
    hold(&mut sim, P0, 200);
    assert_eq!(sim.shared.nodes[i].charges, def.charges - 1);
}

// ----- co-op -----

const LAG: u32 = 18;

struct Game {
    host: Sim,
    guest: Sim,
    link: FakeLink,
}

fn world() -> Sim {
    let mut sim = Sim::new(Tuning::default(), Level::test_level());
    sim.set_feel(Feel::Current);
    sim
}

fn coop_game(at: Vec2) -> Game {
    let mut host = world();
    host.add_crab(HOST, at);
    host.add_remote_crab(GUEST, at);
    host.go_online(true);
    let mut guest = world();
    guest.add_remote_crab(HOST, at);
    guest.add_crab(GUEST, at);
    guest.go_online(false);
    for (sim, id) in [(&mut host, HOST), (&mut guest, GUEST)] {
        // Level 2: quick enough, and below the first second-item perk (Prying 3),
        // so the count of items stays the count of charges taken.
        skill_at(sim, id, Skill::Prying, 2);
    }
    Game {
        host,
        guest,
        link: FakeLink::new(LAG, 4, 0.2, 7),
    }
}

fn step(g: &mut Game) {
    g.host.tick();
    g.guest.tick();
    g.link.carry(&mut [&mut g.host, &mut g.guest]);
}

#[test]
fn a_guests_gather_goes_through_the_host_and_two_crabs_never_get_more_than_the_charges() {
    let probe = world();
    let i = node_of(&probe, NodeKind::MusselClump);
    let p = probe.placed_nodes()[i].pos;
    let at = Vec2::new(p.x, probe.level.floor_height(p.x));
    let mut g = coop_game(at);
    isolate(&mut g.host, i);
    isolate(&mut g.guest, i);
    for _ in 0..5 {
        step(&mut g);
    }
    g.host.set_controls(HOST, holding());
    g.guest.set_controls(GUEST, holding());
    let def = NodeKind::MusselClump.def();
    let mut asked_before_told = false;
    for _ in 0..1500 {
        step(&mut g);
        let (h, u) = (
            count(&g.host, HOST, Item::Mussel),
            count(&g.guest, GUEST, Item::Mussel),
        );
        assert!(h + u <= usize::from(def.charges), "{h} + {u}");
        // The guest's bar waits full for the host's word.
        if g.guest
            .gather_state(GUEST)
            .is_some_and(|s| s.progress >= 1.0)
        {
            asked_before_told = true;
        }
    }
    let (h, u) = (
        count(&g.host, HOST, Item::Mussel),
        count(&g.guest, GUEST, Item::Mussel),
    );
    assert_eq!(h + u, usize::from(def.charges), "all of them were taken");
    assert!(h > 0 && u > 0, "both got some: {h} and {u}");
    assert!(asked_before_told, "the guest waited for the host");
    assert_eq!(g.host.shared.nodes[i].charges, 0);
    assert_eq!(g.guest.shared.nodes[i].charges, 0);
    // XP is each crab's own, on its own device.
    let xp_h = g.host.crab(HOST).unwrap().econ.skills.get(Skill::Prying);
    let xp_u = g.guest.crab(GUEST).unwrap().econ.skills.get(Skill::Prying);
    assert!(xp_h >= xp::xp_for_level(2) + 11 * h as u32);
    assert!(xp_u >= xp::xp_for_level(2) + 11 * u as u32);
    // The host's clock grows it back for both.
    g.host.set_controls(HOST, Controls::default());
    g.guest.set_controls(GUEST, Controls::default());
    for _ in 0..(def.respawn_secs * 120.0) as u32 + 4 * LAG {
        step(&mut g);
    }
    assert_eq!(g.host.shared.nodes[i].charges, def.charges);
    assert_eq!(g.guest.shared.nodes[i].charges, def.charges);
}

#[test]
fn a_guest_joining_late_learns_which_nodes_are_spent() {
    let mut host = world();
    host.add_crab(HOST, Vec2::new(1150.0, 110.0));
    host.shared.nodes[3].charges = 0;
    host.shared.nodes[3].regrow = 777;
    let msg = host.world();
    let mut guest = world();
    guest.add_crab(GUEST, Vec2::new(1150.0, 110.0));
    guest.go_online(false);
    guest.receive(msg.clone());
    assert_eq!(guest.shared.nodes[3].charges, 0);
    assert_eq!(guest.shared.nodes[2].charges, host.shared.nodes[2].charges);
    // And the message reads back whole from its bytes.
    assert_eq!(
        crabsim::net::Msg::from_bytes(&msg.to_bytes()).map(|m| m.to_bytes()),
        Some(msg.to_bytes())
    );
}

// ----- recordings -----

#[test]
fn a_recording_replays_a_gather_exactly() {
    let setup = |sim: &mut Sim| {
        assert!(sim.guide_skip(P0));
        skill_at(sim, P0, Skill::Prying, 20);
        sim.crab_mut(P0).unwrap().econ.tools[0] = Tool::new(ToolKind::Pry, 1);
    };
    let probe = Sim::new(Tuning::default(), Level::test_level());
    let p = probe.placed_nodes()[node_of(&probe, NodeKind::MusselClump)].pos;
    let start = Vec2::new(p.x, probe.level.floor_height(p.x));
    let mut live = Sim::with_one_crab(start);
    setup(&mut live);
    live.start_recording();
    let script = Replay::parse("t=0.2 gather; t=3 walk=right; t=3.1 walk=0; t=5 stop-gathering")
        .expect("script");
    let mut play = script.playback();
    for _ in 0..900 {
        live.set_controls(P0, play.next_controls());
        live.tick();
    }
    assert!(count(&live, P0, Item::Mussel) >= 1, "it gathered");
    let recorded = live.recording().expect("recording");
    let text = recorded.to_text();
    assert!(
        text.contains("gather") && text.contains("stop-gathering"),
        "{text}"
    );
    assert_eq!(recorded.hash, Some(live.state_hash()));
    let replay = Replay::parse(&text).expect("reads back");
    let mut again = Sim::with_one_crab(replay.start);
    setup(&mut again);
    let mut play = replay.playback();
    for _ in 0..replay.length() {
        again.set_controls(P0, play.next_controls());
        again.tick();
    }
    assert_eq!(again.state_hash(), live.state_hash());
    assert_eq!(
        count(&again, P0, Item::Mussel),
        count(&live, P0, Item::Mussel)
    );
}

// ----- the vent pot -----

fn vent_crab(cooking: u32, load: &[Item]) -> (Sim, usize) {
    let (mut sim, i) = at_node(long_level(), NodeKind::VentPot);
    skill_at(&mut sim, P0, Skill::Cooking, cooking);
    sim.crab_mut(P0).unwrap().mover.clinging = load.iter().map(|&it| Carry::Item(it)).collect();
    (sim, i)
}

#[test]
fn the_vent_pot_cooks_only_with_the_ingredients_and_the_level() {
    let def = NodeKind::VentPot.def();
    // Nothing on the shell.
    let (mut sim, _) = vent_crab(70, &[]);
    assert_eq!(
        hold(&mut sim, P0, 20),
        (1, Some(GatherBlocked::Ingredients))
    );
    // Seep mussels but Cooking 49.
    let (mut sim, _) = vent_crab(49, &[Item::SeepMussel, Item::SeepMussel]);
    assert_eq!(
        hold(&mut sim, P0, 20),
        (1, Some(GatherBlocked::Level(Skill::Cooking, 50)))
    );
    // One is not enough.
    let (mut sim, _) = vent_crab(60, &[Item::SeepMussel]);
    assert_eq!(hold(&mut sim, P0, 20).1, Some(GatherBlocked::Ingredients));
    // Cooking 50: two portions, the mussels gone, 110 XP, the pot unspent.
    let (mut sim, i) = vent_crab(50, &[Item::Grit, Item::SeepMussel, Item::SeepMussel]);
    let before = xp_of(&sim, Skill::Cooking);
    let need = def.gather_ticks(50, 4, 0);
    hold(&mut sim, P0, u64::from(need) + 5);
    let crab = sim.crab(P0).unwrap();
    assert_eq!(crab.mover.clinging, vec![Carry::Item(Item::Grit)]);
    let kinds: Vec<_> = crab
        .mover
        .prepared_food
        .slots
        .iter()
        .flatten()
        .map(|p| p.kind)
        .collect();
    let poached = PortionKind::Food(Food::VentPoachedSeepMussels);
    assert_eq!(kinds, vec![poached, poached]);
    assert!(crab.mover.prepared_food.valid());
    assert_eq!(xp_of(&sim, Skill::Cooking), before + 110);
    assert_eq!(sim.shared.nodes[i].charges, def.charges);
    // Two of the four slots are free: a second batch fits, a third has no room.
    for batch in 0..2 {
        sim.crab_mut(P0).unwrap().mover.clinging =
            vec![Carry::Item(Item::SeepMussel), Carry::Item(Item::SeepMussel)];
        sim.set_controls(P0, Controls::default());
        sim.tick();
        let (_, blocked) = hold(&mut sim, P0, u64::from(need) + 5);
        let crab = sim.crab(P0).unwrap();
        if batch == 0 {
            assert_eq!(crab.mover.prepared_food.held(), 4, "the second batch");
        } else {
            assert_eq!(blocked, Some(GatherBlocked::NoRoom));
            assert_eq!(crab.mover.clinging.len(), 2, "kept on the shell");
        }
    }
}

#[test]
fn the_vent_clam_feast_needs_cooking_65() {
    let load = [Item::VentClam, Item::VentClam, Item::ScalyFoot];
    let (mut sim, _) = vent_crab(64, &load);
    assert_eq!(
        hold(&mut sim, P0, 20).1,
        Some(GatherBlocked::Level(Skill::Cooking, 65))
    );
    let (mut sim, _) = vent_crab(65, &load);
    let need = NodeKind::VentPot.def().gather_ticks(65, 4, 0);
    let before = xp_of(&sim, Skill::Cooking);
    hold(&mut sim, P0, u64::from(need) + 5);
    assert!(sim.crab(P0).unwrap().mover.clinging.is_empty());
    assert_eq!(xp_of(&sim, Skill::Cooking), before + 180);
    let feast = PortionKind::Food(Food::VentClamFeast);
    let kinds: Vec<_> = sim
        .crab(P0)
        .unwrap()
        .mover
        .prepared_food
        .slots
        .iter()
        .flatten()
        .map(|p| p.kind)
        .collect();
    assert_eq!(kinds, vec![feast, feast]);
}

// ----- Survival -----

fn deep_node() -> (Vec2, usize) {
    let level = Level::test_level();
    let placed = nodes(&level);
    let n = placed
        .iter()
        .find(|n| band_index(&level, n.pos) >= 2 && n.kind.def().tier == 3)
        .expect("a node in the twilight");
    (n.pos, band_index(&level, n.pos))
}

#[test]
fn survival_experience_comes_only_from_deep_water() {
    let (pos, band) = deep_node();
    // Shallow: nothing.
    let mut shallow = Sim::with_one_crab(Vec2::new(300.0, 0.0));
    for _ in 0..1200 {
        shallow.tick();
    }
    assert_eq!(xp_of(&shallow, Skill::Survival), 0);
    // Deep: (band - 1) a second, 160% early.
    let mut deep = Sim::with_one_crab(pos);
    for _ in 0..1200 {
        deep.tick();
    }
    let got = xp_of(&deep, Skill::Survival);
    // (The first hours pay Survival 160% below level 15.)
    let wanted = (band - 1) as u32 * 16;
    assert!(
        got + 2 >= wanted && got <= wanted + 2,
        "{got} for band {band}: {wanted}"
    );
    // Under hard strain: twice as much.
    let mut hard = Sim::with_one_crab(pos);
    hard.crab_mut(P0).unwrap().strain = 0.9;
    for _ in 0..1200 {
        hard.tick();
    }
    let twice = xp_of(&hard, Skill::Survival);
    assert!(
        twice + 3 >= 2 * wanted && twice <= 2 * wanted + 3,
        "{twice} vs {wanted}"
    );
}

#[test]
fn survival_levels_slow_the_strain_by_one_percent_a_level_to_forty() {
    let (pos, _) = deep_node();
    let strain_after = |level: u32| {
        let mut sim = Sim::with_one_crab(pos);
        sim.crab_mut(P0).unwrap().adapted_depth = pos.y + 5000.0;
        skill_at(&mut sim, P0, Skill::Survival, level);
        // Pin it: the little Survival XP earned must not change the level.
        for _ in 0..600 {
            sim.tick();
            skill_at(&mut sim, P0, Skill::Survival, level);
        }
        sim.crab(P0).unwrap().strain
    };
    let (none, ten, forty, ninety) = (
        strain_after(1),
        strain_after(11),
        strain_after(41),
        strain_after(90),
    );
    assert!(none > 0.0);
    assert!((ten / none - 0.9).abs() < 0.01, "{}", ten / none);
    assert!((forty / none - 0.6).abs() < 0.01, "{}", forty / none);
    assert!((ninety / none - 0.6).abs() < 0.01, "capped at 40%");
}

// ----- rare finds -----

#[test]
fn the_rare_find_is_a_deterministic_roll() {
    let find = |kind: NodeKind, identity, node, n| gather::rare_find(kind.def(), identity, node, n);
    let lucky = |identity: u64, node: usize| {
        (0..100_000u64)
            .find(|&n| roll(identity, node as u64, n).is_multiple_of(64))
            .expect("one in 64")
    };
    let n = lucky(5, 7);
    assert_eq!(find(NodeKind::MusselClump, 5, 7, n), Some(Item::Pearl));
    assert_eq!(find(NodeKind::VentClamBed, 5, 7, n), Some(Item::Pearl));
    assert_eq!(find(NodeKind::ToothBed, 5, 7, n), Some(Item::Tooth));
    assert_eq!(find(NodeKind::ClayBank, 5, 7, n), Some(Item::Tooth));
    assert_eq!(find(NodeKind::GritHeap, 5, 7, n), None);
    assert_eq!(find(NodeKind::ScalyFootSnails, 5, 7, n), Some(Item::Scale));
    assert_eq!(find(NodeKind::GrenadierCarcass, 5, 7, n), None);
    assert_eq!(find(NodeKind::KelpStand, 5, 7, n), None);
    // Same inputs, same answer; other counters mostly nothing.
    assert_eq!(
        find(NodeKind::MusselClump, 5, 7, n),
        find(NodeKind::MusselClump, 5, 7, n)
    );
    let hits = (0..6400u64)
        .filter(|&k| find(NodeKind::MusselClump, 5, 7, k).is_some())
        .count();
    assert!((60..=140).contains(&hits), "about 100 in 6400: {hits}");
    // Every kind that can give one is a node with a rare skill.
    assert!(NODES.iter().any(|d| d.skill == Skill::Prying));
}

#[test]
fn a_rare_find_in_play_is_given_only_with_room() {
    let run = |spare: usize| {
        let (mut sim, _) = mussel();
        let identity = sim.crab(P0).unwrap().salvage_identity.wrapping_add(0);
        let i = node_of(&sim, NodeKind::MusselClump);
        let n = (0..100_000u64)
            .find(|&n| roll(identity, i as u64, n).is_multiple_of(64))
            .expect("one in 64");
        let slots = {
            let t = sim.tuning.clone();
            sim.crab(P0).unwrap().mover.carries(&t)
        };
        let crab = sim.crab_mut(P0).unwrap();
        crab.econ.gathers_done = n;
        crab.mover.clinging = vec![Carry::Grit; slots - spare];
        let def = NodeKind::MusselClump.def();
        hold(&mut sim, P0, u64::from(def.gather_ticks(1, 0, 0)) + 3);
        (count(&sim, P0, Item::Mussel), count(&sim, P0, Item::Pearl))
    };
    assert_eq!(run(2), (1, 1), "room for both");
    assert_eq!(run(1), (1, 0), "room only for the mussel");
}

#[test]
fn a_tools_last_use_cracks_it_and_it_then_works_as_a_bare_claw() {
    let (mut sim, _) = mussel();
    let mut tip = Tool::new(ToolKind::Pry, 1);
    tip.uses_left = 1;
    sim.crab_mut(P0).unwrap().econ.tools[0] = tip;
    sim.set_controls(P0, holding());
    let (mut cracked, mut cue) = (None, false);
    for _ in 0..300 {
        sim.tick();
        cracked = cracked.or(sim.crab(P0).unwrap().events.tool_cracked);
        cue |= sim
            .cues()
            .iter()
            .any(|e| matches!(e.cue, Cue::ToolCracked { .. }));
    }
    assert_eq!(cracked, Some(ToolKind::Pry));
    assert!(cue);
    assert!(sim.crab(P0).unwrap().econ.tool(ToolKind::Pry).cracked());
    // Mussels need no tool: still workable, and the cracked tip wears no more.
    assert_eq!(sim.crab(P0).unwrap().econ.tool(ToolKind::Pry).uses_left, 0);
    assert!(count(&sim, P0, Item::Mussel) >= 1);
}
