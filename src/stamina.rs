//! The paddle legs' stamina, drawn as a few small rings above the crab: one
//! per paddle stroke the crab has grown (one per egg eaten; the first is the
//! double jump), each holding a bead of warm light that goes out as the crab
//! strokes (right to left) and grows back while it stands on something
//! solid. The rings only show while one is spent (or just after an egg), and
//! fade away once they're all full again.
//!
//! Resting draws on the body's reserve, shown as a small bar in the top-left
//! corner (one segment per stroke it holds). It empties as the rings refill
//! and comes back by itself, slowly. Well fed, it holds a few more (gold
//! segments, the bar the same length), and a small "Well fed" sign under the
//! shell bar counts down the time left.

use bevy::prelude::*;
use crabsim::physics::Mover;
use crabsim::{Sim, Skill, TICKS_PER_SECOND};

use crate::effects::SoftTextures;
use crate::player::{FrameEvents, GameSet, LocalPlayer};

/// Height of the rings above the crab's feet.
const LIFT: f32 = 105.0;
/// Distance between ring centres.
const SPACING: f32 = 17.0;
/// Size of a ring; the bead of light inside is a little larger, because the
/// soft dot fades out well before its edge.
const RING: f32 = 15.0;
const BEAD: f32 = 20.0;
/// With the other effects, in front of the crab (~10-11).
const Z: f32 = 20.0;
/// Warm amber: the crab's own colour (see `docs/design/art-direction.md`).
const WARM: Color = Color::srgb(1.0, 0.78, 0.32);
/// How fast the rings fade in when the legs tire, and out once they're
/// rested (per second).
const FADE_IN: f32 = 8.0;
const FADE_OUT: f32 = 2.5;

pub struct StaminaPlugin;

impl Plugin for StaminaPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Startup,
            (spawn_rings, spawn_reserve_bar, spawn_skill, spawn_well_fed),
        )
        .add_systems(
            Update,
            (show_rings, show_reserve, show_skill, show_well_fed).in_set(GameSet::Present),
        );
    }
}

/// One stroke's ring, or (`bead`) the light inside it.
#[derive(Component)]
struct Pip {
    stroke: u32,
    bead: bool,
}

fn spawn_rings(mut commands: Commands, soft: Res<SoftTextures>, sim: Res<Sim>) {
    // Enough for every stroke the crab could have: its own and one per egg.
    let most = sim.tuning.swim_strokes + sim.level.eggs.len() as u32;
    for stroke in 0..most {
        for bead in [false, true] {
            let (image, size) = if bead {
                (soft.dot.clone(), BEAD)
            } else {
                (soft.ring.clone(), RING)
            };
            commands.spawn((
                Pip { stroke, bead },
                Sprite {
                    image,
                    color: Color::NONE,
                    custom_size: Some(Vec2::splat(size)),
                    ..default()
                },
                Transform::default(),
            ));
        }
    }
}

/// Keep the rings over the local crab, fill them from its stamina, and fade
/// them in or out.
fn show_rings(
    time: Res<Time>,
    sim: Res<Sim>,
    mut shown: Local<f32>,
    mut showing_off: Local<f32>,
    crab: Query<(&Mover, &FrameEvents, &Transform), With<LocalPlayer>>,
    mut pips: Query<(&Pip, &mut Transform, &mut Sprite), Without<LocalPlayer>>,
) {
    let Ok((mover, events, crab_at)) = crab.single() else {
        return;
    };
    let strokes = mover.strokes(&sim.tuning);
    // Just ate an egg: show off the new ring for a moment.
    if events.0.ate {
        *showing_off = 1.5;
    }
    *showing_off = (*showing_off - time.delta_secs()).max(0.0);
    let visible = mover.stamina < strokes as f32 - 0.001 || *showing_off > 0.0;
    let step = time.delta_secs() * if visible { FADE_IN } else { FADE_OUT };
    *shown += (f32::from(u8::from(visible)) - *shown).clamp(-step, step);

    let n = strokes as f32;
    for (pip, mut at, mut sprite) in &mut pips {
        if pip.stroke >= strokes {
            sprite.color = Color::NONE; // not grown yet
            continue;
        }
        let x = (pip.stroke as f32 - (n - 1.0) / 2.0) * SPACING;
        let z = if pip.bead { Z + 0.1 } else { Z };
        at.translation = crab_at.translation.truncate().extend(z) + Vec3::new(x, LIFT, 0.0);
        if pip.bead {
            // Strokes left: 2.4 = two full beads and one at 40%.
            let fill = (mover.stamina - pip.stroke as f32).clamp(0.0, 1.0);
            at.scale = Vec3::splat(fill);
            sprite.color = WARM.with_alpha(*shown);
        } else {
            sprite.color = WARM.with_alpha(0.5 * *shown);
        }
    }
}

/// The light inside one segment of the reserve bar.
#[derive(Component)]
struct ReserveFill(u32);

/// One segment of the reserve bar (its frame).
#[derive(Component)]
struct ReserveSegment(u32);

/// The reserve bar's length: its segments share it, however many there are.
const RESERVE_BAR: f32 = 111.0;
const RESERVE_GAP: f32 = 3.0;
/// The well-fed segments and sign: pale gold, apart from the plain reserve.
const FED: Color = Color::srgb(0.98, 0.93, 0.70);

/// The reserve bar: a row of small segments under the HUD text (enough for
/// a well-fed reserve; the extra ones show only then).
fn spawn_reserve_bar(mut commands: Commands, sim: Res<Sim>) {
    commands
        .spawn(Node {
            position_type: PositionType::Absolute,
            top: px(76),
            left: px(12),
            column_gap: px(RESERVE_GAP),
            ..default()
        })
        .with_children(|bar| {
            let plain = sim.tuning.stamina_reserve;
            for segment in 0..plain + sim.tuning.food.reserve_bonus {
                let colour = if segment < plain { WARM } else { FED };
                bar.spawn((
                    ReserveSegment(segment),
                    Node {
                        width: px(16),
                        height: px(7),
                        border: UiRect::all(px(1)),
                        border_radius: BorderRadius::all(px(3)),
                        overflow: Overflow::clip(),
                        ..default()
                    },
                    BorderColor::all(colour.with_alpha(0.45)),
                ))
                .with_child((
                    ReserveFill(segment),
                    Node {
                        width: percent(100),
                        height: percent(100),
                        ..default()
                    },
                    BackgroundColor(colour.with_alpha(0.85)),
                ));
            }
        });
}

/// Fill the reserve bar's segments from the local crab's reserve, showing
/// as many segments as it holds now.
fn show_reserve(
    sim: Res<Sim>,
    crab: Query<&Mover, With<LocalPlayer>>,
    mut segments: Query<(&ReserveSegment, &mut Node), Without<ReserveFill>>,
    mut fills: Query<(&ReserveFill, &mut Node), Without<ReserveSegment>>,
) {
    let Ok(mover) = crab.single() else {
        return;
    };
    let holds = mover.reserve_strokes(&sim.tuning);
    let count = holds as u32;
    let width = px((RESERVE_BAR - RESERVE_GAP * (holds - 1.0)) / holds);
    for (segment, mut node) in &mut segments {
        let display = if segment.0 < count {
            Display::Flex
        } else {
            Display::None
        };
        if node.display != display || node.width != width {
            node.display = display;
            node.width = width;
        }
    }
    let left = mover.reserve * holds;
    for (fill, mut node) in &mut fills {
        let width = percent(100.0 * (left - fill.0 as f32).clamp(0.0, 1.0));
        if node.width != width {
            node.width = width;
        }
    }
}

/// The skills' readout under the reserve bar: one small row per skill the
/// crab has any experience in (its name and level, and a thin bar for the way
/// to the next level). Hidden until the crab first earns some.
#[derive(Component)]
struct SkillRow(Skill);

#[derive(Component)]
struct SkillFill(Skill);

fn spawn_skill(mut commands: Commands) {
    commands
        .spawn(Node {
            position_type: PositionType::Absolute,
            top: px(90),
            left: px(12),
            flex_direction: FlexDirection::Column,
            row_gap: px(4),
            ..default()
        })
        .with_children(|column| {
            for skill in Skill::ALL {
                column
                    .spawn((
                        Node {
                            display: Display::None,
                            flex_direction: FlexDirection::Column,
                            row_gap: px(2),
                            ..default()
                        },
                        SkillRow(skill),
                    ))
                    .with_children(|row| {
                        row.spawn((
                            Text::new(""),
                            TextFont {
                                font_size: FontSize::Px(11.0),
                                ..default()
                            },
                            TextColor(Color::srgba(0.85, 0.92, 0.95, 0.75)),
                        ));
                        row.spawn((
                            Node {
                                width: px(70),
                                height: px(2),
                                ..default()
                            },
                            BackgroundColor(Color::srgba(1.0, 0.88, 0.55, 0.2)),
                        ))
                        .with_child((
                            SkillFill(skill),
                            Node {
                                width: percent(0),
                                height: percent(100),
                                ..default()
                            },
                            BackgroundColor(Color::srgba(1.0, 0.88, 0.55, 0.8)),
                        ));
                    });
            }
        });
}

fn show_skill(
    sim: Res<Sim>,
    crab: Query<&Mover, With<LocalPlayer>>,
    mut rows: Query<(&SkillRow, &mut Node, &Children), Without<SkillFill>>,
    mut texts: Query<&mut Text>,
    mut fills: Query<(&SkillFill, &mut Node), Without<SkillRow>>,
) {
    let Ok(mover) = crab.single() else {
        return;
    };
    for (row, mut node, children) in &mut rows {
        let earned = mover.xp(row.0) > 0.0;
        let display = if earned { Display::Flex } else { Display::None };
        if node.display != display {
            node.display = display;
        }
        if !earned {
            continue;
        }
        let level = mover.level(row.0, &sim.tuning);
        if let Some(mut text) = children.first().and_then(|c| texts.get_mut(*c).ok()) {
            // A feat ready to grow at a moult, growing, or grown.
            let feat = crabsim::Feat::of(row.0);
            let mark = if mover.growing == Some(feat) {
                " · growing"
            } else if mover.has(feat) {
                " · feat"
            } else if mover.feat_ready(row.0, &sim.tuning) {
                " · moult ready"
            } else {
                ""
            };
            let line = format!("{} {level}{mark}", row.0.name());
            if text.0 != line {
                text.0 = line;
            }
        }
    }
    for (fill, mut node) in &mut fills {
        let width = percent(100.0 * mover.progress(fill.0, &sim.tuning));
        if node.width != width {
            node.width = width;
        }
    }
}

/// The well-fed sign under the shell bar: hidden unless well fed.
#[derive(Component)]
struct WellFed;

fn spawn_well_fed(mut commands: Commands) {
    commands.spawn((
        WellFed,
        Text::new(""),
        TextFont {
            font_size: FontSize::Px(12.0),
            ..default()
        },
        TextColor(FED.with_alpha(0.9)),
        Node {
            position_type: PositionType::Absolute,
            top: px(93),
            left: px(140),
            ..default()
        },
        Visibility::Hidden,
    ));
}

/// "Well fed 3:58": the minutes and seconds of play it has left.
fn show_well_fed(
    crab: Query<&Mover, With<LocalPlayer>>,
    mut sign: Query<(&mut Text, &mut Visibility), With<WellFed>>,
) {
    let (Ok(mover), Ok((mut text, mut shown))) = (crab.single(), sign.single_mut()) else {
        return;
    };
    // Soft after a moult (it eats nothing then, so never well fed too): the
    // time left out in the open (tucked in, it hardens faster).
    let (word, ticks) = if mover.is_soft() {
        ("Soft shell", mover.soft)
    } else {
        ("Well fed", mover.well_fed)
    };
    if ticks == 0 {
        shown.set_if_neq(Visibility::Hidden);
        return;
    }
    shown.set_if_neq(Visibility::Inherited);
    let seconds = ticks.div_ceil(TICKS_PER_SECOND);
    let line = format!("{word} {}:{:02}", seconds / 60, seconds % 60);
    if text.0 != line {
        text.0 = line;
    }
}
