//! Gathering, as the player meets it (the rules are `crabsim`'s: `Sim::gather_state`,
//! `economy::gather::check`, `StepEvents::gather_blocked`).
//!
//! * A ring of small dots fills around the node being worked.
//! * A short prompt by the crab while a node is in reach: "Hold C: pry mussels"
//!   ("Hold Gather: ..." on a touch screen), or the reason it can't be worked.
//! * [`why_not`] words the reasons; `notes.rs` floats them when a hold is refused.
//!
//! The key (`C`), the controller's left stick click and the touch Gather button
//! are read in `input.rs`; [`in_reach`] tells it (and the button) when a node is
//! near enough to work.

use bevy::prelude::*;
use crabsim::Sim;
use crabsim::economy::gather::{self, GatherBlocked};
use crabsim::economy::gear::TOOL_MATERIALS;

use crate::coop::LocalId;
use crate::input::TouchControls;
use crate::player::GameSet;

/// The key that gathers (hold).
pub const KEY: &str = "C";

/// Dots in the ring, and its radius around the node's centre.
const DOTS: usize = 20;
const RADIUS: f32 = 24.0;
const RING_Z: f32 = 12.0;
const PROMPT_UP: f32 = 78.0;

const FILLED: Color = Color::srgb(1.0, 0.88, 0.55);
const EMPTY: Color = Color::srgba(0.05, 0.12, 0.14, 0.55);
const PROMPT: Color = Color::srgba(0.96, 0.91, 0.82, 0.92);
const WHY: Color = Color::srgba(0.85, 0.92, 0.95, 0.8);

/// The node the local crab's hold would work, within reach: as the rules
/// choose (`Sim`'s gather takes the nearest node it can work), a workable
/// node with charges first, then any with charges, then the nearest; and
/// whether it has charges. So a new crab beside a level-8 node and a level-1
/// one is offered the one it can gather, not "Foraging 8"; and among those,
/// the kind the guide's current step asks for.
pub fn in_reach(sim: &Sim, id: crabsim::PlayerId) -> Option<(usize, bool)> {
    let crab = sim.crab(id).filter(|c| c.fighter.dead.is_none())?;
    // The kind the guide's current step asks for, which the rules prefer
    // among workable nodes in reach.
    let wanted = (!crab.econ.guide.skipped)
        .then(|| crab.econ.guide.current())
        .flatten()
        .and_then(|step| match step.cond {
            crabsim::economy::guide::Cond::Gather(kind, _) => Some(kind),
            _ => None,
        });
    let mut best: Option<(usize, (bool, bool), bool, f32)> = None;
    for (i, p) in sim.placed_nodes().iter().enumerate() {
        let d = p.pos.distance(crab.mover.pos);
        if d > gather::REACH {
            continue;
        }
        let ready = sim.shared.nodes.get(i).is_some_and(|n| n.charges > 0);
        let workable = ready && gather::check(crab, p.kind, &sim.tuning).is_ok();
        let rank = (workable, workable && wanted == Some(p.kind));
        let better = match best {
            None => true,
            Some((_, was_rank, was, far)) => (rank, ready, -d) > (was_rank, was, -far),
        };
        if better {
            best = Some((i, rank, ready, d));
        }
    }
    best.map(|(i, _, ready, _)| (i, ready))
}

/// The words for a refused hold.
pub fn why_not(why: GatherBlocked) -> String {
    match why {
        GatherBlocked::Level(skill, level) => format!("Opens at {} {level}", skill.name()),
        GatherBlocked::Tool(kind, tier) => {
            let material = TOOL_MATERIALS
                .get(usize::from(tier))
                .copied()
                .unwrap_or("better");
            let a = if material.starts_with(['a', 'e', 'i', 'o', 'u']) {
                "an"
            } else {
                "a"
            };
            format!("Needs {a} {material} {}", kind.name())
        }
        GatherBlocked::NoRoom => "No room on your shell".into(),
        GatherBlocked::Spent => "Spent: grows back soon".into(),
        GatherBlocked::Ingredients => "Needs 2 seep mussels on your shell".into(),
    }
}

/// What the prompt says for the local crab now, if anything.
fn prompt_text(sim: &Sim, id: crabsim::PlayerId, touch: bool) -> Option<String> {
    let (i, ready) = in_reach(sim, id)?;
    let crab = sim.crab(id)?;
    let kind = sim.placed_nodes().get(i)?.kind;
    if crab.gather.node.is_some() {
        return None;
    }
    if !ready {
        return Some(why_not(GatherBlocked::Spent));
    }
    match gather::check(crab, kind, &sim.tuning) {
        Ok(_) => {
            let hold = if touch {
                "Hold Gather".to_string()
            } else {
                format!("Hold {KEY}")
            };
            Some(format!("{hold}: {}", kind.def().verb))
        }
        Err(why) => Some(why_not(why)),
    }
}

pub struct GatherUiPlugin;

impl Plugin for GatherUiPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn)
            .add_systems(Update, show.in_set(GameSet::Present));
    }
}

#[derive(Component)]
struct Dot(usize);

#[derive(Component)]
struct Prompt;

fn spawn(mut commands: Commands) {
    for i in 0..DOTS {
        commands.spawn((
            Dot(i),
            Sprite::from_color(EMPTY, Vec2::splat(5.0)),
            Transform::from_xyz(0.0, 0.0, RING_Z),
            Visibility::Hidden,
        ));
    }
    commands.spawn((
        Prompt,
        Text2d::new(""),
        TextFont {
            font_size: FontSize::Px(13.0),
            ..default()
        },
        TextColor(PROMPT),
        Transform::from_xyz(0.0, 0.0, 21.0),
        Visibility::Hidden,
    ));
}

#[allow(clippy::type_complexity)]
fn show(
    sim: Res<Sim>,
    me: Res<LocalId>,
    touch: Res<TouchControls>,
    mut dots: Query<
        (&Dot, &mut Transform, &mut Sprite, &mut Visibility),
        (Without<Prompt>, Without<Text2d>),
    >,
    mut prompt: Query<
        (&mut Transform, &mut Text2d, &mut TextColor, &mut Visibility),
        (With<Prompt>, Without<Dot>),
    >,
) {
    let working = sim.gather_state(me.0);
    let node_at = working.and_then(|g| sim.placed_nodes().get(g.node).map(|n| (n.pos, g.progress)));
    for (dot, mut at, mut sprite, mut shown) in &mut dots {
        let Some((centre, progress)) = node_at else {
            shown.set_if_neq(Visibility::Hidden);
            continue;
        };
        shown.set_if_neq(Visibility::Inherited);
        // Clockwise from the top.
        let turn = std::f32::consts::TAU * dot.0 as f32 / DOTS as f32;
        at.translation = (centre + Vec2::new(turn.sin(), turn.cos()) * RADIUS).extend(RING_Z);
        let lit = (dot.0 as f32 + 0.5) / DOTS as f32 <= progress;
        sprite.color = if lit { FILLED } else { EMPTY };
    }
    let Ok((mut at, mut text, mut colour, mut shown)) = prompt.single_mut() else {
        return;
    };
    let words = prompt_text(&sim, me.0, touch.visible);
    let crab_at = sim.crab(me.0).map(|c| c.mover.pos);
    match (words, crab_at) {
        (Some(words), Some(pos)) => {
            if text.0 != words {
                text.0 = words.clone();
            }
            let asks = words.starts_with("Hold");
            colour.set_if_neq(TextColor(if asks { PROMPT } else { WHY }));
            at.translation = (pos + Vec2::Y * PROMPT_UP).extend(21.0);
            shown.set_if_neq(Visibility::Inherited);
        }
        _ => {
            shown.set_if_neq(Visibility::Hidden);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crabsim::Skill;
    use crabsim::economy::ToolKind;

    #[test]
    fn the_reasons_read_as_the_spec_says() {
        assert_eq!(
            why_not(GatherBlocked::Level(Skill::Prying, 15)),
            "Opens at Prying 15"
        );
        assert_eq!(
            why_not(GatherBlocked::Tool(ToolKind::Pry, 1)),
            "Needs a shell pry tip"
        );
        assert_eq!(
            why_not(GatherBlocked::Tool(ToolKind::Scoop, 4)),
            "Needs a bone scoop"
        );
        assert_eq!(why_not(GatherBlocked::NoRoom), "No room on your shell");
        assert_eq!(why_not(GatherBlocked::Spent), "Spent: grows back soon");
        assert_eq!(
            why_not(GatherBlocked::Ingredients),
            "Needs 2 seep mussels on your shell"
        );
    }
}
