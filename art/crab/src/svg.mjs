// SVG backend: per-shape wobble on the ink line, per-shape light gradient, heavier line weight on the shadow side.
import { T, bbox, hashStr, rng, norm, sub, perp, through } from './geom.mjs';

const f2 = (v) => (Math.round(v * 10) / 10).toString();
const esc = (s) => String(s).replace(/&/g, '&amp;').replace(/</g, '&lt;');

export function mixHex(a, b, k) {
  const pa = parseInt(a.slice(1), 16), pb = parseInt(b.slice(1), 16);
  const c = [16, 8, 0].map((s) => Math.round(((pa >> s) & 255) * (1 - k) + ((pb >> s) & 255) * k));
  return '#' + c.map((v) => v.toString(16).padStart(2, '0')).join('');
}

// Resolve a material for a shape: [lit, base, shade]
export function matCols(P, it) {
  let m = P[it.mat] || P.leg;
  if (typeof m === 'string') m = [m, m, m];
  if (it.far) m = m.map((c) => mixHex(c, P.shadow, .22));
  if (it.mid) m = m.map((c) => mixHex(c, P.shadow, .1)); // v0.2: middle walking leg, slight recession
  if (it.shade) m = [m[2], mixHex(m[1], m[2], .6), m[2]];
  if (it.under) m = [mixHex(m[1], m[2], .2), mixHex(m[1], m[2], .55), mixHex(m[2], '#3A1A12', .3)];
  return m;
}

// small, low-frequency wobble along the normal, seeded by the shape id (so it stays with the shape)
export function wobble(pts, id, amp) {
  const r = rng(hashStr(id)), n = pts.length, ph = r() * 6.28, ph2 = r() * 6.28, f1 = 2 + Math.floor(r() * 3), f2_ = 5 + Math.floor(r() * 4);
  return pts.map((p, i) => {
    const a = pts[(i - 1 + n) % n], b = pts[(i + 1) % n], nn = perp(norm(sub(b, a))), t = i / n * 6.283;
    const w = amp * (Math.sin(t * f1 + ph) * .7 + Math.sin(t * f2_ + ph2) * .3);
    return [p[0] + nn[0] * w, p[1] + nn[1] * w];
  });
}

export class SvgOut {
  constructor(w, h, bg) { this.w = w; this.h = h; this.defs = []; this.body = []; this.n = 0; if (bg) this.body.push(`<rect width="${w}" height="${h}" fill="${bg}"/>`); }
  open(tag) { this.body.push(tag); }
  close(tag = '</g>') { this.body.push(tag); }
  raw(s) { this.body.push(s); }
  // items already in output coordinates (after transform)
  items(items, P, { light = [-.25, -1], ink = 1, scale = 1, idPrefix = '' } = {}) {
    const L = norm(light);
    let groupOpen = null; const gname = (it) => it.g || 'misc';
    for (const it of items) {
      if (gname(it) !== groupOpen) { if (groupOpen !== null) this.body.push('</g>'); this.body.push(`<g id="${esc(idPrefix + gname(it))}" data-part="${esc(gname(it))}">`); groupOpen = gname(it); }
      if (it.k === 'shape') this.shape(it, P, L, ink * scale, idPrefix);
      else if (it.k === 'line') this.line(it, P, ink * scale, idPrefix);
      else if (it.k === 'glow') this.glow(it, P, scale);
    }
    if (groupOpen !== null) this.body.push('</g>');
  }
  shape(it, P, L, inkK, pre) {
    const id = `${pre}${it.id}`.replace(/[^\w-]/g, '_') + '_' + (this.n++);
    const cols = matCols(P, it);
    const b = bbox(it.pts);
    // gradient along the light direction across this shape's own extent
    const c = [b.cx, b.cy], ext = Math.abs(L[0]) * b.w / 2 + Math.abs(L[1]) * b.h / 2;
    const g1 = [c[0] + L[0] * ext, c[1] + L[1] * ext], g2 = [c[0] - L[0] * ext, c[1] - L[1] * ext];
    this.defs.push(`<linearGradient id="g${id}" gradientUnits="userSpaceOnUse" x1="${f2(g1[0])}" y1="${f2(g1[1])}" x2="${f2(g2[0])}" y2="${f2(g2[1])}"><stop offset="0" stop-color="${cols[0]}"/><stop offset=".48" stop-color="${cols[1]}"/><stop offset="1" stop-color="${cols[2]}"/></linearGradient>`);
    const wob = wobble(it.pts, it.id, Math.min(1.2, .25 + b.w * .004) * Math.max(.4, inkK));
    const d = 'M' + wob.map((p) => f2(p[0]) + ' ' + f2(p[1])).join('L') + 'Z';
    const op = it.op != null ? ` fill-opacity="${it.op}"` : '';
    this.body.push(`<path id="${id}" d="${d}" fill="url(#g${id})"${op}/>`);
    if (it.ink === null) return;
    const sw = (it.sw ?? 1) * 1.6 * inkK;
    const inkCol = it.far ? mixHex(P.ink, P.shadow, .3) : P.ink;
    this.body.push(`<path d="${d}" fill="none" stroke="${inkCol}" stroke-width="${f2(sw)}" stroke-linejoin="round"/>`);
    // heavier weight on the side facing away from the light
    const n = wob.length, runs = []; let run = null;
    for (let i = 0; i < n; i++) {
      const a = wob[(i - 1 + n) % n], bb = wob[(i + 1) % n], t = norm(sub(bb, a)), nn = [t[1], -t[0]]; // outward normal for CW-in-screen
      const area = signedArea(wob), out = area > 0 ? nn : [-nn[0], -nn[1]];
      const away = out[0] * L[0] + out[1] * L[1] < -.35;
      if (away) { if (!run) { run = []; runs.push(run); } run.push(wob[i]); } else run = null;
    }
    if (runs.length > 1 && runs[0][0] === wob[0] && runs[runs.length - 1].slice(-1)[0] === wob[n - 1]) runs[0] = runs.pop().concat(runs[0]);
    for (const r of runs) if (r.length > 2) this.body.push(`<path d="M${r.map((p) => f2(p[0]) + ' ' + f2(p[1])).join('L')}" fill="none" stroke="${inkCol}" stroke-width="${f2(sw * 1.9)}" stroke-linecap="round" stroke-linejoin="round"/>`);
  }
  line(it, P, inkK) {
    const col = it.mat === 'ink' ? P.ink : (P[it.mat] ? (Array.isArray(P[it.mat]) ? P[it.mat][1] : P[it.mat]) : it.mat);
    const pts = it.pts.length > 2 ? through(it.pts, 5) : it.pts;
    this.body.push(`<path d="M${pts.map((p) => f2(p[0]) + ' ' + f2(p[1])).join('L')}" fill="none" stroke="${col}" stroke-width="${f2((it.sw ?? 1) * 1.6 * inkK)}" stroke-linecap="round" stroke-linejoin="round"/>`);
  }
  glow(it, P, scale) {
    const id = 'glow' + (this.n++);
    this.defs.push(`<radialGradient id="${id}"><stop offset="0" stop-color="${it.col}" stop-opacity=".55"/><stop offset=".35" stop-color="${it.col}" stop-opacity=".2"/><stop offset="1" stop-color="${it.col}" stop-opacity="0"/></radialGradient>`);
    this.body.push(`<circle cx="${f2(it.x)}" cy="${f2(it.y)}" r="${f2(it.r)}" fill="url(#${id})" style="mix-blend-mode:screen"/>`);
  }
  text(x, y, s, { size = 20, fill = '#2A140F', weight = 'normal', anchor = 'start', family = 'C059, serif', style = '' } = {}) {
    this.body.push(`<text x="${f2(x)}" y="${f2(y)}" font-family="${family}" font-size="${size}" font-weight="${weight}" fill="${fill}" text-anchor="${anchor}" ${style}>${esc(s)}</text>`);
  }
  toString() {
    return `<svg xmlns="http://www.w3.org/2000/svg" width="${this.w}" height="${this.h}" viewBox="0 0 ${this.w} ${this.h}">\n<defs>\n${this.defs.join('\n')}\n</defs>\n${this.body.join('\n')}\n</svg>\n`;
  }
}
export function signedArea(P) { let a = 0; for (let i = 0; i < P.length; i++) { const p = P[i], q = P[(i + 1) % P.length]; a += p[0] * q[1] - q[0] * p[1]; } return a / 2; }

export function transformItems(items, M) {
  return items.map((it) => {
    if (it.k === 'glow') { const p = T.ap(M, [it.x, it.y]); return { ...it, x: p[0], y: p[1], r: it.r * Math.hypot(M[0], M[1]) }; }
    return { ...it, pts: it.pts.map((p) => T.ap(M, p)) };
  });
}
