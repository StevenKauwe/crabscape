//! "Pick a want" and the shared home: the wants list (what one more kind of
//! thing would let the crab make, and where it comes from, from memory only),
//! the pinned goal (saved, replayed, one-shot "you have what you need"), and
//! the home pieces built into the shared home (solo, host, guest through the
//! host, saved with the world).

use std::collections::BTreeSet;

use bevy_math::Vec2;
use crabsim::craft::CantMakeEcon;
use crabsim::economy::home::{PIECES, Piece, piece_xp};
use crabsim::economy::items::{drops_of, far_drops_of};
use crabsim::economy::recipes::Bench;
use crabsim::economy::wants::Source;
use crabsim::economy::{ECON_RECIPES, Item, NODES, NodeKind, Output, Station, ToolKind, xp};
use crabsim::enemy::KINDS;
use crabsim::net::FakeLink;
use crabsim::{Carry, Controls, Level, PlayerId, Sim, Skill, Stash, Tuning};

const P0: PlayerId = PlayerId(0);
const HOST: PlayerId = PlayerId(0);
const GUEST: PlayerId = PlayerId(1);

fn idle(sim: &mut Sim, n: u32) {
    for _ in 0..n {
        sim.tick();
    }
}

fn set_level(sim: &mut Sim, id: PlayerId, skill: Skill, level: u32) {
    sim.crab_mut(id)
        .unwrap()
        .econ
        .skills
        .set(skill, xp::TABLE[level as usize]);
}

fn give(sim: &mut Sim, id: PlayerId, items: &[(Item, u32)]) {
    sim.crab_mut(id).unwrap().stash = Stash::of(items);
}

/// Walk crab `id` home until it is tucked in.
fn go_home(sim: &mut Sim) {
    for _ in 0..1500 {
        sim.set_controls(
            P0,
            Controls {
                walk: -1.0,
                ..Controls::default()
            },
        );
        sim.tick();
        if sim.crab(P0).unwrap().mover.tucked_in() {
            break;
        }
    }
    sim.set_controls(P0, Controls::default());
    assert!(sim.crab(P0).unwrap().mover.tucked_in(), "got home");
}

fn solo_at_home() -> Sim {
    let mut sim = Sim::with_one_crab(Vec2::new(180.0, 0.0));
    go_home(&mut sim);
    sim
}

fn building_xp(sim: &Sim, id: PlayerId) -> u32 {
    sim.crab(id).unwrap().econ.skills.get(Skill::Building)
}

// ---- Wants ----

#[test]
fn wants_list_exactly_the_recipes_short_by_one_kind() {
    let mut sim = Sim::with_one_crab(Vec2::new(180.0, 0.0));
    // A bare crab: kelp cord (2 kelp) is one kind short.
    let wants = sim.wants(P0);
    let cord = wants
        .iter()
        .find(|w| w.word == "kelp-cord")
        .expect("kelp cord is a want");
    assert_eq!(cord.missing, vec![(Item::Kelp, 2)]);
    assert!(!cord.soon);
    for w in &wants {
        assert_eq!(w.missing.len(), 1, "{}: {:?}", w.word, w.missing);
        assert!(!w.sources.is_empty() || w.needs.iter().all(|n| n.0.tier() == 0));
    }
    // Everything short by two kinds is left out.
    let two_kinds = ECON_RECIPES
        .iter()
        .filter(|r| r.level == 1 && matches!(r.at, Bench::Workbench) && r.needs.len() >= 2)
        .collect::<Vec<_>>();
    for r in two_kinds {
        assert!(
            wants.iter().all(|w| w.word != r.word),
            "{} is two kinds short",
            r.word
        );
    }
    // One kelp in the bank: still short by one kind, by one kelp.
    give(&mut sim, P0, &[(Item::Kelp, 1)]);
    let cord = sim
        .wants(P0)
        .into_iter()
        .find(|w| w.word == "kelp-cord")
        .unwrap();
    assert_eq!(cord.missing, vec![(Item::Kelp, 1)]);
    // Two on the bank: nothing missing, so it is not a want but a craft.
    give(&mut sim, P0, &[(Item::Kelp, 2)]);
    assert!(sim.wants(P0).iter().all(|w| w.word != "kelp-cord"));
}

#[test]
fn the_shell_counts_toward_a_want() {
    let mut sim = Sim::with_one_crab(Vec2::new(180.0, 0.0));
    sim.crab_mut(P0)
        .unwrap()
        .mover
        .clinging
        .push(Carry::Item(Item::Kelp));
    let cord = sim
        .wants(P0)
        .into_iter()
        .find(|w| w.word == "kelp-cord")
        .unwrap();
    assert_eq!(cord.missing, vec![(Item::Kelp, 1)]);
    give(&mut sim, P0, &[(Item::Kelp, 1)]);
    assert!(sim.wants(P0).iter().all(|w| w.word != "kelp-cord"));
}

#[test]
fn recipes_a_few_levels_away_are_soon_and_later_ones_are_hidden() {
    // A workbench recipe of Building whose inputs are all in the bank but one.
    let r = ECON_RECIPES
        .iter()
        .find(|r| {
            r.level >= 8
                && r.needs.len() >= 2
                && matches!(r.at, Bench::Workbench)
                && matches!(r.makes, Output::Items(_))
        })
        .expect("a mid-level building recipe");
    let mut sim = Sim::with_one_crab(Vec2::new(180.0, 0.0));
    let all_but_one: Vec<_> = r.needs[1..].to_vec();
    give(&mut sim, P0, &all_but_one);
    let listed = |sim: &Sim| sim.wants(P0).into_iter().find(|w| w.word == r.word);
    set_level(&mut sim, P0, r.skill, r.level - 2);
    assert!(listed(&sim).expect("soon").soon);
    set_level(&mut sim, P0, r.skill, r.level);
    assert!(!listed(&sim).expect("open").soon);
    set_level(&mut sim, P0, r.skill, r.level.saturating_sub(5).max(1));
    assert!(listed(&sim).is_none(), "five levels away stays hidden");
}

#[test]
fn wants_come_fewest_missing_then_highest_level_first() {
    let mut sim = Sim::with_one_crab(Vec2::new(180.0, 0.0));
    for skill in Skill::ECONOMY {
        set_level(&mut sim, P0, skill, 40);
    }
    let wants = sim.wants(P0);
    assert!(wants.len() > 3);
    let key = |w: &crabsim::economy::wants::Want| {
        (
            w.soon,
            w.missing.iter().map(|m| m.1).sum::<u32>(),
            std::cmp::Reverse(w.level),
        )
    };
    for pair in wants.windows(2) {
        assert!(key(&pair[0]) <= key(&pair[1]), "{pair:?}");
    }
}

#[test]
fn sources_come_from_the_data_and_places_only_from_memory() {
    let mut sim = Sim::with_one_crab(Vec2::new(180.0, 0.0));
    let cord = |sim: &Sim| {
        sim.wants(P0)
            .into_iter()
            .find(|w| w.word == "kelp-cord")
            .unwrap()
    };
    // Never seen a kelp stand: the source is named, with no place.
    let w = cord(&sim);
    let stand = w
        .sources
        .iter()
        .find_map(|s| match s {
            Source::Node {
                node,
                remembered_at,
                ..
            } if NODES
                .iter()
                .any(|n| n.kind == *node && n.yields == Some(Item::Kelp)) =>
            {
                Some(*remembered_at)
            }
            _ => None,
        })
        .expect("a node source for kelp");
    assert_eq!(stand, None, "an unseen node has no place");
    // A creature drop names the creature; a made item names the recipe.
    give(&mut sim, P0, &[]);
    let cord_item = sim
        .wants(P0)
        .into_iter()
        .find(|w| w.missing.first().is_some_and(|m| m.0 == Item::KelpCord));
    if let Some(w) = cord_item {
        assert!(w.sources.contains(&Source::Recipe("kelp-cord")));
    }
    // Stand beside a kelp stand a while: now it is remembered, at its place.
    let i = sim
        .placed_nodes()
        .iter()
        .position(|n| n.kind == NodeKind::KelpStand)
        .unwrap();
    let at = sim.placed_nodes()[i].pos;
    {
        let c = sim.crab_mut(P0).unwrap();
        c.mover.pos = at;
        c.mover.vel = Vec2::ZERO;
    }
    idle(&mut sim, 120);
    let remembered = cord(&sim)
        .sources
        .iter()
        .find_map(|s| match s {
            Source::Node {
                node: NodeKind::KelpStand,
                remembered_at,
                ..
            } => *remembered_at,
            _ => None,
        })
        .expect("remembered after seeing it");
    assert!(
        sim.placed_nodes()
            .iter()
            .any(|p| p.kind == NodeKind::KelpStand && p.pos == remembered)
    );
}

#[test]
fn creature_drops_name_their_creatures() {
    let sim = Sim::with_one_crab(Vec2::new(180.0, 0.0));
    let sources = crabsim::economy::wants::sources_of(&sim, None, Item::SquidBeak);
    let squid: Vec<_> = KINDS
        .into_iter()
        .filter(|k| drops_of(*k).contains(&Item::SquidBeak))
        .collect();
    assert!(!squid.is_empty());
    for k in squid {
        assert!(sources.contains(&Source::Creature(k)));
    }
}

#[test]
fn the_next_unlocks_show_only_what_is_within_a_tenth() {
    let mut sim = Sim::with_one_crab(Vec2::new(180.0, 0.0));
    assert!(sim.next_unlocks(P0).is_empty());
    // Prying 14 and 95% of the way to 15 ("you can open clams").
    let (a, b) = (xp::TABLE[14], xp::TABLE[15]);
    let near = a + ((b - a) as f32 * 0.95) as u32;
    sim.crab_mut(P0)
        .unwrap()
        .econ
        .skills
        .set(Skill::Prying, near);
    let ups = sim.next_unlocks(P0);
    assert!(
        ups.iter()
            .any(|(s, l, lines)| *s == Skill::Prying && *l == 15 && !lines.is_empty())
    );
    assert!(ups.len() <= 3);
    // Halfway: nothing.
    let half = a + (b - a) / 2;
    sim.crab_mut(P0)
        .unwrap()
        .econ
        .skills
        .set(Skill::Prying, half);
    assert!(sim.next_unlocks(P0).iter().all(|u| u.0 != Skill::Prying));
}

#[test]
fn a_level_that_makes_a_recipe_a_want_names_the_new_task() {
    let mut sim = Sim::with_one_crab(Vec2::new(180.0, 0.0));
    let r = ECON_RECIPES
        .iter()
        .find(|r| r.skill == Skill::Weaving && r.level > 1 && matches!(r.at, Bench::Workbench))
        .expect("a later weaving recipe");
    let rest: Vec<_> = r.needs[1..].to_vec();
    give(&mut sim, P0, &rest);
    set_level(&mut sim, P0, Skill::Weaving, r.level);
    let lines = sim.level_up_tasks(P0, Skill::Weaving, r.level);
    assert!(lines.iter().any(|l| l.starts_with("New task")), "{lines:?}");
    assert!(lines.iter().any(|l| l.starts_with("You can make")));
}

// ---- Home pieces are wants ----

#[test]
fn a_home_piece_one_kind_short_is_a_want_and_a_built_one_is_not() {
    let mut sim = Sim::with_one_crab(Vec2::new(180.0, 0.0));
    set_level(&mut sim, P0, Skill::Building, 8);
    give(&mut sim, P0, &[(Item::KelpCord, 1)]);
    let lamp = sim
        .wants(P0)
        .into_iter()
        .find(|w| w.word == "glass-lamp")
        .expect("the glass lamp is a want");
    assert_eq!(lamp.piece, Some(Piece::GlassLamp));
    assert_eq!(lamp.missing, vec![(Item::Glass, 4)]);
    // The lamp's glass grows in the Silt Galleries, and the want says so.
    assert!(lamp.sources.iter().any(|s| matches!(
        s,
        Source::Node {
            node: NodeKind::GlassSpongeGarden,
            place: Some("upper gallery"),
            ..
        }
    )));
    assert!(!lamp.soon);
    // Short by two kinds: not a want.
    give(&mut sim, P0, &[]);
    assert!(sim.wants(P0).iter().all(|w| w.word != "glass-lamp"));
    // A piece a few levels away is soon.
    set_level(&mut sim, P0, Skill::Building, 6);
    give(&mut sim, P0, &[(Item::KelpCord, 1)]);
    assert!(
        sim.wants(P0)
            .iter()
            .any(|w| w.word == "glass-lamp" && w.soon)
    );
}

#[test]
fn a_piece_is_pinned_ready_and_unpinned_when_built() {
    let mut sim = solo_at_home();
    set_level(&mut sim, P0, Skill::Building, 8);
    give(&mut sim, P0, &[(Item::KelpCord, 1)]);
    assert!(sim.pin_want(P0, "glass-lamp"));
    let status = sim.pinned_status(P0).unwrap();
    assert_eq!(status.want.missing, vec![(Item::Glass, 4)]);
    assert!(!status.have_all);
    // Away from home with the last load on the shell: one ready word.
    {
        let c = sim.crab_mut(P0).unwrap();
        c.mover.pos = Vec2::new(1500.0, 0.0);
        c.mover.shelter = crabsim::home::Shelter::Out;
        c.mover.clinging.extend([Carry::Item(Item::Glass); 4]);
    }
    let mut said = 0;
    for _ in 0..60 {
        sim.tick();
        said += u32::from(sim.crab(P0).unwrap().events.goal_ready);
    }
    assert!(sim.pinned_status(P0).unwrap().have_all);
    assert_eq!(said, 1);
    // Home, unloaded, built: the pin lets go by itself.
    let mut sim2 = solo_at_home();
    set_level(&mut sim2, P0, Skill::Building, 8);
    give(&mut sim2, P0, &[(Item::KelpCord, 1), (Item::Glass, 4)]);
    assert!(sim2.pin_want(P0, "glass-lamp"));
    assert_eq!(sim2.build_piece(P0, Piece::GlassLamp), Ok(()));
    assert!(sim2.pinned_status(P0).is_none(), "auto-unpinned");
    assert!(sim2.crab(P0).unwrap().econ.pinned.is_none());
    // Built: no longer a want, and cannot be pinned.
    assert!(sim2.wants(P0).iter().all(|w| w.word != "glass-lamp"));
    assert!(!sim2.pin_want(P0, "glass-lamp"));
}

// ---- The pinned goal ----

#[test]
fn pinning_tracks_the_goal_and_says_so_once_when_it_is_in_hand() {
    let mut sim = Sim::with_one_crab(Vec2::new(180.0, 0.0));
    assert!(!sim.pin_want(P0, "no-such-recipe"));
    assert!(sim.pin_want(P0, "kelp-cord"));
    let status = sim.pinned_status(P0).unwrap();
    assert!(!status.have_all);
    assert_eq!(status.want.missing, vec![(Item::Kelp, 2)]);
    // Two kelp on the shell, away from home: one ready word, ever.
    {
        let c = sim.crab_mut(P0).unwrap();
        c.mover.clinging.push(Carry::Item(Item::Kelp));
        c.mover.clinging.push(Carry::Item(Item::Kelp));
    }
    let mut said = 0;
    for _ in 0..300 {
        sim.tick();
        if sim.crab(P0).unwrap().events.goal_ready {
            said += 1;
        }
    }
    assert!(sim.pinned_status(P0).unwrap().have_all);
    assert_eq!(said, 1, "the word is one-shot");
    // Lose the kelp and find it again: it says so again.
    sim.crab_mut(P0).unwrap().mover.clinging.clear();
    idle(&mut sim, 5);
    assert!(!sim.pinned_status(P0).unwrap().have_all);
    sim.crab_mut(P0)
        .unwrap()
        .mover
        .clinging
        .extend([Carry::Item(Item::Kelp), Carry::Item(Item::Kelp)]);
    let mut again = 0;
    for _ in 0..30 {
        sim.tick();
        if sim.crab(P0).unwrap().events.goal_ready {
            again += 1;
        }
    }
    assert_eq!(again, 1);
    // Let it go.
    sim.unpin(P0);
    assert!(sim.pinned_status(P0).is_none());
}

#[test]
fn having_it_all_at_home_needs_no_word_and_making_it_unpins() {
    let mut sim = solo_at_home();
    give(&mut sim, P0, &[(Item::Kelp, 2)]);
    assert!(sim.pin_want(P0, "kelp-cord"));
    let mut said = 0;
    for _ in 0..30 {
        sim.tick();
        said += u32::from(sim.crab(P0).unwrap().events.goal_ready);
    }
    assert_eq!(said, 0);
    assert!(sim.pinned_status(P0).unwrap().have_all);
    assert_eq!(sim.make_econ(P0, "kelp-cord"), Ok(()));
    assert!(sim.pinned_status(P0).is_none(), "auto-unpinned");
    assert!(sim.crab(P0).unwrap().econ.pinned.is_none());
}

#[test]
fn the_pin_is_saved_with_the_crab() {
    let mut sim = Sim::with_one_crab(Vec2::new(180.0, 0.0));
    sim.pin_want(P0, "kelp-cord");
    let saved = sim.progress(P0).unwrap();
    let mut fresh = Sim::with_one_crab(Vec2::new(180.0, 0.0));
    fresh.restore_progress(P0, &saved);
    assert_eq!(
        fresh.crab(P0).unwrap().econ.pinned.as_ref().unwrap().word,
        "kelp-cord"
    );
}

#[test]
fn a_remote_crab_cannot_be_pinned() {
    let mut sim = Sim::with_one_crab(Vec2::new(180.0, 0.0));
    sim.add_remote_crab(GUEST, Vec2::new(200.0, 0.0));
    assert!(!sim.pin_want(GUEST, "kelp-cord"));
    assert!(sim.wants(GUEST).is_empty());
    assert!(sim.crab(GUEST).unwrap().econ.pinned.is_none());
}

#[test]
fn a_replay_reproduces_pins_and_pieces() {
    let mut live = Sim::with_one_crab(Vec2::new(180.0, 0.0));
    let mut saved = live.progress(P0).unwrap();
    saved.stash = Stash::of(&[(Item::Kelp, 8), (Item::KelpCord, 2)]);
    saved.econ.skills.set(Skill::Building, xp::TABLE[3]);
    live.restore_progress(P0, &saved);
    live.start_recording();
    go_home(&mut live);
    idle(&mut live, 5);
    assert!(live.pin_want(P0, "kelp-cord"));
    idle(&mut live, 5);
    assert_eq!(live.make_econ(P0, "kelp-cord"), Ok(()));
    idle(&mut live, 5);
    assert!(live.pin_want(P0, "braided-cord"));
    idle(&mut live, 5);
    live.unpin(P0);
    idle(&mut live, 5);
    assert!(live.pin_want(P0, "kelp-curtain"));
    idle(&mut live, 5);
    assert_eq!(live.build_piece(P0, Piece::KelpCurtain), Ok(()));
    idle(&mut live, 30);
    let text = live.recording().unwrap().to_text();
    for word in [
        "pin=kelp-cord",
        "pin=braided-cord",
        "pin=none",
        "pin=kelp-curtain",
        "craft=kelp-curtain",
    ] {
        assert!(text.contains(word), "{word} in {text}");
    }
    let replay = crabsim::Replay::parse(&text).unwrap();
    assert_eq!(replay.pins.len(), 4);
    let mut again = Sim::with_one_crab(replay.start);
    crabsim::play(&mut again, &replay, replay.end.unwrap(), |_| {});
    assert_eq!(again.progress(P0), live.progress(P0));
    assert!(again.home_pieces().contains(&Piece::KelpCurtain));
    assert!(again.pinned_status(P0).is_none(), "built, so let go");
    assert_eq!(replay.hash, Some(again.state_hash()), "{text}");
}

// ---- Home pieces ----

#[test]
fn piece_rows_are_well_formed() {
    let mut words = BTreeSet::new();
    let mut last = 0;
    assert_eq!(PIECES.len(), 20);
    for d in PIECES {
        assert!(words.insert(d.word), "duplicate {}", d.word);
        assert!(
            ECON_RECIPES.iter().all(|r| r.word != d.word),
            "{} is also a recipe",
            d.word
        );
        assert!(d.level >= last, "in order of level");
        last = d.level;
        assert!(!d.needs.is_empty());
        assert_eq!(Piece::parse(d.word), Some(d.piece));
        assert_eq!(d.piece.xp(), piece_xp(d.level));
        assert!(d.piece.xp() >= 40);
    }
    // From level 2 to 15 a piece pays a level and a fifth of Building.
    assert_eq!(piece_xp(3), 122);
    assert_eq!(piece_xp(16), 128);
    assert_eq!(piece_xp(35), 280);
    assert_eq!(piece_xp(70), 560);
    assert_eq!(Piece::GlassLamp.level(), 8);
    assert_eq!(Piece::NoduleHearth.level(), 70);
}

#[test]
fn building_a_piece_solo_pays_from_the_bank_and_earns_experience() {
    let mut sim = solo_at_home();
    let bank = [(Item::Kelp, 7), (Item::KelpCord, 2)];
    give(&mut sim, P0, &bank);
    // Level 1: too low.
    assert_eq!(
        sim.can_build_piece(P0, Piece::KelpCurtain),
        Err(CantMakeEcon::Level(Skill::Building, 3))
    );
    set_level(&mut sim, P0, Skill::Building, 3);
    let before = building_xp(&sim, P0);
    give(&mut sim, P0, &[(Item::Kelp, 5), (Item::KelpCord, 2)]);
    assert_eq!(
        sim.can_build_piece(P0, Piece::KelpCurtain),
        Err(CantMakeEcon::Missing(Item::Kelp, 1))
    );
    give(&mut sim, P0, &bank);
    assert_eq!(sim.build_piece(P0, Piece::KelpCurtain), Ok(()));
    let c = sim.crab(P0).unwrap();
    assert_eq!(c.stash.count(Item::Kelp), 1);
    assert_eq!(c.stash.count(Item::KelpCord), 0);
    // Building 3 pays a level and a fifth (122).
    assert_eq!(building_xp(&sim, P0) - before, 122);
    assert!(sim.home_pieces().contains(&Piece::KelpCurtain));
    // Once only.
    give(&mut sim, P0, &bank);
    assert_eq!(
        sim.build_piece(P0, Piece::KelpCurtain),
        Err(CantMakeEcon::Made)
    );
    assert_eq!(sim.crab(P0).unwrap().stash.count(Item::Kelp), 7);
}

#[test]
fn a_piece_is_built_only_tucked_in_at_home() {
    let mut sim = Sim::with_one_crab(Vec2::new(180.0, 0.0));
    set_level(&mut sim, P0, Skill::Building, 3);
    give(&mut sim, P0, &[(Item::Kelp, 6), (Item::KelpCord, 2)]);
    assert_eq!(
        sim.build_piece(P0, Piece::KelpCurtain),
        Err(CantMakeEcon::NotAtHome)
    );
    assert!(sim.home_pieces().is_empty());
}

#[test]
fn the_nodule_hearth_mends_the_shell_sooner() {
    let mend = |hearth: bool| -> u32 {
        let mut sim = solo_at_home();
        if hearth {
            sim.shared.home_pieces.insert(Piece::NoduleHearth);
        }
        let tuning = Tuning::default();
        let whole = sim.crab(P0).unwrap().mover.max_shell(&tuning);
        sim.crab_mut(P0).unwrap().fighter.vitals.health = whole * 0.5;
        sim.crab_mut(P0).unwrap().fighter.vitals.unhit = 0;
        for n in 0..4000u32 {
            sim.tick();
            if sim.crab(P0).unwrap().fighter.vitals.health > whole * 0.5 + 0.01 {
                return n;
            }
        }
        u32::MAX
    };
    let (plain, warm) = (mend(false), mend(true));
    assert!(warm < plain, "{warm} should be sooner than {plain}");
}

// Two devices, as coop.rs wires them.

struct Game {
    host: Sim,
    guest: Sim,
    link: FakeLink,
}

fn world() -> Sim {
    Sim::new(Tuning::default(), Level::test_level())
}

fn two(delay: u32) -> Game {
    let at = Vec2::new(180.0, 0.0);
    let mut host = world();
    host.add_crab(HOST, at);
    host.add_remote_crab(GUEST, at);
    host.go_online(true);
    let mut guest = world();
    guest.add_remote_crab(HOST, at);
    guest.add_crab(GUEST, at);
    guest.go_online(false);
    let mut g = Game {
        host,
        guest,
        link: FakeLink::steady(delay),
    };
    g.guest.receive(g.host.world());
    for _ in 0..1500 {
        let walk = Controls {
            walk: -1.0,
            ..Controls::default()
        };
        g.host.set_controls(HOST, walk);
        g.guest.set_controls(GUEST, walk);
        g.step();
        if g.host.crab(HOST).unwrap().mover.tucked_in()
            && g.guest.crab(GUEST).unwrap().mover.tucked_in()
        {
            break;
        }
    }
    g.host.set_controls(HOST, Controls::default());
    g.guest.set_controls(GUEST, Controls::default());
    assert!(g.host.crab(HOST).unwrap().mover.tucked_in());
    assert!(g.guest.crab(GUEST).unwrap().mover.tucked_in());
    set_level(&mut g.host, HOST, Skill::Building, 20);
    set_level(&mut g.guest, GUEST, Skill::Building, 20);
    g
}

impl Game {
    fn step(&mut self) {
        self.host.tick();
        self.guest.tick();
        self.link.carry(&mut [&mut self.host, &mut self.guest]);
    }
    fn run(&mut self, n: u32) {
        for _ in 0..n {
            self.step();
        }
    }
}

const CURTAIN: [(Item, u32); 2] = [(Item::Kelp, 6), (Item::KelpCord, 2)];

#[test]
fn a_host_build_shows_on_the_guest() {
    let mut g = two(3);
    give(&mut g.host, HOST, &CURTAIN);
    assert_eq!(g.host.build_piece(HOST, Piece::KelpCurtain), Ok(()));
    assert!(
        g.host.home_pieces().contains(&Piece::KelpCurtain),
        "at once"
    );
    g.run(20);
    assert!(g.guest.home_pieces().contains(&Piece::KelpCurtain));
    // The guest cannot build what stands, and keeps its materials.
    give(&mut g.guest, GUEST, &CURTAIN);
    assert_eq!(
        g.guest.can_build_piece(GUEST, Piece::KelpCurtain),
        Err(CantMakeEcon::Made)
    );
}

#[test]
fn a_guest_builds_through_the_host_and_pays_only_when_it_is_confirmed() {
    let mut g = two(6);
    give(&mut g.guest, GUEST, &CURTAIN);
    let before = building_xp(&g.guest, GUEST);
    assert_eq!(g.guest.build_piece(GUEST, Piece::KelpCurtain), Ok(()));
    // Asked, not paid; asking twice is not allowed.
    assert_eq!(g.guest.crab(GUEST).unwrap().stash.count(Item::Kelp), 6);
    assert!(g.guest.home_pieces().is_empty());
    assert_eq!(
        g.guest.can_build_piece(GUEST, Piece::KelpCurtain),
        Err(CantMakeEcon::Made)
    );
    g.run(40);
    assert!(g.host.home_pieces().contains(&Piece::KelpCurtain));
    assert!(g.guest.home_pieces().contains(&Piece::KelpCurtain));
    let c = g.guest.crab(GUEST).unwrap();
    assert_eq!(c.stash.count(Item::Kelp), 0);
    assert_eq!(building_xp(&g.guest, GUEST) - before, 122);
    // The host's own bank, the host's copy of the guest and its xp: untouched.
    assert_eq!(g.host.crab(HOST).unwrap().stash.count(Item::Kelp), 0);
    let remote = g.host.crab(GUEST).unwrap();
    assert!(remote.stash.0.is_empty(), "remote crabs never change");
    assert_eq!(remote.econ.skills.get(Skill::Building), 0);
}

#[test]
fn two_crabs_building_the_same_piece_pay_only_once() {
    for guest_first in [false, true] {
        let mut g = two(8);
        give(&mut g.host, HOST, &CURTAIN);
        give(&mut g.guest, GUEST, &CURTAIN);
        if guest_first {
            assert_eq!(g.guest.build_piece(GUEST, Piece::KelpCurtain), Ok(()));
            g.run(2);
            // The host builds before the guest's ask reaches it.
            assert_eq!(g.host.build_piece(HOST, Piece::KelpCurtain), Ok(()));
        } else {
            assert_eq!(g.host.build_piece(HOST, Piece::KelpCurtain), Ok(()));
            assert_eq!(g.guest.build_piece(GUEST, Piece::KelpCurtain), Ok(()));
        }
        g.run(60);
        let paid = |g: &Game, id: PlayerId, sim: &Sim| {
            let _ = g;
            sim.crab(id).unwrap().stash.count(Item::Kelp) == 0
        };
        let host_paid = paid(&g, HOST, &g.host);
        let guest_paid = paid(&g, GUEST, &g.guest);
        assert!(
            host_paid ^ guest_paid,
            "exactly one pays (guest_first={guest_first}): host {host_paid}, guest {guest_paid}"
        );
        assert_eq!(g.host.home_pieces().len(), 1);
        assert_eq!(g.guest.home_pieces().len(), 1);
    }
}

#[test]
fn a_guest_that_joins_later_sees_the_built_pieces() {
    let mut host = world();
    host.add_crab(HOST, Vec2::new(180.0, 0.0));
    go_home(&mut host);
    set_level(&mut host, HOST, Skill::Building, 3);
    give(&mut host, HOST, &CURTAIN);
    assert_eq!(host.build_piece(HOST, Piece::KelpCurtain), Ok(()));
    host.go_online(true);
    let mut guest = world();
    guest.add_remote_crab(HOST, Vec2::new(180.0, 0.0));
    guest.add_crab(GUEST, Vec2::new(180.0, 0.0));
    guest.go_online(false);
    assert!(guest.home_pieces().is_empty());
    guest.receive(host.world());
    assert!(guest.home_pieces().contains(&Piece::KelpCurtain));
}

#[test]
fn pieces_survive_a_save_and_a_reload_of_the_host() {
    let mut sim = solo_at_home();
    set_level(&mut sim, P0, Skill::Building, 8);
    give(
        &mut sim,
        P0,
        &[(Item::Glass, 16), (Item::KelpCord, 2), (Item::Kelp, 6)],
    );
    assert_eq!(sim.build_piece(P0, Piece::GlassLamp), Ok(()));
    let saved = sim.progress(P0).unwrap();
    assert_eq!(saved.home_pieces, vec![Piece::GlassLamp]);
    let json = serde_json::to_string(&saved).unwrap();
    let back: crabsim::Progress = serde_json::from_str(&json).unwrap();
    let mut fresh = Sim::with_one_crab(Vec2::new(180.0, 0.0));
    assert!(fresh.home_pieces().is_empty());
    fresh.restore_progress(P0, &back);
    assert!(fresh.home_pieces().contains(&Piece::GlassLamp));
}

#[test]
fn a_visiting_guest_saves_its_own_home_not_the_hosts() {
    let mut g = two(2);
    give(&mut g.host, HOST, &CURTAIN);
    assert_eq!(g.host.build_piece(HOST, Piece::KelpCurtain), Ok(()));
    g.run(20);
    assert!(g.guest.home_pieces().contains(&Piece::KelpCurtain));
    assert_eq!(
        g.host.progress(HOST).unwrap().home_pieces,
        vec![Piece::KelpCurtain]
    );
    assert!(g.guest.progress(GUEST).unwrap().home_pieces.is_empty());
}

// ---- The materials stay reachable ----

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Src {
    Prying,
    Harvesting,
    Foraging,
    Culturing,
    Excavating,
    Fighting,
    Diving,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Activity {
    Gathering,
    Fighting,
    Diving,
    Digging,
}

fn activity(s: Src) -> Activity {
    match s {
        Src::Prying | Src::Harvesting | Src::Foraging | Src::Culturing => Activity::Gathering,
        Src::Excavating => Activity::Digging,
        Src::Fighting => Activity::Fighting,
        Src::Diving => Activity::Diving,
    }
}

/// A copy of `economy_every_way_pays.rs`'s classifier: where an item comes
/// from, by the activity.
fn sources(item: Item, depth: u32) -> BTreeSet<Src> {
    let drops: BTreeSet<Item> = KINDS
        .into_iter()
        .flat_map(|k| drops_of(k).into_iter().chain(far_drops_of(k)))
        .collect();
    let mut out = BTreeSet::new();
    for n in NODES.iter().filter(|n| n.yields == Some(item)) {
        out.insert(match n.skill {
            Skill::Prying => Src::Prying,
            Skill::Harvesting => Src::Harvesting,
            Skill::Foraging => Src::Foraging,
            Skill::Culturing => Src::Culturing,
            Skill::Excavating => Src::Excavating,
            other => panic!("{other:?} node"),
        });
        if n.tier >= 4 {
            out.insert(Src::Diving);
        }
    }
    match item {
        Item::Valve | Item::Pearl => {
            out.insert(Src::Prying);
        }
        Item::Scale => {
            out.insert(Src::Foraging);
            out.insert(Src::Diving);
        }
        _ => {}
    }
    if drops.contains(&item) {
        out.insert(Src::Fighting);
        if item.tier() >= 4 {
            out.insert(Src::Diving);
        }
    }
    if out.is_empty() && depth < 8 {
        let made = ECON_RECIPES
            .iter()
            .find(|r| matches!(r.makes, Output::Items(o) if o.iter().any(|&(i, _)| i == item)));
        if let Some(r) = made {
            for &(i, _) in r.needs {
                out.extend(sources(i, depth + 1));
            }
        }
    }
    out
}

#[test]
fn pieces_above_level_ten_draw_on_two_activities() {
    println!("piece                  lvl  activities");
    for d in PIECES {
        let acts: BTreeSet<Activity> = d
            .needs
            .iter()
            .flat_map(|&(i, _)| sources(i, 0))
            .map(activity)
            .collect();
        println!("{:<22} {:>3}  {acts:?}", d.word, d.level);
        for &(i, _) in d.needs {
            assert!(!sources(i, 0).is_empty(), "{}: {i:?} has no source", d.word);
        }
        if d.level >= 10 {
            assert!(acts.len() >= 2, "{} draws on {acts:?}", d.word);
        }
    }
}

/// The soft-lock fixed point of `economy_craft.rs`, with the pieces: from
/// nothing, every piece's materials become reachable.
#[test]
fn the_soft_lock_fixed_point_includes_the_pieces() {
    let mut items: BTreeSet<Item> = BTreeSet::new();
    let mut tools: BTreeSet<(ToolKind, u8)> = BTreeSet::new();
    let mut stations: BTreeSet<Station> = BTreeSet::new();
    loop {
        let before = (items.len(), tools.len(), stations.len());
        for n in NODES {
            let tool_ok = match n.tool {
                None => true,
                Some(_) if n.tool_tier == 0 => true,
                Some(k) => tools.iter().any(|&(tk, t)| tk == k && t >= n.tool_tier),
            };
            if tool_ok && let Some(item) = n.yields {
                items.insert(item);
            }
        }
        if items.contains(&Item::Clam) || items.contains(&Item::Mussel) {
            items.insert(Item::Valve);
            items.insert(Item::Pearl);
        }
        if items.contains(&Item::ScalyFoot) {
            items.insert(Item::Scale);
        }
        for kind in KINDS {
            items.extend(drops_of(kind));
            items.extend(far_drops_of(kind));
        }
        for r in ECON_RECIPES {
            let bench_ok = match r.at {
                Bench::Workbench => true,
                Bench::At(st) => stations.contains(&st),
                Bench::VentPot => items.contains(&Item::VentMat),
            };
            if !bench_ok || !r.needs.iter().all(|(i, _)| items.contains(i)) {
                continue;
            }
            match r.makes {
                Output::Items(out) => items.extend(out.iter().map(|(i, _)| *i)),
                Output::Tool(k, t) => {
                    tools.insert((k, t));
                }
                Output::Station(st) => {
                    stations.insert(st);
                }
                Output::Portions(f, _) => items.extend(f.extra().iter().map(|(i, _)| *i)),
                Output::Gear(_) | Output::Bed(_) | Output::Repair(_) | Output::Effect(_) => {}
            }
        }
        if before == (items.len(), tools.len(), stations.len()) {
            break;
        }
    }
    for d in PIECES {
        for &(item, _) in d.needs {
            assert!(items.contains(&item), "{}: {item:?} is unreachable", d.word);
        }
    }
}
