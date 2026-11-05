//! Authored sediment bodies inside bedrock frames. Thicknesses are world units.
use super::{Level, lower_canyon};
use crate::underwater::Cell;
use bevy_math::{Rect, Vec2};

#[derive(Clone, Debug, PartialEq)]
pub struct StrataBody {
    pub frame: Rect,
    pub layers: Vec<(Cell, f32)>,
    /// Removed closures must also replace procedural bedrock with sediment.
    pub mouths: Vec<Rect>,
}

fn bodies() -> &'static [StrataBody] {
    static BODIES: std::sync::OnceLock<Vec<StrataBody>> = std::sync::OnceLock::new();
    BODIES.get_or_init(|| {
        lower_canyon::frames()
            .into_iter()
            .filter(|(name, _)| {
                matches!(
                    *name,
                    "home burrow" | "mantis mouth" | "clay tunnel" | "bone hall"
                )
            })
            .map(|(name, frame)| StrataBody {
                frame,
                mouths: lower_canyon::plugs(name, frame),
                // Owned pebble aggregates cannot be represented by the scalar bake.
                layers: vec![
                    (
                        if matches!(name, "home burrow" | "mantis mouth") {
                            Cell::Sand
                        } else {
                            Cell::Mud
                        },
                        frame.height() * 0.4,
                    ),
                    (Cell::Mud, frame.height() * 0.6),
                ],
            })
            .collect()
    })
}

impl Level {
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

    pub(crate) fn cell_at(&self, x: f32, y: f32) -> Option<Cell> {
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
                assert!(matches!(
                    body.cell_at(p.x, p.y),
                    Some(Cell::Sand | Cell::Mud)
                ));
            }
            // Boring mode still sees the original solid heightmap here.
            assert!(level.floor_height(body.frame.center().x) > body.frame.max.y);
        }
        level.rocks.clear();
        assert!(level.strata().is_empty());
    }

    #[test]
    fn only_the_four_strata_frames_open_their_plugs() {
        let level = Level::test_level();
        for (name, frame) in lower_canyon::frames() {
            let open = matches!(
                name,
                "home burrow" | "mantis mouth" | "clay tunnel" | "bone hall"
            );
            for plug in lower_canyon::plugs(name, frame) {
                assert_eq!(
                    level.rocks.contains(&plug) || level.brittle.iter().any(|b| b.rect == plug),
                    !open,
                    "{name}"
                );
            }
            if open {
                let places = level.landmarks();
                let place = places.iter().find(|p| p.name == name).expect("place");
                assert!(matches!(place.kind, Kind::Tunnel | Kind::Chamber { .. }));
                assert!(
                    places
                        .iter()
                        .filter(|p| p.kind == Kind::Mouth && p.name.starts_with(name))
                        .count()
                        >= 2
                );
            }
        }
        assert!(level.strata().iter().all(|b| b.frame.min.y >= -10000.0));
    }
}
