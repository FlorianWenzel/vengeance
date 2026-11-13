import { test, expect } from "@playwright/test";
import {
  api,
  ingest,
  loginAsAdmin,
  mailpitClear,
  mailpitWaitFor,
} from "./helpers";

// One realistic journey, in order: create a source, push reports, triage, fix, inventory,
// coverage, notifications. Shares state, so it runs serially.
test.describe.configure({ mode: "serial" });

const run = Date.now().toString(36);
let token = "";

test("create a source and copy its token", async ({ page }) => {
  await loginAsAdmin(page);
  await page.getByTestId("nav-sources").click();
  await page.getByTestId("source-new").click();
  await page.getByTestId("source-name").fill(`ci-${run}`);
  await page.getByTestId("source-save").click();
  const tok = page.getByTestId("source-token");
  await expect(tok).toContainText("vg_");
  token =
    ((await tok.textContent()) ?? "").trim().match(/vg_[A-Za-z0-9_-]+/)?.[0] ??
    "";
  expect(token.length).toBeGreaterThan(20);
});

test("push reports and see findings, fixes and the asset", async ({
  page,
  request,
}) => {
  const img = await ingest(request, token, "image.json", {
    asset: `image:ghcr.io/e2e/api-${run}`,
    project: `proj-${run}`,
  });
  expect(img.new).toBeGreaterThan(40);
  const fs = await ingest(request, token, "fs.json", {
    asset: `repository:github.com/e2e/app-${run}`,
    project: `proj-${run}`,
  });
  expect(fs.new).toBeGreaterThan(10);

  await loginAsAdmin(page);
  await page.getByTestId("nav-findings").click();
  await expect(page.getByTestId("finding-row").first()).toBeVisible();
  const total = Number(
    (await page.getByTestId("findings-total").textContent())?.replace(
      /\D/g,
      "",
    ),
  );
  expect(total).toBeGreaterThanOrEqual(img.new + fs.new);

  // the planted secret in the fs fixture is a P1
  await page.getByTestId("findings-search").fill("generic-api-key");
  await expect(page.getByTestId("finding-row")).toHaveCount(1);
  await page.getByTestId("finding-row").first().click();
  await expect(page.getByTestId("finding-drawer")).toBeVisible();
  await expect(page.getByTestId("finding-drawer")).toContainText(
    "Generic API Key",
  );
  await page.keyboard.press("Escape");

  await page.getByTestId("nav-fixes").click();
  await expect(page.getByTestId("fix-group").first()).toBeVisible();
  await expect(
    page.getByTestId("fix-command").filter({ hasText: "npm install" }).first(),
  ).toBeVisible();

  await page.getByTestId("nav-assets").click();
  await expect(
    page.locator(
      `[data-testid=asset-row][data-identifier="ghcr.io/e2e/api-${run}"]`,
    ),
  ).toBeVisible();
});

test("triage: acknowledge, accept with a date, false positive needs a reason", async ({
  page,
}) => {
  await loginAsAdmin(page);
  await page.goto("/findings?q=CVE&state=open");
  const rows = page.getByTestId("finding-row");
  await expect(rows.first()).toBeVisible();
  const firstId = await rows.first().getAttribute("data-id");
  const secondId = await rows.nth(1).getAttribute("data-id");

  await rows.first().getByTestId("finding-select").check();
  await page.getByTestId("bulk-acknowledge").click();
  await expect
    .poll(
      async () => (await api(page, "GET", `/api/v1/findings/${firstId}`)).state,
    )
    .toBe("acknowledged");

  await page
    .locator(`[data-testid=finding-row][data-id="${secondId}"]`)
    .getByTestId("finding-select")
    .check();
  await page.getByTestId("bulk-accept").click();
  const until = new Date(Date.now() + 30 * 864e5).toISOString().slice(0, 10);
  await page.getByTestId("accept-until").fill(until);
  await page.getByTestId("dialog-confirm").click();
  await expect
    .poll(
      async () =>
        (await api(page, "GET", `/api/v1/findings/${secondId}`)).state,
    )
    .toBe("accepted");
  const accepted = await api(page, "GET", `/api/v1/findings/${secondId}`);
  expect(accepted.acceptedUntil).toBe(until);

  // false positive without a reason is refused by the server
  const res = await page.request.post("/api/v1/findings/triage", {
    headers: { "X-Vengeance-CSRF": "1" },
    data: { ids: [Number(firstId)], state: "false_positive" },
  });
  expect(res.status()).toBe(400);

  // accepted findings leave the open inbox
  await page.goto("/findings?state=accepted");
  await expect(
    page.locator(`[data-testid=finding-row][data-id="${secondId}"]`),
  ).toBeVisible();
});

test("inventory: add a host by hand and see its coverage gap", async ({
  page,
}) => {
  await loginAsAdmin(page);
  await page.getByTestId("nav-assets").click();
  await page.getByTestId("asset-new").click();
  await page.getByTestId("asset-kind").selectOption("host");
  await page.getByTestId("asset-identifier").fill(`node-${run}`);
  await page.getByTestId("asset-name").fill(`Node ${run}`);
  await page.getByTestId("asset-tags").fill("prod, internet-facing");
  await page.getByTestId("asset-save").click();
  const row = page.locator(
    `[data-testid=asset-row][data-identifier="node-${run}"]`,
  );
  await expect(row).toBeVisible();
  await expect(
    row.locator("[data-testid=coverage-cell][data-ok=false]").first(),
  ).toBeVisible();

  const cov = await api(page, "GET", "/api/v1/coverage");
  expect(
    cov.gaps.some((g: { assetName: string }) => g.assetName === `Node ${run}`),
  ).toBeTruthy();
  await page.getByTestId("nav-coverage").click();
  await expect(page.getByText(`Node ${run}`).first()).toBeVisible();
});

test("notifications: email channel test and a new-finding rule deliver through SMTP", async ({
  page,
  request,
}) => {
  await mailpitClear(request);
  await loginAsAdmin(page);
  await page.getByTestId("nav-notifications").click();
  await page.getByTestId("tab-channels").click();
  await page.getByTestId("channel-new").click();
  await page.getByTestId("channel-name").fill(`mail-${run}`);
  await page.getByTestId("channel-kind").selectOption("email");
  await page
    .locator("input[name=to], [data-testid=channel-to]")
    .first()
    .fill("me@example.com");
  await page.getByTestId("channel-save").click();
  await page.getByTestId("channel-test").last().click();
  await mailpitWaitFor(request, "Test notification");

  await page.getByTestId("tab-rules").click();
  await page.getByTestId("rule-new").click();
  await page.getByTestId("rule-name").fill(`all-new-${run}`);
  await page.getByTestId("rule-trigger").selectOption("new_finding");
  await page.getByTestId("rule-channel").selectOption({ label: `mail-${run}` });
  await page.getByTestId("rule-mode").selectOption("instant");
  await page.getByTestId("rule-max-priority").selectOption("4");
  await page.getByTestId("rule-save").click();
  await expect(page.getByText(`all-new-${run}`)).toBeVisible();

  // a brand-new asset produces new findings → one combined email
  await ingest(request, token, "image.json", {
    asset: `image:ghcr.io/e2e/worker-${run}`,
  });
  const mail = await mailpitWaitFor(request, "all-new-");
  expect(mail.Subject).toMatch(/\d+ updates from all-new-/);

  await page.getByTestId("tab-log").click();
  await expect(page.getByTestId("log-row").first()).toBeVisible();
});

test("auto-close: a rescan without a finding marks it fixed", async ({
  page,
  request,
}) => {
  await loginAsAdmin(page);
  // repository scan that only covers vulnerabilities and reports none
  const empty = Buffer.from(
    JSON.stringify({
      ArtifactName: "x",
      ArtifactType: "repository",
      Results: [],
    }),
  );
  const res = await request.post(
    `/api/v1/ingest?format=trivy&asset=repository:github.com/e2e/app-${run}&scanners=vuln`,
    {
      headers: { Authorization: `Bearer ${token}` },
      data: empty,
    },
  );
  expect(res.status()).toBe(200);
  const body = await res.json();
  expect(body.fixed).toBeGreaterThan(0);
  const fixed = await api(page, "GET", "/api/v1/findings?state=fixed&limit=1");
  expect(fixed.total).toBeGreaterThan(0);
});
