//! Presentation and contextual controls for the physical Canyon Drift Run.
use crate::player::{GameSet, LocalPlayer, PlayerId};
use bevy::prelude::*;
use crabsim::{
    Sim,
    canyon_drift::{
        CanyonAction, CatcherPhase, DriftProfile,
        run::{Band, PracticePhase, SlotPhase},
    },
};

pub struct CanyonDriftPlugin;
impl Plugin for CanyonDriftPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn)
            .add_systems(Update, input.in_set(GameSet::Input))
            .add_systems(Update, (show_world, show_choices).in_set(GameSet::Present));
    }
}
#[derive(Component)]
enum Part {
    Slot(usize),
    Practice,
    Valve(Vec2),
}
#[derive(Component)]
struct Button(usize);
#[derive(Component)]
struct Label(usize);
#[derive(Component)]
struct Hint;
#[derive(Resource, Default)]
struct Offered([Option<(CanyonAction, &'static str)>; 2]);

fn spawn(mut commands: Commands) {
    commands.insert_resource(Offered::default());
    for i in 0..2 {
        commands.spawn((
            Part::Slot(i),
            Sprite::from_color(Color::WHITE, Vec2::splat(8.0)),
            Transform::from_xyz(0.0, 0.0, 2.2),
            Visibility::Hidden,
        ));
    }
    commands.spawn((
        Part::Practice,
        Sprite::from_color(Color::srgb(0.63, 0.76, 0.71), Vec2::new(9.0, 7.0)),
        Transform::from_xyz(0.0, 0.0, 2.2),
        Visibility::Hidden,
    ));
    for (offset, size) in [
        (Vec2::new(0.0, -3.0), Vec2::new(40.0, 4.0)),
        (Vec2::new(-18.0, 2.0), Vec2::new(4.0, 12.0)),
        (Vec2::new(18.0, 2.0), Vec2::new(4.0, 12.0)),
    ] {
        commands.spawn((
            Part::Valve(offset),
            Sprite::from_color(Color::srgb(0.63, 0.53, 0.38), size),
            Transform::from_xyz(0.0, 0.0, 2.1),
            Visibility::Hidden,
        ));
    }
    commands.spawn((
        Hint,
        Text::new(""),
        TextFont {
            font_size: FontSize::Px(15.0),
            ..default()
        },
        TextColor(Color::srgb(0.96, 0.89, 0.72)),
        Node {
            position_type: PositionType::Absolute,
            left: px(12.0),
            top: px(202.0),
            ..default()
        },
        Visibility::Hidden,
    ));
    for i in 0..2 {
        commands
            .spawn((
                crate::input::PointerUi,
                Button(i),
                Node {
                    position_type: PositionType::Absolute,
                    left: px(10.0 + 150.0 * i as f32),
                    top: px(145.0),
                    width: px(140.0),
                    height: px(48.0),
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    border: UiRect::all(px(1.0)),
                    border_radius: BorderRadius::all(px(12.0)),
                    ..default()
                },
                BorderColor::all(Color::srgb(0.72, 0.65, 0.45)),
                BackgroundColor(Color::srgba(0.03, 0.09, 0.11, 0.9)),
                Visibility::Hidden,
            ))
            .with_children(|p| {
                p.spawn((
                    Label(i),
                    Text::new(""),
                    TextFont {
                        font_size: FontSize::Px(14.0),
                        ..default()
                    },
                    TextColor(Color::WHITE),
                ));
            });
    }
}
fn show_world(
    sim: Res<Sim>,
    mut parts: Query<(&Part, &mut Transform, &mut Sprite, &mut Visibility)>,
) {
    for (part, mut transform, mut sprite, mut visibility) in &mut parts {
        let Some(run) = &sim.canyon else {
            *visibility = Visibility::Hidden;
            continue;
        };
        let (position, profile, shown) = match part {
            Part::Slot(i) => match run.slots[*i].phase {
                SlotPhase::Free(body) | SlotPhase::Caught(body) if run.owned_slots[*i] => {
                    (body.position, Some(body.profile), true)
                }
                _ => (Vec2::ZERO, None, false),
            },
            Part::Practice => (run.practice.position, None, run.has_practice),
            Part::Valve(offset) => (run.catcher.position + *offset, None, run.has_catcher),
        };
        let visible = shown
            && sim.level.cells.as_ref().is_some_and(|world| {
                world.contains_world(position.x, position.y)
                    && !world.game_solid(
                        position.x - 1.0,
                        position.y - 1.0,
                        position.x + 1.0,
                        position.y + 1.0,
                    )
            });
        *visibility = if visible {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        transform.translation.x = position.x;
        transform.translation.y = position.y;
        if let Some(profile) = profile {
            match profile {
                DriftProfile::Flat => {
                    sprite.custom_size = Some(Vec2::new(16.0, 5.0));
                    sprite.color = Color::srgb(0.9, 0.81, 0.62);
                }
                DriftProfile::Dense => {
                    sprite.custom_size = Some(Vec2::new(8.0, 8.0));
                    sprite.color = Color::srgb(0.64, 0.53, 0.40);
                }
            }
        }
    }
}
#[allow(clippy::type_complexity)]
fn show_choices(
    sim: Res<Sim>,
    crab: Single<&PlayerId, With<LocalPlayer>>,
    mut offer: ResMut<Offered>,
    mut buttons: Query<(&Button, &mut Visibility), Without<Hint>>,
    mut labels: Query<(&Label, &mut Text), Without<Hint>>,
    mut hint: Single<(&mut Text, &mut Visibility), (With<Hint>, Without<Label>)>,
) {
    offer.0 = [None, None];
    let Some(run) = &sim.canyon else {
        *hint.1 = Visibility::Hidden;
        for (_, mut v) in &mut buttons {
            *v = Visibility::Hidden;
        }
        return;
    };
    let Some(world) = &sim.level.cells else {
        *hint.1 = Visibility::Hidden;
        return;
    };
    let Some(c) = sim.crab(**crab) else {
        *hint.1 = Visibility::Hidden;
        return;
    };
    let from = c.mover.pos + Vec2::Y * (c.mover.height(&sim.tuning) * 0.5);
    let near = from.distance(crabsim::canyon_drift::run::UPPER_INLET) < 300.0
        || (run.has_catcher && from.distance(run.catcher.position) < 150.0)
        || (run.has_practice && from.distance(run.practice.position) < 150.0);
    if !near {
        *hint.1 = Visibility::Hidden;
        for (_, mut v) in &mut buttons {
            *v = Visibility::Hidden;
        }
        return;
    }
    let tide = match run.band {
        Band::Slack => "Slack",
        Band::Flood => "Flood",
        Band::Ebb => "Ebb",
    };
    let next = match run.upcoming_profile() {
        DriftProfile::Flat => "flat shell",
        DriftProfile::Dense => "dense chip",
    };
    **hint.0 = format!("{tide} · Next: {next}. Practice is reusable.");
    *hint.1 = Visibility::Inherited;
    if matches!(run.practice_phase, PracticePhase::Carried(id, _) if id == **crab)
        && run.has_practice
    {
        offer.0[0] = Some((CanyonAction::DropPractice, "Set practice down"));
    } else if matches!(run.catcher.phase, CatcherPhase::Carried(id, _) if id == **crab)
        && run.has_catcher
    {
        offer.0[1] = Some((CanyonAction::DropCatcher, "Drop catcher"));
        let intended = c.mover.pos.x + 65.0 * c.fighter.facing.signum();
        for offset in std::iter::once(0).chain((1..=8).flat_map(|n| [-4 * n, 4 * n])) {
            let x = intended + offset as f32;
            let Some(y) = world.game_surface_below(x, 16.0, c.mover.pos.y + 60.0) else {
                continue;
            };
            let at = Vec2::new(x, y + 3.0);
            let clear = sim.level.place_at(at).is_none()
                && sim.crabs().iter().all(|other| {
                    (other.mover.pos.x - at.x).abs() > other.mover.half_width(&sim.tuning) + 20.0
                        || (other.mover.pos.y - at.y).abs() > other.mover.height(&sim.tuning)
                });
            if run.clone().place_catcher(**crab, from, at, clear, world) {
                offer.0[0] = Some((CanyonAction::PlaceCatcher(at), "Set catcher"));
                break;
            }
        }
    } else {
        let small_room =
            c.mover.clinging.len() < c.mover.carries(&sim.tuning) && c.drift_receipts.len() < 32;
        for i in 0..2 {
            if offer.0[0].is_none()
                && run
                    .clone()
                    .request(i, **crab, c.salvage_identity, from, small_room, world)
                    .is_some()
            {
                offer.0[0] = Some((CanyonAction::Take(i as u8), "Take drift"));
            }
            if offer.0[1].is_none() && run.clone().release_outflow(i, from, world) {
                offer.0[1] = Some((CanyonAction::ReleaseOutflow(i as u8), "Release outflow"));
            }
        }
        if run.clone().release_practice(from, world) {
            if offer.0[0].is_none() {
                offer.0[0] = Some((CanyonAction::ReleasePractice, "Release practice"));
            } else if offer.0[1].is_none() {
                offer.0[1] = Some((CanyonAction::ReleasePractice, "Release practice"));
            }
        }
        if offer.0[0].is_none()
            && run.clone().lift_practice(
                **crab,
                c.salvage_identity,
                from,
                c.mover.carrying.is_none(),
                world,
            )
        {
            offer.0[0] = Some((CanyonAction::LiftPractice, "Lift practice"));
        }
        if offer.0[0].is_none()
            && run.clone().lift_catcher(
                **crab,
                c.salvage_identity,
                from,
                c.mover.carrying.is_none()
                    && match run.catcher.contents {
                        None => true,
                        Some(i) if i < 2 => small_room,
                        Some(_) => false,
                    },
                world,
            )
        {
            offer.0[0] = Some((CanyonAction::LiftCatcher, "Lift catcher"));
        }
    }
    for (button, mut v) in &mut buttons {
        *v = if offer.0[button.0].is_some() {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
    }
    for (label, mut text) in &mut labels {
        **text = offer.0[label.0].map_or("", |(_, name)| name).into();
    }
}
#[allow(clippy::too_many_arguments)]
fn input(
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    touches: Res<Touches>,
    window: Single<&Window>,
    settings: Res<crate::settings::panel::Open>,
    offer: Res<Offered>,
    crab: Single<&PlayerId, With<LocalPlayer>>,
    time: Res<Time>,
    mut last_receipt: Local<f64>,
    mut sim: ResMut<Sim>,
) {
    if settings.is_open() {
        return;
    }
    if time.elapsed_secs_f64() - *last_receipt >= 1.0 {
        let next = sim.canyon.as_ref().and_then(|run| {
            let crab_state = sim.crab(**crab)?;
            run.slots.iter().enumerate().find_map(|(i, slot)| {
                if !run.owned_slots[i] {
                    return None;
                }
                let SlotPhase::Grant(grant) = slot.phase else {
                    return None;
                };
                if grant.player != **crab || grant.identity != crab_state.salvage_identity {
                    return None;
                }
                match grant.phase {
                    crabsim::canyon_drift::run::GrantPhase::Offered
                        if !crab_state.drift_receipts.contains(&grant.id) =>
                    {
                        Some(CanyonAction::Grant(grant.id))
                    }
                    crabsim::canyon_drift::run::GrantPhase::FinalizedSaved
                        if crab_state.drift_receipts.contains(&grant.id) =>
                    {
                        Some(CanyonAction::ForgetReceipt(grant))
                    }
                    _ => None,
                }
            })
        });
        if let Some(action) = next {
            *last_receipt = time.elapsed_secs_f64();
            if !crate::coop::cells_action(crate::coop::CellsAction::Canyon(action)) {
                sim.canyon_action(**crab, action);
            }
        }
    }
    let point = touches
        .iter_just_pressed()
        .next()
        .map(|t| t.position())
        .or_else(|| {
            mouse
                .just_pressed(MouseButton::Left)
                .then(|| window.cursor_position())
                .flatten()
        });
    let selected = point
        .and_then(|p| {
            (0..2).find(|&i| {
                let x = 10.0 + 150.0 * i as f32;
                Rect::new(x, 145.0, x + 140.0, 193.0).contains(p)
            })
        })
        .or_else(|| keys.just_pressed(KeyCode::KeyG).then_some(0))
        .or_else(|| keys.just_pressed(KeyCode::KeyV).then_some(1));
    if let Some((action, _)) = selected.and_then(|i| offer.0[i])
        && !crate::coop::cells_action(crate::coop::CellsAction::Canyon(action))
    {
        sim.canyon_action(**crab, action);
    }
}

/// Advance only from the exact player and world records committed by storage.
pub(crate) fn acknowledge_save(
    sim: &mut Sim,
    id: PlayerId,
    ack: &crabsim::cell_codec::spatial::Acknowledgement,
    owner: bool,
) {
    use crabsim::canyon_drift::run::{ANCHOR, DriftClaimId, GrantPhase};
    let mut actions: Vec<(CanyonAction, bool)> = Vec::new();
    if owner && let Some(records) = &ack.canyon {
        for record in records {
            for (index, saved) in record.slots.iter().enumerate() {
                let Some(saved) = saved else {
                    continue;
                };
                match saved.slot.phase {
                    SlotPhase::Grant(grant) => match grant.phase {
                        GrantPhase::Requested => {
                            actions.push((CanyonAction::Offer(grant.id), true))
                        }
                        GrantPhase::Finalized => actions.push((
                            CanyonAction::ConfirmFinalized(grant.id, saved.slot.revision),
                            true,
                        )),
                        GrantPhase::ReceiptRetired => actions
                            .push((CanyonAction::Retire(grant.id, saved.slot.revision), true)),
                        GrantPhase::Offered | GrantPhase::FinalizedSaved => {}
                    },
                    SlotPhase::WashedOut => {
                        let claim = DriftClaimId {
                            world: saved.world,
                            anchor: ANCHOR,
                            slot: index as u8,
                            generation: saved.slot.generation,
                        };
                        actions.push((
                            CanyonAction::RetireOutflow(claim, saved.slot.revision),
                            true,
                        ));
                    }
                    _ => {}
                }
            }
        }
    }
    if let Some(progress) = ack.player.as_ref().and_then(|p| p.progress.as_ref())
        && let Some(run) = &sim.canyon
    {
        for (i, slot) in run.slots.iter().enumerate() {
            if !run.owned_slots[i] {
                continue;
            }
            let SlotPhase::Grant(grant) = slot.phase else {
                continue;
            };
            if grant.identity != progress.salvage_identity || grant.player != id {
                continue;
            }
            if grant.phase == GrantPhase::Offered && progress.drift_receipts.contains(&grant.id) {
                actions.push((CanyonAction::Finalize(grant.id), false));
            }
            if grant.phase == GrantPhase::FinalizedSaved
                && !progress.drift_receipts.contains(&grant.id)
                && sim
                    .crab(id)
                    .is_some_and(|crab| !crab.drift_receipts.contains(&grant.id))
            {
                actions.push((CanyonAction::AcknowledgeReceipt(grant.id), false));
            }
        }
    }
    for (action, owner_action) in actions {
        let slot = match action {
            CanyonAction::Offer(id)
            | CanyonAction::Finalize(id)
            | CanyonAction::AcknowledgeReceipt(id)
            | CanyonAction::ConfirmFinalized(id, _)
            | CanyonAction::Retire(id, _)
            | CanyonAction::RetireOutflow(id, _) => id.slot as usize,
            _ => continue,
        };
        let local_owns = sim
            .canyon
            .as_ref()
            .is_some_and(|run| run.owned_slots.get(slot).copied().unwrap_or(false));
        let sent = if owner_action {
            crate::coop::cells_salvage_offer(crate::coop::CellsAction::Canyon(action), local_owns)
        } else {
            crate::coop::cells_action(crate::coop::CellsAction::Canyon(action))
        };
        if !sent && local_owns {
            sim.canyon_action(id, action);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crabsim::{
        Level, Tuning,
        canyon_drift::{
            DriftBody,
            run::{GrantPhase, SlotPhase},
        },
        cell_codec::spatial::Acknowledgement,
        underwater::World,
    };

    #[test]
    fn saved_receipts_drive_the_generation_without_using_stale_acknowledgements() {
        let id = PlayerId(0);
        let level = Level::test_level();
        let mut terrain = World::from_level(&level);
        assert!(terrain.follow_player(&level, 6380.0, -3200.0));
        let mut sim = Sim::new(Tuning::default(), level);
        assert!(sim.restore_cellular_terrain(terrain, 1));
        sim.add_crab(id, Vec2::new(6380.0, -3200.0));
        sim.crab_mut(id).unwrap().salvage_identity = 77;
        sim.salvage.as_mut().unwrap().world_id = 42;
        sim.canyon.as_mut().unwrap().world_id = 42;
        let from = sim.crab(id).unwrap().mover.pos
            + Vec2::Y * (sim.crab(id).unwrap().mover.height(&sim.tuning) * 0.5);
        let body = DriftBody {
            profile: DriftProfile::Dense,
            position: from + Vec2::new(10.0, 0.0),
            velocity: Vec2::ZERO,
        };
        let run = sim.canyon.as_mut().unwrap();
        run.slots[0].generation = 1;
        run.slots[0].location = body.position;
        run.slots[0].phase = SlotPhase::Free(body);
        let stale = sim.canyon_records();
        assert!(sim.canyon_action(id, CanyonAction::Take(0)));
        let claim = match sim.canyon.as_ref().unwrap().slots[0].phase {
            SlotPhase::Grant(g) => g.id,
            other => panic!("{other:?}"),
        };
        acknowledge_save(
            &mut sim,
            id,
            &Acknowledgement {
                canyon: Some(stale),
                ..Default::default()
            },
            true,
        );
        assert!(
            matches!(sim.canyon.as_ref().unwrap().slots[0].phase, SlotPhase::Grant(g) if g.phase == GrantPhase::Requested)
        );
        let saved = sim.canyon_records();
        acknowledge_save(
            &mut sim,
            id,
            &Acknowledgement {
                canyon: Some(saved),
                ..Default::default()
            },
            true,
        );
        assert!(
            matches!(sim.canyon.as_ref().unwrap().slots[0].phase, SlotPhase::Grant(g) if g.phase == GrantPhase::Offered)
        );
        assert!(sim.canyon_action(id, CanyonAction::Grant(claim)));
        let player_with_receipt = sim.progress(id).unwrap();
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
                player: Some(saved(player_with_receipt.clone())),
                ..Default::default()
            },
            true,
        );
        assert!(
            matches!(sim.canyon.as_ref().unwrap().slots[0].phase, SlotPhase::Grant(g) if g.phase == GrantPhase::Finalized)
        );
        let finalized = sim.canyon_records();
        acknowledge_save(
            &mut sim,
            id,
            &Acknowledgement {
                canyon: Some(finalized),
                ..Default::default()
            },
            true,
        );
        let proof = match sim.canyon.as_ref().unwrap().slots[0].phase {
            SlotPhase::Grant(g) => g,
            other => panic!("{other:?}"),
        };
        assert_eq!(proof.phase, GrantPhase::FinalizedSaved);
        assert!(sim.canyon_action(id, CanyonAction::ForgetReceipt(proof)));
        acknowledge_save(
            &mut sim,
            id,
            &Acknowledgement {
                player: Some(saved(player_with_receipt)),
                ..Default::default()
            },
            true,
        );
        assert!(
            matches!(sim.canyon.as_ref().unwrap().slots[0].phase, SlotPhase::Grant(g) if g.phase == GrantPhase::FinalizedSaved)
        );
        let player_without_receipt = sim.progress(id).unwrap();
        acknowledge_save(
            &mut sim,
            id,
            &Acknowledgement {
                player: Some(saved(player_without_receipt)),
                ..Default::default()
            },
            true,
        );
        assert!(
            matches!(sim.canyon.as_ref().unwrap().slots[0].phase, SlotPhase::Grant(g) if g.phase == GrantPhase::ReceiptRetired)
        );
        let terminal = sim.canyon_records();
        acknowledge_save(
            &mut sim,
            id,
            &Acknowledgement {
                canyon: Some(terminal),
                ..Default::default()
            },
            true,
        );
        assert_eq!(
            sim.canyon.as_ref().unwrap().slots[0].phase,
            SlotPhase::Vacant
        );
    }
}
