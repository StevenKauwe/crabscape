// node paint.mjs scene.json out.png  — paints the scene with p5.brush in headless Chromium (run under `ulimit -c 0`)
import { createRequire } from 'module';
import fs from 'fs';
import path from 'path';
import { fileURLToPath } from 'url';
const here = path.dirname(fileURLToPath(import.meta.url));
// puppeteer-core comes from the music-to-movie-magic checkout next to this repo (or set M2M_DIR).
const m2m = process.env.M2M_DIR || path.join(here, '../../../../music-to-movie-magic');
const require = createRequire(path.join(m2m, 'package.json'));
const puppeteer = require('puppeteer-core');
const [scenePath, outPath] = process.argv.slice(2);
const scene = JSON.parse(fs.readFileSync(scenePath, 'utf8'));
const browser = await puppeteer.launch({ executablePath: '/usr/bin/chromium', headless: 'new',
  args: ['--no-sandbox', '--allow-file-access-from-files', '--use-angle=vulkan', '--enable-gpu', '--ignore-gpu-blocklist', '--enable-unsafe-swiftshader', '--disable-crash-reporter', '--disable-breakpad'] });
try {
  const page = await browser.newPage();
  page.on('pageerror', (e) => console.error('pageerror', e.message));
  page.on('console', (m) => { if (m.type() === 'error') console.error('console', m.text()); });
  await page.goto('file://' + path.join(here, 'brush.html'), { waitUntil: 'load' });
  await page.waitForFunction('window.ready === true', { timeout: 60000 });
  await page.evaluate((w, h, bg) => window.initSheet(w, h, bg), scene.W, scene.H, scene.bg);
  if (scene.rects) await page.evaluate((r) => window.rects(r), scene.rects.filter((r) => !r.over));
  const t0 = Date.now();
  for (const [i, t] of scene.tiles.entries()) {
    const t1 = Date.now();
    await page.evaluate(async (t) => await window.renderTile(t), t);
    if (process.env.V) console.log(`tile ${i} ${t.id} ${Date.now() - t1} ms`);
  }
  console.log(`${scene.tiles.length} tiles in ${((Date.now() - t0) / 1000).toFixed(1)} s`);
  for (const b of scene.blits || []) await page.evaluate((s, d) => window.blit(s, d), b.src, b.dst);
  for (const f of scene.flats || []) await page.evaluate((p, c) => window.flat(p, c), f.polys, f.col);
  for (const s of scene.strokes || []) await page.evaluate((p, c, w) => window.strokes(p, c, w), s.lines, s.col, s.w);
  if (scene.rects) await page.evaluate((r) => window.rects(r), scene.rects.filter((r) => r.over));
  await page.evaluate((t) => window.texts(t), scene.texts || []);
  const url = await page.evaluate(() => window.sheetURL());
  fs.writeFileSync(outPath, Buffer.from(url.split(',')[1], 'base64'));
} finally { await browser.close(); }
