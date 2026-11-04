use bevy_math::Vec2;
use crabsim::{Carry, Controls, Feat, PlayerId, Sim};
const P: PlayerId = PlayerId(0);
fn setup(size: u32, feat: bool, x: f32, y: f32) -> Sim {
    let mut s = Sim::with_one_crab(Vec2::new(x, y));
    let m = &mut s.crab_mut(P).unwrap().mover;
    m.moults = size;
    m.upgrades = 4;
    m.stamina = 4.0;
    if feat {
        m.feats.push(Feat::HeavyCrusher);
    }
    s.enable_underwater_cells();
    s
}
fn tick(s: &mut Sim, c: Controls) {
    s.set_controls(P, c);
    s.tick();
    assert!(s.crab(P).unwrap().fighter.vitals.health > 0.0);
}
fn travel(
    s: &mut Sim,
    limit: u32,
    mut c: impl FnMut(u32, Vec2) -> Controls,
    done: impl Fn(Vec2) -> bool,
) {
    for n in 0..limit {
        let p = s.crab(P).unwrap().mover.pos;
        if done(p) {
            return;
        }
        tick(s, c(n, p));
    }
    panic!("blocked {:?}", s.crab(P).unwrap().mover.pos);
}
fn smash(s: &mut Sim, full: bool) {
    for n in 0..if full { 120 } else { 1 } {
        tick(
            s,
            Controls {
                smash_pressed: n == 0,
                smash_held: true,
                ..Default::default()
            },
        );
    }
    for _ in 0..90 {
        tick(s, Controls::default());
    }
}
#[test]
fn coral_approach_full_smash_collection_and_exit_work_at_both_sizes() {
    for size in [0, 100] {
        let mut s = setup(size, true, 16460.0, -3000.0);
        s.populate();
        travel(
            &mut s,
            1200,
            |n, _| Controls {
                jump_held: true,
                jump_pressed: n % 60 == 0,
                ..Default::default()
            },
            |p| p.y > -2545.0,
        );
        travel(
            &mut s,
            1200,
            |n, p| Controls {
                walk: 1.0,
                jump_held: true,
                jump_pressed: n == 0 || (p.y < -2140.0 && n % 60 == 0),
                ..Default::default()
            },
            |p| p.x > 16700.0 && p.y > -2200.0,
        );
        for _ in 0..250 {
            tick(&mut s, Controls::default());
        }
        travel(
            &mut s,
            600,
            |_, _| Controls {
                walk: 1.0,
                ..Default::default()
            },
            |p| p.x > 16840.0,
        );
        for _ in 0..90 {
            tick(
                &mut s,
                Controls {
                    walk: 1.0,
                    ..Default::default()
                },
            );
        }
        assert!(!s.level.brittle.last().unwrap().cracked);
        assert!(!s.level.culture_unsealed(*s.level.cultures.last().unwrap()));
        smash(&mut s, true);
        assert!(s.level.brittle.last().unwrap().cracked);
        travel(
            &mut s,
            600,
            |_, _| Controls {
                walk: 1.0,
                ..Default::default()
            },
            |p| p.x > 17020.0,
        );
        assert_eq!(s.crab(P).unwrap().mover.carrying, Some(Carry::Culture));
        assert!(*s.crab(P).unwrap().cultures_taken.last().unwrap());
        travel(
            &mut s,
            1000,
            |_, _| Controls {
                walk: -1.0,
                ..Default::default()
            },
            |p| p.x < 16420.0,
        );
        travel(&mut s, 1000, |_, _| Controls::default(), |p| p.y < -2950.0);
    }
}
#[test]
fn intact_cache_rejects_taps_and_crabs_without_the_feat() {
    for (feat, full) in [(false, false), (false, true), (true, false)] {
        let mut s = setup(0, feat, 16855.0, -2200.0);
        for _ in 0..90 {
            tick(
                &mut s,
                Controls {
                    walk: 1.0,
                    ..Default::default()
                },
            );
        }
        smash(&mut s, full);
        assert!(!s.level.brittle.last().unwrap().cracked);
        assert!(!*s.crab(P).unwrap().cultures_taken.last().unwrap());
        assert!(s.crab(P).unwrap().mover.pos.x < 16920.0);
    }
}
#[test]
fn noncrusher_can_jump_over_and_return_without_taking_the_sealed_find() {
    for size in [0, 100] {
        let mut s = setup(size, false, 16800.0, -2200.0);
        travel(
            &mut s,
            600,
            |n, _| Controls {
                walk: 1.0,
                jump_held: true,
                jump_pressed: n % 60 == 0,
                ..Default::default()
            },
            |p| p.x > 17170.0,
        );
        travel(
            &mut s,
            600,
            |n, _| Controls {
                walk: -1.0,
                jump_held: true,
                jump_pressed: n % 60 == 0,
                ..Default::default()
            },
            |p| p.x < 16800.0,
        );
        assert!(!s.level.brittle.last().unwrap().cracked);
        assert!(!*s.crab(P).unwrap().cultures_taken.last().unwrap());
    }
}

#[test]
fn shared_break_opens_collision_but_does_not_award_anyones_culture() {
    use crabsim::net::Msg;
    let mut host = setup(0, true, 16855.0, -2200.0);
    let rock_index = host.level.brittle.len() - 1;
    let reward_index = host.level.cultures.len() - 1;
    let rect = host.level.brittle[rock_index].rect;
    let reward = host.level.cultures[reward_index];
    host.add_crab(PlayerId(1), reward);
    host.crab_mut(PlayerId(1)).unwrap().remote = true;
    host.go_online(true);
    for _ in 0..90 {
        tick(
            &mut host,
            Controls {
                walk: 1.0,
                ..Default::default()
            },
        );
    }
    assert!(host.level.solids().any(|r| r == rect));
    let cells = host.level.cells.as_ref().unwrap();
    assert!(
        !cells.game_solid(
            rect.min.x + 20.0,
            rect.min.y + 20.0,
            rect.max.x - 20.0,
            rect.max.y - 20.0
        ),
        "dynamic rubble must not leave immutable rock cells"
    );
    smash(&mut host, true);
    assert!(!host.level.solids().any(|r| r == rect));
    assert!(!host.crab(P).unwrap().cultures_taken[reward_index]);
    assert!(!host.crab(PlayerId(1)).unwrap().cultures_taken[reward_index]);
    let message = host
        .take_outbox()
        .into_iter()
        .find_map(|out| matches!(out.msg,Msg::RockCracked(i) if i==rock_index).then_some(out.msg))
        .expect("actual attack broadcasts the shared break");
    let mut guest = setup(0, false, 16800.0, -2200.0);
    guest.go_online(false);
    guest.receive(message);
    assert!(guest.level.brittle[rock_index].cracked);
    assert!(guest.level.culture_unsealed(reward));
    assert!(!guest.crab(P).unwrap().cultures_taken[reward_index]);
    let saved = host.progress(P).unwrap();
    let mut restored = setup(0, true, 16800.0, -2200.0);
    restored.restore_progress(P, &saved);
    assert!(restored.level.brittle[rock_index].cracked);
    assert!(!restored.level.solids().any(|r| r == rect));
    assert!(!restored.crab(P).unwrap().cultures_taken[reward_index]);
    let mut joining = setup(0, false, 16800.0, -2200.0);
    joining.go_online(false);
    joining.receive(host.world());
    assert!(joining.level.brittle[rock_index].cracked);
    assert!(!joining.crab(P).unwrap().cultures_taken[reward_index]);
}

#[test]
fn sealed_cultures_are_not_collected_even_by_overlapping_reach() {
    let mut s = setup(0, false, 17010.0, -2170.0);
    tick(&mut s, Controls::default());
    assert!(!*s.crab(P).unwrap().cultures_taken.last().unwrap());
    assert!(!s.level.culture_unsealed(*s.level.cultures.last().unwrap()));
}

#[test]
fn full_smash_and_local_pickup_replay_exactly() {
    let mut live = setup(0, true, 16855.0, -2200.0);
    live.start_recording();
    for _ in 0..90 {
        tick(
            &mut live,
            Controls {
                walk: 1.0,
                ..Default::default()
            },
        );
    }
    smash(&mut live, true);
    for _ in 0..180 {
        tick(
            &mut live,
            Controls {
                walk: 1.0,
                ..Default::default()
            },
        );
    }
    assert!(live.level.brittle.last().unwrap().cracked);
    assert!(*live.crab(P).unwrap().cultures_taken.last().unwrap());
    let replay = crabsim::Replay::parse(&live.recording().unwrap().to_text()).unwrap();
    let mut again = Sim::with_one_crab(replay.start);
    crabsim::play(&mut again, &replay, replay.end.unwrap(), |_| {});
    assert_eq!(replay.hash, Some(again.state_hash()));
    assert_eq!(live.progress(P), again.progress(P));
}
