import { resolve } from "node:path";
import { defineConfig, devices } from "@playwright/test";

// The built site as Cloudflare serves it: wrangler sends dist/_headers, here with the CSP enforced.
// It serves HTTPS, since WebKit loads nothing over plain HTTP under upgrade-insecure-requests.
const port = 8799;
const origin = `https://127.0.0.1:${port}`;

export default defineConfig({
  testDir: "e2e",
  forbidOnly: !!process.env.CI,
  workers: process.env.CI ? 2 : undefined,
  reporter: process.env.CI ? [["list"], ["html", { open: "never" }]] : "list",
  use: { baseURL: origin, ignoreHTTPSErrors: true, trace: "retain-on-failure" },
  expect: { toHaveScreenshot: { animations: "disabled", caret: "hide", scale: "css" } },
  // how each figure looks is compared in one engine at one width
  projects: [
    { name: "chromium", use: { ...devices["Desktop Chrome"] } },
    { name: "webkit", use: { ...devices["Desktop Safari"] }, testIgnore: "static.spec.ts" },
    { name: "firefox", use: { ...devices["Desktop Firefox"] }, testIgnore: "static.spec.ts" },
    { name: "iphone", use: { ...devices["iPhone 15"] }, testIgnore: "static.spec.ts" },
    { name: "pixel", use: { ...devices["Pixel 7"] }, testIgnore: "static.spec.ts" },
  ],
  webServer: {
    command: `node scripts/csp.mjs && node_modules/.bin/wrangler dev --ip 127.0.0.1 --port ${port} --local-protocol https`,
    url: origin,
    ignoreHTTPSErrors: true,
    reuseExistingServer: !process.env.CI,
    timeout: 60_000,
    // wrangler keeps its settings and certificate in a home of its own
    env: {
      PITBOARD_CSP_ENFORCE: "1",
      HOME: resolve(".wrangler/home"),
      XDG_CONFIG_HOME: resolve(".wrangler/home/.config"),
      WRANGLER_SEND_METRICS: "false",
    },
  },
});
