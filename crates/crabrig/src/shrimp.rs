//! The big-clawed snapping shrimp (*Alpheus heterochaelis*) as a cut-out puppet: side view, facing right, from
//! its concept sheet (`art/creatures/snapping-shrimp/`) and rig notes (`docs/design/creatures/snapping-shrimp.md`).
//!
//! Parts drawn behind the carapace come first and have their own joints (a part's depth is its first piece's).
//!
//! Shrimp space: x forward (to the head), y down, the floor at y = 0; about 130 long from the tail fan to the
//! snapper's finger tip. The parts file draws it at twice that, like the crab.

use crate::geom::*;
use crate::item::{Item, Kind};

/// The joint the snapper's finger (the dactyl) swings about, at the top front of the palm.
pub const DACTYL_PIVOT: P = [60.0, -21.0];

/// Ink a little over half the crab's weight: the shrimp's parts are small, and full-weight ink swamps them.
fn shape(id: &str, pts: Vec<P>, mat: &str) -> Item {
    Item::new(Kind::Shape, id, Some(id), pts, mat).sw(0.6)
}

fn line(id: &str, pts: Vec<P>, sw: f64) -> Item {
    Item::new(Kind::Line, id, Some(id), pts, "ink").sw(sw)
}

/// A closed outline through `pts` (a smooth curve, as straight segments).
fn smooth(pts: &[P]) -> Vec<P> {
    through(pts, 4, true)
}

/// The abdomen's top and bottom edges at `x` (it humps over the big 2nd plate).
fn abdomen_top(x: f64) -> f64 {
    -20.0 - 8.0 * (-((x + 8.0) / 13.0).powi(2)).exp() - 1.5 * ((x + 38.0) / 39.0)
}
fn abdomen_bottom(x: f64) -> f64 {
    -5.0 - 2.5 * ((x + 38.0) / 39.0)
}

/// Abdominal plate `n` (1 = next to the carapace), from `x0` back to `x1`, its side lobe (the pleuron) hanging
/// a little below the gut line.
fn plate(n: usize, x0: f64, x1: f64) -> Item {
    let steps = 6;
    let mut pts = vec![];
    for i in 0..=steps {
        let x = lerp(x0, x1, i as f64 / steps as f64);
        pts.push([x, abdomen_top(x) - 0.4]);
    }
    let lobe = if n == 2 { 2.5 } else { 1.2 };
    for i in 0..=steps {
        let t = i as f64 / steps as f64;
        let x = lerp(x1, x0, t);
        let sag = lobe * (t * std::f64::consts::PI).sin();
        pts.push([x, abdomen_bottom(x) + sag]);
    }
    let mat = "shrimp";
    shape(&format!("abdomen-plate{n}"), pts, mat)
}

/// A slender jointed leg from `hip` through `knee` to `foot`.
fn leg(id: &str, hip: P, knee: P, foot: P, w: f64) -> Item {
    shape(
        id,
        limb(
            &[hip, knee, foot],
            |t| w * (1.0 - 0.35 * t),
            LimbOpt::default(),
        ),
        "shrimpLeg",
    )
    .sw(0.35)
}

/// Every piece, back to front.
pub fn build() -> Vec<Item> {
    let mut items = vec![];

    // Far side: two walking legs and the small claw, in shadow.
    for (i, x) in [10.0, 20.0].into_iter().enumerate() {
        items.push(
            leg(
                &format!("leg-far{}", i + 1),
                [x, -9.0],
                [x - 2.5, -4.5],
                [x - 4.0, 0.0],
                1.8,
            )
            .far()
            .pv([x, -9.0]),
        );
    }
    items.push(
        shape(
            "minor-claw",
            limb(
                &[[26.0, -11.0], [36.0, -8.0], [44.0, -7.5]],
                |t| 3.4 - 1.2 * t,
                LimbOpt::default(),
            ),
            "shrimpClaw",
        )
        .far()
        .sw(0.4)
        .pv([26.0, -11.0]),
    );

    // The feelers: a long antenna whip and the shorter antennule, swept forward and up from under the hood.
    items.push(
        line(
            "antennule",
            vec![[32.0, -24.0], [40.0, -30.0], [48.0, -33.0], [57.0, -34.0]],
            0.3,
        )
        .pv([32.0, -24.0]),
    );
    items.push(
        line(
            "antenna",
            vec![
                [31.0, -25.0],
                [38.0, -34.0],
                [48.0, -40.0],
                [60.0, -43.0],
                [72.0, -43.0],
            ],
            0.28,
        )
        .pv([31.0, -25.0]),
    );

    // The abdomen and tail fan, hinged under the carapace's back edge (it flicks the tail to shoot backwards).
    let plates = [
        (1.0, -6.0),
        (-4.0, -15.0),
        (-13.0, -22.0),
        (-20.0, -28.0),
        (-26.0, -33.0),
        (-31.0, -38.5),
    ];
    for (n, (x0, x1)) in plates.iter().enumerate().skip(2).rev() {
        items.push(plate(n + 1, *x0, *x1));
    }
    items.push(plate(1, plates[0].0, plates[0].1));
    // The 2nd plate overlaps both neighbours (the caridean hallmark).
    let mut second = plate(2, plates[1].0 + 1.5, plates[1].1 - 1.5);
    second.pv = Some([2.0, -15.0]);
    second.id = "abdomen".into();
    second.g = Some("abdomen".into());
    items.push(second);
    for (k, (x, y, rx)) in [
        (-10.0, -21.0, 2.2),
        (-17.5, -18.0, 1.6),
        (-27.0, -15.0, 1.4),
    ]
    .iter()
    .enumerate()
    {
        items.push(
            shape(
                &format!("abdomen-mottle{k}"),
                ell(*x, *y, *rx, rx * 0.7, 10, 0.0),
                "shrimpMottle",
            )
            .no_ink(),
        );
    }
    items.push(shape(
        "abdomen-uropod",
        smooth(&[
            [-37.0, -16.0],
            [-45.0, -21.0],
            [-52.0, -19.0],
            [-50.0, -12.0],
            [-42.0, -9.0],
        ]),
        "shrimp",
    ));
    items.push(shape(
        "abdomen-telson",
        smooth(&[
            [-37.0, -13.0],
            [-47.0, -12.0],
            [-53.0, -8.0],
            [-48.0, -4.0],
            [-38.0, -6.0],
        ]),
        "shrimp",
    ));

    // The carapace, its hood over the eyes (an eye shows through as a dark spot), and the near legs.
    items.push(shape(
        "carapace",
        smooth(&[
            [0.0, -14.0],
            [1.0, -24.0],
            [10.0, -28.5],
            [22.0, -28.5],
            [31.0, -27.0],
            [36.0, -23.0],
            [35.5, -18.5],
            [31.0, -14.0],
            [24.0, -10.5],
            [12.0, -9.0],
            [3.0, -9.5],
        ]),
        "shrimp",
    ));
    items.push(
        shape(
            "carapace-mottle",
            ell(14.0, -23.0, 3.0, 2.0, 10, 10.0),
            "shrimpMottle",
        )
        .no_ink(),
    );
    items.push(
        shape(
            "carapace-eye",
            ell(30.0, -21.5, 2.3, 2.1, 10, 0.0),
            "cornea",
        )
        .sw(0.3),
    );
    items.push(line(
        "carapace-hood",
        vec![[26.0, -26.0], [31.0, -25.0], [34.5, -21.5]],
        0.3,
    ));
    // The slender 2nd leg, reaching forward, then three walking legs.
    items.push(leg(
        "leg-slender",
        [24.0, -10.0],
        [31.0, -5.0],
        [35.0, 0.0],
        1.2,
    ));
    for (i, x) in [6.0, 14.0, 22.0].into_iter().enumerate() {
        items.push(leg(
            &format!("leg-near{}", i + 1),
            [x, -9.5],
            [x - 2.0, -4.5],
            [x - 3.5, 0.0],
            2.0,
        ));
    }

    // The snapper claw, held low and forward: its arm, the swollen palm and the fixed finger (red-tipped).
    let mut claw = shape(
        "claw",
        smooth(&[
            [31.0, -12.0],
            [38.0, -18.5],
            [50.0, -22.5],
            [62.0, -22.0],
            [70.0, -18.5],
            [77.0, -14.0],
            [81.5, -11.5],
            [77.0, -9.0],
            [66.0, -5.5],
            [50.0, -4.0],
            [38.0, -6.0],
        ]),
        "shrimpClaw",
    );
    claw.pv = Some([30.0, -11.0]);
    items.push(claw);
    items.push(
        shape(
            "claw-tip",
            smooth(&[[76.0, -13.5], [81.5, -11.5], [77.0, -9.2], [75.0, -11.0]]),
            "shrimpTip",
        )
        .sw(0.3),
    );
    // The finger that cocks open (the tell) and snaps shut.
    let mut dactyl = shape(
        "dactyl",
        limb(
            &[DACTYL_PIVOT, [71.0, -21.0], [82.0, -15.5]],
            |t| 8.0 - 4.0 * t,
            LimbOpt::default(),
        ),
        "shrimpClaw",
    );
    dactyl.pv = Some(DACTYL_PIVOT);
    items.push(dactyl);
    items.push(
        shape(
            "dactyl-tip",
            smooth(&[[77.5, -19.0], [82.5, -15.5], [79.0, -13.6], [76.0, -16.5]]),
            "shrimpTip",
        )
        .sw(0.3),
    );
    items
}

/// The part a piece hangs from: the carapace is the root; plates, mottles and the fan ride on the abdomen; tips on
/// their fingers; everything else on the carapace.
pub fn parent_of(id: &str) -> String {
    match id {
        "carapace" => "root".into(),
        "dactyl" => "claw".into(),
        "dactyl-tip" => "dactyl".into(),
        "claw-tip" => "claw".into(),
        _ if id.starts_with("abdomen-") => "abdomen".into(),
        _ => "carapace".into(),
    }
}

/// `snapping-shrimp-parts.svg`.
pub fn parts_svg() -> String {
    crate::parts::write(
        build(),
        "<!-- CrabScape snapping shrimp: rig-ready cut-out parts, side view facing right, at rest.
  One <g> per part. data-pivot = the joint this part rotates about, in this file's pixels. data-parent = the part
  it hangs from. Draw order = document order. Written by crabrig (src/shrimp.rs). -->",
        "CrabScape snapping shrimp · rig parts (side view)",
        parent_of,
    )
}
