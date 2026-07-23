import { expect, test, type Page } from "@playwright/test";
import { readFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";

// UI automation for the live REPL island — the "Live · try it" build of the
// hero carousel (src/components/Repl.tsx, the real Rust engine via vendored
// WASM). These tests cover ONLY the island's wiring: hydration, the menu
// bar, the mode badge, the panels, and the carousel handoff. Language
// behavior is off-limits here — the engine already has three spec runners
// (Swift, Rust, cucumber-js) over spec/anzan.

const fixture = (name: string) =>
  fileURLToPath(new URL(`./fixtures/${name}`, import.meta.url));

// Open the landing page, flip the carousel to the live build, and wait for
// the island to hydrate + the WASM engine to come up (data-status="ready").
async function openLiveRepl(page: Page) {
  await page.goto("/");
  await page.getByRole("tab", { name: "Live · try it" }).click();
  const repl = page.locator(".repl");
  await expect(repl).toBeVisible();
  await expect(page.locator('.repl[data-status="ready"]')).toBeVisible({
    timeout: 30_000,
  });
  return repl;
}

async function evaluate(page: Page, expression: string) {
  const input = page.getByLabel("Anzan expression");
  await input.fill(expression);
  await input.press("Enter");
}

test("hydrates and evaluates through the real engine", async ({ page }) => {
  await openLiveRepl(page);
  await evaluate(page, "0.1 + 0.2 == 0.3");
  const entry = page.locator(".repl-entry").last();
  await expect(entry.locator(".repl-in")).toHaveText("0.1 + 0.2 == 0.3");
  // Exactness is the product's headline — and a wrong/stale vendored wasm
  // would fail here before anything else does.
  await expect(entry.locator(".repl-out")).toHaveText("= 1");
});

test("shows exactly 10 welcome lines; clicking one runs it", async ({ page }) => {
  await openLiveRepl(page);
  const welcome = page.locator(".repl-welcome-line");
  await expect(welcome).toHaveCount(10);
  await welcome.first().click();
  // The welcome block yields to the log once a line lands.
  await expect(page.locator(".repl-entry")).toHaveCount(1);
  await expect(welcome).toHaveCount(0);
});

test("mode badge cycles # normal → π scientific → </> programmer", async ({ page }) => {
  await openLiveRepl(page);
  const badge = page.locator(".repl-mode");
  await expect(badge).toHaveText("# normal");
  await badge.click();
  await expect(badge).toHaveText("π scientific");
  await badge.click();
  await expect(badge).toHaveText("</> programmer");
  await badge.click();
  await expect(badge).toHaveText("# normal");
});

test("scientific mode echoes a plain result in scientific notation", async ({ page }) => {
  await openLiveRepl(page);
  const badge = page.locator(".repl-mode");
  await badge.click();
  await expect(badge).toHaveText("π scientific");
  await evaluate(page, "123456 * 2");
  // The engine seam (display_description_in) carries the sci echo across the
  // wasm boundary — a stale vendored wasm would answer 246912 here.
  await expect(page.locator(".repl-entry").last().locator(".repl-out")).toHaveText(
    "= 2.46912e5",
  );
});

test("running an example snaps programmer mode back to normal (no bitXor misread)", async ({
  page,
}) => {
  await openLiveRepl(page);
  const badge = page.locator(".repl-mode");
  await badge.click();
  await badge.click();
  await expect(badge).toHaveText("</> programmer");
  // The deterministic route: Examples ▾ → Simple → "2 ^ 64". In programmer
  // mode `^` is XOR — the island must visibly snap the badge to normal
  // before running, so the canonical grammar applies.
  await page.getByRole("button", { name: "Examples ▾" }).click();
  await page.getByRole("menuitem", { name: "2 ^ 64", exact: true }).click();
  await expect(badge).toHaveText("# normal");
  const entry = page.locator(".repl-entry").last();
  await expect(entry.locator(".repl-out")).toHaveText("= 18446744073709551616");
  await expect(entry.locator(".repl-err")).toHaveCount(0);
});

test("Examples ▾ runs the Showcase namespace, then its follow-up", async ({ page }) => {
  await openLiveRepl(page);
  await page.getByRole("button", { name: "Examples ▾" }).click();
  await page.getByRole("menuitem", { name: /^namespace Cash/ }).click();
  await expect(page.locator(".repl-entry")).toHaveCount(1);
  await page.getByRole("button", { name: "Examples ▾" }).click();
  await page
    .getByRole("menuitem", { name: "Cash::changeForDollar(0.95)", exact: true })
    .click();
  await expect(page.locator(".repl-entry").last().locator(".repl-out")).toContainText(
    "Cash::Change(quarters: 0, dimes: 0, nickels: 1, pennies: 0)",
  );
});

test("ENV panel lists the session's variables and functions", async ({ page }) => {
  await openLiveRepl(page);
  await evaluate(page, "rate = 0.05");
  await evaluate(page, "double(x) = x * 2");
  await page.getByRole("button", { name: "ENV" }).click();
  const panel = page.locator('.repl-panel[aria-label="Environment inspector"]');
  await expect(panel).toBeVisible();
  await expect(panel).toContainText("rate = 0.05");
  await expect(panel).toContainText("double(x) = x * 2");
});

test("? panel searches the reference; clicking an entry logs its man page", async ({
  page,
}) => {
  await openLiveRepl(page);
  await page.getByRole("button", { name: "?", exact: true }).click();
  const panel = page.locator('.repl-panel[aria-label="Function reference"]');
  await expect(panel).toBeVisible();
  await page.locator(".repl-search").fill("irr");
  const signatures = panel.locator(".repl-ref-sig");
  await expect(signatures.filter({ hasText: /^irr\(/ })).toHaveCount(1);
  await expect(signatures.filter({ hasText: /^xirr\(/ })).toHaveCount(1);
  await signatures.filter({ hasText: /^irr\(/ }).click();
  const entry = page.locator(".repl-entry").last();
  await expect(entry.locator(".repl-in")).toHaveText("man irr");
  await expect(entry.locator(".repl-out")).toContainText("irr");
});

test("Save As… downloads the session as a runnable .anzan script", async ({ page }) => {
  await openLiveRepl(page);
  await evaluate(page, "puzzle = 6 * 7");
  await evaluate(page, "puzzle + 1");
  const downloadPromise = page.waitForEvent("download");
  await page.getByRole("button", { name: "Save As…" }).click();
  const download = await downloadPromise;
  expect(download.suggestedFilename()).toBe("session.anzan");
  const content = await readFile((await download.path())!, "utf8");
  expect(content).toContain("#!/usr/bin/env soroban");
  expect(content).toContain("puzzle = 6 * 7");
  expect(content).toContain("puzzle + 1");
});

test("Open… runs a .anzan script and logs each statement", async ({ page }) => {
  await openLiveRepl(page);
  await page.locator('.repl input[type="file"]').setInputFiles(fixture("smoke.anzan"));
  // The shebang + comment lines are filtered; the definition and use land.
  const entries = page.locator(".repl-entry");
  await expect(entries).toHaveCount(2);
  await expect(entries.nth(0).locator(".repl-in")).toHaveText("triple(x) = x * 3");
  await expect(entries.nth(1).locator(".repl-in")).toHaveText("triple(14)");
  await expect(entries.nth(1).locator(".repl-out")).toHaveText("= 42");
});

test("Open… halts a script at the first error", async ({ page }) => {
  await openLiveRepl(page);
  await page.locator('.repl input[type="file"]').setInputFiles(fixture("halting.anzan"));
  const entries = page.locator(".repl-entry");
  await expect(entries).toHaveCount(2);
  await expect(entries.nth(0).locator(".repl-in")).toHaveText("1 + 1");
  await expect(entries.nth(1).locator(".repl-err")).toBeVisible();
  // The statement after the error never runs.
  await expect(page.locator(".repl-in", { hasText: "2 + 2" })).toHaveCount(0);
});

test("carousel treats the live build as slide-less; native gets its shots back", async ({
  page,
}) => {
  await page.goto("/");
  const dots = page.locator(".hero-shot .dots");
  await expect(dots).toBeVisible();
  await page.getByRole("tab", { name: "Live · try it" }).click();
  await expect(page.locator(".repl")).toBeVisible();
  await expect(dots).toBeHidden();
  await page.getByRole("tab", { name: "Native · macOS" }).click();
  await expect(page.locator(".hero-shot img.shot.is-active").first()).toBeVisible();
  await expect(dots).toBeVisible();
  await expect(page.locator(".repl")).toBeHidden();
});

test("fullscreen: enter, zoom text, run, and exit", async ({ page }) => {
  await openLiveRepl(page);
  const repl = page.locator(".repl");

  // Enter immersive mode — the menu bar gives way to the minimal top bar.
  await repl.getByRole("button", { name: "Enter fullscreen" }).click();
  await expect(repl).toHaveClass(/is-fullscreen/);
  await expect(repl.locator(".repl-fsbar")).toBeVisible();
  await expect(repl.locator(".repl-menubar")).toBeHidden();

  // Zoom is fullscreen-only and steps the log/input font size, persisted.
  const input = page.getByLabel("Anzan expression");
  const px = () => input.evaluate((el) => parseFloat(getComputedStyle(el).fontSize));
  const base = await px();
  await repl.getByRole("button", { name: "Larger text" }).click();
  await repl.getByRole("button", { name: "Larger text" }).click();
  expect(await px()).toBeGreaterThan(base);

  // The engine still runs from inside fullscreen.
  await evaluate(page, "6 * 7");
  await expect(repl.locator(".repl-entry").last().locator(".repl-out")).toHaveText("= 42");

  // The ⋯ overflow reaches Examples as a centered sheet.
  await repl.getByRole("button", { name: "More" }).click();
  await repl.getByRole("menuitem", { name: "Examples…" }).click();
  await expect(repl.locator(".repl-sheet")).toBeVisible();
  await page.locator(".repl-backdrop").click({ position: { x: 8, y: 8 } });
  await expect(repl.locator(".repl-sheet")).toBeHidden();

  // Escape leaves fullscreen; the zoom persists across a reload.
  await page.keyboard.press("Escape");
  await expect(repl).not.toHaveClass(/is-fullscreen/);

  await page.reload();
  await page.getByRole("tab", { name: "Live · try it" }).click();
  await expect(page.locator('.repl[data-status="ready"]')).toBeVisible({ timeout: 30_000 });
  await page.locator(".repl").getByRole("button", { name: "Enter fullscreen" }).click();
  expect(await px()).toBeGreaterThan(base); // remembered
});

test("fullscreen adapts to a mobile viewport (keyboard-aware height)", async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 }); // iPhone-ish
  await openLiveRepl(page);
  const repl = page.locator(".repl");
  await repl.getByRole("button", { name: "Enter fullscreen" }).click();
  await expect(repl).toHaveClass(/is-fullscreen/);
  // The overlay fills the viewport; the input row stays reachable at the bottom.
  const box = await repl.boundingBox();
  expect(box?.width).toBeCloseTo(390, 0);
  await expect(page.getByLabel("Anzan expression")).toBeVisible();
  await evaluate(page, "$10 * 5%");
  await expect(repl.locator(".repl-entry").last().locator(".repl-out")).toHaveText("= $0.50");
});
