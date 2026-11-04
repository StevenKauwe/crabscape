//! Every descent is a debt repaid on the way back up (`docs/design/vision.md`,
//! `docs/design/digging.md`). A route solver plays the legacy level headless
//! from home with a fixed set of abilities and finds every place it can stand
//! or hang on, by a small set of moves (walks, jumps with any number of
//! strokes held any way, climbs and kick-offs, waits for the tide). It proves
//! the gates hold for each ability set, and measures what a round trip to
//! the deepest place costs.
//!
//! `cargo test -p crabsim --test dive_budget -- --ignored --nocapture` prints
//! the full survey (every kit, every place, the route there and back).

use bevy_math::Vec2;
use crabsim::moult::Feat;
use crabsim::{Controls, Feel, Level, PlayerId, Sim, Tuning};
use std::collections::HashMap;

const ME: PlayerId = PlayerId(0);
const DT: f64 = 1.0 / 120.0;

/// What a crab is given before a run.
#[derive(Clone, Copy, Debug, Default)]
struct Kit {
    eggs: u32,
    scissors: bool,
    glass: bool,
    crusher: bool,
    dactyls: bool,
}

impl Kit {
    const fn eggs(eggs: u32) -> Self {
        Kit {
            eggs,
            scissors: false,
            glass: false,
            crusher: false,
            dactyls: false,
        }
    }
    fn name(&self) -> String {
        let mut s = format!("{} eggs", self.eggs);
        for (on, n) in [
            (self.scissors, "scissors"),
            (self.glass, "glass"),
            (self.crusher, "crusher"),
            (self.dactyls, "dactyls"),
        ] {
            if on {
                s += " + ";
                s += n;
            }
        }
        s
    }
}

/// The key places, as boxes (x0, x1, y0, y1) on the legacy level.
const PLACES: &[(&str, [f32; 4])] = &[
    ("surface", [0.0, 1300.0, 4600.0, 6000.0]),
    ("trench floor", [1301.0, 1819.0, -300.0, -200.0]),
    ("far ledge", [1820.0, 2716.0, 0.0, 400.0]),
    ("cliff crest", [2717.0, 2800.0, 850.0, 1000.0]),
    ("basin", [2800.0, 3300.0, -720.0, 1000.0]),
    ("pit", [3300.0, 4300.0, -8500.0, -720.0]),
    ("pit floor", [3300.0, 4300.0, -8800.0, -8500.0]),
    ("alcove", [4060.0, 4300.0, -5205.0, -5080.0]),
    ("net passage", [4300.0, 4490.0, -4400.0, -4230.0]),
    ("hub", [4490.0, 5371.0, -720.0, 1500.0]),
    ("ridge crest", [4300.0, 4370.0, 2990.0, 3200.0]),
    ("doorstep", [5371.0, 5900.0, -1520.0, -1000.0]),
    ("notch", [5500.0, 5700.0, 540.0, 800.0]),
    ("canyon shelf", [5900.0, 6150.0, -3420.0, -3000.0]),
    ("canyon floor", [6150.0, 7900.0, -9420.0, -9000.0]),
    ("far-wall cave", [7700.0, 7901.0, -5210.0, -5030.0]),
    ("far rim", [7900.0, 8800.0, -1920.0, 3100.0]),
    ("east of the rim", [8800.0, 30000.0, -20000.0, 20000.0]),
];

fn place_of(p: Vec2) -> Option<&'static str> {
    PLACES
        .iter()
        .find(|(_, [x0, x1, y0, y1])| p.x >= *x0 && p.x <= *x1 && p.y >= *y0 && p.y <= *y1)
        .map(|(n, _)| *n)
}

/// Home's side of the trench, on the seabed: where a trip ends.
fn home_side(p: Vec2) -> bool {
    p.x < 1300.0 && p.y > -10.0 && p.y < 1000.0
}

/// One move, as a schedule of controls from its start.
#[derive(Clone, Copy, Debug)]
enum Move {
    /// Hold a direction this long (on and off edges, until it lands).
    Walk(f32, f32),
    /// Jump (or, on a stalk, kick off) the way `dir`, then `strokes` paddle
    /// strokes on the rhythm, holding `dir` until `hold_for` seconds, then
    /// `after` until it lands.
    Jump {
        dir: f32,
        strokes: u32,
        hold_for: f32,
        after: f32,
    },
    /// On a stalk: climb up (or down) this long.
    Climb(bool, f32),
    /// Stand still this long (the legs rest, the tide turns).
    Wait(f32),
}

impl Move {
    fn controls(&self, t: f64) -> Controls {
        let press = |at: f64| t >= at && t < at + DT * 0.99;
        match *self {
            Move::Walk(dir, secs) => Controls {
                walk: if t < f64::from(secs) { dir } else { 0.0 },
                ..Default::default()
            },
            Move::Jump {
                dir,
                strokes,
                hold_for,
                after,
            } => {
                let mut pressed = press(0.0);
                for k in 0..strokes {
                    pressed |= press(0.45 + 0.31 * f64::from(k));
                }
                Controls {
                    walk: if t < f64::from(hold_for) { dir } else { after },
                    jump_pressed: pressed,
                    jump_held: t < 0.45 + 0.31 * f64::from(strokes),
                    ..Default::default()
                }
            }
            Move::Climb(up, secs) => Controls {
                jump_held: up && t < f64::from(secs),
                down: !up && t < f64::from(secs),
                ..Default::default()
            },
            Move::Wait(_) => Controls::default(),
        }
    }
    fn active(&self) -> f64 {
        match *self {
            Move::Walk(_, s) | Move::Climb(_, s) | Move::Wait(s) => f64::from(s),
            Move::Jump { strokes, .. } => 0.45 + 0.31 * f64::from(strokes),
        }
    }
    /// What it holds once its schedule is over and the crab is still moving.
    fn trailing(&self) -> f32 {
        match *self {
            Move::Walk(dir, _) => dir,
            Move::Jump { after, .. } => after,
            _ => 0.0,
        }
    }
}

/// A crab at rest somewhere, and how it got there.
#[derive(Clone)]
struct Node {
    sim: Sim,
    parent: Option<usize>,
    how: Option<Move>,
    /// Seconds since the start.
    time: f64,
    /// The lowest the reserve has been on the way (1 = full).
    min_reserve: f32,
    /// Paddle strokes made on the way.
    strokes: u32,
    /// Seconds with tired paddle legs (less than one stroke left) while
    /// swimming on the way.
    dry: f64,
    lowest: f32,
}

impl Node {
    fn mover(&self) -> &crabsim::Mover {
        &self.sim.crab(ME).expect("the crab").mover
    }
}

fn world(kit: Kit, start: Vec2, foes: bool) -> Sim {
    let mut level = Level::test_level();
    if !foes {
        level.foes.clear();
    }
    let mut sim = Sim::new(Tuning::default(), level);
    let tuning = sim.tuning.clone();
    sim.set_feel(Feel::Current);
    sim.add_crab(ME, start);
    let crab = sim.crab_mut(ME).expect("the crab");
    // Nothing on the way changes the kit: eggs eaten, finds taken.
    crab.eaten.iter_mut().for_each(|e| *e = true);
    crab.teeth_taken.iter_mut().for_each(|e| *e = true);
    crab.sponges_taken.iter_mut().for_each(|e| *e = true);
    crab.cultures_taken.iter_mut().for_each(|e| *e = true);
    let m = &mut crab.mover;
    m.upgrades = kit.eggs;
    m.scissor_claw = kit.scissors;
    m.glass_grips = kit.glass;
    if kit.crusher {
        m.feats.push(Feat::HeavyCrusher);
    }
    if kit.dactyls {
        m.feats.push(Feat::GrippingDactyls);
    }
    m.stamina = m.strokes(&tuning) as f32;
    assert!(foes || sim.enemies().is_empty());
    sim
}

fn at_rest(m: &crabsim::Mover) -> bool {
    (m.grounded || m.climbing) && m.vel.length() < 8.0
}

/// Play `mv` from `from`; the node it comes to rest at, if it does.
fn play(from: &Node, at: usize, mv: Move) -> Option<Node> {
    let mut node = Node {
        sim: from.sim.clone(),
        parent: Some(at),
        how: Some(mv),
        time: from.time,
        min_reserve: from.min_reserve,
        strokes: from.strokes,
        dry: from.dry,
        lowest: from.lowest,
    };
    let (mut strokes, mut min_reserve, mut lowest, mut dry) =
        (node.strokes, node.min_reserve, node.lowest, node.dry);
    let sim = &mut node.sim;
    let mut t = 0.0;
    let mut still = 0;
    let limit = mv.active() + 30.0;
    while t < limit {
        let c = if t < mv.active() {
            mv.controls(t)
        } else {
            Controls {
                walk: mv.trailing(),
                ..Default::default()
            }
        };
        sim.set_controls(ME, c);
        sim.tick();
        t += DT;
        let crab = sim.crab(ME)?;
        let m = &crab.mover;
        if crab.events.paddled {
            strokes += 1;
        }
        min_reserve = min_reserve.min(m.reserve);
        lowest = lowest.min(m.pos.y);
        if !m.grounded && !m.climbing && m.stamina < 1.0 && m.upgrades > 0 {
            dry += DT;
        }
        if t >= mv.active() && at_rest(m) {
            still += 1;
            if still >= 12 {
                break;
            }
        } else {
            still = 0;
        }
    }
    if still < 12 {
        return None;
    }
    // A breath on the floor rests the paddle legs before the next move.
    if sim.crab(ME)?.mover.grounded {
        for _ in 0..120 {
            sim.set_controls(ME, Controls::default());
            sim.tick();
            t += DT;
        }
    }
    node.time += t;
    node.strokes = strokes;
    node.min_reserve = min_reserve;
    node.lowest = lowest;
    node.dry = dry;
    Some(node)
}

fn moves(m: &crabsim::Mover, strokes: u32) -> Vec<Move> {
    let mut out = Vec::new();
    if m.climbing {
        for secs in [1.0, 4.0, 40.0] {
            out.push(Move::Climb(true, secs));
            out.push(Move::Climb(false, secs));
        }
    } else {
        for dir in [-1.0, 1.0] {
            out.push(Move::Walk(dir, 0.4));
            out.push(Move::Walk(dir, 2.0));
        }
        // The tide only matters in the canyon.
        if (5900.0..7900.0).contains(&m.pos.x) {
            out.push(Move::Wait(8.0));
            out.push(Move::Wait(20.0));
        }
    }
    // Straight up first, then steer (onto a ledge overhead).
    if !m.climbing {
        for dir in [-1.0, 1.0] {
            for wait in [0.3, 0.6] {
                out.push(Move::Jump {
                    dir: 0.0,
                    strokes,
                    hold_for: wait,
                    after: dir,
                });
            }
        }
    }
    for dir in [-1.0f32, 0.0, 1.0] {
        if m.climbing && dir == 0.0 {
            continue;
        }
        for k in 0..=strokes {
            out.push(Move::Jump {
                dir,
                strokes: k,
                hold_for: 60.0,
                after: dir,
            });
            if dir != 0.0 {
                // Push off, then let the glide go: straight down.
                out.push(Move::Jump {
                    dir,
                    strokes: k,
                    hold_for: 0.45 + 0.31 * k as f32 + 0.3,
                    after: 0.0,
                });
            }
        }
    }
    out
}

fn key(m: &crabsim::Mover) -> (i32, i32, bool) {
    (
        (m.pos.x / 80.0).round() as i32,
        (m.pos.y / 80.0).round() as i32,
        m.climbing,
    )
}

struct Survey {
    nodes: Vec<Node>,
}

impl Survey {
    fn reached(&self, place: &str) -> Option<usize> {
        // The quickest arrival.
        (0..self.nodes.len())
            .filter(|&i| place_of(self.nodes[i].mover().pos) == Some(place))
            .min_by(|&a, &b| self.nodes[a].time.total_cmp(&self.nodes[b].time))
    }
    fn route(&self, mut i: usize) -> Vec<(Move, Vec2)> {
        let mut out = Vec::new();
        while let (Some(p), Some(mv)) = (self.nodes[i].parent, self.nodes[i].how) {
            out.push((mv, self.nodes[i].mover().pos));
            i = p;
        }
        out.reverse();
        out
    }
    fn describe(&self, i: usize) -> String {
        self.route(i)
            .iter()
            .map(|(mv, p)| {
                let at = place_of(*p).unwrap_or("home side");
                format!("{mv:?} -> {at} ({:.0}, {:.0})", p.x, p.y)
            })
            .collect::<Vec<_>>()
            .join("\n      ")
    }
}

/// Every place the crab can come to rest from `start`, breadth first.
fn survey_from(start: Node, kit: Kit, stop: impl Fn(&Node) -> bool, budget: usize) -> Survey {
    let mut nodes = vec![start];
    let mut seen: HashMap<(i32, i32, bool), usize> = HashMap::new();
    seen.insert(key(nodes[0].mover()), 0);
    let mut next = 0;
    while next < nodes.len() && nodes.len() < budget {
        let here = nodes[next].clone();
        if stop(&here) {
            next += 1;
            continue;
        }
        for mv in moves(here.mover(), kit.eggs) {
            if let Some(n) = play(&here, next, mv) {
                let k = key(n.mover());
                if !seen.contains_key(&k) {
                    seen.insert(k, nodes.len());
                    nodes.push(n);
                }
            }
        }
        next += 1;
    }
    Survey { nodes }
}

fn start_node(kit: Kit, at: Vec2, foes: bool) -> Node {
    Node {
        sim: world(kit, at, foes),
        parent: None,
        how: None,
        time: 0.0,
        min_reserve: 1.0,
        strokes: 0,
        dry: 0.0,
        lowest: at.y,
    }
}

const HOME: Vec2 = Vec2::new(300.0, 0.0);
const BUDGET: usize = 4000;

fn survey(kit: Kit) -> Survey {
    survey_from(start_node(kit, HOME, false), kit, |_| false, BUDGET)
}

/// The way back home from where node `i` of `s` came to rest.
fn way_home(s: &Survey, i: usize, kit: Kit) -> Option<Node> {
    let mut start = s.nodes[i].clone();
    start.parent = None;
    start.how = None;
    let back = survey_from(start, kit, |n| home_side(n.mover().pos), BUDGET);
    (0..back.nodes.len())
        .filter(|&j| home_side(back.nodes[j].mover().pos))
        .min_by(|&a, &b| back.nodes[a].time.total_cmp(&back.nodes[b].time))
        .map(|j| back.nodes[j].clone())
}

const KITS: &[Kit] = &[
    Kit::eggs(0),
    Kit::eggs(1),
    Kit::eggs(2),
    Kit::eggs(3),
    Kit {
        scissors: true,
        ..Kit::eggs(3)
    },
    Kit {
        glass: true,
        ..Kit::eggs(3)
    },
    Kit {
        crusher: true,
        ..Kit::eggs(3)
    },
    Kit {
        dactyls: true,
        ..Kit::eggs(3)
    },
    Kit {
        glass: true,
        ..Kit::eggs(0)
    },
    Kit {
        scissors: true,
        glass: true,
        dactyls: true,
        ..Kit::eggs(6)
    },
];

#[test]
#[ignore = "a survey that prints numbers; run with --ignored --nocapture"]
fn survey_every_kit() {
    let reports: Vec<String> = std::thread::scope(|scope| {
        let jobs: Vec<_> = KITS
            .iter()
            .map(|&kit| scope.spawn(move || report(kit)))
            .collect();
        jobs.into_iter()
            .map(|j| j.join().expect("a survey"))
            .collect()
    });
    for r in reports {
        println!("{r}");
    }
}

fn report(kit: Kit) -> String {
    use std::fmt::Write;
    let mut out = String::new();
    macro_rules! println {
        ($($a:tt)*) => { writeln!(out, $($a)*).expect("text") };
    }
    {
        let s = survey(kit);
        println!("\n== {} ({} resting places)", kit.name(), s.nodes.len());
        for (place, _) in PLACES {
            if let Some(i) = s.reached(place) {
                let n = &s.nodes[i];
                println!(
                    "  {place}: {:.0} s, reserve low {:.2}, {} strokes\n      {}",
                    n.time,
                    n.min_reserve,
                    n.strokes,
                    s.describe(i)
                );
            }
        }
        // The deepest resting place, and the way home from it.
        let deepest = (0..s.nodes.len())
            .min_by(|&a, &b| {
                s.nodes[a]
                    .mover()
                    .pos
                    .y
                    .total_cmp(&s.nodes[b].mover().pos.y)
            })
            .expect("somewhere");
        let d = &s.nodes[deepest];
        println!(
            "  deepest: {:?} at {} after {:.0} s",
            d.mover().pos,
            place_of(d.mover().pos).unwrap_or("home side"),
            d.time
        );
        match way_home(&s, deepest, kit) {
            Some(h) => println!(
                "  round trip: {:.0} s down, {:.0} s back; reserve low {:.2} (down {:.2}); {} strokes; {:.1} s swimming tired",
                d.time,
                h.time - d.time,
                h.min_reserve,
                d.min_reserve,
                h.strokes,
                h.dry
            ),
            None => println!("  NO WAY HOME from the deepest place"),
        }
    }
    out
}

/// The farthest east a kit's crab ever comes to rest, searching only on
/// home's side of the cliff.
fn farthest_east(kit: Kit) -> (Vec2, String) {
    let s = survey_from(
        start_node(kit, HOME, false),
        kit,
        |n| n.mover().pos.x > 2716.0,
        BUDGET,
    );
    // Down in the trench (its floor and stepping stone) is still home's side.
    let x = |i: usize| {
        let p = s.nodes[i].mover().pos;
        if p.x > 1300.0 && p.x < 1820.0 && p.y < 0.0 {
            0.0
        } else {
            p.x
        }
    };
    let i = (0..s.nodes.len())
        .max_by(|&a, &b| x(a).total_cmp(&x(b)))
        .expect("somewhere");
    (Vec2::new(x(i), s.nodes[i].mover().pos.y), s.describe(i))
}

#[test]
fn the_trench_and_the_cliff_hold_without_their_strokes() {
    // The trench needs the double jump (one egg); the cliff three strokes.
    // Not jumping, gliding off the surface kelp (#449) or anything else the
    // solver tries gets past them sooner.
    for (eggs, gate) in [(0, 1300.0), (1, 2716.0), (2, 2716.0)] {
        let (far, how) = farthest_east(Kit::eggs(eggs));
        assert!(far.x < gate, "{eggs} eggs got past x {gate}:\n      {how}");
    }
}

/// A rested crab at the pit's floor by the coral, holding jump to climb
/// straight up the coral and the kelp to the lip.
fn climb_out(kit: Kit, well_fed: bool) -> (f64, f32, Vec2) {
    let mut sim = world(kit, Vec2::new(3460.0, -8700.0), false);
    if well_fed {
        let m = &mut sim.crab_mut(ME).expect("the crab").mover;
        m.well_fed = 120 * 600;
    }
    let mut low = 1.0f32;
    for tick in 0..120 * 150 {
        let c = Controls {
            jump_pressed: tick == 0 || tick == 30,
            jump_held: true,
            ..Default::default()
        };
        sim.set_controls(ME, c);
        sim.tick();
        let m = &sim.crab(ME).expect("the crab").mover;
        low = low.min(m.reserve);
        if m.pos.y > -760.0 && !m.climbing {
            return (tick as f64 * DT, low, m.pos);
        }
    }
    (
        f64::INFINITY,
        low,
        sim.crab(ME).expect("the crab").mover.pos,
    )
}

#[test]
fn an_unprepared_climb_out_of_the_pit_runs_the_reserve_dry_but_never_strands() {
    let (secs, low, at) = climb_out(Kit::eggs(3), false);
    assert_eq!(low, 0.0, "the climb home is paid from the reserve");
    assert!(
        secs > 32.0,
        "tired near the top, it climbs slowly: {secs:.1} s"
    );
    assert!(secs < 90.0, "but it always gets out: {secs:.1} s, at {at}");
}

#[test]
fn a_prepared_climb_out_of_the_pit_is_bounded() {
    // Well fed (a clam eaten at home), or gripping dactyls: out on the
    // reserve it went down with, at full speed.
    for (kit, fed) in [
        (Kit::eggs(3), true),
        (
            Kit {
                dactyls: true,
                ..Kit::eggs(3)
            },
            false,
        ),
    ] {
        let (secs, low, _) = climb_out(kit, fed);
        assert!(low > 0.0, "{} fed {fed}: dry", kit.name());
        assert!(secs < 30.0, "{} fed {fed}: {secs:.1} s", kit.name());
    }
}
