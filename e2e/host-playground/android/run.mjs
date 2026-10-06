#!/usr/bin/env node
// Runs host-playground inside the Android host on a device or emulator.
//
//   node e2e/host-playground/android/run.mjs --apk <path> --mnemonic-file <path> --out <dir> [--serial <adb serial>]
//
// The APK is a nightly build carrying the hooks in ./hooks (see
// e2e.init.gradle.kts): they open WebViews to DevTools and restore an account
// from a mnemonic left in the app's files directory. The runner installs the
// APK clean, seeds the account, opens the product through its deep link,
// drives every test in ../tests.json through ../page-runner.js over the
// DevTools protocol, and
// answers the native approval sheets the tests raise by tapping them through
// uiautomator. It writes results.json and report.md into --out, plus a
// screenshot per failed test and the app's own logcat when anything failed.

import { execFile, spawnSync } from "node:child_process";
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { parseArgs, promisify } from "node:util";
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

/** The "Wait" button of a system "isn't responding" dialog, if one is showing. */
export function findSystemWaitButton(dumpXml) {
  if (!dumpXml.includes("isn't responding") && !dumpXml.includes("isn&apos;t responding")) return null;
  for (const match of dumpXml.matchAll(/<node\b([^>]*?)\/?>/g)) {
    const attributes = Object.fromEntries(
      [...match[1].matchAll(/([\w-]+)="([^"]*)"/g)].map(([, name, value]) => [name, value]),
    );
    if (attributes.package !== "android" || attributes.text?.trim() !== "Wait") continue;
    const bounds = attributes.bounds?.match(/\[(\d+),(\d+)\]\[(\d+),(\d+)\]/);
    if (!bounds) continue;
    const [left, top, right, bottom] = bounds.slice(1).map(Number);
    return { x: Math.round((left + right) / 2), y: Math.round((top + bottom) / 2) };
  }
  return null;
}

/** Polls the screen for native approval sheets until stopped. */
function startApprover(adb) {
  let running = true;
  const loop = (async () => {
    while (running) {
      const dump = await adb(["exec-out", "uiautomator", "dump", "/dev/tty"], { allowFailure: true });
      // The emulator's launcher can stop answering; its dialog then takes the
      // focus and hides the app's sheet from the dump until it is dismissed.
      const wait = dump && findSystemWaitButton(dump);
      if (wait && running) {
        log("dismissing a system \"isn't responding\" dialog");
        await adb.shell(`input tap ${wait.x} ${wait.y}`, { allowFailure: true });
        await sleep(APPROVER_INTERVAL_MS);
        continue;
      }
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
  // A slow emulator raises "isn't responding" dialogs for system apps, which
  // cover the sheets the tests need answered.
  await adb.shell("settings put global hide_error_dialogs 1", { allowFailure: true });
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

/**
 * One page target driven over the DevTools protocol.
 *
 * WebView DevTools rejects the browser-level commands Playwright's
 * connectOverCDP sends on attach, so the runner talks to the page target
 * directly and only ever needs Runtime.evaluate.
 */
class PageTarget {
  #socket;
  #pending = new Map();
  #nextId = 1;

  static async connect(port, target) {
    const socket = new WebSocket(`ws://127.0.0.1:${port}/devtools/page/${target.id}`);
    await new Promise((opened, failed) => {
      socket.addEventListener("open", opened, { once: true });
      socket.addEventListener("error", () => failed(new Error(`could not attach to ${target.url}`)), { once: true });
    });
    return new PageTarget(socket);
  }

  constructor(socket) {
    this.#socket = socket;
    socket.addEventListener("message", ({ data }) => {
      const message = JSON.parse(data);
      const pending = this.#pending.get(message.id);
      if (!pending) return;
      this.#pending.delete(message.id);
      if (message.error) pending.reject(new Error(message.error.message));
      else pending.resolve(message.result);
    });
    socket.addEventListener("close", () => {
      for (const pending of this.#pending.values()) pending.reject(new Error("the page target closed"));
      this.#pending.clear();
    });
  }

  get closed() {
    return this.#socket.readyState !== WebSocket.OPEN;
  }

  /** The value of `expression`, awaited if it is a promise. */
  async evaluate(expression) {
    if (this.closed) throw new Error("the page target closed");
    const id = this.#nextId++;
    const reply = new Promise((resolve, reject) => this.#pending.set(id, { resolve, reject }));
    this.#socket.send(
      JSON.stringify({ id, method: "Runtime.evaluate", params: { expression, awaitPromise: true, returnByValue: true } }),
    );
    const { result, exceptionDetails } = await reply;
    if (exceptionDetails) throw new Error(exceptionDetails.exception?.description ?? exceptionDetails.text);
    return result.value;
  }

  close() {
    this.#socket.close();
  }
}

/** Opens the product through its deep link and attaches to its WebView page. */
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
    const target = (await listTargets(port)).find((candidate) => candidate.type === "page" && isProductUrl(candidate.url));
    if (!target) continue;

    log(`attaching to ${target.url}`);
    return PageTarget.connect(port, target);
  }
  throw new Error(`no WebView showing ${suite.product} within ${PRODUCT_OPEN_TIMEOUT_MS / 1000} s`);
}

/** A page showing the product with the page runner loaded and ready. */
async function readyPage(adb, forwards, page) {
  const href = page && !page.closed ? await page.evaluate("location.href").catch(() => null) : null;
  if (!href || !isProductUrl(href)) {
    page?.close();
    page = await openProduct(adb, forwards);
  }
  // The product can reload while it settles, which drops the injected runner
  // and destroys the context an evaluation was running in, so both are retried
  // until the playground shows its buttons.
  const deadline = Date.now() + READY_TIMEOUT_MS;
  for (;;) {
    const ready = await page
      .evaluate(`(() => { ${pageRunner}; return window.__hostPlaygroundE2E.ready(); })()`)
      .catch((error) => {
        if (page.closed) throw error;
        return false;
      });
    if (ready) return page;
    if (Date.now() >= deadline) throw new Error(`host-playground rendered no tests within ${READY_TIMEOUT_MS / 1000} s`);
    await sleep(500);
  }
}

async function runTest(page, id) {
  const started = Date.now();
  let timer;
  const guard = new Promise((_, reject) => {
    timer = setTimeout(() => reject(new Error("the page stopped answering")), TEST_TIMEOUT_MS + 30_000);
  });
  try {
    const run = page.evaluate(`window.__hostPlaygroundE2E.runOne(${JSON.stringify(id)}, ${TEST_TIMEOUT_MS})`);
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
  let page = null;
  let fatal = null;

  try {
    await adb(["wait-for-device"]);
    await installClean(adb, resolve(values.apk));
    run.app = await appVersion(adb);
    await seedAccount(adb, resolve(values["mnemonic-file"]));
    stopApprover = startApprover(adb);

    for (const id of suite.tests) {
      page = await readyPage(adb, forwards, page);
      log(`running ${id}`);
      const result = await runTest(page, id);
      log(`${id}: ${result.status}${result.outcome ? ` (${result.outcome})` : ""}`);
      run.results.push(result);
      if (classify(result) === "failed") await screenshot(adb, join(out, `failed-${id}.png`));
    }
  } catch (error) {
    fatal = error;
    run.fatal = error.message;
    console.error(`[android e2e] ${error.stack ?? error.message}`);
    await screenshot(adb, join(out, "fatal.png"));
  } finally {
    await stopApprover?.();
    page?.close();
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
