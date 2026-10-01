#!/usr/bin/env node
// Runs host-playground inside the Android host on a device or emulator.
//
//   node e2e/host-playground/android/run.mjs --apk <path> --mnemonic-file <path> --out <dir> [--serial <adb serial>]
//
// The APK is a nightly build carrying the hooks in ./hooks (see
// e2e.init.gradle.kts): they open WebViews to DevTools and restore an account
// from a mnemonic left in the app's files directory. The runner installs the
// APK clean, seeds the account, opens the product through its deep link,
// drives every test in ../tests.json through ../page-runner.js over CDP, and
// answers the native approval sheets the tests raise by tapping them through
// uiautomator. It writes results.json and report.md into --out, plus a
// screenshot per failed test and the app's own logcat when anything failed.

import { execFile, spawnSync } from "node:child_process";
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { parseArgs, promisify } from "node:util";
import { chromium } from "playwright-core";
import { classify } from "../report.mjs";

const SUITE_DIR = dirname(dirname(fileURLToPath(import.meta.url)));
const suite = JSON.parse(readFileSync(join(SUITE_DIR, "tests.json"), "utf8"));
const pageRunner = readFileSync(join(SUITE_DIR, "page-runner.js"), "utf8");

const PACKAGE = `${process.env.APPLICATION_ID || "io.parity.polkadotapp"}.nightly`;
const ROOT_ACTIVITY = "io.paritytech.polkadotapp.app.root.presentation.root.RootActivity";
const PRODUCT_DEEP_LINK = `polkadotapp://${suite.product}`;
const PRODUCT_URL_FRAGMENT = suite.product.split(".")[0];
const MARKER_TAG = "HostPlaygroundE2E";

const RUNTIME_PERMISSIONS = [
  "android.permission.CAMERA",
  "android.permission.RECORD_AUDIO",
  "android.permission.ACCESS_FINE_LOCATION",
  "android.permission.ACCESS_COARSE_LOCATION",
  "android.permission.BLUETOOTH_CONNECT",
  "android.permission.POST_NOTIFICATIONS",
];

/** Labels of the native buttons that approve whatever sheet a test raised. */
export const APPROVE_LABELS = new Set([
  "Sign",
  "Approve",
  "Allow once",
  "Allow always",
  "Allow",
  "Grant",
  "Confirm",
  "Continue",
  "Add",
]);

const SEED_TIMEOUT_MS = 5 * 60_000;
const PRODUCT_OPEN_TIMEOUT_MS = 3 * 60_000;
const DEEP_LINK_RETRY_MS = 20_000;
const READY_TIMEOUT_MS = 2 * 60_000;
const TEST_TIMEOUT_MS = 90_000;
const APPROVER_INTERVAL_MS = 1_500;

const exec = promisify(execFile);
const sleep = (ms) => new Promise((done) => setTimeout(done, ms));
const log = (message) => console.log(`[android e2e] ${message}`);
const firstLine = (text) => String(text ?? "").split("\n")[0];

function adbBinary() {
  if (process.env.ADB) return process.env.ADB;
  const sdk = process.env.ANDROID_HOME || process.env.ANDROID_SDK_ROOT;
  return sdk ? join(sdk, "platform-tools", "adb") : "adb";
}

function createAdb(serial) {
  const binary = adbBinary();
  const target = serial ? ["-s", serial] : [];
  async function adb(args, { allowFailure = false, binaryOutput = false } = {}) {
    try {
      const { stdout } = await exec(binary, [...target, ...args], {
        maxBuffer: 64 * 1024 * 1024,
        encoding: binaryOutput ? "buffer" : "utf8",
      });
      return stdout;
    } catch (error) {
      if (allowFailure) return null;
      throw new Error(`adb ${args.join(" ")} failed: ${firstLine(error.stderr || error.message)}`);
    }
  }
  adb.shell = (command, options) => adb(["shell", command], options);
  return adb;
}

/**
 * The centre of the first native button whose label approves a sheet, or null.
 *
 * Nodes inside a WebView are skipped: they are the product's own DOM, and a
 * product button labelled "Sign" is not a host approval.
 */
export function findApproveButton(dumpXml, packageName) {
  const insideWebView = [];
  for (const match of dumpXml.matchAll(/<node\b([^>]*?)(\/?)>|<\/node>/g)) {
    if (match[0] === "</node>") {
      insideWebView.pop();
      continue;
    }
    const attributes = Object.fromEntries(
      [...match[1].matchAll(/([\w-]+)="([^"]*)"/g)].map(([, name, value]) => [name, value]),
    );
    const isWebView = attributes.class === "android.webkit.WebView";
    const nested = insideWebView.includes(true);
    if (match[2] !== "/") insideWebView.push(isWebView);
    if (nested || isWebView || attributes.package !== packageName || attributes.enabled === "false") continue;
    const label = attributes.text?.trim();
    if (!label || !APPROVE_LABELS.has(label)) continue;
    const bounds = attributes.bounds?.match(/\[(\d+),(\d+)\]\[(\d+),(\d+)\]/);
    if (!bounds) continue;
    const [left, top, right, bottom] = bounds.slice(1).map(Number);
    return { label, x: Math.round((left + right) / 2), y: Math.round((top + bottom) / 2) };
  }
  return null;
}

/** Polls the screen for native approval sheets until stopped. */
function startApprover(adb) {
  let running = true;
  const loop = (async () => {
    while (running) {
      const dump = await adb(["exec-out", "uiautomator", "dump", "/dev/tty"], { allowFailure: true });
      const button = dump && findApproveButton(dump, PACKAGE);
      if (button && running) {
        log(`tapping "${button.label}"`);
        await adb.shell(`input tap ${button.x} ${button.y}`, { allowFailure: true });
      }
      await sleep(APPROVER_INTERVAL_MS);
    }
  })();
  return async () => {
    running = false;
    await loop;
  };
}

async function launchApp(adb) {
  await adb.shell(`am start -W -n ${PACKAGE}/${ROOT_ACTIVITY}`);
}

async function installClean(adb, apk) {
  log(`installing ${apk}`);
  await adb(["uninstall", PACKAGE], { allowFailure: true });
  await adb(["install", apk]);
  for (const permission of RUNTIME_PERMISSIONS) {
    if ((await adb.shell(`pm grant ${PACKAGE} ${permission}`, { allowFailure: true })) === null) {
      log(`could not grant ${permission}`);
    }
  }
}

/** Places the mnemonic in the app's files directory without it appearing in any command line. */
async function deliverSeed(adb, mnemonicFile) {
  const staged = `/data/local/tmp/e2e-seed-${process.pid}`;
  await adb(["push", mnemonicFile, staged]);
  try {
    await adb.shell(`chmod 600 ${staged}`);
    await adb.shell(`run-as ${PACKAGE} mkdir -p files`);
    await adb.shell(`cat ${staged} | run-as ${PACKAGE} sh -c 'cat > files/e2e-seed'`);
  } finally {
    await adb.shell(`rm -f ${staged}`, { allowFailure: true });
  }
}

async function seedAccount(adb, mnemonicFile) {
  await launchApp(adb);
  await adb.shell(`am force-stop ${PACKAGE}`);
  await deliverSeed(adb, mnemonicFile);
  await adb(["logcat", "-c"]);
  await launchApp(adb);

  log("waiting for the account to be seeded");
  const deadline = Date.now() + SEED_TIMEOUT_MS;
  while (Date.now() < deadline) {
    const lines = (await adb(["logcat", "-d", "-v", "raw", "-s", `${MARKER_TAG}:*`])).split("\n");
    if (lines.some((line) => line.trim() === "seeded")) break;
    const failure = lines.find((line) => line.startsWith("seed failed:"));
    if (failure) throw new Error(`the app could not seed the account (${failure.trim()})`);
    await sleep(2_000);
  }
  if (Date.now() >= deadline) throw new Error(`no ${MARKER_TAG} marker within ${SEED_TIMEOUT_MS / 1000} s`);

  // The splash screen read the onboarding status before the seed landed, so
  // the account only counts as onboarded from the next launch.
  await adb.shell(`am force-stop ${PACKAGE}`);
  await launchApp(adb);
  log("account seeded");
}

/** Every page target a DevTools endpoint lists. */
async function listTargets(port) {
  try {
    const response = await fetch(`http://127.0.0.1:${port}/json/list`);
    return response.ok ? await response.json() : [];
  } catch {
    return [];
  }
}

const isProductUrl = (url) => url.includes(PRODUCT_URL_FRAGMENT);

/** Opens the product through its deep link and connects to its WebView over CDP. */
async function openProduct(adb, forwards) {
  const deadline = Date.now() + PRODUCT_OPEN_TIMEOUT_MS;
  let nextDeepLink = 0;
  while (Date.now() < deadline) {
    if (Date.now() >= nextDeepLink) {
      log(`opening ${PRODUCT_DEEP_LINK}`);
      await adb.shell(`am start -a android.intent.action.VIEW -d ${PRODUCT_DEEP_LINK} ${PACKAGE}`, { allowFailure: true });
      nextDeepLink = Date.now() + DEEP_LINK_RETRY_MS;
    }
    await sleep(2_000);

    const pid = (await adb.shell(`pidof ${PACKAGE}`, { allowFailure: true }))?.trim().split(/\s+/)[0];
    if (!pid) continue;
    const socket = `webview_devtools_remote_${pid}`;
    if (!(await adb.shell("cat /proc/net/unix")).includes(`@${socket}`)) continue;

    if (!forwards.has(socket)) {
      const port = Number((await adb(["forward", "tcp:0", `localabstract:${socket}`])).trim());
      forwards.set(socket, port);
    }
    const port = forwards.get(socket);
    if (!(await listTargets(port)).some((target) => target.type === "page" && isProductUrl(target.url))) continue;

    const browser = await chromium.connectOverCDP(`http://127.0.0.1:${port}`);
    for (let attempt = 0; attempt < 10; attempt++) {
      const page = browser.contexts().flatMap((context) => context.pages()).find((candidate) => isProductUrl(candidate.url()));
      if (page) {
        log(`connected to ${page.url()}`);
        return { browser, page };
      }
      await sleep(500);
    }
    await browser.close().catch(() => {});
  }
  throw new Error(`no WebView showing ${suite.product} within ${PRODUCT_OPEN_TIMEOUT_MS / 1000} s`);
}

/** A session whose page is the product with the page runner loaded and ready. */
async function readySession(adb, forwards, session) {
  const usable =
    session && session.browser.isConnected() && !session.page.isClosed() && isProductUrl(session.page.url());
  if (!usable) {
    await session?.browser.close().catch(() => {});
    session = await openProduct(adb, forwards);
  }
  const loaded = await session.page.evaluate(() => Boolean(window.__hostPlaygroundE2E)).catch(() => false);
  if (!loaded) {
    await session.page.evaluate(pageRunner);
    await session.page.waitForFunction(() => window.__hostPlaygroundE2E.ready(), null, { timeout: READY_TIMEOUT_MS });
  }
  return session;
}

async function runTest(session, id) {
  const started = Date.now();
  let timer;
  const guard = new Promise((_, reject) => {
    timer = setTimeout(() => reject(new Error("the page stopped answering")), TEST_TIMEOUT_MS + 30_000);
  });
  try {
    const run = session.page.evaluate(([testId, timeoutMs]) => window.__hostPlaygroundE2E.runOne(testId, timeoutMs), [
      id,
      TEST_TIMEOUT_MS,
    ]);
    return await Promise.race([run, guard]);
  } catch (error) {
    return { id, status: "error", message: `the page went away: ${firstLine(error.message)}`, durationMs: Date.now() - started };
  } finally {
    clearTimeout(timer);
  }
}

async function screenshot(adb, path) {
  const png = await adb(["exec-out", "screencap", "-p"], { allowFailure: true, binaryOutput: true });
  if (png?.length) writeFileSync(path, png);
}

/** The app's own logcat, with long hex strings and SS58-shaped addresses masked. */
async function saveAppLogcat(adb, path) {
  const uid = (await adb.shell(`pm list packages -U ${PACKAGE}`, { allowFailure: true }))?.match(/uid:(\d+)/)?.[1];
  if (!uid) return;
  const text = await adb(["logcat", "-d", "-v", "threadtime", `--uid=${uid}`], { allowFailure: true });
  if (!text) return;
  const masked = text
    .replace(/\b(0x)?[0-9a-fA-F]{32,}\b/g, "<hex>")
    .replace(/\b[1-9A-HJ-NP-Za-km-z]{46,48}\b/g, "<address>");
  writeFileSync(path, masked);
}

async function appVersion(adb) {
  const dump = await adb.shell(`dumpsys package ${PACKAGE}`, { allowFailure: true });
  const version = dump?.match(/versionName=(\S+)/)?.[1];
  return version ? `${PACKAGE} ${version}` : PACKAGE;
}

async function main() {
  const { values } = parseArgs({
    options: {
      apk: { type: "string" },
      "mnemonic-file": { type: "string" },
      out: { type: "string" },
      serial: { type: "string" },
    },
  });
  if (!values.apk || !values["mnemonic-file"] || !values.out) {
    console.error("usage: run.mjs --apk <path> --mnemonic-file <path> --out <dir> [--serial <adb serial>]");
    process.exit(2);
  }
  const out = resolve(values.out);
  mkdirSync(out, { recursive: true });
  const adb = createAdb(values.serial);
  const forwards = new Map();
  const run = {
    platform: "android",
    app: PACKAGE,
    product: suite.product,
    hostPlaygroundCommit: suite.hostPlaygroundCommit,
    startedAt: new Date().toISOString(),
    results: [],
  };
  let stopApprover = null;
  let session = null;
  let fatal = null;

  try {
    await adb(["wait-for-device"]);
    await installClean(adb, resolve(values.apk));
    run.app = await appVersion(adb);
    await seedAccount(adb, resolve(values["mnemonic-file"]));
    stopApprover = startApprover(adb);

    for (const id of suite.tests) {
      session = await readySession(adb, forwards, session);
      log(`running ${id}`);
      const result = await runTest(session, id);
      log(`${id}: ${result.status}${result.outcome ? ` (${result.outcome})` : ""}`);
      run.results.push(result);
      if (classify(result) === "failed") await screenshot(adb, join(out, `failed-${id}.png`));
    }
  } catch (error) {
    fatal = error;
    console.error(`[android e2e] ${error.message}`);
    await screenshot(adb, join(out, "fatal.png"));
  } finally {
    await stopApprover?.();
    await session?.browser.close().catch(() => {});
    for (const port of forwards.values()) await adb(["forward", "--remove", `tcp:${port}`], { allowFailure: true });
  }

  const failed = fatal !== null || run.results.some((result) => classify(result) === "failed");
  if (failed) await saveAppLogcat(adb, join(out, "logcat.txt"));

  const resultsPath = join(out, "results.json");
  writeFileSync(resultsPath, `${JSON.stringify(run, null, 2)}\n`);
  spawnSync(process.execPath, [join(SUITE_DIR, "report.mjs"), resultsPath], { stdio: "inherit" });

  if (fatal) process.exit(2);
  if (failed) process.exit(1);
}

if (import.meta.url === `file://${process.argv[1]}`) await main();
