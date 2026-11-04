use bevy_math::Vec2;
use crabsim::{Controls, Kind, PlayerId, ShellAction, Sim};
const P: PlayerId = PlayerId(0);
fn tick(s: &mut Sim, c: Controls) {
    s.set_controls(P, c);
    s.tick();
}
fn toggle(s: &mut Sim) {
    assert!(s.shell_action(P, ShellAction::ToggleLantern));
}
fn mantis(dim: bool, x: f32) -> Sim {
    let mut s = Sim::with_one_crab(Vec2::new(x, -3000.0));
    s.enable_underwater_cells();
    s.spawn_enemy(Kind::Mantis, Vec2::new(16900.0, -3000.0));
    if dim {
        toggle(&mut s);
    }
    s
}
#[test]
fn shading_delays_initial_sight_but_not_close_contact_or_an_alerted_hunter() {
    let mut full = mantis(false, 16640.0);
    let mut dim = mantis(true, 16640.0);
    for _ in 0..30 {
        tick(&mut full, Controls::default());
        tick(&mut dim, Controls::default());
    }
    assert_eq!(full.enemies()[0].target, Some(P));
    assert_eq!(dim.enemies()[0].target, None);
    toggle(&mut dim);
    tick(&mut dim, Controls::default());
    assert_eq!(dim.enemies()[0].target, Some(P));
    toggle(&mut dim);
    for _ in 0..30 {
        tick(&mut dim, Controls::default());
    }
    assert_eq!(
        dim.enemies()[0].target,
        Some(P),
        "shading is not an aggro reset"
    );
    let mut close = mantis(true, 16780.0);
    tick(&mut close, Controls::default());
    assert_eq!(close.enemies()[0].target, Some(P));
}
#[test]
fn a_dim_crab_can_approach_but_walking_into_close_range_still_alerts() {
    for size in [0, 100] {
        let mut s = mantis(true, 16560.0);
        s.crab_mut(P).unwrap().mover.moults = size;
        for _ in 0..180 {
            if s.enemies()[0].target.is_some() {
                break;
            }
            tick(
                &mut s,
                Controls {
                    walk: 1.0,
                    ..Default::default()
                },
            );
        }
        assert_eq!(s.enemies()[0].target, Some(P));
        let x = s.crab(P).unwrap().mover.pos.x;
        assert!(x > 16670.0 && x < 16800.0, "initial notice at {x}");
        assert!(s.crab(P).unwrap().fighter.vitals.health > 0.0);
    }
}
#[test]
fn shading_does_not_change_scent_vibration_or_sunlit_visibility() {
    for kind in [Kind::Moray, Kind::Snapper] {
        let mut noticed = Vec::new();
        for dim in [false, true] {
            let x = if kind == Kind::Moray {
                12960.0
            } else {
                13070.0
            };
            let mut s = Sim::with_one_crab(Vec2::new(x, -3250.0));
            s.enable_underwater_cells();
            s.spawn_enemy(kind, Vec2::new(13200.0, -3250.0));
            if dim {
                toggle(&mut s);
            }
            let mut first = None;
            for n in 0..500 {
                tick(
                    &mut s,
                    Controls {
                        walk: if kind == Kind::Snapper && n < 35 {
                            1.0
                        } else {
                            0.0
                        },
                        ..Default::default()
                    },
                );
                if s.enemies()[0].target.is_some() {
                    first = Some(n);
                    break;
                }
            }
            assert!(first.is_some());
            noticed.push(first);
        }
        assert_eq!(noticed[0], noticed[1]);
    }
    for dim in [false, true] {
        let mut s = Sim::with_one_crab(Vec2::new(180.0, 0.0));
        s.spawn_enemy(Kind::Mantis, Vec2::new(440.0, 0.0));
        if dim {
            toggle(&mut s);
        }
        tick(&mut s, Controls::default());
        assert_eq!(s.enemies()[0].target, Some(P));
    }
}
#[test]
fn dimming_preserves_upgrade_capacity_known_places_and_loaded_terrain() {
    let mut s = Sim::with_one_crab(Vec2::new(12800.0, -3250.0));
    s.enable_underwater_cells();
    s.crab_mut(P).unwrap().mover.lantern = 3;
    for _ in 0..24 {
        tick(&mut s, Controls::default());
    }
    let full = s.crab(P).unwrap().mover.lantern_sight(&s.tuning);
    let point = full.eye + Vec2::X * full.radii.x * 0.8;
    let cell = s
        .crab(P)
        .unwrap()
        .memory
        .cell_at(point, &s.tuning.memory)
        .unwrap();
    assert!(s.crab(P).unwrap().memory.in_sight(cell));
    let capacity = s.crab(P).unwrap().mover.lantern_glow();
    let bounds = s.level.cells.as_ref().unwrap().game_bounds();
    toggle(&mut s);
    for _ in 0..24 {
        tick(&mut s, Controls::default());
    }
    let c = s.crab(P).unwrap();
    assert_eq!(c.mover.lantern_glow(), capacity);
    assert_eq!(s.level.cells.as_ref().unwrap().game_bounds(), bounds);
    assert!(!c.memory.in_sight(cell));
    assert!(c.memory.visits(cell) > 0);
    toggle(&mut s);
    for _ in 0..24 {
        tick(&mut s, Controls::default());
    }
    assert!(s.crab(P).unwrap().memory.in_sight(cell));
}
#[test]
fn local_setting_persists_defaults_to_full_and_never_toggles_a_remote_crab() {
    let mut s = Sim::with_one_crab(Vec2::ZERO);
    s.add_crab(PlayerId(1), Vec2::ZERO);
    s.crab_mut(PlayerId(1)).unwrap().remote = true;
    assert!(!s.shell_action(PlayerId(1), ShellAction::ToggleLantern));
    toggle(&mut s);
    assert!(!s.crab(PlayerId(1)).unwrap().mover.lantern_dim);
    let p = s.progress(P).unwrap();
    let mut fresh = Sim::with_one_crab(Vec2::ZERO);
    fresh.restore_progress(P, &p);
    assert!(fresh.crab(P).unwrap().mover.lantern_dim);
    let mut value = serde_json::to_value(p).unwrap();
    value.as_object_mut().unwrap().remove("lantern_dim");
    let old: crabsim::Progress = serde_json::from_value(value).unwrap();
    assert!(!old.lantern_dim);
    let mover = s.crab(P).unwrap().mover.clone();
    let bytes = serde_json::to_vec(&mover).unwrap();
    let remote: crabsim::Mover = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(remote.lantern_dim, mover.lantern_dim);
}
#[test]
fn toggles_during_a_cellular_lower_route_replay_exactly() {
    for size in [0, 100] {
        let mut s = Sim::with_one_crab(Vec2::new(12750.0, -3250.0));
        s.enable_underwater_cells();
        // Match replay startup: enable cells before restoring grown progress.
        s.crab_mut(P).unwrap().mover.moults = size;
        let initial = s.progress(P).unwrap();
        s.restore_progress(P, &initial);
        s.start_recording();
        for n in 0..360 {
            if n == 0 || n == 180 {
                toggle(&mut s);
            }
            tick(
                &mut s,
                Controls {
                    walk: 1.0,
                    ..Default::default()
                },
            );
        }
        assert!(s.crab(P).unwrap().mover.pos.x > 13450.0);
        let replay = crabsim::Replay::parse(&s.recording().unwrap().to_text()).unwrap();
        let mut again = Sim::with_one_crab(replay.start);
        crabsim::play(&mut again, &replay, replay.end.unwrap(), |_| {});
        assert_eq!(
            s.progress(P),
            again.progress(P),
            "size {size}; pos {:?} vs {:?}",
            s.crab(P).unwrap().mover.pos,
            again.crab(P).unwrap().mover.pos
        );
        assert_eq!(replay.hash, Some(again.state_hash()), "size {size}");
    }
}
