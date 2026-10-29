//! The Moult button (`crabsim::moult`): a stand-in until the inside-your-home
//! view has a Moult row. It shows only while the crab is tucked into its home
//! shell with a feat ready: one pill per ready feat ("Moult: grow broad
//! paddles"), or, hungry, a quiet "Eat a clam first, then moult". A click or
//! tap on a pill asks the sim to moult (for the local crab; a replay notes
//! it). The notes say the rest (`notes.rs`).

use bevy::prelude::*;
use crabsim::level::Place;
use crabsim::{Feat, Sim};

use crate::player::{GameSet, LocalPlayer, PlayerId};

/// Pills at the bottom middle of the screen, the first lowest.
const WIDTH: f32 = 230.0;
const HEIGHT: f32 = 34.0;
const BOTTOM: f32 = 48.0;
const GAP: f32 = 8.0;
/// One pill per feat at most.
const PILLS: usize = 3;

pub struct MoultPlugin;

impl Plugin for MoultPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_pills)
            .add_systems(Update, press.in_set(GameSet::Input))
            .add_systems(Update, show.in_set(GameSet::Present));
    }
}

#[derive(Component)]
struct Pill(usize);

#[derive(Component)]
struct PillLabel(usize);

/// What the pills offer now.
enum Offer {
    Nothing,
    /// Tucked in at home with these feats ready, and fed.
    Moult(Vec<Feat>),
    /// Tucked in at home with a feat ready, but hungry.
    Hungry,
}

fn offer(sim: &Sim, id: crabsim::PlayerId) -> Offer {
    let Some(crab) = sim.crab(id) else {
        return Offer::Nothing;
    };
    let m = &crab.mover;
    let ready = m.ready_feats(&sim.tuning);
    let home = m.tucked_in() && sim.level.place_at(m.pos) == Some(Place::Home);
    if !home || ready.is_empty() || m.is_soft() || crab.fighter.dead.is_some() {
        Offer::Nothing
    } else if sim.can_moult(id) {
        Offer::Moult(ready)
    } else {
        Offer::Hungry
    }
}

/// Pill `i`'s box, in logical pixels from the top-left.
fn pill_rect(window: Vec2, i: usize) -> Rect {
    let bottom = window.y - BOTTOM - i as f32 * (HEIGHT + GAP);
    let left = (window.x - WIDTH) / 2.0;
    Rect::new(left, bottom - HEIGHT, left + WIDTH, bottom)
}

fn spawn_pills(mut commands: Commands) {
    for i in 0..PILLS {
        commands
            .spawn((
                Pill(i),
                Node {
                    position_type: PositionType::Absolute,
                    bottom: px(BOTTOM + i as f32 * (HEIGHT + GAP)),
                    left: percent(50.0),
                    margin: UiRect::left(px(-WIDTH / 2.0)),
                    width: px(WIDTH),
                    height: px(HEIGHT),
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    border: UiRect::all(px(1.5)),
                    border_radius: BorderRadius::all(px(HEIGHT / 2.0)),
                    ..default()
                },
                BorderColor::all(Color::srgba(1.0, 0.85, 0.55, 0.55)),
                BackgroundColor(Color::srgba(0.05, 0.12, 0.14, 0.8)),
                Visibility::Hidden,
                GlobalZIndex(15),
            ))
            .with_children(|b| {
                b.spawn((
                    PillLabel(i),
                    Text::new(""),
                    TextFont {
                        font_size: FontSize::Px(13.0),
                        ..default()
                    },
                    TextColor(Color::srgba(1.0, 0.9, 0.7, 0.95)),
                ));
            });
    }
}

/// A click or tap on a moult pill: moult for that feat.
fn press(
    mouse: Res<ButtonInput<MouseButton>>,
    touches: Res<Touches>,
    window: Single<&Window>,
    mut sim: ResMut<Sim>,
    crab: Query<&PlayerId, With<LocalPlayer>>,
) {
    let Ok(id) = crab.single() else {
        return;
    };
    let Offer::Moult(ready) = offer(&sim, *id) else {
        return;
    };
    let mut points: Vec<Vec2> = touches.iter_just_pressed().map(|t| t.position()).collect();
    if mouse.just_pressed(MouseButton::Left)
        && let Some(cursor) = window.cursor_position()
    {
        points.push(cursor);
    }
    for (i, feat) in ready.iter().enumerate().take(PILLS) {
        let r = pill_rect(window.size(), i).inflate(6.0);
        if points.iter().any(|p| r.contains(*p)) {
            let done = sim.moult(*id, feat.skill());
            info!("moult: {} -> {done}", feat.name());
            return;
        }
    }
}

fn show(
    sim: Res<Sim>,
    crab: Query<&PlayerId, With<LocalPlayer>>,
    mut pills: Query<(&Pill, &mut Visibility)>,
    mut labels: Query<(&PillLabel, &mut Text, &mut TextColor)>,
) {
    let offer = crab.single().map_or(Offer::Nothing, |id| offer(&sim, *id));
    let words: Vec<(String, bool)> = match offer {
        Offer::Nothing => Vec::new(),
        Offer::Hungry => vec![("Eat a clam first, then moult".into(), false)],
        Offer::Moult(ready) => ready
            .iter()
            .map(|f| (format!("Moult: grow {}", f.name()), true))
            .collect(),
    };
    for (pill, mut shown) in &mut pills {
        shown.set_if_neq(if pill.0 < words.len() {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        });
    }
    for (label, mut text, mut colour) in &mut labels {
        let Some((line, active)) = words.get(label.0) else {
            continue;
        };
        if text.0 != *line {
            text.0.clone_from(line);
        }
        let alpha = if *active { 0.95 } else { 0.6 };
        colour.set_if_neq(TextColor(Color::srgba(1.0, 0.9, 0.7, alpha)));
    }
}
