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
            // How it moves; not what it carries. A shared clam it walks over
            // reaches its shell when the host says it's theirs, a round trip
            // later (the shared-world tests below).
            let moves = |sim: &Sim| {
                let mut m = sim.crab(GUEST).unwrap().mover.clone();
                m.clinging.clear();
                m
            };
            assert_eq!(
                moves(&g.guest),
                moves(&alone),
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
fn skills_grow_only_on_the_crabs_own_device() {
    // The guest smashes the bag the host runs: the guest's crab earns the
    // Fighting experience on the guest's device; the host only learns it from
    // the guest's updates, and the host's own crab earns nothing.
    let mut g = game(NEAR, FakeLink::steady(LAG), bag);
    play(&mut g, "", "t=0.1 smash", ticks(2.0), |_| {});
    let guest = g.guest.crab(GUEST).unwrap().mover.fight_xp;
    assert!(guest > 0.0, "the guest's blow earned Fighting");
    assert_eq!(g.host.crab(GUEST).unwrap().mover.fight_xp, guest);
    assert_eq!(g.host.crab(HOST).unwrap().mover.fight_xp, 0.0);
    assert_eq!(g.guest.crab(HOST).unwrap().mover.fight_xp, 0.0);
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
fn a_name_comes_back_whole_from_its_bytes_whatever_it_spells() {
    use crabsim::net::NAME_MAX;
    for name in [
        "Kai",
        "Infinity",
        "NaN Crab",
        "Ünïcødé 🦀",
        "sixteen chars ok",
    ] {
        assert!(name.chars().count() <= NAME_MAX);
        let msg = Msg::Name {
            crab: GUEST,
            name: name.to_string(),
        };
        assert!(msg.reliable(), "a name must arrive");
        assert_eq!(Msg::from_bytes(&msg.to_bytes()), Some(msg), "{name}");
    }
    // A name no game would send (too long, empty, a control character, a
    // space at either end): dropped.
    for name in ["seventeen chars!!", "", "a\u{7}b", " Kai", "Kai "] {
        let msg = Msg::Name {
            crab: GUEST,
            name: name.to_string(),
        };
        assert_eq!(Msg::from_bytes(&msg.to_bytes()), None, "{name:?}");
    }
    // The simulation lets a name go by: it changes nothing.
    let mut g = game(NEAR, FakeLink::steady(LAG), |_| {});
    let before = format!("{:?}", g.host.crabs());
    g.host.receive(Msg::Name {
        crab: GUEST,
        name: "Kai".into(),
    });
    assert_eq!(format!("{:?}", g.host.crabs()), before);
}

#[test]
fn names_are_cleaned_to_sit_over_a_crab() {
    use crabsim::net::clean_name;
    let clean = |s: &str| clean_name(s);
    assert_eq!(clean("  Kai  "), Some("Kai".into()));
    assert_eq!(clean("Big \t\n  Claw"), Some("Big Claw".into()));
    assert_eq!(clean("Bell\u{7}e"), Some("Belle".into()));
    assert_eq!(clean("\u{7} Kai"), Some("Kai".into()));
    assert_eq!(
        clean("abcdefghijklmnopqrstuvwxyz"),
        Some("abcdefghijklmnop".into())
    );
    // Cut at the limit on a space: no space left hanging at the end.
    assert_eq!(clean("fifteen chars x yz"), Some("fifteen chars x".into()));
    assert_eq!(
        clean("crab 🦀🦀🦀🦀🦀🦀🦀🦀🦀🦀🦀🦀"),
        Some("crab 🦀🦀🦀🦀🦀🦀🦀🦀🦀🦀🦀".into())
    );
    for nothing in ["", "   ", "\u{7}\u{8}", "\n\t"] {
        assert_eq!(clean(nothing), None, "{nothing:?}");
    }
}

#[test]
fn a_message_from_a_newer_game_is_dropped_unread() {
    // A kind of message this game doesn't know (the way a game from before
    // names sees a name): dropped, never misread as another.
    for kind in [16u8, 17, 99, 127] {
        assert_eq!(Msg::from_bytes(&[kind, 1, 2, 3, 4]), None, "{kind}");
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

// --- Shared things, and each player's own (`crabsim::shared`) --------------

use crabsim::{Carry, Controls, Mover};

/// `p` on the floor below it.
fn on_floor(sim: &Sim, p: Vec2) -> Vec2 {
    Vec2::new(p.x, sim.level.floor_height(p.x))
}

/// Put crab `id` (run on this device) standing at `at`, with what it
/// carries.
fn put(sim: &mut Sim, id: PlayerId, at: Vec2) {
    let m = &mut sim.crab_mut(id).unwrap().mover;
    *m = Mover {
        upgrades: m.upgrades,
        carrying: m.carrying,
        clinging: std::mem::take(&mut m.clinging),
        ..Mover::new(at)
    };
}

/// How many clams crab `id` has on its shell, on its own device.
fn clams_on(sim: &Sim, id: PlayerId) -> usize {
    let m = &sim.crab(id).unwrap().mover;
    m.clinging.iter().filter(|c| **c == Carry::Clam).count()
}

/// Run both devices with nobody pressing anything.
fn wait(g: &mut Game, n: u64) {
    play(g, "", "", n, |_| {});
}

#[test]
fn a_clam_one_player_takes_is_gone_for_both_and_only_the_taker_gets_it() {
    let base = world(&|_| {});
    let (bed0, bed1) = (
        on_floor(&base, base.level.clams[0]),
        on_floor(&base, base.level.clams[1]),
    );
    // The guest takes the first: it reaches the guest's shell once the host
    // says it's theirs, and is gone on both screens.
    let mut g = game(bed0, FakeLink::new(LAG, 6, 0.3, 21), |_| {});
    play(&mut g, "", "", 1, |_| {});
    assert!(
        g.guest.shared.clams[0] > 0,
        "gone on the guest's screen at once"
    );
    assert_eq!(
        clams_on(&g.guest, GUEST),
        0,
        "not the guest's until the host says so"
    );
    wait(&mut g, 3 * u64::from(LAG));
    assert_eq!(clams_on(&g.guest, GUEST), 1, "the guest has it");
    assert_eq!(clams_on(&g.host, HOST), 0, "the host doesn't");
    assert!(g.host.shared.clams[0] > 0 && g.guest.shared.clams[0] > 0);
    // The host's crab on the same bed now finds nothing.
    put(&mut g.host, HOST, bed0);
    wait(&mut g, 60);
    assert_eq!(clams_on(&g.host, HOST), 0, "nothing left for the host");
    // The host takes the second: at once on its shell, gone for the guest
    // a lag later, and never the guest's.
    put(&mut g.host, HOST, bed1);
    wait(&mut g, 1);
    assert_eq!(clams_on(&g.host, HOST), 1, "the host's at once");
    assert_eq!(g.guest.shared.clams[1], 0, "the news takes a while");
    wait(&mut g, 2 * u64::from(LAG));
    assert!(g.guest.shared.clams[1] > 0, "gone for the guest");
    put(&mut g.guest, GUEST, bed1);
    wait(&mut g, 3 * u64::from(LAG));
    assert_eq!(clams_on(&g.guest, GUEST), 1, "still only the first");
}

#[test]
fn two_players_grabbing_the_same_clam_at_once_exactly_one_gets_it() {
    let base = world(&|_| {});
    let bed = on_floor(&base, base.level.clams[0]);
    // The guest is on the bed from the start; the host's crab steps on it
    // `late` ticks later. Whoever reaches the host first wins: the host's
    // own crab, or the guest's ask once it arrives.
    let mut winners = Vec::new();
    for late in 0..3 * u64::from(LAG) {
        let mut g = game(bed, FakeLink::new(LAG, 4, 0.5, late), |_| {});
        play(&mut g, "", "", late, |_| {});
        put(&mut g.host, HOST, bed);
        wait(&mut g, 4 * u64::from(LAG));
        let got = (clams_on(&g.host, HOST), clams_on(&g.guest, GUEST));
        assert!(
            got == (1, 0) || got == (0, 1),
            "the host's crab {late} ticks late: {got:?}"
        );
        assert!(
            g.host.shared.clams[0] > 0 && g.guest.shared.clams[0] > 0,
            "gone for both"
        );
        winners.push(if got.0 == 1 { HOST } else { GUEST });
    }
    assert_eq!(winners[0], HOST, "together: the host's crab is there first");
    assert_eq!(
        *winners.last().unwrap(),
        GUEST,
        "the guest's ask got there first"
    );
}

#[test]
fn a_joining_guest_sees_the_hosts_clams_nets_and_rocks_as_they_are() {
    // The host played alone: took the first clam and the first shell grit,
    // cut the ghost net and cracked the brittle rock.
    let mut host = world(&|_| {});
    let (clam0, grit0) = (host.level.clams[0], host.level.grit[0]);
    host.add_crab(HOST, on_floor(&host, clam0));
    host.tick();
    let heap = on_floor(&host, grit0);
    put(&mut host, HOST, heap);
    host.tick();
    let net = host.level.nets[0].rect;
    let by_net = Vec2::new(
        net.min.x - host.tuning.body_half_width - 1.0,
        net.min.y + 40.0,
    );
    put(&mut host, HOST, by_net);
    host.crab_mut(HOST).unwrap().mover.scissor_claw = true;
    host.tick();
    // (A cracked rock as the heavy crusher leaves it: `heavy_crusher.rs`.)
    host.level.brittle[0].cracked = true;
    assert!(host.shared.clams[0] > 0 && host.shared.grit_taken[0] && host.level.nets[0].cut);
    host.go_online(true);
    // The guest's own world is otherwise: it took the second clam.
    let mut guest = world(&|_| {});
    guest.add_crab(GUEST, on_floor(&guest, guest.level.clams[1]));
    guest.tick();
    assert!(guest.shared.clams[1] > 0);
    put(&mut guest, GUEST, NEAR);
    guest.go_online(false);
    // The host's word, through the wire.
    let hello = Msg::from_bytes(&host.world().to_bytes()).expect("a sound message");
    guest.receive(hello);
    let shows = |sim: &Sim| {
        (
            sim.shared.clams.iter().map(|&c| c > 0).collect::<Vec<_>>(),
            sim.shared.grit_taken.clone(),
            sim.level.nets.iter().map(|n| n.cut).collect::<Vec<_>>(),
            sim.level
                .brittle
                .iter()
                .map(|b| b.cracked)
                .collect::<Vec<_>>(),
        )
    };
    assert_eq!(
        shows(&guest),
        shows(&host),
        "the guest sees the host's world"
    );
    assert_eq!(
        guest.shared.clams[1], 0,
        "its own taken clam is there in the host's world"
    );
}

#[test]
fn a_fish_egg_is_each_players_taking_it_in_a_friends_world_grants_you_the_stroke_and_leaves_theirs()
{
    let base = world(&|_| {});
    // Just under the first egg, drifting in the water.
    let egg = base.level.eggs[0] - Vec2::new(0.0, 10.0);
    let mut g = game(egg, FakeLink::new(LAG, 6, 0.3, 4), |_| {});
    wait(&mut g, 1);
    let guest = g.guest.crab(GUEST).unwrap();
    assert!(guest.eaten[0], "the guest ate it");
    assert_eq!(guest.mover.upgrades, 1, "one more swim stroke, at once");
    wait(&mut g, 2 * u64::from(LAG));
    assert_eq!(
        g.host.crab(GUEST).unwrap().mover.upgrades,
        1,
        "the host sees it"
    );
    // The host's own egg is still there for the host's crab.
    let host = g.host.crab(HOST).unwrap();
    assert!(!host.eaten[0] && host.mover.upgrades == 0);
    put(&mut g.host, HOST, egg);
    wait(&mut g, 1);
    let host = g.host.crab(HOST).unwrap();
    assert!(host.eaten[0], "the host ate its own");
    assert_eq!(host.mover.upgrades, 1);
    // And the guest's stroke lasts: back in its own world, saved.
    g.guest.go_offline();
    let saved = g.guest.progress(GUEST).unwrap();
    assert_eq!((saved.upgrades, saved.eaten[0]), (1, true));
}

#[test]
fn a_guests_own_world_after_leaving_keeps_what_they_took_and_isnt_overwritten_by_the_hosts() {
    let base = world(&|_| {});
    let bed = |i: usize| on_floor(&base, base.level.clams[i]);
    // The guest, alone, took the second clam.
    let mut guest = world(&|_| {});
    guest.add_crab(GUEST, bed(1));
    guest.tick();
    assert_eq!(clams_on(&guest, GUEST), 1);
    let own_regrow = guest.shared.clams[1];
    // The host, alone, took the fourth.
    let mut host = world(&|_| {});
    host.add_crab(HOST, bed(3));
    host.tick();
    // Together: the guest takes the first clam in the host's world.
    host.add_remote_crab(GUEST, NEAR);
    host.go_online(true);
    guest.add_remote_crab(HOST, bed(3));
    guest.go_online(false);
    guest.receive(host.world());
    put(&mut guest, GUEST, bed(0));
    let mut g = Game {
        host,
        guest,
        link: FakeLink::new(LAG, 6, 0.3, 8),
    };
    wait(&mut g, 4 * u64::from(LAG));
    assert_eq!(
        clams_on(&g.guest, GUEST),
        2,
        "the guest has its own and the host's"
    );
    assert_eq!(
        g.guest.shared.clams[1], 0,
        "the host's second clam was never taken"
    );
    // Saved while visiting: its own world, not the host's.
    let saved = g.guest.progress(GUEST).unwrap();
    let bare = |c: &[u32]| c.iter().map(|&x| x > 0).collect::<Vec<_>>();
    let mut own = vec![false; base.level.clams.len()];
    own[1] = true;
    assert_eq!(bare(&saved.clams), own, "the save is its own world");
    // The host goes; the guest plays on alone.
    g.guest.go_offline();
    assert_eq!(
        clams_on(&g.guest, GUEST),
        2,
        "what it took stays on its shell"
    );
    assert_eq!(
        bare(&g.guest.shared.clams),
        own,
        "its own clam beds, not the host's"
    );
    assert!(
        g.guest.shared.clams[1] < own_regrow,
        "its own bed kept growing back while it was away"
    );
}

#[test]
fn a_clam_bed_grows_back_on_the_hosts_clock() {
    let setup = |sim: &mut Sim| sim.tuning.food.regrow_ticks = 300;
    let base = world(&setup);
    let bed = on_floor(&base, base.level.clams[0]);
    let mut g = game(bed, FakeLink::steady(LAG), setup);
    wait(&mut g, 3 * u64::from(LAG));
    assert_eq!(clams_on(&g.guest, GUEST), 1);
    // Off the bed, so it doesn't take the next one.
    put(&mut g.guest, GUEST, NEAR);
    let (mut host_back, mut guest_back) = (None, None);
    for tick in 0..400 {
        wait(&mut g, 1);
        if host_back.is_none() && g.host.shared.clams[0] == 0 {
            host_back = Some(tick);
        }
        if guest_back.is_none() && g.guest.shared.clams[0] == 0 {
            guest_back = Some(tick);
        }
    }
    let (h, gb) = (host_back.expect("grew back"), guest_back.expect("shown"));
    assert!(
        gb > h && gb <= h + u64::from(LAG) + 2,
        "host {h}, guest {gb}"
    );
}

#[test]
fn under_any_lag_and_loss_each_shared_thing_goes_to_at_most_one_crab() {
    // Two crabs wander over the shallows' two clam beds and heap of grit,
    // starting on the same bed, over many connections: none grows back, and
    // their shells hold plenty.
    let setup = |sim: &mut Sim| {
        sim.tuning.food.regrow_ticks = u32::MAX / 2;
        sim.tuning.cling = 64;
    };
    let base = world(&setup);
    let start = on_floor(&base, base.level.clams[0]);
    let mut rng = crabsim::rng::Rng::new(99);
    let mut got = [0; 2];
    for run in 0..32 {
        let loss = rng.range(0, 90) as f32 / 100.0;
        let link = FakeLink::new(rng.range(0, 40), rng.range(0, 20), loss, run);
        let mut g = game(start, link, setup);
        put(&mut g.host, HOST, start);
        let mut walk = [0.0f32; 2];
        for _ in 0..480 {
            for (i, (sim, id)) in [(&mut g.host, HOST), (&mut g.guest, GUEST)]
                .into_iter()
                .enumerate()
            {
                let x = sim.crab(id).unwrap().mover.pos.x;
                if rng.range(0, 40) == 0 {
                    walk[i] = [-1.0, 0.0, 1.0][rng.range(0, 2) as usize];
                }
                if x < 220.0 {
                    walk[i] = 1.0;
                } else if x > 540.0 {
                    walk[i] = -1.0;
                }
                sim.set_controls(
                    id,
                    Controls {
                        walk: walk[i],
                        ..Controls::default()
                    },
                );
            }
            g.host.tick();
            g.guest.tick();
            g.link.carry(&mut [&mut g.host, &mut g.guest]);
        }
        wait(&mut g, 4 * 40 + 20);
        let held = |kind: Carry| {
            [(&g.host, HOST), (&g.guest, GUEST)]
                .iter()
                .map(|(sim, id)| {
                    sim.crab(*id)
                        .unwrap()
                        .mover
                        .clinging
                        .iter()
                        .filter(|c| **c == kind)
                        .count()
                })
                .sum::<usize>()
        };
        let clams_gone = g.host.shared.clams.iter().filter(|&&c| c > 0).count();
        let grit_gone = g.host.shared.grit_taken.iter().filter(|&&t| t).count();
        assert_eq!(
            held(Carry::Clam),
            clams_gone,
            "run {run}: each clam taken is on exactly one shell"
        );
        assert_eq!(held(Carry::Grit), grit_gone, "run {run}: the same for grit");
        assert!(
            clams_gone <= 2 && grit_gone <= 1,
            "run {run}: only the shallows' things"
        );
        got[0] += clams_on(&g.host, HOST);
        got[1] += clams_on(&g.guest, GUEST);
        let gone = |sim: &Sim| {
            let s = &sim.shared;
            (
                s.clams.iter().map(|&c| c > 0).collect::<Vec<_>>(),
                s.grit_taken.clone(),
            )
        };
        assert_eq!(
            gone(&g.guest),
            gone(&g.host),
            "run {run}: both see the same world"
        );
    }
    eprintln!("clams won (host, guest): {got:?}");
    assert!(got[0] > 0 && got[1] > 0, "both crabs won some: {got:?}");
}

#[test]
fn a_guest_typing_monkeys_has_the_host_drop_them_in_for_both_to_see() {
    let mut g = game(NEAR, FakeLink::steady(LAG), |_| {});
    assert!(g.guest.enter_typed(GUEST, "monkeys"));
    assert!(g.guest.enemies().is_empty(), "the host runs the creatures");
    play(&mut g, "t=0 walk=stop", "t=0 walk=stop", ticks(3.0), |_| {});
    let list = |sim: &Sim| {
        sim.enemies()
            .iter()
            .map(|e| (e.id, e.kind))
            .collect::<Vec<_>>()
    };
    assert_eq!(g.host.enemies().len(), 3);
    assert!(g.host.enemies().iter().all(|e| e.kind == Kind::Monkey));
    assert_eq!(
        list(&g.guest),
        list(&g.host),
        "the guest sees the same ones"
    );
    assert!(
        g.guest
            .enemies()
            .iter()
            .all(|e| (e.pos.x - NEAR.x).abs() < 400.0)
    );
}

#[test]
fn a_host_typing_monkeys_shows_them_to_the_guest_too() {
    let mut g = game(NEAR, FakeLink::steady(LAG), |_| {});
    assert!(g.host.enter_typed(HOST, "monkeys"));
    play(&mut g, "t=0 walk=stop", "t=0 walk=stop", ticks(2.0), |_| {});
    assert_eq!(g.guest.enemies().len(), 3);
}

#[test]
fn the_pizza_look_reaches_a_friends_screen() {
    let mut g = game(NEAR, FakeLink::steady(LAG), |_| {});
    assert!(g.guest.enter_typed(GUEST, "pizza"));
    play(&mut g, "t=0 walk=stop", "t=0 walk=stop", ticks(1.0), |_| {});
    assert!(g.host.crab(GUEST).unwrap().mover.pizza);
    assert!(g.guest.enter_typed(GUEST, "pizza"));
    play(&mut g, "t=0 walk=stop", "t=0 walk=stop", ticks(1.0), |_| {});
    assert!(!g.host.crab(GUEST).unwrap().mover.pizza);
}

/// The guest's crab, on the guest's own device, grown by `moults` and whole.
fn grow_guest(g: &mut Game, moults: u32) {
    let t = g.guest.tuning.clone();
    let crab = g.guest.crab_mut(GUEST).unwrap();
    crab.mover.moults = moults;
    crab.fighter.vitals.health = crab.mover.max_shell(&t);
}

#[test]
fn a_grown_crab_is_its_size_on_a_friends_screen_and_hits_as_hard_there() {
    const SMASH: &str = "t=0.1 smash";
    // Solo, the same grown crab's smash on the bag, and an ungrown one's.
    let smash_alone = |moults: u32| {
        let mut alone = solo(NEAR, bag);
        alone.crab_mut(GUEST).unwrap().mover.moults = moults;
        let script = Replay::parse(SMASH).unwrap();
        let mut playback = script.playback();
        for _ in 0..ticks(1.5) {
            alone.set_controls(GUEST, playback.next_controls());
            alone.tick();
        }
        alone.enemies()[0].vitals.health
    };
    let (plain, grown) = (smash_alone(0), smash_alone(3));
    assert!(
        grown < plain,
        "a grown crab hits harder: {grown} vs {plain} left"
    );
    // In co-op, grown only on the guest's own device: the host learns its
    // size from the guest's updates, and the blow lands as hard there.
    let mut g = game(NEAR, FakeLink::new(LAG, 6, 0.3, 11), bag);
    grow_guest(&mut g, 3);
    play(&mut g, "", SMASH, ticks(1.5), |_| {});
    let t = g.host.tuning.clone();
    let size = |sim: &Sim| sim.crab(GUEST).unwrap().mover.grown(&t);
    assert!(size(&g.guest) > 1.0);
    assert_eq!(size(&g.host), size(&g.guest), "the host shows it grown");
    assert_eq!(g.host.enemies()[0].vitals.health, grown, "as hard as solo");
}

#[test]
fn the_defender_decides_a_blow_on_a_grown_crab_against_its_bigger_shell() {
    let mut g = game(NEAR, FakeLink::steady(LAG), |sim| {
        sim.spawn_enemy(Kind::Fixture, Vec2::new(240.0, 0.0));
    });
    grow_guest(&mut g, 3);
    let t = g.guest.tuning.clone();
    let whole = g.guest.crab(GUEST).unwrap().mover.max_shell(&t);
    assert!(whole > t.crab.health);
    let mut hurt_at = None;
    play(&mut g, "", "", 160, |g| {
        let crab = g.guest.crab(GUEST).unwrap();
        if crab.events.hurt && hurt_at.is_none() {
            hurt_at = Some(crab.fighter.vitals.health);
        }
    });
    let after = hurt_at.expect("the fixture's blow lands");
    assert_eq!(
        after,
        whole - t.fixture.strike.damage,
        "off the bigger shell"
    );
    wait(&mut g, u64::from(LAG) + 2 * SEND_EVERY);
    let host_view = g.host.crab(GUEST).unwrap();
    assert_eq!(
        host_view.fighter.vitals.health,
        g.guest.crab(GUEST).unwrap().fighter.vitals.health
    );
    assert_eq!(
        host_view.mover.max_shell(&t),
        whole,
        "and the host shows that shell"
    );
}
