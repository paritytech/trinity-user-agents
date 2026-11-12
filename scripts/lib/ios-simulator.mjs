// Copyright 2026 Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: AGPL-3.0-only

import { capture, captureOptional } from "./process.mjs";

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
