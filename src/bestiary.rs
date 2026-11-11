//! The ten bestiary creatures' game side: shared puppet helpers, and each creature's plugin
//! (`src/creatures/<name>.rs`). Each puppet is drawn in ink or oil paint like every other (`creature_art.rs`), and
//! leans back in its tell, lunges in its attack and wobbles when staggered unless its own pose says more.
use crate::creatures::*;
use crate::memory::{Known, Remembered};
use bevy::prelude::*;
use crabsim::Sim;
use crabsim::enemy::{Enemy, State};

pub struct BestiaryPlugin;
impl Plugin for BestiaryPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            turtle::TurtlePlugin,
            squid::SquidPlugin,
            viperfish::ViperfishPlugin,
            grouper::GrouperPlugin,
            lobster::LobsterPlugin,
            hermit::HermitPlugin,
            bobbit::BobbitPlugin,
            isopod::IsopodPlugin,
            hagfish::HagfishPlugin,
            amphipods::AmphipodsPlugin,
        ));
    }
}

/// A bestiary creature's puppet root, by enemy id.
#[derive(Component)]
pub struct Puppet(pub u32);

/// How the whole puppet tilts and lunges for the enemy's state: (angle in radians, forward shift in units).
/// Tell leans back (head up), attack lunges forward with the head down, stagger wobbles.
pub fn state_lean(e: &Enemy) -> (f32, f32) {
    let t = e.state_tick as f32;
    let progress = if e.state_len > 0 {
        (t / e.state_len as f32).min(1.0)
    } else {
        0.0
    };
    match e.state {
        State::Tell => (0.15 * e.facing * progress.max(0.3), 0.0),
        State::Attack => (
            -0.1 * e.facing,
            30.0 * e.facing * (progress * std::f32::consts::PI).sin(),
        ),
        State::Stagger => (0.12 * (t * 0.5).sin(), 0.0),
        _ => (0.0, 0.0),
    }
}

/// Show, hide, place and face one puppet from the sim: hidden when its body is eaten or it is not sensed.
pub fn present(
    sim: &Sim,
    known: &Known,
    id: u32,
    transform: &mut Transform,
    visibility: &mut Visibility,
    remembered: &mut Remembered,
) {
    let Some(e) = sim.enemies().iter().find(|e| e.id == id) else {
        *visibility = Visibility::Hidden;
        return;
    };
    let body = !sim.shared.bodies_eaten.contains(&(e.id, e.deaths));
    let shown = (e.state != State::Gone || body) && known.senses_animal(sim, e.pos);
    *visibility = if shown {
        Visibility::Inherited
    } else {
        Visibility::Hidden
    };
    remembered.0 = e.pos;
    let (angle, shift) = state_lean(e);
    transform.translation = (e.pos + Vec2::new(shift, 0.0)).extend(9.0);
    transform.rotation = Quat::from_rotation_z(angle);
    transform.scale = Vec3::new(e.facing, 1.0, 1.0);
}
