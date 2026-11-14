//! Short notes that float up above the crab and fade: "+1 stroke",
//! "Climbing level 2!", "Rested", "Well fed" ... one for each thing worth telling the
//! player that just happened to the local crab.

use std::collections::HashMap;

use bevy::prelude::*;
use crabsim::{Carry, Fighter, Mover, Recipe, Sim, moult};

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
const HULK_GREEN: Color = Color::srgb(0.55, 1.0, 0.40);
const PALE: Color = Color::srgb(0.80, 0.90, 0.92);

pub struct NotesPlugin;

impl Plugin for NotesPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (note_events, comic_words, hulk_arrivals, float_notes)
                .chain()
                .in_set(GameSet::Present),
        );
    }
}

/// Comic-book words popped over a blow that lands in hulk mode, with their
/// colours; the sim picks which (`StepEvents::hulk_blow`), so a replay and every friend show the same one.
const COMIC: [(&str, Color); 6] = [
    ("SMASH!", Color::srgb(1.0, 0.85, 0.1)),
    ("WHAM!", Color::srgb(1.0, 0.45, 0.1)),
    ("POW!", Color::srgb(1.0, 0.25, 0.3)),
    ("KRAKA-THOOM!", Color::srgb(0.4, 0.95, 0.3)),
    ("BAM!", Color::srgb(0.3, 0.8, 1.0)),
    ("KA-BLAM!", Color::srgb(1.0, 0.5, 0.8)),
];

/// How big a comic word is, and how long its pop (the swell) takes.
const COMIC_SIZE: f32 = 30.0;
const POP: f32 = 0.18;

/// A note that swells as it appears.
#[derive(Component)]
struct Pop;

/// A note floating up and fading.
#[derive(Component)]
struct Note {
    age: f32,
    colour: Color,
    /// Seconds it lasts, and how fast it rises (units/second).
    life: f32,
    rise: f32,
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
        Note {
            age: 0.0,
            colour,
            life: LIFE,
            rise: RISE,
        },
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
#[allow(clippy::too_many_arguments)]
fn note_events(
    mut commands: Commands,
    time: Res<Time>,
    sim: Res<Sim>,
    mut last_tangle: Local<f32>,
    mut last_full: Local<f32>,
    mut last_soft: Local<f32>,
    touch: Res<crate::input::TouchControls>,
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
    if let Some(skill) = ev.proven {
        // "One long climb without letting go! A feat is ready: ...", in two.
        let note = moult::proven_note(skill);
        let (first, then) = note.split_once("! ").unwrap_or((&note, ""));
        notes.push((format!("{first}!"), GOLD));
        notes.push((then.into(), GOLD));
    }
    // A level gained is the HUD's level card (`hud.rs`).
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
        let what: (String, _) = match thing {
            Carry::Tooth => ("Take the tooth home!".into(), PALE),
            Carry::Clam => ("A clam: meal or field portions.".into(), YOLK),
            Carry::Grit => ("Shell grit for the home".into(), PALE),
            Carry::Valve => ("An empty valve for the workbench".into(), PALE),
            Carry::Glass => ("Glass! Take it home.".into(), PALE),
            Carry::Culture => ("Take it home!".into(), WILD_GLOW),
            // Found, not announced: no text over the Plunge.
            Carry::Item(crabsim::Item::WildLight) => (String::new(), PALE),
            Carry::Item(item) => (format!("Took a {}", item.names().0), PALE),
        };
        if !what.0.is_empty() {
            notes.push((what.0, what.1));
        }
    }
    // Walking over a small thing with no room on its shell says so now and
    // then, not every frame.
    if ev.shell_full && now - *last_full > 4.0 {
        *last_full = now;
        notes.push(("Shell full: take it home".into(), PALE));
    }
    if let Some(why) = ev.gather_blocked {
        notes.push((crate::gather_ui::why_not(why), PALE));
    }
    if let Some(kind) = ev.tool_cracked {
        notes.push((format!("Your {} cracked!", kind.name()), GOLD));
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
        // The Silt Galleries' plug is dug open from below, not cut: name the
        // dig control the player has (#750: a player under it didn't find it).
        if under_plug(mover.pos) {
            notes.push(("The plug: dig up to open it".into(), GOLD));
            notes.push((plug_dig_words(touch.visible).into(), GOLD));
        } else {
            notes.push(("A ghost net. Something sharp".into(), PALE));
            notes.push(("could cut through it.".into(), PALE));
        }
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
        notes.push(("The larder is full: clams go to the stash".into(), PALE));
    }
    if ev.rested {
        notes.push(("Rested".into(), PALE));
    }
    if ev.wakes_here {
        notes.push(("A safe crevice.".into(), PALE));
        notes.push(("You'll wake here.".into(), PALE));
    }
    if ev.ate_portion {
        notes.push(("Ate a prepared portion".into(), GOLD));
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

/// A comic word over each hulk blow that lands, on every crab: a friend's too,
/// with the word the attacker's device picked, so everyone sees the same one.
fn comic_words(mut commands: Commands, crabs: Query<(&FrameEvents, &Fighter, &Transform)>) {
    for (events, fighter, at) in &crabs {
        let Some(word) = events.0.hulk_blow else {
            continue;
        };
        let (word, colour) = COMIC[usize::from(word) % COMIC.len()];
        let above = at.translation + Vec3::new(fighter.facing * 70.0, 60.0, 0.0);
        let z = (at.translation.z + 12.0).max(22.0);
        commands.spawn((
            Note {
                age: 0.0,
                colour,
                life: LIFE,
                rise: RISE,
            },
            Pop,
            Text2d::new(word),
            TextFont {
                font_size: FontSize::Px(COMIC_SIZE),
                ..default()
            },
            TextColor(colour),
            Transform::from_translation(above.with_z(z)).with_scale(Vec3::splat(0.2)),
        ));
    }
}

/// "HULK SMASH!" over a friend who bulks up (the crab's own player sees it
/// from the code they typed).
fn hulk_arrivals(
    mut commands: Commands,
    mut was: Local<HashMap<PlayerId, bool>>,
    friends: Query<(&PlayerId, &Mover, &Transform), Without<LocalPlayer>>,
) {
    for (id, mover, at) in &friends {
        let before = was.insert(*id, mover.hulk).unwrap_or(mover.hulk);
        if mover.hulk && !before {
            let above = at.translation.truncate() + Vec2::Y * ABOVE;
            say(&mut commands, "HULK SMASH!", above, HULK_GREEN);
        }
    }
}

/// Float the notes up, fade them out, then remove them.
fn float_notes(
    mut commands: Commands,
    time: Res<Time>,
    mut notes: Query<(Entity, &mut Note, &mut Transform, &mut TextColor, Has<Pop>)>,
) {
    let dt = time.delta_secs();
    for (entity, mut note, mut at, mut text, pops) in &mut notes {
        note.age += dt;
        if note.age >= note.life {
            commands.entity(entity).despawn();
            continue;
        }
        if pops {
            // Swells past full size and settles.
            let k = (note.age / POP).min(1.0);
            at.scale = Vec3::splat(0.2 + 1.0 * (1.0 - (1.0 - k) * (1.0 - k)) + 0.15 * (1.0 - k));
        }
        at.translation.y += note.rise * dt;
        // A short note fades all along; a long one holds, then fades out.
        let fade = if note.life > 2.0 {
            (note.life - note.age).clamp(0.0, 1.0)
        } else {
            1.0 - note.age / note.life
        };
        text.0 = note.colour.with_alpha(fade);
    }
}

/// Is a crab at `pos` under the Silt Galleries' plug (in its shaft, below it)?
fn under_plug(pos: Vec2) -> bool {
    let plug = crabsim::level::silt_galleries::PLUG;
    pos.x >= plug.min.x - 40.0
        && pos.x <= plug.max.x + 40.0
        && pos.y < plug.min.y
        && pos.y > plug.min.y - 160.0
}

/// How to dig up, in the controls on screen: the DIG stick on a touch screen, Space and W on a keyboard.
fn plug_dig_words(touch: bool) -> &'static str {
    if touch {
        "Push the DIG stick up"
    } else {
        "Hold Space + W"
    }
}

#[cfg(test)]
mod plug_hint_tests {
    use super::*;

    #[test]
    fn the_plug_hint_names_the_dig_control_and_only_under_the_plug() {
        let plug = crabsim::level::silt_galleries::PLUG;
        assert!(under_plug(Vec2::new(1003.6, -58.2)), "issue #750's crab");
        assert!(
            !under_plug(Vec2::new(plug.center().x, 30.0)),
            "on the shelf"
        );
        assert!(!under_plug(Vec2::new(600.0, -58.0)), "elsewhere");
        assert!(plug_dig_words(true).contains("DIG"));
        assert!(plug_dig_words(false).contains("Space"));
    }
}
