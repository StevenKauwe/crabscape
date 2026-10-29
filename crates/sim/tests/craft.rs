//! Promises about the workbench at home (the loop's "Craft" step): what a
//! crab brings home is put away there, not used up by itself; the player
//! makes things from it when they choose, through the sim (so replays carry
//! it); each recipe takes what it says and gives what it says, and nothing
//! can be made without what it needs, or away from home. Eating a clam
//! leaves its two valves, and clam-shell guards made from them soften
//! blocked blows. All of it is saved.

use bevy_math::Vec2;
use crabsim::{
    CantMake, Carry, Controls, Hit, Material, Outcome, PlayerId, Progress, RECIPES, Recipe, Sim,
    Stash,
};

const P0: PlayerId = PlayerId(0);

fn idle(sim: &mut Sim, n: u32) {
    for _ in 0..n {
        sim.tick();
    }
}

/// Walk left (home is at the left end of the shallows) until tucked in.
/// Returns the events seen on the way, by name.
fn walk_home(sim: &mut Sim) -> Vec<&'static str> {
    let mut seen = Vec::new();
    for _ in 0..1200 {
        sim.set_controls(
            P0,
            Controls {
                walk: -1.0,
                ..Controls::default()
            },
        );
        sim.tick();
        seen.extend(sim.crab(P0).unwrap().events.names());
        if sim.crab(P0).unwrap().mover.tucked_in() {
            break;
        }
    }
    sim.set_controls(P0, Controls::default());
    assert!(sim.crab(P0).unwrap().mover.tucked_in(), "got home");
    seen
}

/// A crab out on the floor a little way from home.
fn by_home() -> Sim {
    Sim::with_one_crab(Vec2::new(180.0, 0.0))
}

/// A crab tucked in at home with `stash` at its workbench.
fn at_home_with(stash: Stash) -> Sim {
    let mut sim = by_home();
    walk_home(&mut sim);
    sim.crab_mut(P0).unwrap().stash = stash;
    sim
}

/// Enough of everything to make anything once.
fn plenty() -> Stash {
    Stash {
        cultures: 3,
        teeth: 3,
        grit: 3,
        valves: 6,
        glass: 2,
    }
}

#[test]
fn what_it_carries_home_is_put_away_not_used_up() {
    for (thing, material) in [
        (Carry::Culture, Material::Culture),
        (Carry::Tooth, Material::Tooth),
        (Carry::Grit, Material::Grit),
        (Carry::Glass, Material::Glass),
    ] {
        let mut sim = by_home();
        sim.crab_mut(P0).unwrap().mover.carrying = Some(thing);
        let seen = walk_home(&mut sim);
        idle(&mut sim, 2);
        let crab = sim.crab(P0).unwrap();
        assert!(seen.contains(&"put away"), "{thing:?}: {seen:?}");
        assert_eq!(crab.mover.carrying, None, "{thing:?}");
        assert_eq!(crab.stash.count(material), 1, "{thing:?}");
        // ...and nothing was made from it by itself.
        assert_eq!(crab.mover.lantern, 0);
        assert!(!crab.mover.scissor_claw && !crab.mover.glass_grips);
        assert_eq!(crab.home_grit, 0);
    }
}

#[test]
fn a_clam_still_goes_into_the_larder_and_eating_it_leaves_two_valves() {
    let mut sim = by_home();
    sim.crab_mut(P0).unwrap().mover.carrying = Some(Carry::Clam);
    let seen = walk_home(&mut sim);
    assert!(seen.contains(&"stored"), "{seen:?}");
    let eat_after = sim.tuning.food.eat_after_ticks;
    let mut ate = false;
    for _ in 0..eat_after + 10 {
        sim.tick();
        ate |= sim.crab(P0).unwrap().events.ate_clam;
    }
    assert!(ate, "hungry at home: it eats the clam");
    let crab = sim.crab(P0).unwrap();
    assert_eq!(crab.larder, 0);
    assert_eq!(crab.stash.valves, sim.tuning.craft.valves_per_clam);
    assert_eq!(sim.tuning.craft.valves_per_clam, 2, "a clam has two valves");
}

#[test]
fn each_recipe_takes_what_it_needs_and_gives_what_it_says() {
    for def in RECIPES {
        let mut sim = at_home_with(plenty());
        let before = sim.crab(P0).unwrap().clone();
        assert_eq!(sim.make(P0, def.recipe), Ok(()), "{}", def.name);
        idle(&mut sim, 1);
        let after = sim.crab(P0).unwrap();
        // It took exactly what the recipe needs, and nothing else.
        for m in [
            Material::Culture,
            Material::Tooth,
            Material::Grit,
            Material::Valve,
            Material::Glass,
        ] {
            let needs = def
                .needs
                .iter()
                .find(|(n, _)| *n == m)
                .map_or(0, |(_, k)| *k);
            assert_eq!(
                after.stash.count(m),
                before.stash.count(m) - needs,
                "{}: {m:?}",
                def.name
            );
        }
        assert_eq!(after.events.made, Some(def.recipe), "{}", def.name);
        let (b, a) = (&before.mover, &after.mover);
        match def.recipe {
            Recipe::ScissorClaws => {
                assert!(!b.scissor_claw && a.scissor_claw);
                assert!(after.events.fitted_scissors);
            }
            Recipe::Lantern => {
                assert_eq!(a.lantern, b.lantern + 1);
                assert!(after.events.delivered);
            }
            Recipe::BuildHome => {
                assert_eq!(after.home_grit, before.home_grit + 1);
                assert!(
                    after.events.built && after.events.home_grew,
                    "the first piece"
                );
            }
            Recipe::GlassGrips => {
                assert!(!b.glass_grips && a.glass_grips);
                assert!(after.events.fitted_grips);
            }
            Recipe::ClamGuards => assert!(!b.clam_guards && a.clam_guards),
        }
    }
}

#[test]
fn nothing_is_made_without_what_it_needs() {
    for def in RECIPES {
        let mut sim = at_home_with(Stash::default());
        let before = sim.crab(P0).unwrap().clone();
        assert_eq!(
            sim.make(P0, def.recipe),
            Err(CantMake::Missing),
            "{}",
            def.name
        );
        assert_eq!(
            sim.crab(P0).unwrap(),
            &before,
            "{}: nothing changed",
            def.name
        );
    }
    // One valve short of the guards.
    let mut sim = at_home_with(Stash {
        valves: 1,
        ..Stash::default()
    });
    assert_eq!(sim.make(P0, Recipe::ClamGuards), Err(CantMake::Missing));
}

#[test]
fn gear_is_made_once_and_the_rest_again_and_again() {
    let mut sim = at_home_with(plenty());
    assert_eq!(sim.make(P0, Recipe::ScissorClaws), Ok(()));
    assert_eq!(sim.make(P0, Recipe::ScissorClaws), Err(CantMake::Made));
    assert_eq!(sim.make(P0, Recipe::ClamGuards), Ok(()));
    assert_eq!(sim.make(P0, Recipe::ClamGuards), Err(CantMake::Made));
    assert_eq!(sim.make(P0, Recipe::GlassGrips), Ok(()));
    assert_eq!(sim.make(P0, Recipe::GlassGrips), Err(CantMake::Made));
    for _ in 0..3 {
        assert_eq!(sim.make(P0, Recipe::Lantern), Ok(()));
    }
    assert_eq!(sim.make(P0, Recipe::Lantern), Err(CantMake::Missing));
    let crab = sim.crab(P0).unwrap();
    assert_eq!(crab.mover.lantern, 3);
    assert_eq!(crab.stash.teeth, 2, "a second tooth stays put away");
}

#[test]
fn only_tucked_in_at_home_is_there_a_workbench() {
    // Out in the open, with everything it needs at home.
    let mut sim = by_home();
    sim.crab_mut(P0).unwrap().stash = plenty();
    assert_eq!(sim.make(P0, Recipe::Lantern), Err(CantMake::NotAtHome));
    // Tucked into a crevice: no workbench there either.
    let c = Sim::with_one_crab(Vec2::ZERO).level.crevices[0];
    let mut sim = Sim::with_one_crab(Vec2::new(c.door.max.x - 5.0, c.den.y));
    sim.crab_mut(P0).unwrap().stash = plenty();
    for _ in 0..600 {
        sim.set_controls(
            P0,
            Controls {
                walk: -1.0,
                ..Controls::default()
            },
        );
        sim.tick();
    }
    assert!(sim.crab(P0).unwrap().mover.tucked_in());
    assert_eq!(sim.make(P0, Recipe::Lantern), Err(CantMake::NotAtHome));
    // And a friend's crab is never changed from here.
    let mut sim = at_home_with(plenty());
    sim.add_remote_crab(PlayerId(1), Vec2::new(400.0, 0.0));
    assert_eq!(
        sim.make(PlayerId(1), Recipe::Lantern),
        Err(CantMake::NotAtHome)
    );
}

#[test]
fn clam_shell_guards_halve_what_a_block_lets_through() {
    let took = |guards: bool| {
        let mut sim = by_home();
        sim.crab_mut(P0).unwrap().mover.clam_guards = guards;
        sim.set_controls(
            P0,
            Controls {
                block_held: true,
                ..Controls::default()
            },
        );
        // Past the parry window, just holding the guard up.
        idle(&mut sim, 60);
        let before = sim.crab(P0).unwrap().fighter.vitals.health;
        let hit = Hit {
            damage: 50.0,
            blockable: true,
            ..Hit::default()
        };
        let outcome = sim.hit_crab(P0, &hit);
        assert!(matches!(outcome, Outcome::Blocked(_)), "{outcome:?}");
        before - sim.crab(P0).unwrap().fighter.vitals.health
    };
    let (plain, guarded) = (took(false), took(true));
    assert!(plain > 0.0);
    assert!(
        (guarded - plain * 0.5).abs() < 1e-3,
        "with guards {guarded}, without {plain}"
    );
}

#[test]
fn a_replay_with_something_made_plays_the_same() {
    let mut live = by_home();
    let mut saved = live.progress(P0).unwrap();
    saved.stash = Stash {
        cultures: 1,
        valves: 2,
        ..Stash::default()
    };
    live.restore_progress(P0, &saved);
    live.start_recording();
    walk_home(&mut live);
    idle(&mut live, 10);
    assert_eq!(live.make(P0, Recipe::Lantern), Ok(()));
    idle(&mut live, 10);
    assert_eq!(live.make(P0, Recipe::ClamGuards), Ok(()));
    idle(&mut live, 30);

    let text = live.recording().unwrap().to_text();
    assert!(
        text.contains("craft=lantern") && text.contains("craft=guards"),
        "{text}"
    );
    let replay = crabsim::Replay::parse(&text).unwrap();
    let mut again = Sim::with_one_crab(replay.start);
    crabsim::play(&mut again, &replay, replay.end.unwrap(), |_| {});
    let crab = again.crab(P0).unwrap();
    assert_eq!(crab.mover.lantern, 1);
    assert!(crab.mover.clam_guards);
    assert_eq!(again.progress(P0), live.progress(P0));
    assert_eq!(replay.hash, Some(again.state_hash()), "{text}");
}

#[test]
fn what_was_made_and_put_away_comes_back_after_a_reload() {
    let mut sim = at_home_with(plenty());
    assert_eq!(sim.make(P0, Recipe::ClamGuards), Ok(()));
    assert_eq!(sim.make(P0, Recipe::ScissorClaws), Ok(()));
    let saved = sim.progress(P0).unwrap();
    let text = serde_json::to_string(&saved).unwrap();
    let saved: Progress = serde_json::from_str(&text).unwrap();
    assert!(saved.clam_guards && saved.scissor_claw);
    assert_eq!(saved.stash.valves, 4);

    let mut fresh = by_home();
    fresh.restore_progress(P0, &saved);
    assert_eq!(fresh.progress(P0).unwrap(), saved);
    let crab = fresh.crab(P0).unwrap();
    assert!(crab.mover.clam_guards && crab.mover.scissor_claw);
    assert_eq!(crab.stash, sim.crab(P0).unwrap().stash);
}

#[test]
fn a_save_from_before_the_workbench_kept_things_still_loads() {
    let old = r#"{"upgrades":1,"lantern":1,"scissor_claw":true,"climb_xp":0.0,"carrying":null,
        "eaten":[true,false,false,false],"cultures_taken":[false],"clues_found":[false],
        "teeth_taken":[false],"nets_cut":[false],"cast_offs":[],"larder":2,"clams":[0,0,0,0,0],
        "well_fed":0,"wakes_at":null,"grit_taken":[false,false,false,false,false,false],
        "home_grit":1}"#;
    let saved: Progress = serde_json::from_str(old).expect("an old save still reads");
    assert_eq!(saved.stash, Stash::default());
    assert!(!saved.clam_guards);
    let mut sim = by_home();
    sim.restore_progress(P0, &saved);
    let crab = sim.crab(P0).unwrap();
    assert_eq!(
        (crab.mover.lantern, crab.mover.scissor_claw, crab.home_grit),
        (1, true, 1),
        "what it made before is kept"
    );
    assert_eq!(crab.stash, Stash::default());
}
