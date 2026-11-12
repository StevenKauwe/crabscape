//! Authored depth beds shared by perception and presentation.
use super::Level;
use bevy_math::{Rect, Vec2};

/// Smell identities and their visual tints are design choices, not measured
/// chemistry claims. Biology is not verified.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Smell {
    Weed,
    Clay,
    Sulfur,
    Metallic,
}

/// Digging-material hints for future bed systems, without changing terrain.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Material {
    Silt,
    Clay,
    Pebbles,
    Crust,
}

/// What the world sounds like in a bed, from the busy shallows to the one low
/// hum of the deepest ground. Presentation reads it every frame; nothing in
/// it is cued on a clock and nothing in it ever goes silent as a warning.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SoundBed {
    /// Many small clicks: snapping shrimp, surge.
    BusyClicks,
    /// Fewer clicks; the crab's own legs start to be heard.
    LegsHeard,
    /// Water running, far calls, the tide.
    WaterAndFarCalls,
    /// Near silence; the crab's shell creaks and bone groans.
    ShellCreak,
    /// One low hum.
    LowHum,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Band {
    pub name: &'static str,
    /// Inclusive upper edge in world y. The first bed extends upward forever.
    pub top_y: f32,
    pub ambient_light: f32,
    pub lantern_reach_factor: f32,
    /// sRGB multiplier on the lantern's existing warm colours. Identity at home.
    pub lantern_tint: [f32; 3],
    /// How busy the world's sound is, from silence to the busy shallows.
    pub ambience: f32,
    pub sound: SoundBed,
    /// A steady hum of the bed itself (0 to 1 of a vent's hum), found by ear
    /// anywhere in it; zero above the deepest ground.
    pub hum: f32,
    pub smell: Smell,
    pub grain: Material,
    /// Intended residents, including future packages. Ranges are design choices,
    /// biology not verified. This does not spawn or relocate animals.
    pub residents: &'static [&'static str],
}

pub const BANDS: [Band; 5] = [
    Band {
        name: "Shallows",
        top_y: f32::INFINITY,
        ambient_light: 1.0,
        lantern_reach_factor: 1.0,
        lantern_tint: [1.0, 1.0, 1.0],
        ambience: 1.0,
        sound: SoundBed::BusyClicks,
        hum: 0.0,
        smell: Smell::Weed,
        grain: Material::Silt,
        residents: &["snapping shrimp", "clams"],
    },
    Band {
        name: "Basin and mudbanks",
        top_y: -260.0,
        ambient_light: 0.45,
        lantern_reach_factor: 0.95,
        lantern_tint: [0.85, 0.95, 1.0],
        ambience: 0.7,
        sound: SoundBed::LegsHeard,
        hum: 0.0,
        smell: Smell::Clay,
        grain: Material::Silt,
        residents: &["mantis", "moray"],
    },
    Band {
        name: "Canyon walls / twilight",
        top_y: -700.0,
        ambient_light: 0.12,
        lantern_reach_factor: 0.82,
        lantern_tint: [0.55, 0.8, 1.0],
        ambience: 0.4,
        sound: SoundBed::WaterAndFarCalls,
        hum: 0.0,
        smell: Smell::Clay,
        grain: Material::Clay,
        residents: &["lanternfish", "octopus", "alarm jelly"],
    },
    Band {
        name: "Canyon floor / midnight",
        top_y: -3400.0,
        ambient_light: 0.03,
        lantern_reach_factor: 0.65,
        lantern_tint: [0.25, 0.55, 1.0],
        ambience: 0.12,
        sound: SoundBed::ShellCreak,
        hum: 0.0,
        smell: Smell::Sulfur,
        grain: Material::Pebbles,
        residents: &["whale-fall scavengers", "sixgill"],
    },
    Band {
        name: "Deep",
        top_y: -9500.0,
        ambient_light: 0.0,
        lantern_reach_factor: 0.55,
        lantern_tint: [0.18, 0.42, 1.0],
        ambience: 0.04,
        sound: SoundBed::LowHum,
        hum: 0.012,
        smell: Smell::Metallic,
        grain: Material::Crust,
        residents: &["seep life", "guardians"],
    },
];

/// Pure depth lookup, independent of tick, player, horizontal position and seed.
/// Borders belong to the deeper bed. Non-finite NaN falls back to shallows.
pub fn band_at(pos: Vec2) -> &'static Band {
    BANDS
        .iter()
        .rev()
        .find(|band| pos.y <= band.top_y)
        .unwrap_or(&BANDS[0])
}

impl Level {
    /// Immutable beds ordered from shallowest to deepest, outside saved terrain.
    pub fn bands(&self) -> &'static [Band] {
        &BANDS
    }
}

/// How a named place finds its bed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Declared {
    /// Always this bed (an index into [`BANDS`]), whatever its depth: a pocket
    /// keeps the bed of the ground it is dug into.
    Bed(usize),
    /// Spans beds or is only a marker: it takes the depth of the crab in it.
    Depth,
}

/// Every named place of the map and its bed, from the bible's table: the
/// shallows and the home burrow; the basin and mudbanks with the mantis
/// burrow; the canyon walls and the bone hall; the canyon floor and lower
/// canyon; the seep garden, vent field and crust vault. Mouths and plugs
/// follow the depth of their door; numbered refuges, nets, rocks and
/// growths follow their depth. `None` means the place has no bed yet.
pub fn declared_bed(name: &str) -> Option<Declared> {
    let name = name
        .trim_end_matches(|c: char| c.is_ascii_digit())
        .trim_end();
    if let Some(declared) = exact_bed(name) {
        return Some(declared);
    }
    // A mouth or a plug belongs to its place, and takes its door's depth.
    let place = name
        .strip_suffix(" plug")
        .or_else(|| name.strip_suffix(" mouth"))?;
    declared_bed(place).map(|_| Declared::Depth)
}

fn exact_bed(name: &str) -> Option<Declared> {
    use Declared::{Bed, Depth};
    Some(match name {
        "home" | "home burrow" => Bed(0),
        "basin" | "basin floor" | "far basin" | "crack mouth" | "mantis mouth" => Bed(1),
        "clay tunnel" | "bone hall" | "canyon doorstep crossing" => Bed(2),
        "canyon deep water" | "far-rim landing" | "far-wall cave" | "octopus den"
        | "pebble warren" | "clay cathedral" | "cathedral seep" | "hanging garden" => Bed(3),
        "vent field" | "seep garden" | "crust vault" | "warm seam" | "chimney hall"
        | "chimney mouth" | "chimney return" | "odd tunnel" | "Plunge shaft" | "Plunge crack"
        | "angler stage" | "vault duct" | "vault shaft" => Bed(4),
        "seep shaft"
        | "long chimney"
        | "long chimney crust plug"
        | "crack chimney"
        | "canyon shelf crossing"
        | "slump dam"
        | "narrows lock"
        | "crevice"
        | "kelp stand"
        | "coral stand"
        | "bare-rock band"
        | "ghost net"
        | "brittle rock"
        | "basin return ledge"
        // The Silt Galleries (`level/silt_galleries.rs`) take their depth's bed.
        | "upper gallery"
        | "sponge gallery"
        | "plug shaft"
        | "silt mouth"
        | "silt gallery plug"
        | "gallery coral"
        | "lower gallery"
        | "moray hall"
        | "drop cavern"
        | "drop ledge"
        | "silt mouth rest"
        | "moray hall door rest"
        | "plug shaft ledge"
        // The shallows (`level/shallows.rs`).
        | "shrimp row"
        | "the shelf"
        | "the trench"
        | "far ledge"
        | "the crest"
        | "ledge pool"
        | "the lookout"
        | "trench hollow"
        // The Drop and the Deep Galleries (`level/deep_galleries.rs`).
        | "drop approach"
        | "drop shaft"
        | "drop coral"
        | "drop top ledge"
        | "lanternfish hall"
        | "black coral hall"
        | "nodule field"
        | "whale-fall hall"
        | "whale fall"
        | "octopus lair"
        | "octopus lair door"
        | "lair passage"
        | "breach shaft"
        | "bone far door"
        | "bone far shaft"
        | "bone far passage"
        | "bone far step"
        | "bone far ledge"
        // The open canyon (`level/open_canyon.rs`).
        | "kelp column"
        | "kelp column foot"
        | "kelp ledge"
        | "tower shelf"
        | "first arch"
        | "second arch"
        | "sponge garden kelp ledge"
        | "sponge garden tower shelf"
        | "sponge garden first arch"
        | "sponge garden second arch" => Depth,
        _ => return None,
    })
}

/// The mud and silt basin regions, as the map's authored rectangles.
fn basin_regions() -> &'static [Rect] {
    static REGIONS: std::sync::OnceLock<Vec<Rect>> = std::sync::OnceLock::new();
    REGIONS.get_or_init(|| {
        Level::test_level()
            .landmarks()
            .into_iter()
            .filter(|l| matches!(l.name.as_str(), "basin" | "basin floor" | "far basin"))
            .map(|l| l.rect)
            .collect()
    })
}

/// How much of the outside world's sound gets through a pocket's walls
/// (1 outside). The crab's own body is never muted.
pub const POCKET_MUTE: f32 = 0.4;

impl Level {
    /// The bed the crab at `pos` lives in, for presentation: sound, smell and
    /// the lantern's colour. Rules (sight, hunters, the economy) keep the
    /// depth-only [`band_at`]. A pocket is the bed of the ground it is dug
    /// into (the home burrow is the shallows' own); the basin's floor, which
    /// sits on the twilight's depth edge, is the basin's.
    pub fn bed_at(&self, pos: Vec2) -> &'static Band {
        let depth = band_at(pos);
        if let Some(pocket) = self.pocket_at(pos)
            && let Some(Declared::Bed(i)) = declared_bed(pocket.name)
        {
            return &BANDS[i];
        }
        if depth.name == BANDS[2].name && basin_regions().iter().any(|r| r.contains(pos)) {
            return &BANDS[1];
        }
        depth
    }

    /// The share of the outside world's sound that reaches `pos`: walls
    /// mute it inside a pocket. Never zero, and never about the crab's body.
    pub fn pocket_mute(&self, pos: Vec2) -> f32 {
        if self.pocket_at(pos).is_some() {
            POCKET_MUTE
        } else {
            1.0
        }
    }
}
