import { describe, expect, test } from "bun:test";
import {
  CAPABILITIES,
  permissionRows,
  queriesFor,
  type PermissionMode,
  type PermissionsView,
} from "./permissions-status.js";

const MODE: PermissionMode = {
  gate: "on",
  autoApproveNetwork: false,
  archiveWithoutContainer: false,
};

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
    const rows = permissionRows(done({ Camera: "Authorized" }), MODE);
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
      MODE,
    );
    expect(row(rows, "Submit transactions")).toMatchObject({
      value: "Denied",
      state: "warning",
    });
    expect(row(rows, "Network: api.example.test")).toMatchObject({
      value: "Allowed",
    });
  });

  // The relaxation answers one-time, so it saves nothing: the rows must say
  // the prompts are bypassed rather than imply a grant.
  test("names the developer modes that change behavior", () => {
    const rows = permissionRows(done({}), {
      gate: "ended",
      autoApproveNetwork: true,
      archiveWithoutContainer: true,
    });
    expect(row(rows, "Gate")).toMatchObject({
      value: "Container ended",
      state: "warning",
      detail: "relaxed",
    });
    expect(row(rows, "Network prompts")).toMatchObject({
      value: "Approved once, unasked",
      state: "warning",
    });
  });

  test("claims only container-gated scope", () => {
    expect(row(permissionRows(done({}), MODE), "Gate")?.title).toContain(
      "does not gate images",
    );
  });

  test("states no answers without an open product or a session", () => {
    expect(permissionRows({ state: "none-open" }, MODE)).toEqual([
      { label: "Product", value: "None open", state: "unknown" },
    ]);
    expect(
      row(permissionRows({ state: "signed-out" }, MODE), "Current permissions")
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
