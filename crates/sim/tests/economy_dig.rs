//! Digging earns Excavating experience by the sediment moved, once per new
//! block, so holding dig in place or digging a refilled spot again pays
//! little. The pace against gathering is in `economy_pacing.rs`.

#[path = "common/dig.rs"]
mod dig;

use crabsim::Controls;
use dig::*;

#[test]
fn tunnelling_pays_by_the_block_and_replays_the_same() {
    let (xp, secs) = tunnel(&mut dig_sim(), 60.0);
    println!("tunnel: {xp} xp in {secs:.1} s");
    assert!(secs < 60.0, "the gallery's end was reached");
    // About one experience per 16 units of tunnel: the shaft and the gallery
    // are some 950 units.
    assert!((40..=90).contains(&xp), "{xp} xp");
    assert_eq!(tunnel(&mut dig_sim(), 60.0), (xp, secs), "deterministic");
}

#[test]
fn digging_the_same_ground_again_or_in_place_pays_little() {
    let mut sim = dig_sim();
    let (first, _) = tunnel(&mut sim, 60.0);
    // Back west through the same tunnel, digging.
    let back = Controls {
        dig_held: true,
        walk: -1.0,
        ..Controls::default()
    };
    let before = excavating(&sim);
    hold(&mut sim, back, 120 * 20);
    let again = excavating(&sim) - before;
    // Then holding dig down in place for a minute.
    let before = excavating(&sim);
    let down = Controls {
        dig_held: true,
        down: true,
        ..Controls::default()
    };
    hold(&mut sim, down, 120 * 60);
    let in_place = excavating(&sim) - before;
    println!("first pass {first}, back {again}, in place {in_place}");
    assert!(again * 5 <= first, "{again} vs {first}");
    assert!(in_place <= 5, "{in_place} xp for a minute in place");
}

#[test]
fn walking_the_surface_or_swimming_without_dig_earns_nothing() {
    let mut sim = dig_sim();
    hold(
        &mut sim,
        Controls {
            walk: 1.0,
            ..Controls::default()
        },
        120 * 10,
    );
    hold(
        &mut sim,
        Controls {
            down: true,
            ..Controls::default()
        },
        120 * 10,
    );
    assert_eq!(excavating(&sim), 0);
}
