// Copyright 2026 Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: MIT
/**
 * The debugger dial across worker init, which only a dev build can observe: with
 * the gate shut no dial installs, so its release is invisible. `test:dev` runs
 * this file under `--define import.meta.env.DEV=true` with bun's transpiler cache
 * off, since bun 1.3 does not key that cache on `--define`.
 */
import { afterEach, beforeEach, describe, expect, it } from "bun:test";
import { Window } from "happy-dom";

import type { WorkerPairingHostRuntime } from "./create-worker-host-runtime.js";
import {
  asWorker,
  FakeWorker,
  hostConfigFromRuntimeConfig,
  runtimeConfig,
} from "./worker-test-harness.js";
import { makeHostCallbacks } from "../test-support.js";
import { createWebWorkerPairingHostRuntime } from "./index.js";

describe("a dev build's dial across worker init", () => {
  const ENDPOINT = "ws://127.0.0.1:9231";
  const globals = globalThis as unknown as { document?: unknown };
  let had = false;
  let previous: unknown;
  let runtimes: WorkerPairingHostRuntime[] = [];

  beforeEach(() => {
    had = Object.prototype.hasOwnProperty.call(globals, "document");
    previous = globals.document;
    globals.document = new Window().document;
    runtimes = [];
  });
  afterEach(() => {
    // The dial registry is module state and outlives this document, so a live
    // runtime left in it would paint into the next test's page.
    for (const runtime of runtimes) runtime.dispose();
    if (had) globals.document = previous;
    else delete globals.document;
  });

  const badge = (): { textContent: string | null } | null =>
    (
      globalThis.document as unknown as {
        getElementById(id: string): { textContent: string | null } | null;
      }
    ).getElementById("truapi-debugger-indicator");

  const start = (
    worker: FakeWorker,
    {
      endpoint = ENDPOINT,
      initTimeoutMs,
    }: { endpoint?: string; initTimeoutMs?: number } = {},
  ): Promise<WorkerPairingHostRuntime> => {
    const started = createWebWorkerPairingHostRuntime(
      asWorker(worker),
      makeHostCallbacks(),
      {
        hostConfig: hostConfigFromRuntimeConfig(runtimeConfig()),
        debugger: endpoint,
        initTimeoutMs,
      },
    );
    worker.emit({ kind: "loaded" });
    return started;
  };

  // A shut gate installs no dial, and every release test below then passes.
  it("installs the dial before the worker is up", () => {
    const worker = new FakeWorker();
    void start(worker).catch(() => {});
    try {
      const installed = badge();
      if (installed === null)
        throw new Error(
          "no dial installed, so the dev gate is shut: run this file through " +
            "`npm test` on bun >= 1.3.11",
        );
      expect(installed.textContent).toContain(ENDPOINT);
      expect(
        worker.messages.find((message) => message.kind === "init"),
      ).toMatchObject({ debuggerUrl: ENDPOINT });
    } finally {
      worker.emit({ kind: "fatalError", error: "done" });
    }
  });

  // Nothing streams for a worker that never came up, so its badge would name an
  // endpoint no frame reaches.
  const failures: [string, (worker: FakeWorker) => void, string][] = [
    [
      "init reports a fatal error",
      (worker) => worker.emit({ kind: "fatalError", error: "boom" }),
      "boom",
    ],
    [
      "the worker errors",
      (worker) => worker.emitError("no such script"),
      "no such script",
    ],
    [
      "a message cannot be deserialized",
      (worker) => worker.emitMessageError(),
      "could not be deserialized",
    ],
  ];
  for (const [when, fail, message] of failures) {
    it(`takes the dial out of service when ${when}`, async () => {
      const worker = new FakeWorker();
      const started = start(worker);
      expect(badge()).not.toBeNull();
      fail(worker);
      await expect(started).rejects.toThrow(message);
      expect(badge()).toBeNull();
    });
  }

  it("takes the dial out of service when init times out", async () => {
    const worker = new FakeWorker();
    const started = start(worker, { initTimeoutMs: 1 });
    expect(badge()).not.toBeNull();
    await expect(started).rejects.toThrow("timed out");
    expect(badge()).toBeNull();
  });

  // `ready` shares `cleanupInit` with the failures. Its own endpoint, so a dial
  // another test leaked cannot stand in for this one.
  it("keeps the dial once the worker is ready", async () => {
    const endpoint = "ws://127.0.0.1:9300";
    const worker = new FakeWorker();
    const started = start(worker, { endpoint });
    worker.emit({ kind: "ready" });
    runtimes.push(await started);
    expect(badge()?.textContent).toContain(endpoint);
  });
});
