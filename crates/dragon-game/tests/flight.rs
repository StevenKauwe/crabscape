use dragon_game::flight::{FlightRules, Input, Player, Viewport};

fn view() -> Viewport {
    Viewport {
        width: 1000.0,
        height: 700.0,
        scale: 40.0,
        ground_y: 700.0,
        top_limit: 50.0,
        dragon_scale: 1.0,
    }
}
fn player() -> Player {
    Player {
        x: 500.0,
        y: 300.0,
        ..Player::default()
    }
}
fn close(a: f64, b: f64) {
    assert!((a - b).abs() < 1e-9, "{a} != {b}");
}

#[test]
fn acceleration_diagonal_and_frame_schedule_are_preserved() {
    let mut p = player();
    p.step(
        0.05,
        Input {
            right: true,
            down: true,
            ..Input::default()
        },
        view(),
        FlightRules::default(),
    )
    .unwrap();
    close(p.vx, 105.0 / 2.0_f64.sqrt());
    close(p.vy, p.vx);
    close(p.x, 500.0 + p.vx * 0.05);
    let mut whole = player();
    let mut split = player();
    let input = Input {
        right: true,
        ..Input::default()
    };
    whole
        .step(1.0, input, view(), FlightRules::default())
        .unwrap();
    for _ in 0..2 {
        split
            .step(0.025, input, view(), FlightRules::default())
            .unwrap();
    }
    close(whole.vx, 105.0);
    close(split.vx, 95.8125);
    assert_ne!(
        whole.x, split.x,
        "the legacy integrator is frame-schedule dependent"
    );
}

#[test]
fn joystick_overrides_keys_and_has_strict_dead_zone_and_facing_thresholds() {
    for (stick, vx, facing) in [
        ([0.12, 0.0], 0.0, -1),
        ([0.15, 0.0], 15.75, -1),
        ([0.16, 0.0], 16.8, 1),
        ([-2.0, 0.0], -105.0, -1),
    ] {
        let mut p = Player {
            facing: -1,
            ..player()
        };
        p.step(
            0.05,
            Input {
                right: true,
                stick: Some(stick),
                ..Input::default()
            },
            view(),
            FlightRules::default(),
        )
        .unwrap();
        close(p.vx, vx);
        assert_eq!(p.facing, facing);
    }
}

#[test]
fn timer_expiry_precedes_energy_shadow_speed_and_recovery() {
    let mut p = Player {
        tired: 0.025,
        energy: 80.0,
        shadow: 0.025,
        mega_t: 0.025,
        ..player()
    };
    let events = p
        .step(
            0.05,
            Input {
                right: true,
                fire: true,
                ..Input::default()
            },
            view(),
            FlightRules {
                magic: true,
                tea: 2.0,
                ..FlightRules::default()
            },
        )
        .unwrap();
    close(p.energy, 80.325);
    close(p.vx, 121.8);
    assert!(events.mega_blast && events.shadow_burst && events.shoot);
    let events = p
        .step(0.05, Input::default(), view(), FlightRules::default())
        .unwrap();
    assert!(!events.mega_blast && !events.shadow_burst);
    p.down = 0.025;
    p.shadow = 1.0;
    p.dizzy = 1.0;
    p.hearts = 0;
    let events = p
        .step(
            0.05,
            Input {
                fire: true,
                ..Input::default()
            },
            view(),
            FlightRules::default(),
        )
        .unwrap();
    assert!(events.recovered && !events.shoot);
    assert_eq!(p.hearts, 3);
    close(p.safe, 2.5);
    close(p.shadow, 1.0);
    close(p.dizzy, 1.0);
}

#[test]
fn out_and_dizzy_early_paths_and_zero_time_match_legacy() {
    let mut p = Player {
        out: true,
        down: 1.0,
        shadow: 1.0,
        vx: 100.0,
        vy: 100.0,
        ..player()
    };
    let e = p
        .step(
            0.05,
            Input {
                fire: true,
                ..Input::default()
            },
            view(),
            FlightRules::default(),
        )
        .unwrap();
    close(p.x, 467.04);
    close(p.y, 330.24);
    close(p.vx, 0.0);
    close(p.down, 1.0);
    close(p.shadow, 1.0);
    assert!(!e.shoot);
    let mut p = Player {
        dizzy: 0.025,
        spin: 3.0,
        vx: 100.0,
        vy: -100.0,
        ..player()
    };
    let e = p
        .step(
            0.05,
            Input {
                right: true,
                fire: true,
                ..Input::default()
            },
            view(),
            FlightRules::default(),
        )
        .unwrap();
    close(p.vx, 85.0);
    close(p.vy, -85.0);
    close(p.spin, 0.0);
    assert!(!e.shoot);
    let before = p.clone();
    let e = p
        .step(
            0.0,
            Input {
                fire: true,
                ..Input::default()
            },
            view(),
            FlightRules::default(),
        )
        .unwrap();
    assert_eq!(before, p);
    assert!(e.shoot, "legacy zero-time frames still request shooting");
}

#[test]
fn viewport_clamps_tiny_screens_without_reversing_bounds() {
    let v = Viewport::from_screen(390.0, 844.0, 1.5, 96.0, 0.62);
    close(v.width, 260.0);
    close(v.scale, 390.0 / 11.0);
    close(v.ground_y, (844.0 - 844.0 * 0.13) / 1.5);
    close(v.top_limit, 68.0);
    let tiny = Viewport {
        width: 10.0,
        height: 700.0,
        ground_y: 20.0,
        ..view()
    };
    let mut p = Player {
        x: -100.0,
        y: 10000.0,
        ..player()
    };
    tiny.clamp(&mut p);
    close(p.x, 36.0);
    close(p.y, 80.0);
}

#[test]
fn invalid_inputs_leave_state_untouched() {
    for dt in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let mut p = player();
        let old = p.clone();
        assert!(
            p.step(dt, Input::default(), view(), FlightRules::default())
                .is_err()
        );
        assert_eq!(p, old);
    }
    let mut p = player();
    let old = p.clone();
    assert!(
        p.step(
            0.01,
            Input {
                stick: Some([f64::NAN, 0.0]),
                ..Input::default()
            },
            view(),
            FlightRules::default()
        )
        .is_err()
    );
    assert_eq!(p, old);
}
