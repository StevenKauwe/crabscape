//! A recessed end-bank cavern: coral approach, guarded find, sand shortcut.
//! Dimensions and juxtaposition are authored geography, not biological claims.
use super::{Brittle, Climb, Growth, Level};
use crate::enemy::Kind;
use bevy_math::{Rect, Vec2};

/// Like the Rubble Banks pocket, this is sediment above a hard rock floor.
/// Saved edited chunks remain authoritative; this only seeds unseen terrain.
pub(crate) fn sediment_depth(x: f32, floor: f32) -> Option<f32> {
    // A flat bottom avoids a bedrock step beneath the sloping bank.
    (17000.0..17600.0)
        .contains(&x)
        .then_some((floor + 2960.0).max(100.0))
}

pub(super) fn shape(level: &mut Level) {
    // The original lower route still reaches 17200 without a wall or jump.
    // An outer bank provides real clearance beyond the optional dig wall.
    level.max_x = 17600.0;
    level.floor.retain(|p| p.x < 16200.0);
    level.floor.extend([
        Vec2::new(16200.0, -3000.0),
        Vec2::new(16900.0, -3000.0),
        Vec2::new(17400.0, -2600.0),
        Vec2::new(17600.0, -2600.0),
    ]);
    // The roof is anchored into the bank wall. Its buried toe can be
    // excavated underneath, while its top is reachable from either side.
    level.rocks.retain(|r| r.min.x != 17330.0);
    level.rocks.extend([
        Rect::new(16600.0, -2440.0, 17330.0, -2200.0),
        Rect::new(17260.0, -2760.0, 17330.0, -2200.0),
    ]);
    level.climbs.push(Climb {
        x: 16480.0,
        bottom: level.floor_height(16480.0),
        top: -2520.0,
        kind: Growth::Coral,
    });
    // Retain the existing end coral, now on the outer bank for a free
    // climbing return if the player does not want to excavate.
    let mouth_floor = level.floor_height(16280.0);
    if let Some(coral) = level.climbs.iter_mut().find(|c| c.x == 16280.0) {
        coral.bottom = mouth_floor;
    }
    let bank_floor = level.floor_height(17480.0);
    if let Some(coral) = level.climbs.iter_mut().find(|c| c.x == 17280.0) {
        coral.x = 17480.0;
        coral.bottom = bank_floor;
        coral.top = -2120.0;
    }
    // A side discovery on the existing roof, not a gate across the cavern.
    // Keep resource and shared rock indices stable by appending both.
    level.brittle.push(Brittle {
        rect: Rect::new(16920.0, -2200.0, 17100.0, -2030.0),
        cracked: false,
    });
    level.cultures.push(Vec2::new(17010.0, -2170.0));
    // Preserve discovery/resource indices and the existing inhabitant slot.
    let find = Vec2::new(17130.0, level.floor_height(17130.0) + 32.0);
    if let Some(culture) = level.cultures.iter_mut().find(|p| p.x == 17280.0) {
        *culture = find;
    }
    let grit_height = level.floor_height(17100.0) + 4.0;
    if let Some(grit) = level.grit.iter_mut().find(|p| p.x == 17100.0) {
        grit.y = grit_height;
    }
    let den = Vec2::new(16800.0, level.floor_height(16800.0));
    if let Some((_, at)) = level
        .foes
        .iter_mut()
        .find(|(kind, at)| *kind == Kind::Mantis && at.x == 16900.0)
    {
        *at = den;
    }
}

impl Level {
    /// A culture sealed inside brittle rubble appears only after its shared
    /// rock breaks. This does not turn nearby reach into a through-rock take.
    pub fn culture_unsealed(&self, at: Vec2) -> bool {
        !self
            .brittle
            .iter()
            .any(|b| !b.cracked && b.rect.contains(at))
    }
}
