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

/// The guest by the spawn against the fixture at 240, which the host runs.
struct GuestDuel {
    /// The guest's crab's events, tick by tick (index 0 = tick 1)...
    guest: Vec<StepEvents>,
    /// ...and what the host shows of them.
    host_saw: Vec<StepEvents>,
    /// The host's fixture's state, tick by tick.
    host_states: Vec<State>,
    health: f32,
}

fn guest_duel(script: &str) -> GuestDuel {
    let mut g = game(NEAR, FakeLink::new(LAG, 3, 0.1, 3), |sim| {
        sim.spawn_enemy(Kind::Fixture, Vec2::new(240.0, 0.0));
    });
    let (mut guest, mut host_saw, mut host_states) = (Vec::new(), Vec::new(), Vec::new());
    play(&mut g, "", script, 160, |g| {
        guest.push(g.guest.crab(GUEST).unwrap().events);
        host_saw.push(g.host.crab(GUEST).unwrap().events);
        host_states.push(g.host.enemies()[0].state);
    });
    let health = g.guest.crab(GUEST).unwrap().fighter.vitals.health;
    GuestDuel {
        guest,
        host_saw,
        host_states,
        health,
    }
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
    assert_eq!(ticks_where(&parry.guest, |e| e.parried), [h]);
    assert_eq!(parry.health, 100.0, "no damage");
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
    // The host's fixture strikes long before the guest sees it; the host
    // must not judge that blow on its copy of the guest's crab, or the host
    // would show a hit the guest may have parried.
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
