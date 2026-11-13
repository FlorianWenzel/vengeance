// Seeds a fresh stack with a plausible demo company ("Acme") and captures the README screenshots.
// Run against an empty compose stack:  SCREENSHOTS=1 npx playwright test demo/  (see README "Screenshots")
import { test, expect, type Page } from "@playwright/test";
import { api, fixture, loginAsAdmin } from "../tests/helpers";

test.skip(
  !process.env.SCREENSHOTS,
  "set SCREENSHOTS=1 to regenerate docs/screenshots",
);
test.use({
  viewport: { width: 1600, height: 1000 },
  deviceScaleFactor: 2,
  colorScheme: "dark",
});

const OUT = "../docs/screenshots";
const shot = (page: Page, name: string) =>
  page.screenshot({ path: `${OUT}/${name}.png` });

async function ingest(
  page: Page,
  token: string,
  file: string,
  q: Record<string, string>,
) {
  const res = await page.request.post(
    `/api/v1/ingest?${new URLSearchParams({ format: "trivy", ...q })}`,
    {
      headers: { Authorization: `Bearer ${token}` },
      data: fixture(file),
    },
  );
  expect(res.status(), await res.text()).toBe(200);
}

test("seed demo data and capture screenshots", async ({ page }) => {
  test.setTimeout(180_000);
  await loginAsAdmin(page);
  const ci = await api(page, "POST", "/api/v1/sources", {
    name: "github-actions",
    staleAfterHours: 48,
  });
  const node = await api(page, "POST", "/api/v1/sources", {
    name: "edge-01 nightly",
    staleAfterHours: 30,
  });
  await api(page, "POST", "/api/v1/sources", {
    name: "legacy-cron",
    staleAfterHours: 24,
  });

  // inventory
  for (const a of [
    {
      kind: "host",
      identifier: "edge-01",
      name: "edge-01 (Frankfurt)",
      tags: ["prod", "internet-facing"],
    },
    { kind: "cluster", identifier: "edge", name: "edge k3s", tags: ["prod"] },
    {
      kind: "repository",
      identifier: "github.com/acme/storefront",
      name: "storefront",
    },
    {
      kind: "repository",
      identifier: "github.com/acme/payments",
      name: "payments",
    },
    { kind: "repository", identifier: "github.com/acme/infra", name: "infra" },
    {
      kind: "domain",
      identifier: "shop.acme.example",
      tags: ["internet-facing"],
    },
    {
      kind: "domain",
      identifier: "pay.acme.example",
      tags: ["internet-facing"],
    },
    {
      kind: "image",
      identifier: "ghcr.io/acme/storefront-api",
      tags: ["prod", "internet-facing"],
    },
  ])
    await api(page, "POST", "/api/v1/assets", a);

  // scans
  await ingest(page, ci.token, "image.json", {
    asset: "image:ghcr.io/acme/storefront-api",
    scanners: "vuln,secret",
  });
  await ingest(page, ci.token, "fs.json", {
    asset: "repository:github.com/acme/storefront",
    scanners: "vuln,secret,misconfig",
  });
  await ingest(page, node.token, "image.json", {
    asset: "image:ghcr.io/acme/payments-worker",
    scanners: "vuln",
  });
  await ingest(page, node.token, "image.json", {
    asset: "image:postgres",
    scanners: "vuln,secret",
  });

  const topo = {
    cluster: "edge",
    nodes: [
      {
        name: "edge-01",
        host: "host:edge-01",
        cpu: "8",
        memory: "32768000Ki",
        os: "Ubuntu 24.04 LTS",
        kernel: "6.8.0",
        kubelet: "v1.37.2+k3s1",
      },
    ],
    routes: [
      { namespace: "storefront", hostnames: ["shop.acme.example"] },
      { namespace: "payments", hostnames: ["pay.acme.example"] },
    ],
    workloads: [
      {
        namespace: "storefront",
        name: "api",
        kind: "Deployment",
        node: "edge-01",
        ready: 3,
        desired: 3,
        containers: [
          { name: "api", image: "ghcr.io/acme/storefront-api:2.4.1" },
        ],
      },
      {
        namespace: "storefront",
        name: "web",
        kind: "Deployment",
        node: "edge-01",
        ready: 2,
        desired: 2,
        containers: [{ name: "web", image: "nginx:1.30-alpine" }],
      },
      {
        namespace: "storefront",
        name: "postgres",
        kind: "StatefulSet",
        node: "edge-01",
        ready: 1,
        desired: 1,
        containers: [{ name: "pg", image: "postgres:17.11-alpine" }],
      },
      {
        namespace: "payments",
        name: "worker",
        kind: "Deployment",
        node: "edge-01",
        ready: 1,
        desired: 2,
        containers: [
          { name: "worker", image: "ghcr.io/acme/payments-worker:1.9.0" },
        ],
      },
      {
        namespace: "payments",
        name: "gateway",
        kind: "Deployment",
        node: "edge-01",
        ready: 1,
        desired: 1,
        containers: [
          { name: "gw", image: "ghcr.io/acme/payments-gateway:1.2.0" },
        ],
      },
      {
        namespace: "monitoring",
        name: "prometheus",
        kind: "StatefulSet",
        node: "edge-01",
        ready: 1,
        desired: 1,
        containers: [{ name: "prom", image: "prom/prometheus:v3.5.0" }],
      },
      {
        namespace: "monitoring",
        name: "grafana",
        kind: "Deployment",
        node: "edge-01",
        ready: 1,
        desired: 1,
        containers: [{ name: "grafana", image: "grafana/grafana:12.2.0" }],
      },
      {
        namespace: "blog",
        name: "ghost",
        kind: "Deployment",
        node: "edge-01",
        ready: 1,
        desired: 1,
        containers: [{ name: "ghost", image: "ghost:6.75.0" }],
      },
      {
        namespace: "kube-system",
        name: "coredns",
        kind: "Deployment",
        node: "edge-01",
        ready: 1,
        desired: 1,
        containers: [
          { name: "coredns", image: "rancher/mirrored-coredns-coredns:1.13.1" },
        ],
      },
    ],
  };
  const tr = await page.request.post("/api/v1/topology", {
    headers: { Authorization: `Bearer ${node.token}` },
    data: topo,
  });
  expect(tr.status()).toBe(200);

  // a little triage history so the trend and states look lived-in
  const list = await api(page, "GET", "/api/v1/findings?limit=6&sort=priority");
  await api(page, "POST", "/api/v1/findings/triage", {
    ids: [list.items[3].id],
    state: "acknowledged",
  });

  // notification rules
  const ch = await api(page, "POST", "/api/v1/channels", {
    name: "Phone (ntfy)",
    kind: "ntfy",
    config: { server: "https://ntfy.sh", topic: "acme-security-demo" },
  });
  await api(page, "POST", "/api/v1/rules", {
    name: "P1 right away",
    trigger: "new_finding",
    filter: { maxPriority: 1, projectId: null, tags: [], fixAvailable: null },
    mode: "instant",
    channelId: ch.id,
    enabled: true,
  });
  await api(page, "POST", "/api/v1/rules", {
    name: "A scanner went quiet",
    trigger: "stale_source",
    filter: { maxPriority: 4, projectId: null, tags: [], fixAvailable: null },
    mode: "instant",
    channelId: ch.id,
    enabled: true,
  });

  await page.goto("/");
  await page.waitForTimeout(1500);
  await shot(page, "overview");
  await page.goto("/findings");
  await page.waitForTimeout(1000);
  await page.getByTestId("finding-row").first().click();
  await page.waitForTimeout(800);
  await shot(page, "findings");
  await page.keyboard.press("Escape");
  await page.goto("/fixes");
  await page.waitForTimeout(1000);
  await shot(page, "fixes");
  await page.goto("/coverage");
  await page.waitForTimeout(1000);
  await shot(page, "coverage");
  await page.goto("/blueprint");
  await page.waitForTimeout(3000);
  await shot(page, "blueprint");
  await page
    .getByTestId("blueprint-jump")
    .selectOption({ label: "  storefront" });
  await page.waitForTimeout(3000);
  await shot(page, "blueprint-zoom");
  // walk: arrival on the square, then the storefront facade up close
  await page.getByTestId("blueprint-reset").click();
  await page
    .locator('[data-testid=blueprint-panel] button[aria-label="Close details"]')
    .click()
    .catch(() => {});
  await page.getByTestId("blueprint-view-walk").click();
  await page.waitForTimeout(500);
  const hidePrompt = () =>
    page.evaluate(() =>
      document.querySelector("[data-testid=blueprint-enter-walk]")?.remove(),
    );
  await hidePrompt();
  await page.waitForTimeout(2500);
  await shot(page, "walk");
  await page
    .getByTestId("blueprint-jump")
    .selectOption({ label: "  storefront" });
  await page.waitForTimeout(3500);
  await page
    .locator('[data-testid=blueprint-panel] button[aria-label="Close details"]')
    .click();
  const box = (await page.getByTestId("blueprint-canvas").boundingBox())!;
  await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
  await page.mouse.down();
  await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2 + 45, {
    steps: 8,
  });
  await page.mouse.up();
  await hidePrompt();
  await page.waitForTimeout(1500);
  await shot(page, "walk-facade");
  await page.goto("/ai");
  await page.waitForTimeout(800);
  await shot(page, "mcp");
});
