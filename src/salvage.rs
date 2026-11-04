//! Shellwash presentation and contextual actions; game rules live in crabsim.
use crate::player::{GameSet, LocalPlayer, PlayerId};
use bevy::prelude::*;
use crabsim::{
    Sim,
    salvage::{FindPhase, SalvageAction, ValvePhase},
};

pub struct SalvagePlugin;
impl Plugin for SalvagePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn)
            .add_systems(Update, act.in_set(GameSet::Input))
            .add_systems(Update, (show_bodies, show_actions).in_set(GameSet::Present));
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
#[derive(Component)]
struct PlacementOutline;
#[derive(Component)]
struct Choice(usize);
#[derive(Component)]
struct ChoiceText(usize);
#[derive(Component)]
struct BankTip;
#[derive(Resource, Default)]
struct Offered([Option<(SalvageAction, &'static str)>; 2]);

/// Leave the portrait inventory tab above the contextual actions. The same
/// rectangle drives presentation and pointer selection.
fn action_rect(index: usize, landscape: bool) -> Rect {
    let x = 10.0 + 150.0 * index as f32;
    let y = if landscape { 145.0 } else { 190.0 };
    Rect::new(x, y, x + 140.0, y + 48.0)
}

fn spawn(mut commands: Commands) {
    commands.insert_resource(Offered::default());
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
    commands.spawn((
        PlacementOutline,
        Sprite::from_color(Color::srgba(0.8, 0.9, 0.7, 0.45), Vec2::new(40.0, 4.0)),
        Transform::from_xyz(0.0, 0.0, 2.2),
        Visibility::Hidden,
    ));
    commands.spawn((
        BankTip,
        Text::new("Dig beneath the seam; clear its path."),
        TextFont {
            font_size: FontSize::Px(15.0),
            ..default()
        },
        TextColor(Color::srgb(0.96, 0.89, 0.72)),
        Node {
            position_type: PositionType::Absolute,
            left: px(12.0),
            top: px(247.0),
            ..default()
        },
        Visibility::Hidden,
    ));
    for i in 0..2 {
        commands
            .spawn((
                crate::input::PointerUi,
                Choice(i),
                Node {
                    position_type: PositionType::Absolute,
                    left: px(10.0 + 150.0 * i as f32),
                    top: px(190.0),
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
                    ChoiceText(i),
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

#[allow(clippy::type_complexity)] // Disjoint filters keep the outline separate from UI choices.
#[allow(clippy::too_many_arguments)] // Independent simulation, inventory and layout resources.
fn show_actions(
    sim: Res<Sim>,
    crab: Single<&PlayerId, With<LocalPlayer>>,
    mut offer: ResMut<Offered>,
    inventory: Res<crate::shell_inventory::Open>,
    window: Single<&Window>,
    mut choices: Query<
        (&Choice, &mut Visibility, &mut Node),
        (Without<PlacementOutline>, Without<BankTip>),
    >,
    mut tip: Single<
        (&mut Visibility, &mut Text, &mut Node),
        (
            With<BankTip>,
            Without<Choice>,
            Without<PlacementOutline>,
            Without<ChoiceText>,
        ),
    >,
    mut texts: Query<(&ChoiceText, &mut Text), Without<BankTip>>,
    mut outline: Single<
        (&mut Transform, &mut Visibility),
        (With<PlacementOutline>, Without<Choice>),
    >,
) {
    offer.0 = [None, None];
    let landscape = window.width() > window.height();
    tip.2.top = px(action_rect(0, landscape).max.y + 9.0);
    for (choice, _, mut node) in &mut choices {
        let rect = action_rect(choice.0, landscape);
        node.left = px(rect.min.x);
        node.top = px(rect.min.y);
    }
    if inventory.0 {
        *outline.1 = Visibility::Hidden;
        publish_placement(None);
        *tip.0 = Visibility::Hidden;
        for (_, mut visible, _) in &mut choices {
            *visible = Visibility::Hidden;
        }
        return;
    }
    let (Some(site), Some(world), Some(c)) = (&sim.salvage, &sim.level.cells, sim.crab(**crab))
    else {
        *outline.1 = Visibility::Hidden;
        publish_placement(None);
        *tip.0 = Visibility::Hidden;
        for (_, mut visible, _) in &mut choices {
            *visible = Visibility::Hidden;
        }
        return;
    };
    let from = c.mover.pos + Vec2::Y * (c.mover.height(&sim.tuning) * 0.5);
    if site.has_find && site.phase == FindPhase::Caught {
        **tip.1 = "Shell caught! Return to the valve".into();
        *tip.0 = Visibility::Inherited;
    } else if site.has_find
        && site.phase == FindPhase::Embedded
        && from.distance(site.position) < 150.0
    {
        **tip.1 = "Dig beneath the seam; clear its path.".into();
        *tip.0 = Visibility::Inherited;
    } else {
        *tip.0 = Visibility::Hidden;
    }
    if site.has_find && site.phase == FindPhase::Carried(**crab) && c.mover.tucked_in() {
        offer.0[0] = Some((SalvageAction::Display, "Display shell"));
    } else if site.has_find && site.phase == FindPhase::Displayed && c.mover.tucked_in() {
        offer.0[0] = Some((SalvageAction::RetrieveDisplay, "Take display"));
    } else if site.has_find && site.phase == FindPhase::Carried(**crab) {
        offer.0[0] = Some((SalvageAction::DropShell, "Set shell down"));
    } else if site.has_valve && site.valve_phase == ValvePhase::Carried(**crab) {
        offer.0[1] = Some((SalvageAction::DropValve, "Drop valve"));
        // Use the physical bowl support rule, including its edge contacts.
        for at in
            crabsim::salvage::Shellwash::valve_candidates(world, c.mover.pos, c.fighter.facing)
        {
            let clear = sim.level.place_at(at).is_none()
                && sim.crabs().iter().all(|other| {
                    (other.mover.pos.x - at.x).abs() > other.mover.half_width(&sim.tuning) + 20.0
                        || (other.mover.pos.y - at.y).abs() > other.mover.height(&sim.tuning)
                });
            let mut probe = site.clone();
            if probe.place_valve(world, **crab, from, at, clear) {
                offer.0[0] = Some((SalvageAction::PlaceValve(at), "Set valve"));
                break;
            }
        }
    } else if site.can_take(world, from) {
        offer.0[0] = Some((SalvageAction::Take, "Take shell"));
        let mut probe = site.clone();
        if probe.crush(world, from) {
            offer.0[1] = Some((SalvageAction::Crush, "Crush to grit"));
        }
    } else if site.has_valve
        && site.valve_phase == ValvePhase::Placed
        && from.distance(site.valve_position) <= 75.0
    {
        offer.0[0] = Some((SalvageAction::LiftValve, "Lift valve"));
    }
    if let Some(slot) = offer.0.iter().position(Option::is_none) {
        for i in 0..2 {
            let room = c.mover.clinging.len() < c.mover.carries(&sim.tuning)
                && c.salvage_receipts.len() < 32;
            let mut probe = site.clone();
            if probe.request_fragment(world, i, from, **crab, c.salvage_identity, room) {
                offer.0[slot] = Some((SalvageAction::TakeFragment(i as u8), "Take grit"));
                break;
            }
        }
    }
    if let Some((SalvageAction::PlaceValve(at), _)) = offer.0[0] {
        publish_placement(Some(at));
        outline.0.translation.x = at.x;
        outline.0.translation.y = at.y;
        *outline.1 = Visibility::Inherited;
    } else {
        publish_placement(None);
        *outline.1 = Visibility::Hidden;
    }
    for (choice, mut v, _) in &mut choices {
        *v = if offer.0[choice.0].is_some() {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
    }
    for (choice, mut text) in &mut texts {
        **text = offer.0[choice.0].map_or("", |(_, label)| label).into();
    }
}

// Read-only preview coordinates mirror the visible outline for browser tests.
fn publish_placement(at: Option<Vec2>) {
    #[cfg(target_arch = "wasm32")]
    if let Some(canvas) = web_sys::window()
        .and_then(|w| w.document())
        .and_then(|d| d.get_element_by_id("crabscape"))
    {
        if let Some(at) = at {
            let value = format!("{:.1} {:.1}", at.x, at.y);
            if canvas.get_attribute("data-salvage-preview").as_deref() != Some(value.as_str()) {
                let _ = canvas.set_attribute("data-salvage-preview", &value);
            }
        } else if canvas.has_attribute("data-salvage-preview") {
            let _ = canvas.remove_attribute("data-salvage-preview");
        }
    }
    #[cfg(not(target_arch = "wasm32"))]
    let _ = at;
}

#[allow(clippy::too_many_arguments)] // Input resources are provided independently by Bevy.
fn act(
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    touches: Res<Touches>,
    window: Single<&Window>,
    settings: Res<crate::settings::panel::Open>,
    time: Res<Time>,
    mut last_grant: Local<f64>,
    offer: Res<Offered>,
    inventory: Res<crate::shell_inventory::Open>,
    crab: Single<&PlayerId, With<LocalPlayer>>,
    mut sim: ResMut<Sim>,
) {
    if settings.is_open() {
        return;
    }
    // A durable owner offer is applied only by its recipient, once. Retry
    // gently if a session handoff delays the ordered command.
    if time.elapsed_secs_f64() - *last_grant >= 1.0 {
        let grant = sim.salvage.as_ref().and_then(|site| {
            sim.crab(**crab).and_then(|c| {
                site.claims.iter().enumerate().find_map(|(i, claim)| {
                    claim
                        .filter(|claim| {
                            claim.phase == crabsim::salvage::ClaimPhase::Offered
                                && claim.recipient_identity == c.salvage_identity
                                && claim.recipient == **crab
                                && !c.salvage_receipts.contains(&claim.id)
                                && c.mover.clinging.len() < c.mover.carries(&sim.tuning)
                        })
                        .map(|claim| SalvageAction::GrantFragment(i as u8, claim.id))
                })
            })
        });
        if let Some(action) = grant {
            *last_grant = time.elapsed_secs_f64();
            if !crate::coop::cells_action(crate::coop::CellsAction::Salvage(action)) {
                sim.salvage_action(**crab, action);
            }
        }
    }
    if inventory.0 {
        return;
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
            (0..2).find(|&i| action_rect(i, window.width() > window.height()).contains(p))
        })
        .or_else(|| keys.just_pressed(KeyCode::KeyG).then_some(0))
        .or_else(|| keys.just_pressed(KeyCode::KeyV).then_some(1));
    if let Some((action, _)) = selected.and_then(|i| offer.0[i])
        && !crate::coop::cells_action(crate::coop::CellsAction::Salvage(action))
    {
        sim.salvage_action(**crab, action);
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
    fn inventory_and_salvage_have_separate_visible_pointer_targets() {
        use bevy::ecs::system::RunSystemOnce;
        for (width, height, y) in [(430, 742, 214.0), (742, 430, 169.0)] {
            let mut app = App::new();
            let mut sim = Sim::with_one_crab(Vec2::new(286.0, 28.0));
            sim.enable_underwater_cells();
            sim.crab_mut(PlayerId(0)).unwrap().salvage_identity = 77;
            app.insert_resource(sim)
                .init_resource::<Time>()
                .init_resource::<ButtonInput<KeyCode>>()
                .init_resource::<ButtonInput<MouseButton>>()
                .init_resource::<Touches>()
                .init_resource::<crate::settings::panel::Open>()
                .init_resource::<crate::shell_inventory::Open>();
            app.world_mut().spawn((PlayerId(0), LocalPlayer));
            let window = app
                .world_mut()
                .spawn(Window {
                    resolution: (width, height).into(),
                    ..default()
                })
                .id();
            app.world_mut().run_system_once(spawn).unwrap();
            app.world_mut().run_system_once(show_actions).unwrap();
            assert!(matches!(
                app.world().resource::<Offered>().0[0],
                Some((SalvageAction::LiftValve, _))
            ));
            let mut actions = app.world_mut().query::<(&Choice, &Node, &Visibility)>();
            let (_, node, visible) = actions
                .iter(app.world())
                .find(|(c, _, _)| c.0 == 0)
                .unwrap();
            assert_eq!(node.top, px(y - 24.0));
            assert_eq!(*visible, Visibility::Inherited);

            app.world_mut()
                .resource_mut::<ButtonInput<MouseButton>>()
                .press(MouseButton::Left);
            if width < height {
                // The inventory tab is still at y=142. Clicking it must not
                // pick up the valve that used to be hidden underneath it.
                app.world_mut()
                    .get_mut::<Window>(window)
                    .unwrap()
                    .set_cursor_position(Some(Vec2::new(80.0, 160.0)));
                app.world_mut().run_system_once(act).unwrap();
                assert_eq!(
                    app.world()
                        .resource::<Sim>()
                        .salvage
                        .as_ref()
                        .unwrap()
                        .valve_phase,
                    ValvePhase::Placed
                );
            }
            app.world_mut()
                .resource_mut::<crate::shell_inventory::Open>()
                .0 = true;
            app.world_mut()
                .get_mut::<Window>(window)
                .unwrap()
                .set_cursor_position(Some(Vec2::new(80.0, y)));
            // Input must reject even the previous frame's still-offered action.
            app.world_mut().run_system_once(act).unwrap();
            assert_eq!(
                app.world()
                    .resource::<Sim>()
                    .salvage
                    .as_ref()
                    .unwrap()
                    .valve_phase,
                ValvePhase::Placed
            );
            app.world_mut().run_system_once(show_actions).unwrap();
            assert!(
                app.world()
                    .resource::<Offered>()
                    .0
                    .iter()
                    .all(Option::is_none)
            );
            assert!(
                actions
                    .iter(app.world())
                    .all(|(_, _, v)| *v == Visibility::Hidden)
            );

            app.world_mut()
                .resource_mut::<crate::shell_inventory::Open>()
                .0 = false;
            app.world_mut().run_system_once(show_actions).unwrap();
            app.world_mut().run_system_once(act).unwrap();
            assert_eq!(
                app.world()
                    .resource::<Sim>()
                    .salvage
                    .as_ref()
                    .unwrap()
                    .valve_phase,
                ValvePhase::Carried(PlayerId(0))
            );
        }
    }

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
    fn presentation_systems_have_disjoint_mutable_queries() {
        let mut app = App::new();
        let mut sim = Sim::with_one_crab(Vec2::new(300.0, 30.0));
        sim.enable_underwater_cells();
        app.insert_resource(sim)
            .init_resource::<crate::shell_inventory::Open>()
            .add_systems(Startup, spawn)
            .add_systems(Update, (show_bodies, show_actions));
        app.world_mut().spawn((PlayerId(0), LocalPlayer));
        app.world_mut().spawn(Window::default());
        app.update();
    }
}
