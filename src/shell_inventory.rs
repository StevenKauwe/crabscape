//! A small, normally closed shell inventory.
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

#[derive(Component, Clone, Copy)]
enum Choice {
    Open,
    Clams,
    Grit,
    Drop(usize),
    Back,
}

#[derive(Component)]
struct Label(Choice);

fn spawn(mut commands: Commands) {
    commands
        .spawn(Node {
            position_type: PositionType::Absolute,
            top: px(50),
            left: px(8),
            width: px(229),
            flex_wrap: FlexWrap::Wrap,
            row_gap: px(2),
            column_gap: px(3),
            ..default()
        })
        .with_children(|root| {
            for choice in [Choice::Open, Choice::Clams, Choice::Grit]
                .into_iter()
                .chain((0..8).map(Choice::Drop))
                .chain([Choice::Back])
            {
                root.spawn((
                    choice,
                    Node {
                        width: if matches!(choice, Choice::Open) {
                            px(229)
                        } else {
                            px(113)
                        },
                        min_height: px(36),
                        align_items: AlignItems::Center,
                        padding: UiRect::horizontal(px(8)),
                        ..default()
                    },
                    BackgroundColor(Color::srgba(0.05, 0.12, 0.14, 0.88)),
                ))
                .with_children(|button| {
                    button.spawn((
                        Label(choice),
                        Text::new(""),
                        TextFont {
                            font_size: FontSize::Px(14.0),
                            ..default()
                        },
                        TextColor(Color::srgb(1.0, 0.9, 0.7)),
                    ));
                });
            }
        });
}

fn interact(
    mut open: ResMut<Open>,
    mut sim: ResMut<Sim>,
    local: Query<&PlayerId, With<LocalPlayer>>,
    mouse: Res<ButtonInput<MouseButton>>,
    touches: Res<Touches>,
    window: Single<&Window>,
    settings: Res<crate::settings::panel::Open>,
) {
    if settings.is_open() {
        return;
    }
    let Some(id) = local.single().ok().copied() else {
        return;
    };
    let mut taps: Vec<Vec2> = touches
        .iter_just_pressed()
        .map(|touch| touch.position())
        .collect();
    if mouse.just_pressed(MouseButton::Left)
        && let Some(at) = window.cursor_position()
    {
        taps.push(at);
    }
    for at in taps {
        let x = at.x - 8.0;
        let y = at.y - 50.0;
        if !(0.0..229.0).contains(&x) || y < 0.0 {
            continue;
        }
        if y < 36.0 {
            open.0 = !open.0;
            continue;
        }
        if !open.0 || y < 38.0 {
            continue;
        }
        let row = ((y - 38.0) / 38.0) as usize;
        if (y - 38.0) % 38.0 >= 36.0 || (113.0..116.0).contains(&x) {
            continue;
        }
        let index = row * 2 + usize::from(x >= 116.0);
        let carried = sim
            .crab(id)
            .map_or(0, |crab| crab.mover.clinging.len().min(8));
        let action = match index {
            0 => ShellAction::ToggleClams,
            1 => ShellAction::ToggleGrit,
            n if n == carried + 2 => ShellAction::DropBack,
            n => ShellAction::Drop(n - 2),
        };
        if !crate::coop::cells_action(crate::coop::CellsAction::Shell(action)) {
            sim.shell_action(id, action);
        }
    }
}

fn refresh(
    open: Res<Open>,
    sim: Res<Sim>,
    local: Query<&PlayerId, With<LocalPlayer>>,
    mut buttons: Query<(&Choice, &mut Node)>,
    mut labels: Query<(&Label, &mut Text)>,
) {
    let Some(crab) = local.single().ok().and_then(|id| sim.crab(*id)) else {
        return;
    };
    for (choice, mut node) in &mut buttons {
        let show = match choice {
            Choice::Open => true,
            Choice::Clams | Choice::Grit => open.0,
            Choice::Drop(index) => open.0 && *index < crab.mover.clinging.len(),
            Choice::Back => open.0 && crab.mover.carrying.is_some(),
        };
        let display = if show { Display::Flex } else { Display::None };
        if node.display != display {
            node.display = display;
        }
    }
    for (label, mut text) in &mut labels {
        let value = match label.0 {
            Choice::Open => {
                if open.0 {
                    "Shell  ×".into()
                } else {
                    format!("Shell  {}", crab.mover.clinging.len())
                }
            }
            Choice::Clams => format!(
                "Auto clams: {}",
                if crab.pickup.clams { "on" } else { "off" }
            ),
            Choice::Grit => format!("Auto grit: {}", if crab.pickup.grit { "on" } else { "off" }),
            Choice::Drop(index) => match crab.mover.clinging.get(index) {
                Some(Carry::Clam) => "Drop clam".into(),
                Some(Carry::Grit) => "Drop grit".into(),
                Some(Carry::Valve) => "Drop valve".into(),
                _ => "Drop".into(),
            },
            Choice::Back => "Drop back item".into(),
        };
        if text.0 != value {
            text.0 = value;
        }
    }
}
