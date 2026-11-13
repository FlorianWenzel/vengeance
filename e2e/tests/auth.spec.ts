import { test, expect } from "@playwright/test";
import { login, loginAsAdmin } from "./helpers";

test("unauthenticated API calls are rejected", async ({ request }) => {
  const res = await request.get("/api/v1/findings");
  expect(res.status()).toBe(401);
});

test("signing in through the OIDC provider lands on the overview", async ({
  page,
}) => {
  await loginAsAdmin(page, "/findings");
  await expect(page).toHaveURL(/\/findings/);
  await page.getByTestId("nav-overview").click();
  await expect(page.getByTestId("p1-count")).toBeVisible();
});

test("a user outside the allow-list is refused", async ({ page }) => {
  await login(page, "intruder@example.com");
  await expect(
    page.getByText("is not allowed to use this Vengeance instance"),
  ).toBeVisible();
  const me = await page.request.get("/api/v1/me");
  expect(me.status()).toBe(401);
});

test("signing out ends the session", async ({ page }) => {
  await loginAsAdmin(page);
  await page.getByTestId("sign-out").click();
  await page.waitForURL(/\/dex\//);
  const me = await page.request.get("/api/v1/me");
  expect(me.status()).toBe(401);
});
