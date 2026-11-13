import { defineConfig, devices } from "@playwright/test";

// Runs against the compose stack (docker compose up -d --build --wait).
export default defineConfig({
  testDir: ".",
  testMatch: ["tests/**/*.spec.ts", "demo/**/*.spec.ts"],
  timeout: 60_000,
  expect: { timeout: 10_000 },
  fullyParallel: false,
  workers: 1,
  retries: process.env.CI ? 1 : 0,
  reporter: process.env.CI ? [["list"], ["html", { open: "never" }]] : "list",
  use: {
    baseURL: process.env.VENGEANCE_URL ?? "http://localhost:8080",
    trace: "retain-on-failure",
    screenshot: "only-on-failure",
    ...devices["Desktop Chrome"],
    launchOptions: {
      // The OIDC issuer is http://dex:5556 for both the server container and the browser.
      args: ["--host-resolver-rules=MAP dex 127.0.0.1"],
    },
  },
});
