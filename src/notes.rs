//! Short notes that float up above the crab and fade: "+1 stroke",
//! "Climbing level 2!", "Rested", "Well fed" ... one for each thing worth telling the
//! player that just happened to the local crab.

use bevy::prelude::*;
use crabsim::{Carry, Mover, Sim};

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
    commands.spawn((
        Note { age: 0.0, colour },
        Text2d::new(text),
        TextFont {
            font_size: FontSize::Px(13.0),
            ..default()
        },
        TextColor(colour),
        Transform::from_translation(above.extend(21.0)),
    ));
}

/// The notes this frame's events earn.
fn note_events(
    mut commands: Commands,
    time: Res<Time>,
    sim: Res<Sim>,
    mut last_tangle: Local<f32>,
    crab: Query<(&FrameEvents, &Mover, &Transform, &PlayerId), With<LocalPlayer>>,
) {
    let Ok((events, mover, at, id)) = crab.single() else {
        return;
    };
    let ev = events.0;
    let mut notes: Vec<(String, Color)> = Vec::new();
    if ev.ate {
        notes.push(("+1 stroke".into(), YOLK));
    }
    if ev.fed {
        notes.push(("Ate it: shell mended".into(), YOLK));
    }
    if let Some(skill) = ev.levelled {
        let level = mover.level(skill, &sim.tuning);
        notes.push((format!("{} level {level}!", skill.name()), GOLD));
    }
    if ev.picked_up && !ev.recovered {
        let what = match mover.carrying {
            Some(Carry::Tooth) => ("Take the tooth home!", PALE),
            Some(Carry::Clam) => ("A clam! Take it home.", YOLK),
            _ => ("Take it home!", WILD_GLOW),
        };
        notes.push((what.0.into(), what.1));
    }
    if ev.fitted_scissors {
        notes.push(("Scissor claws!".into(), GOLD));
    }
    if ev.cut_net {
        notes.push(("Cut through!".into(), GOLD));
    }
    // Pressing on a net says so now and then, not every frame.
    let now = time.elapsed_secs();
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
        say(
            &mut commands,
            &text,
            at.translation.truncate() + Vec2::Y * lift,
            colour,
        );
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
