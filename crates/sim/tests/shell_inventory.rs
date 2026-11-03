use bevy_math::Vec2;
use crabsim::{Carry, PlayerId, Replay, SPAWN, ShellAction, Sim, play};

const ME: PlayerId = PlayerId(0);

#[test]
fn dropped_items_wait_until_the_crab_leaves_and_returns() {
    let mut sim = Sim::with_one_crab(SPAWN);
    sim.crab_mut(ME).unwrap().mover.clinging.push(Carry::Grit);
    assert!(sim.shell_action(ME, ShellAction::Drop(0)));
    sim.tick();
    assert!(sim.crab(ME).unwrap().mover.clinging.is_empty());
    assert_eq!(sim.crab(ME).unwrap().cast_offs.len(), 1);
    sim.crab_mut(ME).unwrap().mover.pos += Vec2::new(500.0, 0.0);
    sim.tick();
    sim.crab_mut(ME).unwrap().mover.pos = SPAWN;
    sim.tick();
    assert_eq!(sim.crab(ME).unwrap().mover.clinging, vec![Carry::Grit]);
    assert!(sim.crab(ME).unwrap().cast_offs.is_empty());
}

#[test]
fn preferences_and_drops_survive_a_save() {
    let mut sim = Sim::with_one_crab(SPAWN);
    sim.crab_mut(ME).unwrap().mover.clinging.push(Carry::Clam);
    assert!(sim.shell_action(ME, ShellAction::ToggleClams));
    assert!(sim.shell_action(ME, ShellAction::Drop(0)));
    let progress = sim.progress(ME).unwrap();
    let encoded = serde_json::to_string(&progress).unwrap();
    let decoded = serde_json::from_str(&encoded).unwrap();
    let mut loaded = Sim::with_one_crab(SPAWN);
    loaded.restore_progress(ME, &decoded);
    let crab = loaded.crab(ME).unwrap();
    assert!(!crab.pickup.clams && crab.pickup.grit);
    assert_eq!(crab.cast_offs.len(), 1);
    assert!(!crab.cast_offs[0].recoverable);
}

#[test]
fn disabled_auto_pickup_leaves_the_resource_for_later() {
    let mut sim = Sim::with_one_crab(SPAWN);
    let clam = sim.level.clams[0];
    sim.crab_mut(ME).unwrap().mover.pos = clam;
    assert!(sim.shell_action(ME, ShellAction::ToggleClams));
    sim.tick();
    assert_eq!(sim.shared.clams[0], 0);
    assert!(!sim.crab(ME).unwrap().mover.clinging.contains(&Carry::Clam));
    assert!(sim.shell_action(ME, ShellAction::ToggleClams));
    sim.tick();
    assert!(sim.crab(ME).unwrap().mover.clinging.contains(&Carry::Clam));
}

#[test]
fn recorded_inventory_actions_replay() {
    let initial = Sim::with_one_crab(SPAWN);
    let mut live = Sim::with_one_crab(initial.level.clams[0]);
    live.start_recording();
    live.tick();
    assert!(live.shell_action(ME, ShellAction::Drop(0)));
    assert!(live.shell_action(ME, ShellAction::ToggleClams));
    live.tick();
    let replay = Replay::parse(&live.recording().unwrap().to_text()).unwrap();
    let mut again = Sim::with_one_crab(replay.start);
    play(&mut again, &replay, replay.length(), |_| {});
    assert_eq!(replay.hash, Some(again.state_hash()));
}
