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
const MANTIS_RIG: &str =
    include_str!("../../../art/creatures/mantis-shrimp/mantis-shrimp-parts.svg");
const MANTIS_JSON: &str = include_str!("../../../assets/creatures/mantis-shrimp-parts.json");
const OCTOPUS_RIG: &str =
    include_str!("../../../art/creatures/common-octopus/common-octopus-parts.svg");
const OCTOPUS_JSON: &str = include_str!("../../../assets/creatures/common-octopus-parts.json");

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
    // 43, plus the two rows of scissor-claw teeth on the crusher (shown in the game once fitted), plus the shell
    // seen from behind (shown once the crab turns its back), plus the gear and feats shown once fitted or grown:
    // a clam-shell guard per claw, glass grips and gripping hooks on the six walking legs' tips, a broad paddle on
    // each paddle.
    assert_eq!(e.parts, 62);
    for part in [
        "cheliped-R-propodus-guard",
        "leg-L3-dactyl-glass",
        "leg-R1-dactyl-hook",
        "leg-L4-paddle-broad",
    ] {
        assert!(e.json.contains(&format!(r#""name": "{part}""#)), "{part}");
    }
    assert!(e.json.contains(r#""name": "back-carapace""#));
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

#[test]
fn the_mantis_draws_its_committed_rig() {
    // Changed it on purpose: `cargo run -p crabrig --profile art -- parts --creature mantis-shrimp`, then
    // `-- export --creature mantis-shrimp`.
    assert!(
        crabrig::mantis::parts_svg() == MANTIS_RIG,
        "art/creatures/mantis-shrimp/mantis-shrimp-parts.svg is out of date with crates/crabrig"
    );
}

#[test]
fn the_game_mantis_parts_file_matches_its_rig() {
    let e = crabrig::export::export(
        MANTIS_RIG,
        &crabrig::export::Options {
            zoom: 1.0,
            source_name: "art/creatures/mantis-shrimp/mantis-shrimp-parts.svg".into(),
            name: "mantis-shrimp".into(),
        },
    )
    .unwrap();
    // Carapace, abdomen, tail fan, and near and far: raptorial arm and club, eye, antennule, antennal scale; three
    // far walking legs.
    assert_eq!(e.parts, 16);
    assert!(
        e.json == MANTIS_JSON,
        "assets/creatures/mantis-shrimp-parts.json is out of date: \
         cargo run -p crabrig --profile art -- export --creature mantis-shrimp"
    );
}

#[test]
fn the_octopus_draws_its_committed_rig() {
    // Changed it on purpose: `cargo run -p crabrig --profile art -- parts --creature common-octopus`, then
    // `-- export --creature common-octopus`.
    assert!(
        crabrig::octopus::parts_svg() == OCTOPUS_RIG,
        "art/creatures/common-octopus/common-octopus-parts.svg is out of date with crates/crabrig"
    );
}

#[test]
fn the_game_octopus_parts_file_matches_its_rig() {
    let e = crabrig::export::export(
        OCTOPUS_RIG,
        &crabrig::export::Options {
            zoom: 1.0,
            source_name: "art/creatures/common-octopus/common-octopus-parts.svg".into(),
            name: "common-octopus".into(),
        },
    )
    .unwrap();
    // Head, mantle, eye, their two pale copies, the web, the camouflage skin and its eye, and eight arm segments.
    assert_eq!(e.parts, 16);
    assert!(
        e.json == OCTOPUS_JSON,
        "assets/creatures/common-octopus-parts.json is out of date: \
         cargo run -p crabrig --profile art -- export --creature common-octopus"
    );
}

#[test]
fn the_octopus_kit_has_every_piece_the_game_poses() {
    use crabrig::octopus::{SEGMENTS, build, parent_of};
    let items = build();
    let piece = |id: &str| {
        items
            .iter()
            .find(|i| i.id == id)
            .unwrap_or_else(|| panic!("no {id}"))
    };
    let mut wanted: Vec<(String, &str)> = [
        ("head", "root"),
        ("mantle", "head"),
        ("eye", "head"),
        ("head-pale", "head"),
        ("mantle-pale", "mantle"),
        ("web", "root"),
        ("camo", "root"),
        ("camo-eye", "camo"),
    ]
    .into_iter()
    .map(|(id, parent)| (id.to_string(), parent))
    .collect();
    for k in 0..SEGMENTS {
        wanted.push((format!("arm-{k}"), "root"));
    }
    for (id, parent) in &wanted {
        assert!(piece(id).pv.is_some(), "{id} has a joint");
        assert_eq!(parent_of(id), *parent, "{id} hangs from {parent}");
    }
    // Arm segments start at their joint and run forward, the suckers on their underside (y down).
    let seg = piece("arm-0");
    assert_eq!(seg.pv, Some([0.0, 0.0]));
    let sucker = piece("arm-0-sucker0");
    assert!(sucker.pts.iter().all(|p| p[1] > 0.0), "suckers underneath");
    assert_eq!(parent_of("arm-0-sucker0"), "arm-0");
    assert_eq!(parent_of("arm-7-sucker4"), "arm-7");
}
