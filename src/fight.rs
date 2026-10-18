//! Fighting, as the player sees it: the shell (health) and poise bars in the
//! top-left corner, a flash where the crab's blow lands, and short notes for
//! a parry, a counter, a cracked guard. It only reads the simulation.

use bevy::prelude::*;

use crabsim::combat::Claw;
use crabsim::{Fighter, Mover, Sim};

use crate::effects::SoftTextures;
use crate::feel::FeelLab;
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
                (show_bars, show_flash, show_trail, fight_notes).in_set(GameSet::Present),
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

/// The streak along a blow: which way it goes, at a glance. A smash comes
/// down from high behind the claw onto the hitbox; a jab thrusts straight
/// out. It flares as the blow comes out and fades just after.
#[derive(Component)]
struct Trail;

fn spawn_flash(mut commands: Commands, soft: Res<SoftTextures>) {
    commands.spawn((
        Trail,
        Sprite {
            image: soft.streak.clone(),
            color: Color::NONE,
            ..default()
        },
        Transform::from_xyz(0.0, 0.0, 12.5),
    ));
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
    lab: Res<FeelLab>,
    crab: Query<(&Mover, &Fighter, &FrameEvents), With<LocalPlayer>>,
    mut flash: Single<(&mut Sprite, &mut Transform), With<Flash>>,
    mut fade: Local<f32>,
) {
    let Ok((m, f, ev)) = crab.single() else {
        return;
    };
    let (sprite, tr) = &mut *flash;
    let j = lab.juice();
    if let Some(b) = f.hitbox(m, &sim.tuning) {
        tr.translation = b.center().extend(12.0);
        let strong = f.swing.as_ref().is_some_and(|s| s.counter);
        // Juiced, a whiff is only a faint streak and a hit a big bloom.
        let landed = !f.swing.as_ref().is_none_or(|s| s.hits.is_empty());
        let grow = if landed && j.flash > 1.0 {
            j.flash * if strong { j.counter.sqrt() } else { 1.0 }
        } else {
            1.0
        };
        sprite.custom_size = Some(b.size() * Vec2::new(1.6, 1.3) * grow);
        let whiff = if j.sound > 0.0 { 0.3 } else { 0.55 };
        *fade = if ev.0.struck || strong || landed {
            1.0
        } else {
            fade.max(whiff)
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

fn show_trail(
    time: Res<Time>,
    sim: Res<Sim>,
    lab: Res<FeelLab>,
    crab: Query<(&Mover, &Fighter), With<LocalPlayer>>,
    mut trail: Single<(&mut Sprite, &mut Transform), With<Trail>>,
    mut fade: Local<f32>,
) {
    let Ok((m, f)) = crab.single() else {
        return;
    };
    let (sprite, tr) = &mut *trail;
    let strength = lab.juice().trail;
    if let (Some(b), Some(s)) = (f.hitbox(m, &sim.tuning), &f.swing) {
        let heavy = s.claw == Claw::Crusher;
        let end =
            b.center() + Vec2::new(f.facing * b.width() * 0.45, if heavy { -6.0 } else { 0.0 });
        let start = if heavy {
            m.pos + Vec2::new(-f.facing * 6.0, 92.0) // high, over the shell
        } else {
            Vec2::new(m.pos.x, b.center().y) // straight out from the body
        };
        let d = end - start;
        tr.translation = ((start + end) / 2.0).extend(12.5);
        tr.rotation = Quat::from_rotation_z(d.y.atan2(d.x));
        sprite.custom_size = Some(Vec2::new(d.length() * 1.1, if heavy { 30.0 } else { 14.0 }));
        *fade = 1.0;
    }
    *fade = (*fade - time.delta_secs() * 7.0).max(0.0);
    sprite.color = Color::srgb(1.0, 0.95, 0.85).with_alpha(0.75 * *fade * strength);
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
    if ev.seized {
        say("Crush!", GOLD);
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
