// Dev helper: walk-mode screenshots. Needs a server on :8765 serving the repo root.
//   node viz/demo/shoot-walk.mjs [outDir]
import { createRequire } from 'node:module';
import { fileURLToPath } from 'node:url';
import path from 'node:path';
const here = path.dirname(fileURLToPath(import.meta.url));
const require = createRequire(path.join(here, '../../e2e/package.json'));
const { chromium } = require('@playwright/test');
const out = process.argv[2] || '/tmp/blueprint-walk';
const base = process.env.BASE || 'http://localhost:8765/viz/demo/';
// Each shot: [name, scene, steps]; a step is ['key', code, ms] | ['look', dx, dy] | ['focus', id] | ['wait', ms] | ['interact']
const shots = [
  ['entrance', '', []],
  ['approach', '', [['key', 'KeyW', 1100]]],
  ['rack-close', '', [['focus', 'ns:slowbloom/slowbloom'], ['wait', 1300]]],
  ['workload-close', '', [['focus', 'wl:slowbloom/slowbloom/Deployment/api'], ['wait', 1300], ['interact']]],
  ['library', '', [['look', -330, 60], ['key', 'KeyW', 400]]],
  ['uplinks', '', [['key', 'KeyW', 900], ['look', 260, -40]]],
  ['looking-back', '', [['key', 'KeyW', 1400], ['key', 'KeyD', 900], ['look', 700, 0]]],
  ['many-rows', 'many-hosts.json', [['key', 'KeyD', 800], ['key', 'KeyW', 900], ['look', -120, 0]]],
  ['many-aisle', 'many-hosts.json', [['key', 'KeyA', 1300], ['key', 'KeyW', 2600], ['look', 480, 0], ['key', 'KeyW', 1500]]],
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
