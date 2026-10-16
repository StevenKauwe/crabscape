//! The puppet still draws the committed rig, the game's parts file matches it, and every view has its limbs.

use crabrig::crab::{View, base_pose, build_front};
use std::collections::BTreeSet;

const RIG: &str = include_str!("../../../art/crab/crab-parts.svg");
const GAME_JSON: &str = include_str!("../../../assets/crab/crab-parts.json");

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
        },
    )
    .unwrap();
    assert_eq!(e.parts, 43);
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
