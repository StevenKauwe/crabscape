//! Co-op: two devices, the host and a guest, each running its own `Sim`, talking over a pretend connection
//! (`net::FakeLink`) with lag, jitter and lost updates. These are the promises that make co-op feel crisp for
//! everyone (`crabsim::net` explains the model): your own crab never feels the lag, the defender decides a blow
//! on their own screen, and the attacker sees a hit land at once.

use bevy_math::Vec2;
use crabsim::enemy::State;
use crabsim::net::{FakeLink, SEND_EVERY};
use crabsim::physics::StepEvents;
use crabsim::replay::ticks;
use crabsim::{Feel, Kind, Level, PlayerId, Replay, SPAWN, Sim, Tuning};

const HOST: PlayerId = PlayerId(0);
const GUEST: PlayerId = PlayerId(1);

/// Where the host's crab waits: far from the fights near the spawn (the
/// fixture notices crabs 400 away).
const FAR: Vec2 = Vec2::new(1150.0, 110.0);

/// 150 ms each way.
const LAG: u32 = 18;

/// The guest's crab, standing by the spawn.
const NEAR: Vec2 = Vec2::new(SPAWN.x, 0.0);

/// A world on preset A's fighting numbers, like `scenarios.rs`.
fn world(setup: &impl Fn(&mut Sim)) -> Sim {
    let mut sim = Sim::new(Tuning::default(), Level::test_level());
    sim.set_feel(Feel::Current);
    setup(&mut sim);
    sim
}

/// Two devices and the connection between them.
struct Game {
    host: Sim,
    guest: Sim,
    link: FakeLink,
}

/// The host's crab at `FAR`, the guest's at `guest_at`; `setup` runs on both
/// (tuning, enemies), so they start from the same world.
fn game(guest_at: Vec2, link: FakeLink, setup: impl Fn(&mut Sim)) -> Game {
    let mut host = world(&setup);
    host.add_crab(HOST, FAR);
    host.add_remote_crab(GUEST, guest_at);
    host.go_online(true);
    let mut guest = world(&setup);
    guest.add_remote_crab(HOST, FAR);
    guest.add_crab(GUEST, guest_at);
    guest.go_online(false);
    Game { host, guest, link }
}

/// A solo world with only the guest's crab, to compare against.
fn solo(at: Vec2, setup: impl Fn(&mut Sim)) -> Sim {
    let mut sim = world(&setup);
    sim.add_crab(GUEST, at);
    sim
}

/// Play the two scripts tick by tick (`after` sees both devices after each).
fn play(g: &mut Game, host: &str, guest: &str, n: u64, mut after: impl FnMut(&Game)) {
    let (host, guest) = (Replay::parse(host).unwrap(), Replay::parse(guest).unwrap());
    let (mut hp, mut gp) = (host.playback(), guest.playback());
    for _ in 0..n {
        g.host.set_controls(HOST, hp.next_controls());
        g.guest.set_controls(GUEST, gp.next_controls());
        g.host.tick();
        g.guest.tick();
        g.link.carry(&mut [&mut g.host, &mut g.guest]);
        after(g);
    }
}

const BUSY: &str = "t=0 walk=right; t=0.4 jump; t=0.7 jump; t=0.9 release; t=1.6 dodge; t=1.8 walk=-0.4; \
                    t=3 walk=right jump; t=3.4 jump; t=4 dodge; t=5.5 walk=left; t=6 jump; t=6.6 sprint; t=7.4 slow";

#[test]
fn your_own_crab_moves_exactly_as_in_solo_whatever_the_lag() {
    let mut g = game(NEAR, FakeLink::new(LAG, 6, 0.2, 7), |_| {});
    let mut alone = solo(NEAR, |_| {});
    let script = Replay::parse(BUSY).unwrap();
    let mut playback = script.playback();
    play(
        &mut g,
        "t=0 walk=left; t=1 jump; t=3 walk=right",
        BUSY,
        ticks(8.0),
        |g| {
            alone.set_controls(GUEST, playback.next_controls());
            alone.tick();
            assert_eq!(
                g.guest.crab(GUEST).unwrap().mover,
                alone.crab(GUEST).unwrap().mover,
                "tick {}",
                alone.tick_count()
            );
        },
    );
}

#[test]
fn friends_see_each_other_a_little_in_the_past() {
    let mut g = game(NEAR, FakeLink::steady(LAG), |_| {});
    let (mut guest_was, mut host_was) = (Vec::new(), Vec::new());
    play(&mut g, BUSY, BUSY, ticks(8.0), |g| {
        guest_was.push(g.guest.crab(GUEST).unwrap().mover.pos);
        host_was.push(g.host.crab(HOST).unwrap().mover.pos);
        let n = guest_was.len();
        if n < 60 {
            return;
        }
        // What each shows of the other is where it really was, the lag (and
        // at most two updates' gap) ago.
        let recent = |was: &[Vec2]| {
            was[n - LAG as usize - 2 * SEND_EVERY as usize..=n - LAG as usize].to_vec()
        };
        let shown = g.host.crab(GUEST).unwrap().mover.pos;
        assert!(
            recent(&guest_was).contains(&shown),
            "tick {n}: host shows {shown}"
        );
        let shown = g.guest.crab(HOST).unwrap().mover.pos;
        assert!(
            recent(&host_was).contains(&shown),
            "tick {n}: guest shows {shown}"
        );
    });
}

#[test]
fn a_friends_crab_moves_only_by_their_updates() {
    // Every update lost: the host's copy of the guest stays where it started.
    let mut g = game(NEAR, FakeLink::new(LAG, 0, 1.0, 1), |_| {});
    play(&mut g, "", "t=0 walk=right", ticks(2.0), |_| {});
    assert!(g.guest.crab(GUEST).unwrap().mover.pos.x > NEAR.x + 100.0);
    assert_eq!(g.host.crab(GUEST).unwrap().mover.pos, NEAR);
}

/// The guest by the spawn against the fixture at 240. The host starts out
/// running it and hands it to the guest as soon as it hunts the guest.
struct GuestDuel {
    /// The guest's crab's events, tick by tick (index 0 = tick 1)...
    guest: Vec<StepEvents>,
    /// ...and what the host shows of them.
    host_saw: Vec<StepEvents>,
    /// The host's copy of the fixture's state, tick by tick.
    host_states: Vec<State>,
    /// The guest's crab's health, tick by tick.
    health: Vec<f32>,
}

fn guest_duel(script: &str) -> GuestDuel {
    let mut g = game(NEAR, FakeLink::new(LAG, 3, 0.1, 3), |sim| {
        sim.spawn_enemy(Kind::Fixture, Vec2::new(240.0, 0.0));
    });
    let mut d = GuestDuel {
        guest: Vec::new(),
        host_saw: Vec::new(),
        host_states: Vec::new(),
        health: Vec::new(),
    };
    play(&mut g, "", script, 160, |g| {
        let crab = g.guest.crab(GUEST).unwrap();
        d.guest.push(crab.events);
        d.health.push(crab.fighter.vitals.health);
        d.host_saw.push(g.host.crab(GUEST).unwrap().events);
        d.host_states.push(g.host.enemies()[0].state);
    });
    d
}

/// The ticks (1 = the first) on which `f` holds.
fn ticks_where(events: &[StepEvents], f: impl Fn(&StepEvents) -> bool) -> Vec<usize> {
    (0..events.len())
        .filter(|&i| f(&events[i]))
        .map(|i| i + 1)
        .collect()
}

#[test]
fn the_guest_parries_on_their_own_screen_with_the_same_window_as_solo() {
    // When the blow lands on the guest who does nothing, as the guest sees it.
    let h = ticks_where(&guest_duel("").guest, |e| e.hurt)[0];
    // Solo: a press 10 ticks before impact parries, 11 is too early
    // (`scenarios.rs`). Over 150 ms of lag, measured on the guest's screen,
    // it's the same.
    let parry = guest_duel(&format!("@{} parry", h - 11));
    assert_eq!(ticks_where(&parry.guest, |e| e.parried)[0], h);
    assert_eq!(parry.health[h + 20], 100.0, "no damage");
    // The host hears of it and its fixture reels, a lag later.
    let reels = 1 + parry
        .host_states
        .iter()
        .position(|s| *s == State::Stagger)
        .expect("staggered");
    assert!(
        reels > h + LAG as usize,
        "host staggers it at {reels}, guest parried at {h}"
    );
    // (Unparried, it attacks again later: only its first blow counts here.)
    let tapped = guest_duel(&format!("@{} parry", h - 12));
    assert!(ticks_where(&tapped.guest, |e| e.parried).is_empty());
    assert_eq!(ticks_where(&tapped.guest, |e| e.hurt)[0], h);
}

#[test]
fn only_the_guests_device_decides_blows_on_the_guest() {
    // The host's copy of the fixture strikes long before the guest's does;
    // the host must not judge that blow on its copy of the guest's crab, or
    // it would show a hit the guest may have parried.
    let hurt = guest_duel("");
    let h = ticks_where(&hurt.guest, |e| e.hurt)[0];
    let shown = ticks_where(&hurt.host_saw, |e| e.hurt)[0];
    assert!(
        shown > h + LAG as usize,
        "the host shows the hit at {shown}, after the guest's {h}"
    );
    let parry = guest_duel(&format!("@{} parry", h - 11));
    assert!(
        ticks_where(&parry.host_saw, |e| e.hurt).is_empty(),
        "the host never shows a hit"
    );
    assert!(
        !ticks_where(&parry.host_saw, |e| e.parried).is_empty(),
        "it shows the parry"
    );
}

/// A fixture that never notices anyone: a punching bag.
fn bag(sim: &mut Sim) {
    sim.tuning.fixture.notice_range = 0.0;
    sim.spawn_enemy(Kind::Fixture, Vec2::new(240.0, 0.0));
}

#[test]
fn the_guests_hit_shows_at_once_and_lands_on_the_host() {
    const SMASH: &str = "t=0.1 smash; t=0.8 jab";
    // Solo: the tick each claw takes health off the bag.
    let mut alone = solo(NEAR, bag);
    let mut solo_health = Vec::new();
    let script = Replay::parse(SMASH).unwrap();
    let mut playback = script.playback();
    for _ in 0..ticks(1.8) {
        alone.set_controls(GUEST, playback.next_controls());
        alone.tick();
        solo_health.push(alone.enemies()[0].vitals.health);
    }
    // Lost and late updates from the host can't make the guest's copy
    // flick back to full health while the host catches up.
    let mut g = game(NEAR, FakeLink::new(LAG, 6, 0.3, 11), bag);
    let (mut guest_health, mut host_health) = (Vec::new(), Vec::new());
    play(&mut g, "", SMASH, ticks(1.8), |g| {
        guest_health.push(g.guest.enemies()[0].vitals.health);
        host_health.push(g.host.enemies()[0].vitals.health);
    });
    assert_eq!(
        guest_health, solo_health,
        "the guest sees exactly what solo would"
    );
    let first_drop = |h: &[f32]| h.iter().position(|&x| x < 40.0).expect("hit");
    assert!(first_drop(&host_health) > first_drop(&guest_health) + LAG as usize);
    assert!(solo_health.last().unwrap() < &30.0, "both claws landed");
    assert_eq!(
        host_health.last(),
        solo_health.last(),
        "the host has both hits"
    );
}

#[test]
fn a_net_cut_by_one_crab_is_cut_for_everyone() {
    let at = |sim: &Sim| {
        let net = sim.level.nets[0].rect;
        Vec2::new(
            net.min.x - sim.tuning.body_half_width - 1.0,
            net.min.y + 40.0,
        )
    };
    let spot = at(&world(&|_| {}));
    let mut g = game(spot, FakeLink::steady(LAG), |_| {});
    g.guest.crab_mut(GUEST).unwrap().mover.scissor_claw = true;
    play(&mut g, "", "", 1, |_| {});
    assert!(g.guest.level.nets[0].cut, "the guest cut it");
    assert!(!g.host.level.nets[0].cut, "not yet: the news takes a while");
    play(&mut g, "", "", u64::from(LAG) + 1, |_| {});
    assert!(g.host.level.nets[0].cut, "the host heard");
}

// --- Enemies run on the device of the crab they hunt -------------------------

#[test]
fn an_enemy_runs_where_its_prey_plays_and_never_in_two_places() {
    // A fixture that gives up once the guest is 300 from its home.
    let setup = |sim: &mut Sim| {
        sim.tuning.fixture.notice_range = 300.0;
        sim.tuning.fixture.give_up_range = 300.0;
        sim.spawn_enemy(Kind::Fixture, Vec2::new(400.0, 0.0));
    };
    let mut g = game(NEAR, FakeLink::new(LAG, 3, 0.1, 5), setup);
    let mut who = Vec::new();
    play(&mut g, "", "t=1.5 walk=left", ticks(5.0), |g| {
        let runs = (g.host.runs(0), g.guest.runs(0));
        assert!(
            !(runs.0 && runs.1),
            "two runners on tick {}",
            g.host.tick_count()
        );
        who.push(runs);
    });
    who.dedup();
    // (host runs it, guest runs it): the host notices the guest at once and
    // hands it over; a lag later the guest runs it; when the guest walks away
    // it gives up, and goes back to the host.
    assert_eq!(
        who,
        [(false, false), (false, true), (false, false), (true, false)]
    );
}

/// An enemy as the rules see it, leaving out who runs it.
fn same_animal(a: &crabsim::Enemy, b: &crabsim::Enemy) -> bool {
    let mut a = a.clone();
    a.runner = b.runner;
    a.handovers = b.handovers;
    a.heard.clone_from(&b.heard);
    a == *b
}

#[test]
fn a_mantis_hunting_the_guest_fights_on_the_guests_device_exactly_as_in_solo() {
    // The crab on open floor at x 250, the mantis's burrow at 470 (as in
    // `scenarios.rs`). Its combo follows a landed first strike.
    const CRAB: Vec2 = Vec2::new(250.0, 0.0);
    let setup = |sim: &mut Sim| {
        sim.spawn_enemy(Kind::Mantis, Vec2::new(470.0, 0.0));
    };
    let mut alone = solo(CRAB, setup);
    let (mut solo_mantis, mut solo_health) = (Vec::new(), Vec::new());
    for _ in 0..400 {
        alone.tick();
        solo_mantis.push(alone.enemies()[0].clone());
        solo_health.push(alone.crab(GUEST).unwrap().fighter.vitals.health);
    }
    let mut g = game(CRAB, FakeLink::new(LAG, 3, 0.1, 9), setup);
    let (mut mantis, mut health, mut runs) = (Vec::new(), Vec::new(), Vec::new());
    play(&mut g, "", "", 400, |g| {
        mantis.push(g.guest.enemies()[0].clone());
        health.push(g.guest.crab(GUEST).unwrap().fighter.vitals.health);
        runs.push(g.guest.runs(0));
    });
    // From the tick the guest takes it over, it plays out as in solo (a lag
    // later): rear, cocked hold, both strikes of the combo, the recovery.
    let took = runs.iter().position(|&r| r).expect("the guest runs it");
    let k = solo_mantis
        .iter()
        .position(|m| same_animal(&mantis[took], m))
        .expect("the same moment exists in solo");
    let recovered = k + solo_mantis[k..]
        .windows(2)
        .position(|w| w[0].state == State::Recover && w[1].state != State::Recover)
        .expect("it recovers");
    let strikes = solo_mantis[k..=recovered]
        .windows(2)
        .filter(|w| w[0].state != State::Attack && w[1].state == State::Attack)
        .count();
    assert_eq!(strikes, 2, "the whole combo is in the stretch compared");
    for j in 0..=recovered - k {
        assert!(
            same_animal(&mantis[took + j], &solo_mantis[k + j]),
            "{j} ticks after the guest took it over"
        );
        assert_eq!(health[took + j], solo_health[k + j]);
    }
    assert!(
        health[took + recovered - k] < 50.0,
        "two clubs and two bubbles landed"
    );
}

#[test]
fn a_hit_that_crosses_a_hand_over_is_not_lost() {
    // Half a second each way: the guest's smash leaves for the host before
    // the host's hand-over reaches the guest. The host passes it on.
    const SLOW: u32 = 60;
    const SMASH: &str = "t=0.05 smash";
    let fixture = |sim: &mut Sim| {
        sim.spawn_enemy(Kind::Fixture, Vec2::new(240.0, 0.0));
    };
    let mut alone = solo(NEAR, |sim| {
        bag(sim);
    });
    let script = Replay::parse(SMASH).unwrap();
    let mut playback = script.playback();
    let mut hit_at = None;
    for _ in 0..ticks(1.0) {
        alone.set_controls(GUEST, playback.next_controls());
        alone.tick();
        let health = alone.enemies()[0].vitals.health;
        if health < 40.0 && hit_at.is_none() {
            hit_at = Some((alone.tick_count(), health));
        }
    }
    let (hit_at, after) = hit_at.expect("the smash lands");
    assert!(
        hit_at < u64::from(SLOW),
        "the smash leaves before the hand-over arrives"
    );
    let mut g = game(NEAR, FakeLink::new(SLOW, 0, 0.0, 1), fixture);
    play(&mut g, "", SMASH, ticks(3.0), |_| {});
    assert!(g.guest.runs(0), "the guest took it over");
    let health = |sim: &Sim| sim.enemies()[0].vitals.health;
    assert_eq!(health(&g.guest), after, "the smash counted, once");
    assert_eq!(health(&g.host), after, "and the host shows it");
}

// --- Messages as bytes, and friends coming and going -------------------------

use crabsim::net::{MAX_BYTES, Msg};

/// Every message a short fight sends, both ways.
fn fight_messages() -> Vec<Msg> {
    let mut g = game(NEAR, FakeLink::steady(LAG), |sim| {
        sim.spawn_enemy(Kind::Mantis, Vec2::new(400.0, 0.0));
    });
    let mut all = Vec::new();
    let script =
        Replay::parse("t=0.2 smash; t=0.6 jab; t=1 walk=right; t=1.4 parry; t=2 jump").unwrap();
    let mut playback = script.playback();
    for _ in 0..ticks(3.0) {
        g.guest.set_controls(GUEST, playback.next_controls());
        g.host.tick();
        g.guest.tick();
        // Two devices: everything each sends is for the other, at once.
        let (to_guest, to_host) = (g.host.take_outbox(), g.guest.take_outbox());
        for out in to_guest {
            all.push(out.msg.clone());
            g.guest.receive(out.msg);
        }
        for out in to_host {
            all.push(out.msg.clone());
            g.host.receive(out.msg);
        }
    }
    all.push(g.host.world());
    all
}

#[test]
fn every_message_comes_back_whole_from_its_bytes_and_stays_small() {
    let msgs = fight_messages();
    let kinds = |m: &Msg| {
        format!("{m:?}")
            .split([' ', '(', '{'])
            .next()
            .unwrap()
            .to_string()
    };
    let mut biggest: Vec<(String, usize)> = Vec::new();
    for msg in &msgs {
        let bytes = msg.to_bytes();
        assert_eq!(Msg::from_bytes(&bytes).as_ref(), Some(msg));
        let kind = kinds(msg);
        match biggest.iter_mut().find(|b| b.0 == kind) {
            Some(b) => b.1 = b.1.max(bytes.len()),
            None => biggest.push((kind, bytes.len())),
        }
    }
    for (kind, size) in &biggest {
        eprintln!("{kind}: at most {size} bytes");
        assert!(*size < MAX_BYTES / 4, "{kind} is {size} bytes");
    }
    assert!(biggest.iter().any(|b| b.0 == "Crab") && biggest.iter().any(|b| b.0 == "Enemy"));
}

#[test]
fn bad_bytes_are_dropped_never_crash() {
    let msgs = fight_messages();
    let mut rng = crabsim::rng::Rng::new(42);
    // Random junk of every length.
    for _ in 0..20_000 {
        let len = rng.range(0, 600) as usize;
        let junk: Vec<u8> = (0..len).map(|_| rng.range(0, 255) as u8).collect();
        let _ = Msg::from_bytes(&junk);
    }
    // Every real message cut short, and with a byte flipped.
    for msg in msgs.iter().step_by(7) {
        let bytes = msg.to_bytes();
        for cut in 0..bytes.len() {
            let _ = Msg::from_bytes(&bytes[..cut]);
            let mut flipped = bytes.clone();
            flipped[cut] ^= 0xff;
            let _ = Msg::from_bytes(&flipped);
        }
    }
    // Too long: dropped unread.
    assert_eq!(Msg::from_bytes(&vec![0; MAX_BYTES + 1]), None);
    // A crab at a position that isn't a number: dropped.
    let crab = msgs
        .iter()
        .find_map(|m| match m {
            Msg::Crab { crab, .. } => Some(crab.clone()),
            _ => None,
        })
        .unwrap();
    for x in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        let mut bad = crab.clone();
        bad.mover.pos.x = x;
        let bytes = Msg::Crab { at: 1, crab: bad }.to_bytes();
        assert_eq!(Msg::from_bytes(&bytes), None, "{x}");
    }
}

#[test]
fn no_name_in_a_message_reads_as_a_broken_number() {
    // `from_bytes` spots NaN and infinity by how they print; a field or
    // variant named, say, `inflow` would make every message look broken.
    for msg in fight_messages() {
        let text = format!("{msg:?}");
        assert!(!text.contains("NaN") && !text.contains("inf"), "{text}");
    }
}

#[test]
fn a_friend_who_joins_late_appears_and_learns_the_world() {
    // The host plays alone, cuts the net, then a guest joins.
    let mut host = world(&|_| {});
    let spot = {
        let net = host.level.nets[0].rect;
        Vec2::new(
            net.min.x - host.tuning.body_half_width - 1.0,
            net.min.y + 40.0,
        )
    };
    host.add_crab(HOST, spot);
    host.crab_mut(HOST).unwrap().mover.scissor_claw = true;
    host.tick();
    assert!(host.level.nets[0].cut);
    host.go_online(true);
    let mut guest = world(&|_| {});
    guest.add_crab(GUEST, NEAR);
    guest.go_online(false);
    // The host greets the newcomer; both send their crabs as usual.
    guest.receive(host.world());
    let mut link = FakeLink::steady(LAG);
    for _ in 0..60 {
        host.tick();
        guest.tick();
        link.carry(&mut [&mut host, &mut guest]);
    }
    assert!(guest.level.nets[0].cut, "the guest learned the net is cut");
    assert!(
        host.crab(GUEST).is_some_and(|c| c.remote),
        "the host sees the guest"
    );
    assert!(
        guest.crab(HOST).is_some_and(|c| c.remote),
        "the guest sees the host"
    );
}

#[test]
fn when_a_friend_leaves_their_enemies_go_back_to_the_host() {
    let mut g = game(NEAR, FakeLink::steady(LAG), |sim| {
        sim.spawn_enemy(Kind::Fixture, Vec2::new(240.0, 0.0));
    });
    play(&mut g, "", "", 60, |_| {});
    assert!(
        g.guest.runs(0),
        "the fixture hunts the guest, on the guest's device"
    );
    // The guest's tab closes.
    g.host.remove_crab(GUEST);
    assert!(g.host.crab(GUEST).is_none());
    assert!(g.host.runs(0), "the host runs it again");
    assert_eq!(g.host.enemies()[0].target, None, "its prey is gone");
    // The host leaves the guest instead: the guest plays on alone.
    g.guest.go_offline();
    assert!(g.guest.runs(0) && !g.guest.online());
    assert_eq!(g.guest.crabs().len(), 1);
    g.guest.tick();
}

#[test]
fn a_guest_who_rejoins_takes_the_hosts_word_for_every_enemy() {
    let mut g = game(NEAR, FakeLink::steady(LAG), |sim| {
        sim.spawn_enemy(Kind::Fixture, Vec2::new(900.0, 0.0));
    });
    play(&mut g, "", "", 60, |_| {});
    // The host's phone sleeps: the guest plays on alone, and its copy of the
    // enemy goes its own way (moved, and handed over more often than the
    // host's copy ever was).
    g.guest.go_offline();
    let e = &mut g.guest.enemies_mut()[0];
    e.pos.x += 300.0;
    e.handovers += 5;
    // The host is back: the guest goes online again.
    g.guest.go_online(false);
    let mut host_had = Vec::new();
    play(&mut g, "", "", 60, |g| {
        host_had.push(g.host.enemies()[0].pos.x)
    });
    // Where the host had it, the lag (and at most two updates' gap) ago.
    let shown = g.guest.enemies()[0].pos.x;
    let n = host_had.len();
    let recent = &host_had[n - LAG as usize - 2 * SEND_EVERY as usize..=n - LAG as usize];
    assert!(
        recent.iter().any(|x| (x - shown).abs() < 0.01),
        "the guest shows the enemy where the host had it: {shown} vs {recent:?}"
    );
}
