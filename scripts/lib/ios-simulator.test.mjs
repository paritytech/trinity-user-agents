import assert from "node:assert/strict";
import test from "node:test";

import { mkdtempSync, mkdirSync, utimesSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { basename, join } from "node:path";

import {
  DEFAULT_BUNDLE,
  appGroupId,
  selectSimulatorFromList,
  userDataDatabase,
} from "./ios-simulator.mjs";

const simulatorList = {
  devices: {
    "com.apple.CoreSimulator.SimRuntime.iOS-18-3": [
      {
        udid: "e2e-device",
        name: "TrUAPI SSO E2E 18.3",
        state: "Shutdown",
        isAvailable: true,
      },
      {
        udid: "iphone-18",
        name: "iPhone 16 Pro",
        state: "Shutdown",
        isAvailable: true,
      },
    ],
    "com.apple.CoreSimulator.SimRuntime.iOS-26-4": [
      {
        udid: "iphone-26",
        name: "iPhone 17 Pro",
        state: "Booted",
        isAvailable: true,
      },
    ],
  },
};

test("an explicitly requested E2E simulator need not be named iPhone", () => {
  assert.equal(
    selectSimulatorFromList(simulatorList, "e2e-device")?.name,
    "TrUAPI SSO E2E 18.3",
  );
});

test("the default prefers a prepared TrUAPI E2E simulator", () => {
  assert.equal(selectSimulatorFromList(simulatorList)?.udid, "e2e-device");
});

test("the default falls back to a booted iPhone", () => {
  const withoutPreparedE2E = {
    devices: {
      ...simulatorList.devices,
      "com.apple.CoreSimulator.SimRuntime.iOS-18-3":
        simulatorList.devices[
          "com.apple.CoreSimulator.SimRuntime.iOS-18-3"
        ].slice(1),
    },
  };

  assert.equal(selectSimulatorFromList(withoutPreparedE2E)?.udid, "iphone-26");
});

test("the app group follows the bundle id, as the entitlements declare it", () => {
  assert.equal(
    appGroupId("io.parity.polkadotapp.develop"),
    "group.io.parity.polkadotapp.develop",
  );
  assert.equal(appGroupId(DEFAULT_BUNDLE), `group.${DEFAULT_BUNDLE}`);
});

test("the user-data store is the newest UserDataModel generation, not a fixed name", () => {
  const appGroup = mkdtempSync(join(tmpdir(), "app-group-"));
  const coreData = join(appGroup, "CoreData");
  mkdirSync(coreData);
  for (const [name, age] of [
    ["UserDataModel.sqlite", 30],
    ["UserDataModel_v2.sqlite", 20],
    ["UserDataModel_v3.sqlite", 10],
    ["SubstrateDataModel_v3.sqlite", 0],
  ]) {
    const file = join(coreData, name);
    writeFileSync(file, "");
    const when = new Date(Date.now() - age * 1000);
    utimesSync(file, when, when);
  }

  assert.equal(basename(userDataDatabase(appGroup)), "UserDataModel_v3.sqlite");
});

test("a missing store resolves to the legacy name so existence checks stay simple", () => {
  const appGroup = mkdtempSync(join(tmpdir(), "app-group-"));
  assert.equal(
    basename(userDataDatabase(appGroup)),
    "UserDataModel.sqlite",
  );
});
