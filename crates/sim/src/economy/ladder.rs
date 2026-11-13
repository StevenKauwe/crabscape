//! The unlock ladder for levels 2 to 15: one table that says, for each of the
//! thirteen skills and each level, what that level opens
//! (`docs/design/skills-and-resources.md`, "The first hours: levels 1-15").
//!
//! An opening is a recipe, a node kind, a home piece, a tool or a perk. A
//! perk is a real rule with the number its line states, and the rules read
//! their numbers from this table (so the line and the rule cannot drift):
//! the quicker-gathering line is the gather-time formula's own speed, the
//! rare-find chance, the second item of a tier 1 gather, the third portion of
//! a tier 1 dish, a doubled craft, a longer-lasting tool, a quicker culture
//! bed, and the physical skills' per-level effects (climb speed, stroke
//! quickness, claw strength) with their numbers; above level 10, where those
//! stop, quicker rest for the paddle legs and a further share of claw
//! strength (climb speed has no cap).
//!
//! Every chance is a personal [`super::roll`] of the crab's own counters,
//! never the world's random generator, and none of them touches the shared
//! node charges.

use super::gear::ToolKind;
use super::home::Piece;
use super::items::Item;
use super::nodes::NodeKind;
use crate::moult::Feat;
use crate::skills::Skill;

/// The first level the ladder speaks for (level 1 is where the crab starts).
pub const FIRST_EARLY: u32 = 2;
/// The last level of the first hours.
pub const LAST_EARLY: u32 = 15;

/// Percent of its old value a tier 1 recipe's experience is paid at
/// (`recipes::early`): the first-hours factor for crafts.
pub const RECIPE_XP_PERCENT: u32 = 300;
/// Percent a tier 1 node's gather experience is paid at below level 15,
/// fading to 100% by level 30 (the table in `nodes.rs` keeps the old 10 and
/// 8, so the tier-by-tier pacing at higher levels is unchanged).
pub const NODE_XP_PERCENT: u32 = 130;

/// The experience a gather of `def` pays a crab at skill `level`.
pub fn node_xp(def: &super::nodes::NodeDef, level: u32) -> u32 {
    if def.tier != 1 {
        return def.xp;
    }
    let extra = NODE_XP_PERCENT - 100;
    let share = match level {
        0..=14 => 15,
        15..=29 => 30 - level,
        _ => 0,
    };
    def.xp * (100 + extra * share / 15) / 100
}

/// What a group of crafted things is, for the doubled-craft perks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Group {
    /// Kelp cord.
    Cord,
    /// Kelp rope.
    Rope,
    /// Kelp mats.
    Mat,
    /// Shell blanks.
    Blank,
    /// Polished valves and shell cement.
    Polish,
}

impl Group {
    /// Does a recipe's first output belong to the group?
    pub fn holds(self, item: Item) -> bool {
        match self {
            Group::Cord => item == Item::KelpCord,
            Group::Rope => item == Item::KelpRope,
            Group::Mat => item == Item::KelpMat,
            Group::Blank => item == Item::ShellBlank,
            Group::Polish => matches!(item, Item::PolishedValve | Item::ShellCement),
        }
    }

    fn name(self) -> &'static str {
        match self {
            Group::Cord => "kelp cord",
            Group::Rope => "kelp rope",
            Group::Mat => "kelp mats",
            Group::Blank => "shell blanks",
            Group::Polish => "polished valves and shell cement",
        }
    }
}

/// A rule the level gives. The number is in the opening.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Perk {
    /// Gathering this many percent quicker than at level 1 (the gather-time
    /// formula's own speed: 2% a level).
    Quicker,
    /// A second item from a tier 1 gather, this many percent of the time.
    Double,
    /// A rare find comes one gather in this many.
    Rare,
    /// This dish (a recipe word) makes this many portions a batch.
    Portions(&'static str),
    /// This many percent of these crafts come out doubled.
    DoubleCraft(Group),
    /// Tools you make hold this many percent more uses.
    ToolHold,
    /// Culture beds grow this many percent quicker.
    BedQuicker,
    /// Climbing: this many percent quicker than at level 1.
    Climb,
    /// Swimming: strokes this many percent quicker than at level 1.
    Stroke,
    /// Fighting: claws this many percent harder than at level 1.
    Claws,
    /// Swimming above 10: the paddle legs rest this many percent faster.
    Legs,
    /// Fighting above 10: claws a further this many percent harder.
    ClawsBeyond,
    /// Survival: the shell takes pressure this many percent easier.
    Pressure,
    /// Survival: a meal keeps the crab well fed this many percent longer.
    FedLonger,
    /// Survival: pressure strain starts this many units deeper than the
    /// depth the crab is used to.
    Headroom,
    /// A moult feat comes within reach at this level.
    FeatReady(Feat),
}

/// What a level opens.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Opening {
    /// A recipe, by its replay word.
    Recipe(&'static str),
    /// A node kind.
    Node(NodeKind),
    /// A home piece.
    Piece(Piece),
    /// A tool of this kind and tier (the recipe that makes it).
    Tool(ToolKind, u8),
    /// A perk with its number.
    Perk(Perk, u32),
}

/// One row: at `level`, `skill` opens `opening`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Row {
    pub skill: Skill,
    pub level: u32,
    pub opening: Opening,
}

const fn row(skill: Skill, level: u32, opening: Opening) -> Row {
    Row {
        skill,
        level,
        opening,
    }
}

use NodeKind as N;
use Opening::{Node, Perk as P, Piece as Pc, Recipe as Rc, Tool as Tl};
use Perk::{
    BedQuicker, Claws, ClawsBeyond, Climb, Double, DoubleCraft, FeatReady, FedLonger, Headroom,
    Legs, Portions, Pressure, Quicker, Rare, Stroke, ToolHold,
};
use Skill::{
    Building, Climbing, Cooking, Culturing, Excavating, Fighting, Foraging, Harvesting, Prying,
    Shellcraft, Survival, Swimming, Weaving,
};

/// The ladder: levels 2 to 15 of the thirteen skills, in order of skill
/// (the skills panel's order) and level.
pub const LADDER: &[Row] = &[
    // ---- Climbing: +8% climb speed a level to 10, then an easier grip ----
    row(Climbing, 2, P(Climb, 8)),
    row(Climbing, 3, P(Climb, 16)),
    row(Climbing, 4, P(Climb, 24)),
    row(Climbing, 5, P(Climb, 32)),
    row(Climbing, 5, P(FeatReady(Feat::GrippingDactyls), 5)),
    row(Climbing, 6, P(Climb, 40)),
    row(Climbing, 7, P(Climb, 48)),
    row(Climbing, 8, P(Climb, 56)),
    row(Climbing, 9, P(Climb, 64)),
    row(Climbing, 10, P(Climb, 72)),
    row(Climbing, 11, P(Climb, 80)),
    row(Climbing, 12, P(Climb, 88)),
    row(Climbing, 13, P(Climb, 96)),
    row(Climbing, 14, P(Climb, 104)),
    row(Climbing, 15, P(Climb, 112)),
    // ---- Swimming: strokes 5% quicker a level to 10, then quicker rest ----
    row(Swimming, 2, P(Stroke, 5)),
    row(Swimming, 3, P(Stroke, 10)),
    row(Swimming, 4, P(Stroke, 15)),
    row(Swimming, 5, P(Stroke, 20)),
    row(Swimming, 5, P(FeatReady(Feat::BroadPaddles), 5)),
    row(Swimming, 6, P(Stroke, 25)),
    row(Swimming, 7, P(Stroke, 30)),
    row(Swimming, 8, P(Stroke, 35)),
    row(Swimming, 9, P(Stroke, 40)),
    row(Swimming, 10, P(Stroke, 45)),
    row(Swimming, 11, P(Legs, 3)),
    row(Swimming, 12, P(Legs, 6)),
    row(Swimming, 13, P(Legs, 9)),
    row(Swimming, 14, P(Legs, 12)),
    row(Swimming, 15, P(Legs, 15)),
    // ---- Fighting: claws 5% harder a level to 10, then a further 2% ----
    row(Fighting, 2, P(Claws, 5)),
    row(Fighting, 3, P(Claws, 10)),
    row(Fighting, 4, P(Claws, 15)),
    row(Fighting, 5, P(Claws, 20)),
    row(Fighting, 5, P(FeatReady(Feat::HeavyCrusher), 5)),
    row(Fighting, 6, P(Claws, 25)),
    row(Fighting, 7, P(Claws, 30)),
    row(Fighting, 8, P(Claws, 35)),
    row(Fighting, 9, P(Claws, 40)),
    row(Fighting, 10, P(Claws, 45)),
    row(Fighting, 11, P(ClawsBeyond, 2)),
    row(Fighting, 12, P(ClawsBeyond, 4)),
    row(Fighting, 13, P(ClawsBeyond, 6)),
    row(Fighting, 14, P(ClawsBeyond, 8)),
    row(Fighting, 15, P(ClawsBeyond, 10)),
    // ---- Foraging ----
    row(Foraging, 2, P(Quicker, 2)),
    row(Foraging, 3, P(Double, 3)),
    row(Foraging, 4, P(Quicker, 6)),
    row(Foraging, 5, P(Quicker, 8)),
    row(Foraging, 6, P(Double, 6)),
    row(Foraging, 7, P(Quicker, 12)),
    row(Foraging, 8, Node(N::BrittleStarCrevice)),
    row(Foraging, 9, P(Quicker, 16)),
    row(Foraging, 10, P(Double, 9)),
    row(Foraging, 11, P(Quicker, 20)),
    row(Foraging, 12, P(Double, 11)),
    row(Foraging, 13, P(Quicker, 24)),
    row(Foraging, 14, P(Double, 13)),
    row(Foraging, 15, Node(N::WormMud)),
    // ---- Prying ----
    row(Prying, 2, P(Quicker, 2)),
    row(Prying, 3, P(Double, 3)),
    row(Prying, 4, P(Rare, 56)),
    row(Prying, 5, P(Quicker, 8)),
    row(Prying, 6, P(Double, 6)),
    row(Prying, 7, P(Rare, 48)),
    row(Prying, 8, Node(N::BarnacleRock)),
    row(Prying, 9, P(Quicker, 16)),
    row(Prying, 10, P(Double, 9)),
    row(Prying, 11, P(Rare, 44)),
    row(Prying, 12, P(Double, 11)),
    row(Prying, 13, P(Quicker, 24)),
    row(Prying, 14, P(Double, 13)),
    row(Prying, 15, Node(N::ClamBed)),
    row(Prying, 15, P(Rare, 40)),
    // ---- Excavating ----
    row(Excavating, 2, P(Quicker, 2)),
    row(Excavating, 3, P(Double, 3)),
    row(Excavating, 4, P(Quicker, 6)),
    row(Excavating, 5, P(Quicker, 8)),
    row(Excavating, 6, P(Double, 6)),
    row(Excavating, 7, P(Quicker, 12)),
    row(Excavating, 8, Node(N::SandDollarBed)),
    row(Excavating, 9, P(Quicker, 16)),
    row(Excavating, 10, P(Double, 9)),
    row(Excavating, 11, P(Quicker, 20)),
    row(Excavating, 12, P(Double, 11)),
    row(Excavating, 13, P(Quicker, 24)),
    row(Excavating, 14, P(Double, 13)),
    row(Excavating, 15, Node(N::ClayBank)),
    // ---- Harvesting ----
    row(Harvesting, 2, P(Quicker, 2)),
    row(Harvesting, 3, P(Double, 3)),
    row(Harvesting, 4, P(Quicker, 6)),
    row(Harvesting, 5, P(Quicker, 8)),
    row(Harvesting, 6, P(Double, 6)),
    row(Harvesting, 7, P(Quicker, 12)),
    row(Harvesting, 8, Node(N::EelgrassBed)),
    row(Harvesting, 9, P(Quicker, 16)),
    row(Harvesting, 10, P(Double, 9)),
    row(Harvesting, 11, P(Quicker, 20)),
    row(Harvesting, 12, P(Double, 11)),
    row(Harvesting, 13, P(Quicker, 24)),
    row(Harvesting, 14, P(Double, 13)),
    row(Harvesting, 15, Node(N::SeaPenField)),
    // ---- Culturing ----
    row(Culturing, 2, Rc("shore-broth")),
    row(Culturing, 3, P(Double, 3)),
    row(Culturing, 4, Rc("mussel-broth")),
    row(Culturing, 5, P(Double, 5)),
    row(Culturing, 6, P(BedQuicker, 10)),
    row(Culturing, 7, P(Quicker, 12)),
    row(Culturing, 8, Node(N::PurpleFilmMat)),
    row(Culturing, 9, Rc("purple-starter")),
    row(Culturing, 10, P(BedQuicker, 20)),
    row(Culturing, 11, P(Double, 8)),
    row(Culturing, 12, Rc("glow-seed")),
    row(Culturing, 13, P(Double, 10)),
    row(Culturing, 14, P(BedQuicker, 30)),
    row(Culturing, 15, Node(N::PitGlowSite)),
    row(Culturing, 15, Rc("plant-culture")),
    // ---- Cooking ----
    row(Cooking, 2, Rc("kelp-wrapped-mussel")),
    row(Cooking, 3, Rc("snail-half-shell")),
    row(Cooking, 4, P(Portions("shucked-mussels"), 3)),
    row(Cooking, 5, Rc("snail-wrap")),
    row(Cooking, 6, Rc("mussel-snail-hash")),
    row(Cooking, 7, Rc("shrimp-strips")),
    row(Cooking, 8, Rc("barnacle-chowder")),
    row(Cooking, 9, P(Portions("kelp-wrapped-mussel"), 3)),
    row(Cooking, 9, P(Portions("snail-half-shell"), 3)),
    row(Cooking, 10, Rc("snapper-broth")),
    row(Cooking, 11, Rc("brittle-star-stew")),
    row(Cooking, 12, Rc("worm-paste")),
    row(Cooking, 12, Rc("amphipod-mash")),
    row(Cooking, 13, Rc("eelgrass-salad")),
    row(Cooking, 14, Rc("barnacle-kelp-roll")),
    row(Cooking, 14, P(Portions("barnacle-chowder"), 3)),
    row(Cooking, 15, Rc("purple-film-paste")),
    row(Cooking, 15, P(Portions("brittle-star-stew"), 3)),
    // ---- Shellcraft ----
    row(Shellcraft, 2, Rc("polished-valve")),
    row(Shellcraft, 3, P(DoubleCraft(Group::Blank), 10)),
    row(Shellcraft, 4, Rc("shell-cement")),
    row(Shellcraft, 5, Rc("shell-plating")),
    row(Shellcraft, 6, P(DoubleCraft(Group::Blank), 15)),
    row(Shellcraft, 7, P(ToolHold, 10)),
    row(Shellcraft, 8, Rc("sand-polish")),
    row(Shellcraft, 9, Rc("barnacle-cement")),
    row(Shellcraft, 10, P(DoubleCraft(Group::Blank), 20)),
    row(Shellcraft, 11, Rc("mend-sand-dollar")),
    row(Shellcraft, 11, P(DoubleCraft(Group::Polish), 10)),
    row(Shellcraft, 12, P(ToolHold, 20)),
    row(Shellcraft, 13, P(DoubleCraft(Group::Polish), 15)),
    row(Shellcraft, 14, P(DoubleCraft(Group::Polish), 20)),
    row(Shellcraft, 14, Rc("ground-valves")),
    row(Shellcraft, 15, Tl(ToolKind::Pry, 2)),
    row(Shellcraft, 15, Tl(ToolKind::Scoop, 2)),
    row(Shellcraft, 15, Tl(ToolKind::Cutter, 2)),
    row(Shellcraft, 15, Rc("quill-rod")),
    // ---- Weaving ----
    row(Weaving, 2, Rc("kelp-rope")),
    row(Weaving, 3, P(DoubleCraft(Group::Cord), 10)),
    row(Weaving, 4, Rc("kelp-mat")),
    row(Weaving, 5, Rc("quick-twist")),
    row(Weaving, 6, P(DoubleCraft(Group::Cord), 15)),
    row(Weaving, 7, P(DoubleCraft(Group::Rope), 12)),
    row(Weaving, 8, Rc("kelp-sack")),
    row(Weaving, 9, Rc("eelgrass-cord")),
    row(Weaving, 10, Rc("slime-twine")),
    row(Weaving, 11, P(DoubleCraft(Group::Cord), 20)),
    row(Weaving, 11, Rc("loom-mats")),
    row(Weaving, 12, Rc("eelgrass-mat")),
    row(Weaving, 13, P(DoubleCraft(Group::Mat), 15)),
    row(Weaving, 14, Rc("rope-bulk")),
    row(Weaving, 15, P(DoubleCraft(Group::Rope), 20)),
    row(Weaving, 15, P(DoubleCraft(Group::Mat), 20)),
    // ---- Building: a piece or a station at nearly every level ----
    row(Building, 2, Pc(Piece::ValveStep)),
    row(Building, 3, Pc(Piece::KelpCurtain)),
    row(Building, 4, Pc(Piece::SnailCairn)),
    row(Building, 5, Rc("bed-1")),
    row(Building, 6, Pc(Piece::GritPath)),
    row(Building, 7, Pc(Piece::GlowJar)),
    row(Building, 8, Pc(Piece::ValveChime)),
    row(Building, 8, Pc(Piece::GlassLamp)),
    row(Building, 9, Pc(Piece::RopeRail)),
    row(Building, 10, Rc("brine-pot")),
    row(Building, 10, Rc("loom")),
    row(Building, 10, Pc(Piece::KelpGarden)),
    row(Building, 11, Pc(Piece::ClawRack)),
    row(Building, 12, Pc(Piece::ValveShelf)),
    row(Building, 13, Pc(Piece::SandSill)),
    row(Building, 14, Pc(Piece::EelgrassThatch)),
    row(Building, 14, Rc("grindstone")),
    row(Building, 15, Pc(Piece::BarnacleWall)),
    // ---- Survival: 1% less pressure strain a level (cap 40%) ----
    row(Survival, 2, P(Pressure, 2)),
    row(Survival, 3, P(Pressure, 3)),
    row(Survival, 4, P(Pressure, 4)),
    row(Survival, 5, P(Pressure, 5)),
    row(Survival, 5, P(FedLonger, 15)),
    row(Survival, 6, P(Pressure, 6)),
    row(Survival, 7, P(Pressure, 7)),
    row(Survival, 8, P(Pressure, 8)),
    row(Survival, 9, P(Pressure, 9)),
    row(Survival, 10, P(Pressure, 10)),
    row(Survival, 10, P(Headroom, 200)),
    row(Survival, 10, P(FedLonger, 30)),
    row(Survival, 11, P(Pressure, 11)),
    row(Survival, 12, P(Pressure, 12)),
    row(Survival, 13, P(Pressure, 13)),
    row(Survival, 14, P(Pressure, 14)),
    row(Survival, 15, P(Pressure, 15)),
];

// ---- The rules that read the table ----

/// The best value of a perk that a crab of `level` in `skill` has earned
/// (the highest-level row at or below `level` that `want` accepts).
fn best(skill: Skill, level: u32, want: impl Fn(Perk) -> bool) -> Option<u32> {
    LADDER
        .iter()
        .filter(|r| r.skill == skill && r.level <= level)
        .filter_map(|r| match r.opening {
            Opening::Perk(p, n) if want(p) => Some((r.level, n)),
            _ => None,
        })
        .max_by_key(|&(l, _)| l)
        .map(|(_, n)| n)
}

/// Percent faster than at level 1 that gathering is at `level`: the
/// gather-time formula's own 2% a level (the line is the formula).
pub fn quicker_percent(level: u32) -> u32 {
    2 * level.saturating_sub(1)
}

/// Percent of tier 1 gathers of `skill` that give a second item at `level`.
pub fn double_yield_percent(skill: Skill, level: u32) -> u32 {
    best(skill, level, |p| p == Double).unwrap_or(0)
}

/// A rare find comes one gather in this many, at Prying `level`.
pub fn rare_modulus(skill: Skill, level: u32) -> u64 {
    if skill != Skill::Prying {
        return 64;
    }
    u64::from(best(skill, level, |p| p == Rare).unwrap_or(64))
}

/// Portions a batch of the dish `word` makes at Cooking `level`, if it
/// is more than `base`.
pub fn portions(word: &str, base: u8, level: u32) -> u8 {
    let more = best(Cooking, level, |p| p == Portions(word_static(word)));
    more.map_or(base, |n| base.max(n as u8))
}

/// The perk's own word for a dish (the table holds static strings).
fn word_static(word: &str) -> &'static str {
    LADDER
        .iter()
        .find_map(|r| match r.opening {
            Opening::Perk(Perk::Portions(w), _) if w == word => Some(w),
            _ => None,
        })
        .unwrap_or("")
}

/// Percent of crafts of a `skill`'s `item` that come out doubled at `level`.
pub fn double_craft_percent(skill: Skill, level: u32, item: Item) -> u32 {
    LADDER
        .iter()
        .filter(|r| r.skill == skill && r.level <= level)
        .filter_map(|r| match r.opening {
            Opening::Perk(Perk::DoubleCraft(g), n) if g.holds(item) => Some(n),
            _ => None,
        })
        .max()
        .unwrap_or(0)
}

/// Percent more uses a tool made at Shellcraft `level` has.
pub fn tool_hold_percent(level: u32) -> u32 {
    best(Shellcraft, level, |p| p == ToolHold).unwrap_or(0)
}

/// Percent quicker a culture bed grows at Culturing `level`.
pub fn bed_quicker_percent(level: u32) -> u32 {
    best(Culturing, level, |p| p == BedQuicker).unwrap_or(0)
}

/// Percent longer a meal keeps the crab well fed at Survival `level`.
pub fn fed_longer_percent(level: u32) -> u32 {
    best(Survival, level, |p| p == FedLonger).unwrap_or(0)
}

/// How many units deeper than its adapted depth pressure strain starts at
/// Survival `level`.
pub fn headroom(level: u32) -> f32 {
    best(Survival, level, |p| p == Headroom).unwrap_or(0) as f32
}

/// Percent faster the paddle legs rest at Swimming `level` (above 10).
pub fn legs_rest_percent(level: u32) -> u32 {
    best(Swimming, level, |p| p == Legs).unwrap_or(0)
}

/// Percent of further claw strength at Fighting `level` (above 10).
pub fn claws_beyond_percent(level: u32) -> u32 {
    best(Fighting, level, |p| p == ClawsBeyond).unwrap_or(0)
}

// ---- Fighting near home: a steeper early curve ----

/// A beaten creature's win experience and a blow's damage experience are
/// multiplied by this at Fighting `level`, to level 15 (a percent): three
/// snapping shrimps stand on the home side (the pockets' two and the row
/// shrimp) and each returns every three minutes, so the early levels are
/// paid richly; the table above 15 is untouched. Was 1,200 / 900 / 700 with
/// two shrimps, then 900 / 650 / 500 while the bot's far-ledge fights were
/// broken.
pub fn fight_xp_percent(level: u32) -> u32 {
    match level {
        0..=4 => 600,
        5..=9 => 450,
        10..=14 => 350,
        _ => 100,
    }
}

/// Survival earns this percent while below level 15 (it grows with time
/// spent deep, which a new crab has little of).
pub fn survival_xp_percent(level: u32) -> u32 {
    if level < LAST_EARLY { 160 } else { 100 }
}

/// Climbing and Swimming earn this percent while below level 15.
pub fn move_xp_percent(level: u32) -> u32 {
    if level < LAST_EARLY { 220 } else { 100 }
}

// ---- The lines ----

fn quicker_line(skill: Skill, n: u32) -> String {
    let verb = match skill {
        Skill::Prying => "pry",
        Skill::Foraging => "pick",
        Skill::Excavating => "dig",
        Skill::Harvesting => "cut",
        _ => "take",
    };
    format!("You {verb} {n}% quicker than at level 1.")
}

impl Perk {
    /// The line a perk's level-up says (the number is the row's).
    pub fn line(self, skill: Skill, n: u32) -> String {
        match self {
            Perk::Quicker => quicker_line(skill, n),
            Perk::Double => {
                format!("{n}% of your gathers at tier 1 nodes give a second one.")
            }
            Perk::Rare => format!("A pearl comes one pry in {n} instead of 64."),
            Perk::Portions(word) => {
                let name = super::ECON_RECIPES
                    .iter()
                    .find(|r| r.word == word)
                    .map_or(word, |r| r.name)
                    .to_lowercase();
                format!("Your {name} make {n} portions a batch.")
            }
            Perk::DoubleCraft(g) => {
                format!("{n}% of your {} come out doubled.", g.name())
            }
            Perk::ToolHold => format!("Tools you make hold {n}% more uses."),
            Perk::BedQuicker => format!("Your culture beds grow {n}% quicker."),
            Perk::Climb => format!("You climb {n}% quicker than at level 1."),
            Perk::Stroke => format!("Your strokes come {n}% quicker than at level 1."),
            Perk::Claws => format!("Your claws hit {n}% harder than at level 1."),
            Perk::Legs => format!("Your paddle legs rest {n}% faster on solid ground."),
            Perk::ClawsBeyond => format!("Your claws hit a further {n}% harder."),
            Perk::Pressure => format!("Pressure strain is {n}% easier on your shell."),
            Perk::FedLonger => format!("A meal keeps you well fed {n}% longer."),
            Perk::Headroom => {
                format!("Your shell takes {n} more depth before pressure strain starts.")
            }
            Perk::FeatReady(feat) => {
                let what = match feat {
                    Feat::GrippingDactyls => "gripping dactyls",
                    Feat::BroadPaddles => "broad paddles",
                    Feat::HeavyCrusher => "a heavy crusher",
                };
                format!("A moult feat comes within reach: {what}.")
            }
        }
    }
}

impl Row {
    /// The line this opening says.
    pub fn line(&self) -> String {
        match self.opening {
            Opening::Recipe(word) => {
                let name = super::ECON_RECIPES
                    .iter()
                    .find(|r| r.word == word)
                    .map_or(word, |r| r.name);
                format!("You can make {}.", name.to_lowercase())
            }
            Opening::Node(kind) => format!("You can work {}.", kind.def().name.to_lowercase()),
            Opening::Piece(piece) => format!(
                "You can build the {} into the home.",
                piece.name().to_lowercase()
            ),
            Opening::Tool(kind, tier) => format!(
                "You can make a {} {}.",
                super::gear::TOOL_MATERIALS[tier as usize],
                kind.name()
            ),
            Opening::Perk(perk, n) => perk.line(self.skill, n),
        }
    }
}

/// The lines for a level of the first hours (2 to 15), if the ladder
/// speaks for it.
pub fn early_lines(skill: Skill, level: u32) -> Option<Vec<String>> {
    if !(FIRST_EARLY..=LAST_EARLY).contains(&level) {
        return None;
    }
    Some(
        LADDER
            .iter()
            .filter(|r| r.skill == skill && r.level == level)
            .map(Row::line)
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_rows_are_in_order() {
        for w in LADDER.windows(2) {
            let (a, b) = (&w[0], &w[1]);
            let ia = Skill::ALL.iter().position(|&s| s == a.skill);
            let ib = Skill::ALL.iter().position(|&s| s == b.skill);
            assert!(ia <= ib, "{:?} before {:?}", a.skill, b.skill);
            if a.skill == b.skill {
                assert!(a.level <= b.level, "{:?} {} {}", a.skill, a.level, b.level);
            }
        }
    }
}
