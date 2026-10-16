#!/usr/bin/env node
// Copyright 2026 Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: AGPL-3.0-only

// Run a chat product compiled to wasm (the verbosity product) on two Android
// emulators, one user each, against the live network, for a two-user test.
// This is the twin of `launch-ios-verbosity.mjs`.
//
// For each user the launcher:
// - makes or reuses a live account with `truapi-host signing-host`: a
//   registered lite username with People ring membership, on the network the
//   app is built for (`TRUAPI_ANDROID_E2E_VERBOSITY_NETWORK`, `paseo-next-v2`
//   for a debug build with `-Ppolkadotapp.debugEnvironment=NIGHTLY`);
// - boots its own AVD headless (`verbosity-android-a` on port 5740,
//   `verbosity-android-b` on port 5742), installs the debug APK, and gives the
//   app the mnemonic of the account through the `restore_wallet` debug hook,
//   which does the step of the restore screen after the words; the username
//   lookup of the app then finds the username on chain;
// - installs the wasm module as the product's worker by hand
//   (`files/Products/<host>/ChatExtension/index.wasm`), so the app runs it in
//   the core with no WebView, and beside it a Worker executable manifest
//   (`manifest.json`) that declares a foreground worker; the debug hook
//   registers the product as a chat bot with a `file:` worker URL;
// - starts the app again. The worker runs while the app is in the foreground,
//   as for a user (VERBOSITY-WASM-PRODUCT I5 and A2).
//
// Then it writes the state of the run to `<out>/state.json`, keeps the logcat
// of both devices in `<out>/<user>.log`, and answers the permission prompts
// of the product until it is stopped. Drive the users with
// `node scripts/android-verbosity.mjs <user> <command>`. A stop of the
// launcher stops the emulators that it booted.
//
// Environment:
//   TRUAPI_ANDROID_E2E_VERBOSITY_WASM      the product module (required)
//   TRUAPI_ANDROID_E2E_APK                 the debug APK
//   TRUAPI_ANDROID_E2E_VERBOSITY_HOST      product id (default verbosity.dot)
//   TRUAPI_ANDROID_E2E_VERBOSITY_NAME      display name (default Verbosity)
//   TRUAPI_ANDROID_E2E_VERBOSITY_NETWORK   truapi-host network preset
//   TRUAPI_ANDROID_E2E_VERBOSITY_ACCOUNTS  where the CLI keeps the two accounts
//   TRUAPI_ANDROID_E2E_VERBOSITY_SESSION   username base of the accounts
//   TRUAPI_ANDROID_E2E_VERBOSITY_FRESH=1   reinstall the app (drops its data)
//   TRUAPI_ANDROID_E2E_VERBOSITY_OUT       output directory
//   TRUAPI_ANDROID_E2E_VERBOSITY_PROMPTS   the answer to a permission prompt
//                                          of the product: the label of the
//                                          button (default "Allow always"),
//                                          or ask (the launcher leaves it)
//   TRUAPI_ANDROID_E2E_KEEP_EMULATOR=1     keep the emulators after a stop

import { existsSync, mkdirSync, readFileSync, rmSync } from "node:fs";
import { homedir } from "node:os";
import { relative, resolve } from "node:path";

import {
  apkPackageName,
  clearLogcat,
  ensureEmulator,
  grantPermission,
  installApk,
  isDeviceOnline,
  killEmulator,
  emulatorSerial,
  shell,
  streamLogcat,
  waitForProcess,
} from "./lib/android-emulator.mjs";
import {
  ACTIVITY,
  EMULATORS,
  EmulatorUser,
  USERS,
  handInstalledWorker,
  writeAppFile,
  writeState,
} from "./lib/android-verbosity.mjs";
import { WORKER_MANIFEST, provisionIdentity } from "./lib/ios-verbosity.mjs";
import { delay, waitFor } from "./lib/process.mjs";

const repoRoot = resolve(import.meta.dirname, "..");
const environment = process.env;
const apk = resolve(
  repoRoot,
  environment.TRUAPI_ANDROID_E2E_APK ??
    "hosts/android/app/build/outputs/apk/vanilla/debug/app-vanilla-debug.apk",
);
const wasm = environment.TRUAPI_ANDROID_E2E_VERBOSITY_WASM;
const productHost = environment.TRUAPI_ANDROID_E2E_VERBOSITY_HOST ?? "verbosity.dot";
const productName = environment.TRUAPI_ANDROID_E2E_VERBOSITY_NAME ?? "Verbosity";
const network = environment.TRUAPI_ANDROID_E2E_VERBOSITY_NETWORK ?? "paseo-next-v2";
const accountsRoot = resolve(
  environment.TRUAPI_ANDROID_E2E_VERBOSITY_ACCOUNTS ??
    resolve(homedir(), ".local/state/truapi-android-verbosity"),
);
const sessionBase = environment.TRUAPI_ANDROID_E2E_VERBOSITY_SESSION ?? "verbosityand";
const outDir = resolve(
  repoRoot,
  environment.TRUAPI_ANDROID_E2E_VERBOSITY_OUT ?? "playground/test-results/android-verbosity",
);
const fresh = environment.TRUAPI_ANDROID_E2E_VERBOSITY_FRESH === "1";
const promptAnswer = environment.TRUAPI_ANDROID_E2E_VERBOSITY_PROMPTS ?? "Allow always";
const keepEmulators = environment.TRUAPI_ANDROID_E2E_KEEP_EMULATOR === "1";

/** How long the app has to restore the wallet and to start the product's worker. */
const START_TIMEOUT_MS = 300_000;

if (!existsSync(apk)) {
  throw new Error(
    `Debug APK not found: ${apk}\nBuild it: cd hosts/android && ./gradlew :app:assembleVanillaDebug -Ppolkadotapp.debugEnvironment=NIGHTLY`,
  );
}
if (!wasm || !existsSync(wasm)) {
  throw new Error(`set TRUAPI_ANDROID_E2E_VERBOSITY_WASM to the product module (got ${wasm})`);
}
const module = readFileSync(wasm);
if (module.subarray(0, 4).toString("hex") !== "0061736d") {
  throw new Error(`${wasm} is not a wasm module`);
}
const packageName = environment.TRUAPI_ANDROID_E2E_PACKAGE ?? apkPackageName(apk);
if (!packageName) throw new Error(`could not read the package name of ${apk}`);

mkdirSync(outDir, { recursive: true });
const booted = [];
const streams = [];

function cleanup() {
  for (const stream of streams) stream.stop();
  if (keepEmulators) return;
  for (const serial of booted) killEmulator(serial);
}

for (const [signal, code] of [
  ["SIGINT", 130],
  ["SIGTERM", 143],
]) {
  process.once(signal, () => {
    cleanup();
    process.exit(code);
  });
}

try {
  console.log(`provisioning ${USERS.length} accounts on ${network}`);
  const identityStarted = Date.now();
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
  console.log(`accounts ready after ${seconds(identityStarted)} s`);

  const devices = await Promise.all(USERS.map((user) => bootEmulator(user)));
  const users = USERS.map((user, index) => prepareUser(user, devices[index], identities[index]));
  await Promise.all(users.map((user, index) => restoreAndInstall(user, identities[index])));
  await Promise.all(users.map(waitForWorker));

  writeState(resolve(outDir, "state.json"), {
    productHost,
    network,
    packageName,
    users: Object.fromEntries(
      users.map((user, index) => [
        user.name,
        {
          name: user.name,
          username: identities[index].username,
          serial: user.serial,
          packageName,
          appLog: user.appLog,
          lock: user.lock,
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
        device: user.serial,
        rooms: user.rooms(),
      })),
    }),
  );

  if (promptAnswer !== "ask") {
    for (const user of users) answerPrompts(user);
  }
  await new Promise(() => {});
} catch (error) {
  console.error(error instanceof Error ? error.stack : error);
  cleanup();
  process.exit(1);
}

/** Boot the AVD of a user on its own console port, unless it runs already. */
async function bootEmulator(user) {
  const { avd, port } = EMULATORS[user];
  const serial = emulatorSerial(port);
  if (!isDeviceOnline(serial)) booted.push(serial);
  const started = Date.now();
  await ensureEmulator({ avd, port, log: (line) => console.log(`${user}: ${line}`) });
  console.log(`${user}: ${serial} booted after ${seconds(started)} s`);
  return serial;
}

/** Install the app on the emulator of a user, and keep the logcat of the device. */
function prepareUser(name, serial, identity) {
  if (fresh) shell(serial, ["pm", "uninstall", packageName]);
  installApk(serial, apk);
  grantPermission(serial, packageName, "android.permission.POST_NOTIFICATIONS");
  // A dialog of the system over the app hides the room screens from the harness.
  shell(serial, ["settings", "put", "global", "hide_error_dialogs", "1"]);
  for (const ime of [
    "com.google.android.inputmethod.latin/com.android.inputmethod.latin.LatinIME",
    "com.google.android.tts/com.google.android.apps.speech.tts.googletts.settings.asr.voiceime.VoiceInputMethodService",
  ]) {
    shell(serial, ["ime", "disable", ime]);
  }
  clearLogcat(serial);
  const appLog = resolve(outDir, `${name}.log`);
  rmSync(appLog, { force: true });
  streams.push(streamLogcat(serial, appLog));
  const lock = resolve(outDir, `${name}.ui.lock`);
  rmSync(lock, { recursive: true, force: true });
  const user = new EmulatorUser({ name, serial, packageName, productHost, appLog, lock });
  user.promptAnswer = promptAnswer;
  user.expectedUsername = identity.username;
  return user;
}

/**
 * Restore the wallet of the account from its mnemonic, install the module
 * and its manifest by hand, register the product as a chat bot, and start
 * the app again on the restored account.
 */
async function restoreAndInstall(user, identity) {
  const started = Date.now();
  const worker = handInstalledWorker(packageName, productHost);
  shell(user.serial, ["am", "force-stop", packageName]);
  writeAppFile(user.serial, packageName, worker.entry, module);
  writeAppFile(user.serial, packageName, worker.manifest, `${JSON.stringify(WORKER_MANIFEST, null, 2)}\n`);
  user.foreground();
  await waitForProcess(user.serial, packageName);
  await delay(3_000);

  writeAppFile(user.serial, packageName, "files/truapi-e2e/seed", identity.mnemonic, { mode: "600" });
  try {
    const restored = await user.hookAndWait([["--ez", "restore_wallet", "true"]], "restore_wallet done", START_TIMEOUT_MS);
    console.log(`${user.name}: ${restored}`);
    if (!restored.endsWith(`username=${identity.username}`)) {
      throw new Error(`${user.name}: the app found another username than ${identity.username}: ${restored}`);
    }
  } catch (error) {
    // An install that the launcher made before keeps its wallet; the hook removes the seed file also then.
    if (fresh || !/the app has another wallet/.test(error.message)) throw error;
    console.log(`${user.name}: the app has a wallet; rerun with TRUAPI_ANDROID_E2E_VERBOSITY_FRESH=1 for a new install`);
  }
  await user.hookAndWait(
    [
      ["--es", "product_id", productHost],
      ["--es", "product_name", productName],
      ["--es", "worker_url", worker.url],
    ],
    `product registered id=${productHost}`,
  );
  shell(user.serial, ["am", "force-stop", packageName]);
  user.foreground();
  await waitForProcess(user.serial, packageName);
  console.log(`${user.name}: restored and installed after ${seconds(started)} s`);
}

/** Wait until the app of a user runs the wasm worker, or says why it cannot. */
async function waitForWorker(user) {
  const started = Date.now();
  await waitFor(
    () => {
      const text = user.console();
      const fault = text.match(/worker fault, the worker ends: (.*)/);
      if (fault) throw new Error(`${user.name}: ${fault[1]}`);
      return text.includes("TrUAPI wasm worker running:") || undefined;
    },
    {
      timeoutMs: START_TIMEOUT_MS,
      intervalMs: 1_000,
      message: `${user.name}: the app did not start the wasm worker; see ${relative(repoRoot, user.appLog)}`,
    },
  );
  console.log(`${user.name}: wasm worker running after ${seconds(started)} s`);
}

/** Answer each new permission prompt of the product with `promptAnswer`. */
function answerPrompts(user) {
  let answering = false;
  setInterval(async () => {
    if (answering) return;
    answering = true;
    try {
      const answered = await user.answerPrompt(await user.screen());
      if (answered) console.log(`${user.name}: answered a permission prompt with ${answered}`);
    } catch (error) {
      console.error(`${user.name}: ${error.message}`);
    } finally {
      answering = false;
    }
  }, 3_000);
}

function seconds(since) {
  return Math.round((Date.now() - since) / 100) / 10;
}
