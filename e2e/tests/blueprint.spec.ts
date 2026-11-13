import { test, expect } from "@playwright/test";
import { api, ingest, loginAsAdmin } from "./helpers";

const run = Date.now().toString(36);

test("blueprint shows servers, apps and workloads from a reported topology", async ({
  page,
  request,
}) => {
  await loginAsAdmin(page);
  const src = await api(page, "POST", "/api/v1/sources", {
    name: `topo-${run}`,
  });
  await api(page, "POST", "/api/v1/assets", {
    kind: "host",
    identifier: `bp-node-${run}`,
    name: `bp-node-${run}`,
  });
  // one image with findings so its LED is red-ish
  await ingest(request, src.token, "image.json", {
    asset: `image:ghcr.io/e2e/bp-api-${run}`,
  });

  const topo = {
    cluster: `bp-${run}`,
    nodes: [
      {
        name: `n-${run}`,
        host: `host:bp-node-${run}`,
        cpu: "4",
        memory: "8388608Ki",
        os: "Ubuntu 24.04",
        kubelet: "v1.37.2+k3s1",
      },
    ],
    workloads: [
      {
        namespace: "shop",
        name: "api",
        kind: "Deployment",
        node: `n-${run}`,
        ready: 1,
        desired: 2,
        containers: [
          { name: "api", image: `ghcr.io/e2e/bp-api-${run}:1.0`, imageId: "" },
        ],
      },
      {
        namespace: "shop",
        name: "db",
        kind: "StatefulSet",
        node: `n-${run}`,
        ready: 1,
        desired: 1,
        containers: [{ name: "pg", image: "docker.io/library/postgres:17" }],
      },
    ],
  };
  const res = await request.post("/api/v1/topology", {
    headers: { Authorization: `Bearer ${src.token}` },
    data: topo,
  });
  expect(res.status(), await res.text()).toBe(200);
  const bad = await request.post("/api/v1/topology", {
    headers: { Authorization: "Bearer vg_nope" },
    data: topo,
  });
  expect(bad.status()).toBe(401);

  const scene = await api(page, "GET", "/api/v1/topology");
  const host = scene.hosts.find(
    (h: { name: string }) => h.name === `bp-node-${run}`,
  );
  expect(host.subtitle).toContain("4 CPU");
  const shop = host.groups.find((g: { name: string }) => g.name === "shop");
  const apiWl = shop.workloads.find((w: { name: string }) => w.name === "api");
  expect(apiWl.images[0].assetId).not.toBeNull();
  expect(["p1", "p2", "p3", "p4"]).toContain(apiWl.status.level);

  // 3D: the canvas renders and labels appear
  await page.getByTestId("nav-blueprint").click();
  await expect(page.getByTestId("blueprint-canvas")).toBeVisible();
  await expect(
    page
      .locator(".bp-label")
      .filter({ hasText: `bp-node-${run}` })
      .first(),
  ).toBeVisible({ timeout: 15_000 });

  // list view: select the workload, see details and the findings link
  await page.getByTestId("blueprint-view-list").click();
  await page
    .locator(`[data-testid=blueprint-tree-item][data-id="${apiWl.id}"]`)
    .click();
  const panel = page.getByTestId("blueprint-panel");
  await expect(panel).toContainText("api");
  await expect(panel).toContainText("1/2 ready");
  await expect(panel.getByText("open findings").first()).toBeVisible();
});
