#!/usr/bin/env node
// Runs host-playground inside the iOS host app on a simulator. See ../README.md.
//
//   node e2e/host-playground/ios/run.mjs --app <polkadot-app.app | .app.zip> \
//     --mnemonic-file <path> --out <dir> [--device <udid>] [--timeout-minutes <n>]

import { spawn, spawnSync } from "node:child_process";
import {
  chmodSync,
  closeSync,
  copyFileSync,
  existsSync,
  mkdirSync,
  mkdtempSync,
  openSync,
  readdirSync,
  readFileSync,
  rmSync,
  statSync,
  unlinkSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { parseArgs } from "node:util";

import {
  captureOptional,
  delay,
  readPlistValue,
  run,
  selectSimulator,
} from "../../../scripts/lib/ios-simulator.mjs";
import { classify } from "../report.mjs";

const here = dirname(fileURLToPath(import.meta.url));
const shared = resolve(here, "..");
const LOG_SUBSYSTEM = "io.parity.polkadotapp.e2e";
const SEED_READ_TIMEOUT_MS = 60_000;
const FIRST_LIVENESS_CHECK_MS = 30_000;
const LIVENESS_INTERVAL_MS = 10_000;
const MISSED_LIVENESS_CHECKS = 2;
const POLL_MS = 2_000;

const { values: args } = parseArgs({
  options: {
    app: { type: "string" },
    "mnemonic-file": { type: "string" },
    out: { type: "string" },
    device: { type: "string" },
    "timeout-minutes": { type: "string", default: "120" },
  },
});
if (!args.app || !args["mnemonic-file"] || !args.out) {
  console.error(
    "usage: run.mjs --app <path> --mnemonic-file <path> --out <dir> [--device <udid>] [--timeout-minutes <n>]",
  );
  process.exit(2);
}

const out = resolve(args.out);
mkdirSync(out, { recursive: true });
const mnemonicFile = resolve(args["mnemonic-file"]);
if (!existsSync(mnemonicFile)) fatal("the mnemonic file does not exist");

const app = unpackApp(resolve(args.app));
const bundle = readPlistValue(join(app, "Info.plist"), "CFBundleIdentifier");
if (!bundle) fatal(`no CFBundleIdentifier in ${app}/Info.plist`);

const device = chooseDevice(args.device);
console.log(`Simulator: ${device.name} (${device.udid}). App: ${bundle}.`);
const startedAt = new Date();

if (device.state !== "Booted") run("xcrun", ["simctl", "boot", device.udid]);
run("xcrun", ["simctl", "bootstatus", device.udid, "-b"]);

spawnSync("xcrun", ["simctl", "terminate", device.udid, bundle], { stdio: "ignore" });
spawnSync("xcrun", ["simctl", "uninstall", device.udid, bundle], { stdio: "ignore" });
run("xcrun", ["simctl", "install", device.udid, app]);
// System permission alerts belong to SpringBoard, which the app cannot answer.
for (const service of ["location", "microphone"]) {
  spawnSync("xcrun", ["simctl", "privacy", device.udid, "grant", service, bundle], { stdio: "ignore" });
}
// simctl cannot grant notifications or the camera; applesimutils can.
const applesimutils = spawnSync(
  "applesimutils",
  ["--byId", device.udid, "--bundle", bundle, "--setPermissions", "notifications=YES,camera=YES,faceid=YES"],
  { encoding: "utf8" },
);
if (applesimutils.error) {
  console.log("applesimutils is not installed; notification and camera alerts will block their tests.");
} else {
  const output = `${applesimutils.stdout}${applesimutils.stderr}`.trim();
  console.log(`applesimutils ${applesimutils.status === 0 ? "granted" : "failed"}${output ? `: ${output}` : ""}`);
}

const dataContainer = appDataContainer();
const exchange = join(dataContainer, "tmp", "truapi-e2e");
const seed = join(exchange, "seed");

let exitCode = 2;
try {
  rmSync(exchange, { recursive: true, force: true });
  mkdirSync(exchange, { recursive: true });
  copyFileSync(join(shared, "tests.json"), join(exchange, "tests.json"));
  copyFileSync(join(shared, "page-runner.js"), join(exchange, "page-runner.js"));
  copyFileSync(mnemonicFile, seed);
  chmodSync(seed, 0o600);

  // Streamed, because the simulator keeps info lines only briefly.
  const logFile = openSync(join(out, "app.log"), "w");
  const logStream = spawn(
    "xcrun",
    [
      "simctl", "spawn", device.udid, "log", "stream",
      "--style", "compact",
      "--level", "info",
      "--predicate", `subsystem == "${LOG_SUBSYSTEM}"`,
    ],
    { stdio: ["ignore", logFile, "ignore"] },
  );
  try {
    launch({ terminate: true });
    const outcome = await waitForDone(Number(args["timeout-minutes"]) * 60_000);
    exitCode = finish(outcome);
  } finally {
    logStream.kill();
    closeSync(logFile);
  }
} finally {
  rmSync(seed, { force: true });
}
process.exit(exitCode);

function fatal(message) {
  console.error(`run.mjs: ${message}`);
  process.exit(2);
}

function unpackApp(path) {
  if (!path.endsWith(".zip")) return path;
  const target = mkdtempSync(join(tmpdir(), "host-playground-ios-"));
  process.on("exit", () => rmSync(target, { recursive: true, force: true }));
  run("ditto", ["-x", "-k", path, target]);
  const found = findApp(target);
  if (!found) fatal(`no .app inside ${path}`);
  return found;
}

function findApp(directory) {
  for (const name of readdirSync(directory)) {
    const path = join(directory, name);
    if (name.endsWith(".app")) return path;
    if (statSync(path).isDirectory()) {
      const nested = findApp(path);
      if (nested) return nested;
    }
  }
  return undefined;
}

function chooseDevice(requested) {
  if (requested) process.env.TRUAPI_IOS_E2E_DEVICE = requested;
  try {
    return selectSimulator();
  } catch (error) {
    fatal(error.message);
  }
}

function appDataContainer() {
  const query = () => captureOptional("xcrun", ["simctl", "get_app_container", device.udid, bundle, "data"]);
  let container = query();
  if (!container) {
    run("xcrun", ["simctl", "launch", device.udid, bundle], { stdio: "ignore" });
    spawnSync("xcrun", ["simctl", "terminate", device.udid, bundle], { stdio: "ignore" });
    container = query();
  }
  if (!container) fatal(`no data container for ${bundle}`);
  return container;
}

function launch({ terminate }) {
  run(
    "xcrun",
    ["simctl", "launch", ...(terminate ? ["--terminate-running-process"] : []), device.udid, bundle],
    {
      stdio: "ignore",
      env: { ...process.env, SIMCTL_CHILD_TRUAPI_IOS_E2E_HOST_PLAYGROUND: "1" },
    },
  );
}

function readResults() {
  try {
    return JSON.parse(readFileSync(join(exchange, "results.json"), "utf8"));
  } catch {
    return undefined;
  }
}

async function waitForDone(timeoutMs) {
  const deadline = Date.now() + timeoutMs;
  const seedDeadline = Date.now() + SEED_READ_TIMEOUT_MS;
  let reported = 0;
  let nextLivenessCheck = Date.now() + FIRST_LIVENESS_CHECK_MS;
  let missedChecks = 0;

  while (Date.now() < deadline) {
    if (existsSync(join(exchange, "done"))) return "done";

    // A crashed app never writes `done`. More than one miss, since a relaunch briefly drops it.
    if (Date.now() >= nextLivenessCheck) {
      nextLivenessCheck = Date.now() + LIVENESS_INTERVAL_MS;
      missedChecks = appRunning() ? 0 : missedChecks + 1;
      if (missedChecks >= MISSED_LIVENESS_CHECKS) return "the app stopped running before the run finished";
    }

    if (existsSync(seed) && Date.now() > seedDeadline) {
      return "the app never read the seed; is this the simulator build e2e/host-playground/ios/build.sh makes?";
    }

    // A test that opens an external URL leaves Safari in front.
    const backgrounded = join(exchange, "backgrounded");
    if (existsSync(backgrounded)) {
      unlinkSync(backgrounded);
      launch({ terminate: false });
    }

    const snapshot = readResults();
    const results = snapshot?.results ?? [];
    for (const result of results.slice(reported)) {
      const bucket = classify(result);
      const message = result.message ? `  ${String(result.message).replace(/\s+/g, " ").slice(0, 200)}` : "";
      console.log(`  ${bucket.padEnd(20)} ${result.id}${message}`);
      if (bucket === "failed" && !existsSync(join(out, "first-failure.png"))) {
        spawnSync("xcrun", ["simctl", "io", device.udid, "screenshot", join(out, "first-failure.png")], {
          stdio: "ignore",
        });
      }
    }
    if (results.length > reported) {
      writeFileSync(join(out, "results.json"), `${JSON.stringify(snapshot, null, 2)}\n`);
    }
    reported = Math.max(reported, results.length);

    await delay(POLL_MS);
  }
  return `no done marker within ${args["timeout-minutes"]} minutes`;
}

function appRunning() {
  const list = spawnSync("xcrun", ["simctl", "spawn", device.udid, "launchctl", "list"], { encoding: "utf8" });
  // Unknown counts as running, so a flaky simctl call cannot end the run.
  if (list.status !== 0) return true;
  return list.stdout.includes(`UIKitApplication:${bundle}[`);
}

function finish(outcome) {
  const failure = existsSync(join(exchange, "failure"))
    ? readFileSync(join(exchange, "failure"), "utf8")
    : undefined;
  if (failure) console.error(`The app reported: ${failure}`);
  if (outcome !== "done") console.error(`run.mjs: ${outcome}`);

  const results = readResults();
  const stoppedEarly = outcome !== "done" || failure !== undefined || !results;
  let failed = false;
  if (results) {
    const resultsPath = join(out, "results.json");
    writeFileSync(resultsPath, `${JSON.stringify(results, null, 2)}\n`);
    const report = spawnSync(process.execPath, [join(shared, "report.mjs"), resultsPath], {
      stdio: "inherit",
    });
    failed = report.status !== 0 || results.results.some((result) => classify(result) === "failed");
  } else {
    console.error("run.mjs: the app wrote no results.json");
  }

  if (stoppedEarly || failed) captureDiagnostics();
  if (stoppedEarly) return 2;
  return failed ? 1 : 0;
}

function captureDiagnostics() {
  spawnSync("xcrun", ["simctl", "io", device.udid, "screenshot", join(out, "failure.png")], {
    stdio: "ignore",
  });

  const reports = join(process.env.HOME, "Library/Logs/DiagnosticReports");
  const crashes = existsSync(reports)
    ? readdirSync(reports).filter(
        (name) => name.startsWith("polkadot-app") && statSync(join(reports, name)).mtime >= startedAt,
      )
    : [];
  if (crashes.length) console.error(`Crash reports: ${crashes.map((name) => join(reports, name)).join(", ")}`);
  console.error(`Diagnostics in ${out}: failure.png, app.log.`);
}
