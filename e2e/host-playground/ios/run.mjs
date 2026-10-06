#!/usr/bin/env node
// Runs the host-playground test list inside the iOS host app on a Simulator.
//
//   node e2e/host-playground/ios/run.mjs --app <polkadot-app.app | .app.zip> \
//     --mnemonic-file <path> --out <dir> [--device <udid>] [--timeout-minutes <n>]
//
// The app must be a simulator build with the E2E_TEST compilation condition
// (the `build_app_simulator` fastlane lane). The runner installs it fresh,
// places the seed phrase, tests.json and page-runner.js in the app's
// `tmp/truapi-e2e/`, and launches it with TRUAPI_IOS_E2E_HOST_PLAYGROUND=1.
// The app does the rest (see HostPlaygroundE2E.swift) and writes results.json
// and a `done` marker there. The seed phrase is never printed and is deleted by
// the app on first read, and by this runner on exit.

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
  capture,
  captureOptional,
  delay,
  readPlistValue,
  run,
  selectSimulatorFromList,
} from "../../../scripts/lib/ios-simulator.mjs";
import { classify } from "../report.mjs";

const here = dirname(fileURLToPath(import.meta.url));
const shared = resolve(here, "..");
const LOG_SUBSYSTEM = "io.parity.polkadotapp.e2e";

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

// A fresh install removes the app group with the previous run's wallet and
// username, so the gates start from onboarding every time.
spawnSync("xcrun", ["simctl", "terminate", device.udid, bundle], { stdio: "ignore" });
spawnSync("xcrun", ["simctl", "uninstall", device.udid, bundle], { stdio: "ignore" });
run("xcrun", ["simctl", "install", device.udid, app]);
// Granted up front because system permission alerts belong to SpringBoard,
// which the app cannot answer from inside.
for (const service of ["location", "microphone"]) {
  spawnSync("xcrun", ["simctl", "privacy", device.udid, "grant", service, bundle], { stdio: "ignore" });
}
// simctl cannot grant notifications or the camera. Their system alerts would
// otherwise stay up for the whole run, and iOS shows one alert at a time, so
// every later permission request would wait behind them. applesimutils, when
// installed, writes those grants into the simulator directly.
const applesimutils = spawnSync(
  "applesimutils",
  ["--byId", device.udid, "--bundle", bundle, "--setPermissions", "notifications=YES,camera=YES,faceid=YES"],
  { encoding: "utf8" },
);
if (applesimutils.error) {
  console.log("applesimutils is not installed; notification and camera alerts will block their tests.");
} else if (applesimutils.status !== 0) {
  console.log(`applesimutils failed: ${(applesimutils.stderr || applesimutils.stdout).trim()}`);
}

const dataContainer = appDataContainer();
const exchange = join(dataContainer, "tmp", "truapi-e2e");
const seed = join(exchange, "seed");

let exitCode = 1;
try {
  rmSync(exchange, { recursive: true, force: true });
  mkdirSync(exchange, { recursive: true });
  copyFileSync(join(shared, "tests.json"), join(exchange, "tests.json"));
  copyFileSync(join(shared, "page-runner.js"), join(exchange, "page-runner.js"));
  copyFileSync(mnemonicFile, seed);
  chmodSync(seed, 0o600);

  // Streamed for the whole run: the simulator keeps info lines only briefly,
  // so reading them back at the end loses all but the last few minutes.
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
  process.exit(1);
}

/** The .app to install: the path itself, or the one bundle inside a zip. */
function unpackApp(path) {
  if (!path.endsWith(".zip")) return path;
  const target = mkdtempSync(join(tmpdir(), "host-playground-ios-"));
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
  const list = JSON.parse(capture("xcrun", ["simctl", "list", "devices", "available", "-j"]));
  const selected = selectSimulatorFromList(list, requested ?? process.env.TRUAPI_IOS_E2E_DEVICE);
  if (!selected) fatal(requested ? `simulator ${requested} is unavailable` : "no available iPhone simulator");
  return selected;
}

/** The data container exists once the app has launched; launch it once if it is not there yet. */
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

/** Launches the app with the hook enabled; without `terminate` it brings a running app forward. */
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
  const seedDeadline = Date.now() + 60_000;
  let reported = 0;

  while (Date.now() < deadline) {
    if (existsSync(join(exchange, "done"))) return "done";

    if (existsSync(seed) && Date.now() > seedDeadline) {
      return "the app never read the seed; is this an E2E_TEST simulator build?";
    }

    // A test that opens an external URL leaves Safari in front.
    const backgrounded = join(exchange, "backgrounded");
    if (existsSync(backgrounded)) {
      unlinkSync(backgrounded);
      launch({ terminate: false });
    }

    const run = readResults();
    const results = run?.results ?? [];
    for (const result of results.slice(reported)) {
      const message = result.message ? `  ${String(result.message).replace(/\s+/g, " ").slice(0, 200)}` : "";
      console.log(`  ${classify(result).padEnd(20)} ${result.id}${message}`);
      // The first failure's screen, while it is still showing.
      if (classify(result) === "failed" && !existsSync(join(out, "first-failure.png"))) {
        spawnSync("xcrun", ["simctl", "io", device.udid, "screenshot", join(out, "first-failure.png")], {
          stdio: "ignore",
        });
      }
    }
    if (results.length > reported) {
      // Kept current so a cancelled job still uploads what ran.
      writeFileSync(join(out, "results.json"), `${JSON.stringify(run, null, 2)}\n`);
    }
    reported = Math.max(reported, results.length);

    await delay(2000);
  }
  return `no done marker within ${args["timeout-minutes"]} minutes`;
}

function finish(outcome) {
  const failure = existsSync(join(exchange, "failure"))
    ? readFileSync(join(exchange, "failure"), "utf8")
    : undefined;
  if (failure) console.error(`The app reported: ${failure}`);
  if (outcome !== "done") console.error(`run.mjs: ${outcome}`);

  const results = readResults();
  let failed = outcome !== "done" || failure !== undefined;
  if (results) {
    const resultsPath = join(out, "results.json");
    writeFileSync(resultsPath, `${JSON.stringify(results, null, 2)}\n`);
    const report = spawnSync(process.execPath, [join(shared, "report.mjs"), resultsPath], {
      stdio: "inherit",
    });
    failed ||= report.status !== 0 || results.results.some((result) => classify(result) === "failed");
  } else {
    console.error("run.mjs: the app wrote no results.json");
    failed = true;
  }

  if (failed) captureDiagnostics();
  return failed ? 1 : 0;
}

/** A screenshot and the names of any crash reports since the run began; app.log is streamed separately. */
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
