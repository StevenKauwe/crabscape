//! Fighting, as the player sees it: the shell (health) and poise bars in the
//! top-left corner, a flash where the crab's blow lands, and short notes for
//! a parry, a counter, a cracked guard. It only reads the simulation.

use bevy::prelude::*;

use crabsim::{Fighter, Mover, Sim};

use crate::effects::SoftTextures;
use crate::notes;
use crate::player::{FrameEvents, GameSet, LocalPlayer};

const SHELL: Color = Color::srgb(0.92, 0.42, 0.26);
const POISE: Color = Color::srgb(0.95, 0.9, 0.8);
const GOLD: Color = Color::srgb(1.0, 0.85, 0.4);
const PALE: Color = Color::srgb(0.85, 0.92, 0.95);

pub struct FightPlugin;

impl Plugin for FightPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, (spawn_bars, spawn_flash))
            .add_systems(
                Update,
                (show_bars, show_flash, fight_notes).in_set(GameSet::Present),
            );
    }
}

#[derive(Component)]
struct ShellFill;

#[derive(Component)]
struct PoiseFill;

/// The shell bar (and a thin poise bar under it), right of the reserve bar.
fn spawn_bars(mut commands: Commands) {
    let bar = |commands: &mut ChildSpawnerCommands, height: f32, colour: Color, fill: bool| {
        let mut back = commands.spawn((
            Node {
                width: px(120),
                height: px(height),
                border: UiRect::all(px(1)),
                border_radius: BorderRadius::all(px(3)),
                overflow: Overflow::clip(),
                ..default()
            },
            BorderColor::all(colour.with_alpha(0.45)),
        ));
        back.with_children(|b| {
            let node = Node {
                width: percent(100),
                height: percent(100),
                ..default()
            };
            if fill {
                b.spawn((ShellFill, node, BackgroundColor(colour.with_alpha(0.85))));
            } else {
                b.spawn((PoiseFill, node, BackgroundColor(colour.with_alpha(0.6))));
            }
        });
    };
    commands
        .spawn(Node {
            position_type: PositionType::Absolute,
            top: px(76),
            left: px(140),
            flex_direction: FlexDirection::Column,
            row_gap: px(2),
            ..default()
        })
        .with_children(|c| {
            bar(c, 7.0, SHELL, true);
            bar(c, 4.0, POISE, false);
        });
}

fn show_bars(
    sim: Res<Sim>,
    crab: Query<&Fighter, With<LocalPlayer>>,
    mut shell: Query<&mut Node, (With<ShellFill>, Without<PoiseFill>)>,
    mut poise: Query<&mut Node, (With<PoiseFill>, Without<ShellFill>)>,
) {
    let Ok(f) = crab.single() else {
        return;
    };
    let t = &sim.tuning.crab;
    let set = |node: &mut Node, share: f32| {
        let w = percent(100.0 * share.clamp(0.0, 1.0));
        if node.width != w {
            node.width = w;
        }
    };
    for mut n in &mut shell {
        set(&mut n, f.vitals.health / t.health);
    }
    for mut n in &mut poise {
        set(&mut n, f.vitals.poise / t.poise);
    }
}

/// The streak where the crab's blow is out.
#[derive(Component)]
struct Flash;

fn spawn_flash(mut commands: Commands, soft: Res<SoftTextures>) {
    commands.spawn((
        Flash,
        Sprite {
            image: soft.dot.clone(),
            color: Color::NONE,
            ..default()
        },
        Transform::from_xyz(0.0, 0.0, 12.0),
    ));
}

fn show_flash(
    time: Res<Time>,
    sim: Res<Sim>,
    crab: Query<(&Mover, &Fighter, &FrameEvents), With<LocalPlayer>>,
    mut flash: Single<(&mut Sprite, &mut Transform), With<Flash>>,
    mut fade: Local<f32>,
) {
    let Ok((m, f, ev)) = crab.single() else {
        return;
    };
    let (sprite, tr) = &mut *flash;
    if let Some(b) = f.hitbox(m, &sim.tuning) {
        tr.translation = b.center().extend(12.0);
        sprite.custom_size = Some(b.size() * Vec2::new(1.6, 1.3));
        let strong = f.swing.as_ref().is_some_and(|s| s.counter);
        *fade = if ev.0.struck || strong {
            1.0
        } else {
            fade.max(0.55)
        };
    } else if ev.0.parried {
        // A pale flash at the raised claws.
        let front = m.pos + Vec2::new(f.facing * 40.0, 34.0);
        tr.translation = front.extend(12.0);
        sprite.custom_size = Some(Vec2::splat(70.0));
        *fade = 1.0;
    }
    *fade = (*fade - time.delta_secs() * 5.0).max(0.0);
    sprite.color = Color::srgb(1.0, 0.93, 0.75).with_alpha(0.8 * *fade);
}

fn fight_notes(
    mut commands: Commands,
    crab: Query<(&FrameEvents, &Fighter, &Transform), With<LocalPlayer>>,
    mut cracked_before: Local<bool>,
) {
    let Ok((ev, f, at)) = crab.single() else {
        return;
    };
    let ev = ev.0;
    let above = at.translation.truncate() + Vec2::Y * 92.0;
    let cracked = f.armour_break > 0;
    let mut say = |text: &str, colour: Color| notes::say(&mut commands, text, above, colour);
    if ev.parried {
        say("Parry! Smash now", GOLD);
    }
    if ev.countered {
        say("Counter!", GOLD);
    }
    if cracked && !*cracked_before {
        say("Guard cracked", SHELL);
    }
    if ev.staggered {
        say("Staggered", PALE);
    }
    if ev.died {
        say("Shell broken...", SHELL);
    }
    *cracked_before = cracked;
}
