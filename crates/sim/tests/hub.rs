//! The far basin as a hub (`docs/design/world.md`, "The crossroads"): every
//! branch out of it ends in a find that makes the way home cheaper, and the
//! hub never softlocks. With exactly the abilities a gate asks for it opens;
//! without them it holds; nothing a gate needs lies behind that gate; and
//! from every new place a crab gets back out on free routes (walking,
//! dropping, coral and kelp). Played headless with input scripts.

use bevy_math::Vec2;
use crabsim::replay::ticks;
use crabsim::{Carry, Controls, CutFrom, Feel, Level, PlayerId, Replay, Sim, Tuning};

const ME: PlayerId = PlayerId(0);

/// What a crab is given before a run.
#[derive(Clone, Copy, Default)]
struct Kit {
    strokes: u32,
    glass: bool,
    scissors: bool,
}

const FREE: Kit = Kit {
    strokes: 0,
    glass: false,
    scissors: false,
};

struct Run {
    sim: Sim,
    stood: Vec<Vec2>,
    path: Vec<Vec2>,
    highest: f32,
}

impl Run {
    fn crab(&self) -> &crabsim::Mover {
        &self.sim.crab(ME).unwrap().mover
    }

    fn stood_on(&self, (left, right, y): (f32, f32, f32)) -> bool {
        self.stood
            .iter()
            .any(|p| (p.y - y).abs() < 1.0 && p.x > left && p.x < right)
    }
}

fn run_in(sim: Sim, script: &str, seconds: f64, kit: Kit) -> Run {
    let replay = Replay::parse(script).expect("the script reads");
    let mut sim = sim;
    sim.add_crab(ME, replay.start);
    let m = &mut sim.crab_mut(ME).unwrap().mover;
    m.upgrades = kit.strokes;
    m.stamina = kit.strokes as f32;
    m.glass_grips = kit.glass;
    m.scissor_claw = kit.scissors;
    let mut run = Run {
        highest: replay.start.y,
        sim: sim.clone(),
        stood: Vec::new(),
        path: Vec::new(),
    };
    let mut playback = replay.playback();
    for _ in 0..ticks(seconds) {
        sim.set_controls(ME, playback.next_controls());
        sim.tick();
        let crab = sim.crab(ME).unwrap();
        run.highest = run.highest.max(crab.mover.pos.y);
        run.path.push(crab.mover.pos);
        if crab.mover.grounded {
            run.stood.push(crab.mover.pos);
        }
    }
    run.sim = sim;
    run
}

fn world() -> Sim {
    let mut sim = Sim::new(Tuning::default(), Level::test_level());
    sim.set_feel(Feel::Current);
    sim
}

fn run(script: &str, seconds: f64, kit: Kit) -> Run {
    run_in(world(), script, seconds, kit)
}

/// A jump and `strokes` strokes, pressed on the rhythm, from `at` s.
fn swim(at: f64, strokes: u32) -> String {
    let mut s = format!("; t={at} jump");
    for k in 0..strokes {
        s += &format!("; t={} jump", at + 0.45 + 0.31 * f64::from(k));
    }
    s
}

// The hub's places, from the level.
/// The far basin's floor, from the ridge to the crack.
const HUB: (f32, f32, f32) = (4370.0, 5200.0, -700.0);
const HUB_EAST: (f32, f32, f32) = (5200.0, 5371.0, -560.0);
/// The ridge between the pit and the hub, and its crest.
const RIDGE: (f32, f32) = (4300.0, 4370.0);
const CREST: (f32, f32, f32) = (4300.0, 4370.0, 3000.0);
/// The cliff's front foot: west of it is the way home.
const CLIFF: f32 = 2717.0;
const WINDOW: usize = 0;

fn window_net() -> crabsim::level::Net {
    Level::test_level().nets[WINDOW]
}

/// Did a run ever get past the ridge (into the shaft, the hub or beyond)?
fn got_past_the_ridge(r: &Run) -> Option<Vec2> {
    r.path.iter().copied().find(|p| p.x > RIDGE.1 + 10.0)
}

/// Off the cliff's crest (walking off east) or off the shallow kelp at the
/// surface, with `strokes` strokes pressed on the way.
fn glides(strokes: u32) -> [String; 2] {
    [
        format!("start x=2780 y=860\nt=0 walk=right{}", swim(0.05, strokes)),
        format!("start x=950 y=4800\nt=0 walk=right{}", swim(0.1, strokes)),
    ]
}

#[test]
fn the_window_is_a_ghost_net_in_frame_rock_cut_from_either_side() {
    let level = Level::test_level();
    let net = window_net();
    // The pit is filled: the basin floor runs straight to the window, so
    // the net is the scissor claws' gate and its knots reach from both sides.
    assert_eq!(net.from, CutFrom::Either);
    assert!(net.cuttable_from(net.rect.min.x - 50.0) && net.cuttable_from(net.rect.max.x + 50.0));
    assert!(net.rect.min.x >= RIDGE.0 && net.rect.max.x <= RIDGE.1);
    // Rock under its sill and over its roof, all the way up to the crest.
    let rock = |x: f32, y: f32| level.rocks.iter().any(|r| r.contains(Vec2::new(x, y)));
    for x in [RIDGE.0 + 1.0, RIDGE.1 - 1.0] {
        assert!(rock(x, net.rect.min.y - 10.0) && rock(x, -4000.0));
        assert!(rock(x, net.rect.max.y + 10.0) && rock(x, CREST.2 - 10.0));
        assert!(!rock(x, net.rect.center().y), "the window is open rock");
    }
}

#[test]
fn nothing_gets_into_the_hub_without_scissors_until_the_window_is_cut() {
    for strokes in [0, 1, 3, 5] {
        for script in glides(strokes) {
            let r = run(&script, 90.0, Kit { strokes, ..FREE });
            assert_eq!(got_past_the_ridge(&r), None, "{script}");
            assert!(!r.sim.level.nets[WINDOW].cut, "no cutting it without claws");
            // Off the crest it ends up on the basin floor before the ridge;
            // off the surface kelp the sea arch stops it short of the
            // trench's far side (#449).
            let end = r.crab().pos;
            if script.starts_with("start x=950") {
                assert!(end.x < 1820.0, "not across the trench: {end}");
            } else {
                assert!(
                    end.x > 2800.0 && end.x < window_net().rect.min.x,
                    "in the basin: {end}"
                );
            }
        }
    }
    // Walking east along the basin floor stops at the net.
    let r = run("start x=4000 y=-700\nt=0 walk=right", 20.0, FREE);
    assert_eq!(got_past_the_ridge(&r), None);
}

#[test]
fn a_crab_with_scissors_cuts_the_window_from_either_side() {
    // From the basin (walking east along the flat floor)...
    let from_basin = "start x=4200 y=-700\nt=0 walk=right";
    let r = run(
        from_basin,
        5.0,
        Kit {
            scissors: true,
            ..FREE
        },
    );
    assert!(r.sim.level.nets[WINDOW].cut, "cut from the basin side");
    assert!(got_past_the_ridge(&r).is_some(), "and through it");
    let r = run(from_basin, 5.0, FREE);
    assert!(!r.sim.level.nets[WINDOW].cut, "not without scissor claws");
    // ...and from the hub (walking west).
    let off = "start x=4520 y=-700\nt=0 walk=left";
    let r = run(
        off,
        3.0,
        Kit {
            scissors: true,
            ..FREE
        },
    );
    assert!(r.sim.level.nets[WINDOW].cut, "cut from the hub side");
    let r = run(off, 3.0, FREE);
    assert!(!r.sim.level.nets[WINDOW].cut, "not without scissor claws");
}

#[test]
fn once_cut_the_basin_floor_and_the_crests_glide_both_lead_into_the_hub() {
    let mut sim = world();
    sim.level.nets[WINDOW].cut = true;
    // Walking along the floor, no scissors needed now.
    let r = run_in(
        sim.clone(),
        "start x=4000 y=-700\nt=0 walk=right",
        20.0,
        FREE,
    );
    assert!(r.stood_on(HUB), "on the hub's floor: {}", r.crab().pos);
    // A jump and a stroke off the crest also carry a crab over the basin,
    // through the window and onto the hub's floor.
    let r = run_in(sim, &glides(1)[0], 30.0, Kit { strokes: 1, ..FREE });
    assert!(r.stood_on(HUB), "on the hub's floor: {}", r.crab().pos);
}

#[test]
fn glass_grips_climb_the_ridge_and_its_crest_is_a_glide_home() {
    // From the top of the shaft's coral, up the bare rock to the crest.
    let up = "start x=4420 y=-700\nt=0 jump; t=0.2 release; t=0.3 jump walk=left";
    let r = run(
        up,
        60.0,
        Kit {
            glass: true,
            ..FREE
        },
    );
    assert!(
        r.highest >= CREST.2,
        "up over the crest: highest {}",
        r.highest
    );
    // Holding left, off it and home: it lands past the cliff, free.
    let home = r.stood.iter().find(|p| p.x < CLIFF);
    assert!(home.is_some(), "landed past the cliff: {}", r.crab().pos);
    assert_eq!(r.crab().stamina, 0.0, "no strokes spent (it had none)");
    // Without glass grips, nothing gets a crab near the crest from the hub.
    let r = run(up, 30.0, FREE);
    assert!(r.highest < 0.0, "highest {}", r.highest);
    for strokes in [3, 5] {
        let r = run(
            &format!("start x=4520 y=-700\nt=0 walk=left{}", swim(0.1, strokes)),
            10.0,
            Kit { strokes, ..FREE },
        );
        assert!(r.highest < CREST.2 - 1500.0, "highest {}", r.highest);
    }
}

#[test]
fn the_flood_runs_up_the_crack_to_the_hub_and_the_coral_still_climbs_it_on_the_ebb() {
    // The tide runs up for its first 24 s: jump in at the chimney's foot,
    // and it carries a crab up to the hub, no strokes, no climbing.
    for at in [0.5, 3.0, 8.0, 15.0] {
        let ride = format!(
            "start x=5440 y=-1500\nt={at} jump; t={} release; t={} walk=left",
            at + 0.2,
            at + 3.0
        );
        let r = run(&ride, at + 8.0, FREE);
        assert!(
            r.stood_on(HUB_EAST) || r.stood_on(HUB),
            "{at}: {}",
            r.crab().pos
        );
    }
    // On the ebb (from 30 s) the water runs down it, and the coral is the
    // free way up, as it always was.
    let climb = "start x=5440 y=-1500\nt=34 jump; t=34.2 release; t=34.3 jump walk=left";
    let r = run(climb, 50.0, FREE);
    assert!(r.sim.level.tide < 0.0, "the ebb runs");
    assert!(r.stood_on(HUB_EAST) || r.stood_on(HUB), "{}", r.crab().pos);
}

#[test]
fn a_glass_sponge_lies_buried_in_the_hubs_sand_and_digging_turns_it_up() {
    let level = Level::test_level();
    // Found by place: later regions append their own sponges.
    let at = *level
        .sponges
        .iter()
        .find(|p| p.x > HUB.0 && p.x < HUB.1)
        .expect("a sponge in the hub");
    assert!(at.y < HUB.2 - 50.0, "buried, not lying on the floor");
    // In the material world, a crab with nothing but its scoop digs down
    // to it: no glass grips, no canyon.
    let mut sim = world();
    sim.add_crab(ME, Vec2::new(at.x, HUB.2));
    sim.enable_underwater_cells();
    for _ in 0..ticks(8.0) {
        sim.set_controls(
            ME,
            Controls {
                down: true,
                dig_held: true,
                ..Default::default()
            },
        );
        sim.tick();
    }
    let crab = sim.crab(ME).unwrap();
    assert_eq!(
        crab.mover.carrying,
        Some(Carry::Glass),
        "at {}",
        crab.mover.pos
    );
    let index = sim
        .level
        .sponges
        .iter()
        .position(|p| *p == at)
        .expect("the hub's sponge");
    assert_eq!(crab.sponges_taken.get(index), Some(&true));
    // Walking over the spot does not find it.
    let r = run(
        &format!("start x={} y={}\nt=0 walk=right", at.x - 100.0, HUB.2),
        2.0,
        FREE,
    );
    assert_eq!(r.sim.crab(ME).unwrap().sponges_taken.last(), Some(&false));
}

#[test]
fn nothing_a_gate_needs_lies_behind_that_gate() {
    let level = Level::test_level();
    // Scissor claws (the window's and the passage's nets) come from the
    // tooth in the pit, west of both.
    assert!(level.teeth[0].x < RIDGE.0 && level.teeth[0].x < level.nets[0].rect.min.x);
    // Glass grips (the ridge and the notch) come from sponges lying low:
    // the canyon's floor and the hub's sand, never up a bare face.
    for at in &level.sponges[..1] {
        assert!(at.y <= level.floor_height(at.x) + 10.0, "{at}");
    }
    let buried = *level.sponges.last().unwrap();
    assert!(buried.y < level.floor_height(buried.x));
    // The window, once cut, needs nothing: the glide off the cliff (above)
    // takes one stroke, which the second egg gives.
}
