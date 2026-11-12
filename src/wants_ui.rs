//! The pinned goal while out: one small line under the HUD's skill rows,
//! "Goal: glass-sponge lamp, 2/4 glass", gold once everything is in hand,
//! and a note when `goal_ready` fires (the rules are `crabsim`'s:
//! `Sim::pinned_status`, `StepEvents::goal_ready`). Hidden at home and when
//! nothing is pinned. The task board and the home menu are in `interior.rs`.

use bevy::prelude::*;
use crabsim::Sim;
use crabsim::level::Place;

use crate::notes::say;
use crate::player::{FrameEvents, GameSet, LocalPlayer, PlayerId};

const DIM: Color = Color::srgba(0.85, 0.92, 0.95, 0.75);
const GOLD: Color = Color::srgb(1.0, 0.86, 0.42);
/// How high over the crab the "go home" note starts.
const ABOVE: f32 = 150.0;

pub struct WantsUiPlugin;

impl Plugin for WantsUiPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (show_goal, goal_note).in_set(GameSet::Present));
    }
}

/// The goal line (a child of the skill column in `stamina.rs`).
#[derive(Component)]
pub struct GoalLine;

/// What the goal line says, and whether it is ready (gold), for crab `id`:
/// `None` when nothing is pinned.
pub fn goal_text(sim: &Sim, id: PlayerId) -> Option<(String, bool)> {
    let status = sim.pinned_status(id)?;
    let name = status.want.recipe.name.to_lowercase();
    if status.have_all {
        return Some((format!("Goal: {name}, ready: go home"), true));
    }
    let (item, short) = *status.want.missing.first()?;
    let need = status
        .want
        .recipe
        .needs
        .iter()
        .find(|n| n.0 == item)
        .map_or(short, |n| n.1);
    let (one, many) = item.names();
    let word = if need == 1 { one } else { many };
    Some((
        format!(
            "Goal: {name}, {}/{need} {}",
            need.saturating_sub(short),
            word.to_lowercase()
        ),
        false,
    ))
}

fn show_goal(
    sim: Res<Sim>,
    me: Query<(&PlayerId, &crabsim::Mover), With<LocalPlayer>>,
    mut line: Query<(&mut Text, &mut TextColor, &mut Node), With<GoalLine>>,
) {
    let Ok((id, mover)) = me.single() else {
        return;
    };
    let home = sim.level.place_at(mover.pos) == Some(Place::Home);
    let goal = if home { None } else { goal_text(&sim, *id) };
    for (mut text, mut colour, mut node) in &mut line {
        let display = if goal.is_some() {
            Display::Flex
        } else {
            Display::None
        };
        if node.display != display {
            node.display = display;
        }
        let Some((words, ready)) = &goal else {
            continue;
        };
        if text.0 != *words {
            text.0.clone_from(words);
        }
        colour.set_if_neq(TextColor(if *ready { GOLD } else { DIM }));
    }
}

/// Everything for the pinned goal is in hand: say so over the crab.
fn goal_note(mut commands: Commands, me: Query<(&FrameEvents, &Transform), With<LocalPlayer>>) {
    let Ok((events, at)) = me.single() else {
        return;
    };
    if events.0.goal_ready {
        say(
            &mut commands,
            "You have what you need: go home and make it",
            at.translation.truncate() + Vec2::Y * ABOVE,
            GOLD,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crabsim::Item;

    #[test]
    fn the_goal_line_reads_have_over_need_then_goes_home() {
        let mut sim = Sim::with_one_crab(Vec2::new(900.0, 0.0));
        sim.crab_mut(PlayerId(0)).unwrap().stash.add(Item::Kelp, 0);
        let id = PlayerId(0);
        assert_eq!(goal_text(&sim, id), None, "nothing pinned, nothing shown");
        assert!(sim.pin_want(id, "kelp-cord"));
        let (words, ready) = goal_text(&sim, id).unwrap();
        assert!(!ready);
        assert!(words.starts_with("Goal: kelp cord, 0/2 "), "{words}");
        sim.crab_mut(id).unwrap().stash.add(Item::Kelp, 1);
        let (words, ready) = goal_text(&sim, id).unwrap();
        assert!(!ready);
        assert!(words.starts_with("Goal: kelp cord, 1/2 "), "{words}");
        sim.crab_mut(id).unwrap().stash.add(Item::Kelp, 1);
        let (words, ready) = goal_text(&sim, id).unwrap();
        assert!(ready, "{words}");
        assert!(words.contains("go home"), "{words}");
    }
}
