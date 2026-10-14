/**
 * Full TrUAPI diagnosis against a local `truapi-host dev`.
 *
 * The product runs in a plain browser tab, hosted through the bridge script the
 * CLI serves, so there is no host shell to sign into and no confirmation modal
 * to click: `dev` auto-approves. What is left is the playground's own coverage
 * report, written next to the committed ones for comparison.
 */
import { chromium, type Browser, type Page } from "@playwright/test";
import { spawn, type ChildProcess } from "node:child_process";
import { mkdirSync, writeFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { createConnection } from "node:net";

const currentDir = dirname(fileURLToPath(import.meta.url));
const repoRoot = resolve(currentDir, "../../..");
const playgroundRoot = resolve(repoRoot, "playground");
const outputDir = resolve(playgroundRoot, "test-results/e2e-cli");

const appPort = 3000;
const hostPort = 9955;
const appUrl = `http://127.0.0.1:${appPort}`;
const bridgeUrl = `http://127.0.0.1:${hostPort}/bootstrap.js`;
const network = process.env.E2E_CLI_NETWORK ?? "paseo-next-v2";
const hostBinary = process.env.TRUAPI_HOST_BIN ?? "truapi-host";
const browserChannel = process.env.PLAYWRIGHT_CHANNEL;
const headless = process.env.HEADED !== "1";
const diagnosisTimeoutMs = Number(
  process.env.E2E_CLI_DIAGNOSIS_TIMEOUT_MS ?? 20 * 60_000,
);

function log(message: string): void {
  console.log(`[e2e-cli] ${message}`);
}

function sleep(ms: number): Promise<void> {
  return new Promise((done) => setTimeout(done, ms));
}

function portIsOpen(port: number): Promise<boolean> {
  return new Promise((done) => {
    const socket = createConnection({ port, host: "127.0.0.1" });
    socket.once("connect", () => (socket.destroy(), done(true)));
    socket.once("error", () => (socket.destroy(), done(false)));
  });
}

interface StackStatus {
  appPortOpen: boolean;
  bridgePortOpen: boolean;
  playgroundReady: boolean;
  bridgeReady: boolean;
}

async function responseContains(url: string, marker: string): Promise<boolean> {
  try {
    const response = await fetch(url, {
      signal: AbortSignal.timeout(2_000),
    });
    return response.ok && (await response.text()).includes(marker);
  } catch {
    return false;
  }
}

async function inspectStack(): Promise<StackStatus> {
  const [appPortOpen, bridgePortOpen, playgroundReady, bridgeReady] =
    await Promise.all([
      portIsOpen(appPort),
      portIsOpen(hostPort),
      responseContains(appUrl, "TrUAPI Playground"),
      responseContains(bridgeUrl, "window.__truapi_localhost"),
    ]);
  return { appPortOpen, bridgePortOpen, playgroundReady, bridgeReady };
}

function stackDescription(status: StackStatus): string {
  return [
    `playground port=${status.appPortOpen ? "open" : "closed"}`,
    `playground response=${status.playgroundReady ? "expected" : "missing/unrelated"}`,
    `bridge port=${status.bridgePortOpen ? "open" : "closed"}`,
    `bridge response=${status.bridgeReady ? "expected" : "missing/unrelated"}`,
  ].join(", ");
}

async function waitForStack(child: ChildProcess): Promise<void> {
  const deadline = Date.now() + 5 * 60_000;
  let spawnError: Error | undefined;
  child.once("error", (error) => {
    spawnError = error;
  });

  while (Date.now() < deadline) {
    if (spawnError) throw spawnError;
    if (child.exitCode !== null || child.signalCode !== null) {
      throw new Error(
        `truapi-host exited before the stack was ready (${child.exitCode ?? child.signalCode})`,
      );
    }
    const status = await inspectStack();
    if (status.playgroundReady && status.bridgeReady) return;
    await sleep(2_000);
  }
  throw new Error("playground and browser bridge did not become ready");
}

/** Start `truapi-host dev`, or adopt the complete expected stack. */
async function startStack(): Promise<ChildProcess | null> {
  const existing = await inspectStack();
  if (existing.playgroundReady && existing.bridgeReady) {
    log(`reusing the expected stack at ${appUrl} and ${bridgeUrl}`);
    return null;
  }
  if (existing.appPortOpen || existing.bridgePortOpen) {
    throw new Error(
      `refusing to reuse a partial or unrelated stack: ${stackDescription(existing)}`,
    );
  }

  log(`starting ${hostBinary} dev -- yarn dev`);
  const child = spawn(
    hostBinary,
    ["dev", "--network", network, "--", "yarn", "dev"],
    // Detached, so the host leads its own process group and stopping it takes
    // the dev server with it rather than this runner.
    { cwd: playgroundRoot, stdio: ["ignore", "pipe", "pipe"], detached: true },
  );
  const relay = (chunk: Buffer) => {
    for (const line of chunk.toString().split("\n")) {
      if (line.trim()) console.log(`[host] ${line}`);
    }
  };
  child.stdout?.on("data", relay);
  child.stderr?.on("data", relay);
  try {
    await waitForStack(child);
    return child;
  } catch (error) {
    await stopStack(child);
    throw error;
  }
}

async function waitForExit(
  child: ChildProcess,
  timeoutMs: number,
): Promise<boolean> {
  if (child.exitCode !== null || child.signalCode !== null) return true;
  return new Promise((done) => {
    const onExit = () => {
      clearTimeout(timeout);
      done(true);
    };
    const timeout = setTimeout(() => {
      child.off("exit", onExit);
      done(false);
    }, timeoutMs);
    child.once("exit", onExit);
  });
}

async function waitForStackClosed(): Promise<void> {
  const deadline = Date.now() + 10_000;
  while (Date.now() < deadline) {
    const [appOpen, bridgeOpen] = await Promise.all([
      portIsOpen(appPort),
      portIsOpen(hostPort),
    ]);
    if (!appOpen && !bridgeOpen) return;
    await sleep(250);
  }
  const status = await inspectStack();
  throw new Error(
    `stack did not release its ports: ${stackDescription(status)}`,
  );
}

async function stopStack(stack: ChildProcess): Promise<void> {
  if (stack.pid === undefined) return;
  try {
    process.kill(-stack.pid, "SIGTERM");
  } catch (error) {
    if ((error as NodeJS.ErrnoException).code !== "ESRCH") throw error;
  }
  if (!(await waitForExit(stack, 10_000))) {
    try {
      process.kill(-stack.pid, "SIGKILL");
    } catch (error) {
      if ((error as NodeJS.ErrnoException).code !== "ESRCH") throw error;
    }
    const killed = await waitForExit(stack, 5_000);
    await waitForStackClosed();
    if (!killed) {
      throw new Error("truapi-host did not exit after SIGKILL");
    }
    throw new Error(
      "truapi-host did not exit within its graceful shutdown window",
    );
  }
  await waitForStackClosed();
  log("host, playground, and bridge stopped cleanly");
}

/** Run the diagnosis and return the report the playground itself renders. */
async function runDiagnosis(page: Page): Promise<{
  summary: string;
  report: string;
  failed: string[];
  skipped: string[];
}> {
  await page.locator('[data-testid="diagnosis-run"]').click();
  log("diagnosis running");

  const deadline = Date.now() + diagnosisTimeoutMs;
  let lastLogAt = 0;
  while (Date.now() < deadline) {
    const ready = await page
      .locator('[data-testid="diagnosis-copy-report"]')
      .isVisible({ timeout: 1_000 })
      .catch(() => false);
    if (ready) break;
    if (Date.now() - lastLogAt >= 30_000) {
      lastLogAt = Date.now();
      log(`progress: ${await progressLine(page)}`);
    }
    await sleep(5_000);
  }
  if (
    !(await page.locator('[data-testid="diagnosis-copy-report"]').isVisible())
  ) {
    throw new Error(`diagnosis did not finish within ${diagnosisTimeoutMs}ms`);
  }

  const summary = await page
    .locator('[data-testid="diagnosis-summary"]')
    .innerText({ timeout: 5_000 });
  const report =
    (await page
      .locator('[data-testid="diagnosis-report-markdown"]')
      .textContent({ timeout: 5_000 })) ?? "";
  if (report.trim().length === 0) {
    throw new Error("diagnosis report markdown is empty");
  }
  // Skipped methods render as failures but are intentional gaps, so they are
  // reported separately from the ones that actually broke.
  const failed = await page
    .locator(
      '[data-testid="diagnosis-row"][data-status="fail"]:not([data-skipped="true"]) .diag__name',
    )
    .allInnerTexts();
  const skipped = await page
    .locator('[data-testid="diagnosis-row"][data-skipped="true"] .diag__name')
    .allInnerTexts();
  return { summary, report, failed, skipped };
}

async function progressLine(page: Page): Promise<string> {
  return page.evaluate(() => {
    const counts: Record<string, number> = {};
    let running = "none";
    for (const row of document.querySelectorAll<HTMLElement>(
      '[data-testid="diagnosis-row"]',
    )) {
      const status = row.dataset.status ?? "unknown";
      counts[status] = (counts[status] ?? 0) + 1;
      if (status === "running") {
        running =
          row.querySelector<HTMLElement>(".diag__name")?.innerText ?? "unknown";
      }
    }
    return `${Object.entries(counts)
      .sort(([a], [b]) => a.localeCompare(b))
      .map(([status, count]) => `${status}=${count}`)
      .join(" ")} running=${running}`;
  });
}

async function main(): Promise<void> {
  mkdirSync(outputDir, { recursive: true });
  const stack = await startStack();
  const pageErrors: string[] = [];
  let browser: Browser | undefined;

  try {
    browser = await chromium.launch({
      headless,
      ...(browserChannel ? { channel: browserChannel } : {}),
    });
    const page = await browser.newPage();
    // Raw Playwright API calls default to no action timeout, so an element that
    // never becomes actionable would hang the run.
    page.setDefaultTimeout(60_000);
    page.on("pageerror", (error) => pageErrors.push(String(error)));

    await page.addInitScript(() => {
      localStorage.setItem("truapi:playground:e2e", "1");
      (
        window as typeof window & { __TRUAPI_PLAYGROUND_E2E__?: boolean }
      ).__TRUAPI_PLAYGROUND_E2E__ = true;
    });
    // The diagnosis screen is deep-linked by `?view=`, which the playground
    // supports so the screen can be reached without driving the service rail.
    await page.goto(`http://localhost:${appPort}/?view=diagnosis`, {
      waitUntil: "domcontentloaded",
      timeout: 60_000,
    });
    await page
      .locator('[data-testid="diagnosis-run"]')
      .waitFor({ state: "visible", timeout: 60_000 });
    log("playground is hosted; starting diagnosis");

    const { summary, report, failed, skipped } = await runDiagnosis(page);
    const reportPath = resolve(outputDir, "diagnosis-report.md");
    writeFileSync(reportPath, `${report.trim()}\n`);
    writeFileSync(
      resolve(outputDir, "diagnosis-run.json"),
      `${JSON.stringify(
        {
          host: "truapi-host dev",
          network,
          summary,
          failed,
          skipped,
          pageErrors,
          timestamp: new Date().toISOString(),
        },
        null,
        2,
      )}\n`,
    );

    log(`summary: ${summary.replace(/\s+/g, " ").trim()}`);
    log(`report: ${reportPath}`);
    if (skipped.length > 0)
      log(`skipped (${skipped.length}): ${skipped.join(", ")}`);
    if (pageErrors.length > 0) {
      throw new Error(`browser page errors occurred: ${pageErrors.length}`);
    }
    if (failed.length > 0) {
      log(`diagnosed failures (${failed.length}): ${failed.join(", ")}`);
    }
    log("diagnosis complete; findings recorded");
  } finally {
    await browser?.close();
    if (stack) await stopStack(stack);
  }
}

main().catch((error) => {
  console.error(`[e2e-cli] ${error instanceof Error ? error.message : error}`);
  process.exit(1);
});
