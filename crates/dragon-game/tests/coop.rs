use dragon_game::{
    authority::Authority,
    combat::Shot,
    coop::Session,
    enemies::{Enemy, Kind, World},
    flight::{Input, Viewport},
    game::Game,
    missions::Leg,
};
use serde_json::json;
fn view() -> Viewport {
    Viewport::from_screen(1000., 700., 1., 40., 0.62)
}
fn pair() -> (Session, Session) {
    let mut host = Session::new(
        Game::first_flight(view(), 1),
        "a".into(),
        0,
        "run1".into(),
        Authority {
            guest: false,
            host_peer: None,
            host_rider: 0,
            epoch: 0,
        },
    )
    .unwrap();
    let mut guest = Session::new(
        Game::first_flight(view(), 2),
        "b".into(),
        1,
        "run1".into(),
        Authority {
            guest: true,
            host_peer: Some("a".into()),
            host_rider: 0,
            epoch: 0,
        },
    )
    .unwrap();
    host.bind_peer("b".into(), 1, false).unwrap();
    guest.bind_peer("a".into(), 0, false).unwrap();
    host.game.player.safe = 100.;
    guest.game.player.safe = 100.;
    (host, guest)
}
fn exchange(a: &mut Session, b: &mut Session, t: f64) {
    for _ in 0..6 {
        let aa = a.drain_outbound();
        let bb = b.drain_outbound();
        if aa.is_empty() && bb.is_empty() {
            break;
        }
        for p in aa {
            assert!(
                b.receive(&a.peer, p.topic, &p.data, t),
                "{} {}",
                p.topic,
                p.data
            );
        }
        for p in bb {
            assert!(
                a.receive(&b.peer, p.topic, &p.data, t),
                "{} {}",
                p.topic,
                p.data
            );
        }
    }
}
fn enemy(s: &mut Session, kind: Kind) -> u64 {
    let v = view();
    let id = 71;
    let e = Enemy::spawn(
        kind,
        id,
        false,
        Some([500., 200.]),
        &World {
            width: v.width,
            ground_y: v.ground_y,
            top: v.top_limit,
            sky_bottom: v.ground_y - v.scale * 0.5,
            scale: v.scale,
            player: [100., 200.],
            aim: [100., 200.],
            quiet: false,
            safe: true,
            dragon_scale: v.dragon_scale,
        },
        s.game.scaling(),
        &mut || 0.5,
    );
    s.game.enemies.push(e);
    s.game.mission.leg = Leg::Fight;
    s.game.mission.wave = 0;
    s.game.mission.total = 1;
    s.game.mission.spawn_t = 10.;
    id
}
#[test]
fn guest_hit_host_defeat_keeps_local_rewards_and_host_health_authority() {
    let (mut h, mut g) = pair();
    let id = enemy(&mut h, Kind::Cloud);
    h.step(0., Input::default(), view(), 0., false).unwrap();
    g.step(0., Input::default(), view(), 0., false).unwrap();
    exchange(&mut h, &mut g, 0.);
    let t = &g.game.enemies[0].target;
    let hp = t.hp;
    g.game.shots.push(Shot {
        x: t.x,
        y: t.y,
        r: 2.,
        dmg: 20.,
        life: 1.,
        ..Shot::default()
    });
    g.step(0., Input::default(), view(), 0.01, false).unwrap();
    assert_eq!(g.game.enemies[0].target.hp, hp);
    assert_eq!(g.game.xp, 0.);
    assert!(g.game.gems.is_empty());
    exchange(&mut h, &mut g, 0.01);
    assert!(
        h.game
            .enemies
            .iter()
            .find(|e| e.target.id == id)
            .unwrap()
            .target
            .dead
    );
    assert!(g.game.enemies.is_empty());
    assert_eq!(h.game.mission.kills, 1);
    assert_eq!(h.game.kills, 0);
    assert_eq!(g.game.kills, 1);
    assert_eq!(h.game.xp, 0.);
    assert_eq!(g.game.xp, 4.);
    assert_eq!(h.game.gems.len(), 2);
    assert_eq!(g.game.gems.len(), 2);
    assert_ne!(
        h.game.gems[0].vx, g.game.gems[0].vx,
        "each device owns local reward RNG"
    );
}
#[test]
fn authenticated_peer_run_epoch_and_slot_are_required() {
    let (mut h, mut g) = pair();
    let id = enemy(&mut h, Kind::Cloud);
    let packet = json!({"r":1,"mid":"m1","run":"run1","epoch":0,"uid":id,"dmg":20});
    assert!(!h.receive("unknown", "hit", &packet, 0.));
    for (key, value) in [
        ("r", json!(0)),
        ("run", json!("old")),
        ("epoch", json!(1)),
        ("mid", json!("m2")),
    ] {
        let mut p = packet.clone();
        p[key] = value;
        assert!(!h.receive("b", "hit", &p, 0.));
    }
    assert!(h.receive("b", "hit", &packet, 0.));
    assert!(
        !h.receive("b", "hit", &packet, 0.),
        "already dead target cannot be hit again"
    );
    let stolen = json!({"r":0,"mid":"m1","run":"run1","epoch":0,"uid":id,"type":"cloud","x":0.5,"y":0.5,"by":1});
    assert!(!g.receive("b", "defeat", &stolen, 0.));
    g.disconnect("a");
    assert!(!g.receive("a", "defeat", &stolen, 0.));
}
#[test]
fn paused_host_prevents_timeout_then_disconnect_transfers_queue_without_local_rewards() {
    let (mut h, mut g) = pair();
    enemy(&mut h, Kind::Cloud);
    h.game.mission.queue.push_back("bat".into());
    h.step(0., Input::default(), view(), 0., false).unwrap();
    g.step(0., Input::default(), view(), 0., false).unwrap();
    exchange(&mut h, &mut g, 0.);
    g.game.mission.carry = 7.;
    g.game.xp = 9.;
    g.game.mission.gem_timer = 1000.;
    h.step(0., Input::default(), view(), 1., true).unwrap();
    exchange(&mut h, &mut g, 1.);
    let old = h.game.enemies[0].t;
    for i in 1..=80 {
        g.step(0.05, Input::default(), view(), 1. + i as f64 * 0.05, false)
            .unwrap();
    }
    assert!(g.authority().guest);
    assert_eq!(h.game.enemies[0].t, old);
    g.disconnect("a");
    for i in 1..=80 {
        g.step(0.05, Input::default(), view(), 5. + i as f64 * 0.05, false)
            .unwrap();
        if !g.authority().guest {
            break;
        }
    }
    assert!(!g.authority().guest);
    assert_eq!(g.authority().epoch, 1);
    assert_eq!(
        g.game.mission.queue.front().map(String::as_str),
        Some("bat")
    );
    assert_eq!(g.game.mission.carry, 7.);
    assert_eq!(g.game.xp, 9.);
    assert!(g.game.enemies[0].entered);
    assert_eq!(g.game.enemies[0].windup, 0.);
}
#[test]
fn explicit_handoff_preserves_run_and_new_host_rejects_old_epoch() {
    let (mut h, mut g) = pair();
    let id = enemy(&mut h, Kind::Cloud);
    h.step(0., Input::default(), view(), 0., false).unwrap();
    g.step(0., Input::default(), view(), 0., false).unwrap();
    exchange(&mut h, &mut g, 0.);
    h.handoff();
    exchange(&mut h, &mut g, 0.);
    assert!(h.authority().guest);
    assert!(!g.authority().guest);
    assert_eq!(g.authority().epoch, 1);
    assert_eq!(g.run(), "run1");
    assert!(!g.receive(
        "a",
        "hit",
        &json!({"r":0,"mid":"m1","run":"run1","epoch":0,"uid":id,"dmg":20}),
        0.
    ));
    g.step(0., Input::default(), view(), 0.1, false).unwrap();
    exchange(&mut h, &mut g, 0.1);
    assert_eq!(h.authority().epoch, 1);
}
#[test]
fn guardian_presence_heals_once_and_never_revives_retired_riders() {
    let (mut h, mut g) = pair();
    h.game.player.x = 300.;
    g.game.player.x = 300.;
    h.game.player.y = 200.;
    g.game.player.y = 200.;
    h.step(0., Input::default(), view(), 0., false).unwrap();
    g.step(0., Input::default(), view(), 0., false).unwrap();
    exchange(&mut h, &mut g, 0.);
    g.step(0., Input::default(), view(), 0.01, false).unwrap();
    g.game.player.hearts = 2;
    g.game.player.shield = 0.;
    h.game.cast_count = 1;
    h.game.last_cast = Some("guard".into());
    h.step(0., Input::default(), view(), 0.1, false).unwrap();
    let packets = h.drain_outbound();
    let p = packets.iter().find(|p| p.topic == "presence").unwrap();
    assert!(g.receive("a", "presence", &p.data, 0.1));
    assert_eq!(g.game.player.hearts, 3);
    assert_eq!(g.game.player.shield, 2.5);
    assert!(g.receive("a", "presence", &p.data, 0.1));
    assert_eq!(g.game.player.hearts, 3);
    g.game.player.out = true;
    g.game.player.hearts = 0;
    let mut next = p.data.clone();
    next["cs"] = json!([2, "guard"]);
    assert!(g.receive("a", "presence", &next, 0.2));
    assert!(g.game.player.out);
    assert_eq!(g.game.player.hearts, 0);
}
#[test]
fn guest_world_resize_keeps_normalized_positions_and_local_carry() {
    let (mut h, mut g) = pair();
    enemy(&mut h, Kind::Cloud);
    g.game.mission.carry = 11.;
    let v = Viewport::from_screen(1400., 900., 1., 40., 0.62);
    g.step(0., Input::default(), v, 0., false).unwrap();
    h.step(0., Input::default(), view(), 0., false).unwrap();
    exchange(&mut h, &mut g, 0.);
    assert_eq!(g.game.mission.carry, 11.);
    assert!(
        (g.game.enemies[0].target.x / v.width - h.game.enemies[0].target.x / view().width).abs()
            < 0.001
    );
    let before = g.game.enemies[0].target.hp;
    g.step(0.05, Input::default(), v, 0.05, false).unwrap();
    assert_eq!(g.game.enemies[0].target.hp, before);
    assert!(g.game.hazards.is_empty());
}

#[test]
fn actual_transport_admission_blocks_replay_before_gameplay() {
    use dragon_peer::{Delivery, Session as Transport};
    let (mut host, mut guest) = pair();
    let mut sender = Transport::default();
    let mut receiver = Transport::default();
    assert!(sender.connected(guest.peer.clone()));
    assert!(receiver.connected(host.peer.clone()));
    for i in 0..4 {
        host.step(0.05, Input::default(), view(), i as f64 * 0.05, false)
            .unwrap();
    }
    let packets = host.drain_outbound();
    assert!(packets.iter().any(|p| p.topic == "world"));
    for packet in packets {
        let delivery = if matches!(packet.topic, "presence" | "world") {
            Delivery::Unreliable
        } else {
            Delivery::Reliable
        };
        let bytes = sender.encode(packet.topic, packet.data).unwrap();
        let admitted = receiver.receive(&host.peer, delivery, &bytes).unwrap();
        assert!(guest.receive(&admitted.peer, &admitted.topic, &admitted.data, 0.2));
        let before = (
            guest.game.player.hearts,
            guest.game.mission.carry,
            guest.game.earned,
            guest.authority().epoch,
        );
        assert!(receiver.receive(&host.peer, delivery, &bytes).is_none());
        assert_eq!(
            before,
            (
                guest.game.player.hearts,
                guest.game.mission.carry,
                guest.game.earned,
                guest.authority().epoch
            )
        );
        assert!(
            receiver
                .receive(&"unadmitted".into(), delivery, &bytes)
                .is_none()
        );
    }
}
