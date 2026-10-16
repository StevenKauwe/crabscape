//! Small 2D geometry kit for the crab puppet (port of `geom.mjs`). Points are `[x, y]`, y points down.

pub type P = [f64; 2];

pub const TAU: f64 = std::f64::consts::PI * 2.0;
pub const D: f64 = std::f64::consts::PI / 180.0;

pub fn lerp(a: f64, b: f64, t: f64) -> f64 {
    a + (b - a) * t
}
pub fn add(a: P, b: P) -> P {
    [a[0] + b[0], a[1] + b[1]]
}
pub fn sub(a: P, b: P) -> P {
    [a[0] - b[0], a[1] - b[1]]
}
pub fn mul(a: P, k: f64) -> P {
    [a[0] * k, a[1] * k]
}
pub fn len(a: P) -> f64 {
    a[0].hypot(a[1])
}
pub fn norm(a: P) -> P {
    let l = len(a);
    let l = if l == 0.0 || l.is_nan() { 1.0 } else { l };
    [a[0] / l, a[1] / l]
}
pub fn perp(a: P) -> P {
    [-a[1], a[0]]
}
pub fn dir(deg: f64) -> P {
    [(deg * D).cos(), (deg * D).sin()]
}
pub fn rot(p: P, deg: f64, c: P) -> P {
    let a = deg * D;
    let (s, co) = (a.sin(), a.cos());
    let (x, y) = (p[0] - c[0], p[1] - c[1]);
    [c[0] + x * co - y * s, c[1] + x * s + y * co]
}
pub fn rot0(p: P, deg: f64) -> P {
    rot(p, deg, [0.0, 0.0])
}
pub fn lerp_p(a: P, b: P, t: f64) -> P {
    [lerp(a[0], b[0], t), lerp(a[1], b[1], t)]
}

/// A 2D affine transform `[a, b, c, d, e, f]`: x' = a x + c y + e, y' = b x + d y + f.
pub type M = [f64; 6];
pub const I: M = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];
pub fn t_mul(m: M, n: M) -> M {
    [
        m[0] * n[0] + m[2] * n[1],
        m[1] * n[0] + m[3] * n[1],
        m[0] * n[2] + m[2] * n[3],
        m[1] * n[2] + m[3] * n[3],
        m[0] * n[4] + m[2] * n[5] + m[4],
        m[1] * n[4] + m[3] * n[5] + m[5],
    ]
}
pub fn tr(x: f64, y: f64) -> M {
    [1.0, 0.0, 0.0, 1.0, x, y]
}
pub fn sc(s: f64) -> M {
    [s, 0.0, 0.0, s, 0.0, 0.0]
}
pub fn ro(deg: f64) -> M {
    let a = deg * D;
    [a.cos(), a.sin(), -a.sin(), a.cos(), 0.0, 0.0]
}
pub fn ap(m: M, p: P) -> P {
    [
        m[0] * p[0] + m[2] * p[1] + m[4],
        m[1] * p[0] + m[3] * p[1] + m[5],
    ]
}
pub fn chain(ms: &[M]) -> M {
    ms.iter().fold(I, |acc, m| t_mul(acc, *m))
}

/// Catmull-Rom through points, `n` samples per span.
pub fn through(pts: &[P], n: usize, closed: bool) -> Vec<P> {
    let nn = pts.len();
    if nn < 3 {
        return pts.to_vec();
    }
    let get = |i: i64| -> P {
        if closed {
            pts[i.rem_euclid(nn as i64) as usize]
        } else {
            pts[i.clamp(0, nn as i64 - 1) as usize]
        }
    };
    let spans = if closed { nn } else { nn - 1 };
    let mut out = vec![];
    for i in 0..spans as i64 {
        let (p0, p1, p2, p3) = (get(i - 1), get(i), get(i + 1), get(i + 2));
        for k in 0..n {
            let u = k as f64 / n as f64;
            let (u2, u3) = (u * u, u * u * u);
            out.push(std::array::from_fn(|d| {
                0.5 * (2.0 * p1[d]
                    + (p2[d] - p0[d]) * u
                    + (2.0 * p0[d] - 5.0 * p1[d] + 4.0 * p2[d] - p3[d]) * u2
                    + (3.0 * p1[d] - p0[d] - 3.0 * p2[d] + p3[d]) * u3)
            }));
        }
    }
    if !closed {
        out.push(pts[nn - 1]);
    }
    out
}

#[derive(Clone, Copy)]
pub struct LimbOpt {
    pub cap0: bool,
    pub cap1: bool,
    pub nc: usize,
    pub smooth: bool,
}
impl Default for LimbOpt {
    fn default() -> Self {
        LimbOpt {
            cap0: true,
            cap1: true,
            nc: 5,
            smooth: true,
        }
    }
}
/// No round cap at the far end.
pub const OPEN_END: LimbOpt = LimbOpt {
    cap0: true,
    cap1: false,
    nc: 5,
    smooth: true,
};

/// Closed outline around a spine polyline, width given by `wf(t)` (t 0..1), round caps of `nc` points.
pub fn limb(spine: &[P], wf: impl Fn(f64) -> f64, o: LimbOpt) -> Vec<P> {
    let c = if o.smooth && spine.len() > 2 {
        through(spine, 5, false)
    } else {
        resample(spine, 10)
    };
    let n = c.len();
    let (mut l, mut r) = (vec![], vec![]);
    for i in 0..n {
        let (a, b) = (c[i.saturating_sub(1)], c[(i + 1).min(n - 1)]);
        let t = norm(sub(b, a));
        let nn = perp(t);
        let w = wf(i as f64 / (n - 1) as f64) / 2.0;
        l.push(add(c[i], mul(nn, w)));
        r.push(sub(c[i], mul(nn, w)));
    }
    let tan = |i: usize, j: usize| norm(sub(c[j], c[i]));
    let mut out = l;
    if o.cap1 {
        let (cc, w0, t) = (c[n - 1], wf(1.0) / 2.0, tan(n - 2, n - 1));
        let nn = perp(t);
        if w0 > 0.8 {
            for k in 1..o.nc {
                let a = k as f64 / o.nc as f64 * std::f64::consts::PI;
                out.push(add(cc, mul(add(mul(nn, a.cos()), mul(t, a.sin())), w0)));
            }
        }
    }
    out.extend(r.iter().rev());
    if o.cap0 {
        let (cc, w0, t) = (c[0], wf(0.0) / 2.0, tan(0, 1));
        let nn = perp(t);
        if w0 > 0.8 {
            for k in 1..o.nc {
                let a = k as f64 / o.nc as f64 * std::f64::consts::PI;
                out.push(add(cc, mul(add(mul(nn, -a.cos()), mul(t, -a.sin())), w0)));
            }
        }
    }
    out
}
pub fn resample(pts: &[P], n: usize) -> Vec<P> {
    let mut out = vec![];
    for i in 0..pts.len() - 1 {
        for k in 0..n {
            out.push(lerp_p(pts[i], pts[i + 1], k as f64 / n as f64));
        }
    }
    out.push(pts[pts.len() - 1]);
    out
}
pub fn ell(cx: f64, cy: f64, rx: f64, ry: f64, n: usize, r: f64) -> Vec<P> {
    (0..n)
        .map(|i| {
            let a = i as f64 / n as f64 * TAU;
            rot([cx + a.cos() * rx, cy + a.sin() * ry], r, [cx, cy])
        })
        .collect()
}
#[derive(Clone, Copy, Debug)]
pub struct BBox {
    pub x0: f64,
    pub y0: f64,
    pub x1: f64,
    pub y1: f64,
    pub w: f64,
    pub h: f64,
    pub cx: f64,
    pub cy: f64,
}
pub fn bbox<'a>(pts: impl IntoIterator<Item = &'a P>) -> BBox {
    let (mut x0, mut y0, mut x1, mut y1) = (
        f64::INFINITY,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::NEG_INFINITY,
    );
    for &[x, y] in pts {
        if x < x0 {
            x0 = x;
        }
        if y < y0 {
            y0 = y;
        }
        if x > x1 {
            x1 = x;
        }
        if y > y1 {
            y1 = y;
        }
    }
    BBox {
        x0,
        y0,
        x1,
        y1,
        w: x1 - x0,
        h: y1 - y0,
        cx: (x0 + x1) / 2.0,
        cy: (y0 + y1) / 2.0,
    }
}

pub struct Ik {
    pub joint: P,
    pub end: P,
}
/// Two-bone IK: from `a` to target `t` with bone lengths l1, l2; pick the joint solution scored highest by `pick`.
pub fn ik2(a: P, t: P, l1: f64, l2: f64, pick: impl Fn(P) -> f64) -> Ik {
    let mut d = sub(t, a);
    let mut dl = len(d);
    let (max_r, min_r) = (l1 + l2 - 0.01, (l1 - l2).abs() + 0.01);
    if dl > max_r {
        d = mul(norm(d), max_r);
        dl = max_r;
    }
    if dl < min_r {
        d = mul(norm(d), min_r);
        dl = min_r;
    }
    let cos_a = (l1 * l1 + dl * dl - l2 * l2) / (2.0 * l1 * dl);
    let aa = cos_a.clamp(-1.0, 1.0).acos();
    let base = d[1].atan2(d[0]);
    let j1 = add(a, [(base + aa).cos() * l1, (base + aa).sin() * l1]);
    let j2 = add(a, [(base - aa).cos() * l1, (base - aa).sin() * l1]);
    Ik {
        joint: if pick(j1) >= pick(j2) { j1 } else { j2 },
        end: add(a, d),
    }
}

/// Deterministic string hash (FNV-1a, as `hashStr`).
pub fn hash_str(s: &str) -> u32 {
    let mut h: u32 = 2166136261;
    for c in s.encode_utf16() {
        h = (h ^ c as u32).wrapping_mul(16777619);
    }
    h
}
/// The JS kit's small seeded random numbers in [0, 1) (as `rng`).
pub fn rng(seed: u32) -> impl FnMut() -> f64 {
    let mut s = if seed == 0 { 1 } else { seed };
    move || {
        s = (s ^ (s >> 15))
            .wrapping_mul(2246822507)
            .wrapping_add(0x6D2B79F5);
        s ^= s >> 13;
        (s % 100000) as f64 / 100000.0
    }
}
