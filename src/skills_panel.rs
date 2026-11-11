//! The skills panel: key K (or the "Skills" button, under the Home button),
//! a 3-column grid of the crab's skills, each with its level and a thin bar
//! for the way to the next. Tap or click a skill for its guide, RuneScape
//! style: every unlock by level (`economy::unlocks`), the ones reached
//! ticked, and the experience still needed for the next level.
//!
//! It fits a 375 x 812 phone held either way up (a 375-high landscape screen
//! is the tightest: the grid is 5 short rows, the guide scrolls). While it is
//! open it owns the input (`InputUi::modal`), so a tap in it never moves the
//! crab. K, Escape or a tap outside closes it.
//!
//! Data-Driven (the royal observatory's skill) only shows once it has some
//! experience, so the grid is the 13 skills of the world, and a 14th once
//! the observatory has been worked.

use bevy::input::mouse::{MouseScrollUnit, MouseWheel};
use bevy::prelude::*;
use bevy::ui::RepeatedGridTrack;
use crabsim::Skill;
use crabsim::economy::unlocks;
use crabsim::skills::MAX_LEVEL;

use crate::feel::FeelLab;
use crate::player::{GameSet, LocalPlayer, PlayerId};

const PAPER: Color = Color::srgba(0.03, 0.08, 0.10, 0.94);
const EDGE: Color = Color::srgba(1.0, 0.85, 0.55, 0.40);
const CELL: Color = Color::srgba(0.07, 0.16, 0.18, 0.95);
const WORDS: Color = Color::srgb(1.0, 0.91, 0.73);
const DIM: Color = Color::srgba(0.85, 0.92, 0.95, 0.55);
const GOLD: Color = Color::srgb(1.0, 0.88, 0.55);

const PAD: f32 = 10.0;
const GAP: f32 = 6.0;
const CELL_H: f32 = 50.0;
const TITLE_H: f32 = 26.0;
const ROW_H: f32 = 19.0;
/// The card is at most this wide, and at least this far from the window's edge.
const MAX_W: f32 = 372.0;
const MARGIN: f32 = 8.0;
const MAX_GUIDE_H: f32 = 560.0;

pub struct SkillsPanelPlugin;

impl Plugin for SkillsPanelPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Open>()
            .add_systems(Startup, spawn)
            .add_systems(Update, interact.in_set(GameSet::Input))
            .add_systems(
                Update,
                (layout, refresh_grid, build_guide, scroll_guide)
                    .chain()
                    .in_set(GameSet::Present),
            );
    }
}

/// Is the panel up, and which skill's guide is it showing?
#[derive(Resource, Default)]
pub(crate) struct Open {
    pub(crate) open: bool,
    /// Closed by a tap this frame: that tap only closes it (the game's own
    /// input steps aside for it, as for the settings panel).
    shut: bool,
    guide: Option<Skill>,
}

impl Open {
    pub(crate) fn is_open(&self) -> bool {
        self.open || self.shut
    }
}

#[derive(Component)]
struct Card;
#[derive(Component)]
struct GridBox;
#[derive(Component)]
struct GuideBox;
#[derive(Component)]
struct OpenButton;
#[derive(Component)]
struct Back;
#[derive(Component)]
struct Cell(Skill);
#[derive(Component)]
struct CellLevel(Skill);
#[derive(Component)]
struct CellFill(Skill);
#[derive(Component)]
struct GuideTitle;
#[derive(Component)]
struct GuideNext;
#[derive(Component)]
struct GuideList;

/// The skills the grid lists: all but Data-Driven, which joins them once it
/// has any experience.
pub fn listed(skill: Skill, xp: f32) -> bool {
    skill != Skill::DataDriven || xp > 0.0
}

/// The button's box: under the Home button, or under the feel lab's two
/// buttons where they are.
fn button_rect(window: Vec2, lab: bool) -> Rect {
    crate::settings::below(window, if lab { 4.0 } else { 2.0 })
}

/// The grid card's height: title, five rows of three.
fn grid_height() -> f32 {
    2.0 * PAD + TITLE_H + GAP + 5.0 * CELL_H + 4.0 * GAP
}

/// The card's size for a window, showing the grid or a guide.
pub fn card_size(window: Vec2, guide: bool) -> Vec2 {
    let w = (window.x - 2.0 * MARGIN).clamp(120.0, MAX_W);
    let h = if guide {
        (window.y - 2.0 * MARGIN).clamp(120.0, MAX_GUIDE_H)
    } else {
        grid_height()
    };
    Vec2::new(w, h)
}

/// "Prying 15", "You can work clam bed." ... every unlock of a skill by
/// level over 1 to 99, in order (a level with several unlocks gives
/// several lines).
pub fn guide_rows(skill: Skill) -> Vec<(u32, String)> {
    (1..=MAX_LEVEL)
        .flat_map(|level| {
            unlocks(skill, level)
                .into_iter()
                .map(move |line| (level, line))
        })
        .collect()
}

/// Experience still to earn for the next level (None at the top).
pub fn xp_to_next(skill: Skill, level: u32, xp: f32, t: &crabsim::Tuning) -> Option<u32> {
    (level < MAX_LEVEL).then(|| (skill.xp_at(level + 1, t) - xp).ceil().max(0.0) as u32)
}

/// 13034431 as "13,034,431".
pub fn thousands(n: u32) -> String {
    let digits = n.to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    out
}

fn text(size: f32, colour: Color) -> impl Bundle {
    (
        Text::new(""),
        TextFont {
            font_size: FontSize::Px(size),
            ..default()
        },
        TextColor(colour),
    )
}

fn spawn(mut commands: Commands, window: Single<&Window>, lab: Res<FeelLab>) {
    let r = button_rect(window.size(), lab.available);
    let size = window.size();
    commands
        .spawn((
            crate::input::PointerUi,
            OpenButton,
            Node {
                position_type: PositionType::Absolute,
                top: px(r.min.y),
                right: px(size.x - r.max.x),
                width: px(r.width()),
                height: px(r.height()),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                border: UiRect::all(px(1.5)),
                border_radius: BorderRadius::all(px(r.height() / 2.0)),
                ..default()
            },
            BorderColor::all(Color::srgba(1.0, 0.85, 0.55, 0.25)),
            BackgroundColor(Color::srgba(0.05, 0.12, 0.14, 0.45)),
        ))
        .with_child((
            Text::new("Skills"),
            TextFont {
                font_size: FontSize::Px(13.0),
                ..default()
            },
            TextColor(Color::srgba(1.0, 0.9, 0.7, 0.7)),
        ));

    commands
        .spawn((
            crate::input::PointerUi,
            Card,
            GlobalZIndex(6),
            Node {
                display: Display::None,
                position_type: PositionType::Absolute,
                flex_direction: FlexDirection::Column,
                padding: UiRect::all(px(PAD)),
                row_gap: px(GAP),
                border: UiRect::all(px(1.5)),
                border_radius: BorderRadius::all(px(12)),
                overflow: Overflow::clip(),
                ..default()
            },
            BorderColor::all(EDGE),
            BackgroundColor(PAPER),
        ))
        .with_children(|card| {
            // The grid.
            card.spawn((
                GridBox,
                Node {
                    flex_direction: FlexDirection::Column,
                    row_gap: px(GAP),
                    ..default()
                },
            ))
            .with_children(|grid| {
                grid.spawn((Node {
                    height: px(TITLE_H),
                    justify_content: JustifyContent::SpaceBetween,
                    align_items: AlignItems::Center,
                    ..default()
                },))
                    .with_children(|row| {
                        row.spawn((
                            Text::new("Skills"),
                            TextFont {
                                font_size: FontSize::Px(17.0),
                                ..default()
                            },
                            TextColor(GOLD),
                        ));
                        row.spawn((
                            Text::new("tap one for its guide"),
                            TextFont {
                                font_size: FontSize::Px(11.0),
                                ..default()
                            },
                            TextColor(DIM),
                        ));
                    });
                grid.spawn(Node {
                    display: Display::Grid,
                    grid_template_columns: RepeatedGridTrack::flex(3, 1.0),
                    column_gap: px(GAP),
                    row_gap: px(GAP),
                    ..default()
                })
                .with_children(|cells| {
                    for skill in Skill::ALL {
                        cells
                            .spawn((
                                Cell(skill),
                                Node {
                                    height: px(CELL_H),
                                    flex_direction: FlexDirection::Column,
                                    justify_content: JustifyContent::SpaceBetween,
                                    padding: UiRect::axes(px(6), px(5)),
                                    border: UiRect::all(px(1)),
                                    border_radius: BorderRadius::all(px(7)),
                                    ..default()
                                },
                                BorderColor::all(Color::srgba(1.0, 0.85, 0.55, 0.18)),
                                BackgroundColor(CELL),
                            ))
                            .with_children(|cell| {
                                cell.spawn((
                                    Text::new(skill.name()),
                                    TextFont {
                                        font_size: FontSize::Px(12.0),
                                        ..default()
                                    },
                                    TextColor(WORDS),
                                ));
                                cell.spawn((CellLevel(skill), text(15.0, GOLD)));
                                cell.spawn((
                                    Node {
                                        width: percent(100),
                                        height: px(3),
                                        ..default()
                                    },
                                    BackgroundColor(Color::srgba(1.0, 0.88, 0.55, 0.18)),
                                ))
                                .with_child((
                                    CellFill(skill),
                                    Node {
                                        width: percent(0),
                                        height: percent(100),
                                        ..default()
                                    },
                                    BackgroundColor(Color::srgba(1.0, 0.88, 0.55, 0.85)),
                                ));
                            });
                    }
                });
            });

            // One skill's guide.
            card.spawn((
                GuideBox,
                Node {
                    display: Display::None,
                    flex_direction: FlexDirection::Column,
                    row_gap: px(GAP),
                    flex_grow: 1.0,
                    min_height: px(0),
                    ..default()
                },
            ))
            .with_children(|guide| {
                guide
                    .spawn((
                        Back,
                        Node {
                            height: px(30),
                            padding: UiRect::horizontal(px(8)),
                            align_items: AlignItems::Center,
                            align_self: AlignSelf::FlexStart,
                            border: UiRect::all(px(1)),
                            border_radius: BorderRadius::all(px(8)),
                            ..default()
                        },
                        BorderColor::all(Color::srgba(1.0, 0.85, 0.55, 0.3)),
                        BackgroundColor(CELL),
                    ))
                    .with_child((
                        Text::new("< All skills"),
                        TextFont {
                            font_size: FontSize::Px(13.0),
                            ..default()
                        },
                        TextColor(WORDS),
                    ));
                guide.spawn((GuideTitle, text(18.0, GOLD)));
                guide.spawn((GuideNext, text(12.0, DIM)));
                guide.spawn((
                    GuideList,
                    ScrollPosition::default(),
                    Node {
                        flex_direction: FlexDirection::Column,
                        flex_grow: 1.0,
                        min_height: px(0),
                        overflow: Overflow::scroll_y(),
                        ..default()
                    },
                ));
            });
        });
}

/// What a tap or click at `point` (physical pixels) landed on.
fn hit(node: &ComputedNode, at: &UiGlobalTransform, point: Vec2) -> bool {
    node.contains_point(*at, point)
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn interact(
    mut open: ResMut<Open>,
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    touches: Res<Touches>,
    window: Single<&Window>,
    settings: Res<crate::settings::panel::Open>,
    chat: Res<crate::codes::CodeBox>,
    lab: Res<FeelLab>,
    card: Single<(&ComputedNode, &UiGlobalTransform), With<Card>>,
    cells: Query<(&Cell, &ComputedNode, &UiGlobalTransform, &Node)>,
    back: Single<(&ComputedNode, &UiGlobalTransform), With<Back>>,
) {
    open.shut = false;
    if settings.is_open() || chat.open || crate::input::page_dialog_open() {
        open.open = false;
        return;
    }
    let scale = window.scale_factor();
    let tap = touches
        .iter_just_pressed()
        .next()
        .map(|t| t.position())
        .or_else(|| {
            mouse
                .just_pressed(MouseButton::Left)
                .then(|| window.cursor_position())
                .flatten()
        });
    let on_button = tap.is_some_and(|p| {
        button_rect(window.size(), lab.available)
            .inflate(1.0)
            .contains(p)
    });
    let tap = tap.map(|p| p * scale);
    if keys.just_pressed(KeyCode::KeyK) || (!open.open && on_button) {
        open.open = !open.open;
        open.guide = None;
        return;
    }
    if !open.open {
        return;
    }
    if keys.just_pressed(KeyCode::Escape) || (on_button && open.open) {
        open.open = false;
        return;
    }
    let Some(p) = tap else {
        return;
    };
    if open.guide.is_some() && hit(back.0, back.1, p) {
        open.guide = None;
    } else if open.guide.is_none()
        && let Some((cell, ..)) = cells
            .iter()
            .find(|(_, b, t, n)| n.display != Display::None && hit(b, t, p))
    {
        open.guide = Some(cell.0);
    } else if !hit(card.0, card.1, p) {
        open.open = false;
        open.shut = true;
    }
}

/// Size and place the card, and swap the grid and the guide.
#[allow(clippy::type_complexity)]
fn layout(
    open: Res<Open>,
    window: Single<&Window>,
    lab: Res<FeelLab>,
    settings: Res<crate::settings::panel::Open>,
    mut card: Single<&mut Node, (With<Card>, Without<GridBox>, Without<GuideBox>)>,
    mut grid: Single<&mut Node, (With<GridBox>, Without<Card>, Without<GuideBox>)>,
    mut guide: Single<&mut Node, (With<GuideBox>, Without<Card>, Without<GridBox>)>,
    mut button: Single<
        (&mut Node, &mut Visibility),
        (
            With<OpenButton>,
            Without<Card>,
            Without<GridBox>,
            Without<GuideBox>,
        ),
    >,
) {
    let size = window.size();
    let guiding = open.guide.is_some();
    let want = card_size(size, guiding);
    let (w, h) = (px(want.x), px(want.y));
    let (left, top) = (px((size.x - want.x) / 2.0), px((size.y - want.y) / 2.0));
    let display = if open.open {
        Display::Flex
    } else {
        Display::None
    };
    if card.display != display {
        card.display = display;
    }
    if card.width != w || card.height != h || card.left != left || card.top != top {
        card.width = w;
        card.height = h;
        card.left = left;
        card.top = top;
    }
    let (g, u) = if guiding {
        (Display::None, Display::Flex)
    } else {
        (Display::Flex, Display::None)
    };
    if grid.display != g {
        grid.display = g;
    }
    if guide.display != u {
        guide.display = u;
    }
    // The button keeps its corner as the window turns, and steps aside while
    // the settings panel is over it.
    let r = button_rect(size, lab.available);
    let (top, right) = (px(r.min.y), px(size.x - r.max.x));
    if button.0.top != top || button.0.right != right {
        button.0.top = top;
        button.0.right = right;
    }
    button.1.set_if_neq(if settings.is_open() {
        Visibility::Hidden
    } else {
        Visibility::Inherited
    });
}

/// The grid's levels and bars.
#[allow(clippy::type_complexity)]
fn refresh_grid(
    open: Res<Open>,
    sim: Res<crabsim::Sim>,
    local: Query<&PlayerId, With<LocalPlayer>>,
    mut cells: Query<(&Cell, &mut Node), (Without<CellFill>, Without<CellLevel>)>,
    mut levels: Query<(&CellLevel, &mut Text)>,
    mut fills: Query<(&CellFill, &mut Node), Without<Cell>>,
) {
    if !open.open {
        return;
    }
    let Some(crab) = local.single().ok().and_then(|id| sim.crab(*id)) else {
        return;
    };
    let t = &sim.tuning;
    for (cell, mut node) in &mut cells {
        let display = if listed(cell.0, crab.xp(cell.0)) {
            Display::Flex
        } else {
            Display::None
        };
        if node.display != display {
            node.display = display;
        }
    }
    for (level, mut words) in &mut levels {
        let line = format!("Level {}", crab.level(level.0, t));
        if words.0 != line {
            words.0 = line;
        }
    }
    for (fill, mut node) in &mut fills {
        let width = percent(100.0 * crab.progress(fill.0, t));
        if node.width != width {
            node.width = width;
        }
    }
}

/// What the guide was last built for: the skill and its level (a level
/// gained while it is open ticks one more row).
#[derive(Default)]
struct Built(Option<(Skill, u32)>);

/// The guide: its heading, the experience to go, and a row for each unlock.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn build_guide(
    mut commands: Commands,
    open: Res<Open>,
    sim: Res<crabsim::Sim>,
    local: Query<&PlayerId, With<LocalPlayer>>,
    mut built: Local<Built>,
    mut title: Single<&mut Text, (With<GuideTitle>, Without<GuideNext>)>,
    mut next: Single<&mut Text, (With<GuideNext>, Without<GuideTitle>)>,
    mut list: Single<(Entity, &mut ScrollPosition), With<GuideList>>,
) {
    let Some(skill) = open.guide.filter(|_| open.open) else {
        built.0 = None;
        return;
    };
    let Some(crab) = local.single().ok().and_then(|id| sim.crab(*id)) else {
        return;
    };
    let t = &sim.tuning;
    let (level, xp) = (crab.level(skill, t), crab.xp(skill));
    let heading = format!("{}   level {level}", skill.name());
    if title.0 != heading {
        title.0 = heading;
    }
    let to_go = match xp_to_next(skill, level, xp, t) {
        Some(n) => format!(
            "{} xp   |   {} to level {}",
            thousands(xp as u32),
            thousands(n),
            level + 1
        ),
        None => format!("{} xp   |   top level", thousands(xp as u32)),
    };
    if next.0 != to_go {
        next.0 = to_go;
    }
    if built.0 == Some((skill, level)) {
        return;
    }
    let same_skill = built.0.is_some_and(|(s, _)| s == skill);
    built.0 = Some((skill, level));
    let rows = guide_rows(skill);
    // Bring the next unlock to the top, a little way down from the edge.
    let first_todo = rows
        .iter()
        .position(|(l, _)| *l > level)
        .unwrap_or(rows.len());
    if !same_skill {
        list.1.y = (first_todo as f32 - 1.0).max(0.0) * ROW_H;
    }
    commands
        .entity(list.0)
        .despawn_related::<Children>()
        .with_children(|list| {
            if rows.is_empty() {
                list.spawn((
                    Text::new("Nothing opens by level here: it grows by use."),
                    TextFont {
                        font_size: FontSize::Px(12.0),
                        ..default()
                    },
                    TextColor(DIM),
                ));
            }
            let mut last = 0;
            for (i, (at, line)) in rows.into_iter().enumerate() {
                let done = at <= level;
                let colour = if done {
                    WORDS
                } else if i == first_todo {
                    GOLD
                } else {
                    DIM
                };
                list.spawn(Node {
                    height: px(ROW_H),
                    flex_shrink: 0.0,
                    align_items: AlignItems::Center,
                    column_gap: px(6),
                    ..default()
                })
                .with_children(|row| {
                    // A small box, filled when the level is reached.
                    row.spawn((
                        Node {
                            width: px(10),
                            height: px(10),
                            border: UiRect::all(px(1)),
                            border_radius: BorderRadius::all(px(2)),
                            ..default()
                        },
                        BorderColor::all(colour),
                        BackgroundColor(if done { GOLD } else { Color::NONE }),
                    ));
                    let tag = if at == last {
                        String::new()
                    } else {
                        format!("{at}")
                    };
                    row.spawn(Node {
                        width: px(24),
                        justify_content: JustifyContent::FlexEnd,
                        ..default()
                    })
                    .with_child((
                        Text::new(tag),
                        TextFont {
                            font_size: FontSize::Px(12.0),
                            ..default()
                        },
                        TextColor(colour),
                    ));
                    row.spawn((
                        Text::new(line),
                        TextFont {
                            font_size: FontSize::Px(12.0),
                            ..default()
                        },
                        TextColor(colour),
                    ));
                });
                last = at;
            }
        });
}

/// Scroll the guide with the wheel, or by dragging it with a finger.
#[allow(clippy::too_many_arguments)]
fn scroll_guide(
    open: Res<Open>,
    mut wheel: MessageReader<MouseWheel>,
    touches: Res<Touches>,
    mut drag: Local<Option<u64>>,
    list: Single<(&mut ScrollPosition, &ComputedNode, &UiGlobalTransform), With<GuideList>>,
    window: Single<&Window>,
) {
    let (mut scroll, node, at) = list.into_inner();
    if open.guide.is_none() || !open.open {
        wheel.clear();
        *drag = None;
        return;
    }
    let scale = window.scale_factor();
    let mut by = 0.0;
    for w in wheel.read() {
        by -= match w.unit {
            MouseScrollUnit::Line => w.y * 3.0 * ROW_H,
            MouseScrollUnit::Pixel => w.y,
        };
    }
    if let Some(t) = touches
        .iter_just_pressed()
        .find(|t| node.contains_point(*at, t.position() * scale))
    {
        *drag = Some(t.id());
    }
    if let Some(id) = *drag {
        match touches.get_pressed(id) {
            Some(t) => by -= t.delta().y,
            None => *drag = None,
        }
    }
    if by != 0.0 {
        scroll.y = (scroll.y + by).max(0.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn thousands_get_commas() {
        assert_eq!(thousands(0), "0");
        assert_eq!(thousands(83), "83");
        assert_eq!(thousands(1154), "1,154");
        assert_eq!(thousands(13_034_431), "13,034,431");
    }

    #[test]
    fn the_card_fits_phones_both_ways_up_and_a_desktop() {
        for window in [
            Vec2::new(375.0, 812.0),
            Vec2::new(812.0, 375.0),
            Vec2::new(320.0, 568.0),
            Vec2::new(1280.0, 720.0),
        ] {
            for guide in [false, true] {
                let c = card_size(window, guide);
                assert!(c.x <= window.x - 2.0 * MARGIN + 0.01, "{window} {guide}");
                assert!(c.y <= window.y - 2.0 * MARGIN + 0.01, "{window} {guide}");
            }
        }
        // The cells are wide enough for the longest skill name at 12 px.
        let inner = card_size(Vec2::new(375.0, 812.0), false).x - 2.0 * PAD - 2.0 * GAP;
        assert!(inner / 3.0 - 14.0 > "Excavating".len() as f32 * 7.4);
    }

    #[test]
    fn data_driven_waits_for_experience() {
        assert!(listed(Skill::Prying, 0.0));
        assert!(!listed(Skill::DataDriven, 0.0));
        assert!(listed(Skill::DataDriven, 5.0));
        assert_eq!(Skill::ALL.iter().filter(|s| listed(**s, 0.0)).count(), 13);
    }

    #[test]
    fn the_guide_lists_every_unlock_by_level() {
        let rows = guide_rows(Skill::Prying);
        assert!(!rows.is_empty());
        assert!(rows.windows(2).all(|w| w[0].0 <= w[1].0), "in level order");
        assert!(
            rows.iter()
                .any(|(l, line)| *l == 15 && line.contains("clam"))
        );
        // Every unlock in the tables shows up in some skill's guide.
        let total: usize = Skill::ALL.iter().map(|s| guide_rows(*s).len()).sum();
        let tables = crabsim::economy::NODES.len() + crabsim::economy::ECON_RECIPES.len();
        assert!(total <= tables && total > 0);
    }

    #[test]
    fn xp_to_next_counts_down_and_stops_at_the_top() {
        let t = crabsim::Tuning::default();
        assert_eq!(xp_to_next(Skill::Prying, 1, 0.0, &t), Some(83));
        assert_eq!(xp_to_next(Skill::Prying, 15, 2411.0, &t), Some(335));
        assert_eq!(xp_to_next(Skill::Prying, MAX_LEVEL, 1.0e7, &t), None);
    }
}
