// The CrabScape player crab as a cut-out puppet (v0.2 'charm pass'; v0.1 is frozen in crab-v0.1.mjs). Each builder returns draw-ordered items in crab space
// (units ≈ px at scale 1; carapace about 290 wide spine-to-spine; y down; ground at y = 100 in the neutral stance).
//
// Sides use the CRAB's own left/right: R = crab's right (screen LEFT in the front view), L = crab's left.
// Heterochely: the crusher claw is on the crab's right (R), the cutter on the left (L).
//
// Item: { k: 'shape', id, g, pts, mat, sw?, ink? } | { k: 'line', id, g, pts, mat, sw } | { k: 'glow', x, y, r, col }
import { add, sub, mul, norm, perp, dir, rot, lerp, lerpP, limb, ell, ik2, through, len, D } from './geom.mjs';

const S = (side) => (side === 'L' ? 1 : -1); // screen-x sign in the front view

// ---------- design proportions (v0.2 charm pass; DESIGN_V01 reproduces v0.1's proportions where they differ) ----------
export const DESIGN = {
  cara: 1.12,      // carapace (and everything on it) relative to the legs
  eye: 1.45,       // cornea size
  stalk: 1.16,     // eyestalk length
  leg: .9,         // walking-leg thickness
  crusher: { L: 90, h: 74, F: 44 }, cutter: { L: 74, h: 42, F: 58 },
  frontTeeth: .35, // frontal-tooth depth (between the orbits)
};
export const DESIGN_V01 = { cara: 1, eye: 1, stalk: 1, leg: 1, crusher: { L: 84, h: 60, F: 46 }, cutter: { L: 70, h: 40, F: 50 }, frontTeeth: 1 };

// ---------- default pose ----------
export function basePose() {
  return {
    view: 'front',
    design: DESIGN,
    body: { dx: 0, dy: 0, rot: 0, sx: 1, sy: 1 },
    eyes: { R: { a: -17, fold: 0, len: 1 }, L: { a: 7, fold: 0, len: 1.03 } },      // a: deg from vertical (+ = toward screen right)
    ant: { R: { a: -38, curl: .3 }, L: { a: 38, curl: .3 } },                  // antennae angles from vertical
    claws: {
      // hero: the heavy crusher rests low across the front like a fist; the slim cutter is held up beside the face
      R: { wrist: [-116, 70], aim: -6, gape: 4, size: 1 },             // wrist in body space; aim: deg up from 'pointing to midline'
      L: { wrist: [148, 44], aim: 66, gape: 18, size: 1 },
    },
    // feet in WORLD space (ground). up: lift for walk. paddles: dactyl tip target + blade angle
    legs: {
      R: [{ f: [-200, 104] }, { f: [-236, 98] }, { f: [-262, 90] }, { f: [-176, -78], blade: -60 }],
      L: [{ f: [200, 104] }, { f: [236, 98] }, { f: [262, 90] }, { f: [176, -78], blade: 60 }],
    },
    gear: {},           // { helmet: true, lantern: 'L' | 'hang', shield: 'L' }
    regrowR: 0,         // 1 = crusher replaced by a small regrowing claw
  };
}
export const clone = (o) => JSON.parse(JSON.stringify(o));

// ---------- materials (resolved per tile by palette) ----------
// shell, leg, vent (pale underside), stalk, cornea, glint, tip (dark fingertips), ink, plus gear mats
export const PALETTES = {
  normal: {
    shell: ['#F07C4E', '#C9452C', '#7C2418'], leg: ['#F38A58', '#D05634', '#85301C'], vent: ['#F9D4AE', '#E7A37A', '#A6634A'],
    stalk: ['#F7C494', '#E39466', '#9E5A3C'], cornea: ['#4A3550', '#22172A', '#120A16'], glint: ['#FFF8EC', '#FFF3DE', '#EFE2C8'],
    tip: ['#5A3A30', '#2E1C17', '#1A0F0C'], ink: '#2A140F', shadow: '#2C4950',
    cap: ['#F2D27A', '#C8962E', '#6E4A16'], capRed: ['#E85A48', '#B42D24', '#5E1610'], glassAmber: ['#FBD58A', '#D9973A', '#8A5418'],
    jarGlass: ['#EAF2EC', '#C8D8D2', '#8BA19C'], glow: ['#FFF3B0', '#FFD45E', '#D89A22'], line: '#CFC7B2',
    // v0.2: the face sits in the shade of the carapace brim (warm, not pale); pale cream undersides on the claws
    face: ['#D98A62', '#B9673F', '#7A3A24'], belly: ['#FCE2BC', '#F2BC8C', '#C27F55'], corneaRim: ['#8A6A9A', '#5B4470', '#2E2040'],
  },
};
function lighten(hex, k, toward = [255, 248, 236]) {
  const n = parseInt(hex.slice(1), 16), c = [n >> 16, (n >> 8) & 255, n & 255].map((v, i) => Math.round(lerp(v, toward[i], k)));
  return '#' + c.map((v) => v.toString(16).padStart(2, '0')).join('');
}
const mapPal = (P, f, keys) => { const o = { ...P }; for (const k of keys) o[k] = P[k].map(f); return o; };
// freshly moulted: soft, pale (artistic reading, see anatomy-notes)
PALETTES.soft = { ...mapPal(PALETTES.normal, (c) => lighten(c, .5), ['shell', 'leg', 'vent', 'stalk', 'face', 'belly']), ink: '#8A5A48' };
// the same red crab seen at ~150 m: no red light left, so the red pigment reflects almost nothing (NOAA)
const deepen = (c) => { const n = parseInt(c.slice(1), 16), r = n >> 16, g = (n >> 8) & 255, b = n & 255; const y = .05 * r + .5 * g + .45 * b; const o = [y * .25, y * .55, y * .7].map((v) => Math.round(Math.min(255, v * .6))); return '#' + o.map((v) => v.toString(16).padStart(2, '0')).join(''); };
PALETTES.deep = { ...mapPal(PALETTES.normal, deepen, ['shell', 'leg', 'vent', 'stalk', 'tip', 'cornea', 'face', 'belly', 'corneaRim']), ink: '#071216' };
PALETTES.mid = { ...mapPal(PALETTES.normal, (c) => { const n = parseInt(c.slice(1), 16), r = n >> 16, g = (n >> 8) & 255, b = n & 255; const o = [r * .45, g * .75, b * .95 + 10]; return '#' + o.map((v) => Math.round(Math.min(255, v)).toString(16).padStart(2, '0')).join(''); }, ['shell', 'leg', 'vent', 'stalk', 'tip', 'face', 'belly']) };

// ---------- the front / back view puppet ----------
export function buildFront(pose, opt = {}) {
  const back = pose.view === 'back';
  const out = [];
  const B = pose.body;
  const DS = { ...DESIGN_V01, ...(pose.design || {}) };
  const CS = DS.cara; // carapace scale about its centre (0, -6)
  const cs = (p) => [p[0] * CS, (p[1] + 6) * CS - 6];
  const bodyM = (p) => { // body-local → world
    let q = [p[0] * B.sx, p[1] * B.sy]; q = rot(q, B.rot); return [q[0] + B.dx, q[1] + B.dy];
  };
  const bp = (P) => P.map(bodyM);
  const push = (id, g, pts, mat, extra = {}) => out.push({ k: 'shape', id, g, pts, mat, ...extra });
  const line = (id, g, pts, mat, sw) => out.push({ k: 'line', id, g, pts, mat, sw });
  // in the back view the crab's right is on screen RIGHT
  const sx = (side) => (back ? -S(side) : S(side));

  // ----- walking legs -----
  const legDims = [
    { hip: [78, 26], m: 76, c: 30, p: 44, d: 36 },
    { hip: [94, 18], m: 82, c: 32, p: 46, d: 38 },
    { hip: [106, 10], m: 80, c: 31, p: 44, d: 36 },
    { hip: [112, -2], m: 60, c: 26, p: 30, d: 40, paddle: true },
  ];
  function walkLeg(side, i) {
    const s = sx(side), Ld = legDims[i], spec = pose.legs[side][i];
    const hip = bodyM([s * Ld.hip[0], Ld.hip[1]]);
    let f = spec.f.slice(); if (back) f[0] = -f[0];
    const gid = `leg-${side}${i + 1}`;
    if (!Ld.paddle) {
      const dd = norm([s * (spec.dout ?? .32), 1]);
      const P = sub(f, mul(dd, Ld.d));
      const l2 = Ld.c + Ld.p - 4;
      const { joint: K, end } = ik2(hip, P, Ld.m, l2, (j) => -j[1] * 1 + 0.15 * s * j[0]); // knee up
      const C = add(K, add(mul(norm(sub(end, K)), Ld.c), mul([s, 0], 3))); // carpus end, slight outward kink
      const Pend = end, F = add(Pend, mul(dd, Ld.d));
      const W = (i === 1 ? 1.06 : 1) * DS.leg;
      const dp = DS.eye > 1 && !back ? { far: i === 2, mid: i === 1 } : {};
      push(`${gid}-merus`, gid, limb([hip, lerpP(hip, K, .5), K], (t) => (21 - 4 * t + 3 * Math.sin(t * Math.PI)) * W), 'leg', { pv: hip, ...dp });
      push(`${gid}-carpus`, gid, limb([K, C], (t) => 16 * W - t * 1.5), 'leg', { pv: K, ...dp });
      push(`${gid}-propodus`, gid, limb([C, Pend], (t) => (14 - 4 * t) * W), 'leg', { pv: C, ...dp });
      push(`${gid}-dactyl`, gid, limb([Pend, lerpP(Pend, F, .55), F], (t) => (10.5 * (1 - t) + 1.2) * W, { cap1: false }), 'leg', { pv: Pend, ...dp });
    } else {
      // swimming paddle (5th pereiopod): propodus + dactyl flattened into an oval blade
      const blade = (spec.blade ?? 70) * (back ? -1 : 1);
      const bd = dir(blade - 90 + 0); // blade direction (angle from vertical)
      const bdir = [Math.sin(blade * D), -Math.cos(blade * D)];
      const P = sub(f, mul(bdir, Ld.d));
      const { joint: K, end } = ik2(hip, P, Ld.m, Ld.c + Ld.p - 4, (j) => s * j[0] * 1 + j[1] * .3);
      const C = add(K, mul(norm(sub(end, K)), Ld.c));
      push(`${gid}-merus`, gid, limb([hip, lerpP(hip, K, .5), K], (t) => 18 - 3 * t), 'leg', { pv: hip });
      push(`${gid}-carpus`, gid, limb([K, C], () => 14), 'leg', { pv: K });
      push(`${gid}-propodus`, gid, limb([C, end], (t) => 15 + 5 * t), 'leg', { pv: C });
      // blade: lens shape along bdir from end to f
      const mid = lerpP(end, f, .5), half = Ld.d / 2 + 4, wid = spec.bladeW ?? 13;
      const n = perp(bdir), pts = [];
      for (let k = 0; k <= 16; k++) { const a = k / 16 * Math.PI; pts.push(add(mid, add(mul(bdir, -Math.cos(a) * half), mul(n, Math.sin(a) * wid)))); }
      for (let k = 1; k < 16; k++) { const a = k / 16 * Math.PI; pts.push(add(mid, add(mul(bdir, Math.cos(a) * half), mul(n, -Math.sin(a) * wid * .8)))); }
      push(`${gid}-dactyl-paddle`, gid, pts, 'leg', { pv: end });
      line(`${gid}-paddle-rib`, gid, [lerpP(end, f, .12), lerpP(end, f, .5), lerpP(end, f, .85)], 'ink', .5);
    }
  }

  // ----- chelipeds -----
  function cheliped(side) {
    const s = sx(side), spec = pose.claws[side];
    const crusher = side === 'R' && !pose.regrowR;
    const small = side === 'R' && pose.regrowR;
    const gid = `cheliped-${side}`;
    const coxa = bodyM([s * 60, 30]);
    let wr = spec.wrist.slice(); if (back) wr[0] = -wr[0];
    const W = bodyM(wr);
    const kM = small ? .62 : 1;
    const ml = 50 * kM, cl = 28 * kM;
    const { joint: E } = ik2(coxa, W, ml, cl, (j) => s * j[0] + j[1] * .4); // elbow outward/down
    const mw = (crusher ? 28 + 6 * (DS.crusher.h / 60 - 1) * 3 : 24) * kM;
    push(`${gid}-merus`, gid, limb([coxa, lerpP(coxa, E, .5), E], (t) => mw - 4 * t + 2 * Math.sin(t * Math.PI)), 'leg', { pv: coxa });
    // merus spines on the upper edge
    const md = norm(sub(E, coxa)), mn = mul(perp(md), md[0] * s > 0 ? -1 : 1);
    for (const t of [.35, .65]) { const b = lerpP(coxa, E, t), up = [0, -1]; const nrm = mn[1] < 0 ? mn : mul(mn, -1); push(`${gid}-merus-spine${t}`, gid, [add(b, mul(md, -4 * kM)), add(b, mul(nrm, mw * .5 + 3.5 * kM)), add(b, mul(md, 4 * kM))], 'leg', { sw: .6 }); }
    // carpus (wrist) knob with inner carpal spine
    const cd = norm(sub(W, E));
    const cw = crusher ? 24 + (DS.crusher.h - 60) * .4 : 24;
    push(`${gid}-carpus`, gid, limb([E, lerpP(E, W, .5), W], (t) => (cw + 6 * Math.sin(t * Math.PI)) * kM), 'leg', { pv: E });
    const cs = lerpP(E, W, .7), cn = perp(cd), cnU = cn[1] < 0 ? cn : mul(cn, -1);
    push(`${gid}-carpus-spine`, gid, [add(cs, mul(cd, -5 * kM)), add(cs, add(mul(cnU, 18 * kM), mul(cd, 6 * kM))), add(cs, mul(cd, 6 * kM))], 'leg', { sw: .6 });
    // hand: built pointing +x with the FIXED finger below (+y); mirrored for the other side
    const dims = crusher ? DS.crusher : small ? { L: 38, h: 22, F: 26 } : DS.cutter;
    const k = spec.size ?? 1; const L = dims.L * k, h = dims.h * k, F = dims.F * k;
    // which way the fingers point: toward the midline in screen space, then raised by aim
    const inward = W[0] < (B.dx) ? 1 : -1; // hand on the left of the body points right
    const flip = spec.flip ?? (s < 0 ? 1 : -1) * (back ? -1 : 1);
    const aim = spec.aim ?? 0;
    const toW = (p) => { // hand-local → world
      let q = [p[0], p[1]];
      if (flip < 0) q = [-q[0], q[1]];
      q = rot(q, flip > 0 ? -aim + B.rot : aim + B.rot);
      return add(W, q);
    };
    const hp = (P) => P.map(toW);
    // fixed finger (part of the propodus) — drawn first, palm overlaps its root
    const ff = [[L * .78, h * .22], [L + F * .5, h * .30], [L + F, h * .12]];
    const fixed = limb(ff, (t) => h * .42 * (1 - t) + 3, { cap1: false });
    const fixedTip = limb([lerpP(ff[1], ff[2], .35), ff[2]], (t) => h * .42 * (1 - (.55 + .45 * t)) + 3, { cap1: false, smooth: false });
    // palm
    const palm = through([[-4, 0], [2, -h * .40], [L * .45, -h * .54], [L * .92, -h * .44], [L * 1.04, -h * .12], [L * .98, h * .30], [L * .6, h * .5], [L * .1, h * .44]], 5, true);
    // dactyl: pivot near the top-front of the palm, closes down onto the fixed finger
    const piv = [L * .9, -h * .26], g = (spec.gape ?? 0);
    const dsp = [[0, 0], [F * .55 + L * .05, -h * .06], [F + L * .1, h * .34]].map((p) => add(piv, rot(p, -g)));
    const dact = limb(dsp, (t) => h * .40 * (1 - t) + 3, { cap1: false });
    const dTip = limb([lerpP(dsp[1], dsp[2], .35), dsp[2]], (t) => h * .40 * (1 - (.55 + .45 * t)) + 3, { cap1: false, smooth: false });
    push(`${gid}-propodus-fixedfinger`, `${gid}-propodus`, hp(fixed), 'leg');
    push(`${gid}-propodus-fixedfinger-tip`, `${gid}-propodus`, hp(fixedTip), 'tip', { sw: .5 });
    if (crusher) { // molariform teeth on the crusher's fixed finger
      for (const t of [.45, .8]) { const p = lerpP(ff[0], ff[1], t); push(`${gid}-molar${t}`, `${gid}-propodus`, hp(ell(p[0] + 4, p[1] - h * .1, 4.2 * k, 3.2 * k, 10)), 'vent', { sw: .45 }); }
    }
    push(`${gid}-propodus-palm`, `${gid}-propodus`, hp(palm), 'leg', { pv: W });
    // pale underside of the hand (the chela's ventral face shows a little in front view), v0.2 only
    if (DS.eye > 1) push(`${gid}-palm-belly`, `${gid}-propodus`, hp(through([[L * .14, h * .40], [L * .5, h * .26], [L * .9, h * .2], [L * .97, h * .3], [L * .6, h * .45], [L * .16, h * .41]], 4, true)), 'belly', { ink: null });
    // palm ridge line (portunid hands are ridged)
    line(`${gid}-palm-ridge`, `${gid}-propodus`, hp([[L * .12, -h * .1], [L * .5, -h * .2], [L * .86, -h * .12]]), 'ink', .55);
    push(`${gid}-dactyl`, `${gid}-dactyl`, hp(dact), 'leg', { pv: toW(piv) });
    push(`${gid}-dactyl-tip`, `${gid}-dactyl`, hp(dTip), 'tip', { sw: .5 });
    return { W, toW, L, h, F, hand: hp };
  }

  // ----- carapace -----
  const SPX = 146;
  function carapacePts() {
    // spine tips, back edge, anterolateral teeth, orbits, frontal teeth
    const P = [];
    const backEdge = through([[-124, -18], [-96, -38], [-50, -51], [0, -55], [50, -51], [96, -38], [124, -18]], 4);
    P.push([-SPX, -12]); P.push(...backEdge); P.push([SPX, -12]); P.push([126, -2]);
    // left anterolateral margin with 6 teeth, from spine base to orbit
    const al = (sgn) => {
      const pts = [], A = [126 * sgn, -2], Bp = [44 * sgn, 7], Cc = [96 * sgn, 10];
      const q = (t) => [lerp(lerp(A[0], Cc[0], t), lerp(Cc[0], Bp[0], t), t), lerp(lerp(A[1], Cc[1], t), lerp(Cc[1], Bp[1], t), t)];
      const N = 6;
      for (let i = 0; i < N; i++) {
        const t0 = i / N, t1 = (i + .55) / N, a = q(t0), b = q(t1), tg = norm(sub(q(t0 + .02), a));
        const amp = DS.eye > 1 ? 1.25 - .75 * i / (N - 1) : 1;
        pts.push(a); const tip = add(lerpP(a, b, .8), DS.eye > 1 ? [sgn * 6 * amp, -2.6 * amp] : [sgn * 5, 1.2]); pts.push(tip);
      }
      pts.push(Bp);
      return pts;
    };
    P.push(...al(1));
    // left orbit notch, frontal teeth (4), right orbit notch
    P.push([40, 4], [34, 3], [30, 7]);
    const ft = DS.frontTeeth, fy = (y) => 10 + (y - 10) * ft; // frontal teeth depth
    P.push([24, 9], [19, fy(12.5)], [13, 10], [6, fy(12.5)], [0, 10], [-6, fy(12.5)], [-13, 10], [-19, fy(12.5)], [-24, 9]);
    P.push([-30, 7], [-34, 3], [-40, 4]);
    P.push(...al(-1).reverse());
    P.push([-126, -2]);
    return P.map(cs);
  }
  function backCarapacePts() {
    // seen from behind and slightly above: posterior border nearest (low), toothed anterolateral edge is the far skyline
    const P = [];
    const sky = through([[-124, -6], [-100, -30], [-60, -46], [0, -52], [60, -46], [100, -30], [124, -6]], 4);
    // add teeth bumps to the skyline
    const toothy = sky.map((p, i) => (i % 3 === 1 && Math.abs(p[0]) > 40 ? [p[0], p[1] - 4] : p));
    P.push([-SPX, -8], ...toothy, [SPX, -8]);
    P.push(...through([[126, 0], [96, 14], [56, 24], [36, 30], [0, 31], [-36, 30], [-56, 24], [-96, 14], [-126, 0]], 4));
    return P.map(cs);
  }
  function facePts() {
    return through([[-128, -4], [-96, 6], [-44, 8], [0, 9], [44, 8], [96, 6], [128, -4], [112, 14], [76, 30], [36, 40], [0, 42], [-36, 40], [-76, 30], [-112, 14]], 4, true).map(cs);
  }

  // ----- eyes -----
  function eye(side) {
    const s = S(side), spec = pose.eyes[side], gid = `eye-${side}`;
    const base = cs([s * 36, 2]), E = DS.eye, v2 = E > 1;
    const fold = spec.fold ?? 0;
    // folded: the stalk lies outward along the front margin in its orbit groove
    const a = lerp(spec.a ?? 0, s * 96, fold);
    const Lk = 58 * DS.stalk * (spec.len ?? 1) * lerp(1, .72, fold);
    const d = [Math.sin(a * D), -Math.cos(a * D)];
    const tip = add(base, mul(d, Lk));
    const mid = add(lerpP(base, tip, .5), mul(perp(d), s * 3 * (1 - fold)));
    const stalk = limb([base, mid, tip], v2 ? (t) => 13 - 3 * t : (t) => 13 - 2 * t);
    push(`${gid}-stalk`, gid, bp(stalk), 'stalk', { pv: bodyM(base) });
    // eye bulb: stalk end + dark cornea cap
    const ec = add(tip, mul(d, 6 * E));
    const bulb = ell(ec[0], ec[1], 14.5 * E, 17.5 * E, 28, a);
    push(`${gid}-cornea`, gid, bp(bulb), 'cornea');
    if (v2) {
      // glossy: light bouncing up off the floor makes a soft crescent low on the cornea, a big key glint up top
      const rimC = add(ec, rot([1.5 * E, 6 * E], a));
      push(`${gid}-cornea-rim`, gid, bp(ell(rimC[0], rimC[1], 10.5 * E, 9 * E, 20, a)).filter((q, i, arr) => true), 'corneaRim', { ink: null, op: .55 });
      push(`${gid}-cornea-core`, gid, bp(ell(ec[0] - .5 * E, ec[1] - 1.8 * E, 12 * E, 14 * E, 24, a)), 'cornea', { ink: null });
    }
    const g = add(ec, rot(v2 ? [-5.2 * E, -6.8 * E] : [-5, -6.5], a));
    push(`${gid}-glint`, gid, bp(ell(g[0], g[1], 3.8 * (v2 ? 1.25 * E : 1), 4.8 * (v2 ? 1.2 * E : 1), 14, a + (v2 ? -20 : 0))), 'glint', { ink: null });
    const g2 = add(ec, rot(v2 ? [5.5 * E, 6.5 * E] : [5, 6], a));
    push(`${gid}-glint2`, gid, bp(ell(g2[0], g2[1], 1.6 * (v2 ? 1.3 * E : 1), 1.8 * (v2 ? 1.3 * E : 1), 10, a)), 'glint', { ink: null, op: .7 });
  }
  function antenna(side) {
    const s = S(side), spec = pose.ant[side], gid = `antenna-${side}`;
    const b = cs([s * 24, 6]), a = spec.a ?? s * 38, Lk = 46 * CS, c = spec.curl ?? .3;
    const pts = []; for (let i = 0; i <= 5; i++) { const t = i / 5, aa = (a + s * c * 60 * t) * D; const prev = pts.length ? pts[pts.length - 1] : b; pts.push(i ? add(prev, mul([Math.sin(aa), -Math.cos(aa)], Lk / 5)) : b); }
    line(`${gid}-flagellum`, gid, bp(pts), 'ink', 1.0);
    // antennule: small folded pair in its fossa
    const ab = cs([s * 10, 10]);
    line(`antennule-${side}`, `antennule-${side}`, bp([ab, add(ab, [s * 3, -9]), add(ab, [s * 8, -4])]), 'ink', .8);
  }
  function mouth() {
    // third maxillipeds: two plates like doors over the mouth
    for (const side of ['R', 'L']) {
      const s = S(side), gid = `maxilliped3-${side}`;
      const x0 = s * 1.5, x1 = s * 17;
      const plate = DS.eye > 1 ? through([[x0, 16], [x1 - s * 2, 15], [x1, 24], [x1 - s * 2, 36], [x0, 38]], 3).map(cs) : [[x0, 15], [x1, 14], [x1 + s * 1, 26], [x1 - s * 1, 38], [x0, 39]];
      push(`${gid}`, 'mouthparts', bp(plate), DS.eye > 1 ? 'face' : 'vent', { sw: .6 });
      line(`${gid}-suture`, 'mouthparts', bp([[x0, 27], [x1, 26]].map(cs)), 'ink', .4);
    }
  }

  // ----- draw order -----
  const legOrder = back ? [0, 1, 2] : [3, 2, 1, 0];
  let clawInfo = {};
  if (back) { clawInfo.R = cheliped('R'); clawInfo.L = cheliped('L'); }
  for (const i of legOrder) { walkLeg('R', i); walkLeg('L', i); }
  if (!back) {
    // underside in shadow, then face, carapace
    push('underside-sternum', 'body', bp(ell(0, 40, 104, 16, 24).map(cs)), DS.eye > 1 ? 'face' : 'vent', { shade: true });
    push('face-pterygostomial', 'body', bp(facePts()), DS.eye > 1 ? 'face' : 'vent', DS.eye > 1 ? {} : { under: true });
    mouth();
    if (DS.eye > 1) { // the carapace brim's soft shadow across the top of the face
      const sh = through([[-118, 0], [-60, 12], [0, 14], [60, 12], [118, 0], [96, 14], [40, 22], [0, 23], [-40, 22], [-96, 14]], 4, true).map(cs);
      out.push({ k: 'shape', id: 'face-brim-shadow', g: 'body', pts: bp(sh), mat: 'face', shade: true, ink: null, op: .8 });
    }
    push('carapace', 'carapace', bp(carapacePts()), 'shell');
    // grooves (cervical) and branchial bumps, quiet
    line('carapace-groove-R', 'carapace', bp([[-30, -40], [-46, -26], [-72, -20]].map(cs)), 'ink', .5);
    line('carapace-groove-L', 'carapace', bp([[30, -40], [46, -26], [72, -20]].map(cs)), 'ink', .5);
    if (pose.gear?.helmet) helmet(out, bodyM, bp);
    eye('R'); eye('L'); antenna('R'); antenna('L');
    clawInfo.L = cheliped('L'); clawInfo.R = cheliped('R');
  } else {
    // abdomen: only the folded base shows under the posterior border
    push('underside-sternum', 'body', bp(ell(0, 34, 92, 14, 24)), 'vent', { shade: true });
    push('abdomen-fold', 'abdomen', bp(through([[-20, 28], [20, 28], [16, 42], [0, 45], [-16, 42]], 4, true)), 'vent');
    // eyestalk tips peek above the far edge
    for (const side of ['R', 'L']) { const s = -S(side); const sp = pose.eyes[side]; const a = -(sp.a ?? 0); const tip = [s * 40 + Math.sin(a * D) * 50, -40 - Math.cos(a * D) * 48]; push(`eye-${side}-stalk`, `eye-${side}`, bp(limb([[s * 36, -30], tip], () => 12)), 'stalk'); push(`eye-${side}-cornea`, `eye-${side}`, bp(ell(tip[0], tip[1] - 6, 12, 15, 20, a)), 'cornea'); }
    push('carapace', 'carapace', bp(backCarapacePts()), 'shell');
    line('carapace-groove-R', 'carapace', bp([[-40, -30], [-30, -16], [-26, 0]]), 'ink', .45);
    line('carapace-groove-L', 'carapace', bp([[40, -30], [30, -16], [26, 0]]), 'ink', .45);
    if (pose.gear?.helmet) helmet(out, bodyM, bp, true);
    walkLeg('R', 3); walkLeg('L', 3);
  }
  // gear held in claws
  if (pose.gear?.lantern && clawInfo[pose.gear.lantern]) lanternInClaw(out, clawInfo[pose.gear.lantern], pose);
  if (pose.gear?.lantern === 'hang') lanternHung(out, bodyM, pose);
  if (pose.gear?.shield && clawInfo[pose.gear.shield]) shieldInClaw(out, clawInfo[pose.gear.shield], pose);
  return out;
}

// ---------- gear ----------
function helmet(out, bodyM, bp, back = false) {
  // dented crown bottle cap held on by hooked setae (decorator-crab trait), sitting on the gastric region
  const y = back ? -44 : -50, w = 46;
  const skirt = []; for (let i = 0; i <= 20; i++) { const t = i / 20, x = lerp(-w, w, t); skirt.push([x, y + 6 + (i % 2 ? 4 : 0)]); }
  const shape = [[-w + 3, y - 10], ...through([[-w + 3, y - 10], [-20, y - 16], [6, y - 13], [20, y - 17], [w - 3, y - 10]], 4).slice(1), [w, y + 4], ...skirt.reverse()];
  out.push({ k: 'shape', id: 'gear-helmet-bottlecap', g: 'gear-helmet', pts: bp(shape), mat: 'capRed' });
  out.push({ k: 'shape', id: 'gear-helmet-top', g: 'gear-helmet', pts: bp(ell(0, y - 12, w - 4, 7, 24)), mat: 'cap', sw: .6 });
  out.push({ k: 'line', id: 'gear-helmet-dent', g: 'gear-helmet', pts: bp([[8, y - 13], [16, y - 7], [24, y - 12]]), mat: 'ink', sw: .6 });
}
function lanternPts(c, scale = 1, tilt = 0) {
  // a small glass jar (screw neck), bottle-cap lid, fishing-line bail
  const s = scale, R = (p) => add(c, rot(mul(p, s), tilt));
  const jar = through([[-14, -10], [-17, 0], [-17, 22], [-14, 28], [14, 28], [17, 22], [17, 0], [14, -10]], 4, true).map(R);
  const lid = [[-15, -18], [15, -18], [16, -9], [-16, -9]].map(R);
  const glowBody = through([[-12, 2], [-12, 22], [12, 22], [12, 2], [0, -2]], 4, true).map(R);
  const bail = through([[-13, -14], [-10, -32], [0, -38], [10, -32], [13, -14]], 5).map(R);
  return { jar, lid, glowBody, bail, top: R([0, -38]), center: R([0, 12]) };
}
function drawLantern(out, L, gid) {
  out.push({ k: 'line', id: `${gid}-bail-fishingline`, g: gid, pts: L.bail, mat: 'ink', sw: .7 });
  out.push({ k: 'shape', id: `${gid}-jar-glass`, g: gid, pts: L.jar, mat: 'jarGlass', sw: .8, op: .7 });
  out.push({ k: 'shape', id: `${gid}-bacteria`, g: gid, pts: L.glowBody, mat: 'glow', ink: null });
  out.push({ k: 'shape', id: `${gid}-lid-bottlecap`, g: gid, pts: L.lid, mat: 'cap', sw: .7 });
  out.push({ k: 'glow', g: gid, id: `${gid}-light`, x: L.center[0], y: L.center[1], r: 90, col: '#FFD86A' });
}
function lanternInClaw(out, ci, pose) {
  // hangs from the closed dactyl/fixed-finger tips by its bail
  const tipPt = ci.hand([[ci.L + ci.F * .8, ci.h * .14]])[0];
  const L = lanternPts([tipPt[0], tipPt[1] + 38], 1.15);
  // shift so the bail top sits in the fingertips
  const off = sub(tipPt, L.top);
  const Lm = lanternPts([tipPt[0] + off[0], tipPt[1] + 38 + off[1]], 1.15);
  drawLantern(out, Lm, 'gear-lantern');
}
function lanternHung(out, bodyM, pose) {
  const hook = bodyM([-118, -16]);
  const L = lanternPts([hook[0] - 6, hook[1] + 66], 1.0, 6);
  out.push({ k: 'line', id: 'gear-lantern-tether', g: 'gear-lantern', pts: [hook, add(hook, [-3, 12]), L.top], mat: 'ink', sw: .7 });
  drawLantern(out, L, 'gear-lantern');
}
function shieldInClaw(out, ci) {
  // a frosted amber sea-glass shard gripped between the cutter's fingers, held in front of the body
  const c = ci.hand([[ci.L + ci.F * .45, ci.h * .1]])[0];
  const pts = through([[-4, -58], [8, -60], [34, -30], [36, -24], [30, 30], [24, 40], [-18, 50], [-26, 46], [-38, 4], [-36, -8], [-14, -50]], 3, true).map((p) => add(c, rot(mul(p, .9), -6)));
  out.push({ k: 'shape', id: 'gear-shield-seaglass', g: 'gear-shield', pts, mat: 'glassAmber', op: .82 });
  out.push({ k: 'line', id: 'gear-shield-edge', g: 'gear-shield', pts: [add(c, [-22, -30]), add(c, [-8, 0]), add(c, [-14, 30])], mat: 'glint', sw: 1.2 });
}

// ---------- top (dorsal) view ----------
export function buildTop(opt = {}) {
  const DS = { ...DESIGN_V01, ...(opt.design || DESIGN) };
  const out = [];
  const push = (id, g, pts, mat, extra = {}) => out.push({ k: 'shape', id, g, pts, mat, ...extra });
  const line = (id, g, pts, mat, sw) => out.push({ k: 'line', id, g, pts, mat, sw });
  // legs (from under the margin) — splay sideways, walking legs slightly back-swept
  const legs = [
    { hip: [84, 18], a: 8, m: 80, c: 30, p: 44, d: 36 },
    { hip: [92, 34], a: 22, m: 86, c: 32, p: 46, d: 38 },
    { hip: [88, 48], a: 38, m: 82, c: 31, p: 44, d: 36 },
    { hip: [62, 58], a: 62, m: 60, c: 26, p: 30, d: 42, paddle: true },
  ];
  for (const side of ['R', 'L']) {
    const s = side === 'L' ? 1 : -1; // dorsal view, anterior up: crab's left on screen right
    for (let i = 3; i >= 0; i--) {
      const Lg = legs[i], gid = `leg-${side}${i + 1}`;
      const h = [s * Lg.hip[0], Lg.hip[1]];
      const d1 = [s * Math.cos(Lg.a * D), Math.sin(Lg.a * D)];
      const K = add(h, mul(d1, Lg.m));
      // distal leg bends down (away from viewer) so it foreshortens a little and curves back
      const d2 = norm(add(d1, [0, .25 + i * .05])), C = add(K, mul(d2, Lg.c)), P = add(C, mul(norm(add(d2, [0, .15])), Lg.p * .9));
      const F = add(P, mul(norm(add(d2, [0, .35])), Lg.d * .8));
      push(`${gid}-merus`, gid, limb([h, lerpP(h, K, .5), K], (t) => 22 - 4 * t + 3 * Math.sin(t * Math.PI)), 'leg');
      push(`${gid}-carpus`, gid, limb([K, C], () => 16), 'leg');
      if (!Lg.paddle) {
        push(`${gid}-propodus`, gid, limb([C, P], (t) => 14 - 4 * t), 'leg');
        push(`${gid}-dactyl`, gid, limb([P, lerpP(P, F, .5), F], (t) => 10 * (1 - t) + 1.2, { cap1: false }), 'leg');
      } else {
        const P2 = add(C, mul(d1, 30)), bd = d1, n = perp(bd), mid = add(P2, mul(bd, 26)), pts = [];
        push(`${gid}-propodus`, gid, limb([C, P2], (t) => 16 + 6 * t), 'leg');
        for (let k = 0; k <= 16; k++) { const a = k / 16 * Math.PI; pts.push(add(mid, add(mul(bd, -Math.cos(a) * 28), mul(n, Math.sin(a) * 18)))); }
        for (let k = 1; k < 16; k++) { const a = k / 16 * Math.PI; pts.push(add(mid, add(mul(bd, Math.cos(a) * 28), mul(n, -Math.sin(a) * 15)))); }
        push(`${gid}-dactyl-paddle`, gid, pts, 'leg');
        line(`${gid}-paddle-rib`, gid, [add(mid, mul(bd, -22)), add(mid, mul(bd, 22))], 'ink', .5);
      }
    }
  }
  // carapace dorsal outline: front margin up
  const P = [];
  const front = [[-44, -46], [-36, -50], [-30, -46], [-24, -50], [-18, -53], [-12, -50], [-6, -54], [0, -51], [6, -54], [12, -50], [18, -53], [24, -50], [30, -46], [36, -50], [44, -46]];
  const alm = (sgn) => { // anterolateral margin: 7 teeth + lateral spine
    const out = []; const A = [46 * sgn, -46], Bq = [128 * sgn, 2], C = [104 * sgn, -42];
    const q = (t) => [lerp(lerp(A[0], C[0], t), lerp(C[0], Bq[0], t), t), lerp(lerp(A[1], C[1], t), lerp(C[1], Bq[1], t), t)];
    for (let i = 0; i < 7; i++) { const a = q(i / 7), b = q((i + .6) / 7), tg = norm(sub(b, a)), nn = [tg[1] * sgn, -tg[0] * sgn]; out.push(a, add(lerpP(a, b, .8), add(mul(nn, -9), mul(tg, 3)))); }
    out.push(Bq, [150 * sgn, 6], [126 * sgn, 14]);
    return out;
  };
  P.push(...front.slice().reverse().map(([x, y]) => [-x, y]).reverse());
  P.length = 0; P.push(...front);
  P.push(...alm(1));
  P.push(...through([[118, 22], [86, 44], [50, 62], [0, 66], [-50, 62], [-86, 44], [-118, 22]], 4));
  P.push(...alm(-1).reverse());
  // chelipeds: forward and inward in front of the front margin
  for (const side of ['R', 'L']) {
    const s = side === 'L' ? 1 : -1, gid = `cheliped-${side}`, crusher = side === 'R';
    const cx = [s * 58, -36], E = [s * 112, -70], Wp = [s * 96, -92];
    push(`${gid}-merus`, gid, limb([cx, lerpP(cx, E, .5), E], (t) => (crusher ? 28 : 24) - 4 * t), 'leg');
    push(`${gid}-carpus`, gid, limb([E, Wp], (t) => (crusher ? 28 : 24) + 4 * Math.sin(t * Math.PI)), 'leg');
    const L = crusher ? 80 * DS.crusher.L / 84 : 68 * DS.cutter.L / 70, h = crusher ? 44 * DS.crusher.h / 60 : 32 * DS.cutter.h / 40, F = crusher ? DS.crusher.F : DS.cutter.F;
    const hd = [-s * Math.cos(8 * D), -Math.sin(8 * D)]; // pointing inward, slightly forward
    const hn = perp(hd);
    const hw = (p) => add(Wp, add(mul(hd, p[0]), mul(hn, p[1] * -s)));
    const palm = through([[-4, 0], [4, -h * .5], [L * .6, -h * .52], [L, -h * .3], [L, h * .3], [L * .6, h * .52], [4, h * .5]], 4, true).map(hw);
    const ff = limb([[L * .8, h * .18], [L + F * .5, h * .22], [L + F, h * .05]].map(hw), (t) => h * .4 * (1 - t) + 2.5, { cap1: false });
    const dc = limb([[L * .8, -h * .18], [L + F * .5, -h * .22], [L + F, -h * .05]].map(hw), (t) => h * .4 * (1 - t) + 2.5, { cap1: false });
    push(`${gid}-propodus-fixedfinger`, `${gid}-propodus`, ff, 'leg');
    push(`${gid}-dactyl`, `${gid}-dactyl`, dc, 'leg');
    push(`${gid}-propodus-palm`, `${gid}-propodus`, palm, 'leg');
    push(`${gid}-fingertips`, `${gid}-dactyl`, ell(...hw([L + F * .92, 0]), 7, 5, 10), 'tip', { sw: .5 });
  }
  // eyestalks lie forward/up from the orbits; seen from above they are short
  for (const side of ['R', 'L']) {
    const s = side === 'L' ? 1 : -1;
    push(`eye-${side}-stalk`, `eye-${side}`, limb([[s * 36, -48], [s * 42, -70]], () => 12), 'stalk');
    const E = DS.eye;
    push(`eye-${side}-cornea`, `eye-${side}`, ell(s * 43, -76 - 4 * E, 11 * E, 13 * E, 20), 'cornea');
    push(`eye-${side}-glint`, `eye-${side}`, ell(s * 43 - 3 * E, -80 - 4 * E, 3 * E, 3.5 * E, 10), 'glint', { ink: null });
    line(`antenna-${side}`, `antenna-${side}`, [[s * 20, -52], [s * 30, -76], [s * 46, -98]], 'ink', 1);
    line(`antennule-${side}`, `antennule-${side}`, [[s * 9, -52], [s * 12, -60], [s * 16, -56]], 'ink', .8);
  }
  push('carapace', 'carapace', P.map(([x, y]) => [x * DS.cara, (y - 8) * DS.cara + 8]), 'shell');
  // H-shaped grooves (gastric / cardiac regions) and a few granules
  line('groove-cervical-R', 'carapace', [[-52, -22], [-36, -8], [-22, -4]], 'ink', .5);
  line('groove-cervical-L', 'carapace', [[52, -22], [36, -8], [22, -4]], 'ink', .5);
  line('groove-H-bar', 'carapace', [[-12, 12], [12, 12]], 'ink', .45);
  line('groove-cardiac-R', 'carapace', [[-14, 0], [-17, 18], [-12, 34]], 'ink', .45);
  line('groove-cardiac-L', 'carapace', [[14, 0], [17, 18], [12, 34]], 'ink', .45);
  line('groove-branchial-R', 'carapace', [[-60, 8], [-86, 26]], 'ink', .45);
  line('groove-branchial-L', 'carapace', [[60, 8], [86, 26]], 'ink', .45);
  return out;
}

// ---------- side (end-on along the walking axis, crab's RIGHT flank toward camera, anterior → screen right) ----------
export function buildSide(opt = {}) {
  const DS = { ...DESIGN_V01, ...(opt.design || DESIGN) };
  const out = [];
  const push = (id, g, pts, mat, extra = {}) => out.push({ k: 'shape', id, g, pts, mat, ...extra });
  const line = (id, g, pts, mat, sw) => out.push({ k: 'line', id, g, pts, mat, sw });
  const CS = DS.cara, cs = (p) => [p[0] * CS, (p[1] + 4) * CS - 4];
  // v0.2: legs fan fore-aft in a clear ") | (" rhythm instead of a cage of verticals. The far (left) legs are
  // short dark stubs set between the near feet; near legs come toward the camera, knee just above the flank.
  const farFeet = [[30, 58], [2, -36], [-26, -112]];
  farFeet.forEach(([hx, fx], i) => {
    const gid = `leg-L${i + 1}`, hip = [hx, 16], K = [hx + (fx - hx) * .35, 2], F = [fx, 94];
    const P = lerpP(K, F, .66);
    push(`${gid}-merus`, gid, limb([hip, K], () => 12), 'leg', { far: true });
    push(`${gid}-carpus-propodus`, gid, limb([K, P], (t) => 10.5 - 2.5 * t), 'leg', { far: true });
    push(`${gid}-dactyl`, gid, limb([P, F], (t) => 7.5 * (1 - t) + 1, { cap1: false }), 'leg', { far: true });
  });
  // far paddle, raised behind
  push('leg-L4-merus', 'leg-L4', limb([[-40, -4], [-58, -32], [-70, -54]], () => 12), 'leg', { far: true });
  push('leg-L4-dactyl-paddle', 'leg-L4', ell(-76, -76, 10, 22, 18, -18), 'leg', { far: true });
  // far claw (cutter): held a little higher, its slim fingers show above and beyond the crusher
  push('cheliped-L-merus', 'cheliped-L', limb([[44, 6], [78, -6]], () => 18), 'leg', { far: true });
  push('cheliped-L-propodus-palm', 'cheliped-L-propodus', limb([[76, -6], [112, -22], [130, -30]], (t) => 26 - 8 * t), 'leg', { far: true });
  push('cheliped-L-dactyl', 'cheliped-L-dactyl', limb([[124, -36], [150, -50], [170, -48]], (t) => 10 * (1 - t) + 2, { cap1: false }), 'leg', { far: true });
  push('cheliped-L-propodus-fixedfinger', 'cheliped-L-propodus', limb([[124, -24], [150, -34], [172, -42]], (t) => 10 * (1 - t) + 2, { cap1: false }), 'leg', { far: true });
  // body
  const body = through([[-66, 6], [-62, -18], [-40, -34], [0, -40], [40, -34], [62, -16], [70, 4], [64, 22], [40, 32], [0, 34], [-40, 30], [-62, 20]], 5, true).map(cs);
  push('carapace', 'carapace', body, 'shell');
  push('underside-sternum', 'body', through([[-56, 18], [0, 30], [56, 20], [52, 32], [0, 40], [-52, 30]], 4, true).map(cs), DS.eye > 1 ? 'face' : 'vent');
  push('carapace-lateral-spine', 'carapace', ell(...cs([-6, -10]), 9, 7, 14), 'shell', { sw: .8 });
  line('carapace-margin', 'carapace', through([[-62, 4], [-30, -4], [-6, -10], [30, 2], [66, 8]], 4).map(cs), 'ink', .55);
  // eyestalk (near) at the front, leaning forward-up; same big glossy eye as the front view
  const E = DS.eye, eb = cs([62, -16]), et = add(eb, [16 * DS.stalk, -52 * DS.stalk]);
  push('eye-R-stalk', 'eye-R', limb([eb, add(lerpP(eb, et, .5), [4, 0]), et], (t) => 12 - 2.5 * t), 'stalk');
  const ec = add(et, [2 * E, -6 * E]);
  push('eye-R-cornea', 'eye-R', ell(ec[0], ec[1], 14.5 * E, 17.5 * E, 24, 16), 'cornea');
  push('eye-R-glint', 'eye-R', ell(ec[0] - 4 * E, ec[1] - 7 * E, 4.6 * E, 5.6 * E, 12, -10), 'glint', { ink: null });
  push('eye-R-glint2', 'eye-R', ell(ec[0] + 5.5 * E, ec[1] + 6 * E, 2 * E, 2.2 * E, 8), 'glint', { ink: null, op: .7 });
  line('antenna-R', 'antenna-R', [cs([70, -14]), cs([96, -30]), cs([120, -34])], 'ink', 1);
  push('maxilliped3-R', 'mouthparts', limb([cs([66, 6]), cs([72, 22])], () => 10), DS.eye > 1 ? 'face' : 'vent', { sw: .6 });
  // near paddle at the back, raised
  push('leg-R4-merus', 'leg-R4', limb([[-54, 14], [-76, -16], [-92, -38]], () => 17), 'leg');
  push('leg-R4-dactyl-paddle', 'leg-R4', ell(-104, -58, 13, 28, 20, -24), 'leg');
  line('leg-R4-paddle-rib', 'leg-R4', [[-98, -36], [-104, -58], [-110, -80]], 'ink', .5);
  // near (right) walking legs: ")  |  (" — front leg reaches forward, back leg back
  const near = [[38, 22, 100], [8, 6, 10], [-24, -14, -90]];
  for (let i = 2; i >= 0; i--) {
    const [hx, kx, fx] = near[i], gid = `leg-R${i + 1}`, hip = [hx, 22];
    const K = [kx + (i === 1 ? 0 : (i === 0 ? 6 : -6)), -20], F = [fx, 104];
    const C = lerpP(K, F, .4), P = lerpP(K, F, .7);
    const bend = (i - 1) * -6; // a gentle bow along each leg
    push(`${gid}-merus`, gid, limb([hip, K], (t) => 21 + 3 * t), 'leg');
    push(`${gid}-carpus`, gid, limb([K, add(C, [bend, 0])], () => 17 * DS.leg), 'leg');
    push(`${gid}-propodus`, gid, limb([add(C, [bend, 0]), P], (t) => (16 - 3 * t) * DS.leg), 'leg');
    push(`${gid}-dactyl`, gid, limb([P, lerpP(P, F, .5), F], (t) => 11 * (1 - t) + 1.4, { cap1: false }), 'leg');
  }
  // near claw (crusher) in profile, pointing forward
  const L = DS.crusher.L, h = DS.crusher.h, F = DS.crusher.F, wp = [96, 22];
  push('cheliped-R-merus', 'cheliped-R', limb([[46, 26], [82, 24]], () => 26 + (h - 60) * .3), 'leg');
  push('cheliped-R-carpus', 'cheliped-R', limb([[80, 24], [98, 22]], () => 28 + (h - 60) * .3), 'leg');
  const hw = (p) => add(wp, p);
  push('cheliped-R-propodus-fixedfinger', 'cheliped-R-propodus', limb([[L * .78, h * .22], [L + F * .5, h * .30], [L + F, h * .12]].map(hw), (t) => h * .42 * (1 - t) + 3, { cap1: false }), 'leg');
  push('cheliped-R-propodus-palm', 'cheliped-R-propodus', through([[-4, 0], [2, -h * .40], [L * .45, -h * .54], [L * .92, -h * .44], [L * 1.04, -h * .12], [L * .98, h * .30], [L * .6, h * .5], [L * .1, h * .44]], 5, true).map(hw), 'leg');
  if (DS.eye > 1) push('cheliped-R-palm-belly', 'cheliped-R-propodus', through([[L * .14, h * .40], [L * .5, h * .26], [L * .9, h * .2], [L * .97, h * .3], [L * .6, h * .45], [L * .16, h * .41]], 4, true).map(hw), 'belly', { ink: null });
  push('cheliped-R-dactyl', 'cheliped-R-dactyl', limb([[L * .9, -h * .26], [L + F * .55, -h * .3], [L + F * 1.02, h * .08]].map(hw), (t) => h * .4 * (1 - t) + 3, { cap1: false }), 'leg');
  push('cheliped-R-tip', 'cheliped-R-dactyl', ell(...hw([L + F * .9, h * .1]), 8, 6, 10), 'tip', { sw: .5 });
  return out;
}
