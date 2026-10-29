//! Short notes that float up above the crab and fade: "+1 stroke",
//! "Climbing level 2!", "Rested", "Well fed" ... one for each thing worth telling the
//! player that just happened to the local crab.

use bevy::prelude::*;
use crabsim::{Carry, Mover, Recipe, Sim, moult};

use crate::player::{FrameEvents, GameSet, LocalPlayer, PlayerId};

/// How long a note lasts (seconds), and how fast it rises (units/second).
const LIFE: f32 = 1.8;
const RISE: f32 = 30.0;
/// Where the first note starts, above the crab's feet.
const ABOVE: f32 = 135.0;

const YOLK: Color = Color::srgb(1.0, 0.80, 0.42);
const GOLD: Color = Color::srgb(1.0, 0.88, 0.55);
const WILD_GLOW: Color = Color::srgb(0.45, 0.95, 0.80);
const WARM: Color = Color::srgb(1.0, 0.78, 0.32);
const PALE: Color = Color::srgb(0.80, 0.90, 0.92);

pub struct NotesPlugin;

impl Plugin for NotesPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (note_events, float_notes).chain().in_set(GameSet::Present),
        );
    }
}

/// A note floating up and fading.
#[derive(Component)]
struct Note {
    age: f32,
    colour: Color,
}

/// Float a note up from `above` (a world position), for anything that isn't
/// one of the crab's own events.
pub fn say(commands: &mut Commands, text: &str, above: Vec2, colour: Color) {
    say_at(commands, text, above.extend(21.0), colour);
}

/// [`say`], at a depth: in front of whatever is drawn at `at.z` (the home's
/// cut-away is drawn in front of the level).
fn say_at(commands: &mut Commands, text: &str, at: Vec3, colour: Color) {
    commands.spawn((
        Note { age: 0.0, colour },
        Text2d::new(text),
        TextFont {
            font_size: FontSize::Px(13.0),
            ..default()
        },
        TextColor(colour),
        Transform::from_translation(at),
    ));
}

/// The notes this frame's events earn.
fn note_events(
    mut commands: Commands,
    time: Res<Time>,
    sim: Res<Sim>,
    mut last_tangle: Local<f32>,
    mut last_full: Local<f32>,
    mut last_soft: Local<f32>,
    crab: Query<(&FrameEvents, &Mover, &Transform, &PlayerId), With<LocalPlayer>>,
) {
    let Ok((events, mover, at, id)) = crab.single() else {
        return;
    };
    let ev = events.0;
    let now = time.elapsed_secs();
    let mut notes: Vec<(String, Color)> = Vec::new();
    if ev.ate {
        notes.push(("+1 stroke".into(), YOLK));
    }
    if ev.fed {
        notes.push(("Ate it: shell mended".into(), YOLK));
    }
    if let Some(skill) = ev.levelled {
        let level = mover.level(skill, &sim.tuning);
        if mover.feat_ready(skill, &sim.tuning) {
            // "Climbing level 5! A feat is ready: moult to grow it.", in two.
            let note = moult::ready_note(skill, level);
            let (first, then) = note.split_once("! ").unwrap_or((&note, ""));
            notes.push((format!("{first}!"), GOLD));
            notes.push((then.into(), GOLD));
        } else {
            notes.push((format!("{} level {level}!", skill.name()), GOLD));
        }
    }
    if ev.moulted {
        notes.push(("Moulted: soft for a while.".into(), PALE));
        notes.push(("Rest tucked in to harden.".into(), PALE));
    }
    if let Some(feat) = ev.hardened {
        let note = moult::hardened_note(feat);
        let (first, then) = note.split_once(": ").unwrap_or((&note, ""));
        notes.push((format!("{first}:"), GOLD));
        notes.push((then.into(), GOLD));
    }
    if ev.too_soft && now - *last_soft > 4.0 {
        *last_soft = now;
        notes.push(("Too soft to eat yet".into(), PALE));
    }
    if let Some(thing) = ev.picked_up
        && !ev.recovered
    {
        let what = match thing {
            Carry::Tooth => ("Take the tooth home!", PALE),
            Carry::Clam => ("A clam! Take it home.", YOLK),
            Carry::Grit => ("Shell grit for the home", PALE),
            Carry::Glass => ("Glass! Take it home.", PALE),
            Carry::Culture => ("Take it home!", WILD_GLOW),
        };
        notes.push((what.0.into(), what.1));
    }
    // Walking over a small thing with no room on its shell says so now and
    // then, not every frame.
    if ev.shell_full && now - *last_full > 4.0 {
        *last_full = now;
        notes.push(("No room on your shell".into(), PALE));
    }
    if ev.fitted_scissors {
        notes.push(("Scissor claws!".into(), GOLD));
    }
    if ev.fitted_grips {
        notes.push(("Glass grips: bare rock holds now".into(), GOLD));
    }
    if ev.cut_net {
        notes.push(("Cut through!".into(), GOLD));
    }
    if ev.cracked_rock {
        notes.push(("The rock cracks open!".into(), GOLD));
    }
    // Pressing on a net says so now and then, not every frame.
    if ev.tangled && now - *last_tangle > 4.0 {
        *last_tangle = now;
        notes.push(("A ghost net. Something sharp".into(), PALE));
        notes.push(("could cut through it.".into(), PALE));
    }
    if ev.recovered {
        notes.push(("Got it back!".into(), GOLD));
    }
    let left_behind = sim.crab(*id).is_some_and(|c| !c.cast_offs.is_empty());
    if ev.respawned && left_behind {
        notes.push(("Your old shell lies where you fell,".into(), PALE));
        notes.push(("with what you carried.".into(), PALE));
    }
    if ev.delivered {
        notes.push(("Lantern brighter!".into(), WARM));
    }
    if ev.stored {
        notes.push(("Into the larder".into(), YOLK));
    }
    if ev.built
        && let Some(me) = sim.crab(*id)
    {
        let t = &sim.tuning;
        let stage = me.home_stage(t);
        let next = t.build.stages.get(stage as usize);
        notes.push(match (ev.home_grew, stage, next) {
            (true, 1, _) => ("The home grows: you mend sooner".into(), GOLD),
            (true, 2, _) => ("The home grows: a bigger larder".into(), GOLD),
            (true, _, _) => ("The home grows: meals last longer".into(), GOLD),
            (false, _, Some(n)) => (format!("Built in: {} more to grow", n - me.home_grit), WARM),
            (false, _, None) => ("Built into the home".into(), WARM),
        });
    }
    if ev.put_away {
        notes.push(("Put away by the workbench".into(), PALE));
    }
    if ev.made == Some(Recipe::ClamGuards) {
        notes.push(("Clam-shell guards!".into(), GOLD));
    }
    if ev.larder_full {
        notes.push(("The larder is full".into(), PALE));
    }
    if ev.rested {
        notes.push(("Rested".into(), PALE));
    }
    if ev.wakes_here {
        notes.push(("A safe crevice.".into(), PALE));
        notes.push(("You'll wake here.".into(), PALE));
    }
    if ev.ate_clam {
        notes.push(("Cracked a clam: well fed".into(), GOLD));
        notes.push(("Two valves for the workbench".into(), PALE));
    }
    if ev.hungry {
        notes.push(("The meal wore off".into(), PALE));
    }
    if ev.found_clue == Some(0) {
        notes.push(("A tooth bigger than you.".into(), PALE));
        notes.push(("Something huge sheds these.".into(), PALE));
    }
    // The first note on top, so several read in order.
    let count = notes.len();
    for (i, (text, colour)) in notes.into_iter().enumerate() {
        let lift = ABOVE + 24.0 * (count - 1 - i) as f32;
        let above = at.translation + Vec3::Y * lift;
        let z = (at.translation.z + 11.0).max(21.0);
        say_at(&mut commands, &text, above.with_z(z), colour);
    }
}

/// Float the notes up, fade them out, then remove them.
fn float_notes(
    mut commands: Commands,
    time: Res<Time>,
    mut notes: Query<(Entity, &mut Note, &mut Transform, &mut TextColor)>,
) {
    let dt = time.delta_secs();
    for (entity, mut note, mut at, mut text) in &mut notes {
        note.age += dt;
        if note.age >= LIFE {
            commands.entity(entity).despawn();
            continue;
        }
        at.translation.y += RISE * dt;
        text.0 = note.colour.with_alpha(1.0 - note.age / LIFE);
    }
}
