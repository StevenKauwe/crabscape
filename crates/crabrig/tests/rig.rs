//! The puppets still draw the committed rigs, the game's parts files match them, and every view has its limbs.

use crabrig::crab::{View, base_pose, build_front};
use std::collections::BTreeSet;

const RIG: &str = include_str!("../../../art/crab/crab-parts.svg");
const GAME_JSON: &str = include_str!("../../../assets/crab/crab-parts.json");
const SHRIMP_RIG: &str =
    include_str!("../../../art/creatures/snapping-shrimp/snapping-shrimp-parts.svg");
const SHRIMP_JSON: &str = include_str!("../../../assets/creatures/snapping-shrimp-parts.json");
const MORAY_RIG: &str =
    include_str!("../../../art/creatures/snowflake-moray/snowflake-moray-parts.svg");
const MORAY_JSON: &str = include_str!("../../../assets/creatures/snowflake-moray-parts.json");

#[test]
fn the_puppet_draws_the_committed_rig() {
    // If you changed the crab on purpose: `cargo run -p crabrig --profile art -- parts`, then `-- export`.
    assert!(
        crabrig::parts::parts_svg(&base_pose()) == RIG,
        "art/crab/crab-parts.svg is out of date with crates/crabrig"
    );
}

#[test]
fn the_game_parts_file_matches_the_rig() {
    let e = crabrig::export::export(
        RIG,
        &crabrig::export::Options {
            zoom: 1.0,
            source_name: "art/crab/crab-parts.svg".into(),
            name: "crab".into(),
        },
    )
    .unwrap();
    // 43, plus the two rows of scissor-claw teeth on the crusher (shown in the game once fitted).
    assert_eq!(e.parts, 45);
    assert!(
        e.json == GAME_JSON,
        "assets/crab/crab-parts.json is out of date: cargo run -p crabrig --profile art -- export"
    );
}

/// Walking legs + paddles and chelipeds seen in a view (the model sheet's count check).
fn counts(view: View) -> (usize, usize) {
    let mut pose = base_pose();
    pose.view = view;
    let (mut legs, mut claws) = (BTreeSet::new(), BTreeSet::new());
    for it in build_front(&pose) {
        if let Some(l) = it
            .id
            .strip_prefix("leg-")
            .filter(|r| r.len() > 2 && r.as_bytes()[2] == b'-')
        {
            legs.insert(l[..2].to_string());
        }
        if let Some(c) = it
            .id
            .strip_prefix("cheliped-")
            .filter(|r| r.len() > 1 && r.as_bytes()[1] == b'-')
        {
            claws.insert(c[..1].to_string());
        }
    }
    (legs.len(), claws.len())
}

#[test]
fn ten_legs_in_every_view() {
    // 2 claws + 6 walking legs + 2 swimming paddles
    assert_eq!(counts(View::Front), (8, 2));
    assert_eq!(counts(View::Back), (8, 2));
}

#[test]
fn the_shrimp_draws_its_committed_rig() {
    // Changed it on purpose: `cargo run -p crabrig --profile art -- parts --creature snapping-shrimp`, then
    // `-- export --creature snapping-shrimp`.
    assert!(
        crabrig::shrimp::parts_svg() == SHRIMP_RIG,
        "art/creatures/snapping-shrimp/snapping-shrimp-parts.svg is out of date with crates/crabrig"
    );
}

#[test]
fn the_game_shrimp_parts_file_matches_its_rig() {
    let e = crabrig::export::export(
        SHRIMP_RIG,
        &crabrig::export::Options {
            zoom: 1.0,
            source_name: "art/creatures/snapping-shrimp/snapping-shrimp-parts.svg".into(),
            name: "snapping-shrimp".into(),
        },
    )
    .unwrap();
    // Carapace, abdomen, snapper claw and its finger, two feelers, two far legs and the small claw.
    assert_eq!(e.parts, 9);
    assert!(
        e.json == SHRIMP_JSON,
        "assets/creatures/snapping-shrimp-parts.json is out of date: \
         cargo run -p crabrig --profile art -- export --creature snapping-shrimp"
    );
}

#[test]
fn the_moray_draws_its_committed_rig() {
    // Changed it on purpose: `cargo run -p crabrig --profile art -- parts --creature snowflake-moray`, then
    // `-- export --creature snowflake-moray`.
    assert!(
        crabrig::moray::parts_svg() == MORAY_RIG,
        "art/creatures/snowflake-moray/snowflake-moray-parts.svg is out of date with crates/crabrig"
    );
}

#[test]
fn the_game_moray_parts_file_matches_its_rig() {
    let e = crabrig::export::export(
        MORAY_RIG,
        &crabrig::export::Options {
            zoom: 1.0,
            source_name: "art/creatures/snowflake-moray/snowflake-moray-parts.svg".into(),
            name: "snowflake-moray".into(),
        },
    )
    .unwrap();
    // Head, lower jaw, the inside of the mouth, the throat jaws and three body segments.
    assert_eq!(e.parts, 7);
    assert!(
        e.json == MORAY_JSON,
        "assets/creatures/snowflake-moray-parts.json is out of date: \
         cargo run -p crabrig --profile art -- export --creature snowflake-moray"
    );
}
