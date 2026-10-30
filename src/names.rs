//! Names over crabs, for playing together: each player goes by a name, set
//! in Settings (Co-op tab, "Your name"), kept on the device (Start over keeps
//! it: it's the player's, not the crab's) and carried in an exported
//! character. Until one is set it's "Crab" and two digits, picked once.
//!
//! A friend's crab shows its player's name over it, small and pale, at the
//! same size whatever the zoom; your own crab shows yours only while a
//! friend's crab is here too (alone, there's nobody to tell apart). The name
//! goes over the co-op link when friends meet and whenever it changes
//! (`crabsim::net::Msg::Name`, `coop.rs`); a friend on a game from before
//! names drops it unread and shows no name over your crab, and theirs has
//! none over it here.
#![cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]

use bevy::prelude::*;
use crabsim::PlayerId;
use crabsim::net::clean_name;

use crate::player::{Crab, GameSet, LocalPlayer};

/// Where the device keeps the name (kept through Start over, `save.rs`).
pub(crate) const KEPT: &str = "-name";
/// How high over the crab's feet the name sits (world units): above the
/// stamina rings (`stamina.rs`).
const LIFT: f32 = 135.0;
/// The words' size (pixels).
const SIZE: f32 = 12.0;

/// The name this player goes by, and the names friends' devices sent.
#[derive(Resource, Debug, Default)]
pub struct Names {
    mine: String,
    others: Vec<(PlayerId, String)>,
    /// Ours changed since it was last sent to everyone.
    pub(crate) changed: bool,
}

impl Names {
    /// The name this player goes by.
    pub fn mine(&self) -> &str {
        &self.mine
    }

    /// Go by `raw` (cleaned) from now on: kept, and sent to friends. A name
    /// with nothing left once cleaned changes nothing.
    pub fn set_mine(&mut self, raw: &str) -> bool {
        let Some(name) = clean_name(raw) else {
            return false;
        };
        if name != self.mine {
            crate::save::write_key(KEPT, &name);
            self.mine = name;
            self.changed = true;
        }
        true
    }

    /// A friend's device says crab `id`'s player goes by `name`.
    pub fn heard(&mut self, id: PlayerId, name: &str) {
        let Some(name) = clean_name(name) else {
            return;
        };
        match self.others.iter_mut().find(|(who, _)| *who == id) {
            Some(known) => known.1 = name,
            None => self.others.push((id, name)),
        }
    }

    /// The name over crab `id`: a friend's as their device sent it, ours for
    /// ours.
    pub fn of(&self, id: PlayerId, me: PlayerId) -> Option<&str> {
        if id == me {
            return Some(&self.mine);
        }
        self.others
            .iter()
            .find(|(who, _)| *who == id)
            .map(|(_, n)| n.as_str())
    }

    /// Every friend's name we know, for the browser tests (`data-names`).
    pub fn others(&self) -> impl Iterator<Item = &str> {
        self.others.iter().map(|(_, n)| n.as_str())
    }
}

/// The name a new player goes by: "Crab" and a number from 10 to 99, picked
/// from `random` (0..1).
fn first_name(random: f64) -> String {
    let n = 10 + ((random * 90.0) as u32).min(89);
    format!("Crab {n}")
}

/// A number in 0..1 from the browser (or, on a desktop, the clock).
fn random() -> f64 {
    #[cfg(target_arch = "wasm32")]
    {
        js_sys::Math::random()
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.subsec_nanos());
        f64::from(nanos % 1000) / 1000.0
    }
}

/// The kept name, or a first one (kept from now on).
fn kept_or_first() -> Names {
    let mine = match crate::save::read_key(KEPT).and_then(|n| clean_name(&n)) {
        Some(name) => name,
        None => {
            let name = first_name(random());
            crate::save::write_key(KEPT, &name);
            name
        }
    };
    Names { mine, ..default() }
}

/// A name shown over a crab.
#[derive(Component)]
struct Tag(Entity);

pub struct NamesPlugin;

impl Plugin for NamesPlugin {
    fn build(&self, app: &mut App) {
        // Placed after every frame's moves (the camera's too), before the
        // UI is laid out, so a name never trails its crab by a frame.
        app.insert_resource(kept_or_first())
            .add_systems(Update, tag_crabs.in_set(GameSet::Present))
            .add_systems(PostUpdate, place_tags.before(bevy::ui::UiSystems::Prepare));
        #[cfg(target_arch = "wasm32")]
        app.add_systems(Update, tell_tests.in_set(GameSet::Present));
    }
}

/// A tag for each crab that has none; a tag whose crab is gone goes too.
fn tag_crabs(
    mut commands: Commands,
    crabs: Query<Entity, With<Crab>>,
    tags: Query<(Entity, &Tag)>,
) {
    for (tag, Tag(crab)) in &tags {
        if crabs.get(*crab).is_err() {
            commands.entity(tag).despawn();
        }
    }
    for crab in &crabs {
        if tags.iter().all(|(_, Tag(c))| *c != crab) {
            commands.spawn((
                Tag(crab),
                Node {
                    position_type: PositionType::Absolute,
                    ..default()
                },
                Text::new(""),
                TextFont {
                    font_size: FontSize::Px(SIZE),
                    ..default()
                },
                TextColor(Color::srgba(0.92, 0.96, 1.0, 0.8)),
                TextShadow::default(),
                Visibility::Hidden,
            ));
        }
    }
}

/// Each tag over its crab, centred, in screen pixels: shown when there is a
/// name to show and the crab is on screen.
#[allow(clippy::type_complexity)]
fn place_tags(
    names: Res<Names>,
    me: Res<crate::coop::LocalId>,
    camera: Single<(&Camera, &Transform), With<Camera2d>>,
    crabs: Query<(&PlayerId, &Transform, Has<LocalPlayer>), With<Crab>>,
    mut tags: Query<(&Tag, &mut Node, &mut Text, &mut Visibility, &ComputedNode)>,
) {
    let (camera, at) = *camera;
    // This frame's camera (its GlobalTransform is only brought up to date
    // later in the frame).
    let at = GlobalTransform::from(*at);
    let friends = crabs.iter().filter(|(_, _, local)| !local).count();
    for (Tag(crab), mut node, mut text, mut shown, size) in &mut tags {
        let Ok((id, crab_at, local)) = crabs.get(*crab) else {
            continue;
        };
        let name = names.of(*id, me.0).filter(|_| !local || friends > 0);
        let spot = camera
            .world_to_viewport(&at, crab_at.translation + Vec3::Y * LIFT)
            .ok();
        let (Some(name), Some(spot)) = (name, spot) else {
            shown.set_if_neq(Visibility::Hidden);
            continue;
        };
        if text.0 != name {
            text.0 = name.to_string();
        }
        // The words' size is known once laid out (in physical pixels).
        let width = size.size().x * size.inverse_scale_factor();
        let (left, top) = (px(spot.x - width / 2.0), px(spot.y - SIZE));
        if node.left != left {
            node.left = left;
        }
        if node.top != top {
            node.top = top;
        }
        shown.set_if_neq(Visibility::Inherited);
    }
}

/// The canvas's `data-name` (ours) and `data-names` (friends', by `, `), for
/// the browser tests, which can't read the game's own words.
#[cfg(target_arch = "wasm32")]
fn tell_tests(names: Res<Names>, mut said: Local<String>) {
    let others: Vec<&str> = names.others().collect();
    let now = format!("{}\n{}", names.mine(), others.join(", "));
    if *said == now {
        return;
    }
    let Some(canvas) = web_sys::window()
        .and_then(|w| w.document())
        .and_then(|d| d.get_element_by_id("crabscape"))
    else {
        return;
    };
    let _ = canvas.set_attribute("data-name", names.mine());
    let _ = canvas.set_attribute("data-names", &others.join(", "));
    *said = now;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_player_is_crab_and_two_digits() {
        for (r, name) in [
            (0.0, "Crab 10"),
            (0.5, "Crab 55"),
            (0.999, "Crab 99"),
            (1.0, "Crab 99"),
        ] {
            assert_eq!(first_name(r), name);
        }
    }

    #[test]
    fn a_name_is_set_cleaned_and_marked_to_send_only_when_it_changes() {
        let mut names = Names {
            mine: "Crab 42".into(),
            ..default()
        };
        assert!(!names.set_mine("   "), "nothing left: not taken");
        assert_eq!(names.mine(), "Crab 42");
        assert!(!names.changed);
        assert!(names.set_mine("  Big   Claw "));
        assert_eq!(names.mine(), "Big Claw");
        assert!(names.changed);
        names.changed = false;
        assert!(names.set_mine("Big Claw"));
        assert!(!names.changed, "the same name isn't sent again");
    }

    #[test]
    fn each_crab_shows_its_own_players_name() {
        let me = PlayerId(0);
        let (kai, ana) = (PlayerId(7), PlayerId(9));
        let mut names = Names {
            mine: "Host".into(),
            ..default()
        };
        assert_eq!(names.of(me, me), Some("Host"));
        assert_eq!(names.of(kai, me), None, "nothing heard yet");
        names.heard(kai, "Kai");
        names.heard(ana, "Ana");
        names.heard(kai, " Kai  2 ");
        names.heard(ana, "\u{7}");
        assert_eq!(names.of(kai, me), Some("Kai 2"), "the newest, cleaned");
        assert_eq!(names.of(ana, me), Some("Ana"), "an empty name is ignored");
        assert_eq!(names.others().collect::<Vec<_>>(), ["Kai 2", "Ana"]);
    }
}
