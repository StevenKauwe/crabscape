//! On-shell cargo and automatic pickup preferences.
use bevy::prelude::*;
use crabsim::{Carry, ShellAction, Sim};

use crate::player::{GameSet, LocalPlayer, PlayerId};

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
struct Open(bool);

#[derive(Component, Clone, Copy, PartialEq, Eq)]
enum Choice {
    Open,
    Pickup,
    Clams,
    Grit,
    Cargo,
    Drop(Carry),
    Back,
}

const ROWS: [Choice; 9] = [
    Choice::Open,
    Choice::Pickup,
    Choice::Clams,
    Choice::Grit,
    Choice::Cargo,
    Choice::Drop(Carry::Clam),
    Choice::Drop(Carry::Grit),
    Choice::Drop(Carry::Valve),
    Choice::Back,
];

const SHORTCUTS: [(Choice, KeyCode); 6] = [
    (Choice::Clams, KeyCode::Digit1),
    (Choice::Grit, KeyCode::Digit2),
    (Choice::Drop(Carry::Clam), KeyCode::Digit3),
    (Choice::Drop(Carry::Grit), KeyCode::Digit4),
    (Choice::Drop(Carry::Valve), KeyCode::Digit5),
    (Choice::Back, KeyCode::Digit6),
];

#[derive(Component)]
pub(crate) struct Panel;

fn spawn(mut commands: Commands) {
    commands
        .spawn((
            Panel,
            GlobalZIndex(3),
            Node {
                position_type: PositionType::Absolute,
                width: px(258),
                flex_wrap: FlexWrap::Wrap,
                column_gap: px(2),
                ..default()
            },
        ))
        .with_children(|panel| {
            for choice in ROWS {
                let heading = matches!(choice, Choice::Pickup | Choice::Cargo);
                let half = matches!(
                    choice,
                    Choice::Clams | Choice::Grit | Choice::Drop(_) | Choice::Back
                );
                panel
                    .spawn((
                        choice,
                        Node {
                            width: px(if half {
                                128
                            } else if matches!(choice, Choice::Open) {
                                155
                            } else {
                                258
                            }),
                            height: px(if heading { 26 } else { 40 }),
                            border_radius: BorderRadius::all(px(if heading { 0 } else { 8 })),
                            border: UiRect::all(px(if heading { 0 } else { 1 })),
                            padding: UiRect::horizontal(px(8)),
                            align_items: AlignItems::Center,
                            ..default()
                        },
                        BorderColor::all(Color::srgb(0.41, 0.46, 0.39)),
                        BackgroundColor(if heading {
                            Color::srgb(0.05, 0.13, 0.15)
                        } else {
                            Color::srgb(0.10, 0.22, 0.24)
                        }),
                    ))
                    .with_children(|row| {
                        row.spawn((
                            Text::new(""),
                            TextFont {
                                font_size: FontSize::Px(13.0),
                                ..default()
                            },
                            TextColor(Color::srgb(1.0, 0.91, 0.73)),
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
    rows: Query<(&Choice, &ComputedNode, &UiGlobalTransform, &Node)>,
) {
    if settings.is_open() {
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
    window: Single<&Window>,
    settings: Res<crate::settings::panel::Open>,
    mut panel: Single<(&mut Node, &mut Visibility), With<Panel>>,
    mut rows: Query<(&Choice, &mut Node, &Children), Without<Panel>>,
    mut labels: Query<&mut Text>,
) {
    let Some(crab) = local.single().ok().and_then(|id| sim.crab(*id)) else {
        return;
    };
    let landscape = window.width() > window.height();
    panel.0.width = px(if open.0 { 258 } else { 155 });
    panel.0.left = px(if landscape {
        (window.width() - 258.0) * 0.5
    } else {
        8.0
    });
    panel.0.top = px(if landscape { 90 } else { 142 });
    panel.1.set_if_neq(if settings.is_open() {
        Visibility::Hidden
    } else {
        Visibility::Inherited
    });
    let touching = crate::input::has_touch_screen();
    let shortcut = if touching { "" } else { "  [B]" };
    let carried = crab.mover.clinging.len() + usize::from(crab.mover.carrying.is_some());
    for (choice, mut node, children) in &mut rows {
        let on = |value| if value { "On" } else { "Off" };
        let caption = match *choice {
            Choice::Open if open.0 => Some(format!("Close inventory{shortcut}")),
            Choice::Open => Some(format!("Inventory   ({carried}){shortcut}")),
            _ if !open.0 => None,
            Choice::Pickup => Some("Pick up automatically".into()),
            Choice::Clams => Some(format!("Clams   {}", on(crab.pickup.clams))),
            Choice::Grit => Some(format!("Grit   {}", on(crab.pickup.grit))),
            Choice::Cargo => Some(if carried == 0 {
                "Carrying nothing".into()
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
        if *choice == Choice::Open {
            node.width = px(if open.0 { 258 } else { 155 });
        }
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
