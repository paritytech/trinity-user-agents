import { expect, test } from "bun:test";
import { CORE_MARKERS } from "./generated/markers.js";

test("a marker the core exports carries the severity hosts alert on", () => {
  expect(CORE_MARKERS.get("truapi.tracing.error")).toEqual({
    kind: "export",
    severity: "error",
  });
});

test("a marker whose detail names what the person is doing stays local", () => {
  expect(CORE_MARKERS.get("truapi.native.callback.push_notification")).toEqual({
    kind: "local",
  });
});

test("a target the table does not hold has no class", () => {
  expect(CORE_MARKERS.get("truapi.not.registered")).toBeUndefined();
});
