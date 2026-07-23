import { defineConfig, devices } from "@playwright/test";

// UI automation for the live REPL island (tests/repl.spec.ts) — island
// wiring only; language behavior belongs to the three spec runners.
// Runs against the real static build: `npm run build && npm run preview`.
export default defineConfig({
  testDir: "./tests",
  fullyParallel: true,
  forbidOnly: !!process.env.CI,
  retries: process.env.CI ? 2 : 0,
  reporter: process.env.CI ? [["list"], ["html", { open: "never" }]] : "list",
  use: {
    // A dedicated port — never colliding with (or silently reusing) a
    // `npm run dev` server on astro's default 4321.
    baseURL: "http://localhost:4331",
    trace: "on-first-retry",
  },
  projects: [{ name: "chromium", use: { ...devices["Desktop Chrome"] } }],
  webServer: {
    command: "npm run build && npm run preview -- --port 4331",
    url: "http://localhost:4331",
    reuseExistingServer: !process.env.CI,
    timeout: 300_000,
  },
});
