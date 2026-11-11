//! Authored sediment bodies inside bedrock frames. Thicknesses are world units.
use super::{Level, lower_canyon};
use crate::underwater::Cell;
use bevy_math::{Rect, Vec2};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PocketKind {
    Tunnel,
    Chamber,
}

/// A sediment refuge and its doors, derived from the authored frame.
#[derive(Clone, Debug, PartialEq)]
pub struct Pocket {
    pub name: &'static str,
    pub frame: Rect,
    pub mouths: Vec<Rect>,
    pub kind: PocketKind,
}

fn pockets() -> &'static [Pocket] {
    static POCKETS: std::sync::OnceLock<Vec<Pocket>> = std::sync::OnceLock::new();
    POCKETS.get_or_init(|| {
        lower_canyon::specs()
            .iter()
            .filter_map(|f| {
                Some(Pocket {
                    name: f.name,
                    frame: f.rect,
                    mouths: if f.doors.is_empty() {
                        f.gaps.iter().filter(|g| g.open).map(|g| g.rect).collect()
                    } else {
                        f.doors.clone()
                    },
                    kind: f.kind?,
                })
            })
            .collect()
    })
}

/// The silt shafts' open interiors above a pocket's frame (the home burrow's):
/// sanctuary like the frame, but stopping just under the floor so a walker
/// standing on the ground is never inside one.
fn shaft_interiors<'a>(name: &'a str) -> impl Iterator<Item = Rect> + 'a {
    lower_canyon::specs()
        .iter()
        .filter(move |f| f.name == name)
        .flat_map(|f| f.shafts.iter())
        .map(|r| Rect::new(r.min.x, r.min.y + 20.0, r.max.x, r.max.y - 4.0))
}

#[derive(Clone, Debug, PartialEq)]
pub struct StrataBody {
    pub frame: Rect,
    pub layers: Vec<(Cell, f32)>,
    /// Removed closures must also replace procedural bedrock with sediment.
    pub mouths: Vec<Rect>,
    /// Water cavities (rooms, passages and open doorways) inside the body.
    pub voids: Vec<Rect>,
    /// Silt shafts rising from the body's mouths to the ground: diggable sand.
    pub shafts: Vec<Rect>,
}

fn bodies() -> &'static [StrataBody] {
    static BODIES: std::sync::OnceLock<Vec<StrataBody>> = std::sync::OnceLock::new();
    BODIES.get_or_init(|| {
        lower_canyon::specs()
            .iter()
            .map(|f| StrataBody {
                frame: f.rect,
                mouths: f
                    .gaps
                    .iter()
                    .filter(|g| g.open)
                    .map(|g| g.rect)
                    .chain(f.shafts.iter().copied())
                    .collect(),
                // Owned pebble aggregates cannot be represented by the scalar
                // bake: clay (mud) stands in for every layer.
                layers: vec![(Cell::Mud, f.rect.height())],
                voids: f
                    .voids
                    .iter()
                    .copied()
                    .chain(f.gaps.iter().filter(|g| g.open).map(|g| g.rect))
                    .collect(),
                shafts: f.shafts.clone(),
            })
            .collect()
    })
}

impl Level {
    pub fn pockets(&self) -> Vec<Pocket> {
        self.pocket_refs().cloned().collect()
    }

    pub(crate) fn pocket_refs(&self) -> impl Iterator<Item = &Pocket> {
        pockets().iter().filter(|p| {
            self.rocks
                .iter()
                .any(|r| r.contains(p.frame.min - Vec2::splat(2.0)))
        })
    }

    /// Allocation-free geometry shared with the Tunnel landmarks.
    pub fn tunnel_interiors(&self) -> impl Iterator<Item = Rect> + '_ {
        self.pocket_refs()
            .filter(|p| p.kind == PocketKind::Tunnel)
            .flat_map(|p| std::iter::once(p.frame).chain(shaft_interiors(p.name)))
            .chain(std::iter::once(Rect::new(
                5371.0,
                -1500.0,
                5500.0,
                self.crack.y,
            )))
    }

    /// Derived authoring data, never part of saved Level state. Custom levels
    /// without the corresponding bedrock outline have no strata body there.
    pub fn strata(&self) -> Vec<StrataBody> {
        bodies()
            .iter()
            .filter(|body| body.present(self))
            .cloned()
            .collect()
    }
}

/// Allocation-free scalar counterpart of the column generator.
pub(crate) fn cell_at(level: &Level, x: f32, y: f32) -> Option<Cell> {
    bodies()
        .iter()
        .find_map(|body| body.cell_at(x, y).filter(|_| body.present(level)))
}

impl StrataBody {
    fn present(&self, level: &Level) -> bool {
        level
            .rocks
            .iter()
            .any(|r| r.contains(self.frame.min - Vec2::splat(2.0)))
    }

    /// The authored material at a point (water inside a cavity), if this
    /// body owns it.
    pub fn cell_at(&self, x: f32, y: f32) -> Option<Cell> {
        let inside = |r: &Rect| x >= r.min.x && x < r.max.x && y >= r.min.y && y < r.max.y;
        if self.shafts.iter().any(inside) {
            return Some(Cell::Sand);
        }
        if self.voids.iter().any(inside) {
            return Some(Cell::Water);
        }
        let interior = x > self.frame.min.x
            && x < self.frame.max.x
            && y > self.frame.min.y
            && y < self.frame.max.y;
        if !interior && !self.mouths.iter().any(|r| r.contains(Vec2::new(x, y))) {
            return None;
        }
        let y = y.clamp(self.frame.min.y, self.frame.max.y);
        let mut top = self.frame.max.y;
        for &(material, thickness) in &self.layers {
            top -= thickness;
            if y >= top {
                return Some(material);
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::level::landmarks::Kind;

    #[test]
    fn strata_are_derived_only_for_existing_frames_and_stay_diggable() {
        let mut level = Level::test_level();
        for body in level.strata() {
            assert_eq!(
                body.layers.iter().map(|(_, depth)| depth).sum::<f32>(),
                body.frame.height()
            );
            assert!(
                body.layers.iter().all(|(material, depth)| matches!(
                    material,
                    Cell::Sand | Cell::Mud
                ) && *depth > 0.0)
            );
            for mouth in &body.mouths {
                let p = mouth.center();
                assert!(
                    body.cell_at(p.x, p.y).is_some(),
                    "a mouth the body does not own: {mouth:?}"
                );
            }
            // Boring mode still sees the original solid heightmap here.
            assert!(level.floor_height(body.frame.center().x) > body.frame.max.y);
        }
        level.rocks.clear();
        assert!(level.strata().is_empty());
    }

    #[test]
    fn closed_gaps_are_rock_and_open_gaps_are_not() {
        let level = Level::test_level();
        let places = level.landmarks();
        for spec in lower_canyon::specs() {
            for gap in &spec.gaps {
                let solid = level.rocks.contains(&gap.rect)
                    || level.brittle.iter().any(|b| b.rect == gap.rect);
                assert_eq!(
                    solid,
                    !gap.open || level.brittle.iter().any(|b| b.rect == gap.rect),
                    "{}",
                    spec.name
                );
            }
            let place = places.iter().find(|p| p.name == spec.name).expect("place");
            match spec.kind {
                Some(PocketKind::Tunnel) => assert_eq!(place.kind, Kind::Tunnel, "{}", spec.name),
                Some(PocketKind::Chamber) => {
                    assert!(matches!(place.kind, Kind::Chamber { .. }), "{}", spec.name)
                }
                None => assert_eq!(place.kind, Kind::Frame, "{}", spec.name),
            }
        }
    }
}
