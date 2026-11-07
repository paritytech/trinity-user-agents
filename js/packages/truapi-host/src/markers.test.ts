import { expect, test } from "bun:test";
import { CORE_MARKERS } from "./index.js";

test("a marker the core exports carries the severity hosts alert on", () => {
  expect(CORE_MARKERS.get("truapi.tracing.error")).toStrictEqual({
    kind: "export",
    severity: "error",
  });
});

test("a marker whose detail names what the person is doing stays local", () => {
  expect(
    CORE_MARKERS.get("truapi.native.callback.push_notification"),
  ).toStrictEqual({
    kind: "local",
  });
});

test("a target the table does not hold has no class", () => {
  expect(CORE_MARKERS.get("truapi.not.registered")).toBeUndefined();
});

test("every entry is a target with a class a host can act on", () => {
  expect(CORE_MARKERS.size).toBeGreaterThan(0);
  for (const [target, markerClass] of CORE_MARKERS) {
    expect(target).toStartWith("truapi.");
    if (markerClass.kind === "export") {
      expect(["info", "warn", "error"]).toContain(markerClass.severity);
      expect(Object.keys(markerClass).sort()).toStrictEqual([
        "kind",
        "severity",
      ]);
    } else {
      expect(markerClass).toStrictEqual({ kind: "local" });
    }
  }
});
