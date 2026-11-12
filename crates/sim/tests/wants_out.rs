//! Wants that send the crab out (`docs/design/core-loop.md`, "wants that send
//! you out"): home pieces and recipes whose one missing kind of thing is found
//! in one far place only (the bone hall, the seep garden, the clay cathedral),
//! or is the part of a creature worth a crossing (the mantises past the canyon,
//! the far morays). Each costs one good trip: a shell load of the far thing.

use bevy_math::Vec2;
use crabsim::economy::items::{BEYOND_CANYON, FAR_DROPS, drops_at, drops_of};
use crabsim::economy::wants::{Source, sources_of};
use crabsim::economy::{ECON_RECIPES, Item, NODES, NodeKind, Piece, Station, xp};
use crabsim::enemy::{KINDS, Kind, State};
use crabsim::{Carry, Level, PlayerId, Sim, Skill, Stash};

const P0: PlayerId = PlayerId(0);

/// Where the one missing thing is found.
#[derive(Clone, Copy)]
enum Where {
    /// Nodes that lie only inside this landmark.
    Landmark(&'static str),
    /// A creature's far drop, at this place.
    Far(Kind, &'static str),
}

struct Case {
    word: &'static str,
    skill: Skill,
    level: u32,
    item: Item,
    count: u32,
    at: Where,
}

const CASES: [Case; 5] = [
    Case {
        word: "shell-mosaic",
        skill: Skill::Building,
        level: 17,
        item: Item::Tooth,
        count: 4,
        at: Where::Landmark("bone hall"),
    },
    Case {
        word: "whelk-door",
        skill: Skill::Building,
        level: 20,
        item: Item::SeepMussel,
        count: 2,
        at: Where::Landmark("seep garden"),
    },
    Case {
        word: "coral-garden",
        skill: Skill::Building,
        level: 55,
        item: Item::MantisTelson,
        count: 1,
        at: Where::Far(Kind::Mantis, "clay cathedral"),
    },
    Case {
        word: "bed-3",
        skill: Skill::Building,
        level: 40,
        item: Item::GreatMantisClub,
        count: 1,
        at: Where::Far(Kind::Mantis, "beyond the canyon"),
    },
    Case {
        word: "moray-tips",
        skill: Skill::Shellcraft,
        level: 42,
        item: Item::MorayFang,
        count: 3,
        at: Where::Far(Kind::Moray, "beyond the canyon"),
    },
];

/// What the goal takes: a recipe's or a piece's.
fn needs_of(word: &str) -> &'static [(Item, u32)] {
    if let Some(p) = Piece::parse(word) {
        return p.needs();
    }
    ECON_RECIPES
        .iter()
        .find(|r| r.word == word)
        .expect("a recipe or piece")
        .needs
}

/// A crab at home with the levels, the stations and the bank of the case:
/// everything but the far thing.
fn crab_short_of(c: &Case) -> Sim {
    let mut sim = Sim::with_one_crab(Vec2::new(180.0, 0.0));
    let bank: Vec<(Item, u32)> = needs_of(c.word)
        .iter()
        .copied()
        .filter(|&(i, _)| i != c.item)
        .collect();
    let crab = sim.crab_mut(P0).expect("crab");
    crab.stash = Stash::of(&bank);
    crab.econ.skills.set(c.skill, xp::TABLE[c.level as usize]);
    crab.econ.stations.insert(Station::Grindstone);
    crab.econ.culture_beds = 2;
    sim
}

#[test]
fn each_far_want_is_listed_with_its_one_missing_thing() {
    for c in &CASES {
        let sim = crab_short_of(c);
        let wants = sim.wants(P0);
        let w = wants
            .iter()
            .find(|w| w.word == c.word)
            .unwrap_or_else(|| panic!("{} is not a want", c.word));
        assert_eq!(w.missing, vec![(c.item, c.count)], "{}", c.word);
        assert!(!w.sources.is_empty(), "{} names no source", c.word);
        assert!(!w.soon, "{}", c.word);
    }
}

#[test]
fn the_missing_thing_comes_only_from_the_target_place() {
    let sim = Sim::with_one_crab(Vec2::new(180.0, 0.0));
    for c in &CASES {
        let sources = sources_of(&sim, None, c.item);
        assert!(!sources.is_empty(), "{:?} has no source", c.item);
        for s in &sources {
            match (c.at, s) {
                (
                    Where::Landmark(name),
                    Source::Node {
                        node,
                        place: Some(p),
                        ..
                    },
                ) => {
                    assert_eq!(*p, name, "{:?} grows in {p}", node);
                }
                (Where::Far(kind, place), Source::Far { kind: k, place: p }) => {
                    assert_eq!((*k, *p), (kind, place), "{:?}", c.item);
                }
                (_, other) => panic!("{:?} also comes from {other:?}", c.item),
            }
        }
    }
}

#[test]
fn a_node_source_lies_inside_its_landmark_and_nowhere_near_home() {
    let level = Level::test_level();
    let marks = level.landmarks();
    let sim = Sim::with_one_crab(Vec2::new(180.0, 0.0));
    for c in &CASES {
        let Where::Landmark(name) = c.at else {
            continue;
        };
        let rect = marks
            .iter()
            .find(|l| l.name == name)
            .expect("landmark")
            .rect;
        let kinds: Vec<NodeKind> = NODES
            .iter()
            .filter(|n| n.yields == Some(c.item))
            .map(|n| n.kind)
            .collect();
        assert!(!kinds.is_empty(), "{:?}", c.item);
        let mut seen = 0;
        for p in sim
            .placed_nodes()
            .iter()
            .filter(|p| kinds.contains(&p.kind))
        {
            seen += 1;
            assert!(
                p.pos.x >= rect.min.x
                    && p.pos.x <= rect.max.x
                    && p.pos.y >= rect.min.y
                    && p.pos.y <= rect.max.y,
                "{:?} at {:?} lies outside the {name}",
                p.kind,
                p.pos
            );
        }
        assert!(seen > 0, "no {:?} on the map", c.item);
        // Not a drop, not made.
        for k in KINDS {
            assert!(!drops_of(k).contains(&c.item));
        }
    }
}

#[test]
fn a_far_drop_is_left_only_by_the_far_creatures() {
    let level = Level::test_level();
    for d in FAR_DROPS {
        // Never at home, never near it, and not by the usual drop.
        assert!(!drops_of(d.kind).contains(&d.item), "{:?}", d.item);
        let mut x = 0.0;
        while x < BEYOND_CANYON {
            assert!(
                !drops_at(d.kind, x).contains(&d.item),
                "{:?} at {x}",
                d.item
            );
            x += 250.0;
        }
        // Some creature of the kind stands where it is left.
        let lives_there = level
            .foes
            .iter()
            .any(|(k, at)| *k == d.kind && at.x >= d.from_x.max(BEYOND_CANYON) && at.x < d.to_x);
        assert!(
            lives_there,
            "no {:?} lives where {:?} is left",
            d.kind, d.item
        );
        assert!(drops_at(d.kind, d.from_x).contains(&d.item));
    }
    // The two mantises: the reef's gives the club, the rise's the telson.
    assert_eq!(
        drops_at(Kind::Mantis, 10_720.0),
        vec![Item::GreatMantisClub]
    );
    assert_eq!(drops_at(Kind::Mantis, 16_700.0), vec![Item::MantisTelson]);
    // The morays near home keep their teeth; the far ones shed fangs.
    assert_eq!(drops_at(Kind::Moray, 1_760.0), vec![Item::MorayTooth]);
    assert_eq!(drops_at(Kind::Moray, 9_880.0), vec![Item::MorayFang; 3]);
    assert_eq!(drops_at(Kind::Moray, 15_220.0), vec![Item::MorayFang; 3]);
}

#[test]
fn a_beaten_far_creature_leaves_its_far_part_on_the_shell() {
    for (kind, x, want) in [
        (Kind::Mantis, 10_720.0, vec![Item::GreatMantisClub]),
        (Kind::Mantis, 16_700.0, vec![Item::MantisTelson]),
        (Kind::Moray, 9_880.0, vec![Item::MorayFang; 3]),
        (Kind::Mantis, 1_000.0, vec![Item::MantisClub]),
    ] {
        let floor = Level::test_level().floor_height(x);
        let at = Vec2::new(x, floor + 40.0);
        let mut sim = Sim::with_one_crab(at);
        let id = sim.spawn_enemy(kind, at);
        let e = sim
            .enemies_mut()
            .iter_mut()
            .find(|e| e.id == id)
            .expect("spawned");
        e.state = State::Gone;
        e.deaths = 1;
        e.pos = at;
        for _ in 0..20 {
            sim.tick();
        }
        let shell = sim.crab(P0).expect("crab").mover.clinging.clone();
        for item in want {
            assert!(
                shell.contains(&Carry::Item(item)),
                "{kind:?} at {x} gave no {item:?}: {shell:?}"
            );
        }
        if x > 9000.0 {
            assert!(!shell.contains(&Carry::Item(Item::MantisClub)));
            assert!(!shell.contains(&Carry::Item(Item::MorayTooth)));
        }
    }
}

#[test]
fn every_far_want_costs_one_shell_load_of_the_far_thing() {
    for c in &CASES {
        // Four small things, or one big one: a shell load.
        assert!(!c.item.big(), "{:?}", c.item);
        assert!(c.count <= 4, "{}: {} of {:?}", c.word, c.count, c.item);
        // The rest of the want is the bank's common stock: no other far
        // thing in it.
        for &(item, _) in needs_of(c.word) {
            if item != c.item {
                assert!(
                    !FAR_DROPS.iter().any(|d| d.item == item),
                    "{}: {item:?} is a second far thing",
                    c.word
                );
                assert!(
                    sources_of(&Sim::with_one_crab(Vec2::ZERO), None, item)
                        .iter()
                        .all(|s| !matches!(s, Source::Far { .. })),
                    "{}: {item:?}",
                    c.word
                );
            }
        }
    }
    // A creature leaves a full load at most.
    for d in FAR_DROPS {
        assert!(d.count <= 4);
    }
}

#[test]
fn the_far_wants_can_be_pinned_and_say_when_the_crab_has_it_all() {
    for c in &CASES {
        let mut sim = crab_short_of(c);
        assert!(sim.pin_want(P0, c.word), "{}", c.word);
        let status = sim.pinned_status(P0).expect("pinned");
        assert!(!status.have_all);
        assert_eq!(status.want.missing, vec![(c.item, c.count)]);
        // With the thing in the bank it is ready.
        let crab = sim.crab_mut(P0).expect("crab");
        crab.stash.add(c.item, c.count);
        assert!(sim.pinned_status(P0).expect("pinned").have_all);
    }
}
