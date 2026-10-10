// Copyright 2026 Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: AGPL-3.0-only

import { spawn, spawnSync } from "node:child_process";
import { existsSync, readdirSync, statSync } from "node:fs";
import { resolve } from "node:path";

/** Development bundle id of the Polkadot iOS app: `APP_MAIN_BUNDLE` in `hosts/ios/Configs/base.debug.xcconfig`. */
export const DEFAULT_BUNDLE = "io.parity.polkadotapp.develop";

/** Default DerivedData location of the sibling polkadot-app-ios-v2 checkout. */
export function defaultAppPath(repoRoot) {
  return resolve(
    repoRoot,
    "../polkadot-app-ios-v2/build/DerivedData/Build/Products/Debug-iphonesimulator/polkadot-app.app",
  );
}

/**
 * The app's user-data store: the newest `UserDataModel*.sqlite` under the app
 * group's `CoreData/`. The app renames the store on every schema generation
 * (`UserDataModel_v3.sqlite` today) and migrates on first launch, so a fixed
 * name goes stale and reads the pre-migration copy. Resolved on every read for
 * the same reason.
 */
export function userDataDatabase(appGroup) {
  const directory = resolve(appGroup, "CoreData");
  if (!existsSync(directory)) return resolve(directory, "UserDataModel.sqlite");
  const [newest] = readdirSync(directory)
    .filter((name) => /^UserDataModel(_v\d+)?\.sqlite$/.test(name))
    .map((name) => resolve(directory, name))
    .sort((a, b) => statSync(b).mtimeMs - statSync(a).mtimeMs);
  return newest ?? resolve(directory, "UserDataModel.sqlite");
}

/** App-group container id: the entitlements declare `group.$(APP_MAIN_BUNDLE)`. */
export function appGroupId(bundle) {
  return `group.${bundle}`;
}

/** Read one key from a plist; undefined when the file or key is missing. */
export function readPlistValue(plist, key) {
  return captureOptional("/usr/libexec/PlistBuddy", [
    "-c",
    `Print :${key}`,
    plist,
  ]);
}

/**
 * Poll `condition` until it returns a truthy value (which is returned) or the
 * deadline passes; `message` may be a string or a lazy `() => string`.
 */
export async function waitFor(condition, { timeoutMs, intervalMs = 250, message }) {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    const value = await condition();
    if (value) {
      return value;
    }
    await delay(intervalMs);
  }
  throw new Error(typeof message === "function" ? message() : message);
}

export function capture(command, args) {
  const result = spawnSync(command, args, { encoding: "utf8" });
  if (result.status !== 0) {
    throw new Error(
      `${command} ${args.join(" ")} failed with ${result.status}`,
    );
  }
  return result.stdout;
}

export function captureOptional(command, args) {
  const result = spawnSync(command, args, { encoding: "utf8" });
  return result.status === 0 ? result.stdout.trim() : undefined;
}

export function run(command, args, options = {}) {
  const result = spawnSync(command, args, { stdio: "inherit", ...options });
  if (result.status !== 0) {
    throw new Error(
      `${command} ${args.join(" ")} failed with ${result.status}`,
    );
  }
}

/** Run a command without blocking, so callers can overlap independent steps. */
export function runAsync(command, args, options = {}) {
  return new Promise((resolvePromise, reject) => {
    const child = spawn(command, args, { stdio: "inherit", ...options });
    child.once("error", reject);
    child.once("exit", (code, signal) => {
      if (code === 0) {
        resolvePromise();
      } else {
        reject(
          new Error(`${command} ${args.join(" ")} failed with ${code ?? signal}`),
        );
      }
    });
  });
}

export function selectSimulator() {
  const requested =
    process.env.TRUAPI_IOS_E2E_DEVICE ?? process.env.IOS_SIMULATOR_DEVICE;
  const simulatorList = JSON.parse(
    capture("xcrun", ["simctl", "list", "devices", "available", "-j"]),
  );
  const selected = selectSimulatorFromList(simulatorList, requested);

  if (!selected) {
    throw new Error(
      requested
        ? `Requested simulator is unavailable: ${requested}`
        : "No available iPhone simulator found",
    );
  }
  return selected;
}

const runtimeVersion = (runtime) =>
  (runtime.match(/iOS-(\d+)-(\d+)/) ?? [0, 0, 0]).slice(1).map(Number);

const newestRuntimeFirst = ([left], [right]) => {
  const [leftMajor, leftMinor] = runtimeVersion(left);
  const [rightMajor, rightMinor] = runtimeVersion(right);
  return rightMajor - leftMajor || rightMinor - leftMinor;
};

export function selectSimulatorFromList(simulatorList, requested) {
  const available = Object.entries(simulatorList.devices)
    .sort(newestRuntimeFirst)
    .flatMap(([, devices]) => devices)
    .filter((candidate) => candidate.isAvailable);
  if (requested) {
    return available.find(
      (candidate) =>
        candidate.udid === requested || candidate.name === requested,
    );
  }

  const preparedE2E = available.find(
    (candidate) =>
      candidate.name.includes("TrUAPI") && candidate.name.includes("E2E"),
  );
  if (preparedE2E) {
    return preparedE2E;
  }

  const iPhones = available.filter((candidate) =>
    candidate.name.startsWith("iPhone"),
  );
  return (
    iPhones.find((candidate) => candidate.state === "Booted") ?? iPhones[0]
  );
}

export function bootAndInstallApp(app) {
  const device = selectSimulator();
  // Only for watching the run: Xcode 27 no longer ships Simulator.app inside
  // Xcode, and the device boots, installs and screenshots headless without it.
  const opened = spawnSync(
    "open",
    ["-a", "Simulator", "--args", "-CurrentDeviceUDID", device.udid],
    { stdio: "ignore" },
  );
  if (opened.status !== 0) {
    console.warn("Simulator.app not found; running the device headless.");
  }
  if (device.state !== "Booted") {
    run("xcrun", ["simctl", "boot", device.udid]);
  }
  run("xcrun", ["simctl", "bootstatus", device.udid, "-b"]);
  run("xcrun", ["simctl", "install", device.udid, app]);
  return device;
}

export function isLoopback(url) {
  return ["localhost", "127.0.0.1", "::1", "[::1]"].includes(url.hostname);
}

export function delay(milliseconds) {
  return new Promise((resolveDelay) => setTimeout(resolveDelay, milliseconds));
}
