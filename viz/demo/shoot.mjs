// Dev helper: screenshots of the demo page. Needs a server on :8765 serving the repo root.
//   node viz/demo/shoot.mjs [outDir]
import { createRequire } from 'node:module';
import { fileURLToPath } from 'node:url';
import path from 'node:path';
const here = path.dirname(fileURLToPath(import.meta.url));
const require = createRequire(path.join(here, '../../e2e/package.json'));
const { chromium } = require('@playwright/test');
const out = process.argv[2] || '/tmp/blueprint-shots';
const base = process.env.BASE || 'http://localhost:8765/viz/demo/';
const shots = [
  ['overview', ''],
  ['host', 'host:12'],
  ['group', 'ns:slowbloom/slowbloom'],
  ['group-monitoring', 'ns:slowbloom/monitoring'],
  ['workload', 'wl:slowbloom/slowbloom/Deployment/api'],
  ['many', '', 'many-hosts.json'],
];
const browser = await chromium.launch({ args: ['--use-angle=swiftshader', '--enable-unsafe-swiftshader', '--ignore-gpu-blocklist'] });
const page = await browser.newPage({ viewport: { width: 1400, height: 860 }, deviceScaleFactor: 2 });
page.on('console', (m) => console.log('[page]', m.text()));
page.on('pageerror', (e) => console.log('[pageerror]', e.message));
for (const [name, focus, scene] of shots) {
  await page.goto(base + '?' + (focus ? `focus=${encodeURIComponent(focus)}` : '') + (scene ? `&scene=${scene}` : ''));
  await page.waitForFunction(() => window.blueprintReady === true, null, { timeout: 15000 });
  await page.waitForTimeout(1600);
  if (process.env.HOVER && name === 'group') await page.mouse.move(700, 430);
  await page.screenshot({ path: path.join(out, `${name}.png`) });
  console.log('shot', name);
}
await browser.close();
