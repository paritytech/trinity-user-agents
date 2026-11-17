import assert from "node:assert/strict";
import test from "node:test";

import { selectSimulatorFromList } from "./ios-simulator.mjs";

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

test("the default takes the newest runtime's iPhone when none is booted", () => {
  const shutDown = {
    devices: {
      "com.apple.CoreSimulator.SimRuntime.iOS-9-3": [
        { udid: "iphone-9", name: "iPhone 6s", state: "Shutdown", isAvailable: true },
      ],
      "com.apple.CoreSimulator.SimRuntime.iOS-26-4": [
        { udid: "iphone-26", name: "iPhone 17 Pro", state: "Shutdown", isAvailable: true },
      ],
      "com.apple.CoreSimulator.SimRuntime.iOS-18-3": [
        { udid: "iphone-18", name: "iPhone 16 Pro", state: "Shutdown", isAvailable: true },
      ],
    },
  };

  assert.equal(selectSimulatorFromList(shutDown)?.udid, "iphone-26");
});
