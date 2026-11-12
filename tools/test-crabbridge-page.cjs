// Packaged peer frontend: nested routes and fail-closed deployment-header policy.
const { chromium } = require(process.env.PLAYWRIGHT_MODULE || 'playwright');
const http = require('http'), fs = require('fs'), path = require('path');
const root = path.resolve(process.argv[2] || 'web/crabbridge');
let browser, policy, headerMode = 'valid', holdHead = false, releaseHead, headArrived;
const assertLocked = async page => {
  if (await page.locator('#deployment-controls').getAttribute('disabled') === null) throw Error('Deployment fieldset unlocked');
  const enabled = await page.locator('input,button,select,textarea').evaluateAll(nodes => nodes.filter(node => !node.matches(':disabled')).map(node => node.id));
  if (enabled.length) throw Error('Enabled controls survived lock: ' + enabled.join(','));
};
const server = http.createServer((req, res) => {
  const prefix = '/pr-fixture/crabbridge/';
  if (!req.url.startsWith(prefix) || !['GET', 'HEAD'].includes(req.method)) { res.writeHead(404); return res.end(); }
  const name = req.url.slice(prefix.length) || 'index.html';
  if (name.includes('..')) { res.writeHead(404); return res.end(); }
  if (headerMode !== 'missing') {
    res.setHeader('Content-Security-Policy', policy.DEVELOPMENT_CSP);
    res.setHeader('X-Frame-Options', 'DENY');
    res.setHeader('Cache-Control', 'no-store');
    res.setHeader('Referrer-Policy', 'no-referrer');
    res.setHeader('X-Content-Type-Options', 'nosniff');
    res.setHeader('Permissions-Policy', 'camera=(), microphone=(), geolocation=()');
    if (headerMode !== 'incomplete') res.setHeader('X-CrabBridge-Policy', '1');
  }
  try {
    res.setHeader('Content-Type', name.endsWith('.wasm') ? 'application/wasm' : name.endsWith('.js') ? 'text/javascript' : name.endsWith('.css') ? 'text/css' : 'text/html');
    const content = fs.readFileSync(path.join(root, name));
    if (req.method === 'HEAD' && holdHead) { releaseHead = () => res.end(); headArrived?.(); }
    else res.end(req.method === 'HEAD' ? undefined : content);
  } catch { res.writeHead(404); res.end(); }
});
(async () => {
  try {
    policy = await import('data:text/javascript;base64,' + fs.readFileSync(path.join(root, 'deployment.js')).toString('base64'));
    if (!policy.DEVELOPMENT_CSP || !policy.PRODUCTION_CSP) throw Error('Deployment policy exports missing');
    await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
    browser = await chromium.launch({ headless: true, chromiumSandbox: true, ...(process.env.CHROME_BIN ? { executablePath: process.env.CHROME_BIN } : {}) });
    const url = `http://127.0.0.1:${server.address().port}/pr-fixture/crabbridge/`;
    for (const mode of ['missing', 'incomplete', 'valid']) {
      headerMode = mode;
      const page = await browser.newPage(), errors = [];
      page.on('pageerror', error => errors.push(String(error)));
      await page.goto(url);
      if (mode !== 'valid') {
        await page.waitForFunction(() => document.querySelector('#status')?.textContent.startsWith('Locked:'));
        for (const id of ['invite', 'password', 'offer', 'login']) {
          if (!await page.locator('#' + id).isDisabled()) throw Error(`${mode}: ${id} accepted input on untrusted deployment`);
        }
      } else {
        await page.waitForFunction(() => document.querySelector('#status')?.textContent.startsWith('Ready.'));
        if (await page.locator('#username').count()) throw Error('Local HTTP UI exposed');
        if (await page.locator('#invite').isDisabled()) throw Error('Verified peer invitation UI disabled');
        if (errors.length) throw Error(errors.join('\n'));
      }
      await page.close();
    }
    // HTML itself must fail closed before any script is allowed to execute.
    const noJs = await browser.newContext({ javaScriptEnabled: false });
    const noJsPage = await noJs.newPage();
    await noJsPage.goto(url);
    await assertLocked(noJsPage);
    await noJs.close();

    // Deterministic startup race: hold the policy HEAD response until controllerchange is delivered.
    holdHead = true;
    const arrived = new Promise(resolve => { headArrived = resolve; });
    const delayed = await browser.newPage();
    delayed.on('pageerror', () => {}); // Expected fail-closed startup rejection.
    await delayed.goto(url, { waitUntil: 'commit' });
    await Promise.race([arrived, new Promise((_, reject) => { const timer = setTimeout(() => reject(Error('Policy HEAD never arrived')), 10000); timer.unref(); })]);
    await delayed.evaluate(() => navigator.serviceWorker.dispatchEvent(new Event('controllerchange')));
    const response = delayed.waitForResponse(r => r.request().method() === 'HEAD');
    releaseHead();
    await response;
    await delayed.waitForFunction(() => document.querySelector('#status').textContent.startsWith('Locked:'));
    await assertLocked(delayed);
    await delayed.evaluate(() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))));
    await assertLocked(delayed);
    await delayed.close();
    holdHead = false;

    // A later controller change also permanently revokes the already-ready page.
    const ready = await browser.newPage();
    await ready.goto(url);
    await ready.waitForFunction(() => document.querySelector('#status').textContent.startsWith('Ready.'));
    await ready.evaluate(() => navigator.serviceWorker.dispatchEvent(new Event('controllerchange')));
    await assertLocked(ready);
    await ready.evaluate(() => { navigator.serviceWorker.dispatchEvent(new Event('controllerchange')); document.querySelector('#disconnect').click(); });
    await ready.evaluate(() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))));
    await assertLocked(ready);
    if (!(await ready.locator('#status').textContent()).startsWith('Locked:')) throw Error('Controller lock was overwritten');
    await ready.close();
    console.log('JS-disabled HTML and controllerchange during/after HEAD preserve sticky credential lock.');
    console.log('Packaged peer page: nested assets/WASM load; missing/incomplete headers lock credential controls; valid headers unlock.');
  } finally {
    releaseHead?.();
    if (browser) await browser.close();
    await new Promise(resolve => server.close(resolve));
  }
})().catch(error => { console.error(error); process.exitCode = 1; });
