import { expect, test } from "bun:test";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

import { CORE_MARKERS } from "./markers.js";

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

// The entry point is only reachable because the manifest lists it. A host
// reading the table takes this subpath and no other, so losing the entry would
// make it import the root and carry the host runtime to get at a policy table.
test("the table is published under its own subpath", () => {
  const manifest = JSON.parse(
    readFileSync(
      fileURLToPath(new URL("../package.json", import.meta.url)),
      "utf8",
    ),
  ) as { exports: Record<string, { types: string; import: string }> };

  expect(manifest.exports["./markers"]).toStrictEqual({
    types: "./dist/markers.d.ts",
    import: "./dist/markers.js",
  });
});

test("the root entry point does not carry the table", async () => {
  const root = (await import("./index.js")) as Record<string, unknown>;

  expect(root.CORE_MARKERS).toBeUndefined();
});
