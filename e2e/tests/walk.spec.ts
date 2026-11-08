import { test, expect } from '@playwright/test';
import { api, loginAsAdmin } from './helpers';

const run = Date.now().toString(36);

test('walk mode: enter the datacenter view, see the floor plan, switch back', async ({ page, request }) => {
  await loginAsAdmin(page);
  const src = await api(page, 'POST', '/api/v1/sources', { name: `walk-${run}` });
  const res = await request.post('/api/v1/topology', {
    headers: { Authorization: `Bearer ${src.token}` },
    data: { cluster: `walk-${run}`, nodes: [{ name: `w-${run}`, host: `host:w-${run}` }],
      workloads: [{ namespace: 'shop', name: 'api', kind: 'Deployment', node: `w-${run}`, ready: 1, desired: 1, containers: [{ name: 'api', image: 'nginx:1' }] }] },
  });
  expect(res.status()).toBe(200);

  await page.goto('/blueprint');
  await expect(page.getByTestId('blueprint-canvas')).toBeVisible();
  await page.getByTestId('blueprint-view-walk').click();
  await expect(page.getByTestId('blueprint-enter-walk')).toBeVisible();
  await expect(page.getByTestId('blueprint-minimap')).toBeVisible();
  // the floor plan is drawn (non-empty pixels)
  await expect.poll(async () => page.getByTestId('blueprint-minimap').evaluate((c: HTMLCanvasElement) => {
    const d = c.getContext('2d')!.getImageData(0, 0, c.width, c.height).data;
    let n = 0;
    for (let i = 3; i < d.length; i += 4) if (d[i] > 0) n++;
    return n;
  })).toBeGreaterThan(100);
  await page.getByTestId('blueprint-view-3d').click();
  await expect(page.getByTestId('blueprint-enter-walk')).toHaveCount(0);
  await expect(page.locator('.bp-label').first()).toBeVisible();
});
