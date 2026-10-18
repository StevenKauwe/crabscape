//! The feel lab: try different ways the crab's attacks can feel, side by
//! side, in preview builds (and native runs). A "FEEL" button under "Start
//! over" (or the backtick key, or 1-9) cycles the presets; a "Juice" button
//! (or backslash) switches the presentation extras off and on, so you can
//! feel what they add. The choice is remembered per build.
//!
//! A preset is two rows of data: its rules (timings, lunge, chaining... in
//! `crabsim::feel`, so the simulation, replays and tests know them) and its
//! [`Juice`] here: how it looks and sounds (anticipation pose, camera shake
//! and kick, flash, debris, sound, the freeze shiver). The live game shows
//! no lab and plays the recommended preset, [`Feel::default`].

use bevy::prelude::*;
use crabsim::{Feel, Sim};

use crate::player::GameSet;
use crate::{reset, save};

/// How a preset looks and sounds. [`Juice::OFF`] is the game's look before
/// the lab: the old anticipation pose and flash, no shake, debris or sound.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Juice {
    /// Anticipation: the body rears back over the wind-up (radians)...
    pub rear: f32,
    /// ...crouching into it (squash, 0..1)...
    pub crouch: f32,
    /// ...and how early the pose gets there: the share of the wind-up done
    /// is raised to this power (below 1: most of the pull-back shows at once,
    /// so a press is answered on the next frame; 2: the old slow start).
    pub curve: f32,
    /// The blow: the body thrown forward (radians) and stretched (0..1).
    pub throw: f32,
    pub stretch: f32,
    /// Camera shake ("trauma", 0..1) for a plain smash that lands; a jab
    /// gets a third, harder blows more.
    pub shake: f32,
    /// Camera kick along the blow, in world units.
    pub kick: f32,
    /// Camera zoom punch on a counter (a share of the view).
    pub zoom: f32,
    /// Size of the impact flash (1 = the old one).
    pub flash: f32,
    /// Shell chips and sand thrown off by a smash that lands.
    pub debris: f32,
    /// Loudness of the sounds (0 = silent) and their pitch (below 1: deeper).
    pub sound: f32,
    pub pitch: f32,
    /// How hard the struck body shivers while the blow holds it still.
    pub jitter: f32,
    /// A counter's payoff: shake, flash and debris times this.
    pub counter: f32,
    /// The streak along each blow (0 = none), so you see which way it goes.
    pub trail: f32,
}

impl Juice {
    pub const OFF: Juice = Juice {
        rear: 0.12,
        crouch: 0.0,
        curve: 2.0,
        throw: 0.16,
        stretch: 0.0,
        shake: 0.0,
        kick: 0.0,
        zoom: 0.0,
        flash: 1.0,
        debris: 0.0,
        sound: 0.0,
        pitch: 1.0,
        jitter: 0.0,
        counter: 1.0,
        trail: 0.0,
    };

    /// Each preset's juice, in `crabsim::feel::ALL`'s order.
    pub fn of(feel: Feel) -> &'static Juice {
        &JUICE[feel as usize]
    }
}

/// The juice table, one row per preset.
const JUICE: [Juice; 9] = [
    // A. Current: the game as it was.
    Juice::OFF,
    // B. Heavy tank: a big, early pull-back and crouch, the whole body thrown
    // in, a deep crunch and a heavy shake.
    Juice {
        rear: 0.24,
        crouch: 0.4,
        curve: 0.45,
        throw: 0.26,
        stretch: 0.35,
        shake: 0.55,
        kick: 9.0,
        zoom: 0.06,
        flash: 1.4,
        debris: 14.0,
        sound: 1.0,
        pitch: 0.8,
        jitter: 3.5,
        counter: 1.6,
        trail: 1.0,
    },
    // C. Snappy brawler: small, instant tells, crisp clicks, short sharp kicks.
    Juice {
        rear: 0.1,
        crouch: 0.15,
        curve: 0.35,
        throw: 0.2,
        stretch: 0.25,
        shake: 0.35,
        kick: 7.0,
        zoom: 0.03,
        flash: 1.0,
        debris: 8.0,
        sound: 0.9,
        pitch: 1.15,
        jitter: 2.5,
        counter: 1.4,
        trail: 1.0,
    },
    // D. Momentum: like Heavy, and everything grows with the blow's strength.
    Juice {
        rear: 0.18,
        crouch: 0.3,
        curve: 0.5,
        throw: 0.24,
        stretch: 0.3,
        shake: 0.45,
        kick: 10.0,
        zoom: 0.04,
        flash: 1.2,
        debris: 11.0,
        sound: 1.0,
        pitch: 0.9,
        jitter: 3.0,
        counter: 1.5,
        trail: 1.0,
    },
    // E. Silksong-ish: barely any tell, light and bright, the bounce is the juice.
    Juice {
        rear: 0.06,
        crouch: 0.1,
        curve: 0.3,
        throw: 0.18,
        stretch: 0.2,
        shake: 0.25,
        kick: 5.0,
        zoom: 0.02,
        flash: 0.9,
        debris: 6.0,
        sound: 0.8,
        pitch: 1.25,
        jitter: 2.0,
        counter: 1.4,
        trail: 1.0,
    },
    // F. Expedition timing: ordinary blows kept modest, so the counter's
    // zoom, boom and burst stand out.
    Juice {
        rear: 0.18,
        crouch: 0.3,
        curve: 0.45,
        throw: 0.22,
        stretch: 0.3,
        shake: 0.4,
        kick: 8.0,
        zoom: 0.12,
        flash: 1.2,
        debris: 10.0,
        sound: 1.0,
        pitch: 0.9,
        jitter: 3.0,
        counter: 2.6,
        trail: 1.0,
    },
    // G. Momentum + Silksong + Expedition: D's growing weight, E's light
    // bright jab, F's big counter payoff (the crush).
    Juice {
        rear: 0.16,
        crouch: 0.28,
        curve: 0.4,
        throw: 0.24,
        stretch: 0.3,
        shake: 0.45,
        kick: 9.0,
        zoom: 0.12,
        flash: 1.2,
        debris: 11.0,
        sound: 1.0,
        pitch: 0.95,
        jitter: 3.0,
        counter: 2.4,
        trail: 1.0,
    },
    // H. Heavy + Silksong + Expedition: B's big rear-back and crunch, a
    // light quick jab, F's counter payoff.
    Juice {
        rear: 0.24,
        crouch: 0.4,
        curve: 0.45,
        throw: 0.26,
        stretch: 0.35,
        shake: 0.55,
        kick: 9.0,
        zoom: 0.12,
        flash: 1.4,
        debris: 14.0,
        sound: 1.0,
        pitch: 0.8,
        jitter: 3.5,
        counter: 2.4,
        trail: 1.0,
    },
    // I. Charge brawler: crisp jabs, a heavy finisher.
    Juice {
        rear: 0.18,
        crouch: 0.3,
        curve: 0.4,
        throw: 0.24,
        stretch: 0.3,
        shake: 0.5,
        kick: 9.0,
        zoom: 0.06,
        flash: 1.3,
        debris: 12.0,
        sound: 1.0,
        pitch: 0.9,
        jitter: 3.0,
        counter: 1.8,
        trail: 1.0,
    },
];

/// The lab's state: which preset, juice on or off, and whether the lab's
/// buttons show at all.
#[derive(Resource, Debug)]
pub struct FeelLab {
    pub feel: Feel,
    pub juice_on: bool,
    /// Preview builds and native runs: the buttons show.
    pub open: bool,
    /// Seconds left showing the preset's name and pitch.
    toast: f32,
}

impl FeelLab {
    /// The juice to show now.
    pub fn juice(&self) -> &'static Juice {
        if self.juice_on {
            Juice::of(self.feel)
        } else {
            &Juice::OFF
        }
    }
}

/// The feel the game starts with: the remembered one in a preview build,
/// else the recommended one.
pub fn starting() -> (Feel, bool) {
    if !lab_open() {
        return (Feel::default(), true);
    }
    let feel = save::read_key("-feel")
        .as_deref()
        .and_then(Feel::parse)
        .unwrap_or_default();
    let juice = save::read_key("-juice").as_deref() != Some("off");
    (feel, juice)
}

/// Is this a build that shows the lab? Previews (`/pr-<n>/`, `/branches/..`),
/// `?feel` in the address, and native runs.
fn lab_open() -> bool {
    #[cfg(target_arch = "wasm32")]
    {
        save::is_preview()
            || web_sys::window()
                .and_then(|w| w.location().search().ok())
                .is_some_and(|q| q.contains("feel"))
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        true
    }
}

/// How long the name and pitch show after a switch (seconds).
const TOAST: f32 = 2.4;
const GAP: f32 = 8.0;

pub struct FeelPlugin;

impl Plugin for FeelPlugin {
    fn build(&self, app: &mut App) {
        let (feel, juice_on) = starting();
        let open = lab_open();
        app.insert_resource(FeelLab {
            feel,
            juice_on,
            open,
            toast: if open { TOAST } else { 0.0 },
        })
        .add_systems(Startup, (spawn_lab, slow_motion))
        .add_systems(Update, switch.in_set(GameSet::Input))
        .add_systems(Update, (show_lab, place_buttons).in_set(GameSet::Present));
    }
}

#[derive(Component, Clone, Copy, PartialEq, Eq)]
enum LabButton {
    Feel,
    Juice,
}

#[derive(Component)]
struct ButtonLabel;

#[derive(Component)]
struct Toast;

#[derive(Component)]
struct ToastText;

/// The two buttons: under "Start over", the same size, stacked.
fn button_rect(window: Vec2, which: LabButton) -> Rect {
    let above = reset::button_rect(window);
    let rows = match which {
        LabButton::Feel => 1.0,
        LabButton::Juice => 2.0,
    };
    let down = Vec2::Y * (above.height() + GAP) * rows;
    Rect::from_corners(above.min + down, above.max + down)
}

fn spawn_lab(mut commands: Commands, lab: Res<FeelLab>, window: Single<&Window>) {
    if !lab.open {
        return;
    }
    for which in [LabButton::Feel, LabButton::Juice] {
        let r = button_rect(window.size(), which);
        commands
            .spawn((
                which,
                Node {
                    position_type: PositionType::Absolute,
                    top: px(r.min.y),
                    right: px(10),
                    width: px(r.width()),
                    height: px(r.height()),
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    border: UiRect::all(px(1.5)),
                    border_radius: BorderRadius::all(px(r.height() / 2.0)),
                    ..default()
                },
                BorderColor::all(Color::srgba(0.55, 0.95, 0.85, 0.45)),
                BackgroundColor(Color::srgba(0.05, 0.12, 0.14, 0.55)),
            ))
            .with_children(|b| {
                b.spawn((
                    ButtonLabel,
                    Text::new(""),
                    TextFont {
                        font_size: FontSize::Px(14.0),
                        ..default()
                    },
                    TextColor(Color::srgba(0.8, 1.0, 0.92, 0.9)),
                ));
            });
    }
    // The name and pitch, across the top-middle, for a moment after a switch.
    commands
        .spawn((
            Toast,
            Node {
                position_type: PositionType::Absolute,
                top: px(104),
                left: percent(8),
                right: percent(8),
                justify_content: JustifyContent::Center,
                ..default()
            },
        ))
        .with_children(|t| {
            t.spawn((
                ToastText,
                Text::new(""),
                TextFont {
                    font_size: FontSize::Px(16.0),
                    ..default()
                },
                TextColor(Color::NONE),
                TextLayout::justify(Justify::Center),
            ));
        });
}

/// `?slow=8` in the address plays the whole game 8 times slower (the
/// simulation and every effect alike), to look at a blow frame by frame
/// (the WebKit test's frame strips use it).
fn slow_motion(mut time: ResMut<Time<Virtual>>) {
    #[cfg(target_arch = "wasm32")]
    {
        let factor = web_sys::window()
            .and_then(|w| w.location().search().ok())
            .and_then(|q| {
                q.trim_start_matches('?')
                    .split('&')
                    .find_map(|kv| kv.strip_prefix("slow=")?.parse::<f32>().ok())
            })
            .filter(|f| *f >= 1.0);
        if let Some(f) = factor {
            time.set_relative_speed(1.0 / f);
        }
    }
    #[cfg(not(target_arch = "wasm32"))]
    let _ = &mut time;
}

/// Backtick (or the FEEL button) for the next preset, 1-9 for one of them,
/// backslash (or the Juice button) for juice off/on.
#[allow(clippy::too_many_arguments)]
fn switch(
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    touches: Res<Touches>,
    window: Single<&Window>,
    mut lab: ResMut<FeelLab>,
    mut sim: ResMut<Sim>,
) {
    if !lab.open {
        return;
    }
    let size = window.size();
    let hit = |which| {
        let r = button_rect(size, which).inflate(6.0);
        (mouse.just_pressed(MouseButton::Left)
            && window.cursor_position().is_some_and(|p| r.contains(p)))
            || touches
                .iter_just_pressed()
                .any(|t| r.contains(t.position()))
    };
    let digits = [
        KeyCode::Digit1,
        KeyCode::Digit2,
        KeyCode::Digit3,
        KeyCode::Digit4,
        KeyCode::Digit5,
        KeyCode::Digit6,
        KeyCode::Digit7,
        KeyCode::Digit8,
        KeyCode::Digit9,
    ];
    let mut feel = lab.feel;
    if keys.just_pressed(KeyCode::Backquote) || hit(LabButton::Feel) {
        feel = feel.next();
    }
    for (key, f) in digits.into_iter().zip(crabsim::feel::ALL) {
        if keys.just_pressed(key) {
            feel = f;
        }
    }
    let juice = lab.juice_on ^ (keys.just_pressed(KeyCode::Backslash) || hit(LabButton::Juice));
    if feel != lab.feel {
        lab.feel = feel;
        lab.toast = TOAST;
        sim.set_feel(feel);
        save::write_key("-feel", feel.name());
    }
    if juice != lab.juice_on {
        lab.juice_on = juice;
        lab.toast = TOAST;
        save::write_key("-juice", if juice { "on" } else { "off" });
    }
}

#[allow(clippy::type_complexity)]
fn show_lab(
    time: Res<Time<Real>>,
    mut lab: ResMut<FeelLab>,
    buttons: Query<(&LabButton, &Children)>,
    mut labels: Query<&mut Text, (With<ButtonLabel>, Without<ToastText>)>,
    mut toast: Query<(&mut Text, &mut TextColor), With<ToastText>>,
) {
    if !lab.open {
        return;
    }
    lab.toast = (lab.toast - time.delta_secs()).max(0.0);
    if lab.is_changed() {
        for (which, kids) in &buttons {
            let text = match which {
                LabButton::Feel => format!("FEEL {}", lab.feel.letter()),
                LabButton::Juice => format!("Juice {}", if lab.juice_on { "on" } else { "off" }),
            };
            for kid in kids.iter() {
                if let Ok(mut t) = labels.get_mut(kid)
                    && t.0 != text
                {
                    t.0 = text.clone();
                }
            }
        }
        #[cfg(target_arch = "wasm32")]
        if let Some(canvas) = web_sys::window()
            .and_then(|w| w.document())
            .and_then(|d| d.get_element_by_id("crabscape"))
        {
            let juice = if lab.juice_on { "on" } else { "off" };
            let _ = canvas.set_attribute("data-feel", &format!("{} {juice}", lab.feel.name()));
        }
    }
    for (mut text, mut colour) in &mut toast {
        let juice = if lab.juice_on { "" } else { "  (juice off)" };
        let line = format!("{}{juice}\n{}", lab.feel.title(), lab.feel.pitch());
        if text.0 != line {
            text.0 = line;
        }
        // Full for most of its time, fading over the last half second.
        let alpha = (lab.toast / 0.5).min(1.0);
        colour.set_if_neq(TextColor(Color::srgba(0.85, 1.0, 0.93, 0.95 * alpha)));
    }
}

/// Keep the buttons where [`button_rect`] says, as the window changes.
fn place_buttons(window: Single<&Window>, mut buttons: Query<(&LabButton, &mut Node)>) {
    for (which, mut node) in &mut buttons {
        let top = px(button_rect(window.size(), *which).min.y);
        if node.top != top {
            node.top = top;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crabsim::feel::ALL;

    #[test]
    fn every_preset_but_current_has_juice_and_current_is_the_old_look() {
        assert_eq!(*Juice::of(Feel::Current), Juice::OFF);
        for f in ALL.into_iter().skip(1) {
            let j = Juice::of(f);
            assert!(j.sound > 0.0 && j.shake > 0.0 && j.debris > 0.0, "{f:?}");
            assert!(j.curve < 1.0, "{f:?}: the pull-back shows at once");
        }
    }

    #[test]
    fn the_lab_buttons_sit_under_start_over_and_clear_the_touch_buttons() {
        for size in [Vec2::new(390.0, 844.0), Vec2::new(844.0, 390.0)] {
            let reset = reset::button_rect(size);
            let feel = button_rect(size, LabButton::Feel);
            let juice = button_rect(size, LabButton::Juice);
            assert!(feel.min.y > reset.max.y && juice.min.y > feel.max.y);
            assert!(juice.max.y < size.y * 0.5, "{size}: out of the way");
        }
    }
}
