//! The CrabScape player crab as a cut-out puppet (design v0.2, the 'charm pass'). Each builder returns
//! draw-ordered items in crab space (units ≈ px at scale 1; carapace about 290 wide spine-to-spine; y down; ground at
//! y = 100 in the neutral stance).
//!
//! Sides use the CRAB's own left/right: R = crab's right (screen LEFT in the front view), L = crab's left.
//! Heterochely: the crusher claw is on the crab's right (R), the cutter on the left (L).

use crate::geom::*;
use crate::item::{Item, Kind};

/// Screen-x sign in the front view.
fn s_of(side: char) -> f64 {
    if side == 'L' { 1.0 } else { -1.0 }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hand {
    /// palm length
    pub l: f64,
    /// palm height
    pub h: f64,
    /// finger length
    pub f: f64,
}

/// Design proportions.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Design {
    /// carapace (and everything on it) relative to the legs
    pub cara: f64,
    /// cornea size
    pub eye: f64,
    /// eyestalk length
    pub stalk: f64,
    /// walking-leg thickness
    pub leg: f64,
    pub crusher: Hand,
    pub cutter: Hand,
    /// frontal-tooth depth (between the orbits)
    pub front_teeth: f64,
}
/// v0.2 charm pass (the default).
pub const DESIGN: Design = Design {
    cara: 1.12,
    eye: 1.45,
    stalk: 1.16,
    leg: 0.9,
    crusher: Hand {
        l: 90.0,
        h: 74.0,
        f: 44.0,
    },
    cutter: Hand {
        l: 74.0,
        h: 42.0,
        f: 58.0,
    },
    front_teeth: 0.35,
};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Body {
    pub dx: f64,
    pub dy: f64,
    pub rot: f64,
    pub sx: f64,
    pub sy: f64,
}
/// An eye: `a` deg from vertical (+ = toward screen right). Unset fields take the builder's defaults.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Eye {
    pub a: Option<f64>,
    pub fold: Option<f64>,
    pub len: Option<f64>,
}
/// An antenna: angle from vertical, curl.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Antenna {
    pub a: Option<f64>,
    pub curl: Option<f64>,
}
/// A claw: wrist in body space; aim: deg up from 'pointing to midline'.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Claw {
    pub wrist: P,
    pub aim: Option<f64>,
    pub gape: Option<f64>,
    pub size: Option<f64>,
    pub flip: Option<f64>,
}
/// A leg's foot in WORLD space (ground). Paddles: dactyl tip target + blade angle.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Leg {
    pub f: P,
    pub dout: Option<f64>,
    pub blade: Option<f64>,
    pub blade_w: Option<f64>,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Lantern {
    #[default]
    None,
    /// held in a claw ('R' or 'L')
    Claw(char),
    /// hung from a setae hook
    Hang,
}
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Gear {
    pub helmet: bool,
    pub lantern: Lantern,
    pub shield: Option<char>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum View {
    Front,
    Back,
}
/// Per side: index 0 = R, 1 = L.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pose {
    pub view: View,
    pub design: Design,
    pub body: Body,
    pub eyes: [Eye; 2],
    pub ant: [Antenna; 2],
    pub claws: [Claw; 2],
    pub legs: [[Leg; 4]; 2],
    pub gear: Gear,
    /// 1 = crusher replaced by a small regrowing claw
    pub regrow_r: bool,
}
pub fn si(side: char) -> usize {
    if side == 'R' { 0 } else { 1 }
}
fn leg(x: f64, y: f64) -> Leg {
    Leg {
        f: [x, y],
        ..Default::default()
    }
}

/// The default pose.
pub fn base_pose() -> Pose {
    Pose {
        view: View::Front,
        design: DESIGN,
        body: Body {
            dx: 0.0,
            dy: 0.0,
            rot: 0.0,
            sx: 1.0,
            sy: 1.0,
        },
        eyes: [
            Eye {
                a: Some(-17.0),
                fold: Some(0.0),
                len: Some(1.0),
            },
            Eye {
                a: Some(7.0),
                fold: Some(0.0),
                len: Some(1.03),
            },
        ],
        ant: [
            Antenna {
                a: Some(-38.0),
                curl: Some(0.3),
            },
            Antenna {
                a: Some(38.0),
                curl: Some(0.3),
            },
        ],
        // hero: the heavy crusher rests low across the front like a fist; the slim cutter is held up beside the face
        claws: [
            Claw {
                wrist: [-130.0, 78.0],
                aim: Some(-6.0),
                gape: Some(4.0),
                size: Some(1.0),
                flip: None,
            },
            Claw {
                wrist: [162.0, 50.0],
                aim: Some(66.0),
                gape: Some(18.0),
                size: Some(1.0),
                flip: None,
            },
        ],
        legs: [
            [
                leg(-200.0, 104.0),
                leg(-236.0, 98.0),
                leg(-262.0, 90.0),
                Leg {
                    f: [-176.0, -78.0],
                    blade: Some(-60.0),
                    ..Default::default()
                },
            ],
            [
                leg(200.0, 104.0),
                leg(236.0, 98.0),
                leg(262.0, 90.0),
                Leg {
                    f: [176.0, -78.0],
                    blade: Some(60.0),
                    ..Default::default()
                },
            ],
        ],
        gear: Gear::default(),
        regrow_r: false,
    }
}

/// What a claw builder hands back (for gear held in it).
#[derive(Clone, Copy)]
struct ClawInfo {
    w: P,
    flip: f64,
    aim: f64,
    rot: f64,
    l: f64,
    h: f64,
    f: f64,
}
impl ClawInfo {
    fn to_w(self, p: P) -> P {
        let mut q = p;
        if self.flip < 0.0 {
            q = [-q[0], q[1]];
        }
        q = rot0(
            q,
            if self.flip > 0.0 {
                -self.aim + self.rot
            } else {
                self.aim + self.rot
            },
        );
        add(self.w, q)
    }
    fn hand(&self, pts: &[P]) -> Vec<P> {
        pts.iter().map(|p| self.to_w(*p)).collect()
    }
}

/// One finger's row of scissor-claw teeth: the part it makes (`owner`), the finger's spine and width (as given
/// to `limb`), a point on the other finger (so the teeth face it), where along the finger they go, and the part's
/// pivot (its finger's joint).
struct ToothRow<'a> {
    owner: String,
    spine: &'a [P],
    width: &'a dyn Fn(f64) -> f64,
    toward: P,
    ts: [f64; 4],
    pivot: P,
}

/// Points on one edge of a limb drawn with `limb(spine, width, ..)` (smooth): for each `t` along it, the
/// edge point on the side that faces `toward`, the way out from the edge there, and the way along it.
fn edge_at(spine: &[P], width: &dyn Fn(f64) -> f64, ts: &[f64], toward: P) -> Vec<(P, P, P)> {
    let c = through(spine, 5, false);
    let n = c.len();
    let frame = |i: usize| {
        let (a, b) = (c[i.saturating_sub(1)], c[(i + 1).min(n - 1)]);
        let along = norm(sub(b, a));
        (along, perp(along), width(i as f64 / (n - 1) as f64) / 2.0)
    };
    // Which side faces `toward`: the one whose middle edge point is nearer to it.
    let mid = n / 2;
    let (_, nm, wm) = frame(mid);
    let plus = add(c[mid], [nm[0] * wm, nm[1] * wm]);
    let minus = add(c[mid], [-nm[0] * wm, -nm[1] * wm]);
    let side = if len(sub(plus, toward)) < len(sub(minus, toward)) {
        1.0
    } else {
        -1.0
    };
    ts.iter()
        .map(|&t| {
            let i = ((t * (n - 1) as f64).round() as usize).min(n - 1);
            let (along, nn, w) = frame(i);
            let out = [nn[0] * side, nn[1] * side];
            (add(c[i], [out[0] * w, out[1] * w]), out, along)
        })
        .collect()
}

struct Out(Vec<Item>);
impl Out {
    fn push(&mut self, id: impl Into<String>, g: &str, pts: Vec<P>, mat: &str) -> &mut Item {
        self.0.push(Item::new(Kind::Shape, id, Some(g), pts, mat));
        self.0.last_mut().unwrap()
    }
    fn line(&mut self, id: impl Into<String>, g: &str, pts: Vec<P>, mat: &str, sw: f64) {
        self.0
            .push(Item::new(Kind::Line, id, Some(g), pts, mat).sw(sw));
    }
}

/// JS number to string, for ids like `merus-spine0.35`.
fn js_num(v: f64) -> String {
    let s = format!("{v}");
    s
}

/// The shell seen from behind, for the game's turn to strike across: the back view's carapace (posterior border
/// nearest, the toothed edge as the far skyline), its grooves, the sternum's shadow and the folded abdomen under
/// the border, renamed `back-...`. `back-carapace` turns about its middle; the rest ride on it. The game shows it
/// in place of the front's carapace and face once the crab has turned its back.
pub fn back_shell(pose: &Pose) -> Vec<Item> {
    let keep = [
        "underside-sternum",
        "abdomen-fold",
        "carapace",
        "carapace-groove-R",
        "carapace-groove-L",
    ];
    let mut back = *pose;
    back.view = View::Back;
    back.gear = Gear::default();
    build_front(&back)
        .into_iter()
        .filter(|it| keep.contains(&it.id.as_str()))
        .map(|mut it| {
            if it.id == "carapace" {
                let b = bbox(it.pts.iter());
                it.pv = Some([b.x0 + b.w / 2.0, b.y0 + b.h / 2.0]);
            }
            it.id = format!("back-{}", it.id);
            it.g = Some("back-carapace".into());
            it
        })
        .collect()
}

/// The angles (degrees about the crab's up axis, 0 = facing the camera, 180 = its back to it) the shell is also drawn
/// at for the turn round, between the front and back views: three-quarters from the front, the side (end-on from
/// its side: narrow, the front to the right) and three-quarters from behind. Each is turned with the crab's right
/// (the crusher's side) towards the camera; the game mirrors them for a turn the other way.
pub const TURN_VIEWS: [u32; 3] = [45, 90, 135];

/// How far the camera looks down onto the crab (the sine of the angle): the front view's own look, the posterior
/// border higher than the front margin.
const LOOK_DOWN: f64 = 0.39;

/// The carapace seen from above: its margin at the height of the lateral spines (x across, z forwards with the front
/// at +z, y down; the front view's units before its carapace scale). The frontal teeth, orbits, six anterolateral
/// teeth (tips raised a little), the lateral spine, the posterolateral margin and the posterior border, all the way
/// round. With each point, the way out from the shell there.
fn shell_margin(front_teeth: f64) -> Vec<([f64; 3], P)> {
    // The crab's left half (+x), from the front middle back to the posterior middle.
    let mut half: Vec<[f64; 3]> = vec![];
    let fz = |z: f64| 57.0 + (z - 57.0) * front_teeth * 2.0;
    half.extend([
        [0.0, 0.0, 57.0],
        [6.0, 0.0, fz(59.5)],
        [13.0, 0.0, 57.0],
        [19.0, 0.0, fz(59.5)],
        [24.0, 0.0, 56.5],
        [30.0, 0.0, 55.0],
        [34.0, 0.0, 51.5],
        [40.0, 0.0, 53.5],
    ]);
    let (a, c, b) = ([44.0, 52.0], [104.0, 44.0], [128.0, 10.0]);
    let q = |t: f64| -> P {
        [
            lerp(lerp(a[0], c[0], t), lerp(c[0], b[0], t), t),
            lerp(lerp(a[1], c[1], t), lerp(c[1], b[1], t), t),
        ]
    };
    let n = 6;
    for i in 0..n {
        let (t0, t1) = (i as f64 / n as f64, (i as f64 + 0.55) / n as f64);
        let (p0, p1) = (q(t0), q(t1));
        let amp = 1.25 - 0.75 * i as f64 / (n - 1) as f64;
        let along = norm(sub(p1, p0));
        let out = [along[1], -along[0]];
        let tip = add(lerp_p(p0, p1, 0.8), mul(out, 6.0 * amp));
        half.push([p0[0], 0.0, p0[1]]);
        half.push([tip[0], -2.6 * amp, tip[1]]);
    }
    half.extend([
        [128.0, 0.0, 10.0],
        [148.0, -3.0, 4.0],
        [126.0, 0.0, -2.0],
    ]);
    let back: Vec<P> = through(
        &[[126.0, -2.0], [114.0, -22.0], [92.0, -40.0], [62.0, -51.0], [32.0, -55.5], [0.0, -57.0]],
        3,
        false,
    );
    half.extend(back.into_iter().skip(1).map(|p| [p[0], 0.0, p[1]]));
    // Round: the left half forwards, then the right half (mirrored) backwards.
    let mut ring = half.clone();
    ring.pop();
    ring.extend(half.iter().rev().skip(1).map(|p| [-p[0], p[1], p[2]]));
    ring.pop();
    let n = ring.len();
    (0..n)
        .map(|i| {
            let (prev, next) = (ring[(i + n - 1) % n], ring[(i + 1) % n]);
            let along = norm([next[0] - prev[0], next[2] - prev[2]]);
            let mut out = [along[1], -along[0]];
            if out[0] * ring[i][0] + out[1] * ring[i][2] < 0.0 {
                out = mul(out, -1.0);
            }
            (ring[i], out)
        })
        .collect()
}

/// The dome of the carapace over its margin: highest a little behind the middle.
fn shell_dome(x: f64, z: f64) -> f64 {
    let r = (x / 122.0).powi(2) + ((z + 4.0) / 52.0).powi(2);
    -30.0 * (1.0 - r).max(0.0).sqrt()
}

/// Upper convex hull (smallest y, y down) of `pts`, from the leftmost to the rightmost.
fn upper_hull(pts: &[P]) -> Vec<P> {
    let mut s: Vec<P> = pts.to_vec();
    s.sort_by(|a, b| a[0].total_cmp(&b[0]).then(a[1].total_cmp(&b[1])));
    let mut hull: Vec<P> = vec![];
    for p in s {
        while hull.len() >= 2 {
            let (a, b) = (hull[hull.len() - 2], hull[hull.len() - 1]);
            // keep only clockwise turns on screen (y down): the top of the shape
            if (b[0] - a[0]) * (p[1] - a[1]) - (b[1] - a[1]) * (p[0] - a[0]) <= 0.0 {
                hull.pop();
            } else {
                break;
            }
        }
        hull.push(p);
    }
    hull
}

/// The shell drawn turned `yaw` degrees round (see [`TURN_VIEWS`]): a 3D model of the carapace (its toothed margin
/// and dome) and the body under it, seen from the front view's height. The outline is the dome's top over the margin
/// on the near side (its teeth), the body under it (warm face from the front, the pale underside from behind, in
/// shadow), the brim's shadow, the grooves where they face the camera, and from in front the mouthparts. Named
/// `turn<yaw>-...`; `turn<yaw>-carapace` turns about its middle and the rest ride on it.
pub fn turn_shell(pose: &Pose, yaw: u32) -> Vec<Item> {
    let ds = pose.design;
    let b = pose.body;
    let body_m = move |p: P| -> P {
        let q = rot0([p[0] * b.sx, p[1] * b.sy], b.rot);
        [q[0] + b.dx, q[1] + b.dy]
    };
    let csc = ds.cara;
    let th = yaw as f64 * D;
    let (st, ct) = th.sin_cos();
    let ce = (1.0 - LOOK_DOWN * LOOK_DOWN).sqrt();
    // Nearer the camera is lower on screen; far past the margins the look flattens (so a spine pointing at the
    // camera doesn't drop below the legs).
    let depth = move |p: [f64; 3]| -p[0] * st + p[2] * ct;
    let proj = move |p: [f64; 3]| -> P {
        let d = depth(p);
        let d = if d.abs() > 60.0 {
            d.signum() * (60.0 + (d.abs() - 60.0) * 0.3)
        } else {
            d
        };
        let x = p[0] * ct + p[2] * st;
        let y = p[1] * ce + d * LOOK_DOWN - 12.6;
        body_m([x * csc, (y + 6.0) * csc - 6.0])
    };
    let proj_all = |pts: &[[f64; 3]]| -> Vec<P> { pts.iter().map(|p| proj(*p)).collect() };
    let id = |s: &str| format!("turn{yaw}-{s}");
    let g = id("carapace");
    let mut out = Out(vec![]);
    let from_front = ct > 0.2;

    // The body under the shell: the margin's underside and the sternum, with the face in front.
    let ring = |y: f64, rx: f64, zf: f64, zb: f64| -> Vec<[f64; 3]> {
        (0..32)
            .map(|i| {
                let a = i as f64 / 32.0 * std::f64::consts::TAU;
                let z = if a.sin() > 0.0 { zf } else { zb };
                [a.cos() * rx, y, a.sin() * z]
            })
            .collect()
    };
    let sternum = ring(46.0, 100.0, 40.0, 44.0);
    let st_pts = proj_all(&sternum);
    out.push(id("underside"), &g, convex(&st_pts), if from_front { "face" } else { "vent" })
        .shade = true;
    let mut body: Vec<[f64; 3]> = ring(2.0, 122.0, 52.0, 52.0);
    body.extend(ring(36.0, 104.0, 46.0, 42.0));
    if from_front {
        // the mouth's front, between the eyes
        body.extend([[-36.0, 40.0, 50.0], [36.0, 40.0, 50.0], [0.0, 42.0, 52.0]]);
    }
    let it = out.push(
        id("body"),
        &g,
        convex(&proj_all(&body)),
        if from_front { "face" } else { "vent" },
    );
    it.under = true;
    if from_front {
        // third maxillipeds: two plates like doors over the mouth, on the front
        for s in [-1.0, 1.0] {
            let (x0, x1) = (s * 1.5, s * 17.0);
            let plate: Vec<[f64; 3]> = through(
                &[[x0, 16.0], [x1 - s * 2.0, 15.0], [x1, 24.0], [x1 - s * 2.0, 36.0], [x0, 38.0]],
                3,
                false,
            )
            .into_iter()
            .map(|p| [p[0], p[1], 50.0])
            .collect();
            out.push(id(&format!("maxilliped{s}")), &g, proj_all(&plate), "face")
                .sw = Some(0.6);
        }
    } else {
        // the folded abdomen under the posterior border
        let fold: Vec<[f64; 3]> = through(
            &[[-20.0, 28.0], [20.0, 28.0], [16.0, 42.0], [0.0, 45.0], [-16.0, 42.0]],
            4,
            true,
        )
        .into_iter()
        .map(|p| [p[0], p[1], -44.0])
        .collect();
        out.push(id("abdomen"), &g, proj_all(&fold), "vent");
    }

    // The shell: the dome's top, then back along the margin's near side.
    let margin = shell_margin(ds.front_teeth);
    let rim: Vec<P> = margin.iter().map(|(p, _)| proj(*p)).collect();
    let mut tops: Vec<P> = rim.clone();
    for i in 0..=24 {
        for k in 0..=12 {
            let (x, z) = (-122.0 + 244.0 * i as f64 / 24.0, -56.0 + 104.0 * k as f64 / 12.0);
            let y = shell_dome(x, z);
            if y < -0.5 {
                tops.push(proj([x, y, z]));
            }
        }
    }
    let top = upper_hull(&tops);
    let (left, right) = (top[0], top[top.len() - 1]);
    let find = |q: P| {
        rim.iter()
            .position(|p| p[0] == q[0] && p[1] == q[1])
            .unwrap_or(0)
    };
    let (il, ir) = (find(left), find(right));
    let n = rim.len();
    // the two ways round the margin from right to left; the near one is in front
    let arc = |step: usize| -> Vec<usize> {
        let mut v = vec![ir];
        let mut i = ir;
        while i != il {
            i = (i + step) % n;
            v.push(i);
        }
        v
    };
    let (a1, a2) = (arc(1), arc(n - 1));
    let near_of = |v: &[usize]| v.iter().map(|&i| depth(margin[i].0)).sum::<f64>() / v.len() as f64;
    let near = if near_of(&a1) > near_of(&a2) { a1 } else { a2 };
    let mut shell = top.clone();
    shell.extend(near.iter().skip(1).take(near.len().saturating_sub(2)).map(|&i| rim[i]));
    let cara = out.push(g.clone(), &g, shell, "shell");
    let bb = bbox(cara.pts.iter());
    cara.pv = Some([bb.x0 + bb.w / 2.0, bb.y0 + bb.h / 2.0]);
    // the brim's soft shadow on the body, just under the near margin
    let brim: Vec<[f64; 3]> = near
        .iter()
        .map(|&i| margin[i].0)
        .filter(|p| p[1] == 0.0)
        .collect();
    if brim.len() > 2 {
        let mut sh: Vec<P> = proj_all(&brim);
        sh.extend(brim.iter().rev().map(|p| proj([p[0] * 0.94, 11.0, p[2] * 0.94])));
        let it = out.push(id("brim-shadow"), &g, sh, "face");
        it.shade = true;
        it.ink = false;
        it.op = Some(0.7);
    }
    // grooves on the dome (cervical, branchial), where they face the camera
    let toward = [-st * ce, -LOOK_DOWN, ct * ce];
    let faces = |x: f64, z: f64| {
        let h = 0.5;
        let dx = (shell_dome(x + h, z) - shell_dome(x - h, z)) / (2.0 * h);
        let dz = (shell_dome(x, z + h) - shell_dome(x, z - h)) / (2.0 * h);
        dx * toward[0] - toward[1] + dz * toward[2] > 0.15
    };
    for (s, side) in [(-1.0, "R"), (1.0, "L")] {
        let groove: Vec<[f64; 3]> = [[26.0, -20.0], [42.0, -8.0], [70.0, 2.0]]
            .iter()
            .filter(|p| faces(s * p[0], p[1]))
            .map(|p| [s * p[0], shell_dome(s * p[0], p[1]), p[1]])
            .collect();
        if groove.len() >= 2 {
            out.line(id(&format!("groove-{side}")), &g, proj_all(&groove), "ink", 0.5);
        }
    }
    out.0
}

/// Convex hull of `pts` (a closed outline).
fn convex(pts: &[P]) -> Vec<P> {
    let mut top = upper_hull(pts);
    let flipped: Vec<P> = pts.iter().map(|p| [p[0], -p[1]]).collect();
    let mut bottom: Vec<P> = upper_hull(&flipped)
        .into_iter()
        .map(|p| [p[0], -p[1]])
        .collect();
    bottom.reverse();
    top.pop();
    bottom.pop();
    top.extend(bottom);
    top
}

// ---------- the front / back view puppet ----------
pub fn build_front(pose: &Pose) -> Vec<Item> {
    let back = pose.view == View::Back;
    let mut out = Out(vec![]);
    let b = pose.body;
    let ds = pose.design;
    let csc = ds.cara; // carapace scale about its centre (0, -6)
    let cs = move |p: P| -> P { [p[0] * csc, (p[1] + 6.0) * csc - 6.0] };
    let body_m = move |p: P| -> P {
        let q = rot0([p[0] * b.sx, p[1] * b.sy], b.rot);
        [q[0] + b.dx, q[1] + b.dy]
    };
    let bp = move |pts: &[P]| -> Vec<P> { pts.iter().map(|p| body_m(*p)).collect() };
    // in the back view the crab's right is on screen RIGHT
    let sx = |side: char| if back { -s_of(side) } else { s_of(side) };
    let v2 = ds.eye > 1.0;

    // ----- walking legs -----
    struct LegDim {
        hip: P,
        m: f64,
        c: f64,
        p: f64,
        d: f64,
        paddle: bool,
    }
    let leg_dims = [
        LegDim {
            hip: [78.0, 26.0],
            m: 76.0,
            c: 30.0,
            p: 44.0,
            d: 36.0,
            paddle: false,
        },
        LegDim {
            hip: [94.0, 18.0],
            m: 82.0,
            c: 32.0,
            p: 46.0,
            d: 38.0,
            paddle: false,
        },
        LegDim {
            hip: [106.0, 10.0],
            m: 80.0,
            c: 31.0,
            p: 44.0,
            d: 36.0,
            paddle: false,
        },
        LegDim {
            hip: [112.0, -2.0],
            m: 60.0,
            c: 26.0,
            p: 30.0,
            d: 40.0,
            paddle: true,
        },
    ];
    let walk_leg = |out: &mut Out, side: char, i: usize| {
        let (s, ld, spec) = (sx(side), &leg_dims[i], pose.legs[si(side)][i]);
        let hip = body_m([s * ld.hip[0], ld.hip[1]]);
        let mut f = spec.f;
        if back {
            f[0] = -f[0];
        }
        let gid = format!("leg-{side}{}", i + 1);
        if !ld.paddle {
            let dd = norm([s * spec.dout.unwrap_or(0.32), 1.0]);
            let p = sub(f, mul(dd, ld.d));
            let l2 = ld.c + ld.p - 4.0;
            let Ik { joint: k, end } = ik2(hip, p, ld.m, l2, |j| -j[1] * 1.0 + 0.15 * s * j[0]); // knee up
            let c = add(k, add(mul(norm(sub(end, k)), ld.c), mul([s, 0.0], 3.0))); // carpus end, slight outward kink
            let (pend, ff) = (end, add(end, mul(dd, ld.d)));
            let w = if i == 1 { 1.06 } else { 1.0 } * ds.leg;
            let (far, mid) = if v2 && !back {
                (i == 2, i == 1)
            } else {
                (false, false)
            };
            let mut put = |id: String, pts: Vec<P>, pv: P| {
                let it = out.push(id, &gid, pts, "leg");
                it.pv = Some(pv);
                it.far = far;
                it.mid = mid;
            };
            put(
                format!("{gid}-merus"),
                limb(
                    &[hip, lerp_p(hip, k, 0.5), k],
                    |t| (21.0 - 4.0 * t + 3.0 * (t * std::f64::consts::PI).sin()) * w,
                    LimbOpt::default(),
                ),
                hip,
            );
            put(
                format!("{gid}-carpus"),
                limb(&[k, c], |t| 16.0 * w - t * 1.5, LimbOpt::default()),
                k,
            );
            put(
                format!("{gid}-propodus"),
                limb(&[c, pend], |t| (14.0 - 4.0 * t) * w, LimbOpt::default()),
                c,
            );
            put(
                format!("{gid}-dactyl"),
                limb(
                    &[pend, lerp_p(pend, ff, 0.55), ff],
                    |t| (10.5 * (1.0 - t) + 1.2) * w,
                    OPEN_END,
                ),
                pend,
            );
            // Gear and a feat on the leg tip, each its own part riding on the dactyl; the game shows them only
            // once fitted or grown. Glass grips: a glass sponge's spicules set over the tip as a pale sheath with
            // barbs, its point a little past the foot. Gripping dactyls: the tip grows a dark hook curling in
            // towards the body.
            let along = norm(sub(ff, pend));
            let mut inward = perp(along);
            if inward[0] * s > 0.0 {
                inward = mul(inward, -1.0);
            }
            let at = |t: f64, side: f64| add(lerp_p(pend, ff, t), mul(inward, side));
            let glass = vec![
                at(0.30, 5.5 * w),
                at(0.52, 9.0 * w),
                at(0.58, 5.0 * w),
                at(0.80, 6.5 * w),
                add(ff, mul(along, 7.0)),
                at(0.80, -6.5 * w),
                at(0.58, -5.0 * w),
                at(0.52, -9.0 * w),
                at(0.30, -5.5 * w),
            ];
            let mut push_tip = |id: String, pts: Vec<P>, mat: &str, sw: f64| {
                let it = out.push(id, &gid, pts, mat);
                it.pv = Some(pend);
                it.far = far;
                it.mid = mid;
                it.sw = Some(sw);
            };
            push_tip(format!("{gid}-dactyl-glass"), glass, "glassSponge", 0.5);
            let hook = limb(
                &[
                    lerp_p(pend, ff, 0.62),
                    add(ff, mul(along, 2.0)),
                    add(add(ff, mul(along, 1.0)), mul(inward, 8.0)),
                    add(add(ff, mul(along, -5.0)), mul(inward, 12.0)),
                ],
                |t| 7.5 * (1.0 - t) * w + 1.4,
                OPEN_END,
            );
            push_tip(format!("{gid}-dactyl-hook"), hook, "tip", 0.5);
        } else {
            // swimming paddle (5th pereiopod): propodus + dactyl flattened into an oval blade
            let blade = spec.blade.unwrap_or(70.0) * if back { -1.0 } else { 1.0 };
            let bdir = [(blade * D).sin(), -(blade * D).cos()];
            let p = sub(f, mul(bdir, ld.d));
            let Ik { joint: k, end } = ik2(hip, p, ld.m, ld.c + ld.p - 4.0, |j| {
                s * j[0] * 1.0 + j[1] * 0.3
            });
            let c = add(k, mul(norm(sub(end, k)), ld.c));
            out.push(
                format!("{gid}-merus"),
                &gid,
                limb(
                    &[hip, lerp_p(hip, k, 0.5), k],
                    |t| 18.0 - 3.0 * t,
                    LimbOpt::default(),
                ),
                "leg",
            )
            .pv = Some(hip);
            out.push(
                format!("{gid}-carpus"),
                &gid,
                limb(&[k, c], |_| 14.0, LimbOpt::default()),
                "leg",
            )
            .pv = Some(k);
            out.push(
                format!("{gid}-propodus"),
                &gid,
                limb(&[c, end], |t| 15.0 + 5.0 * t, LimbOpt::default()),
                "leg",
            )
            .pv = Some(c);
            // blade: lens shape along bdir from end to f
            let (mid, half, wid) = (
                lerp_p(end, f, 0.5),
                ld.d / 2.0 + 4.0,
                spec.blade_w.unwrap_or(13.0),
            );
            let n = perp(bdir);
            let mut pts = vec![];
            for kk in 0..=16 {
                let a = kk as f64 / 16.0 * std::f64::consts::PI;
                pts.push(add(
                    mid,
                    add(mul(bdir, -a.cos() * half), mul(n, a.sin() * wid)),
                ));
            }
            for kk in 1..16 {
                let a = kk as f64 / 16.0 * std::f64::consts::PI;
                pts.push(add(
                    mid,
                    add(mul(bdir, a.cos() * half), mul(n, -a.sin() * wid * 0.8)),
                ));
            }
            out.push(format!("{gid}-dactyl-paddle"), &gid, pts, "leg")
                .pv = Some(end);
            out.line(
                format!("{gid}-paddle-rib"),
                &gid,
                vec![
                    lerp_p(end, f, 0.12),
                    lerp_p(end, f, 0.5),
                    lerp_p(end, f, 0.85),
                ],
                "ink",
                0.5,
            );
            // The Swimming feat, broad paddles: a broader, longer blade over the paddle, its own part riding on
            // it with two ribs (the game shows it once grown).
            let (bhalf, bwid) = (half * 1.18, wid * 1.85);
            let mut broad = vec![];
            for kk in 0..=16 {
                let a = kk as f64 / 16.0 * std::f64::consts::PI;
                broad.push(add(
                    add(mid, mul(bdir, 3.0)),
                    add(mul(bdir, -a.cos() * bhalf), mul(n, a.sin() * bwid)),
                ));
            }
            for kk in 1..16 {
                let a = kk as f64 / 16.0 * std::f64::consts::PI;
                broad.push(add(
                    add(mid, mul(bdir, 3.0)),
                    add(mul(bdir, a.cos() * bhalf), mul(n, -a.sin() * bwid * 0.75)),
                ));
            }
            out.push(format!("{gid}-paddle-broad"), &gid, broad, "leg")
                .pv = Some(end);
            for (r, side) in [(0, 0.45), (1, -0.35)] {
                out.line(
                    format!("{gid}-paddle-broad-rib{r}"),
                    &gid,
                    vec![
                        lerp_p(end, f, 0.15),
                        add(lerp_p(end, f, 0.55), mul(n, side * bwid)),
                        add(lerp_p(end, f, 0.95), mul(n, side * bwid * 0.6)),
                    ],
                    "ink",
                    0.5,
                );
            }
        }
    };

    // ----- chelipeds -----
    let cheliped = |out: &mut Out, side: char| -> ClawInfo {
        let (s, spec) = (sx(side), pose.claws[si(side)]);
        let crusher = side == 'R' && !pose.regrow_r;
        let small = side == 'R' && pose.regrow_r;
        let gid = format!("cheliped-{side}");
        let coxa = body_m([s * 60.0, 30.0]);
        let mut wr = spec.wrist;
        if back {
            wr[0] = -wr[0];
        }
        let w = body_m(wr);
        let km = if small { 0.62 } else { 1.0 };
        // Merus and carpus: a little long for a swimming crab's reach (#90).
        let (ml, cl) = (60.0 * km, 34.0 * km);
        let e = ik2(coxa, w, ml, cl, |j| s * j[0] + j[1] * 0.4).joint; // elbow outward/down
        let mw = if crusher {
            28.0 + 6.0 * (ds.crusher.h / 60.0 - 1.0) * 3.0
        } else {
            24.0
        } * km;
        out.push(
            format!("{gid}-merus"),
            &gid,
            limb(
                &[coxa, lerp_p(coxa, e, 0.5), e],
                |t| mw - 4.0 * t + 2.0 * (t * std::f64::consts::PI).sin(),
                LimbOpt::default(),
            ),
            "leg",
        )
        .pv = Some(coxa);
        // merus spines on the upper edge
        let md = norm(sub(e, coxa));
        let mn = mul(perp(md), if md[0] * s > 0.0 { -1.0 } else { 1.0 });
        for t in [0.35, 0.65] {
            let bb = lerp_p(coxa, e, t);
            let nrm = if mn[1] < 0.0 { mn } else { mul(mn, -1.0) };
            out.push(
                format!("{gid}-merus-spine{}", js_num(t)),
                &gid,
                vec![
                    add(bb, mul(md, -4.0 * km)),
                    add(bb, mul(nrm, mw * 0.5 + 3.5 * km)),
                    add(bb, mul(md, 4.0 * km)),
                ],
                "leg",
            )
            .sw = Some(0.6);
        }
        // carpus (wrist) knob with inner carpal spine
        let cd = norm(sub(w, e));
        let cw = if crusher {
            24.0 + (ds.crusher.h - 60.0) * 0.4
        } else {
            24.0
        };
        out.push(
            format!("{gid}-carpus"),
            &gid,
            limb(
                &[e, lerp_p(e, w, 0.5), w],
                |t| (cw + 6.0 * (t * std::f64::consts::PI).sin()) * km,
                LimbOpt::default(),
            ),
            "leg",
        )
        .pv = Some(e);
        let csp = lerp_p(e, w, 0.7);
        let cn = perp(cd);
        let cnu = if cn[1] < 0.0 { cn } else { mul(cn, -1.0) };
        out.push(
            format!("{gid}-carpus-spine"),
            &gid,
            vec![
                add(csp, mul(cd, -5.0 * km)),
                add(csp, add(mul(cnu, 18.0 * km), mul(cd, 6.0 * km))),
                add(csp, mul(cd, 6.0 * km)),
            ],
            "leg",
        )
        .sw = Some(0.6);
        // hand: built pointing +x with the FIXED finger below (+y); mirrored for the other side
        let dims = if crusher {
            ds.crusher
        } else if small {
            Hand {
                l: 38.0,
                h: 22.0,
                f: 26.0,
            }
        } else {
            ds.cutter
        };
        let k = spec.size.unwrap_or(1.0);
        let (l, h, ff) = (dims.l * k, dims.h * k, dims.f * k);
        // which way the fingers point: toward the midline in screen space, then raised by aim
        let flip = spec
            .flip
            .unwrap_or(if s < 0.0 { 1.0 } else { -1.0 } * if back { -1.0 } else { 1.0 });
        let ci = ClawInfo {
            w,
            flip,
            aim: spec.aim.unwrap_or(0.0),
            rot: b.rot,
            l,
            h,
            f: ff,
        };
        let hp = |pts: &[P]| ci.hand(pts);
        let prop = format!("{gid}-propodus");
        // fixed finger (part of the propodus) — drawn first, palm overlaps its root
        let fpts = [
            [l * 0.78, h * 0.22],
            [l + ff * 0.5, h * 0.30],
            [l + ff, h * 0.12],
        ];
        let fixed = limb(&fpts, |t| h * 0.42 * (1.0 - t) + 3.0, OPEN_END);
        let fixed_tip = limb(
            &[lerp_p(fpts[1], fpts[2], 0.35), fpts[2]],
            |t| h * 0.42 * (1.0 - (0.55 + 0.45 * t)) + 3.0,
            LimbOpt {
                cap1: false,
                smooth: false,
                ..Default::default()
            },
        );
        // palm
        let palm = through(
            &[
                [-4.0, 0.0],
                [2.0, -h * 0.40],
                [l * 0.45, -h * 0.54],
                [l * 0.92, -h * 0.44],
                [l * 1.04, -h * 0.12],
                [l * 0.98, h * 0.30],
                [l * 0.6, h * 0.5],
                [l * 0.1, h * 0.44],
            ],
            5,
            true,
        );
        // dactyl: pivot near the top-front of the palm, closes down onto the fixed finger
        let (piv, g) = ([l * 0.9, -h * 0.26], spec.gape.unwrap_or(0.0));
        let dsp: Vec<P> = [
            [0.0, 0.0],
            [ff * 0.55 + l * 0.05, -h * 0.06],
            [ff + l * 0.1, h * 0.34],
        ]
        .iter()
        .map(|p| add(piv, rot0(*p, -g)))
        .collect();
        let dact = limb(&dsp, |t| h * 0.40 * (1.0 - t) + 3.0, OPEN_END);
        let dtip = limb(
            &[lerp_p(dsp[1], dsp[2], 0.35), dsp[2]],
            |t| h * 0.40 * (1.0 - (0.55 + 0.45 * t)) + 3.0,
            LimbOpt {
                cap1: false,
                smooth: false,
                ..Default::default()
            },
        );
        out.push(
            format!("{gid}-propodus-fixedfinger"),
            &prop,
            hp(&fixed),
            "leg",
        );
        out.push(
            format!("{gid}-propodus-fixedfinger-tip"),
            &prop,
            hp(&fixed_tip),
            "tip",
        )
        .sw = Some(0.5);
        if crusher {
            // molariform teeth on the crusher's fixed finger
            for t in [0.45, 0.8] {
                let p = lerp_p(fpts[0], fpts[1], t);
                out.push(
                    format!("{gid}-molar{}", js_num(t)),
                    &prop,
                    hp(&ell(p[0] + 4.0, p[1] - h * 0.1, 4.2 * k, 3.2 * k, 10, 0.0)),
                    "vent",
                )
                .sw = Some(0.45);
            }
        }
        out.push(format!("{gid}-propodus-palm"), &prop, hp(&palm), "leg")
            .pv = Some(w);
        // pale underside of the hand (the chela's ventral face shows a little in front view), v0.2 only
        if v2 {
            out.push(
                format!("{gid}-palm-belly"),
                &prop,
                hp(&through(
                    &[
                        [l * 0.14, h * 0.40],
                        [l * 0.5, h * 0.26],
                        [l * 0.9, h * 0.2],
                        [l * 0.97, h * 0.3],
                        [l * 0.6, h * 0.45],
                        [l * 0.16, h * 0.41],
                    ],
                    4,
                    true,
                )),
                "belly",
            )
            .ink = false;
        }
        // palm ridge line (portunid hands are ridged)
        out.line(
            format!("{gid}-palm-ridge"),
            &prop,
            hp(&[
                [l * 0.12, -h * 0.1],
                [l * 0.5, -h * 0.2],
                [l * 0.86, -h * 0.12],
            ]),
            "ink",
            0.55,
        );
        // Clam-shell guards (made at the workbench from a clam's two valves): a valve strapped over the outside of
        // the palm, its hinge at the wrist and its ribbed fan opening towards the fingers, which stay free. Its own
        // part, riding on the palm; the game shows it only once made.
        let valve_at = |a: f64, r: f64| -> P {
            let a = a * D;
            [
                l * 0.08 + a.cos() * l * 0.86 * r,
                -h * 0.02 + a.sin() * h * 0.56 * r,
            ]
        };
        let mut valve = vec![valve_at(180.0, 0.12)];
        for kk in 0..=12 {
            let a = -78.0 + 150.0 * kk as f64 / 12.0;
            // A clam's growing edge is a little wavy where each rib ends.
            let r = if kk % 2 == 0 { 1.0 } else { 0.96 };
            valve.push(valve_at(a, r));
        }
        let guard = format!("{gid}-propodus-guard");
        let it = out.push(guard.clone(), &prop, hp(&valve), "clamValve");
        it.pv = Some(w);
        it.sw = Some(0.6);
        for (r, a) in [-50.0, -18.0, 14.0, 44.0].into_iter().enumerate() {
            out.line(
                format!("{guard}-rib{r}"),
                &prop,
                hp(&[valve_at(a, 0.22), valve_at(a, 0.62), valve_at(a, 0.9)]),
                "ink",
                0.45,
            );
        }
        let dg = format!("{gid}-dactyl");
        out.push(format!("{gid}-dactyl"), &dg, hp(&dact), "leg").pv = Some(ci.to_w(piv));
        out.push(format!("{gid}-dactyl-tip"), &dg, hp(&dtip), "tip")
            .sw = Some(0.5);
        if crusher {
            // Scissor claws (an upgrade in the game: a shed tooth's edges fitted along both fingers): a row of
            // teeth on each finger's facing edge, offset so the rows interlock when the claw closes. Each row
            // is its own part, pivoting with its finger; the game shows them only once fitted.
            let fixed_w = |t: f64| h * 0.42 * (1.0 - t) + 3.0;
            let dact_w = |t: f64| h * 0.40 * (1.0 - t) + 3.0;
            let rows = [
                ToothRow {
                    owner: format!("{gid}-propodus-scissor"),
                    spine: &fpts[..],
                    width: &fixed_w,
                    toward: dsp[1],
                    ts: [0.5, 0.63, 0.76, 0.89],
                    pivot: w,
                },
                ToothRow {
                    owner: format!("{gid}-dactyl-scissor"),
                    spine: &dsp[..],
                    width: &dact_w,
                    toward: fpts[1],
                    ts: [0.44, 0.57, 0.70, 0.83],
                    pivot: ci.to_w(piv),
                },
            ];
            for ToothRow {
                owner,
                spine,
                width,
                toward,
                ts,
                pivot,
            } in rows
            {
                for (i, (e, o, a)) in edge_at(spine, width, &ts, toward).into_iter().enumerate() {
                    let (bw, th) = (2.8 * k, 5.0 * k);
                    let tooth = vec![
                        [e[0] + a[0] * bw, e[1] + a[1] * bw],
                        [e[0] - a[0] * bw, e[1] - a[1] * bw],
                        [e[0] + o[0] * th, e[1] + o[1] * th],
                    ];
                    let id = if i == 0 {
                        owner.clone()
                    } else {
                        format!("{owner}-tooth{i}")
                    };
                    let it = out.push(id, &owner, hp(&tooth), "vent");
                    it.sw = Some(0.45);
                    if i == 0 {
                        it.pv = Some(pivot);
                    }
                }
            }
        }
        ci
    };

    // ----- carapace -----
    const SPX: f64 = 146.0;
    let carapace_pts = || -> Vec<P> {
        // spine tips, back edge, anterolateral teeth, orbits, frontal teeth
        let mut p: Vec<P> = vec![];
        let back_edge = through(
            &[
                [-124.0, -18.0],
                [-96.0, -38.0],
                [-50.0, -51.0],
                [0.0, -55.0],
                [50.0, -51.0],
                [96.0, -38.0],
                [124.0, -18.0],
            ],
            4,
            false,
        );
        p.push([-SPX, -12.0]);
        p.extend(back_edge);
        p.push([SPX, -12.0]);
        p.push([126.0, -2.0]);
        // left anterolateral margin with 6 teeth, from spine base to orbit
        let al = |sgn: f64| -> Vec<P> {
            let mut pts = vec![];
            let (a, bp_, cc) = ([126.0 * sgn, -2.0], [44.0 * sgn, 7.0], [96.0 * sgn, 10.0]);
            let q = |t: f64| {
                [
                    lerp(lerp(a[0], cc[0], t), lerp(cc[0], bp_[0], t), t),
                    lerp(lerp(a[1], cc[1], t), lerp(cc[1], bp_[1], t), t),
                ]
            };
            let n = 6;
            for i in 0..n {
                let (t0, t1) = (i as f64 / n as f64, (i as f64 + 0.55) / n as f64);
                let (aa, bb) = (q(t0), q(t1));
                let amp = if v2 {
                    1.25 - 0.75 * i as f64 / (n - 1) as f64
                } else {
                    1.0
                };
                pts.push(aa);
                let tip = add(
                    lerp_p(aa, bb, 0.8),
                    if v2 {
                        [sgn * 6.0 * amp, -2.6 * amp]
                    } else {
                        [sgn * 5.0, 1.2]
                    },
                );
                pts.push(tip);
            }
            pts.push(bp_);
            pts
        };
        p.extend(al(1.0));
        // left orbit notch, frontal teeth (4), right orbit notch
        p.extend([[40.0, 4.0], [34.0, 3.0], [30.0, 7.0]]);
        let ft = ds.front_teeth;
        let fy = |y: f64| 10.0 + (y - 10.0) * ft; // frontal teeth depth
        p.extend([
            [24.0, 9.0],
            [19.0, fy(12.5)],
            [13.0, 10.0],
            [6.0, fy(12.5)],
            [0.0, 10.0],
            [-6.0, fy(12.5)],
            [-13.0, 10.0],
            [-19.0, fy(12.5)],
            [-24.0, 9.0],
        ]);
        p.extend([[-30.0, 7.0], [-34.0, 3.0], [-40.0, 4.0]]);
        let mut r = al(-1.0);
        r.reverse();
        p.extend(r);
        p.push([-126.0, -2.0]);
        p.into_iter().map(cs).collect()
    };
    let back_carapace_pts = || -> Vec<P> {
        // seen from behind and slightly above: posterior border nearest (low), toothed anterolateral edge is the far skyline
        let sky = through(
            &[
                [-124.0, -6.0],
                [-100.0, -30.0],
                [-60.0, -46.0],
                [0.0, -52.0],
                [60.0, -46.0],
                [100.0, -30.0],
                [124.0, -6.0],
            ],
            4,
            false,
        );
        // add teeth bumps to the skyline
        let toothy = sky.iter().enumerate().map(|(i, p)| {
            if i % 3 == 1 && p[0].abs() > 40.0 {
                [p[0], p[1] - 4.0]
            } else {
                *p
            }
        });
        let mut p: Vec<P> = vec![[-SPX, -8.0]];
        p.extend(toothy);
        p.push([SPX, -8.0]);
        p.extend(through(
            &[
                [126.0, 0.0],
                [96.0, 14.0],
                [56.0, 24.0],
                [36.0, 30.0],
                [0.0, 31.0],
                [-36.0, 30.0],
                [-56.0, 24.0],
                [-96.0, 14.0],
                [-126.0, 0.0],
            ],
            4,
            false,
        ));
        p.into_iter().map(cs).collect()
    };
    let face_pts = || -> Vec<P> {
        through(
            &[
                [-128.0, -4.0],
                [-96.0, 6.0],
                [-44.0, 8.0],
                [0.0, 9.0],
                [44.0, 8.0],
                [96.0, 6.0],
                [128.0, -4.0],
                [112.0, 14.0],
                [76.0, 30.0],
                [36.0, 40.0],
                [0.0, 42.0],
                [-36.0, 40.0],
                [-76.0, 30.0],
                [-112.0, 14.0],
            ],
            4,
            true,
        )
        .into_iter()
        .map(cs)
        .collect()
    };

    // ----- eyes -----
    let eye = |out: &mut Out, side: char| {
        let (s, spec) = (s_of(side), pose.eyes[si(side)]);
        let gid = format!("eye-{side}");
        let (base, e) = (cs([s * 36.0, 2.0]), ds.eye);
        let fold = spec.fold.unwrap_or(0.0);
        // folded: the stalk lies outward along the front margin in its orbit groove
        let a = lerp(spec.a.unwrap_or(0.0), s * 96.0, fold);
        let lk = 58.0 * ds.stalk * spec.len.unwrap_or(1.0) * lerp(1.0, 0.72, fold);
        let d = [(a * D).sin(), -(a * D).cos()];
        let tip = add(base, mul(d, lk));
        let mid = add(lerp_p(base, tip, 0.5), mul(perp(d), s * 3.0 * (1.0 - fold)));
        let stalk = if v2 {
            limb(&[base, mid, tip], |t| 13.0 - 3.0 * t, LimbOpt::default())
        } else {
            limb(&[base, mid, tip], |t| 13.0 - 2.0 * t, LimbOpt::default())
        };
        out.push(format!("{gid}-stalk"), &gid, bp(&stalk), "stalk")
            .pv = Some(body_m(base));
        // eye bulb: stalk end + dark cornea cap
        let ec = add(tip, mul(d, 6.0 * e));
        out.push(
            format!("{gid}-cornea"),
            &gid,
            bp(&ell(ec[0], ec[1], 14.5 * e, 17.5 * e, 28, a)),
            "cornea",
        );
        if v2 {
            // glossy: light bouncing up off the floor makes a soft crescent low on the cornea, a big key glint up top
            let rim_c = add(ec, rot0([1.5 * e, 6.0 * e], a));
            let it = out.push(
                format!("{gid}-cornea-rim"),
                &gid,
                bp(&ell(rim_c[0], rim_c[1], 10.5 * e, 9.0 * e, 20, a)),
                "corneaRim",
            );
            it.ink = false;
            it.op = Some(0.55);
            out.push(
                format!("{gid}-cornea-core"),
                &gid,
                bp(&ell(
                    ec[0] - 0.5 * e,
                    ec[1] - 1.8 * e,
                    12.0 * e,
                    14.0 * e,
                    24,
                    a,
                )),
                "cornea",
            )
            .ink = false;
        }
        let g = add(
            ec,
            rot0(
                if v2 {
                    [-5.2 * e, -6.8 * e]
                } else {
                    [-5.0, -6.5]
                },
                a,
            ),
        );
        let gk = |v: f64| if v2 { v * e } else { 1.0 };
        out.push(
            format!("{gid}-glint"),
            &gid,
            bp(&ell(
                g[0],
                g[1],
                3.8 * gk(1.25),
                4.8 * gk(1.2),
                14,
                a + if v2 { -20.0 } else { 0.0 },
            )),
            "glint",
        )
        .ink = false;
        let g2 = add(
            ec,
            rot0(if v2 { [5.5 * e, 6.5 * e] } else { [5.0, 6.0] }, a),
        );
        let it = out.push(
            format!("{gid}-glint2"),
            &gid,
            bp(&ell(g2[0], g2[1], 1.6 * gk(1.3), 1.8 * gk(1.3), 10, a)),
            "glint",
        );
        it.ink = false;
        it.op = Some(0.7);
    };
    let antenna = |out: &mut Out, side: char| {
        let (s, spec) = (s_of(side), pose.ant[si(side)]);
        let gid = format!("antenna-{side}");
        let (bb, a, lk, c) = (
            cs([s * 24.0, 6.0]),
            spec.a.unwrap_or(s * 38.0),
            46.0 * csc,
            spec.curl.unwrap_or(0.3),
        );
        let mut pts: Vec<P> = vec![];
        for i in 0..=5 {
            let t = i as f64 / 5.0;
            let aa = (a + s * c * 60.0 * t) * D;
            let prev = *pts.last().unwrap_or(&bb);
            pts.push(if i > 0 {
                add(prev, mul([aa.sin(), -aa.cos()], lk / 5.0))
            } else {
                bb
            });
        }
        out.line(format!("{gid}-flagellum"), &gid, bp(&pts), "ink", 1.0);
        // antennule: small folded pair in its fossa
        let ab = cs([s * 10.0, 10.0]);
        out.line(
            format!("antennule-{side}"),
            &format!("antennule-{side}"),
            bp(&[ab, add(ab, [s * 3.0, -9.0]), add(ab, [s * 8.0, -4.0])]),
            "ink",
            0.8,
        );
    };
    let mouth = |out: &mut Out| {
        // third maxillipeds: two plates like doors over the mouth
        for side in ['R', 'L'] {
            let s = s_of(side);
            let gid = format!("maxilliped3-{side}");
            let (x0, x1) = (s * 1.5, s * 17.0);
            let plate: Vec<P> = if v2 {
                through(
                    &[
                        [x0, 16.0],
                        [x1 - s * 2.0, 15.0],
                        [x1, 24.0],
                        [x1 - s * 2.0, 36.0],
                        [x0, 38.0],
                    ],
                    3,
                    false,
                )
                .into_iter()
                .map(cs)
                .collect()
            } else {
                vec![
                    [x0, 15.0],
                    [x1, 14.0],
                    [x1 + s * 1.0, 26.0],
                    [x1 - s * 1.0, 38.0],
                    [x0, 39.0],
                ]
            };
            out.push(
                gid.clone(),
                "mouthparts",
                bp(&plate),
                if v2 { "face" } else { "vent" },
            )
            .sw = Some(0.6);
            out.line(
                format!("{gid}-suture"),
                "mouthparts",
                bp(&[cs([x0, 27.0]), cs([x1, 26.0])]),
                "ink",
                0.4,
            );
        }
    };

    // ----- draw order -----
    let leg_order: &[usize] = if back { &[0, 1, 2] } else { &[3, 2, 1, 0] };
    let mut claw_info: [Option<ClawInfo>; 2] = [None, None];
    if back {
        claw_info[0] = Some(cheliped(&mut out, 'R'));
        claw_info[1] = Some(cheliped(&mut out, 'L'));
    }
    for &i in leg_order {
        walk_leg(&mut out, 'R', i);
        walk_leg(&mut out, 'L', i);
    }
    if !back {
        // underside in shadow, then face, carapace
        let und: Vec<P> = ell(0.0, 40.0, 104.0, 16.0, 24, 0.0)
            .into_iter()
            .map(cs)
            .collect();
        out.push(
            "underside-sternum",
            "body",
            bp(&und),
            if v2 { "face" } else { "vent" },
        )
        .shade = true;
        let it = out.push(
            "face-pterygostomial",
            "body",
            bp(&face_pts()),
            if v2 { "face" } else { "vent" },
        );
        if !v2 {
            it.under = true;
        }
        mouth(&mut out);
        if v2 {
            // the carapace brim's soft shadow across the top of the face
            let sh: Vec<P> = through(
                &[
                    [-118.0, 0.0],
                    [-60.0, 12.0],
                    [0.0, 14.0],
                    [60.0, 12.0],
                    [118.0, 0.0],
                    [96.0, 14.0],
                    [40.0, 22.0],
                    [0.0, 23.0],
                    [-40.0, 22.0],
                    [-96.0, 14.0],
                ],
                4,
                true,
            )
            .into_iter()
            .map(cs)
            .collect();
            let it = out.push("face-brim-shadow", "body", bp(&sh), "face");
            it.shade = true;
            it.ink = false;
            it.op = Some(0.8);
        }
        out.push("carapace", "carapace", bp(&carapace_pts()), "shell");
        // grooves (cervical) and branchial bumps, quiet
        out.line(
            "carapace-groove-R",
            "carapace",
            bp(&[cs([-30.0, -40.0]), cs([-46.0, -26.0]), cs([-72.0, -20.0])]),
            "ink",
            0.5,
        );
        out.line(
            "carapace-groove-L",
            "carapace",
            bp(&[cs([30.0, -40.0]), cs([46.0, -26.0]), cs([72.0, -20.0])]),
            "ink",
            0.5,
        );
        if pose.gear.helmet {
            helmet(&mut out, &bp, false);
        }
        eye(&mut out, 'R');
        eye(&mut out, 'L');
        antenna(&mut out, 'R');
        antenna(&mut out, 'L');
        claw_info[1] = Some(cheliped(&mut out, 'L'));
        claw_info[0] = Some(cheliped(&mut out, 'R'));
    } else {
        // abdomen: only the folded base shows under the posterior border
        out.push(
            "underside-sternum",
            "body",
            bp(&ell(0.0, 34.0, 92.0, 14.0, 24, 0.0)),
            "vent",
        )
        .shade = true;
        out.push(
            "abdomen-fold",
            "abdomen",
            bp(&through(
                &[
                    [-20.0, 28.0],
                    [20.0, 28.0],
                    [16.0, 42.0],
                    [0.0, 45.0],
                    [-16.0, 42.0],
                ],
                4,
                true,
            )),
            "vent",
        );
        // eyestalk tips peek above the far edge
        for side in ['R', 'L'] {
            let s = -s_of(side);
            let sp = pose.eyes[si(side)];
            let a = -sp.a.unwrap_or(0.0);
            let tip = [
                s * 40.0 + (a * D).sin() * 50.0,
                -40.0 - (a * D).cos() * 48.0,
            ];
            out.push(
                format!("eye-{side}-stalk"),
                &format!("eye-{side}"),
                bp(&limb(
                    &[[s * 36.0, -30.0], tip],
                    |_| 12.0,
                    LimbOpt::default(),
                )),
                "stalk",
            );
            out.push(
                format!("eye-{side}-cornea"),
                &format!("eye-{side}"),
                bp(&ell(tip[0], tip[1] - 6.0, 12.0, 15.0, 20, a)),
                "cornea",
            );
        }
        out.push("carapace", "carapace", bp(&back_carapace_pts()), "shell");
        out.line(
            "carapace-groove-R",
            "carapace",
            bp(&[[-40.0, -30.0], [-30.0, -16.0], [-26.0, 0.0]]),
            "ink",
            0.45,
        );
        out.line(
            "carapace-groove-L",
            "carapace",
            bp(&[[40.0, -30.0], [30.0, -16.0], [26.0, 0.0]]),
            "ink",
            0.45,
        );
        if pose.gear.helmet {
            helmet(&mut out, &bp, true);
        }
        walk_leg(&mut out, 'R', 3);
        walk_leg(&mut out, 'L', 3);
    }
    // gear held in claws
    if let Lantern::Claw(side) = pose.gear.lantern
        && let Some(ci) = claw_info[si(side)]
    {
        lantern_in_claw(&mut out, &ci);
    }
    if pose.gear.lantern == Lantern::Hang {
        lantern_hung(&mut out, &body_m);
    }
    if let Some(side) = pose.gear.shield
        && let Some(ci) = claw_info[si(side)]
    {
        shield_in_claw(&mut out, &ci);
    }
    out.0
}

// ---------- gear ----------
fn helmet(out: &mut Out, bp: &dyn Fn(&[P]) -> Vec<P>, back: bool) {
    // dented crown bottle cap held on by hooked setae (decorator-crab trait), sitting on the gastric region
    let (y, w) = (if back { -44.0 } else { -50.0 }, 46.0);
    let mut skirt: Vec<P> = (0..=20)
        .map(|i| {
            [
                lerp(-w, w, i as f64 / 20.0),
                y + 6.0 + if i % 2 == 1 { 4.0 } else { 0.0 },
            ]
        })
        .collect();
    skirt.reverse();
    let mut shape: Vec<P> = vec![[-w + 3.0, y - 10.0]];
    shape.extend(
        through(
            &[
                [-w + 3.0, y - 10.0],
                [-20.0, y - 16.0],
                [6.0, y - 13.0],
                [20.0, y - 17.0],
                [w - 3.0, y - 10.0],
            ],
            4,
            false,
        )
        .into_iter()
        .skip(1),
    );
    shape.push([w, y + 4.0]);
    shape.extend(skirt);
    out.push("gear-helmet-bottlecap", "gear-helmet", bp(&shape), "capRed");
    out.push(
        "gear-helmet-top",
        "gear-helmet",
        bp(&ell(0.0, y - 12.0, w - 4.0, 7.0, 24, 0.0)),
        "cap",
    )
    .sw = Some(0.6);
    out.line(
        "gear-helmet-dent",
        "gear-helmet",
        bp(&[[8.0, y - 13.0], [16.0, y - 7.0], [24.0, y - 12.0]]),
        "ink",
        0.6,
    );
}
struct LanternPts {
    jar: Vec<P>,
    lid: Vec<P>,
    glow_body: Vec<P>,
    bail: Vec<P>,
    top: P,
    center: P,
}
fn lantern_pts(c: P, s: f64, tilt: f64) -> LanternPts {
    // a small glass jar (screw neck), bottle-cap lid, fishing-line bail
    let r = |p: P| add(c, rot0(mul(p, s), tilt));
    let m = |v: Vec<P>| v.into_iter().map(r).collect::<Vec<P>>();
    LanternPts {
        jar: m(through(
            &[
                [-14.0, -10.0],
                [-17.0, 0.0],
                [-17.0, 22.0],
                [-14.0, 28.0],
                [14.0, 28.0],
                [17.0, 22.0],
                [17.0, 0.0],
                [14.0, -10.0],
            ],
            4,
            true,
        )),
        lid: m(vec![
            [-15.0, -18.0],
            [15.0, -18.0],
            [16.0, -9.0],
            [-16.0, -9.0],
        ]),
        glow_body: m(through(
            &[
                [-12.0, 2.0],
                [-12.0, 22.0],
                [12.0, 22.0],
                [12.0, 2.0],
                [0.0, -2.0],
            ],
            4,
            true,
        )),
        bail: m(through(
            &[
                [-13.0, -14.0],
                [-10.0, -32.0],
                [0.0, -38.0],
                [10.0, -32.0],
                [13.0, -14.0],
            ],
            5,
            false,
        )),
        top: r([0.0, -38.0]),
        center: r([0.0, 12.0]),
    }
}
fn draw_lantern(out: &mut Out, l: &LanternPts, gid: &str) {
    out.line(
        format!("{gid}-bail-fishingline"),
        gid,
        l.bail.clone(),
        "ink",
        0.7,
    );
    let it = out.push(format!("{gid}-jar-glass"), gid, l.jar.clone(), "jarGlass");
    it.sw = Some(0.8);
    it.op = Some(0.7);
    out.push(format!("{gid}-bacteria"), gid, l.glow_body.clone(), "glow")
        .ink = false;
    out.push(format!("{gid}-lid-bottlecap"), gid, l.lid.clone(), "cap")
        .sw = Some(0.7);
    out.0.push(Item::glow(
        format!("{gid}-light"),
        Some(gid),
        l.center[0],
        l.center[1],
        90.0,
        "#FFD86A",
        None,
    ));
}
fn lantern_in_claw(out: &mut Out, ci: &ClawInfo) {
    // hangs from the closed dactyl/fixed-finger tips by its bail
    let tip = ci.hand(&[[ci.l + ci.f * 0.8, ci.h * 0.14]])[0];
    let l = lantern_pts([tip[0], tip[1] + 38.0], 1.15, 0.0);
    // shift so the bail top sits in the fingertips
    let off = sub(tip, l.top);
    let lm = lantern_pts([tip[0] + off[0], tip[1] + 38.0 + off[1]], 1.15, 0.0);
    draw_lantern(out, &lm, "gear-lantern");
}
fn lantern_hung(out: &mut Out, body_m: &dyn Fn(P) -> P) {
    let hook = body_m([-118.0, -16.0]);
    let l = lantern_pts([hook[0] - 6.0, hook[1] + 66.0], 1.0, 6.0);
    out.line(
        "gear-lantern-tether",
        "gear-lantern",
        vec![hook, add(hook, [-3.0, 12.0]), l.top],
        "ink",
        0.7,
    );
    draw_lantern(out, &l, "gear-lantern");
}
fn shield_in_claw(out: &mut Out, ci: &ClawInfo) {
    // a frosted amber sea-glass shard gripped between the cutter's fingers, held in front of the body
    let c = ci.hand(&[[ci.l + ci.f * 0.45, ci.h * 0.1]])[0];
    let pts: Vec<P> = through(
        &[
            [-4.0, -58.0],
            [8.0, -60.0],
            [34.0, -30.0],
            [36.0, -24.0],
            [30.0, 30.0],
            [24.0, 40.0],
            [-18.0, 50.0],
            [-26.0, 46.0],
            [-38.0, 4.0],
            [-36.0, -8.0],
            [-14.0, -50.0],
        ],
        3,
        true,
    )
    .into_iter()
    .map(|p| add(c, rot0(mul(p, 0.9), -6.0)))
    .collect();
    out.push("gear-shield-seaglass", "gear-shield", pts, "glassAmber")
        .op = Some(0.82);
    out.line(
        "gear-shield-edge",
        "gear-shield",
        vec![
            add(c, [-22.0, -30.0]),
            add(c, [-8.0, 0.0]),
            add(c, [-14.0, 30.0]),
        ],
        "glint",
        1.2,
    );
}
