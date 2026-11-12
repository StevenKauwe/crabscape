//! On-shell cargo and automatic pickup preferences: the cargo chip in the
//! top-left corner, under the vitals ("Cargo 3", B), opens into the shell's
//! list (what clings to it, what rides on its back, what it picks up by
//! itself, and the lantern).
use bevy::prelude::*;
use crabsim::salvage::{FindPhase, ValvePhase};
use crabsim::{Carry, ShellAction, Sim};

use crate::player::{GameSet, LocalPlayer, PlayerId};
use crate::ui_theme as theme;

pub struct ShellInventoryPlugin;

impl Plugin for ShellInventoryPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Open>()
            .add_systems(Startup, spawn)
            .add_systems(Update, interact.in_set(GameSet::Input))
            .add_systems(Update, refresh.in_set(GameSet::Present));
    }
}

#[derive(Resource, Default)]
pub(crate) struct Open(pub(crate) bool);

#[derive(Component, Clone, Copy, PartialEq, Eq)]
enum Choice {
    Open,
    Lantern,
    Pickup,
    Clams,
    Grit,
    Cargo,
    Drop(Carry),
    Back,
    HeldValve,
    HeldShell,
}

const ROWS: [Choice; 12] = [
    Choice::Open,
    Choice::Pickup,
    Choice::Clams,
    Choice::Grit,
    Choice::Cargo,
    Choice::Drop(Carry::Clam),
    Choice::Drop(Carry::Grit),
    Choice::Drop(Carry::Valve),
    Choice::Back,
    Choice::HeldValve,
    Choice::HeldShell,
    // Last, so the rows the WebKit test taps (`tools/webtest`) keep their places.
    Choice::Lantern,
];

const SHORTCUTS: [(Choice, KeyCode); 6] = [
    (Choice::Clams, KeyCode::Digit1),
    (Choice::Grit, KeyCode::Digit2),
    (Choice::Drop(Carry::Clam), KeyCode::Digit3),
    (Choice::Drop(Carry::Grit), KeyCode::Digit4),
    (Choice::Drop(Carry::Valve), KeyCode::Digit5),
    (Choice::Back, KeyCode::Digit6),
];

pub(crate) fn is_shortcut(key: KeyCode) -> bool {
    key == KeyCode::KeyB
        || key == KeyCode::KeyH
        || SHORTCUTS.iter().any(|(_, shortcut)| key == *shortcut)
}

#[derive(Component)]
pub(crate) struct Panel;

/// The closed chip's width, and the open list's.
const CHIP_W: f32 = 118.0;
const LIST_W: f32 = 258.0;

fn spawn(mut commands: Commands) {
    commands
        .spawn((
            crate::input::PointerUi,
            Panel,
            GlobalZIndex(3),
            Node {
                position_type: PositionType::Absolute,
                width: px(LIST_W),
                flex_wrap: FlexWrap::Wrap,
                column_gap: px(2),
                row_gap: px(4),
                ..default()
            },
        ))
        .with_children(|panel| {
            for choice in ROWS {
                let heading = matches!(choice, Choice::Pickup | Choice::Cargo);
                let passive = heading || matches!(choice, Choice::HeldValve | Choice::HeldShell);
                let half = matches!(
                    choice,
                    Choice::Clams | Choice::Grit | Choice::Drop(_) | Choice::Back
                );
                panel
                    .spawn((
                        choice,
                        Node {
                            width: px(if half {
                                128.0
                            } else if matches!(choice, Choice::Open) {
                                CHIP_W
                            } else {
                                LIST_W
                            }),
                            height: px(if heading { 26 } else { 40 }),
                            border_radius: BorderRadius::all(px(if passive { 4 } else { 10 })),
                            border: UiRect::all(px(if passive { 0 } else { 1 })),
                            padding: UiRect::horizontal(px(10)),
                            align_items: AlignItems::Center,
                            ..default()
                        },
                        BorderColor::all(theme::RIM_FAINT),
                        BackgroundColor(if passive { theme::PANEL } else { theme::BUTTON }),
                    ))
                    .with_children(|row| {
                        row.spawn(theme::words(
                            "",
                            if heading { theme::CAPTION } else { theme::BODY },
                            if heading { theme::MUTED } else { theme::TEXT },
                        ));
                    });
            }
        });
}

#[allow(clippy::too_many_arguments)]
fn interact(
    mut open: ResMut<Open>,
    mut sim: ResMut<Sim>,
    local: Query<&PlayerId, With<LocalPlayer>>,
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    touches: Res<Touches>,
    window: Single<&Window>,
    settings: Res<crate::settings::panel::Open>,
    chat: Res<crate::codes::CodeBox>,
    rows: Query<(&Choice, &ComputedNode, &UiGlobalTransform, &Node)>,
) {
    if settings.is_open() || chat.open || crate::input::page_dialog_open() {
        return;
    }
    let Ok(&id) = local.single() else { return };
    let tap = touches
        .iter_just_pressed()
        .next()
        .map(|t| t.position())
        .or_else(|| {
            mouse
                .just_pressed(MouseButton::Left)
                .then(|| window.cursor_position())
                .flatten()
        });
    let choice = if keys.just_pressed(KeyCode::KeyB) {
        Some(Choice::Open)
    } else if keys.just_pressed(KeyCode::KeyH) {
        Some(Choice::Lantern)
    } else {
        rows.iter()
            .find(|(c, b, t, n)| {
                n.display != Display::None
                    && (tap.is_some_and(|p| b.contains_point(**t, p * window.scale_factor()))
                        || SHORTCUTS
                            .iter()
                            .any(|(choice, key)| choice == *c && keys.just_pressed(*key)))
            })
            .map(|(choice, ..)| *choice)
    };
    let action = match choice {
        Some(Choice::Open) => {
            open.0 = !open.0;
            return;
        }
        Some(Choice::Lantern) => ShellAction::ToggleLantern,
        Some(Choice::Clams) if open.0 => ShellAction::ToggleClams,
        Some(Choice::Grit) if open.0 => ShellAction::ToggleGrit,
        Some(Choice::Drop(kind)) if open.0 => {
            let Some(index) = sim
                .crab(id)
                .and_then(|c| c.mover.clinging.iter().position(|item| *item == kind))
            else {
                return;
            };
            ShellAction::Drop(index)
        }
        Some(Choice::Back) if open.0 => ShellAction::DropBack,
        _ => return,
    };
    if !crate::coop::cells_action(crate::coop::CellsAction::Shell(action)) {
        sim.shell_action(id, action);
    }
}

fn name(item: Carry) -> &'static str {
    match item {
        Carry::Item(item) => item.names().0,
        Carry::Clam => "clam",
        Carry::Grit => "grit",
        Carry::Valve => "valve",
        Carry::Culture => "culture",
        Carry::Tooth => "tooth",
        Carry::Glass => "glass",
    }
}

#[allow(clippy::too_many_arguments)]
fn refresh(
    open: Res<Open>,
    sim: Res<Sim>,
    local: Query<&PlayerId, With<LocalPlayer>>,
    settings: Res<crate::settings::panel::Open>,
    mut panel: Single<(&mut Node, &mut Visibility), With<Panel>>,
    mut rows: Query<(&Choice, &mut Node, &Children), Without<Panel>>,
    mut labels: Query<&mut Text>,
) {
    let Some(crab) = local.single().ok().and_then(|id| sim.crab(*id)) else {
        return;
    };
    panel.0.width = px(if open.0 { LIST_W } else { CHIP_W });
    panel.0.left = px(theme::EDGE);
    panel.0.top = px(theme::CARGO_TOP);
    panel.1.set_if_neq(if settings.is_open() {
        Visibility::Hidden
    } else {
        Visibility::Inherited
    });
    let touching = crate::input::has_touch_screen();
    let shortcut = if touching { "" } else { "  [B]" };
    let cargo = crab.mover.clinging.len() + usize::from(crab.mover.carrying.is_some());
    let held_valve = sim
        .salvage
        .as_ref()
        .is_some_and(|site| site.has_valve && site.valve_phase == ValvePhase::Carried(crab.id));
    let held_shell = sim
        .salvage
        .as_ref()
        .is_some_and(|site| site.has_find && site.phase == FindPhase::Carried(crab.id));
    let carried = cargo + usize::from(held_valve) + usize::from(held_shell);
    for (choice, mut node, children) in &mut rows {
        let on = |value| if value { "On" } else { "Off" };
        let caption = match *choice {
            Choice::Open if open.0 => Some(format!("Close cargo{shortcut}")),
            Choice::Open => Some(format!("Cargo  {carried}{shortcut}")),
            _ if !open.0 => None,
            Choice::Lantern => Some(format!(
                "{}Lantern: {} — tap to {}",
                if touching { "" } else { "[H] " },
                if crab.mover.lantern_dim {
                    "DIM"
                } else {
                    "FULL"
                },
                if crab.mover.lantern_dim {
                    "brighten"
                } else {
                    "dim"
                }
            )),
            Choice::Pickup => Some("Pick up automatically".into()),
            Choice::Clams => Some(format!("Clams   {}", on(crab.pickup.clams))),
            Choice::Grit => Some(format!("Grit   {}", on(crab.pickup.grit))),
            Choice::Cargo => Some(if carried == 0 {
                "Carrying nothing".into()
            } else if cargo == 0 {
                "Carried".into()
            } else {
                "Carried - tap to drop one".into()
            }),
            Choice::Drop(kind) => {
                let count = crab
                    .mover
                    .clinging
                    .iter()
                    .filter(|item| **item == kind)
                    .count();
                (count > 0).then(|| format!("{} x{}", name(kind), count))
            }
            Choice::HeldValve => {
                held_valve.then(|| "Valve held\nClose inventory to set down".into())
            }
            Choice::HeldShell => {
                held_shell.then(|| "Shell held\nClose inventory to set down".into())
            }
            Choice::Back => crab
                .mover
                .carrying
                .map(|item| format!("{} (back)", name(item))),
        };
        let caption = caption.map(|text| {
            if !touching
                && let Some(index) = SHORTCUTS.iter().position(|(action, _)| action == choice)
            {
                return format!("[{}] {text}", index + 1);
            }
            text
        });

        let display = if caption.is_some() {
            Display::Flex
        } else {
            Display::None
        };
        if node.display != display {
            node.display = display;
        }
        if let Some(caption) = caption
            && let Ok(mut label) = labels.get_mut(children[0])
            && label.0 != caption
        {
            label.0 = caption;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(app: &mut App, choice: Choice) -> (String, Display) {
        let mut rows = app.world_mut().query::<(&Choice, &Node, &Children)>();
        let (_, node, children) = rows
            .iter(app.world())
            .find(|(kind, ..)| **kind == choice)
            .unwrap();
        (
            app.world().get::<Text>(children[0]).unwrap().0.clone(),
            node.display,
        )
    }

    #[test]
    fn keyboard_and_phone_row_toggle_the_same_local_lantern_action() {
        use bevy::input::touch::{TouchInput, TouchPhase, touch_screen_input_system};
        let mut app = App::new();
        app.insert_resource(Sim::with_one_crab(Vec2::new(12800.0, -3250.0)))
            .init_resource::<Open>()
            .init_resource::<crate::settings::panel::Open>()
            .init_resource::<crate::codes::CodeBox>()
            .init_resource::<ButtonInput<KeyCode>>()
            .init_resource::<ButtonInput<MouseButton>>()
            .init_resource::<Touches>()
            .add_message::<TouchInput>()
            .add_systems(Startup, spawn)
            .add_systems(PreUpdate, touch_screen_input_system)
            .add_systems(Update, (interact, refresh).chain());
        app.world_mut().spawn((PlayerId(0), LocalPlayer));
        let window = app
            .world_mut()
            .spawn(Window {
                resolution: (430, 742).into(),
                ..default()
            })
            .id();
        app.update();
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::KeyH);
        app.update();
        assert!(
            app.world()
                .resource::<Sim>()
                .crab(PlayerId(0))
                .unwrap()
                .mover
                .lantern_dim
        );
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .reset_all();
        app.world_mut().resource_mut::<Open>().0 = true;
        app.update();
        assert!(row(&mut app, Choice::Lantern).0.contains("DIM"));
        assert_eq!(row(&mut app, Choice::Lantern).1, Display::Flex);
        let mut rows = app
            .world_mut()
            .query::<(&Choice, &mut ComputedNode, &mut UiGlobalTransform)>();
        for (choice, mut computed, mut transform) in rows.iter_mut(app.world_mut()) {
            if *choice == Choice::Lantern {
                computed.size = Vec2::new(258.0, 40.0);
                *transform = UiGlobalTransform::from(bevy::math::Affine2::from_translation(
                    Vec2::new(150.0, 180.0),
                ));
            }
        }
        app.world_mut().write_message(TouchInput {
            phase: TouchPhase::Started,
            position: Vec2::new(150.0, 180.0),
            window,
            force: None,
            id: 1,
        });
        app.update();
        assert!(
            !app.world()
                .resource::<Sim>()
                .crab(PlayerId(0))
                .unwrap()
                .mover
                .lantern_dim
        );
        assert!(row(&mut app, Choice::Lantern).0.contains("FULL"));
        app.update();
        assert!(
            !app.world()
                .resource::<Sim>()
                .crab(PlayerId(0))
                .unwrap()
                .mover
                .lantern_dim,
            "held touch must not repeat"
        );
    }

    #[test]
    fn inventory_reports_only_local_held_salvage_without_extra_drop_actions() {
        let mut app = App::new();
        let mut sim = Sim::with_one_crab(Vec2::new(286.0, 28.0));
        sim.enable_underwater_cells();
        app.insert_resource(sim)
            .init_resource::<Open>()
            .init_resource::<crate::settings::panel::Open>()
            .add_systems(Startup, spawn)
            .add_systems(Update, refresh);
        app.world_mut().spawn((PlayerId(0), LocalPlayer));
        app.world_mut().spawn(Window {
            resolution: (430, 742).into(),
            ..default()
        });
        app.update();
        assert!(row(&mut app, Choice::Open).0.contains("Cargo  0"));
        app.world_mut()
            .resource_mut::<Sim>()
            .salvage
            .as_mut()
            .unwrap()
            .valve_phase = ValvePhase::Carried(PlayerId(0));
        app.update();
        assert!(row(&mut app, Choice::Open).0.contains("Cargo  1"));
        app.world_mut().resource_mut::<Open>().0 = true;
        app.update();
        assert_eq!(row(&mut app, Choice::Cargo).0, "Carried");
        assert_eq!(
            row(&mut app, Choice::HeldValve),
            (
                "Valve held\nClose inventory to set down".into(),
                Display::Flex
            )
        );
        assert_eq!(row(&mut app, Choice::Drop(Carry::Valve)).1, Display::None);
        assert_eq!(row(&mut app, Choice::Back).1, Display::None);
        {
            let mut sim = app.world_mut().resource_mut::<Sim>();
            let site = sim.salvage.as_mut().unwrap();
            site.valve_phase = ValvePhase::Carried(PlayerId(1));
            site.phase = FindPhase::Carried(PlayerId(0));
        }
        app.update();
        assert_eq!(row(&mut app, Choice::HeldValve).1, Display::None);
        assert_eq!(row(&mut app, Choice::HeldShell).1, Display::Flex);
        app.world_mut()
            .resource_mut::<Sim>()
            .salvage
            .as_mut()
            .unwrap()
            .phase = FindPhase::Free;
        app.update();
        assert_eq!(row(&mut app, Choice::HeldShell).1, Display::None);
        assert_eq!(row(&mut app, Choice::Cargo).0, "Carrying nothing");
    }
}
