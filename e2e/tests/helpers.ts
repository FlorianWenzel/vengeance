import { expect, type Page, type APIRequestContext } from '@playwright/test';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';

const here = dirname(fileURLToPath(import.meta.url));
export const fixture = (name: string) => readFileSync(join(here, '../../internal/ingest/testdata', name));

export const MAILPIT = process.env.MAILPIT_URL ?? 'http://localhost:8025';

export async function login(page: Page, email = 'admin@example.com', next = '/') {
  await page.goto(next);
  // Dex local password form
  await page.waitForURL(/\/dex\//);
  await page.locator('input[name=login]').fill(email);
  await page.locator('input[name=password]').fill('password');
  await page.locator('button[type=submit]').click();
}

export async function loginAsAdmin(page: Page, next = '/') {
  await login(page, 'admin@example.com', next);
  await expect(page.getByTestId('user-email')).toHaveText('admin@example.com');
}

export async function ingest(request: APIRequestContext, token: string, file: string, query: Record<string, string>) {
  const qs = new URLSearchParams({ format: 'trivy', ...query }).toString();
  const res = await request.post(`/api/v1/ingest?${qs}`, {
    headers: { Authorization: `Bearer ${token}`, 'Content-Type': 'application/json' },
    data: fixture(file),
  });
  expect(res.status(), await res.text()).toBe(200);
  return res.json();
}

/** Calls the UI API with the page's session cookie. */
export async function api(page: Page, method: string, path: string, data?: unknown) {
  const res = await page.request.fetch(path, {
    method,
    headers: { 'X-Vengeance-CSRF': '1', 'Content-Type': 'application/json' },
    data: data === undefined ? undefined : JSON.stringify(data),
  });
  const text = await res.text();
  expect(res.ok(), `${method} ${path}: ${res.status()} ${text}`).toBeTruthy();
  return text ? JSON.parse(text) : null;
}

export async function mailpitClear(request: APIRequestContext) {
  await request.delete(`${MAILPIT}/api/v1/messages`);
}

export async function mailpitWaitFor(request: APIRequestContext, subjectPart: string, timeoutMs = 45_000) {
  const until = Date.now() + timeoutMs;
  while (Date.now() < until) {
    const res = await request.get(`${MAILPIT}/api/v1/messages`);
    const body = await res.json();
    const hit = (body.messages ?? []).find((m: { Subject: string }) => m.Subject.includes(subjectPart));
    if (hit) return hit;
    await new Promise((r) => setTimeout(r, 1000));
  }
  throw new Error(`no mail with subject containing "${subjectPart}" within ${timeoutMs}ms`);
}
