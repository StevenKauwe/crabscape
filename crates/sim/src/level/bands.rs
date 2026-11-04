//! Authored depth beds shared by perception and presentation.
use super::Level;
use bevy_math::Vec2;

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
