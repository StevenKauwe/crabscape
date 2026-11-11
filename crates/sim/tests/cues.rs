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

fn now(sim: &Sim) -> Vec<Cue> {
    sim.cues()
        .iter()
        .filter(|e| e.tick == sim.tick_count())
        .map(|e| e.cue)
        .collect()
}

fn tells(cues: &[Cue]) -> Vec<u64> {
    cues.iter()
        .filter_map(|c| match c {
            Cue::Tell {
                crab: PlayerId(0),
                strike_tick,
            } => Some(*strike_tick),
            _ => None,
        })
        .collect()
}

#[test]
fn a_tell_cue_starts_with_the_tell_and_names_the_strike_tick() {
    for kind in [Kind::Fixture, Kind::Mantis, Kind::Snapper, Kind::Moray] {
        let mut sim = encounter(kind);
        let mut announced = None;
        let mut previous = State::Approach;
        for _ in 0..900 {
            sim.tick();
            let state = sim.enemies()[0].state;
            if state == State::Tell && previous != State::Tell {
                let strike = tells(&now(&sim));
                assert_eq!(
                    strike.len(),
                    1,
                    "{kind:?}: one tell cue at the tell's start"
                );
                assert!(sim.threatened(PlayerId(0)));
                announced = Some(strike[0]);
            }
            if state == State::Attack && previous == State::Tell {
                let strike = announced.expect("a strike follows a tell cue");
                assert!(
                    sim.tick_count().abs_diff(strike) <= 1,
                    "{kind:?}: the swell peaks on the strike ({} vs {strike})",
                    sim.tick_count()
                );
                break;
            }
            previous = state;
        }
        assert!(announced.is_some(), "{kind:?} never told");
    }
}

#[test]
fn every_telegraphed_strike_is_announced_once() {
    for kind in [Kind::Fixture, Kind::Mantis, Kind::Snapper, Kind::Moray] {
        let mut sim = encounter(kind);
        if kind == Kind::Fixture {
            sim.tuning.fixture.tell_ticks = 120;
        }
        let (mut onsets, mut announced, mut strikes) = (0, 0, 0);
        let mut previous = State::Approach;
        for _ in 0..1200 {
            sim.tick();
            announced += tells(&now(&sim)).len();
            let e = &sim.enemies()[0];
            if e.state == State::Tell && previous != State::Tell {
                onsets += 1;
            }
            if e.state == State::Attack && previous != State::Attack {
                strikes += 1;
            }
            previous = e.state;
            sim.crab_mut(PlayerId(0)).unwrap().mover.pos.x = 1000.0;
        }
        assert!(strikes > 0, "{kind:?}");
        assert_eq!(announced, onsets, "{kind:?}: one swell per tell, no storm");
    }
}

#[test]
fn hitstop_never_repeats_a_tell() {
    let mut sim = encounter(Kind::Fixture);
    sim.tuning.fixture.tell_ticks = 120;
    let mut announced = 0;
    for _ in 0..600 {
        sim.tick();
        announced += tells(&now(&sim)).len();
        if sim.enemies()[0].state == State::Tell {
            sim.enemies_mut()[0].vitals.hitstop = 10;
            if announced > 0 && sim.enemies()[0].state_tick > 0 {
                break;
            }
        }
    }
    for _ in 0..60 {
        sim.tick();
        announced += tells(&now(&sim)).len();
    }
    assert_eq!(announced, 1);
}

#[test]
fn groans_are_rarer_and_lower_deeper_and_always_in_key() {
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
            for cue in now(&sim) {
                if let Cue::Groan { at, .. } = cue {
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
        assert!(groan_pitch(Vec2::new(0.0, pair[0])) >= groan_pitch(Vec2::new(0.0, pair[1])));
    }
    assert!(groan_pitch(Vec2::new(0.0, -10000.0)) < groan_pitch(Vec2::new(0.0, -400.0)));
    for y in depths {
        // Whole octaves only: playback never leaves the music's D.
        let octaves = groan_pitch(Vec2::new(0.0, y)).log2();
        assert_eq!(octaves, octaves.round(), "{y}");
    }
}

#[test]
fn cues_are_deterministic() {
    let mut a = encounter(Kind::Mantis);
    let mut b = a.clone();
    let mut count = 0;
    for _ in 0..3600 {
        for sim in [&mut a, &mut b] {
            sim.crab_mut(PlayerId(0)).unwrap().strain = 0.7;
        }
        a.tick();
        b.tick();
        assert_eq!(a.cues(), b.cues());
        count += now(&a).len();
    }
    assert!(count > 10);
    assert_eq!(a.state_hash(), b.state_hash());
}

#[test]
fn every_kind_can_use_the_generic_tell_hook() {
    for kind in crabsim::enemy::KINDS {
        if matches!(kind, Kind::AlarmJelly | Kind::AmphipodSwarm) {
            continue;
        } // Contact hazard / smell-drawn swarm: no tell without a scent.
        let mut sim = encounter(kind);
        let e = &mut sim.enemies_mut()[0];
        e.state = State::Tell;
        e.state_tick = 0;
        e.state_len = 120;
        sim.tick();
        assert!(
            sim.cues().iter().any(|event| matches!(event.cue, Cue::Tell { crab: PlayerId(0), strike_tick } if strike_tick >= 120)),
            "{kind:?}"
        );
    }
}

#[test]
fn interrupted_tell_releases_the_threat_and_remote_bodies_are_untouched() {
    let mut sim = encounter(Kind::Mantis);
    sim.add_crab(PlayerId(9), Vec2::new(400.0, -4000.0));
    let remote = sim.crab_mut(PlayerId(9)).unwrap();
    remote.remote = true;
    remote.strain = 0.8;
    let before = remote.clone();
    for _ in 0..600 {
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
    assert!(!sim.threatened(PlayerId(0)));
}

#[test]
fn intermediate_tick_cues_survive_a_render_frame_then_expire() {
    let mut sim = Sim::with_one_crab(Vec2::new(1000.0, 0.0));
    sim.crab_mut(PlayerId(0)).unwrap().strain = 1.0;
    let mut first = None;
    for _ in 0..1200 {
        sim.tick();
        if now(&sim).iter().any(|c| matches!(c, Cue::ShellTick { .. })) {
            first = Some(sim.tick_count());
            break;
        }
    }
    let first = first.expect("a strained shell creaks");
    sim.tick();
    assert!(sim.cues().iter().any(|e| e.tick == first));
    for _ in 0..240 {
        sim.tick();
    }
    assert!(sim.cues().iter().all(|e| e.tick > first));
    assert!(sim.cues().len() <= 2048);
}

#[test]
fn shell_creaks_are_irregular_never_a_beat_and_never_a_storm() {
    let mut sim = Sim::with_one_crab(Vec2::new(1000.0, -4000.0));
    sim.crab_mut(PlayerId(0)).unwrap().strain = 1.0;
    let mut at = Vec::new();
    let mut per_second = [0u32; 60];
    for _ in 0..7200 {
        sim.tick();
        sim.crab_mut(PlayerId(0)).unwrap().strain = 1.0;
        let cues = now(&sim);
        per_second[(sim.tick_count() / 120 % 60) as usize] += cues.len() as u32;
        if cues.iter().any(|c| matches!(c, Cue::ShellTick { .. })) {
            at.push(sim.tick_count());
        }
    }
    let gaps: Vec<u64> = at.windows(2).map(|w| w[1] - w[0]).collect();
    let mean = gaps.iter().sum::<u64>() as f32 / gaps.len() as f32 / 120.0;
    eprintln!("{} creaks in 60 s, mean gap {mean:.2} s", at.len());
    assert!((1.0..3.0).contains(&mean), "{mean}");
    let distinct: std::collections::BTreeSet<_> = gaps.iter().collect();
    assert!(
        distinct.len() >= 4,
        "creaks must not fall on a fixed beat: {gaps:?}"
    );
    assert!(per_second.iter().all(|&n| n <= 6), "{per_second:?}");
}

#[test]
fn the_steady_soundscape_is_not_cued() {
    // Nothing is retriggered on a clock: a still crab with an unstrained shell
    // in the shallows hears no cues at all for ten seconds (the surge and the
    // deep rumble are steady loops read from the band, not cues).
    let mut sim = Sim::with_one_crab(Vec2::new(1000.0, 0.0));
    sim.level.upwelling.clear();
    for _ in 0..240 {
        sim.tick();
    }
    let mut heard = Vec::new();
    for _ in 0..1200 {
        sim.tick();
        heard.extend(
            now(&sim)
                .into_iter()
                .filter(|c| !matches!(c, Cue::Step { .. })),
        );
    }
    assert!(heard.is_empty(), "{heard:?}");
}
