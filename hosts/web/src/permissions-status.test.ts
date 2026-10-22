import { describe, expect, test } from "bun:test";
import {
  CAPABILITIES,
  permissionRows,
  queriesFor,
  type PermissionsView,
} from "./permissions-status.js";

const done = (
  statuses: Record<string, "Authorized" | "Denied" | "NotDetermined">,
  networks: Record<string, "Authorized" | "Denied" | "NotDetermined"> = {},
): PermissionsView => {
  const queries = queriesFor(Object.keys(networks));
  return {
    state: "done",
    capabilities: queries.capabilities.map((query) => ({
      query,
      status: statuses[query.label] ?? "NotDetermined",
    })),
    networks: queries.networks.map((query) => ({
      query,
      status: networks[query.label],
    })),
  };
};

const row = (rows: ReturnType<typeof permissionRows>, label: string) =>
  rows.find((r) => r.label === label);

describe("permission rows", () => {
  // No answer is a question the core will ask, not a refusal.
  test("shows no answer as Ask, not Denied", () => {
    const rows = permissionRows(done({ Camera: "Authorized" }));
    expect(row(rows, "Camera")).toMatchObject({ value: "Allowed" });
    expect(row(rows, "Ask")?.value).toContain("Microphone");
    expect(row(rows, "Ask")?.value).not.toContain("Camera");
    expect(rows.some((r) => r.value === "Denied")).toBe(false);
  });

  test("lists a denial as a warning and a domain under its own name", () => {
    const rows = permissionRows(
      done(
        { "Submit transactions": "Denied" },
        { "api.example.test": "Authorized" },
      ),
    );
    expect(row(rows, "Submit transactions")).toMatchObject({
      value: "Denied",
      state: "warning",
    });
    expect(row(rows, "Network: api.example.test")).toMatchObject({
      value: "Allowed",
    });
  });

  test("claims only explicit TrUAPI calls, not the page's own requests", () => {
    const scope = row(permissionRows(done({})), "Scope");
    expect(scope?.value).toBe("Explicit TrUAPI calls");
    expect(scope?.title).toContain("follow the browser");
  });

  test("states no answers without an open product or a session", () => {
    expect(permissionRows({ state: "none-open" })).toEqual([
      { label: "Product", value: "None open", state: "unknown" },
    ]);
    expect(
      row(permissionRows({ state: "signed-out" }), "Current permissions")
        ?.value,
    ).toBe("Sign in first");
  });
});

describe("permission queries", () => {
  test("asks about each fixed capability once and each domain once", () => {
    const { capabilities, networks } = queriesFor([
      "a.test",
      "b.test",
      "a.test",
    ]);
    expect(capabilities).toBe(CAPABILITIES);
    expect(networks.map((q) => q.request)).toEqual([
      {
        tag: "Remote",
        value: {
          permission: { tag: "Remote", value: { domains: ["a.test"] } },
        },
      },
      {
        tag: "Remote",
        value: {
          permission: { tag: "Remote", value: { domains: ["b.test"] } },
        },
      },
    ]);
  });
});
