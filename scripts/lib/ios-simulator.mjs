// Copyright 2026 Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: AGPL-3.0-only

import { spawnSync } from "node:child_process";
import { existsSync, readdirSync, statSync } from "node:fs";
import { resolve } from "node:path";
import { capture, captureOptional, run } from "./process.mjs";

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

export function selectSimulatorFromList(simulatorList, requested) {
  const available = Object.values(simulatorList.devices)
    .flat()
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
