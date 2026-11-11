//! The ten bestiary creatures' rigs (ink placeholders at true size), by art name. See `crates/sim/src/creatures.rs`.

pub mod amphipods;
pub mod bobbit;
pub mod grouper;
pub mod hagfish;
pub mod hermit;
pub mod isopod;
pub mod lobster;
pub mod squid;
pub mod turtle;
pub mod viperfish;

/// The art names, in the bestiary's fixed order.
pub const NAMES: [&str; 10] = [
    "loggerhead-turtle",
    "humboldt-squid",
    "viperfish",
    "goliath-grouper",
    "american-lobster",
    "hermit-crab",
    "bobbit-worm",
    "giant-isopod",
    "hagfish",
    "amphipod-swarm",
];

/// The creature's parts file (`art/creatures/<name>/<name>-parts.svg`), if
/// `name` is one of the ten.
pub fn parts_svg(name: &str) -> Option<String> {
    match name {
        "loggerhead-turtle" => Some(turtle::parts_svg()),
        "humboldt-squid" => Some(squid::parts_svg()),
        "viperfish" => Some(viperfish::parts_svg()),
        "goliath-grouper" => Some(grouper::parts_svg()),
        "american-lobster" => Some(lobster::parts_svg()),
        "hermit-crab" => Some(hermit::parts_svg()),
        "bobbit-worm" => Some(bobbit::parts_svg()),
        "giant-isopod" => Some(isopod::parts_svg()),
        "hagfish" => Some(hagfish::parts_svg()),
        "amphipod-swarm" => Some(amphipods::parts_svg()),
        _ => None,
    }
}
