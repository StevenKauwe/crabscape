// Builds the model sheet: scene.json (for the p5.brush painter), model-sheet.svg and crab-parts.svg.
// node sheet.mjs <outdir> <scene.json>
import fs from 'fs';
import path from 'path';
import { basePose, buildFront, buildTop, buildSide, PALETTES, clone } from './crab-v0.1.mjs';
import { SvgOut, transformItems } from './svg.mjs';
import { T, chain, ell, through, bbox, rng, hashStr, lerp } from './geom.mjs';

const [outDir, scenePath] = process.argv.slice(2);
const SW = 3200, BG = '#F3EBDD', INK = '#2A140F', NOTE = '#6B4A3C';
const tiles = [], texts = [], rects = [], blits = [], flats = [], strokesOut = [];
const counts = [];

// ---------- poses ----------
const P0 = basePose();
const pose = (f) => { const p = clone(P0); f(p); return p; };
const feet = (p, fn) => { for (const s of ['R', 'L']) p.legs[s].forEach((l, i) => { if (i < 3) l.f = fn(l.f, s, i); }); };

const POSES = {
  hero: pose(() => {}),
  idle: pose((p) => { p.body.dy = 4; }),
  // walking to screen-right (the crab's LEFT side leads). Set A swings L1, L3, R2 while the others push.
  walkA: pose((p) => {
    p.body.dx = 6; p.body.dy = -3; p.body.rot = 1.5;
    const lift = { L: [0, 2], R: [1] };
    feet(p, (f, s, i) => lift[s].includes(i) ? [f[0] + 40, f[1] - 46] : [f[0] - 16, f[1]]);
    p.claws.R.wrist = [-114, 58]; p.claws.L.wrist = [108, 38];
  }),
  walkB: pose((p) => {
    p.body.dx = 6; p.body.dy = -1; p.body.rot = -1.5;
    const lift = { R: [0, 2], L: [1] };
    feet(p, (f, s, i) => lift[s].includes(i) ? [f[0] + 40, f[1] - 46] : [f[0] - 16, f[1]]);
    p.claws.R.wrist = [-116, 62]; p.claws.L.wrist = [106, 42];
  }),
  jumpWind: pose((p) => {
    p.body.dy = 26; p.body.sy = .95; p.body.sx = 1.03;
    feet(p, (f, s) => [f[0] + (s === 'L' ? 14 : -14), f[1]]);
    p.claws.R = { wrist: [-72, 48], aim: 28, gape: 0 }; p.claws.L = { wrist: [72, 48], aim: 26, gape: 0 };
    p.legs.R[3] = { f: [-140, -96], blade: -20 }; p.legs.L[3] = { f: [140, -96], blade: 20 };
    p.eyes.R = { a: -4, fold: .25 }; p.eyes.L = { a: 4, fold: .25 };
  }),
  paddle: pose((p) => {
    p.body.dy = -70;
    feet(p, (f, s, i) => [f[0] * .8, f[1] - 10 + i * 4]);
    p.legs.R[3] = { f: [-236, 30], blade: -128, bladeW: 17 }; p.legs.L[3] = { f: [236, 30], blade: 128, bladeW: 17 };
    p.claws.R = { wrist: [-80, 52], aim: 18, gape: 0 }; p.claws.L = { wrist: [80, 52], aim: 16, gape: 0 };
    p.eyes.R = { a: -12 }; p.eyes.L = { a: 12 };
    p.ant.R = { a: -70, curl: -.2 }; p.ant.L = { a: 70, curl: -.2 };
  }),
  land: pose((p) => {
    p.body.dy = 30; p.body.sx = 1.05; p.body.sy = .9;
    feet(p, (f, s, i) => [f[0] * 1.12, f[1] + 2]);
    p.claws.R = { wrist: [-140, 46], aim: -22, gape: 10 }; p.claws.L = { wrist: [138, 46], aim: -20, gape: 12 };
    p.eyes.R = { a: -20, fold: .45 }; p.eyes.L = { a: 20, fold: .45 };
    p.legs.R[3] = { f: [-196, -40], blade: -100 }; p.legs.L[3] = { f: [196, -40], blade: 100 };
  }),
  climb: pose((p) => {
    // drawn standing, then rotated so the 'ground' is a wall on the right
    p.claws.R = { wrist: [-108, 70], aim: -50, gape: 22 }; p.claws.L = { wrist: [104, 68], aim: -48, gape: 26 };
    feet(p, (f, s, i) => [f[0] * .96 + (s === 'L' ? 10 : -6), 104 + (i === 1 ? 4 : 0)]);
    p.eyes.R = { a: -24 }; p.eyes.L = { a: -4 };
  }),
  lantern: pose((p) => {
    p.gear = { lantern: 'L' };
    p.claws.L = { wrist: [124, -6], aim: 56, gape: 2 };
    p.eyes.R = { a: 2 }; p.eyes.L = { a: 18 };
    p.ant.L = { a: 30, curl: .6 };
  }),
  // ---- combat (crusher strikes to screen-left; the cutter is the quick right-hand jab)
  guard: pose((p) => {
    p.body.dy = 12;
    feet(p, (f, s) => [f[0] * 1.06, f[1]]);
    p.claws.R = { wrist: [-58, 14], aim: 78, gape: 0 }; p.claws.L = { wrist: [70, 26], aim: 64, gape: 0 };
    p.eyes.R = { a: -10, fold: .2 }; p.eyes.L = { a: 10, fold: .2 };
  }),
  parry: pose((p) => {
    p.body.dy = 10; p.body.rot = -5; p.body.dx = -6;
    feet(p, (f, s) => [f[0] * 1.08, f[1]]);
    p.claws.R = { wrist: [-128, -44], aim: 112, gape: 0 }; p.claws.L = { wrist: [62, 30], aim: 50, gape: 0 };
    p.eyes.R = { a: -20 }; p.eyes.L = { a: -8 };
  }),
  windup: pose((p) => {
    p.body.dx = 16; p.body.rot = 8; p.body.dy = 6;
    feet(p, (f, s) => [f[0] + (s === 'R' ? -10 : 6), f[1]]);
    p.claws.R = { wrist: [-48, -74], aim: 150, gape: 38 }; p.claws.L = { wrist: [96, 44], aim: 10, gape: 4 };
    p.eyes.R = { a: -30 }; p.eyes.L = { a: -22 };
    p.ant.R = { a: -60, curl: .1 }; p.ant.L = { a: 10, curl: .4 };
  }),
  strike: pose((p) => {
    p.body.dx = -26; p.body.rot = -9; p.body.dy = 8;
    feet(p, (f, s) => [f[0] + (s === 'R' ? -22 : -4), f[1]]);
    p.claws.R = { wrist: [-196, 6], flip: -1, aim: 6, gape: 0 }; p.claws.L = { wrist: [86, 40], aim: 20, gape: 2 };
    p.eyes.R = { a: -36 }; p.eyes.L = { a: -26 };
    p.ant.R = { a: -80, curl: -.2 }; p.ant.L = { a: -10, curl: .2 };
  }),
  follow: pose((p) => {
    p.body.dx = -34; p.body.rot = -13; p.body.dy = 16;
    feet(p, (f, s) => [f[0] + (s === 'R' ? -26 : 8), f[1]]);
    p.claws.R = { wrist: [-196, 22], flip: -1, aim: -12, gape: 0 }; p.claws.L = { wrist: [92, 26], aim: 34, gape: 4 };
    p.eyes.R = { a: -30, fold: .15 }; p.eyes.L = { a: -20, fold: .15 };
  }),
  recover: pose((p) => {
    p.body.dx = -10; p.body.rot = -3; p.body.dy = 8;
    p.claws.R = { wrist: [-104, 36], aim: 34, gape: 6 }; p.claws.L = { wrist: [84, 34], aim: 34, gape: 4 };
    p.eyes.R = { a: -12 }; p.eyes.L = { a: 6 };
  }),
  counter: pose((p) => {
    p.body.dx = -18; p.body.rot = -10; p.body.dy = -12;
    feet(p, (f, s) => [f[0] + (s === 'R' ? -20 : 0), f[1]]);
    p.claws.R = { wrist: [-170, -64], flip: -1, aim: 52, gape: 0 }; p.claws.L = { wrist: [66, 26], aim: 50, gape: 0 };
    p.eyes.R = { a: -34 }; p.eyes.L = { a: -24 };
    p.ant.R = { a: -84, curl: -.2 };
  }),
  // ---- expressions
  curious: pose((p) => {
    p.body.rot = 4; p.body.dy = -4;
    p.eyes.R = { a: -22, len: 1.06 }; p.eyes.L = { a: 2, len: .96 };
    p.ant.R = { a: -18, curl: .7 }; p.ant.L = { a: 22, curl: .6 };
    p.claws.L = { wrist: [118, 18], aim: 44, gape: 22 };
  }),
  scared: pose((p) => {
    p.body.dy = 18;
    feet(p, (f, s, i) => [f[0] * .84, f[1]]);
    p.eyes.R = { fold: 1, a: 0 }; p.eyes.L = { fold: 1, a: 0 };
    p.claws.R = { wrist: [-58, 36], aim: 52, gape: 0 }; p.claws.L = { wrist: [60, 38], aim: 54, gape: 0 };
    p.ant.R = { a: -100, curl: .2 }; p.ant.L = { a: 100, curl: .2 };
    p.legs.R[3] = { f: [-140, -50], blade: -40 }; p.legs.L[3] = { f: [140, -50], blade: 40 };
  }),
  threat: pose((p) => {
    p.body.dy = -20;
    feet(p, (f, s) => [f[0] * 1.02, f[1]]);
    p.claws.R = { wrist: [-158, -46], aim: 84, gape: 36 }; p.claws.L = { wrist: [156, -44], aim: 84, gape: 38 };
    p.eyes.R = { a: -3, len: 1.06 }; p.eyes.L = { a: 3, len: 1.06 };
    p.ant.R = { a: -64, curl: -.1 }; p.ant.L = { a: 64, curl: -.1 };
  }),
  cozy: pose((p) => {
    p.body.dy = 22;
    feet(p, (f, s, i) => [f[0] * .9, f[1]]);
    p.claws.R = { wrist: [-96, 52], aim: -14, gape: 2 }; p.claws.L = { wrist: [92, 34], aim: -2, gape: 2 };
    p.eyes.R = { a: -24 }; p.eyes.L = { a: 24 };
    p.ant.R = { a: -46, curl: .9 }; p.ant.L = { a: 46, curl: .9 };
    p.gear = { lantern: 'hang' };
  }),
  tired: pose((p) => {
    p.body.dy = 40; p.body.rot = 3; p.body.sy = .97;
    feet(p, (f, s, i) => [f[0] * 1.1, 104]);
    p.claws.R = { wrist: [-140, 44], aim: -14, gape: 8 }; p.claws.L = { wrist: [134, 46], aim: -16, gape: 12 };
    p.eyes.R = { a: -56, fold: .4 }; p.eyes.L = { a: 50, fold: .4 };
    p.ant.R = { a: -116, curl: .1 }; p.ant.L = { a: 112, curl: .1 };
    p.legs.R[3] = { f: [-190, -20], blade: -110 }; p.legs.L[3] = { f: [190, -20], blade: 110 };
  }),
  armour: pose((p) => {
    p.gear = { helmet: true, shield: 'L', lantern: 'hang' };
    p.claws.L = { wrist: [150, 0], aim: 40, gape: 0 };
    p.body.dy = 6;
  }),
  soft: pose((p) => {
    p.body.dy = 14;
    p.eyes.R = { a: -6, fold: .3 }; p.eyes.L = { a: 6, fold: .3 };
    p.claws.R = { wrist: [-104, 50], aim: 6, gape: 0 }; p.claws.L = { wrist: [98, 34], aim: 10, gape: 0 };
    feet(p, (f) => [f[0] * .92, f[1]]);
  }),
  regrow: pose((p) => {
    p.regrowR = 1;
    p.claws.R = { wrist: [-112, 40], aim: 14, gape: 10 };
    p.eyes.R = { a: -16 }; p.eyes.L = { a: 6 };
  }),
};

// ---------- helpers ----------
const groundShadow = (cx, gy, k, w = 230) => ({ k: 'fill', id: 'shadow' + cx + gy, pts: ell(cx, gy + 2 * k, w * k, 13 * k, 32), col: '#3D4A48', op: 85, bleed: .08 });
function silt(cx, gy, k, n = 7, seedKey = 'silt') {
  const r = rng(hashStr(seedKey)), out = [];
  for (let i = 0; i < n; i++) { const x = cx + (r() - .5) * 420 * k, y = gy - r() * 26 * k, rr = (10 + r() * 22) * k; out.push({ k: 'fill', id: `${seedKey}${i}`, pts: ell(x, y, rr * 1.4, rr * .8, 18), col: '#C9B48E', op: 70, bleed: .2 }); }
  return out;
}
function bubbles(cx, cy, k, n = 6, seedKey = 'bub') {
  const r = rng(hashStr(seedKey)), out = [];
  for (let i = 0; i < n; i++) { const x = cx + (r() - .5) * 160 * k, y = cy - r() * 200 * k, rr = (3 + r() * 7) * k; out.push({ k: 'shape', id: `${seedKey}${i}`, g: 'fx', pts: ell(x, y, rr, rr, 12), mat: 'glint', sw: .5, op: .55 }); }
  return out;
}
function countParts(items) {
  const legs = new Set(), claws = new Set();
  for (const it of items) { const m = /^(leg-[RL][1-4])-/.exec(it.id || ''); if (m) legs.add(m[1]); const c = /^(cheliped-[RL])-/.exec(it.id || ''); if (c) claws.add(c[1]); }
  return { legs: legs.size, claws: claws.size };
}

// A tile: background items + crab
function tile(id, x, y, w, h, o = {}) {
  const t = { id, x, y, w, h, bg: o.bg || '#F7F1E6', pal: PALETTES[o.pal || 'normal'], palName: o.pal || 'normal', k: o.k || 1, light: o.light, items: [], label: o.label, note: o.note, frame: o.frame !== false };
  tiles.push(t);
  return t;
}
function addCrab(t, items, cx, gy, k, o = {}) {
  const M = chain(T.tr(cx, gy - 100 * k), o.rot ? T.ro(o.rot) : [1, 0, 0, 1, 0, 0], T.sc(k));
  const its = transformItems(items, M).map((it) => ({ ...it, crab: o.tag || t.id }));
  t.items.push(...its);
  const c = countParts(items); counts.push({ tile: t.id, view: o.view || 'front', ...c });
  return its;
}

// ---------- layout ----------
let Y = 40;
texts.push({ x: 40, y: Y + 64, s: 'CRABSCAPE · PLAYER CRAB · MODEL SHEET', size: 64, weight: 'bold', family: 'C059, serif' });
texts.push({ x: 40, y: Y + 112, s: 'v0.1 first design pass · a swimming crab (Portunidae) with a stone-crab crusher and long-eyed stalks · 10 legs: 2 claws + 6 walking legs + 2 swimming paddles', size: 27, fill: NOTE, family: 'C059, serif', style: 'italic' });
texts.push({ x: SW - 40, y: Y + 64, s: 'R = the crab’s own right (screen left in front view) · crusher on R, cutter on L', size: 24, anchor: 'end', fill: NOTE, family: 'Nimbus Sans, sans-serif' });
texts.push({ x: SW - 40, y: Y + 100, s: 'in-game camera sees the FRONT: a crab walks sideways across the screen', size: 24, anchor: 'end', fill: NOTE, family: 'Nimbus Sans, sans-serif' });
Y += 150;

// Row 1: hero + turnaround
const heroK = 3.0;
const hero = tile('hero', 40, Y, 1860, 1120, { k: heroK, label: '1 · MAIN IN-GAME VIEW (front: what the camera sees as the crab walks sideways)', note: 'Low, wide, planted: knees high and out, dactyl tips dug in. Claws carried low like heavy fists. Paddles (5th legs) held up over the back.' });
hero.items.push(groundShadow(930, 950, heroK, 260));
const heroItems = addCrab(hero, buildFront(POSES.hero), 930, 950, heroK, { tag: 'hero' });

const TX = 1930, TW = 610, TH = 545;
const tFront = tile('turn-front', TX, Y, TW, TH, { k: 1.0, label: '2a · FRONT', note: '2 claws + 8 legs (paddles up)' });
tFront.items.push(groundShadow(305, 430, 1.0)); addCrab(tFront, buildFront(POSES.hero), 305, 430, 1.0);
const tTop = tile('turn-top', TX + TW + 20, Y, TW, TH, { k: .95, label: '2b · TOP (dorsal)', note: 'no tail: abdomen folded under · 9 anterolateral teeth each side' });
addCrab(tTop, buildTop(), TX + TW + 20 - (TX + TW + 20) + 305, 300, .95, { view: 'top' });
const tSide = tile('turn-side', TX, Y + TH + 30, TW, TH, { k: 1.25, label: '2c · SIDE (end-on, along the walking axis)', note: 'crab’s right flank: near legs foreshortened toward us' });
tSide.items.push(groundShadow(290, 440, 1.25, 120)); addCrab(tSide, buildSide(), 290, 440, 1.25, { view: 'side' });
const tBack = tile('turn-back', TX + TW + 20, Y + TH + 30, TW, TH, { k: 1.0, label: '2d · BACK', note: 'paddles nearest · abdomen fold just visible' });
tBack.items.push(groundShadow(305, 430, 1.0)); addCrab(tBack, buildFront({ ...clone(POSES.hero), view: 'back' }), 305, 430, 1.0, { view: 'back' });
Y += 1120 + 50;

// Row 2: expressions
const section = (y, s, sub) => { texts.push({ x: 40, y, s, size: 34, weight: 'bold', family: 'C059, serif' }); if (sub) texts.push({ x: 40, y: y + 34, s: sub, size: 22, fill: NOTE, family: 'Nimbus Sans, sans-serif', style: 'italic' }); };
section(Y + 34, '3 · EXPRESSIONS', 'No face muscles: emotion is carried by the eyestalks, antennae, claws and how high the body rides on its legs.');
Y += 80;
const EX = [['curious', 'curious', 'eyes cocked at different angles, antennae forward, cutter lifted'], ['scared', 'scared', 'eyes folded into the orbits, claws tucked over the face, body dropped'], ['threat', 'angry / threat', 'up on its legs, claws raised and spread wide, fingers gaping'], ['cozy', 'happy / cozy', 'settled low, claws folded, eyes relaxed outward, lantern hung'], ['tired', 'exhausted', 'belly on the sand, claws dragging, eyes half-folded']];
EX.forEach(([key, lab, note], i) => {
  const x = 40 + i * 630, t = tile('ex-' + key, x, Y, 600, 520, { k: 1.0, label: lab.toUpperCase(), note });
  t.items.push(groundShadow(300, 450, 1.1)); addCrab(t, buildFront(POSES[key]), 300, 450, 1.1);
});
Y += 520 + 50;

// Row 3: movement
section(Y + 34, '4 · MOVEMENT · a heavy tank: momentum, wind-up, weight', 'Walk: diagonal sets alternate (L1+L3+R2, then R1+R3+L2) while the claws stay off the ground. Gait timing from Barnes 1975 (Uca), adapted for paddles held up: see notes.');
Y += 80;
const MW = 433, MH = 480;
const MV = [['idle', 'IDLE · planted', 'weight low, feet wide'], ['walkA', 'WALK · A', '→ L1 L3 R2 swing'], ['walkB', 'WALK · B', '→ R1 R3 L2 swing'], ['jumpWind', 'JUMP · WIND-UP', 'sink, claws in, paddles cocked'], ['paddle', 'JUMP · PADDLE PUSH', 'paddles drive down: double jump'], ['land', 'HEAVY LANDING', 'squash, splay, claws brace, silt'], ['climb', 'CLIMB · CLING', 'dactyls hooked into rock']];
MV.forEach(([key, lab, note], i) => {
  const x = 40 + i * (MW + 18), t = tile('mv-' + key, x, Y, MW, MH, { k: .7, label: lab, note });
  if (key === 'climb') {
    // wall on the right: rotate the crab so its feet meet the wall
    t.items.push({ k: 'wash', id: 'wall', pts: through([[350, -10], [338, 80], [356, 170], [342, 260], [360, 350], [346, 440], [358, 500], [460, 500], [460, -10]], 3), col: '#8C7F6E' });
    t.items.push({ k: 'fill', id: 'wall2', pts: through([[352, 40], [372, 120], [360, 230], [378, 330], [366, 420], [440, 420], [440, 40]], 3), col: '#5E5548', op: 70 });
    // rotated -90°: the crab's ground (y=+100) maps to +x, so the body centre sits 100k left of the wall
    addCrab(t, buildFront(POSES[key]), 350 - 100 * .62, 262 + 100 * .62, .62, { rot: -90 });
  } else {
    const gy = 415;
    t.items.push(groundShadow(216, gy, key === 'paddle' ? .5 : .8, key === 'paddle' ? 150 : 230));
    if (key === 'land') t.items.push(...silt(216, gy, .8, 8, 'landsilt'));
    addCrab(t, buildFront(POSES[key]), 216, gy, .8);
    if (key === 'paddle') { t.items.push(...bubbles(50, 400, .5, 4, 'padb')); t.items.push(...bubbles(382, 400, .5, 4, 'padc')); }
  }
});
Y += MH + 50;

// Row 4: combat
section(Y + 34, '5 · COMBAT · guard, timed parry, crusher combo, counter', 'Crusher (right claw) swings to screen-left: slow, crushing. Cutter (left) is the quick jab to screen-right. Every hit starts with a readable wind-up.');
Y += 80;
const CB = [['guard', 'GUARD', 'claws up in front as a shield'], ['parry', 'PARRY (last instant)', 'crusher snaps up to deflect'], ['windup', 'ATTACK 1 · WIND-UP', 'weight back, crusher cocked open'], ['strike', 'ATTACK 2 · STRIKE', 'lunge, claw slams shut'], ['follow', 'ATTACK 3 · FOLLOW-THROUGH', 'momentum carries past'], ['recover', 'ATTACK 4 · RECOVERY', 'settle back to guard'], ['counter', 'COUNTER-ATTACK', 'after a parry: rising crush']];
CB.forEach(([key, lab, note], i) => {
  const x = 40 + i * (MW + 18), t = tile('cb-' + key, x, Y, MW, MH, { k: .7, label: lab, note });
  t.items.push(groundShadow(232, 415, .8));
  if (key === 'parry') { for (let j = 0; j < 5; j++) { const a = (-150 + j * 22) * Math.PI / 180, c = [80, 168]; t.items.push({ k: 'line', id: 'clack' + j, g: 'fx', pts: [[c[0] + Math.cos(a) * 34, c[1] + Math.sin(a) * 34], [c[0] + Math.cos(a) * 56, c[1] + Math.sin(a) * 56]], mat: 'ink', sw: .9 }); } }
  if (key === 'follow') t.items.push(...silt(90, 415, .5, 5, 'fsilt'));
  addCrab(t, buildFront(POSES[key]), ['strike', 'follow', 'counter'].includes(key) ? 262 : 236, 415, ['strike', 'follow', 'counter'].includes(key) ? .74 : .8);
});
Y += MH + 50;

// Row 5: gear + states
section(Y + 34, '6 · GEAR (found litter, real biology)   ·   7 · STATES', 'Lantern: a jar culture of yellow-glowing Vibrio-type bacteria (a real yellow strain exists). Helmet held by hooked setae like a decorator crab. Shield gripped like a boxer crab’s anemone.');
Y += 80;
const GH = 560;
const gL = tile('gear-lantern', 40, Y, 600, GH, { k: .95, label: 'LANTERN IN THE CUTTER', note: 'costs a claw: no cutter jab while carried', bg: '#E9E4D8' });
gL.items.push(groundShadow(300, 470, .9)); addCrab(gL, buildFront(POSES.lantern), 290, 470, 1.0);
const gA = tile('gear-armour', 670, Y, 600, GH, { k: .95, label: 'ARMOUR SET', note: 'bottle-cap helmet · amber sea-glass shield · lantern hung from a setae hook' });
gA.items.push(groundShadow(300, 470, .9)); addCrab(gA, buildFront(POSES.armour), 290, 470, 1.0);
// prop callouts at large scale
const gP = tile('gear-props', 1300, Y, 600, GH, { k: 1.6, label: 'PROPS (crab scale: cap ≈ ⅓ carapace width)', note: 'jar + bottle-cap lid + fishing-line bail · crown cap · sea glass' });
{
  const lan = buildFront({ ...clone(POSES.lantern) }).filter((it) => (it.g || '').startsWith('gear-lantern'));
  const b = bbox(lan.filter((i) => i.pts).flatMap((i) => i.pts));
  gP.items.push(...transformItems(lan, chain(T.tr(150, 300), T.sc(2.6), T.tr(-b.cx, -b.cy))));
  const arm = buildFront(POSES.armour);
  const hel = arm.filter((it) => (it.g || '') === 'gear-helmet'), hb = bbox(hel.flatMap((i) => i.pts));
  gP.items.push(...transformItems(hel, chain(T.tr(420, 190), T.sc(2.6), T.tr(-hb.cx, -hb.cy))));
  const sh = arm.filter((it) => (it.g || '') === 'gear-shield'), sb = bbox(sh.flatMap((i) => i.pts));
  gP.items.push(...transformItems(sh, chain(T.tr(430, 400), T.sc(1.5), T.tr(-sb.cx, -sb.cy))));
}
const sS = tile('state-soft', 1930, Y, 600, GH, { k: .95, label: 'SOFT-SHELL (fresh moult)', note: 'pale, cautious; hardens over ~1 day (paper → buckram)', pal: 'soft' });
sS.items.push(groundShadow(300, 470, .9)); addCrab(sS, buildFront(POSES.soft), 300, 470, 1.0);
const sR = tile('state-regrow', 2560, Y, 600, GH, { k: .95, label: 'REGROWING CLAW', note: 'lost crusher returns small and slender; grows back over moults' });
sR.items.push(groundShadow(300, 470, .9)); addCrab(sR, buildFront(POSES.regrow), 300, 470, 1.0);
Y += GH + 50;

// Row 6: readability + colour
section(Y + 34, '8 · READABILITY (phone)   ·   9 · COLOUR KEY + RED-AT-DEPTH', 'Downscaled from the painted hero as the game would show it, plus flat silhouettes.');
Y += 80;
const RH = 660;
rects.push({ x: 40, y: Y, w: 1240, h: RH, fill: '#F7F1E6' });
const heroB = bbox(heroItems.filter((i) => i.pts).flatMap((i) => i.pts));
const heroSrc = [Math.round(hero.x + heroB.x0 - 8), Math.round(hero.y + heroB.y0 - 8), Math.round(heroB.w + 16), Math.round(heroB.h + 16)];
const ar = heroSrc[2] / heroSrc[3];
let sxp = 80;
for (const hpx of [40, 80, 160]) {
  const w = Math.round(hpx * ar);
  blits.push({ src: heroSrc, dst: [sxp, Y + 250 - hpx, w, hpx] });
  texts.push({ x: sxp, y: Y + 278, s: `${hpx} px`, size: 22, family: 'Nimbus Sans, sans-serif', fill: NOTE });
  sxp += w + 50;
}
// silhouettes: idle, threat, jump, guard at 40 px and idle at 80 & 160
function silhouette(items, dx, dy, hpx) {
  const polys = items.filter((i) => i.k === 'shape' && i.mat !== 'glint' && i.mat !== 'glow').map((i) => i.pts);
  const lines = items.filter((i) => i.k === 'line' && /antenna/.test(i.id)).map((i) => i.pts);
  const b = bbox(polys.flat()), k = hpx / b.h;
  const tr = (P) => P.map(([x, y]) => [dx + (x - b.x0) * k, dy + (y - b.y0) * k]);
  flats.push({ polys: polys.map(tr), col: '#1E1612' });
  strokesOut.push({ lines: lines.map(tr), col: '#1E1612', w: Math.max(1, hpx / 90) });
  return b.w * k;
}
let sx2 = 80; const base2 = Y + 580;
texts.push({ x: 80, y: Y + 620, s: 'flat silhouettes · idle at 160 / 80 px, then idle, threat, guard, paddle-jump at 40 px', size: 20, family: 'Nimbus Sans, sans-serif', fill: NOTE });
{ const w = silhouette(buildFront(POSES.idle), sx2, base2 - 160, 160); sx2 += w + 36; } { const w = silhouette(buildFront(POSES.idle), sx2, base2 - 80, 80); sx2 += w + 36; }
for (const key of ['idle', 'threat', 'guard', 'paddle']) { const w = silhouette(buildFront(POSES[key]), sx2, base2 - 40, 40); sx2 += w + 20; }
texts.push({ x: 80, y: Y + 40, s: 'painted hero, downscaled', size: 20, family: 'Nimbus Sans, sans-serif', fill: NOTE });

// colour key chips
const CK = [
  ['crab shell', ['#F07C4E', '#C9452C', '#7C2418']], ['underside', ['#F9D4AE', '#E7A37A', '#A6634A']], ['home / amber', ['#F2D27A', '#C8962E', '#6E4A16']],
  ['lantern light', ['#FFF3B0', '#FFD45E', '#D89A22']], ['the deep', ['#3E8C8C', '#1B4752', '#06141A']], ['wild biolum.', ['#8CF2FF', '#6E5BFF', '#B05CFF']],
];
const CX0 = 1310;
CK.forEach(([name, cols], i) => {
  const x = CX0 + (i % 3) * 210, y = Y + 20 + Math.floor(i / 3) * 150;
  cols.forEach((c, j) => rects.push({ x: x + j * 58, y, w: 56, h: 90, fill: c, over: false }));
  texts.push({ x, y: y + 116, s: name, size: 20, family: 'Nimbus Sans, sans-serif', fill: NOTE });
});
// red-at-depth demo: same crab, three depths
const DW = 400, DX = 1960;
const depths = [['0–10 m · sunlit', 'normal', '#9FD3D6', [-.25, -1]], ['~30–50 m · reds fade', 'mid', '#3F7F8C', [-.2, -1]], ['100 m + · red = black', 'deep', '#0B2530', [-.1, -1]]];
depths.forEach(([lab, pal, bg, L], i) => {
  const t = tile('depth-' + pal, DX + i * DW, Y, DW - 10, RH, { k: .62, label: lab.toUpperCase(), note: i === 2 ? 'camouflage: no red light reaches here (NOAA)' : i === 1 ? 'red and orange absorbed first' : 'true colours', pal, bg, light: L });
  t.items.push(groundShadow(195, 420, .6, 220));
  addCrab(t, buildFront(POSES.idle), 195, 420, .6);
  if (i === 2) { // a lit lantern shows what the deep hides
    t.items.push({ k: 'glow', id: 'deepglow', x: 330, y: 160, r: 60, col: '#6E5BFF', a: .8 });
    t.items.push({ k: 'glow', id: 'deepglow2', x: 70, y: 110, r: 40, col: '#8CF2FF', a: .7 });
  }
});
rects.push({ x: CX0, y: Y + 330, w: 630, h: 2, fill: '#CBBFAE' });
texts.push({ x: CX0, y: Y + 370, s: 'Warm = crab, home, lantern. Cold teal → black = the deep.', size: 21, family: 'Nimbus Sans, sans-serif', fill: NOTE });
texts.push({ x: CX0, y: Y + 400, s: 'Violet / cyan = wild bioluminescence. Our lantern is the', size: 21, family: 'Nimbus Sans, sans-serif', fill: NOTE });
texts.push({ x: CX0, y: Y + 430, s: 'only warm light in the deep, so the player reads it instantly.', size: 21, family: 'Nimbus Sans, sans-serif', fill: NOTE });
texts.push({ x: CX0, y: Y + 470, s: 'Ink #2A140F · paper #F3EBDD', size: 19, family: 'Nimbus Sans, sans-serif', fill: NOTE });
Y += RH + 40;
const SH = Y + 10;

// hero callouts (anatomy)
const anchorPt = (re, fx = .5, fy = .5) => { const its = heroItems.filter((i) => re.test(i.id || '') && i.pts); if (!its.length) return null; const b = bbox(its.flatMap((i) => i.pts)); return [hero.x + b.x0 + b.w * fx, hero.y + b.y0 + b.h * fy]; };
const call = (re, fx, fy, side, ty, s) => {
  const p = anchorPt(re, fx, fy); if (!p) return;
  const X = side === 'L' ? hero.x + 24 : hero.x + hero.w - 24, Yy = hero.y + ty, anchor = side === 'L' ? 'start' : 'end';
  const tw = s.length * 10.6, ex = side === 'L' ? X + tw + 8 : X - tw - 8;
  strokesOut.push({ lines: [[[ex, Yy - 7], [ex + (side === 'L' ? 20 : -20), Yy - 7], p]], col: '#9A7A6A', w: 1.5 });
  flats.push({ polys: [ell(p[0], p[1], 4.5, 4.5, 10)], col: '#9A7A6A' });
  texts.push({ x: X, y: Yy, s, size: 22, family: 'Nimbus Sans, sans-serif', fill: NOTE, anchor });
};
call(/^leg-R4-dactyl-paddle/, .3, .3, 'L', 150, 'swimming paddle = 5th leg pair (Portunidae)');
call(/^carapace$/, .035, .6, 'L', 320, 'anterolateral teeth + lateral spine');
call(/^antennule-L/, .5, .2, 'R', 330, 'antennule (1st antennae, folded)');
call(/^maxilliped3-R$/, .4, .55, 'L', 470, '3rd maxillipeds: the mouth’s double door');
call(/^cheliped-R-propodus-fixedfinger-tip/, .5, .5, 'L', 1060, 'CRUSHER (right claw): molar teeth');
call(/^eye-L-cornea/, .8, .3, 'R', 150, 'eye on a stalk · folds into its orbit');
call(/^antenna-L/, .9, .2, 'R', 240, 'antenna (2nd pair)');
call(/^leg-L3-carpus/, .5, .5, 'R', 420, 'walking legs 1–3: merus · carpus · propodus · dactyl');
call(/^leg-L1-dactyl/, .5, .9, 'R', 980, 'dactyl tips grip the substrate');
call(/^cheliped-L-dactyl$/, .5, .3, 'R', 1060, 'CUTTER (left claw): slimmer · only the dactyl moves');

// labels for tiles
for (const t of tiles) {
  if (t.label) texts.push({ x: t.x + 16, y: t.y + 32, s: t.label, size: t.id === 'hero' ? 30 : 21, weight: 'bold', family: 'Nimbus Sans, sans-serif', fill: t.palName === 'deep' || t.palName === 'mid' ? '#EAF2F2' : INK });
  if (t.note) texts.push({ x: t.x + 16, y: t.y + t.h - 16, s: t.note, size: t.id === 'hero' ? 24 : 18, family: 'Nimbus Sans, sans-serif', fill: t.palName === 'deep' || t.palName === 'mid' ? '#CFE3E3' : NOTE, style: 'italic' });
}

// ---------- write the scene for the painter (tile-local coords) ----------
const scene = {
  W: SW, H: SH, bg: BG, rects, blits, flats, strokes: strokesOut, texts,
  tiles: tiles.map((t) => ({ id: t.id, x: t.x, y: t.y, w: t.w, h: t.h, bg: t.bg, pal: t.pal, k: t.k, light: t.light, items: t.items })),
};
fs.writeFileSync(scenePath, JSON.stringify(scene));

// ---------- SVG sheet ----------
const svg = new SvgOut(SW, SH, BG);
for (const r of rects.filter((r) => !r.over)) svg.raw(`<rect x="${r.x}" y="${r.y}" width="${r.w}" height="${r.h}" fill="${r.fill}"/>`);
for (const t of tiles) {
  svg.raw(`<g id="panel-${t.id}" transform="translate(${t.x} ${t.y})">`);
  svg.raw(`<rect width="${t.w}" height="${t.h}" fill="${t.bg}"/>`);
  svg.raw(`<clipPath id="clip-${t.id}"><rect width="${t.w}" height="${t.h}"/></clipPath><g clip-path="url(#clip-${t.id})">`);
  const pal = t.pal;
  for (const it of t.items) {
    if (it.k === 'wash') svg.raw(`<path d="M${it.pts.map((p) => p.map((v) => v.toFixed(1)).join(' ')).join('L')}Z" fill="${it.col}" fill-opacity="${it.op ?? 1}"/>`);
    else if (it.k === 'fill') svg.raw(`<path d="M${it.pts.map((p) => p.map((v) => v.toFixed(1)).join(' ')).join('L')}Z" fill="${it.col}" fill-opacity="${(it.op ?? 90) / 255 * 1.4}"/>`);
  }
  svg.items(t.items.filter((it) => it.k !== 'wash' && it.k !== 'fill'), pal, { light: t.light, scale: t.k, idPrefix: `${t.id}--` });
  svg.raw('</g></g>');
}
for (const b of blits) { // SVG version of the downscale strip: reuse the hero group scaled
  const k = b.dst[3] / b.src[3];
  svg.raw(`<use href="#panel-hero" transform="translate(${b.dst[0]} ${b.dst[1]}) scale(${k}) translate(${-b.src[0]} ${-b.src[1]})" clip-path="none"/>`);
}
for (const f of flats) for (const P of f.polys) svg.raw(`<path d="M${P.map((p) => p.map((v) => v.toFixed(1)).join(' ')).join('L')}Z" fill="${f.col}"/>`);
for (const s of strokesOut) for (const P of s.lines) svg.raw(`<path d="M${P.map((p) => p.map((v) => v.toFixed(1)).join(' ')).join('L')}" fill="none" stroke="${s.col}" stroke-width="${s.w}" stroke-linecap="round"/>`);
for (const r of rects.filter((r) => r.over)) svg.raw(`<rect x="${r.x}" y="${r.y}" width="${r.w}" height="${r.h}" fill="${r.fill}"/>`);
for (const t of texts) svg.text(t.x, t.y, t.s, { size: t.size, fill: t.fill || INK, weight: t.weight || 'normal', anchor: t.anchor || 'start', family: t.family, style: t.style ? `font-style="${t.style}"` : '' });
fs.writeFileSync(path.join(outDir, 'model-sheet.svg'), svg.toString());

// ---------- rig-ready parts (front view, neutral) ----------
{
  const items = buildFront(POSES.hero);
  const b = bbox(items.filter((i) => i.pts).flatMap((i) => i.pts));
  const k = 2, W = Math.ceil(b.w * k + 120), H = Math.ceil(b.h * k + 160);
  const M = chain(T.tr(60 - b.x0 * k, 100 - b.y0 * k), T.sc(k));
  const its = transformItems(items, M).map((it) => ({ ...it, pv: it.pv ? T.ap(M, it.pv) : undefined }));
  const s = new SvgOut(W, H, null);
  s.raw(`<!-- CrabScape player crab: rig-ready cut-out parts, front (in-game) view, neutral pose.
  One <g> per part. data-pivot = the joint this part rotates about (its proximal end), in this file's pixels.
  data-parent = the part it hangs from. Draw order = document order. R/L = the crab's own right/left
  (R is screen-left in this view). Legs: 1-3 walking legs, 4 = swimming paddle (the 5th pereiopod). -->`);
  s.text(20, 40, 'CrabScape player crab · rig parts (front view)', { size: 22, family: 'Nimbus Sans, sans-serif' });
  // group per part id (one shape per group, lines too); add pivot + parent attributes
  const parentOf = (id) => {
    let m;
    if ((m = /^(leg-[RL]4)-paddle-rib/.exec(id))) return `${m[1]}-dactyl-paddle`;
    if ((m = /^(cheliped-[RL])-(merus)-spine/.exec(id))) return `${m[1]}-merus`;
    if ((m = /^(cheliped-[RL])-carpus-spine/.exec(id))) return `${m[1]}-carpus`;
    if ((m = /^(cheliped-[RL])-(palm-ridge|molar|propodus-fixedfinger)/.exec(id))) return `${m[1]}-propodus-palm`;
    if ((m = /^(cheliped-[RL])-dactyl-tip/.exec(id))) return `${m[1]}-dactyl`;
    if ((m = /^(eye-[RL])-(cornea|glint)/.exec(id))) return `${m[1]}-stalk`;
    if (/^maxilliped3-[RL]-suture/.test(id)) return id.replace('-suture', '');
    if ((m = /^(leg-[RL]\d)-(merus|carpus|propodus|dactyl)/.exec(id))) return { merus: 'carapace', carpus: `${m[1]}-merus`, propodus: `${m[1]}-carpus`, dactyl: `${m[1]}-propodus` }[m[2]];
    if ((m = /^(cheliped-[RL])-(merus|carpus|propodus|dactyl)/.exec(id))) return { merus: 'carapace', carpus: `${m[1]}-merus`, propodus: `${m[1]}-carpus`, dactyl: `${m[1]}-propodus-palm` }[m[2]];
    if (/^eye-|^antenn|^maxilliped|^face|^underside|^gear-helmet/.test(id)) return 'carapace';
    return id === 'carapace' ? 'root' : 'carapace';
  };
  const P = PALETTES.normal;
  for (const it of its) {
    const pv = it.pv ? ` data-pivot="${it.pv[0].toFixed(1)},${it.pv[1].toFixed(1)}"` : '';
    s.raw(`<g id="${it.id}" data-parent="${parentOf(it.id)}"${pv}>`);
    s.items([{ ...it, g: undefined }], P, { scale: k, idPrefix: 'p-' });
    s.raw('</g>');
  }
  // pivots as tiny markers in a hidden layer
  s.raw('<g id="pivots" style="display:none">');
  for (const it of its) if (it.pv) s.raw(`<circle cx="${it.pv[0].toFixed(1)}" cy="${it.pv[1].toFixed(1)}" r="3" fill="#00A0FF"><title>${it.id}</title></circle>`);
  s.raw('</g>');
  fs.writeFileSync(path.join(outDir, 'crab-parts.svg'), s.toString().replace(/<g id="p-misc" data-part="misc">/g, '<g>'));
}

// ---------- leg / claw counts per view ----------
console.log('view counts (legs = walking legs + paddles, claws = chelipeds):');
for (const c of counts) console.log(`  ${c.tile.padEnd(16)} ${c.view.padEnd(6)} legs ${c.legs}  claws ${c.claws}${c.legs === 8 && c.claws === 2 ? '' : '   <-- CHECK'}`);
console.log('sheet', SW, 'x', SH);
