//! Static world labels are complete and never affect play or persistence.
use bevy_math::{Rect, Vec2};
use crabsim::level::Growth;
use crabsim::level::landmarks::{Kind, Needs};
use crabsim::Level;
use std::collections::HashSet;

#[test]
fn every_landmark_has_a_unique_name_and_positive_area() {
    let level = Level::test_level();
    assert!(!level.landmarks().is_empty());
    let mut names = HashSet::new();
    for landmark in &level.landmarks() {
        assert!(!landmark.name.trim().is_empty());
        assert!(names.insert(&landmark.name), "duplicate {}", landmark.name);
        assert!(landmark.rect.min.is_finite() && landmark.rect.max.is_finite());
        assert!(landmark.rect.width() > 0.0 && landmark.rect.height() > 0.0);
    }
}

#[test]
fn every_gate_landmark_covers_an_existing_net_brittle_or_bare_rock() {
    let level = Level::test_level();
    let expected: Vec<_> = level
        .nets
        .iter()
        .map(|n| (n.rect, Needs::Scissors))
        .chain(level.brittle.iter().map(|b| (b.rect, Needs::Crusher)))
        .chain(
            level
                .climbs
                .iter()
                .filter(|c| c.kind == Growth::Rock)
                .map(|c| {
                    (
                        Rect::new(c.x - 60.0, c.bottom, c.x + 60.0, c.top),
                        Needs::GlassGrips,
                    )
                }),
        )
        .collect();
    let actual: Vec<_> = level
        .landmarks()
        .iter()
        .filter_map(|l| match l.kind {
            Kind::Gate { needs } => Some((l.rect, needs)),
            _ => None,
        })
        .collect();
    assert_eq!(actual.len(), expected.len());
    for gate in &expected {
        assert_eq!(actual.iter().filter(|g| *g == gate).count(), 1, "{gate:?}");
    }
    println!("gates tagged {}/{}", actual.len(), expected.len());
}

#[test]
fn home_and_every_crevice_are_refuges() {
    let level = Level::test_level();
    let expected: Vec<_> = std::iter::once(level.home)
        .chain(level.crevices.iter().map(|c| c.door))
        .chain(
            level
                .climbs
                .iter()
                .filter(|c| c.kind != Growth::Rock)
                .map(|c| Rect::new(c.x - 60.0, c.bottom, c.x + 60.0, c.top)),
        )
        .collect();
    for rect in &expected {
        assert_eq!(
            level
                .landmarks()
                .iter()
                .filter(|l| l.kind == Kind::Refuge && l.rect == *rect)
                .count(),
            1
        );
    }
    println!("refuges tagged {}/{}", expected.len(), expected.len());
}

#[test]
fn landmarks_are_derived_the_same_every_time() {
    let level = Level::test_level();
    assert_eq!(level.landmarks(), Level::test_level().landmarks());
    assert_eq!(level.landmarks(), level.clone().landmarks());
}

#[test]
fn authored_chambers_and_chimney_are_tagged() {
    let level = Level::test_level();
    for (name, rect, prize) in [
        ("octopus den", level.den, Vec2::new(8650.0, -1896.0)),
        ("far-wall cave", level.secrets[1], level.eggs[4]),
    ] {
        let all = level.landmarks();
        let landmark = all.iter().find(|l| l.name == name).unwrap();
        assert_eq!(landmark.rect, rect);
        let Kind::Chamber {
            door,
            prize: actual,
        } = landmark.kind
        else {
            panic!("{name} must be a chamber")
        };
        assert_eq!(actual, prize);
        assert!(rect.contains(prize));
        assert_eq!(door.min.x, rect.min.x);
        assert_eq!(door.min.y, rect.min.y);
        assert_eq!(door.max.y, rect.max.y);
        assert!(door.width() > 0.0 && door.width() <= 60.0);
        println!(
            "{name} door-to-prize {:.1} units",
            door.center().distance(prize)
        );
    }
    assert!(
        level
            .landmarks()
            .iter()
            .any(|l| l.kind == Kind::Tunnel && l.rect.contains(Vec2::new(5440.0, -1000.0)))
    );
    for point in [
        Vec2::new(7000.0, -2000.0),
        Vec2::new(7000.0, -9000.0),
        Vec2::new(3800.0, -8000.0),
        Vec2::new(3000.0, -600.0),
        Vec2::new(4800.0, -600.0),
    ] {
        assert!(
            level
                .landmarks()
                .iter()
                .any(|l| l.kind == Kind::OpenWater && l.rect.contains(point)),
            "{point:?}"
        );
    }
}
