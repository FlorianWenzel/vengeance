import { test, expect, type APIRequestContext } from '@playwright/test';
import { loginAsAdmin } from './helpers';

const run = Date.now().toString(36);

async function rpc(request: APIRequestContext, token: string, method: string, params: unknown = {}) {
  const res = await request.post('/mcp', {
    headers: {
      Authorization: `Bearer ${token}`,
      'Content-Type': 'application/json',
      Accept: 'application/json, text/event-stream',
      'MCP-Protocol-Version': '2025-06-18',
    },
    data: { jsonrpc: '2.0', id: 1, method, params },
  });
  return { status: res.status(), body: res.status() === 200 ? await res.json() : await res.text() };
}

test('an AI assistant connects over MCP, adds an asset, and loses access when revoked', async ({ page, request }) => {
  await loginAsAdmin(page);
  await page.getByTestId('nav-ai').click();
  await page.getByTestId('ai-token-new').click();
  await page.getByTestId('ai-token-name').fill(`claude-${run}`);
  await page.getByTestId('ai-token-scope-write').check();
  await page.getByTestId('ai-token-save').click();
  const token = ((await page.getByTestId('ai-token-value').textContent()) ?? '').trim();
  expect(token).toMatch(/^vgp_/);
  await expect(page.getByTestId('ai-claude-code')).toContainText('claude mcp add --transport http vengeance');

  const init = await rpc(request, token, 'initialize', { protocolVersion: '2025-06-18', capabilities: {}, clientInfo: { name: 'e2e', version: '1' } });
  expect(init.status).toBe(200);
  expect(init.body.result.serverInfo.name).toBe('vengeance');

  const tools = await rpc(request, token, 'tools/list');
  const names = tools.body.result.tools.map((t: { name: string }) => t.name);
  expect(names).toEqual(expect.arrayContaining(['vengeance_overview', 'list_findings', 'create_asset', 'triage_findings']));

  const created = await rpc(request, token, 'tools/call', {
    name: 'create_asset',
    arguments: { kind: 'repository', identifier: `github.com/e2e/from-mcp-${run}`, project: `mcp-${run}`, tags: ['prod'] },
  });
  expect(created.body.result.isError ?? false).toBe(false);

  await page.keyboard.press('Escape');
  await page.getByTestId('nav-assets').click();
  await expect(page.locator(`[data-testid=asset-row][data-identifier="github.com/e2e/from-mcp-${run}"]`)).toBeVisible();

  // last used shows up, then revoke
  await page.getByTestId('nav-ai').click();
  const row = page.locator(`[data-testid=ai-token-row][data-name="claude-${run}"]`);
  await expect(row).not.toContainText('never');
  await row.getByTestId('ai-token-revoke').click();
  await page.getByTestId('ai-token-revoke-confirm').click();
  await expect(row).toHaveCount(0);
  const after = await rpc(request, token, 'tools/list');
  expect(after.status).toBe(401);
});
