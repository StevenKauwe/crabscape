//! The HUD's reward line, top and centre (`docs/design/hud.md`): where the game
//! says "you earned something".
//!
//! - **Experience drops** (RuneScape): a skill that earns shows a small tracker,
//!   its name, level and a bar to the next level, with "+25" popping up out of
//!   it; it fades a few seconds after the last drop. A skill that earns by the
//!   unit (climbing, swimming, digging) drops whole numbers, gathered up.
//! - **Level-ups**: a card pops in, "LEVEL UP", "Prying 15" in big gold, a line
//!   of gold drawing under it, and what the level opens ("You can work clam
//!   bed.", "New task: make kelp cord."). It holds, then fades; a second level
//!   waits its turn. The crab's own juice (the ring, the claws lifting, the
//!   chime) is `skill_juice.rs`, `puppet.rs` and `sound.rs`.
//! - **A goal in hand**: the same card in amber, "GOAL READY", "go home and
//!   make it".
//! - The touch hint ("JUMP again in the water: swim") sits under them for the
//!   first seconds of play (`view.rs` writes it).
//!
//! Presentation only: it reads the sim's experience and the local crab's
//! events, and changes nothing.

use std::collections::VecDeque;

use bevy::prelude::*;
use crabsim::{Sim, Skill};

use crate::player::{FrameEvents, GameSet, LocalPlayer, PlayerId};
use crate::ui_theme::{self as theme, UiFonts};

/// The reward line's width.
const WIDTH: f32 = 300.0;
/// How long a level card holds (seconds) before it fades, plus a little per
/// line it says; how long its fade and its pop take.
const HOLD: f32 = 3.6;
const HOLD_PER_LINE: f32 = 0.7;
const FADE: f32 = 0.45;
const POP: f32 = 0.32;
/// The most unlock lines a card lists (the rest are "and N more: see Skills").
const MOST_LINES: usize = 3;
/// How long the tracker stays after the last drop, and its fade.
const TRACK_HOLD: f32 = 4.0;
/// A drop's life and rise (logical pixels).
const DROP_LIFE: f32 = 1.1;
const DROP_RISE: f32 = 26.0;
/// A by-the-unit skill drops at most this often per skill (seconds).
const TRICKLE: f32 = 0.7;

pub struct HudPlugin;

impl Plugin for HudPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Cards>()
            .add_systems(Startup, spawn)
            .add_systems(
                Update,
                (place, demo, queue_cards, show_card, track_xp, float_drops)
                    .chain()
                    .in_set(GameSet::Present),
            );
    }
}

/// Where the reward line's top sits for a window: high on a wide screen; on a
/// narrow (portrait) one under the corner buttons, clear of the touch buttons.
pub fn line_top(window: Vec2) -> f32 {
    if crate::settings::narrow(window) {
        196.0
    } else if window.x < 900.0 {
        56.0
    } else {
        14.0
    }
}

/// One card to show.
#[derive(Clone, Debug, PartialEq)]
pub struct Card {
    pub caption: &'static str,
    pub title: String,
    pub lines: Vec<String>,
    /// Gold (a level), or amber (a goal).
    pub gold: bool,
}

/// The cards waiting, and the one up and its age.
#[derive(Resource, Default)]
pub struct Cards {
    waiting: VecDeque<Card>,
    up: Option<(Card, f32)>,
}

impl Cards {
    pub fn push(&mut self, card: Card) {
        self.waiting.push_back(card);
    }
}

/// A level card for `skill` reaching `level`, saying what it opens (`more`:
/// the lines from the rules, at most [`MOST_LINES`] of them shown).
pub fn level_card(skill: Skill, level: u32, more: Vec<String>) -> Card {
    let mut lines: Vec<String> = more.iter().take(MOST_LINES).cloned().collect();
    if more.len() > MOST_LINES {
        lines.push(format!("and {} more: see Skills", more.len() - MOST_LINES));
    }
    Card {
        caption: "LEVEL UP",
        title: format!("{} {level}", skill.name()),
        lines,
        gold: true,
    }
}

/// How long a card stays up, in all.
/// A first-hour guide step done: the step's title and what it paid.
pub fn guide_card(step: &crabsim::economy::guide::Step) -> Card {
    Card {
        caption: "GUIDE",
        title: step.title.to_string(),
        lines: vec![format!("You get {}.", step.reward.line())],
        gold: false,
    }
}

fn card_life(card: &Card) -> f32 {
    HOLD + HOLD_PER_LINE * card.lines.len() as f32
}

#[derive(Component)]
struct Line;
#[derive(Component)]
struct CardBox;
#[derive(Component)]
struct CardCaption;
#[derive(Component)]
struct CardTitle;
#[derive(Component)]
struct CardShine;
#[derive(Component)]
struct CardLines;
#[derive(Component)]
struct Tracker;
#[derive(Component)]
struct TrackName;
#[derive(Component)]
struct TrackLevel;
#[derive(Component)]
struct TrackFill;
#[derive(Component)]
struct XpDrop {
    age: f32,
}

fn spawn(mut commands: Commands, fonts: Res<UiFonts>, window: Single<&Window>) {
    let size = window.size();
    commands
        .spawn((
            Line,
            Node {
                position_type: PositionType::Absolute,
                top: px(line_top(size)),
                left: px(0),
                width: percent(100),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                row_gap: px(8),
                ..default()
            },
            // Under every panel and button (it never takes a tap), over the world.
            GlobalZIndex(-1),
        ))
        .with_children(|line| {
            // The card.
            line.spawn((
                CardBox,
                Node {
                    display: Display::None,
                    width: px(WIDTH),
                    flex_direction: FlexDirection::Column,
                    align_items: AlignItems::Center,
                    padding: UiRect::axes(px(16), px(10)),
                    row_gap: px(3),
                    border: UiRect::all(px(theme::RIM_WIDTH)),
                    border_radius: BorderRadius::all(px(theme::RADIUS)),
                    ..default()
                },
                BackgroundColor(theme::PANEL),
                BorderColor::all(theme::GOLD),
                theme::panel_shadow(),
                UiTransform::default(),
            ))
            .with_children(|card| {
                card.spawn((
                    CardCaption,
                    theme::bold(&fonts, "", theme::CAPTION, theme::AMBER),
                ));
                card.spawn((
                    CardTitle,
                    theme::bold(&fonts, "", theme::DISPLAY, theme::GOLD),
                ));
                card.spawn((
                    CardShine,
                    Node {
                        width: percent(0),
                        height: px(2),
                        margin: UiRect::vertical(px(2)),
                        ..default()
                    },
                    BackgroundColor(theme::GOLD),
                ));
                card.spawn((
                    CardLines,
                    Node {
                        flex_direction: FlexDirection::Column,
                        align_items: AlignItems::Center,
                        row_gap: px(2),
                        ..default()
                    },
                ));
            });
            // The experience tracker.
            line.spawn((
                Tracker,
                Node {
                    display: Display::None,
                    align_items: AlignItems::Center,
                    column_gap: px(8),
                    padding: UiRect::axes(px(12), px(6)),
                    border: UiRect::all(px(1)),
                    border_radius: BorderRadius::all(px(16)),
                    ..default()
                },
                BackgroundColor(theme::BUTTON),
                BorderColor::all(theme::RIM_FAINT),
            ))
            .with_children(|t| {
                t.spawn((TrackName, theme::bold(&fonts, "", theme::BODY, theme::TEXT)));
                t.spawn((TrackLevel, theme::words("", theme::CAPTION, theme::MUTED)));
                t.spawn((
                    Node {
                        width: px(84),
                        height: px(5),
                        border_radius: BorderRadius::all(px(3)),
                        overflow: Overflow::clip(),
                        ..default()
                    },
                    BackgroundColor(Color::srgba(1.0, 0.87, 0.5, 0.16)),
                ))
                .with_child((
                    TrackFill,
                    Node {
                        width: percent(0),
                        height: percent(100),
                        ..default()
                    },
                    BackgroundColor(theme::GOLD),
                ));
            });
            // The touch hint (`view.rs` writes it).
            line.spawn((
                crate::view::HelpText,
                theme::words("", theme::BODY, theme::MUTED),
            ));
        });
}

/// Keep the line where it belongs as the window turns.
fn place(window: Single<&Window>, mut line: Single<&mut Node, With<Line>>) {
    let top = px(line_top(window.size()));
    if line.top != top {
        line.top = top;
    }
}

/// `?hud-demo` in the address (`--hud-demo` on the desktop): every ten
/// seconds, show a level card as Prying 15 would, for screenshots and checking
/// the look by eye. Changes nothing in the sim.
fn demo(
    time: Res<Time<Real>>,
    sim: Res<Sim>,
    mut cards: ResMut<Cards>,
    crab: Query<&PlayerId, With<LocalPlayer>>,
    mut next: Local<Option<f32>>,
) {
    let Ok(id) = crab.single() else { return };
    let now = time.elapsed_secs();
    let at = *next.get_or_insert_with(|| {
        if demo_asked() {
            now + 3.0
        } else {
            f32::INFINITY
        }
    });
    if now < at {
        return;
    }
    *next = Some(at + 10.0);
    let more = crate::skill_juice::level_up_lines(&sim, *id, Skill::Prying, 15).split_off(1);
    cards.push(level_card(Skill::Prying, 15, more));
}

fn demo_asked() -> bool {
    #[cfg(target_arch = "wasm32")]
    {
        web_sys::window()
            .and_then(|w| w.location().search().ok())
            .is_some_and(|q| {
                q.trim_start_matches('?')
                    .split('&')
                    .any(|kv| kv == "hud-demo")
            })
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        std::env::args().any(|a| a == "--hud-demo")
    }
}

/// Level-ups and a goal in hand become cards.
fn queue_cards(
    sim: Res<Sim>,
    mut cards: ResMut<Cards>,
    crab: Query<(&FrameEvents, &PlayerId), With<LocalPlayer>>,
) {
    let Ok((events, id)) = crab.single() else {
        return;
    };
    let ev = &events.0;
    let t = &sim.tuning;
    if let Some(up) = ev.level_up {
        let mut more =
            crate::skill_juice::level_up_lines(&sim, *id, up.skill, up.level).split_off(1);
        if let Some(crab) = sim.crab(*id)
            && crab.mover.feat_ready(up.skill, t)
            && crabsim::Feat::of(up.skill).is_some_and(|f| !crab.mover.has(f))
        {
            more.insert(0, "A feat is ready: moult to grow it.".into());
        }
        cards.push(level_card(up.skill, up.level, more));
    } else if let Some(skill) = ev.levelled
        && let Some(crab) = sim.crab(*id)
    {
        let level = crab.level(skill, t);
        let more = if crab.mover.feat_ready(skill, t) {
            vec!["A feat is ready: moult to grow it.".into()]
        } else {
            Vec::new()
        };
        cards.push(level_card(skill, level, more));
    }
    if let Some(step) = ev
        .guide_done
        .and_then(|i| crabsim::economy::guide::GUIDE.get(usize::from(i)))
    {
        cards.push(guide_card(step));
    }
    if ev.goal_ready {
        cards.push(Card {
            caption: "GOAL READY",
            title: "All in hand".into(),
            lines: vec!["Go home and make it.".into()],
            gold: false,
        });
    }
}

/// Bring the next card up, pop it in, draw its shine, hold it and fade it.
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn show_card(
    mut commands: Commands,
    time: Res<Time<Real>>,
    mut cards: ResMut<Cards>,
    mut card: Single<
        (
            &mut Node,
            &mut UiTransform,
            &mut BackgroundColor,
            &mut BorderColor,
        ),
        (With<CardBox>, Without<CardShine>),
    >,
    mut caption: Single<(&mut Text, &mut TextColor), (With<CardCaption>, Without<CardTitle>)>,
    mut title: Single<(&mut Text, &mut TextColor), (With<CardTitle>, Without<CardCaption>)>,
    mut shine: Single<(&mut Node, &mut BackgroundColor), (With<CardShine>, Without<CardBox>)>,
    lines: Single<Entity, With<CardLines>>,
    mut line_words: Query<
        &mut TextColor,
        (Without<CardTitle>, Without<CardCaption>, With<ChildOf>),
    >,
    children: Query<&Children>,
) {
    let dt = time.delta_secs();
    if cards.up.is_none() {
        let Some(next) = cards.waiting.pop_front() else {
            if card.0.display != Display::None {
                card.0.display = Display::None;
            }
            return;
        };
        caption.0.0 = next.caption.to_string();
        title.0.0.clone_from(&next.title);
        commands
            .entity(*lines)
            .despawn_related::<Children>()
            .with_children(|l| {
                for line in &next.lines {
                    l.spawn(theme::words(line.clone(), theme::BODY, theme::TEXT));
                }
            });
        cards.up = Some((next, 0.0));
    }
    let Some((up, age)) = cards.up.as_mut() else {
        return;
    };
    *age += dt;
    let (age, life, gold) = (*age, card_life(up), up.gold);
    if age >= life {
        cards.up = None;
        return;
    }
    if card.0.display != Display::Flex {
        card.0.display = Display::Flex;
    }
    // Pop: quick swell past full size, then settle; a slight sink as it goes.
    let k = (age / POP).min(1.0);
    let swell = if k < 0.55 {
        0.82 + 0.28 * (k / 0.55)
    } else {
        1.10 - 0.10 * ((k - 0.55) / 0.45)
    };
    let gone = ((age - (life - FADE)) / FADE).clamp(0.0, 1.0);
    let alpha = (age / 0.12).min(1.0) * (1.0 - gone);
    card.1.scale = Vec2::splat(swell * (1.0 - 0.04 * gone));
    card.1.translation = Val2::px(0.0, -6.0 * gone);
    let accent = if gold { theme::GOLD } else { theme::AMBER };
    card.2.0 = theme::PANEL.with_alpha(theme::PANEL.alpha() * alpha);
    *card.3 = BorderColor::all(accent.with_alpha(0.85 * alpha));
    caption.1.0 = theme::AMBER.with_alpha(alpha);
    title.1.0 = accent.with_alpha(alpha);
    // The shine draws across under the title, then dims.
    let drawn = ((age - 0.1) / 0.5).clamp(0.0, 1.0);
    let drawn = 1.0 - (1.0 - drawn) * (1.0 - drawn);
    shine.0.width = percent(80.0 * drawn);
    shine.1.0 = accent.with_alpha(alpha * (1.0 - 0.6 * ((age - 0.6) / 1.0).clamp(0.0, 1.0)));
    if let Ok(kids) = children.get(*lines) {
        for kid in kids.iter() {
            if let Ok(mut c) = line_words.get_mut(kid) {
                c.0 = theme::TEXT.with_alpha(alpha);
            }
        }
    }
}

/// What the tracker last saw: each skill's experience, what is gathered up
/// for a by-the-unit skill, when it last dropped, and which skill shows.
#[derive(Default)]
struct Seen {
    crab: Option<PlayerId>,
    xp: Vec<f32>,
    gathered: Vec<f32>,
    since_drop: Vec<f32>,
    showing: Option<Skill>,
    since_gain: f32,
}

/// Skills that earn by the unit (climbing, swimming, digging, the cold), so a
/// drop gathers their crumbs up into whole numbers.
fn trickles(skill: Skill) -> bool {
    matches!(
        skill,
        Skill::Climbing | Skill::Swimming | Skill::Excavating | Skill::Survival | Skill::DataDriven
    )
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn track_xp(
    mut commands: Commands,
    time: Res<Time<Real>>,
    sim: Res<Sim>,
    fonts: Res<UiFonts>,
    mut seen: Local<Seen>,
    crab: Query<&PlayerId, With<LocalPlayer>>,
    tracker: Single<(Entity, &mut Node, &mut BackgroundColor, &mut BorderColor), With<Tracker>>,
    mut name: Single<(&mut Text, &mut TextColor), (With<TrackName>, Without<TrackLevel>)>,
    mut level: Single<(&mut Text, &mut TextColor), (With<TrackLevel>, Without<TrackName>)>,
    mut fill: Single<&mut Node, (With<TrackFill>, Without<Tracker>)>,
) {
    let dt = time.delta_secs();
    let Some(me) = crab.single().ok().and_then(|id| sim.crab(*id)) else {
        return;
    };
    let now: Vec<f32> = Skill::ALL.iter().map(|s| me.xp(*s)).collect();
    // First look, or a different crab (a loaded save, a typed code): start
    // from where it stands, with no drop.
    if seen.crab != Some(me.id) || seen.xp.len() != now.len() {
        *seen = Seen {
            crab: Some(me.id),
            gathered: vec![0.0; now.len()],
            since_drop: vec![f32::MAX; now.len()],
            xp: now,
            showing: None,
            since_gain: f32::MAX,
        };
        return;
    }
    let (tracker_entity, mut node, mut back, mut rim) = tracker.into_inner();
    seen.since_gain += dt;
    for (i, skill) in Skill::ALL.into_iter().enumerate() {
        let gained = now[i] - seen.xp[i];
        seen.since_drop[i] += dt;
        if gained < 0.0 {
            // Set back (a typed level): nothing to drop.
            seen.gathered[i] = 0.0;
            continue;
        }
        if gained > 0.0 {
            seen.gathered[i] += gained;
            seen.showing = Some(skill);
            seen.since_gain = 0.0;
        }
        let whole = seen.gathered[i].floor();
        let ready = whole >= 1.0 && (!trickles(skill) || seen.since_drop[i] >= TRICKLE);
        if ready {
            seen.gathered[i] -= whole;
            seen.since_drop[i] = 0.0;
            commands.entity(tracker_entity).with_child((
                XpDrop { age: 0.0 },
                Node {
                    position_type: PositionType::Absolute,
                    right: px(-6),
                    top: px(-10),
                    ..default()
                },
                UiTransform::default(),
                theme::bold(
                    &fonts,
                    format!("+{}", crate::skills_panel::thousands(whole as u32)),
                    theme::LABEL,
                    theme::GOLD,
                ),
            ));
        }
    }
    seen.xp = now;
    let Some(skill) = seen.showing else {
        return;
    };
    let gone = ((seen.since_gain - TRACK_HOLD) / FADE).clamp(0.0, 1.0);
    let display = if gone < 1.0 {
        Display::Flex
    } else {
        Display::None
    };
    if node.display != display {
        node.display = display;
    }
    if gone >= 1.0 {
        return;
    }
    let alpha = 1.0 - gone;
    back.0 = theme::BUTTON.with_alpha(theme::BUTTON.alpha() * alpha);
    *rim = BorderColor::all(theme::RIM_FAINT.with_alpha(theme::RIM_FAINT.alpha() * alpha));
    let t = &sim.tuning;
    let words = skill.name().to_string();
    if name.0.0 != words {
        name.0.0 = words;
    }
    name.1.0 = theme::TEXT.with_alpha(alpha);
    let lv = format!("level {}", me.level(skill, t));
    if level.0.0 != lv {
        level.0.0 = lv;
    }
    level.1.0 = theme::MUTED.with_alpha(theme::MUTED.alpha() * alpha);
    let w = percent(100.0 * me.progress(skill, t));
    if fill.width != w {
        fill.width = w;
    }
}

/// Drops float up out of the tracker and fade.
fn float_drops(
    mut commands: Commands,
    time: Res<Time<Real>>,
    mut drops: Query<(Entity, &mut XpDrop, &mut UiTransform, &mut TextColor)>,
) {
    for (e, mut drop, mut at, mut colour) in &mut drops {
        drop.age += time.delta_secs();
        let k = drop.age / DROP_LIFE;
        if k >= 1.0 {
            commands.entity(e).despawn();
            continue;
        }
        let ease = 1.0 - (1.0 - k) * (1.0 - k);
        at.translation = Val2::px(0.0, -DROP_RISE * ease);
        at.scale = Vec2::splat(if k < 0.15 {
            0.7 + 2.6 * k
        } else {
            1.09 - 0.09 * k
        });
        colour.0 = theme::GOLD.with_alpha((1.0 - k).powf(0.6));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_level_card_names_the_skill_and_keeps_its_lines_short() {
        let card = level_card(Skill::Prying, 15, vec!["You can work clam bed.".into()]);
        assert_eq!(card.title, "Prying 15");
        assert_eq!(card.caption, "LEVEL UP");
        assert_eq!(card.lines, ["You can work clam bed."]);
        let many: Vec<String> = (0..6).map(|i| format!("line {i}")).collect();
        let card = level_card(Skill::Building, 20, many);
        assert_eq!(card.lines.len(), MOST_LINES + 1);
        assert_eq!(card.lines[MOST_LINES], "and 3 more: see Skills");
        assert!(card_life(&card) > card_life(&level_card(Skill::Building, 2, vec![])));
    }

    #[test]
    fn a_guide_card_names_the_step_and_what_it_paid() {
        let first = &crabsim::economy::guide::GUIDE[0];
        let card = guide_card(first);
        assert_eq!(card.caption, "GUIDE");
        assert_eq!(card.title, first.title);
        assert_eq!(card.lines.len(), 1);
        assert!(
            card.lines[0].starts_with("You get 2 kelp"),
            "{:?}",
            card.lines
        );
    }

    #[test]
    fn a_pushed_card_comes_up_with_its_words_then_goes() {
        let mut app = App::new();
        app.insert_resource(Time::<Real>::default())
            .insert_resource(UiFonts {
                bold: Handle::default(),
            })
            .init_resource::<Cards>()
            .add_systems(Startup, spawn)
            .add_systems(Update, show_card);
        app.world_mut().spawn(Window::default());
        app.update();
        app.world_mut().resource_mut::<Cards>().push(level_card(
            Skill::Prying,
            15,
            vec!["You can work clam bed.".into()],
        ));
        app.update();
        app.update();
        let mut q = app.world_mut().query_filtered::<&Node, With<CardBox>>();
        assert_eq!(q.single(app.world()).unwrap().display, Display::Flex);
        let mut t = app.world_mut().query_filtered::<&Text, With<CardTitle>>();
        assert_eq!(t.single(app.world()).unwrap().0, "Prying 15");
        // Long after its life, it is gone.
        app.world_mut()
            .resource_mut::<Cards>()
            .up
            .as_mut()
            .unwrap()
            .1 = 100.0;
        app.update();
        app.update();
        assert_eq!(q.single(app.world()).unwrap().display, Display::None);
    }

    #[test]
    fn the_reward_line_clears_the_corner_buttons_and_the_touch_buttons() {
        for window in [
            Vec2::new(390.0, 844.0),
            Vec2::new(375.0, 667.0),
            Vec2::new(844.0, 390.0),
            Vec2::new(667.0, 375.0),
            Vec2::new(1280.0, 800.0),
        ] {
            let top = line_top(window);
            let left = (window.x - WIDTH) / 2.0;
            let card = Rect::new(left, top, left + WIDTH, top + 110.0);
            // The corner row.
            for slot in crate::corner::Slot::ALL {
                let r = crate::corner::rect(window, slot);
                assert!(card.intersect(r).is_empty(), "{window}: {card:?} {r:?}");
            }
            if crate::settings::narrow(window) {
                for r in crate::input::touch_rects(window) {
                    assert!(card.intersect(r).is_empty(), "{window}: touch {r:?}");
                }
            }
        }
    }
}
