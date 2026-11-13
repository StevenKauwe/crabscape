//! Shore crab (*Carcinus maenas*), a juvenile: a cut-out rig, side view, facing right, standing on y = 0. The
//! carapace is about 4 cm (26 world units) across (`creatures::shore_crab::BODY`; the size is a game choice, not
//! verified). The rig is drawn four times the world's size so its small details survive the paint (the game scales
//! it back down: `ART_SCALE` in `src/creatures/shore_crab.rs`); the shapes below are written in world units and
//! multiplied by [`S`].
//!
//! Parts: `carapace` (root, with the five teeth on each side of the eyes, mottling and the pale underside as
//! painted-in details), two eyestalks with wet eyes, two claws (`claw-near`, `claw-far`, each with a movable
//! finger) that rise and spread in the pinch's tell, and six walking legs.
//!
//! Colours: mottled green to olive-brown above, a paler yellow-green to orange below, claws a little lighter. The
//! species' usual look, pushed a little for the dark sea: general knowledge, not verified against an opened source.

use crate::bestiary::{detail, glints, owner};
use crate::geom::{P, ell, through};
use crate::item::{Item, Kind};
use crate::palette::{self, Mat};

/// The rig is drawn this many times the world's size.
pub const S: f64 = 4.0;

/// Width and height of the carapace, in world units (1 cm = 6.25).
pub const BODY: (f64, f64) = (27.0, 12.5);

fn tri(a: &str, b: &str, c: &str) -> Mat {
    Mat::Tri([a.into(), b.into(), c.into()])
}

fn big(pts: &[P]) -> Vec<P> {
    pts.iter().map(|p| [p[0] * S, p[1] * S]).collect()
}

/// A smooth closed outline through `pts` (world units).
fn smooth(pts: &[P]) -> Vec<P> {
    big(&through(pts, 4, true))
}

fn shape(id: &str, pts: &[P], mat: &str) -> Item {
    Item::new(Kind::Shape, id, Some(id), smooth(pts), mat).sw(1.1)
}

fn pv(p: P) -> P {
    [p[0] * S, p[1] * S]
}

fn blob(cx: f64, cy: f64, rx: f64, ry: f64, rot: f64) -> Vec<P> {
    big(&ell(cx, cy, rx, ry, 12, rot))
}

/// A tapering limb strip along `spine` (world units), `w0` wide at the start and `w1` at the end.
fn strip(spine: &[P], w0: f64, w1: f64) -> Vec<P> {
    let n = spine.len();
    let (mut left, mut right) = (vec![], vec![]);
    for (i, p) in spine.iter().enumerate() {
        let a = spine[i.saturating_sub(1)];
        let b = spine[(i + 1).min(n - 1)];
        let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
        let l = (dx * dx + dy * dy).sqrt().max(1e-6);
        let (nx, ny) = (-dy / l, dx / l);
        let w = (w0 + (w1 - w0) * i as f64 / (n - 1).max(1) as f64) / 2.0;
        left.push([p[0] + nx * w, p[1] + ny * w]);
        right.push([p[0] - nx * w, p[1] - ny * w]);
    }
    right.reverse();
    left.extend(right);
    left
}

/// The five teeth of one side along the carapace's front edge: small triangles standing out of the outline.
fn teeth() -> Vec<Item> {
    let centre: P = [0.0, -9.0];
    let edge: [P; 5] = [
        [2.0, -15.4],
        [5.4, -14.7],
        [8.5, -13.1],
        [11.0, -11.0],
        [12.8, -8.6],
    ];
    edge.iter()
        .enumerate()
        .map(|(i, a)| {
            let (dx, dy) = (a[0] - centre[0], a[1] - centre[1]);
            let l = (dx * dx + dy * dy).sqrt();
            let n = [dx / l, dy / l];
            let t = [-n[1], n[0]];
            let h = 1.9;
            let hw = 1.25;
            // The tips lean forward (to the right), like the real teeth.
            let tip = [a[0] + n[0] * h + t[0] * 0.5, a[1] + n[1] * h + t[1] * 0.5];
            detail(
                "carapace",
                &format!("tooth-{i}"),
                big(&[
                    [a[0] - t[0] * hw - n[0] * 0.8, a[1] - t[1] * hw - n[1] * 0.8],
                    tip,
                    [a[0] + t[0] * hw - n[0] * 0.8, a[1] + t[1] * hw - n[1] * 0.8],
                ]),
                "crabTooth",
                0.6,
            )
        })
        .collect()
}

fn leg(id: &str, x0: f64, far: bool) -> Item {
    // Near legs splay forward, far legs back, so the two sets read apart.
    let d = if far { -1.0 } else { 1.0 };
    let spine = [
        [x0, -5.2],
        [x0 + 2.2 * d, -3.6],
        [x0 + 4.4 * d, -1.8],
        [x0 + 5.4 * d, 0.0],
    ];
    Item::new(
        Kind::Shape,
        id,
        Some(id),
        big(&strip(&spine, 3.2, 1.2)),
        if far { "crabLegFar" } else { "crabLeg" },
    )
    .sw(1.0)
    .pv(pv([x0, -5.2]))
}

/// One claw: palm (with its arm, a pale underside and spots drawn in) and a hinged upper finger. `dx`, `dy` move
/// it (the far claw sits up and back of the near one).
fn claw(name: &str, dx: f64, dy: f64, far: bool, v: &mut Vec<Item>) {
    let mat = if far { "crabClawFar" } else { "crabClaw" };
    let o = |pts: &[P]| -> Vec<P> { pts.iter().map(|p| [p[0] + dx, p[1] + dy]).collect() };
    v.push(
        shape(
            name,
            &o(&[
                [10.0, -7.5],
                [13.5, -10.5],
                [17.5, -13.2],
                [23.0, -13.8],
                [26.5, -11.5],
                [26.8, -8.2],
                [24.0, -6.2],
                [19.5, -6.0],
                [15.0, -5.8],
                [10.5, -5.0],
            ]),
            mat,
        )
        .pv(pv([10.8 + dx, -6.4 + dy])),
    );
    // The pale underside of the palm and the fixed finger below the hinge.
    v.push(detail(
        name,
        "belly",
        big(&through(
            &o(&[
                [11.0, -6.3],
                [16.0, -6.6],
                [22.0, -6.6],
                [26.4, -8.0],
                [24.0, -6.2],
                [19.5, -6.0],
                [15.0, -5.8],
                [10.5, -5.0],
            ]),
            3,
            true,
        )),
        if far { "crabBellyFar" } else { "crabBelly" },
        0.0,
    ));
    v.push(detail(
        name,
        "pollex",
        big(&through(
            &o(&[
                [23.5, -8.4],
                [28.0, -8.6],
                [31.8, -7.2],
                [28.5, -5.4],
                [24.0, -5.6],
            ]),
            3,
            true,
        )),
        mat,
        0.6,
    ));
    v.push(detail(
        name,
        "pollex-tip",
        big(&o(&[[29.8, -7.9], [31.8, -7.2], [29.6, -5.8]])),
        "tip",
        0.0,
    ));
    for (i, (x, y)) in [(16.0, -9.8), (20.5, -11.6), (13.5, -8.2)]
        .into_iter()
        .enumerate()
    {
        v.push(detail(
            name,
            &format!("spot-{i}"),
            blob(x + dx, y + dy, 1.4, 1.0, 20.0 * i as f64),
            "crabMottle",
            0.0,
        ));
    }
    v.push(
        shape(
            &format!("{name}-finger"),
            &o(&[
                [22.5, -11.2],
                [27.0, -13.6],
                [31.8, -9.8],
                [32.0, -8.4],
                [28.0, -9.4],
                [23.5, -8.8],
            ]),
            mat,
        )
        .pv(pv([23.5 + dx, -10.0 + dy])),
    );
    v.push(detail(
        &format!("{name}-finger"),
        "tip",
        big(&o(&[
            [29.8, -11.6],
            [31.8, -9.8],
            [32.0, -8.4],
            [29.6, -9.2],
        ])),
        "tip",
        0.0,
    ));
}

pub fn build() -> Vec<Item> {
    let mut v = vec![];
    // Far legs, then the far claw (behind the body).
    for (id, x0) in [("leg-d", -7.0), ("leg-e", -2.0), ("leg-f", 4.0)] {
        v.push(leg(id, x0, true));
    }
    claw("claw-far", -2.0, -2.2, true, &mut v);
    // Eyestalks, behind the carapace's front edge: the stalk and a wet eye.
    for (id, base, top, r) in [
        ("eye-a", [7.6, -13.4], [8.0, -17.6], 1.7),
        ("eye-b", [11.0, -11.4], [12.8, -15.4], 1.7),
    ] {
        v.push(
            Item::new(
                Kind::Shape,
                id,
                Some(id),
                big(&strip(
                    &[
                        base,
                        [(base[0] + top[0]) / 2.0, (base[1] + top[1]) / 2.0],
                        top,
                    ],
                    2.2,
                    1.9,
                )),
                "crabLeg",
            )
            .sw(0.9)
            .pv(pv(base)),
        );
        v.push(detail(
            id,
            "cornea",
            blob(top[0] + 0.3, top[1] - 0.3, r, r * 0.95, 0.0),
            "cornea",
            0.6,
        ));
        v.extend(glints(
            id,
            [
                pv([top[0] + 0.3, top[1] - 0.3])[0],
                pv([top[0] + 0.3, top[1] - 0.3])[1],
            ],
            r * S,
        ));
    }
    // The carapace: a low dome, wider than tall, with the pale underside drawn in at its lower rim.
    v.push(shape(
        "carapace",
        &[
            [-13.0, -7.0],
            [-12.4, -11.0],
            [-9.0, -14.4],
            [-3.0, -15.9],
            [4.0, -15.4],
            [9.0, -13.0],
            [12.6, -9.6],
            [13.6, -6.4],
            [10.0, -4.4],
            [0.0, -3.6],
            [-9.0, -4.2],
        ],
        "crabShell",
    ));
    v.push(detail(
        "carapace",
        "belly",
        big(&through(
            &[
                [-12.6, -6.4],
                [-6.0, -5.4],
                [2.0, -5.0],
                [11.0, -5.6],
                [13.2, -6.5],
                [10.0, -4.4],
                [0.0, -3.6],
                [-9.0, -4.2],
            ],
            3,
            true,
        )),
        "crabBelly",
        0.0,
    ));
    // Mottling: ragged olive-brown blotches and yellow-green patches, not neat dots.
    for (i, (x, y, rx, ry, r)) in [
        (-8.0, -10.5, 3.6, 2.0, 25.0),
        (-1.5, -12.4, 3.2, 1.8, -15.0),
        (4.5, -9.6, 3.0, 2.1, 40.0),
        (-5.5, -7.2, 2.4, 1.2, 5.0),
        (8.5, -7.6, 1.8, 1.2, -30.0),
        (0.5, -7.6, 2.2, 1.0, 12.0),
    ]
    .into_iter()
    .enumerate()
    {
        v.push(detail(
            "carapace",
            &format!("mottle-{i}"),
            blob(x, y, rx, ry, r),
            "crabMottle",
            0.0,
        ));
    }
    for (i, (x, y, rx, ry, r)) in [
        (-3.5, -14.0, 2.6, 1.1, -10.0),
        (2.8, -12.0, 2.2, 1.3, 30.0),
        (-9.5, -8.8, 1.6, 1.0, 0.0),
        (7.0, -11.2, 1.4, 0.9, 35.0),
    ]
    .into_iter()
    .enumerate()
    {
        v.push(detail(
            "carapace",
            &format!("patch-{i}"),
            blob(x, y, rx, ry, r),
            "crabPatch",
            0.0,
        ));
    }
    v.extend(teeth());
    // Near legs, in front of the lower rim, then the near claw.
    for (id, x0) in [("leg-a", -8.0), ("leg-b", -3.0), ("leg-c", 3.0)] {
        v.push(leg(id, x0, false));
    }
    claw("claw-near", 0.0, 0.0, false, &mut v);
    v
}

pub fn parent_of(id: &str) -> String {
    if let Some(part) = owner(id) {
        return part;
    }
    match id {
        "carapace" => "root".into(),
        "claw-near-finger" => "claw-near".into(),
        "claw-far-finger" => "claw-far".into(),
        _ => "carapace".into(),
    }
}

pub fn parts_svg() -> String {
    let mut pal = palette::normal();
    let far = |m: [&str; 3]| Mat::Tri(m.map(|c| palette::mix_hex(c, "#2C4950", 0.25)));
    let claw = ["#CBD27A", "#8FA24A", "#46561F"];
    let belly = ["#FFEBB0", "#F4BC68", "#B9733C"];
    let leg = ["#D8C277", "#938748", "#4B4422"];
    pal.insert("crabShell", tri("#A8B85A", "#5F7C33", "#2A3B1A"));
    pal.insert("crabMottle", tri("#A29450", "#6E5E30", "#3A2F18"));
    pal.insert("crabPatch", tri("#CFD683", "#A9B95B", "#6A7E34"));
    pal.insert("crabTooth", tri("#EEF0A8", "#BCC468", "#6A7A30"));
    pal.insert("crabClaw", tri(claw[0], claw[1], claw[2]));
    pal.insert("crabClawFar", far(claw));
    pal.insert("crabBelly", tri(belly[0], belly[1], belly[2]));
    pal.insert("crabBellyFar", far(belly));
    pal.insert("crabLeg", tri(leg[0], leg[1], leg[2]));
    pal.insert("crabLegFar", far(leg));
    crate::parts::write_with(
        build(),
        "<!-- Shore crab: Rust-authored cut-out puppet (bestiary lane). -->",
        "Shore crab · rig parts",
        parent_of,
        &pal,
    )
}
