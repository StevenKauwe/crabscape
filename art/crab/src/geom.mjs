// Small 2D geometry kit for the crab puppet. Points are [x, y], y points down (screen space).
export const TAU = Math.PI * 2;
export const D = Math.PI / 180;
export const lerp = (a, b, t) => a + (b - a) * t;
export const add = (a, b) => [a[0] + b[0], a[1] + b[1]];
export const sub = (a, b) => [a[0] - b[0], a[1] - b[1]];
export const mul = (a, k) => [a[0] * k, a[1] * k];
export const len = (a) => Math.hypot(a[0], a[1]);
export const norm = (a) => { const l = len(a) || 1; return [a[0] / l, a[1] / l]; };
export const perp = (a) => [-a[1], a[0]];
export const dir = (deg) => [Math.cos(deg * D), Math.sin(deg * D)];
export const rot = (p, deg, c = [0, 0]) => {
  const a = deg * D, s = Math.sin(a), co = Math.cos(a), x = p[0] - c[0], y = p[1] - c[1];
  return [c[0] + x * co - y * s, c[1] + x * s + y * co];
};
export const lerpP = (a, b, t) => [lerp(a[0], b[0], t), lerp(a[1], b[1], t)];

// A 2D affine transform [a, b, c, d, e, f]: x' = a x + c y + e, y' = b x + d y + f
export const I = [1, 0, 0, 1, 0, 0];
export const T = {
  mul: (m, n) => [m[0] * n[0] + m[2] * n[1], m[1] * n[0] + m[3] * n[1], m[0] * n[2] + m[2] * n[3], m[1] * n[2] + m[3] * n[3],
    m[0] * n[4] + m[2] * n[5] + m[4], m[1] * n[4] + m[3] * n[5] + m[5]],
  tr: (x, y) => [1, 0, 0, 1, x, y],
  sc: (sx, sy = sx) => [sx, 0, 0, sy, 0, 0],
  ro: (deg) => { const a = deg * D; return [Math.cos(a), Math.sin(a), -Math.sin(a), Math.cos(a), 0, 0]; },
  ap: (m, p) => [m[0] * p[0] + m[2] * p[1] + m[4], m[1] * p[0] + m[3] * p[1] + m[5]],
};
export const chain = (...ms) => ms.reduce((acc, m) => T.mul(acc, m), I);

// Catmull-Rom through points
export function through(P, n = 6, closed = false) {
  const N = P.length; if (N < 3) return P.slice();
  const out = [], get = (i) => closed ? P[(i + N) % N] : P[Math.max(0, Math.min(N - 1, i))];
  const spans = closed ? N : N - 1;
  for (let i = 0; i < spans; i++) {
    const p0 = get(i - 1), p1 = get(i), p2 = get(i + 1), p3 = get(i + 2);
    for (let k = 0; k < n; k++) {
      const u = k / n, u2 = u * u, u3 = u2 * u;
      out.push([0, 1].map((d) => .5 * (2 * p1[d] + (p2[d] - p0[d]) * u + (2 * p0[d] - 5 * p1[d] + 4 * p2[d] - p3[d]) * u2 + (3 * p1[d] - p0[d] - 3 * p2[d] + p3[d]) * u3)));
    }
  }
  if (!closed) out.push(P[N - 1]);
  return out;
}

// Closed outline around a spine polyline, width given by wf(t) (t 0..1), round caps of nc points.
export function limb(spine, wf, { cap0 = true, cap1 = true, nc = 5, smooth = true } = {}) {
  const C = smooth && spine.length > 2 ? through(spine, 5) : resample(spine, 10);
  const n = C.length, L = [], R = [];
  for (let i = 0; i < n; i++) {
    const a = C[Math.max(0, i - 1)], b = C[Math.min(n - 1, i + 1)], t = norm(sub(b, a)), nn = perp(t), w = wf(i / (n - 1)) / 2;
    L.push(add(C[i], mul(nn, w))); R.push(sub(C[i], mul(nn, w)));
  }
  const tan = (i, j) => norm(sub(C[j], C[i]));
  const out = [...L];
  if (cap1) { const c = C[n - 1], w0 = wf(1) / 2, t = tan(n - 2, n - 1), nn = perp(t);
    if (w0 > .8) for (let k = 1; k < nc; k++) { const a = k / nc * Math.PI; out.push(add(c, mul(add(mul(nn, Math.cos(a)), mul(t, Math.sin(a))), w0))); } }
  out.push(...R.slice().reverse());
  if (cap0) { const c = C[0], w0 = wf(0) / 2, t = tan(0, 1), nn = perp(t);
    if (w0 > .8) for (let k = 1; k < nc; k++) { const a = k / nc * Math.PI; out.push(add(c, mul(add(mul(nn, -Math.cos(a)), mul(t, -Math.sin(a))), w0))); } }
  return out;
}
export function resample(P, n) {
  const out = []; for (let i = 0; i < P.length - 1; i++) for (let k = 0; k < n; k++) out.push(lerpP(P[i], P[i + 1], k / n));
  out.push(P[P.length - 1]); return out;
}
export function ell(cx, cy, rx, ry, n = 28, r = 0) {
  const out = []; for (let i = 0; i < n; i++) { const a = i / n * TAU; out.push(rot([cx + Math.cos(a) * rx, cy + Math.sin(a) * ry], r, [cx, cy])); }
  return out;
}
export function bbox(P) {
  let x0 = Infinity, y0 = Infinity, x1 = -Infinity, y1 = -Infinity;
  for (const [x, y] of P) { if (x < x0) x0 = x; if (y < y0) y0 = y; if (x > x1) x1 = x; if (y > y1) y1 = y; }
  return { x0, y0, x1, y1, w: x1 - x0, h: y1 - y0, cx: (x0 + x1) / 2, cy: (y0 + y1) / 2 };
}
// clip polygon to half-plane n·p <= c  (Sutherland–Hodgman)
export function clipHalf(P, n, c) {
  const out = [], f = (p) => n[0] * p[0] + n[1] * p[1] - c;
  for (let i = 0; i < P.length; i++) {
    const a = P[i], b = P[(i + 1) % P.length], fa = f(a), fb = f(b);
    if (fa <= 0) out.push(a);
    if ((fa <= 0) !== (fb <= 0)) { const t = fa / (fa - fb); out.push(lerpP(a, b, t)); }
  }
  return out;
}
export function shrink(P, k) { const b = bbox(P); return P.map(([x, y]) => [b.cx + (x - b.cx) * k, b.cy + (y - b.cy) * k]); }

// Two-bone IK: from a to target t with bone lengths l1, l2; pick the joint solution scored highest by pick(j).
export function ik2(a, t, l1, l2, pick) {
  let d = sub(t, a), dl = len(d);
  const maxR = l1 + l2 - .01, minR = Math.abs(l1 - l2) + .01;
  if (dl > maxR) { d = mul(norm(d), maxR); dl = maxR; }
  if (dl < minR) { d = mul(norm(d), minR); dl = minR; }
  const cosA = (l1 * l1 + dl * dl - l2 * l2) / (2 * l1 * dl), A = Math.acos(Math.max(-1, Math.min(1, cosA)));
  const base = Math.atan2(d[1], d[0]);
  const j1 = add(a, [Math.cos(base + A) * l1, Math.sin(base + A) * l1]);
  const j2 = add(a, [Math.cos(base - A) * l1, Math.sin(base - A) * l1]);
  const end = add(a, d);
  return { joint: pick(j1) >= pick(j2) ? j1 : j2, end };
}

// deterministic hash noise
export function hashStr(s) { let h = 2166136261; for (const c of s) h = Math.imul(h ^ c.charCodeAt(0), 16777619); return h >>> 0; }
export function rng(seed) { let s = seed || 1; return () => { s = Math.imul(s ^ (s >>> 15), 2246822507) + 0x6D2B79F5 | 0; s ^= s >>> 13; return ((s >>> 0) % 100000) / 100000; }; }
