// Dev helper: walk-mode screenshots. Needs a server on :8765 serving the repo root.
//   node viz/demo/shoot-walk.mjs [outDir]
import { createRequire } from 'node:module';
import { fileURLToPath } from 'node:url';
import path from 'node:path';
const here = path.dirname(fileURLToPath(import.meta.url));
const require = createRequire(path.join(here, '../../e2e/package.json'));
const { chromium } = require('@playwright/test');
const out = process.argv[2] || '/tmp/blueprint-city';
const base = process.env.BASE || 'http://localhost:8765/viz/demo/';
// Each shot: [name, scene, steps]; a step is ['key', code, ms] | ['look', dx, dy] | ['focus', id] | ['wait', ms] | ['interact']
const shots = [
  ['city-arrival', '', []],
  ['city-street', '', [['key', 'KeyW', 2500], ['look', -60, -40]]],
  ['city-facade-ghost', '', [['focus', 'wl:slowbloom/ghost/Deployment/ghost'], ['wait', 1300]]],
  ['city-facade-slowbloom', '', [['focus', 'wl:slowbloom/slowbloom/Deployment/api'], ['wait', 1300]]],
  ['city-doors', '', [['focus', 'asset:7'], ['wait', 1300], ['key', 'KeyS', 900]]],
  ['city-library', '', [['focus', 'asset:5'], ['wait', 1300], ['key', 'KeyS', 3800], ['look', 0, 60]]],
  ['city-library-books', '', [['focus', 'asset:5'], ['wait', 1300]]],
  ['city-harbour', '', [['focus', 'asset:10'], ['wait', 1300], ['look', -260, 90]]],
  ['city-crate-fire', '', [['focus', 'img:152'], ['wait', 1300]]],
  ['city-searchlight', '', [['look', 120, -520], ['wait', 300]]],
  ['city-square-door', '', [['focus', 'asset:11'], ['wait', 1300], ['key', 'KeyS', 600]]],
  ['city-many', 'many-hosts.json', [['key', 'KeyW', 3500], ['look', 80, -60]]],
];
const browser = await chromium.launch({ args: ['--use-angle=swiftshader', '--enable-unsafe-swiftshader', '--ignore-gpu-blocklist'] });
const page = await browser.newPage({ viewport: { width: 1400, height: 860 }, deviceScaleFactor: 2 });
page.on('pageerror', (e) => console.log('[pageerror]', e.message));
page.on('console', (m) => { if (!m.text().includes('GL Driver')) console.log('[page]', m.text()); });
for (const [name, scene, steps] of shots) {
  if (process.argv[3] && !name.includes(process.argv[3])) continue;
  await page.goto(base + '?mode=walk&nolock=1' + (scene ? `&scene=${scene}` : ''));
  await page.waitForFunction(() => window.blueprintReady === true, null, { timeout: 15000 });
  await page.waitForTimeout(300);
  for (const s of steps) {
    if (s[0] === 'key') { await page.evaluate((c) => window.bp.key_down(c), s[1]); await page.waitForTimeout(s[2]); await page.evaluate((c) => window.bp.key_up(c), s[1]); }
    if (s[0] === 'look') await page.evaluate(([x, y]) => window.bp.look(x, y), [s[1], s[2]]);
    if (s[0] === 'focus') await page.evaluate((id) => window.bp.focus(id), s[1]);
    if (s[0] === 'interact') await page.evaluate(() => window.bp.interact());
    if (s[0] === 'wait') await page.waitForTimeout(s[1]);
  }
  await page.waitForTimeout(700);
  await page.screenshot({ path: path.join(out, `${name}.png`) });
  console.log('shot', name);
}
await browser.close();
