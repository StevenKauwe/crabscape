use bevy_math::Vec2;
use crabsim::{Kind, PlayerId, Sim, enemy::State, world::cues::Cue};

fn encounter(kind: Kind) -> Sim {
    let mut sim = Sim::with_one_crab(Vec2::new(500.0, 0.0));
    sim.spawn_enemy(kind, Vec2::new(560.0, 0.0));
    let e = &mut sim.enemies_mut()[0];
    e.target = Some(PlayerId(0));
    e.state = State::Approach;
    e.facing = -1.0;
    sim
}

#[test]
fn hush_starts_at_tell_start() {
    for kind in [Kind::Fixture, Kind::Mantis, Kind::Snapper, Kind::Moray] {
        let mut sim = encounter(kind);
        let mut found = false;
        for _ in 0..600 {
            sim.tick();
            if sim.enemies()[0].state == State::Tell {
                assert!(
                    sim.cues().iter().any(|e| e.tick == sim.tick_count()
                        && matches!(
                            e.cue,
                            Cue::Hush {
                                crab: PlayerId(0),
                                ..
                            }
                        )),
                    "{kind:?}"
                );
                assert!(sim.hushed(PlayerId(0)));
                found = true;
                break;
            }
        }
        assert!(found, "{kind:?} never told");
    }
}

#[test]
fn every_telegraphed_strike_follows_a_hush() {
    for kind in [Kind::Fixture, Kind::Mantis, Kind::Snapper, Kind::Moray] {
        let mut sim = encounter(kind);
        if kind == Kind::Fixture {
            sim.tuning.fixture.tell_ticks = 120;
        }
        let mut onset = None;
        let mut strikes = 0;
        let mut previous = State::Approach;
        for _ in 0..600 {
            sim.tick();
            let e = &sim.enemies()[0];
            if e.state == State::Tell && previous != State::Tell {
                onset = Some((sim.tick_count(), e.state_len));
            }
            if e.state == State::Attack && previous != State::Attack {
                let (start, tell) = onset.expect("strike must have a tell");
                assert!(sim.hushed(PlayerId(0)));
                if tell >= 120 {
                    assert!(sim.tick_count() - start >= 120);
                }
                strikes += 1;
            }
            previous = e.state;
            if onset.is_some() {
                sim.crab_mut(PlayerId(0)).unwrap().mover.pos.x = 1000.0;
            }
        }
        assert!(strikes > 0, "{kind:?}");
    }
}

#[test]
fn groans_are_rarer_and_lower_deeper() {
    use crabsim::world::cues::{groan_interval, groan_pitch};
    let depths = [0.0, -400.0, -1000.0, -4000.0, -10000.0];
    let mut counts = Vec::new();
    for y in depths {
        let mut sim = Sim::with_one_crab(Vec2::new(1000.0, y));
        sim.level.floor = vec![Vec2::new(0.0, y), Vec2::new(20000.0, y)];
        sim.level.rocks.clear();
        let mut groans = 0;
        for _ in 0..240000 {
            sim.tick();
            for event in sim.cues().iter().filter(|e| e.tick == sim.tick_count()) {
                if let Cue::Groan { at, .. } = event.cue {
                    assert_eq!(groan_pitch(at), groan_pitch(Vec2::new(1000.0, y)));
                    groans += 1;
                }
            }
        }
        counts.push(groans);
    }
    eprintln!("groans in 2000 sim seconds, shallow to deep: {counts:?}");
    assert_eq!(
        counts[0], 0,
        "the shallows are busy with living sound, not groans"
    );
    assert!(
        counts[1..].windows(2).all(|pair| pair[0] > pair[1]),
        "{counts:?}"
    );
    for pair in depths.windows(2) {
        assert!(groan_interval(Vec2::new(0.0, pair[0])) < groan_interval(Vec2::new(0.0, pair[1])));
        assert!(groan_pitch(Vec2::new(0.0, pair[0])) > groan_pitch(Vec2::new(0.0, pair[1])));
    }
}

#[test]
fn cues_are_deterministic() {
    let mut a = encounter(Kind::Mantis);
    let mut b = a.clone();
    let mut count = 0;
    for _ in 0..3600 {
        a.tick();
        b.tick();
        assert_eq!(a.cues(), b.cues());
        count += a.cues().iter().filter(|e| e.tick == a.tick_count()).count();
    }
    assert!(count > 10);
    assert_eq!(a.state_hash(), b.state_hash());
}

#[test]
fn every_kind_can_use_the_generic_hush_hook() {
    for kind in crabsim::enemy::KINDS {
        if kind == Kind::AlarmJelly {
            continue;
        } // Contact hazard, no tell state.
        let mut sim = encounter(kind);
        let e = &mut sim.enemies_mut()[0];
        e.state = State::Tell;
        e.state_tick = 0;
        e.state_len = 120;
        sim.tick();
        assert!(sim.cues().iter().any(|event| matches!(event.cue, Cue::Hush { crab: PlayerId(0), until_tick } if until_tick >= 120)), "{kind:?}");
    }
}

#[test]
fn interrupted_tell_releases_silence_and_remote_bodies_are_untouched() {
    let mut sim = encounter(Kind::Mantis);
    sim.add_crab(PlayerId(9), Vec2::new(400.0, -4000.0));
    let remote = sim.crab_mut(PlayerId(9)).unwrap();
    remote.remote = true;
    remote.strain = 0.8;
    let before = remote.clone();
    for _ in 0..180 {
        sim.tick();
    }
    assert_eq!(*sim.crab(PlayerId(9)).unwrap(), before);
    assert!(!sim.cues().iter().any(|e| matches!(
        e.cue,
        Cue::ShellTick {
            crab: PlayerId(9),
            ..
        }
    )));
    sim.enemies_mut()[0].state = State::Stagger;
    sim.enemies_mut()[0].state_len = 60;
    sim.enemies_mut()[0].state_tick = 0;
    sim.tick();
    assert!(!sim.hushed(PlayerId(0)));
}

#[test]
fn intermediate_tick_cues_survive_a_render_frame_then_expire() {
    let mut sim = Sim::with_one_crab(Vec2::new(1000.0, 0.0));
    for _ in 0..61 {
        sim.tick();
    }
    assert!(
        sim.cues()
            .iter()
            .any(|e| e.tick == 60 && matches!(e.cue, Cue::Bed { .. }))
    );
    for _ in 0..240 {
        sim.tick();
    }
    assert!(sim.cues().iter().all(|e| e.tick > 60));
    assert!(sim.cues().len() <= 2048);
}

#[test]
fn hush_stops_the_small_bed_but_keeps_the_crabs_body() {
    let mut sim = encounter(Kind::Fixture);
    sim.tuning.fixture.tell_ticks = 300;
    sim.crab_mut(PlayerId(0)).unwrap().strain = 1.0;
    let mut heard_shell = false;
    for _ in 0..240 {
        sim.tick();
        assert!(sim.hushed(PlayerId(0)));
        for event in sim.cues().iter().filter(|e| e.tick == sim.tick_count()) {
            assert!(!matches!(event.cue, Cue::Bed { crab: PlayerId(0) }));
            heard_shell |= matches!(event.cue, Cue::ShellTick { crab: PlayerId(0), strength } if strength > 0.0);
        }
    }
    assert!(heard_shell);
}

#[test]
fn hush_deadline_tracks_a_strike_paused_by_hitstop() {
    let mut sim = encounter(Kind::Fixture);
    for _ in 0..180 {
        sim.tick();
        if sim.enemies()[0].state == State::Tell {
            sim.crab_mut(PlayerId(0)).unwrap().mover.pos.x = 1000.0;
        }
        if sim.enemies()[0].state == State::Attack {
            let previous_until = sim
                .cues()
                .iter()
                .filter_map(|e| match e.cue {
                    Cue::Hush { until_tick, .. } => Some(until_tick),
                    _ => None,
                })
                .max()
                .unwrap();
            sim.enemies_mut()[0].vitals.hitstop = 10;
            for _ in 0..4 {
                sim.tick();
            }
            assert!(sim.cues().iter().any(
                |e| matches!(e.cue, Cue::Hush { until_tick, .. } if until_tick > previous_until)
            ));
            return;
        }
    }
    panic!("fixture never attacked");
}
