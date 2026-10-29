//! The Home button (`crabsim::home`, `Sim::go_home`): one press puts the
//! crab straight back in its home shell, tucked in as if it had walked in,
//! from anywhere and even mid-fight; and a replay carries it so it plays
//! back exactly.

use bevy_math::Vec2;
use crabsim::combat::Crush;
use crabsim::enemy::State;
use crabsim::level::Place;
use crabsim::replay::ticks;
use crabsim::{Carry, Controls, Hit, Kind, PlayerId, Replay, SPAWN, Shelter, Sim, Source, play};

const P0: PlayerId = PlayerId(0);

/// Tucked right in, in the home shell (not a crevice), at its den.
fn at_home(sim: &Sim) -> bool {
    let m = &sim.crab(P0).unwrap().mover;
    m.tucked_in()
        && m.shelter == Shelter::In
        && m.pos == sim.level.home_den
        && sim.level.place_at(m.pos) == Some(Place::Home)
}

/// Run `ticks` ticks with nobody touching the controls, collecting what
/// happened to the crab.
fn wait(sim: &mut Sim, ticks: u64) -> Vec<&'static str> {
    let mut events = Vec::new();
    for _ in 0..ticks {
        sim.set_controls(P0, Controls::default());
        sim.tick();
        events.extend(sim.crab(P0).unwrap().events.names());
    }
    events
}

#[test]
fn from_far_away_it_goes_straight_home_tucked_in_as_if_it_walked_in() {
    // Dropped far out on the sea floor, walking on.
    let mut sim = Sim::with_one_crab(Vec2::new(1800.0, 300.0));
    play(
        &mut sim,
        &Replay::parse("t=1.5 walk=right").unwrap(),
        ticks(2.0),
        |_| {},
    );
    let far = sim.crab(P0).unwrap().mover.pos;
    assert!(far.x - sim.level.home_den.x > 1000.0, "far away: {far}");
    // Tired from the walk, a clam on its shell for the larder.
    let crab = sim.crab_mut(P0).unwrap();
    crab.mover.stamina = 0.0;
    crab.mover.clinging.push(Carry::Clam);

    assert!(sim.can_go_home(P0));
    assert!(sim.go_home(P0));
    assert!(at_home(&sim), "home at once");
    assert_eq!(
        sim.crab(P0).unwrap().prev_pos,
        sim.level.home_den,
        "no slide across the map"
    );
    let events = wait(&mut sim, 1);
    assert!(
        events.contains(&"went home") && events.contains(&"tucked in"),
        "{events:?}"
    );
    // Resting there as a crab that walked in does: rested, the clam stored.
    let crab = sim.crab(P0).unwrap();
    assert_eq!(crab.mover.stamina, crab.mover.strokes(&sim.tuning) as f32);
    assert_eq!(crab.larder, 1);
    assert!(crab.mover.clinging.is_empty());
    wait(&mut sim, ticks(2.0));
    assert!(at_home(&sim), "it stays tucked in");

    // The same place and pose as a crab that walked in through the door.
    let mut walked = Sim::with_one_crab(SPAWN);
    play(
        &mut walked,
        &Replay::parse("t=0 walk=left").unwrap(),
        ticks(3.0),
        |_| {},
    );
    let (a, b) = (
        &sim.crab(P0).unwrap().mover,
        &walked.crab(P0).unwrap().mover,
    );
    assert_eq!((a.pos, a.tucked, a.shelter), (b.pos, b.tucked, b.shelter));

    // Walking out works as ever.
    play(
        &mut sim,
        &Replay::parse("t=0 walk=right").unwrap(),
        ticks(1.5),
        |_| {},
    );
    assert_eq!(sim.crab(P0).unwrap().mover.shelter, Shelter::Out);
}

#[test]
fn mid_fight_held_in_a_morays_jaws_it_goes_home_and_the_moray_lets_go() {
    // In reach of a moray's lunge from its crevice at 400, until it bites.
    let mut sim = Sim::with_one_crab(Vec2::new(290.0, 0.0));
    sim.spawn_enemy(Kind::Moray, Vec2::new(400.0, 0.0));
    let mut bitten = false;
    for _ in 0..600 {
        if wait(&mut sim, 1).contains(&"hurt") {
            bitten = true;
            break;
        }
    }
    assert!(bitten, "the moray never bit");
    wait(&mut sim, 5);
    let health = sim.crab(P0).unwrap().fighter.vitals.health;
    assert_eq!(sim.crab(P0).unwrap().fighter.status(&sim.tuning), "grabbed");
    assert_eq!(sim.enemies()[0].state, State::Grab);

    assert!(sim.go_home(P0), "any time, even held");
    assert!(at_home(&sim));
    let f = &sim.crab(P0).unwrap().fighter;
    assert!(f.grabbed.is_none() && f.vitals.hitstop == 0 && f.knocked == 0);
    assert_eq!(f.vitals.health, health, "its hurts come home with it");

    let events = wait(&mut sim, ticks(3.0));
    assert!(!events.contains(&"hurt"), "nothing reaches it: {events:?}");
    let e = &sim.enemies()[0];
    assert_ne!(e.state, State::Grab, "the moray let go");
    assert_eq!(e.target, None, "and lost it");
    assert!(at_home(&sim));
}

#[test]
fn mid_swing_and_mid_crush_it_goes_home_and_lets_go() {
    // A smash just started at a fixture in reach...
    let mut sim = Sim::with_one_crab(SPAWN);
    sim.spawn_enemy(Kind::Fixture, Vec2::new(SPAWN.x + 100.0, 0.0));
    play(&mut sim, &Replay::parse("t=0 smash").unwrap(), 4, |_| {});
    assert!(sim.crab(P0).unwrap().fighter.swing.is_some(), "swinging");
    // ...the fixture seized in its crusher, and everything else a fight can
    // leave it in at once: knocked back, staggered, frozen, guarding.
    let id = sim.enemies()[0].id;
    sim.enemies_mut()[0].held = true;
    let f = &mut sim.crab_mut(P0).unwrap().fighter;
    f.crush = Some(Crush {
        target: id,
        tick: 3,
        cracks: 1,
    });
    f.blocking = true;
    f.knocked = 20;
    f.vitals.stagger = 30;
    f.vitals.poise = 0.0;
    f.vitals.hitstop = 8;

    assert!(sim.go_home(P0));
    let f = &sim.crab(P0).unwrap().fighter;
    assert!(f.swing.is_none() && f.queued.is_none() && f.crush.is_none());
    assert!(!f.blocking && f.knocked == 0 && f.vitals.stagger == 0 && f.vitals.hitstop == 0);
    assert_eq!(f.vitals.poise, sim.tuning.crab.poise, "steady again");
    assert!(!sim.enemies()[0].held, "the fixture is let go");
    let events = wait(&mut sim, ticks(1.0));
    assert!(
        !events
            .iter()
            .any(|e| ["smashed", "struck", "cracked", "crushed"].contains(e)),
        "{events:?}"
    );
    assert!(at_home(&sim));
    assert_eq!(
        sim.enemies()[0].vitals.health,
        40.0,
        "the fixture untouched"
    );
}

#[test]
fn only_this_devices_living_crab_away_from_home_goes() {
    // A friend's crab is moved only on its own device.
    let mut sim = Sim::with_one_crab(Vec2::new(900.0, 0.0));
    sim.add_remote_crab(PlayerId(1), Vec2::new(900.0, 0.0));
    assert!(!sim.go_home(PlayerId(1)));
    assert_eq!(
        sim.crab(PlayerId(1)).unwrap().mover.pos,
        Vec2::new(900.0, 0.0)
    );
    // A dead crab is already on its way home: dying takes it there.
    let mut dead = Sim::with_one_crab(Vec2::new(900.0, 0.0));
    dead.hit_crab(P0, &Hit::new(1000.0, 0.0, Source::World));
    let before = dead.state_hash();
    assert!(!dead.can_go_home(P0) && !dead.go_home(P0));
    assert_eq!(dead.state_hash(), before);
    // Already tucked in at home: nothing to do.
    let mut home = Sim::with_one_crab(SPAWN);
    assert!(home.go_home(P0));
    let before = home.state_hash();
    assert!(!home.go_home(P0));
    assert_eq!(home.state_hash(), before);
    // No crab by that id.
    assert!(!home.go_home(PlayerId(7)));
}

#[test]
fn a_script_can_press_home() {
    let script = Replay::parse("t=0 walk=right; t=4 walk=stop home").unwrap();
    assert_eq!(script.homes, [ticks(4.0)]);
    let mut sim = Sim::with_one_crab(SPAWN);
    play(&mut sim, &script, ticks(5.0), |_| {});
    assert!(at_home(&sim));
    assert!(Replay::parse("t=1 hmoe").is_err());
}

#[test]
fn a_recording_with_the_home_button_replays_to_the_same_hash() {
    // Out past the moray, bitten, home, then out again to fight a fixture.
    let script = Replay::parse(
        "t=0 walk=right; t=0.9 walk=stop; t=4 walk=right; t=5 walk=stop; t=5.5 smash; t=6.5 walk=right",
    )
    .unwrap();
    let run = |home: bool| {
        let mut live = Sim::with_one_crab(SPAWN);
        live.start_recording();
        live.spawn_enemy(Kind::Moray, Vec2::new(400.0, 0.0));
        live.spawn_enemy(Kind::Fixture, Vec2::new(420.0, 0.0));
        let mut playback = script.playback();
        let mut went = 0;
        for frame in 0..1100 {
            if home && [330, 900].contains(&frame) && live.go_home(P0) {
                went += 1;
            }
            live.set_controls(P0, playback.next_controls());
            for _ in 0..[1, 0, 2, 1][frame % 4] {
                live.tick();
            }
        }
        (live, went)
    };
    let (live, went) = run(true);
    assert_eq!(went, 2);
    let text = live.recording().unwrap().to_text();
    assert_eq!(text.matches(" home").count(), 2, "{text}");
    let replay = Replay::parse(&text).unwrap();
    assert_eq!(Replay::parse(&replay.to_text()).unwrap(), replay);
    let mut again = Sim::with_one_crab(SPAWN);
    play(&mut again, &replay, replay.length(), |_| {});
    assert_eq!(again.state_hash(), live.state_hash());
    assert_eq!(replay.hash, Some(live.state_hash()));
    assert_ne!(
        run(false).0.state_hash(),
        live.state_hash(),
        "going home made a difference"
    );
}
