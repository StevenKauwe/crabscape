//! Fighting, as the player sees it: the shell (health) and poise bars in the
//! top-left corner, a flash where the crab's blow lands, and short notes for
//! a parry, a counter, a cracked guard. It only reads the simulation.

mod hints;

use bevy::prelude::*;

use crabsim::combat::Claw;
use crabsim::enemy::State;
use crabsim::{Fighter, Kind, Mover, Sim};

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
        app.add_plugins(hints::HintPlugin)
            .add_systems(Startup, (spawn_bars, spawn_flash, spawn_counter))
            .add_systems(
                Update,
                (show_bars, show_flash, show_trail, show_counter, fight_notes)
                    .in_set(GameSet::Present),
            );
    }
}

#[derive(Component)]
struct ShellFill;

/// The shell bar's frame: longer while the shell holds more (well fed).
#[derive(Component)]
struct ShellBack;

/// The shell bar's length for a plain shell.
const SHELL_BAR: f32 = 120.0;

#[derive(Component)]
struct PoiseFill;

/// The shell bar (and a thin poise bar under it), right of the reserve bar.
fn spawn_bars(mut commands: Commands) {
    let bar = |commands: &mut ChildSpawnerCommands, height: f32, colour: Color, fill: bool| {
        let mut back = commands.spawn((
            Node {
                width: px(SHELL_BAR),
                height: px(height),
                border: UiRect::all(px(1)),
                border_radius: BorderRadius::all(px(3)),
                overflow: Overflow::clip(),
                ..default()
            },
            BorderColor::all(colour.with_alpha(0.45)),
        ));
        if fill {
            back.insert(ShellBack);
        }
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

/// The shell bar's frame, its fill and the poise bar's fill: each never
/// one of the others.
type BackOnly = (With<ShellBack>, Without<ShellFill>, Without<PoiseFill>);
type ShellOnly = (With<ShellFill>, Without<PoiseFill>, Without<ShellBack>);
type PoiseOnly = (With<PoiseFill>, Without<ShellFill>, Without<ShellBack>);

fn show_bars(
    sim: Res<Sim>,
    crab: Query<(&Fighter, &Mover), With<LocalPlayer>>,
    mut back: Query<&mut Node, BackOnly>,
    mut shell: Query<&mut Node, ShellOnly>,
    mut poise: Query<&mut Node, PoiseOnly>,
) {
    let Ok((f, mover)) = crab.single() else {
        return;
    };
    let t = &sim.tuning.crab;
    let set = |node: &mut Node, share: f32| {
        let w = percent(100.0 * share.clamp(0.0, 1.0));
        if node.width != w {
            node.width = w;
        }
    };
    // Well fed, the shell holds more: a longer bar.
    let whole = mover.max_shell(&sim.tuning);
    // Hulk changes capacity temporarily, not the HUD footprint.
    let displayed = if mover.hulk {
        whole / crabsim::codes::HULK_SHELL
    } else {
        whole
    };
    for mut n in &mut back {
        let w = px(SHELL_BAR * displayed / t.health);
        if n.width != w {
            n.width = w;
        }
    }
    for mut n in &mut shell {
        set(&mut n, f.vitals.health / whole);
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
        *fade = if f.crush.is_some() {
            // Clamped on: a low glow, so the squeeze shows.
            fade.clamp(0.2, 0.35)
        } else if ev.0.struck || strong || landed {
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
    if let (Some(b), Some(s), None) = (f.hitbox(m, &sim.tuning), &f.swing, f.crush) {
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

/// A hint, once per creature kind, the first time one starts its tell near
/// the crab: what to do about it.
fn hint(kind: Kind) -> Option<&'static str> {
    match kind {
        Kind::Snapper => Some(
            "Snapping shrimp send a floor shock. JUMP or DODGE.\nAfter repeated snaps, a lowered claw gives a longer opening.",
        ),
        Kind::Mantis => Some("A mantis holds still before it strikes.\nTap GUARD then to parry."),
        Kind::Moray => Some("A moray coils before its bite.\nDODGE, or tap GUARD to parry."),
        Kind::Octopus => Some("Octopus: DODGE the pounce and web.\nGUARD its reaching arm tips."),
        Kind::Triggerfish => Some(
            "Nose down: water jet. Hold GUARD to brace.\nThen DODGE or tap GUARD for the bite.",
        ),
        Kind::Monkey => Some("Space monkeys!\nWatch those long asparagus arms."),
        Kind::AlarmJelly => Some(
            "GUARD the trailing tentacle. Strike or bump the bell to flash blue.\nA nearby idle hunter investigates the light; slip past below.",
        ),
        Kind::Fixture => None,
    }
}

#[derive(Default)]
struct EncounterMemory {
    kinds: Vec<Kind>,
    regions: [bool; 3],
    silt_current: bool,
    roof_cache: bool,
    downward_dive: bool,
}

fn fight_notes(
    mut commands: Commands,
    sim: Res<Sim>,
    crab: Query<(&FrameEvents, &Fighter, &Mover, &Transform), With<LocalPlayer>>,
    mut cracked_before: Local<bool>,
    mut hinted: Local<EncounterMemory>,
    mut grabbed_before: Local<bool>,
    mut instructions: MessageWriter<hints::ShowHint>,
) {
    let Ok((ev, f, m, at)) = crab.single() else {
        return;
    };
    // Geography is visible before an encounter starts. Give each new area
    // one brief orientation hint, using the same phone-safe instruction UI.
    if !hinted.downward_dive
        && (3300.0..4300.0).contains(&m.pos.x)
        && m.pos.y < -800.0
        && m.vel.y < 0.0
        && !m.grounded
        && f.dead.is_none()
    {
        hinted.downward_dive = true;
        instructions.write(hints::ShowHint::Encounter(
            "Hold down for an edge-on dive.\nOn touch, pull the TURN stick down. JUMP paddles up.",
        ));
    }
    let region = if m.pos.x >= 14900.0 {
        Some((
            2,
            "Coral Rise\nA culture hides in the bank. Climb above, or dig below.",
        ))
    } else if m.pos.x >= 12000.0 {
        Some((
            1,
            "Silt Hollow\nCoral crowns lead to a sponge. Clam beds lie below.",
        ))
    } else if m.pos.x >= 8900.0 {
        Some((
            0,
            "Rubble Banks\nA tooth lies beneath the rock cap. Dig around its edge.",
        ))
    } else {
        None
    };
    if let Some((index, words)) = region
        && !hinted.regions[index]
        && f.dead.is_none()
    {
        hinted.regions[index] = true;
        instructions.write(hints::ShowHint::Encounter(words));
    }
    if !hinted.silt_current
        && (13200.0..13600.0).contains(&m.pos.x)
        && m.pos.y < -2800.0
        && f.dead.is_none()
    {
        hinted.silt_current = true;
        instructions.write(hints::ShowHint::Encounter(
            "Watch the drifting specks. Jump into rising water.\nCoral on the right climbs past ebb or slack.",
        ));
    }
    if !hinted.roof_cache
        && (16760.0..17200.0).contains(&m.pos.x)
        && m.pos.y > -2300.0
        && f.dead.is_none()
    {
        hinted.roof_cache = true;
        instructions.write(hints::ShowHint::Encounter(
            "Cracked rubble seals a culture.\nHeavy Crusher: hold a full charge, then release. Or jump past.",
        ));
    }
    let grabbed = f.grabbed.is_some();
    if grabbed && !*grabbed_before {
        instructions.write(hints::ShowHint::Grabbed);
    }
    if !grabbed && *grabbed_before {
        instructions.write(hints::ShowHint::Released);
    }
    *grabbed_before = grabbed;
    let near_tell = sim.enemies().iter().find(|e| {
        !grabbed
            && f.dead.is_none()
            && (e.state == State::Tell || e.kind == Kind::AlarmJelly)
            && e.pos.distance_squared(m.pos) < 400.0 * 400.0
            && !hinted.kinds.contains(&e.kind)
    });
    if let Some(e) = near_tell {
        hinted.kinds.push(e.kind);
        if let Some(text) = hint(e.kind) {
            instructions.write(hints::ShowHint::Encounter(text));
        }
    }
    let ev = ev.0;
    let above = at.translation.truncate() + Vec2::Y * 92.0;
    // Later beats of a parry stack higher, so they don't overlap.
    let higher = |n: f32| above + Vec2::Y * 16.0 * n;
    let cracked = f.armour_break > 0;
    let mut say =
        |text: &str, colour: Color, n: f32| notes::say(&mut commands, text, higher(n), colour);
    if ev.dropped_leg {
        say("Dropped a leg! It grows back next moult", SHELL, 0.0);
    }
    if ev.countered {
        say("Counter!", GOLD, 1.0);
    }
    if ev.seized {
        say("Crush!", GOLD, 2.0);
    }
    if cracked && !*cracked_before {
        say("Guard cracked", SHELL, 0.0);
    }
    if ev.staggered {
        say("Staggered", PALE, 0.0);
    }
    if ev.died {
        say("Shell broken...", SHELL, 0.0);
    }
    *cracked_before = cracked;
}

/// Unlike a floating note, this opportunity must disappear when its actual
/// simulation window ends. A parry note used to invite a smash for 1.8 s even
/// though the default game's counter lasts only half a second.
#[derive(Component)]
struct CounterCue;

#[derive(Component)]
struct CounterFill;

const COUNTER_WIDTH: f32 = 86.0;

fn spawn_counter(mut commands: Commands) {
    commands
        .spawn((CounterCue, Transform::default(), Visibility::Hidden))
        .with_children(|c| {
            c.spawn((
                Text2d::new("Parry! SMASH"),
                TextFont {
                    font_size: FontSize::Px(14.0),
                    ..default()
                },
                TextColor(GOLD),
                Transform::default(),
            ));
            c.spawn((
                Sprite::from_color(
                    Color::srgb(0.08, 0.12, 0.13),
                    Vec2::new(COUNTER_WIDTH + 4.0, 6.0),
                ),
                Transform::from_xyz(0.0, -13.0, -0.1),
            ));
            c.spawn((
                CounterFill,
                Sprite::from_color(GOLD, Vec2::new(COUNTER_WIDTH, 2.0)),
                Transform::from_xyz(0.0, -13.0, 0.0),
            ));
        });
}

type CounterCueOnly = (With<CounterCue>, Without<LocalPlayer>);

fn show_counter(
    sim: Res<Sim>,
    crab: Query<(&Fighter, &Transform), With<LocalPlayer>>,
    mut cue: Single<(&mut Transform, &mut Visibility), CounterCueOnly>,
    mut fill: Single<&mut Sprite, With<CounterFill>>,
) {
    let (at, visible) = &mut *cue;
    let Ok((fighter, crab_at)) = crab.single() else {
        **visible = Visibility::Hidden;
        return;
    };
    let ready = fighter.counter > 0
        && fighter.dead.is_none()
        && fighter.grabbed.is_none()
        && fighter.vitals.stagger == 0;
    **visible = if ready {
        Visibility::Inherited
    } else {
        Visibility::Hidden
    };
    if ready {
        at.translation = crab_at.translation + Vec3::new(0.0, 190.0, 12.0);
        let remaining = fighter.counter as f32 / sim.tuning.counter_ticks.max(1) as f32;
        fill.custom_size = Some(Vec2::new(COUNTER_WIDTH * remaining.clamp(0.0, 1.0), 2.0));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counter_cue_tracks_the_simulation_window_and_consumption() {
        let mut app = App::new();
        let mut sim = Sim::with_one_crab(crabsim::SPAWN);
        sim.tuning.counter_ticks = 60;
        let mut fighter = Fighter::new(&sim.tuning);
        fighter.counter = 60;
        app.insert_resource(sim)
            .add_systems(Startup, spawn_counter)
            .add_systems(Update, show_counter);
        let crab = app
            .world_mut()
            .spawn((LocalPlayer, fighter, Transform::default()))
            .id();
        app.update();
        let cue = app
            .world_mut()
            .query_filtered::<Entity, With<CounterCue>>()
            .single(app.world())
            .unwrap();
        let fill = app
            .world_mut()
            .query_filtered::<Entity, With<CounterFill>>()
            .single(app.world())
            .unwrap();
        assert_eq!(
            *app.world().get::<Visibility>(cue).unwrap(),
            Visibility::Inherited
        );
        assert_eq!(
            app.world()
                .get::<Sprite>(fill)
                .unwrap()
                .custom_size
                .unwrap()
                .x,
            COUNTER_WIDTH
        );

        app.world_mut().get_mut::<Fighter>(crab).unwrap().counter = 30;
        app.update();
        assert_eq!(
            app.world()
                .get::<Sprite>(fill)
                .unwrap()
                .custom_size
                .unwrap()
                .x,
            COUNTER_WIDTH / 2.0
        );

        // Both expiration and starting the counter set this to zero. The
        // cue must vanish on that frame, not linger like a floating note.
        app.world_mut().get_mut::<Fighter>(crab).unwrap().counter = 0;
        app.update();
        assert_eq!(
            *app.world().get::<Visibility>(cue).unwrap(),
            Visibility::Hidden
        );

        app.world_mut().get_mut::<Fighter>(crab).unwrap().counter = 30;
        app.world_mut().get_mut::<Fighter>(crab).unwrap().dead = Some(0);
        app.update();
        assert_eq!(
            *app.world().get::<Visibility>(cue).unwrap(),
            Visibility::Hidden
        );
    }
}
