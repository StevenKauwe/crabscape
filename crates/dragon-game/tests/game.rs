use dragon_game::{
    combat::Shot,
    damage::Hazard,
    flight::{Input, Viewport},
    game::Game,
    gems::Gem,
};
fn view() -> Viewport {
    Viewport::from_screen(1000., 700., 1., 40., 0.62)
}
#[test]
fn first_flight_can_clear_every_stop_and_sweep_before_results() {
    let mut g = Game::first_flight(view(), 42);
    g.player.safe = 10000.;
    let mut max_stop = 0;
    for _ in 0..6000 {
        // Synthetic perfect shots exercise collision -> defeat -> drops -> waves, not
        // a claim that a stationary human could win or that the game feel is verified.
        for e in &g.enemies {
            if !e.target.dead && !e.target.calm {
                g.shots.push(Shot {
                    x: e.target.x,
                    y: e.target.y,
                    r: 2.,
                    dmg: 10.,
                    life: 1.,
                    owner: "bond".into(),
                    ..Shot::default()
                });
            }
        }
        g.step(0.05, Input::default(), view()).unwrap();
        max_stop = max_stop.max(g.mission.stop);
        if g.result.is_some() {
            break;
        }
    }
    assert_eq!(max_stop, 3);
    assert_eq!(g.result, Some(true));
    assert!(g.kills >= 28);
    assert!(g.xp >= 112.);
    assert!(g.earned > 0.);
    assert_eq!(g.mission.carry, 0.);
    let before = (g.earned, g.xp, g.mission.t, g.rng);
    g.step(
        0.05,
        Input {
            fire: true,
            ..Input::default()
        },
        view(),
    )
    .unwrap();
    assert_eq!(before, (g.earned, g.xp, g.mission.t, g.rng));
}
#[test]
fn last_heart_retires_before_gem_collection_and_results_do_not_bank() {
    let mut g = Game::first_flight(view(), 7);
    g.player.safe = 0.;
    g.player.hearts = 1;
    g.mission.carry = 5.;
    g.hazards.push(Hazard {
        x: g.player.x,
        y: g.player.y,
        r: 20.,
        life: 3.,
        uid: 1,
        ..Hazard::default()
    });
    g.gems.push(Gem {
        x: g.player.x,
        y: g.player.y,
        life: 3.,
        r: 10.,
        value: 2.,
        ..Gem::default()
    });
    g.step(0., Input::default(), view()).unwrap();
    assert!(g.player.out);
    assert_eq!(g.player.hearts, 0);
    assert_eq!(g.lost, 3.);
    assert_eq!(g.mission.carry, 2.);
    assert_eq!(g.earned, 2.);
    assert_eq!(g.mission.fail_t, 1.6);
    for _ in 0..40 {
        g.step(0.05, Input::default(), view()).unwrap();
    }
    assert_eq!(g.result, Some(false));
    assert_eq!(g.mission.banked, 0.);
}
#[test]
fn seeded_replay_and_invalid_input_are_stable() {
    let mut a = Game::first_flight(view(), 123);
    let mut b = Game::first_flight(view(), 123);
    for i in 0..900 {
        let input = Input {
            fire: true,
            right: i % 70 < 35,
            up: i % 60 < 30,
            ..Input::default()
        };
        a.step(1. / 60., input, view()).unwrap();
        b.step(1. / 60., input, view()).unwrap();
    }
    assert_eq!(a.player, b.player);
    assert_eq!(a.enemies, b.enemies);
    assert_eq!(a.gems, b.gems);
    assert_eq!(a.mission, b.mission);
    assert_eq!(a.rng, b.rng);
    let before = a.player.clone();
    assert!(a.step(f64::NAN, Input::default(), view()).is_err());
    assert_eq!(a.player, before);
}
#[test]
fn zero_time_fires_from_pre_motion_mouth() {
    let mut g = Game::first_flight(view(), 2);
    let x = g.player.x;
    let y = g.player.y;
    g.step(
        0.,
        Input {
            fire: true,
            ..Input::default()
        },
        view(),
    )
    .unwrap();
    assert_eq!(g.shots.len(), 1);
    assert_eq!(g.shots[0].x, x + view().scale * view().dragon_scale * 0.95);
    assert_eq!(g.shots[0].y, y - view().scale * view().dragon_scale * 0.2);
}

#[test]
fn nonlethal_bond_hit_freezes_the_following_frame() {
    let mut g = Game::first_flight(view(), 3);
    g.player.safe = 100.;
    for _ in 0..200 {
        g.step(0.05, Input::default(), view()).unwrap();
        if !g.enemies.is_empty() {
            break;
        }
    }
    let e = &g.enemies[0];
    assert_eq!(e.target.max_hp, 1.15);
    g.shots.push(Shot {
        x: e.target.x,
        y: e.target.y,
        r: 2.,
        dmg: 0.6,
        life: 1.,
        owner: "bond".into(),
        ..Shot::default()
    });
    g.step(0., Input::default(), view()).unwrap();
    assert_eq!(g.hit_stop, 0.025);
    assert!(!g.enemies[0].target.dead);
    let t = g.mission.t;
    g.step(0.01, Input::default(), view()).unwrap();
    assert_eq!(g.mission.t, t);
    assert!((g.hit_stop - 0.015).abs() < 1e-12);
}

#[test]
fn late_sweep_gems_are_banked_before_success_result() {
    let mut g = Game::first_flight(view(), 3);
    g.mission.done = true;
    g.mission.end_t = 0.01;
    g.mission.carry = 4.;
    g.gems.push(Gem {
        x: g.player.x,
        y: g.player.y,
        life: 1.,
        r: 10.,
        value: 3.,
        ..Gem::default()
    });
    g.step(0.02, Input::default(), view()).unwrap();
    assert_eq!(g.result, Some(true));
    assert_eq!(g.earned, 7.);
    assert_eq!(g.mission.banked, 7.);
    assert_eq!(g.mission.carry, 0.);
}

fn perfect_shots(g: &mut Game) {
    for e in &g.enemies {
        if !e.target.dead && !e.target.calm {
            g.shots.push(Shot {
                x: e.target.x,
                y: e.target.y,
                r: 2.,
                dmg: 1000.,
                life: 1.,
                owner: "bond".into(),
                ..Shot::default()
            });
        }
    }
}
#[test]
fn all_four_hollowbrook_missions_reach_results() {
    for id in ["m1", "m2", "m3", "m4"] {
        let mut g = Game::start(id, view(), 42).unwrap();
        g.player.safe = 10000.;
        let mut objective_seen = false;
        let mut boss_seen = false;
        for _ in 0..10000 {
            objective_seen |= g.objective.is_some();
            boss_seen |= g.enemies.iter().any(|e| e.target.boss);
            perfect_shots(&mut g);
            g.step(0.05, Input::default(), view()).unwrap();
            if g.result.is_some() {
                break;
            }
        }
        assert_eq!(g.result, Some(true), "{id}");
        assert_eq!(g.mission.stop, 3);
        assert_eq!(objective_seen, id == "m3");
        assert_eq!(boss_seen, id == "m4");
    }
    assert!(Game::start("m5", view(), 1).is_err());
}
#[test]
fn protecting_the_barn_can_fail_the_mission() {
    let mut g = Game::start("m3", view(), 7).unwrap();
    g.player.safe = 10000.;
    for _ in 0..10000 {
        perfect_shots(&mut g);
        g.step(0.05, Input::default(), view()).unwrap();
        if g.objective.is_some() {
            break;
        }
    }
    let ob = g.objective.as_mut().unwrap();
    ob.hp = 1;
    ob.hurt_t = 0.;
    g.hazards.push(Hazard {
        x: ob.x,
        y: ob.y,
        r: 20.,
        life: 3.,
        kind: "spark".into(),
        uid: 99999,
        ..Hazard::default()
    });
    g.step(0., Input::default(), view()).unwrap();
    assert_eq!(g.mission.fail_t, 1.6);
    assert_eq!(g.fail_reason.as_deref(), Some("The hay barn was lost!"));
    for _ in 0..40 {
        g.step(0.05, Input::default(), view()).unwrap();
    }
    assert_eq!(g.result, Some(false));
}
#[test]
fn boss_phase_spawns_help_and_defeat_calms_it_before_adjacent_shot() {
    let mut g = Game::start("m4", view(), 11).unwrap();
    g.player.safe = 10000.;
    for _ in 0..10000 {
        perfect_shots(&mut g);
        g.step(0.05, Input::default(), view()).unwrap();
        if g.enemies.iter().any(|e| e.target.boss) {
            break;
        }
    }
    let b = g.enemies.iter().find(|e| e.target.boss).unwrap();
    let hp = b.target.hp;
    g.shots.push(Shot {
        x: b.target.x,
        y: b.target.y,
        r: 2.,
        dmg: hp * 0.5,
        life: 1.,
        owner: "bond".into(),
        ..Shot::default()
    });
    g.step(0., Input::default(), view()).unwrap();
    assert_eq!(g.mission.phase, 1);
    assert_eq!(
        g.enemies
            .iter()
            .filter(|e| e.extra && !e.target.calm)
            .count(),
        4
    );
    let extra = g
        .enemies
        .iter()
        .find(|e| e.extra && !e.target.calm)
        .unwrap();
    let extra_id = extra.target.id;
    let extra_hp = extra.target.hp;
    // Source traverses shots backwards: kill boss first, then the add shot must skip.
    g.shots.push(Shot {
        x: extra.target.x,
        y: extra.target.y,
        r: 2.,
        dmg: 1000.,
        life: 1.,
        owner: "bond".into(),
        ..Shot::default()
    });
    let b = g.enemies.iter().find(|e| e.target.boss).unwrap();
    g.shots.push(Shot {
        x: b.target.x,
        y: b.target.y,
        r: 2.,
        dmg: 1000.,
        life: 1.,
        owner: "bond".into(),
        ..Shot::default()
    });
    let xp = g.xp;
    g.step(0., Input::default(), view()).unwrap();
    let extra = g.enemies.iter().find(|e| e.target.id == extra_id).unwrap();
    assert!(extra.target.calm);
    assert_eq!(extra.target.hp, extra_hp);
    assert!(g.xp >= xp + 80.);
    assert!(g.mission.done);
    assert!(g.gems.iter().filter(|g| g.golden).count() >= 3);
}
