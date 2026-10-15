#!/usr/bin/env node
// Copyright 2026 Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: AGPL-3.0-only

// Run a chat product compiled to wasm (the verbosity product) on two iPhone
// simulators, one user each, against the live network, for a two-user test.
//
// For each user the launcher:
// - makes or reuses a live account with `truapi-host signing-host`: a
//   registered lite username with People ring membership, on the network the
//   app is built for (`TRUAPI_IOS_E2E_VERBOSITY_NETWORK`, `paseo-next-v2`
//   for the Nightly flags of `make ios-build`);
// - boots its simulator, installs the app, and gives the app the mnemonic of
//   the account through `TrUAPIE2EWalletRestore`; the app then finds the
//   username on chain, as after a restore through the screens;
// - installs the wasm module as the product's worker
//   (`Products/<host>/ChatExtension/index.wasm`), so the app runs it in the
//   core with no web view;
// - starts the app with the product as a chat bot, its worker held, and the
//   command feed on.
//
// Then it writes the state of the run to `<out>/state.json` and keeps the
// consoles of both apps in `<out>/<user>.log` until it is stopped. Drive the
// users with `node scripts/ios-verbosity.mjs <user> <command>`.
//
// Environment:
//   TRUAPI_IOS_E2E_VERBOSITY_WASM      the product module (required)
//   TRUAPI_IOS_E2E_VERBOSITY_HOST      product id (default verbosity.dot)
//   TRUAPI_IOS_E2E_VERBOSITY_NAME      display name (default Verbosity)
//   TRUAPI_IOS_E2E_VERBOSITY_NETWORK   truapi-host network preset
//   TRUAPI_IOS_E2E_VERBOSITY_ACCOUNTS  where the CLI keeps the two accounts
//   TRUAPI_IOS_E2E_VERBOSITY_SESSION   username base of the accounts
//   TRUAPI_IOS_E2E_VERBOSITY_DEVICES   two simulator names or udids
//   TRUAPI_IOS_E2E_VERBOSITY_FRESH=1   reinstall the app (drops its data)
//   TRUAPI_IOS_E2E_VERBOSITY_OUT       output directory
//   TRUAPI_IOS_E2E_VERBOSITY_PROMPTS   the answer to a permission prompt of
//                                      the product: always (default), once,
//                                      deny, or ask (the launcher leaves it)

import { spawn } from "node:child_process";
import {
  chmodSync,
  createWriteStream,
  existsSync,
  mkdirSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { homedir } from "node:os";
import { dirname, resolve } from "node:path";

import {
  DEFAULT_BUNDLE,
  appGroupId,
  defaultAppPath,
  selectSimulatorFromList,
} from "./lib/ios-simulator.mjs";
import {
  USERS,
  SimulatorUser,
  provisionIdentity,
  writeState,
} from "./lib/ios-verbosity.mjs";
import { capture, captureOptional, run, waitFor } from "./lib/process.mjs";

const repoRoot = resolve(import.meta.dirname, "..");
const environment = process.env;
const bundle = environment.TRUAPI_IOS_E2E_BUNDLE ?? DEFAULT_BUNDLE;
const app = environment.TRUAPI_IOS_E2E_APP ?? defaultAppPath(repoRoot);
const wasm = environment.TRUAPI_IOS_E2E_VERBOSITY_WASM;
const productHost =
  environment.TRUAPI_IOS_E2E_VERBOSITY_HOST ?? "verbosity.dot";
const productName = environment.TRUAPI_IOS_E2E_VERBOSITY_NAME ?? "Verbosity";
const network = environment.TRUAPI_IOS_E2E_VERBOSITY_NETWORK ?? "paseo-next-v2";
const accountsRoot = resolve(
  environment.TRUAPI_IOS_E2E_VERBOSITY_ACCOUNTS ??
    resolve(homedir(), ".local/state/truapi-ios-verbosity"),
);
const sessionBase =
  environment.TRUAPI_IOS_E2E_VERBOSITY_SESSION ?? "verbosityios";
const outDir = resolve(
  repoRoot,
  environment.TRUAPI_IOS_E2E_VERBOSITY_OUT ??
    "playground/test-results/ios-verbosity",
);
const deviceNames = (
  environment.TRUAPI_IOS_E2E_VERBOSITY_DEVICES ?? "Verbosity A,Verbosity B"
)
  .split(",")
  .map((name) => name.trim());

/** How long the app has to start the product's worker. */
const START_TIMEOUT_MS = 180_000;

/**
 * The harness cannot tap the simulator, so the launcher answers a permission
 * prompt of the product as the user would, through the `permission` action
 * of the app. The first statement a product submits asks for
 * `statementSubmitAccess`.
 */
const promptAnswer = environment.TRUAPI_IOS_E2E_VERBOSITY_PROMPTS ?? "always";

if (!existsSync(app)) throw new Error(`iOS app bundle not found: ${app}`);
if (!wasm || !existsSync(wasm)) {
  throw new Error(
    `set TRUAPI_IOS_E2E_VERBOSITY_WASM to the product module (got ${wasm})`,
  );
}
if (deviceNames.length !== USERS.length) {
  throw new Error("TRUAPI_IOS_E2E_VERBOSITY_DEVICES names two simulators");
}
const module = readFileSync(wasm);
if (module.subarray(0, 4).toString("hex") !== "0061736d") {
  throw new Error(`${wasm} is not a wasm module`);
}

mkdirSync(outDir, { recursive: true });

console.log(`provisioning ${USERS.length} accounts on ${network}`);
const identities = await Promise.all(
  USERS.map((user) =>
    provisionIdentity({
      basePath: resolve(accountsRoot, network, user),
      session: `${sessionBase}${user}`,
      network,
      productHost,
    }),
  ),
);
for (const [index, user] of USERS.entries()) {
  console.log(`user ${user}: ${identities[index].username}`);
}

const users = USERS.map((user, index) =>
  prepareUser(user, deviceNames[index], identities[index]),
);
const consoles = users.map(launch);

try {
  await Promise.all(users.map(waitForWorker));
} catch (error) {
  for (const child of consoles) child.kill();
  throw error;
}

writeState(resolve(outDir, "state.json"), {
  productHost,
  network,
  users: Object.fromEntries(
    users.map((user, index) => [
      user.name,
      {
        name: user.name,
        username: identities[index].username,
        udid: user.udid,
        appData: user.appData,
        appGroup: user.appGroup,
        appLog: user.appLog,
        productHost,
      },
    ]),
  ),
});
console.log(
  JSON.stringify({
    ready: true,
    state: resolve(outDir, "state.json"),
    users: users.map((user, index) => ({
      user: user.name,
      username: identities[index].username,
      device: user.udid,
      rooms: user.rooms(),
    })),
  }),
);

if (promptAnswer !== "ask") {
  for (const user of users) answerPrompts(user);
}

const stop = () => {
  for (const user of users) {
    captureOptional("xcrun", ["simctl", "terminate", user.udid, bundle]);
  }
  for (const child of consoles) child.kill();
  process.exit(0);
};
process.on("SIGINT", stop);
process.on("SIGTERM", stop);
await new Promise(() => {});

/** Boot the simulator of a user, install the app, the seed and the module. */
function prepareUser(name, device, identity) {
  const udid = simulator(device);
  run("xcrun", ["simctl", "bootstatus", udid, "-b"], { stdio: "ignore" });
  if (environment.TRUAPI_IOS_E2E_VERBOSITY_FRESH === "1") {
    captureOptional("xcrun", ["simctl", "uninstall", udid, bundle]);
  }
  run("xcrun", ["simctl", "install", udid, app]);

  const appData = capture("xcrun", [
    "simctl",
    "get_app_container",
    udid,
    bundle,
    "data",
  ]).trim();
  const appGroup = capture("xcrun", [
    "simctl",
    "get_app_container",
    udid,
    bundle,
    appGroupId(bundle),
  ]).trim();

  // The markers and commands of an earlier run would mislead this one.
  rmSync(resolve(appData, "tmp/truapi-e2e"), { recursive: true, force: true });
  mkdirSync(resolve(appData, "tmp/truapi-e2e/commands"), { recursive: true });
  const seed = resolve(appData, "tmp/truapi-e2e/seed");
  writeFileSync(seed, identity.mnemonic, { mode: 0o600 });
  chmodSync(seed, 0o600);

  const products = resolve(
    appData,
    "Library/Application Support/Products",
    productHost,
  );
  rmSync(resolve(products, "ChatExtension/index.js"), { force: true });
  const destination = resolve(products, "ChatExtension/index.wasm");
  mkdirSync(dirname(destination), { recursive: true });
  writeFileSync(destination, module);

  return new SimulatorUser({
    name,
    udid,
    appData,
    appGroup,
    appLog: resolve(outDir, `${name}.log`),
    productHost,
  });
}

/**
 * The udid of a simulator by name or udid. A name that no simulator has gets
 * a new iPhone Pro on the newest iOS runtime.
 */
function simulator(device) {
  const list = JSON.parse(
    capture("xcrun", ["simctl", "list", "devices", "available", "-j"]),
  );
  const devices = Object.values(list.devices).flat();
  let found = devices.find(
    (candidate) => candidate.udid === device || candidate.name === device,
  );
  if (!found) {
    const template = selectSimulatorFromList(list, undefined);
    if (!template) throw new Error("no iPhone simulator to copy");
    const udid = capture("xcrun", [
      "simctl",
      "create",
      device,
      template.deviceTypeIdentifier,
    ]).trim();
    found = { udid, state: "Shutdown" };
  }
  if (found.state !== "Booted") {
    run("xcrun", ["simctl", "boot", found.udid]);
  }
  return found.udid;
}

/** Start the app of a user with the harness switches, and keep its console. */
function launch(user) {
  rmSync(user.appLog, { force: true });
  const log = createWriteStream(user.appLog);
  const child = spawn(
    "xcrun",
    [
      "simctl",
      "launch",
      "--console-pty",
      "--terminate-running-process",
      user.udid,
      bundle,
    ],
    {
      env: {
        ...environment,
        SIMCTL_CHILD_RUST_BACKTRACE: "1",
        SIMCTL_CHILD_TRUAPI_IOS_E2E_RESTORE_WALLET: "1",
        SIMCTL_CHILD_TRUAPI_IOS_E2E_CHAT_PRODUCT_HOST: productHost,
        SIMCTL_CHILD_TRUAPI_IOS_E2E_CHAT_PRODUCT_NAME: productName,
        SIMCTL_CHILD_TRUAPI_IOS_E2E_CHAT_COMMANDS: "1",
        SIMCTL_CHILD_TRUAPI_IOS_E2E_RUNTIME_MARKERS: "1",
        SIMCTL_CHILD_TRUAPI_IOS_E2E_NO_PUSH_PROMPT: "1",
        SIMCTL_CHILD_TRUAPI_IOS_E2E_GRANT_NOTIFICATIONS: "1",
      },
      stdio: ["ignore", "pipe", "pipe"],
    },
  );
  child.stdout.pipe(log);
  child.stderr.pipe(log);
  return child;
}

/** Wait until the app of a user runs the wasm worker, or says why it cannot. */
async function waitForWorker(user) {
  await waitFor(
    () => {
      const text = user.console();
      if (/wallet restore failed/.test(text)) {
        throw new Error(
          `${user.name}: the app has another wallet; rerun with TRUAPI_IOS_E2E_VERBOSITY_FRESH=1`,
        );
      }
      const fault = text.match(/worker fault, the worker ends: (.*)/);
      if (fault) throw new Error(`${user.name}: ${fault[1]}`);
      return text.includes("wasm worker running:") || undefined;
    },
    {
      timeoutMs: START_TIMEOUT_MS,
      intervalMs: 1_000,
      message: `${user.name}: the app did not start the wasm worker; see ${user.appLog}`,
    },
  );
}

/** Answer each new permission prompt of the product with `promptAnswer`. */
function answerPrompts(user) {
  let answering = false;
  setInterval(async () => {
    if (answering) return;
    const last = user
      .markers()
      .filter(
        (marker) =>
          (marker.key === "prompt" || marker.key === "prompt-answer") &&
          marker.room === productHost,
      )
      .at(-1);
    if (last?.key !== "prompt") return;
    answering = true;
    console.log(`${user.name}: answering ${last.value} with ${promptAnswer}`);
    try {
      await user.action(productHost, "permission", promptAnswer);
    } catch (error) {
      console.error(error.message);
    } finally {
      answering = false;
    }
  }, 1_000);
}
