//! The pinned goal while out: a small pill in the top-left corner, under the
//! cargo chip, an amber diamond and "Goal: glass-sponge lamp, 2/4 glass",
//! turning gold once everything is in hand (the rules are `crabsim`'s:
//! `Sim::pinned_status`, `StepEvents::goal_ready`; the "GOAL READY" card is
//! `hud.rs`). Hidden at home and when nothing is pinned. The task board and
//! the home menu are in `interior.rs`.

use bevy::prelude::*;
use crabsim::Sim;
use crabsim::level::Place;

use crate::player::{GameSet, LocalPlayer, PlayerId};
use crate::ui_theme as theme;

pub struct WantsUiPlugin;

impl Plugin for WantsUiPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn)
            .add_systems(Update, show_goal.in_set(GameSet::Present));
    }
}

/// The goal pill, its diamond and its words.
#[derive(Component)]
struct GoalPill;
#[derive(Component)]
struct GoalMark;
#[derive(Component)]
pub struct GoalLine;

fn spawn(mut commands: Commands) {
    commands
        .spawn((
            GoalPill,
            Node {
                display: Display::None,
                position_type: PositionType::Absolute,
                top: px(theme::GOAL_TOP),
                left: px(theme::EDGE),
                max_width: px(250),
                align_items: AlignItems::Center,
                column_gap: px(8),
                padding: UiRect::new(px(10), px(12), px(5), px(5)),
                border: UiRect::all(px(1)),
                border_radius: BorderRadius::all(px(14)),
                ..default()
            },
            BackgroundColor(theme::BUTTON),
            BorderColor::all(theme::RIM_FAINT),
        ))
        .with_children(|pill| {
            // The goal's diamond: the HUD's mark for "what you're after".
            pill.spawn((
                GoalMark,
                Node {
                    width: px(8),
                    height: px(8),
                    flex_shrink: 0.0,
                    border: UiRect::all(px(1.5)),
                    ..default()
                },
                UiTransform::from_rotation(Rot2::degrees(45.0)),
                BorderColor::all(theme::AMBER),
                BackgroundColor(Color::NONE),
            ));
            pill.spawn((
                GoalLine,
                theme::words("", theme::CAPTION + 1.0, theme::TEXT),
            ));
        });
}

/// What the guide line says while nothing else is pinned: the current
/// first-hour step, with its count when it has one. `None` once the guide is
/// skipped or done.
pub fn guide_text(sim: &Sim, id: PlayerId) -> Option<String> {
    let status = sim.guide_status(id).filter(|s| !s.skipped)?;
    let step = status.step?;
    if guide_shell_full(sim, id) {
        let count = status
            .progress
            .map_or(String::new(), |(have, want)| format!(", {have}/{want}"));
        return Some(format!("Guide: shell full, take it home{count}"));
    }
    Some(match status.progress {
        Some((have, want)) => format!("Guide: {}, {have}/{want}", step.title.to_lowercase()),
        None => format!("Guide: {}", step.title.to_lowercase()),
    })
}

/// Whether the guide's gather step can't go on until crab `id` takes its
/// load home: its shell has no room for what the step's node gives (walking
/// in at the door unloads it). The guide line and its marker say so.
pub fn guide_shell_full(sim: &Sim, id: PlayerId) -> bool {
    let Some(step) = sim
        .guide_status(id)
        .filter(|s| !s.skipped)
        .and_then(|s| s.step)
    else {
        return false;
    };
    let crabsim::economy::guide::Cond::Gather(kind, _) = step.cond else {
        return false;
    };
    let Some(item) = kind.def().yields else {
        return false;
    };
    sim.crab(id)
        .is_some_and(|c| !c.mover.room_for(crabsim::Carry::Item(item), &sim.tuning))
}

/// What the goal line says, and whether it is ready (gold), for crab `id`:
/// `None` when nothing is pinned.
pub fn goal_text(sim: &Sim, id: PlayerId) -> Option<(String, bool)> {
    let status = sim.pinned_status(id)?;
    let name = status.want.name.to_lowercase();
    if status.have_all {
        return Some((format!("Goal: {name}, ready: go home"), true));
    }
    let (item, short) = *status.want.missing.first()?;
    let need = status
        .want
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

#[allow(clippy::type_complexity)]
fn show_goal(
    sim: Res<Sim>,
    me: Query<(&PlayerId, &crabsim::Mover), With<LocalPlayer>>,
    mut pill: Single<(&mut Node, &mut BorderColor), (With<GoalPill>, Without<GoalMark>)>,
    mut mark: Single<(&mut BorderColor, &mut BackgroundColor), (With<GoalMark>, Without<GoalPill>)>,
    mut line: Single<(&mut Text, &mut TextColor), With<GoalLine>>,
    inside: Res<crate::interior::Inside>,
    cargo: Option<Res<crate::shell_inventory::Open>>,
) {
    let Ok((id, mover)) = me.single() else {
        return;
    };
    let home = sim.level.place_at(mover.pos) == Some(Place::Home);
    // A pinned goal while out; otherwise the first-hour guide's step, out or
    // on the doorstep (inside the shell the Goals tab shows it): the one
    // clear next goal.
    let goal = if home { None } else { goal_text(&sim, *id) }.or_else(|| {
        if inside.on {
            None
        } else {
            guide_text(&sim, *id).map(|words| (words, false))
        }
    });
    // The open cargo panel covers the pill's place: hide it then, not under.
    let display = if goal.is_some() && !cargo.is_some_and(|c| c.0) {
        Display::Flex
    } else {
        Display::None
    };
    if pill.0.display != display {
        pill.0.display = display;
    }
    let Some((words, ready)) = goal else {
        return;
    };
    if line.0.0 != words {
        line.0.0 = words;
    }
    let (accent, rim, fill) = if ready {
        (theme::GOLD, theme::GOLD.with_alpha(0.7), theme::GOLD)
    } else {
        (theme::TEXT, theme::RIM_FAINT, Color::NONE)
    };
    line.1.set_if_neq(TextColor(accent));
    pill.1.set_if_neq(BorderColor::all(rim));
    mark.0.set_if_neq(BorderColor::all(if ready {
        theme::GOLD
    } else {
        theme::AMBER
    }));
    mark.1.set_if_neq(BackgroundColor(fill));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crabsim::Item;

    #[test]
    fn the_guide_line_shows_the_step_until_skipped() {
        let mut sim = Sim::with_one_crab(Vec2::new(900.0, 0.0));
        let id = PlayerId(0);
        let words = guide_text(&sim, id).unwrap();
        assert!(words.starts_with("Guide: cut kelp"), "{words}");
        assert!(words.ends_with("0/6"), "{words}");
        assert!(sim.guide_skip(id));
        assert_eq!(guide_text(&sim, id), None);
        assert!(sim.guide_on(id));
        assert!(guide_text(&sim, id).is_some());
    }

    #[test]
    fn a_full_shell_turns_the_guide_line_home() {
        let mut sim = Sim::with_one_crab(Vec2::new(900.0, 0.0));
        let id = PlayerId(0);
        let tuning = sim.tuning.clone();
        let crab = sim.crab_mut(id).unwrap();
        while crab
            .mover
            .take_up(crabsim::Carry::Item(Item::Grit), &tuning)
        {}
        assert!(guide_shell_full(&sim, id));
        let words = guide_text(&sim, id).unwrap();
        assert_eq!(words, "Guide: shell full, take it home, 0/6");
    }

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
