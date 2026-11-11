//! Seep chimneys and the long chimney home (`crabsim::chimney`): inside an
//! open lane the water carries silt, and it shows. A few dozen specks of
//! sediment (marine snow, silt motes) drift with the flow, drawn as small
//! world-space specks only inside the lane's own rectangles, so the ride
//! reads as being taken along. No bubbles, no air, no screen filter.

use bevy::prelude::*;
use crabsim::Sim;
use crabsim::chimney;
use crabsim::ride;

use crate::effects::SoftTextures;
use crate::lantern::{Lit, LitTint};
use crate::player::{GameSet, LocalPlayer};

/// Specks of silt kept for the lanes near the crab.
const MOTES: usize = 56;
/// Motes live within this box round the crab (half-size, units).
const NEAR: f32 = 520.0;
/// A lane further than this from the crab shows nothing.
const SHOW_WITHIN: f32 = 700.0;

pub struct ChimneyPlugin;

impl Plugin for ChimneyPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(Dice(0x9E37_79B9))
            .add_systems(Startup, spawn_motes)
            .add_systems(Update, drift_motes.in_set(GameSet::Present));
    }
}

/// A tiny xorshift generator: presentation only, never the sim's.
#[derive(Resource)]
struct Dice(u32);

impl Dice {
    fn next(&mut self) -> f32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 17;
        self.0 ^= self.0 << 5;
        (self.0 >> 8) as f32 / (1u32 << 24) as f32
    }
    fn range(&mut self, lo: f32, hi: f32) -> f32 {
        lo + (hi - lo) * self.next()
    }
}

/// One speck of silt in a chimney's water.
#[derive(Component)]
struct Mote {
    sway: f32,
    phase: f32,
}

fn spawn_motes(mut commands: Commands, soft: Res<SoftTextures>, mut dice: ResMut<Dice>) {
    for _ in 0..MOTES {
        commands.spawn((
            Mote {
                sway: dice.range(8.0, 30.0),
                phase: dice.range(0.0, 6.3),
            },
            Sprite {
                image: soft.dot.clone(),
                custom_size: Some(Vec2::splat(dice.range(2.5, 5.0))),
                ..default()
            },
            Transform::from_translation(Vec3::new(0.0, 0.0, 4.0)),
            Visibility::Hidden,
            Lit::default(),
            LitTint {
                dark: Color::srgba(0.45, 0.40, 0.34, 0.25),
                bright: Color::srgba(0.95, 0.85, 0.68, 0.85),
            },
        ));
    }
}

/// Where in an open lane near `centre` to put a fresh speck.
fn fresh_spot(sim: &Sim, centre: Vec2, dice: &mut Dice) -> Option<Vec2> {
    let near = Rect::from_center_half_size(centre, Vec2::splat(NEAR));
    let mut rects = Vec::new();
    for lane in chimney::lanes().iter().filter(|l| l.is_open(&sim.level)) {
        for seg in &lane.segments {
            let r = seg.rect;
            let gap = (r.center() - centre).abs() - r.half_size();
            if gap.max_element() <= SHOW_WITHIN {
                let cut = r.intersect(near);
                if !cut.is_empty() {
                    rects.push(cut);
                }
            }
        }
    }
    if rects.is_empty() {
        return None;
    }
    let pick = (dice.next() * rects.len() as f32) as usize % rects.len();
    let r = rects[pick];
    Some(Vec2::new(
        dice.range(r.min.x, r.max.x),
        dice.range(r.min.y, r.max.y),
    ))
}

fn drift_motes(
    time: Res<Time>,
    sim: Res<Sim>,
    crab: Single<&crabsim::Mover, With<LocalPlayer>>,
    mut dice: ResMut<Dice>,
    mut motes: Query<(&Mote, &mut Transform, &mut Visibility)>,
) {
    let dt = time.delta_secs();
    let t = time.elapsed_secs();
    let centre = crab.pos + Vec2::Y * 30.0;
    for (mote, mut tr, mut shown) in &mut motes {
        let at = tr.translation.truncate();
        let flow = ride::flow_at(chimney::lanes(), &sim.level, at, None);
        let stale = flow.is_none() || (at - centre).abs().max_element() > NEAR + 60.0;
        let (at, flow) = if stale {
            // Put it back into a lane near the crab, or hide it.
            match fresh_spot(&sim, centre, &mut dice) {
                Some(p) => (p, ride::flow_at(chimney::lanes(), &sim.level, p, None)),
                None => {
                    shown.set_if_neq(Visibility::Hidden);
                    continue;
                }
            }
        } else {
            (at, flow)
        };
        let Some(flow) = flow else {
            shown.set_if_neq(Visibility::Hidden);
            continue;
        };
        shown.set_if_neq(Visibility::Inherited);
        // Carried with the water, with a little sway across it.
        let dir = flow.velocity.normalize_or_zero();
        let across = Vec2::new(-dir.y, dir.x);
        let wobble = mote.sway * (t * 0.9 + mote.phase).sin();
        let p = at + (flow.velocity + across * wobble) * dt;
        tr.translation.x = p.x;
        tr.translation.y = p.y;
    }
}
