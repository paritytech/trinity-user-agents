// The host driver only learns a worker failed through an `error` event. A Node
// thread can also just exit, which a browser worker cannot, so an exit nobody
// asked for must arrive as an error instead of leaving calls waiting forever.

import assert from "node:assert/strict";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { pathToFileURL } from "node:url";
import { after, test } from "node:test";

import { createNodePairingHostRuntime } from "../../dist/node/index.js";
import { ThreadWorker } from "../../dist/node/thread-worker.js";

const dirs = [];
after(() => {
  for (const dir of dirs) rmSync(dir, { recursive: true, force: true });
});

function threadRunning(source) {
  const dir = mkdtempSync(join(tmpdir(), "thread-worker-"));
  dirs.push(dir);
  const file = join(dir, "worker.mjs");
  writeFileSync(file, source);
  return new ThreadWorker(pathToFileURL(file));
}

const nextError = (worker) =>
  new Promise((resolve) =>
    worker.addEventListener("error", (event) => resolve(event.message)),
  );

test("a thread that exits on its own is reported as an error", async () => {
  const worker = threadRunning("process.exit(3);");
  assert.equal(await nextError(worker), "worker thread exited with code 3");
});

test("an exception in the thread is reported with its message", async () => {
  const worker = threadRunning('throw new Error("boom");');
  assert.equal(await nextError(worker), "boom");
});

test("terminating the thread is not reported as an error", async () => {
  const worker = threadRunning("setInterval(() => {}, 1000);");
  let reported = false;
  worker.addEventListener("error", () => (reported = true));
  worker.terminate();
  await new Promise((resolve) => setTimeout(resolve, 200));
  assert.equal(reported, false);
});

// Node's EventTarget would rethrow a listener's exception as uncaught and end
// the process; a browser reports it and the other listeners still run.
test("an exception in one listener neither ends the process nor stops the others", async () => {
  const worker = threadRunning("process.exit(4);");
  const reported = [];
  const originalError = console.error;
  console.error = (error) => reported.push(String(error));
  try {
    worker.addEventListener("error", () => {
      throw new Error("listener failed");
    });
    const message = await nextError(worker);
    await new Promise((resolve) => setTimeout(resolve, 50));
    assert.deepEqual(
      { message, reported },
      {
        message: "worker thread exited with code 4",
        reported: ["Error: listener failed"],
      },
    );
  } finally {
    console.error = originalError;
  }
});

// The thread starts before the runtime is set up; if setup fails it must stop,
// or it keeps the process alive and this file never finishes.
test("a runtime that fails to set up stops its thread", async () => {
  await assert.rejects(createNodePairingHostRuntime({}, undefined));
});
