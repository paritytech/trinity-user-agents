import { afterEach, describe, expect, spyOn, test } from "bun:test";
import {
  installProbe,
  workerStatus,
  type ProbeStatus,
} from "../../../../src/chat/worker/probe.js";

/**
 * A chat worker's status goes to its console, which the host's Workers view
 * shows. Only a probe the worker installed keeps it, and nothing reaches the
 * window without one.
 */

const saved = (globalThis as { window?: unknown }).window;
afterEach(() => {
  Object.assign(globalThis, { window: saved });
});

describe("a chat worker's status", () => {
  test("is said on the console, and kept on nothing without a probe", () => {
    const fakeWindow: Record<string, unknown> = {};
    Object.assign(globalThis, { window: fakeWindow });
    const said: string[] = [];
    const spies = (["info", "warn", "error"] as const).map((level) =>
      spyOn(console, level).mockImplementation((line: string) => {
        said.push(`${level}: ${line}`);
      }),
    );
    try {
      const status = workerStatus("Example Chat", null);
      status.report("storage: full");
      status.running();
      status.failed("gone");
    } finally {
      for (const spy of spies) spy.mockRestore();
    }
    expect({ said, window: fakeWindow }).toEqual({
      said: [
        "warn: Example Chat: storage: full",
        "info: Example Chat is running.",
        "error: Example Chat failed: gone",
      ],
      window: {},
    });
  });

  test("is kept on an installed probe, its last errors only", () => {
    const fakeWindow: Record<string, unknown> = {};
    Object.assign(globalThis, { window: fakeWindow });
    const probe = installProbe<ProbeStatus>("__exampleWorker", {
      stage: "starting",
      errors: [],
    });
    const spies = (["info", "warn", "error"] as const).map((level) =>
      spyOn(console, level).mockImplementation(() => {}),
    );
    try {
      const status = workerStatus("Example Chat", probe, 2);
      for (const reason of ["one", "two", "three"]) status.report(reason);
      status.running();
    } finally {
      for (const spy of spies) spy.mockRestore();
    }
    expect(fakeWindow.__exampleWorker).toEqual({
      stage: "running",
      errors: ["two", "three"],
    });
  });
});
