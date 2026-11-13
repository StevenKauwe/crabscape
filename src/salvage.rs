//! Shellwash presentation; game rules live in crabsim.
//!
//! Retired from the front door (2026-11-13): the "Take shell / Crush to grit" buttons, their tips, the G / V keys
//! and the owner's fragment-grant retries are gone from the client; the grit heaps cover what they did. The bodies
//! are still drawn (the cells world holds them as solids), the sim's rules, saves and co-op records are
//! untouched, and `acknowledge_save` still settles claims already in a save.
use bevy::prelude::*;
use crabsim::{Sim, salvage::FindPhase, salvage::SalvageAction};

use crate::player::{GameSet, PlayerId};

pub struct SalvagePlugin;
impl Plugin for SalvagePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn)
            .add_systems(Update, show_bodies.in_set(GameSet::Present));
    }
}
#[derive(Component)]
struct ShellPixel(i32, i32);
#[derive(Component)]
struct BankHint(Vec2);
#[derive(Component)]
struct ValvePart(Vec2);
#[derive(Component)]
struct Fragment(usize);
fn spawn(mut commands: Commands) {
    // Individually masked chunks let sediment cover the actual shell rather
    // than drawing an opaque silhouette through the cellular material.
    for y in 0..3 {
        for x in 0..5 {
            if y == 2 && (x == 0 || x == 4) {
                continue;
            }
            commands.spawn((
                ShellPixel(x, y),
                Sprite::from_color(
                    if x % 2 == 0 {
                        Color::srgb(0.86, 0.77, 0.56)
                    } else {
                        Color::srgb(0.56, 0.44, 0.31)
                    },
                    Vec2::splat(4.0),
                ),
                Transform::from_xyz(0.0, 0.0, 2.1),
                Visibility::Hidden,
            ));
        }
    }
    // A restrained exposed seam marks the diggable bank before the shell
    // itself becomes visible through the actual material mask.
    for (offset, size) in [
        (Vec2::new(-8.0, 23.0), Vec2::new(8.0, 2.0)),
        (Vec2::new(3.0, 25.0), Vec2::new(6.0, 2.0)),
        (Vec2::new(11.0, 22.0), Vec2::new(4.0, 2.0)),
    ] {
        commands.spawn((
            BankHint(offset),
            Sprite::from_color(Color::srgb(0.83, 0.69, 0.43), size),
            Transform::from_xyz(0.0, 0.0, 2.2),
            Visibility::Hidden,
        ));
    }
    for (offset, size) in [
        (Vec2::new(0.0, -3.0), Vec2::new(40.0, 4.0)),
        (Vec2::new(-18.0, 2.0), Vec2::new(4.0, 12.0)),
        (Vec2::new(18.0, 2.0), Vec2::new(4.0, 12.0)),
    ] {
        commands.spawn((
            ValvePart(offset),
            Sprite::from_color(Color::srgb(0.63, 0.53, 0.38), size),
            Transform::from_xyz(0.0, 0.0, 2.0),
            Visibility::Hidden,
        ));
    }
    for i in 0..2 {
        commands.spawn((
            Fragment(i),
            Sprite::from_color(Color::srgb(0.76, 0.67, 0.49), Vec2::splat(6.0)),
            Transform::from_xyz(0.0, 0.0, 2.1),
            Visibility::Hidden,
        ));
    }
}

#[allow(clippy::type_complexity)] // Disjoint Bevy queries for three independently updated body parts.
fn show_bodies(
    sim: Res<Sim>,
    mut pixels: Query<
        (&ShellPixel, &mut Transform, &mut Visibility),
        (Without<ValvePart>, Without<Fragment>, Without<BankHint>),
    >,
    mut valve: Query<
        (&ValvePart, &mut Transform, &mut Visibility),
        (Without<ShellPixel>, Without<Fragment>, Without<BankHint>),
    >,
    mut hints: Query<
        (&BankHint, &mut Transform, &mut Visibility),
        (Without<ShellPixel>, Without<Fragment>, Without<ValvePart>),
    >,
    mut fragments: Query<
        (&Fragment, &mut Transform, &mut Visibility),
        (Without<ShellPixel>, Without<ValvePart>, Without<BankHint>),
    >,
) {
    let (Some(site), Some(world)) = (sim.salvage.as_ref(), sim.level.cells.as_ref()) else {
        for (_, _, mut visible) in &mut pixels {
            *visible = Visibility::Hidden;
        }
        for (_, _, mut visible) in &mut valve {
            *visible = Visibility::Hidden;
        }
        for (_, _, mut visible) in &mut hints {
            *visible = Visibility::Hidden;
        }
        for (_, _, mut visible) in &mut fragments {
            *visible = Visibility::Hidden;
        }
        return;
    };
    for (pixel, mut at, mut visible) in &mut pixels {
        let p = site.position + Vec2::new((pixel.0 - 2) as f32 * 4.0, (pixel.1 - 1) as f32 * 4.0);
        at.translation.x = p.x;
        at.translation.y = p.y;
        let shown = site.has_find
            && site.phase != FindPhase::Consumed
            && world.contains_world(p.x, p.y)
            && !world.game_solid(p.x - 1.0, p.y - 1.0, p.x + 1.0, p.y + 1.0);
        *visible = if shown {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
    }
    for (hint, mut at, mut visible) in &mut hints {
        let p = site.position + hint.0;
        at.translation.x = p.x;
        at.translation.y = p.y;
        *visible = if site.has_find
            && site.phase == FindPhase::Embedded
            && world.contains_world(p.x, p.y)
            && !world.game_solid(p.x - 1.0, p.y - 1.0, p.x + 1.0, p.y + 1.0)
        {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
    }
    for (part, mut at, mut visible) in &mut valve {
        let p = site.valve_position + part.0;
        at.translation.x = p.x;
        at.translation.y = p.y;
        *visible = if site.has_valve {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
    }
    for (part, mut at, mut visible) in &mut fragments {
        let p = site.fragment_positions[part.0];
        at.translation.x = p.x;
        at.translation.y = p.y;
        *visible = if site.has_fragments[part.0]
            && site.fragments[part.0]
            && world.contains_world(p.x, p.y)
            && !world.game_solid(p.x - 1.0, p.y - 1.0, p.x + 1.0, p.y + 1.0)
        {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
    }
}

/// Advance only claims proven by the exact saved snapshots, never by the
/// current in-memory state. The session stream orders the resulting actions.
pub(crate) fn acknowledge_save(
    sim: &mut Sim,
    id: PlayerId,
    ack: &crabsim::cell_codec::spatial::Acknowledgement,
    owner: bool,
) {
    use crabsim::salvage::ClaimPhase;
    let mut actions = Vec::new();
    if owner && let Some(records) = &ack.salvage {
        for record in records {
            for (index, fragment) in record.fragments.iter().enumerate() {
                if let Some(claim) = fragment.as_ref().and_then(|f| f.claim)
                    && claim.phase == ClaimPhase::Requested
                {
                    actions.push(SalvageAction::OfferFragment(index as u8, claim.id));
                }
            }
        }
    }
    if let Some(progress) = ack.player.as_ref().and_then(|p| p.progress.as_ref())
        && let Some(site) = &sim.salvage
    {
        for (index, claim) in site.claims.iter().enumerate() {
            if let Some(claim) = claim
                && claim.phase == ClaimPhase::Offered
                && claim.recipient_identity == progress.salvage_identity
                && progress.salvage_receipts.contains(&claim.id)
            {
                actions.push(SalvageAction::FinalizeFragment(index as u8, claim.id));
            }
        }
    }
    for action in actions {
        let local_owns = sim.salvage.as_ref().is_some_and(|site| match action {
            SalvageAction::OfferFragment(i, _) | SalvageAction::FinalizeFragment(i, _) => {
                site.has_fragments.get(i as usize).copied().unwrap_or(false)
            }
            _ => site.has_find,
        });
        let sent = if matches!(action, SalvageAction::OfferFragment(..)) {
            crate::coop::cells_salvage_offer(crate::coop::CellsAction::Salvage(action), local_owns)
        } else {
            crate::coop::cells_action(crate::coop::CellsAction::Salvage(action))
        };
        if !sent && local_owns {
            sim.salvage_action(id, action);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claim_acknowledges_only_exact_durable_snapshots() {
        use crabsim::{
            cell_codec::spatial::Acknowledgement,
            salvage::{ClaimPhase, FindPhase},
        };
        let id = PlayerId(0);
        let mut sim = Sim::with_one_crab(Vec2::new(260.0, 70.0));
        sim.enable_underwater_cells();
        sim.crab_mut(id).unwrap().salvage_identity = 77;
        let site = sim.salvage.as_mut().unwrap();
        site.world_id = 42;
        site.phase = FindPhase::Consumed;
        site.fragments = [true, true];
        site.fragment_positions = [Vec2::new(260.0, 80.0), Vec2::new(264.0, 80.0)];
        let stale = sim.salvage_records();
        assert!(sim.salvage_action(id, SalvageAction::TakeFragment(0)));
        let claim = sim.salvage.as_ref().unwrap().claims[0].unwrap();
        acknowledge_save(
            &mut sim,
            id,
            &Acknowledgement {
                salvage: Some(stale),
                ..Default::default()
            },
            true,
        );
        assert_eq!(
            sim.salvage.as_ref().unwrap().claims[0].unwrap().phase,
            ClaimPhase::Requested
        );
        let pending = sim.salvage_records();
        acknowledge_save(
            &mut sim,
            id,
            &Acknowledgement {
                salvage: Some(pending),
                ..Default::default()
            },
            true,
        );
        assert_eq!(
            sim.salvage.as_ref().unwrap().claims[0].unwrap().phase,
            ClaimPhase::Offered
        );
        let old_progress = sim.progress(id).unwrap();
        assert!(sim.salvage_action(id, SalvageAction::GrantFragment(0, claim.id)));
        let saved = |progress| crabsim::SaveData {
            version: crabsim::SAVE_VERSION,
            x: 0.0,
            y: 0.0,
            memory: None,
            progress: Some(progress),
        };
        acknowledge_save(
            &mut sim,
            id,
            &Acknowledgement {
                player: Some(saved(old_progress)),
                ..Default::default()
            },
            true,
        );
        assert_eq!(
            sim.salvage.as_ref().unwrap().claims[0].unwrap().phase,
            ClaimPhase::Offered
        );
        let committed = sim.progress(id).unwrap();
        acknowledge_save(
            &mut sim,
            id,
            &Acknowledgement {
                player: Some(saved(committed)),
                ..Default::default()
            },
            true,
        );
        assert_eq!(
            sim.salvage.as_ref().unwrap().claims[0].unwrap().phase,
            ClaimPhase::Finalized
        );
    }

    #[test]
    fn the_bodies_still_draw_with_no_actions_offered() {
        let mut app = App::new();
        let mut sim = Sim::with_one_crab(Vec2::new(300.0, 30.0));
        sim.enable_underwater_cells();
        app.insert_resource(sim)
            .add_systems(Startup, spawn)
            .add_systems(Update, show_bodies);
        app.update();
    }
}
