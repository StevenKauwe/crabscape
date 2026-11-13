//! The unlock ladder for levels 2 to 15 (`economy/ladder.rs`): no silent
//! level, the table agrees with the data, perk lines state the rules'
//! own numbers, and the crafting skills never stall below 15 for want of
//! clay, quill or glass.

use crabsim::economy::ladder::{self, LADDER, Opening, Perk};
use crabsim::economy::{ECON_RECIPES, Item, NODES, NodeKind, Output, PIECES, unlocks, xp};
use crabsim::{Skill, Tuning};

const THIRTEEN: [Skill; 13] = [
    Skill::Climbing,
    Skill::Swimming,
    Skill::Fighting,
    Skill::Foraging,
    Skill::Prying,
    Skill::Excavating,
    Skill::Harvesting,
    Skill::Culturing,
    Skill::Cooking,
    Skill::Shellcraft,
    Skill::Weaving,
    Skill::Building,
    Skill::Survival,
];

#[test]
fn no_level_from_2_to_15_is_silent() {
    for s in THIRTEEN {
        for l in 2..=15 {
            assert!(!unlocks(s, l).is_empty(), "{} {l} is silent", s.name());
        }
    }
}

#[test]
fn the_table_agrees_with_the_data() {
    for r in LADDER {
        assert!((2..=15).contains(&r.level), "{r:?}");
        match r.opening {
            Opening::Recipe(w) => {
                let d = ECON_RECIPES.iter().find(|d| d.word == w).expect(w);
                assert_eq!((d.skill, d.level), (r.skill, r.level), "{w}");
            }
            Opening::Node(k) => {
                let d = k.def();
                assert_eq!((d.skill, d.level), (r.skill, r.level), "{k:?}");
            }
            Opening::Piece(p) => {
                assert_eq!((Skill::Building, p.level()), (r.skill, r.level), "{p:?}");
            }
            Opening::Tool(kind, tier) => {
                let found = ECON_RECIPES.iter().any(|d| {
                    d.makes == Output::Tool(kind, tier) && d.skill == r.skill && d.level == r.level
                });
                assert!(found, "{kind:?} {tier}");
            }
            Opening::Perk(Perk::Portions(w), _) => {
                assert!(ECON_RECIPES.iter().any(|d| d.word == w), "{w}");
            }
            Opening::Perk(..) => {}
        }
    }
}

#[test]
fn everything_the_data_opens_is_in_the_table() {
    let listed = |skill: Skill, level: u32, f: &dyn Fn(&Opening) -> bool| {
        LADDER
            .iter()
            .any(|r| r.skill == skill && r.level == level && f(&r.opening))
    };
    for d in ECON_RECIPES.iter().filter(|d| (2..=15).contains(&d.level)) {
        let ok = listed(d.skill, d.level, &|o| match *o {
            Opening::Recipe(w) => w == d.word,
            Opening::Tool(k, t) => d.makes == Output::Tool(k, t),
            _ => false,
        });
        // Repairs and re-tips at level 1 are not here; any recipe at 2-15 is.
        assert!(
            ok,
            "recipe {} ({} {}) is not in the ladder",
            d.word,
            d.skill.name(),
            d.level
        );
    }
    for n in NODES
        .iter()
        .filter(|n| (2..=15).contains(&n.level) && n.yields.is_some())
    {
        assert!(
            listed(n.skill, n.level, &|o| *o == Opening::Node(n.kind)),
            "{:?}",
            n.kind
        );
    }
    for p in PIECES.iter().filter(|p| (2..=15).contains(&p.level)) {
        assert!(
            listed(Skill::Building, p.level, &|o| *o == Opening::Piece(p.piece)),
            "{:?}",
            p.piece
        );
    }
}

#[test]
fn the_physical_lines_state_the_real_numbers() {
    let t = Tuning::default();
    let climb = t.climb_speed_per_level;
    let stroke = t.swim_quicker_per_level;
    let claw = t.fight_damage_per_level;
    for r in LADDER {
        let Opening::Perk(p, n) = r.opening else {
            continue;
        };
        let up = r.level - 1;
        match p {
            Perk::Climb => assert_eq!(n, (100.0 * climb * up as f32).round() as u32),
            Perk::Stroke => assert_eq!(n, (100.0 * stroke * up as f32).round() as u32),
            Perk::Claws => assert_eq!(n, (100.0 * claw * up as f32).round() as u32),
            Perk::Quicker => assert_eq!(n, ladder::quicker_percent(r.level)),
            Perk::Pressure => assert_eq!(n, r.level),
            Perk::FedLonger => assert_eq!(n, ladder::fed_longer_percent(r.level)),
            Perk::Headroom => assert_eq!(n as f32, ladder::headroom(r.level)),
            _ => {}
        }
    }
    // The three stop at the skill cap: the lines past it are other rules.
    assert_eq!(t.skill_cap, 10);
}

#[test]
fn the_quicker_line_is_the_gather_time_formulas_speed() {
    let def = NodeKind::MusselClump.def();
    let base = def.gather_ticks(1, 0, 0) as f32;
    for l in 2..=15 {
        let n = ladder::quicker_percent(l);
        let at = def.gather_ticks(l, 0, 0) as f32;
        let speed = base / at;
        assert!(
            (speed - (1.0 + n as f32 / 100.0)).abs() < 0.02,
            "level {l}: {speed} vs {n}%"
        );
    }
}

#[test]
fn perks_only_grow_with_level() {
    for l in 1..15 {
        for s in [Skill::Prying, Skill::Foraging, Skill::Culturing] {
            assert!(ladder::double_yield_percent(s, l) <= ladder::double_yield_percent(s, l + 1));
        }
        assert!(
            ladder::rare_modulus(Skill::Prying, l) >= ladder::rare_modulus(Skill::Prying, l + 1)
        );
        assert!(ladder::bed_quicker_percent(l) <= ladder::bed_quicker_percent(l + 1));
        assert!(ladder::tool_hold_percent(l) <= ladder::tool_hold_percent(l + 1));
    }
    assert_eq!(ladder::rare_modulus(Skill::Prying, 1), 64);
    assert_eq!(ladder::rare_modulus(Skill::Prying, 7), 48);
    assert_eq!(ladder::portions("shucked-mussels", 2, 3), 2);
    assert_eq!(ladder::portions("shucked-mussels", 2, 4), 3);
}

/// Items a new crab can reach without clay, quill or glass: tier 1 nodes,
/// the two snapping-shrimp drops, valves and what they are processed into.
fn reachable() -> Vec<Item> {
    let mut have: Vec<Item> = NODES
        .iter()
        .filter(|n| n.tier == 1)
        .filter_map(|n| n.yields)
        .collect();
    have.extend([
        Item::Valve,
        Item::SnapperClaw,
        Item::ShrimpMeat,
        Item::Pearl,
    ]);
    loop {
        let before = have.len();
        for r in ECON_RECIPES.iter().filter(|r| r.level < 15) {
            if r.needs.iter().all(|(i, _)| have.contains(i))
                && let Output::Items(out) = r.makes
            {
                for (i, _) in out {
                    if !have.contains(i) {
                        have.push(*i);
                    }
                }
            }
        }
        if have.len() == before {
            return have;
        }
    }
}

#[test]
fn the_crafting_skills_never_stall_for_clay_quill_or_glass_below_15() {
    let have = reachable();
    for skill in [
        Skill::Shellcraft,
        Skill::Weaving,
        Skill::Building,
        Skill::Cooking,
        Skill::Culturing,
    ] {
        for level in 2..15 {
            // Something to make at exactly this level, or an earlier one
            // that stays repeatable, from tier 1 things only.
            let recipe = ECON_RECIPES.iter().any(|r| {
                r.skill == skill
                    && r.level <= level
                    && r.level >= 2
                    && (skill == Skill::Building || !matches!(r.makes, Output::Station(_)))
                    && r.needs.iter().all(|(i, _)| have.contains(i))
            });
            let piece = skill == Skill::Building
                && PIECES
                    .iter()
                    .any(|p| p.level == level && p.needs.iter().all(|(i, _)| have.contains(i)));
            assert!(recipe || piece, "{} stalls at level {level}", skill.name());
        }
    }
}

#[test]
fn building_pieces_from_tier_one_pay_every_level_without_clay_quill_or_glass() {
    let have = reachable();
    // Grit stages give 100 Building XP at level 1; then each tier-1 piece or
    // station pays at least its own level.
    let mut total: u32 = 100;
    for level in 2..=14u32 {
        for r in ECON_RECIPES
            .iter()
            .filter(|r| r.skill == Skill::Building && r.level == level)
        {
            if matches!(r.makes, Output::Station(_))
                && r.needs.iter().all(|(i, _)| have.contains(i))
            {
                total += r.xp;
            }
        }
        for p in PIECES.iter().filter(|p| p.level == level) {
            if p.needs.iter().all(|(i, _)| have.contains(i)) {
                total += p.piece.xp();
            }
        }
        assert!(
            total >= xp::TABLE[level as usize + 1],
            "Building has {total} XP from tier 1 pieces at level {level}; needs {}",
            xp::TABLE[level as usize + 1]
        );
    }
}
