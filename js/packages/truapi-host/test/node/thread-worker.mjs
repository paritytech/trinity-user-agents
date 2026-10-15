// The host driver only learns a worker failed through an `error` event. A Node
// thread can also just exit, which a browser worker cannot, so an exit nobody
// asked for must arrive as an error instead of leaving calls waiting forever.

import assert from "node:assert/strict";
import { mkdtempSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { pathToFileURL } from "node:url";
import { test } from "node:test";

import { ThreadWorker } from "../../dist/node/thread-worker.js";

function threadRunning(source) {
  const file = join(
    mkdtempSync(join(tmpdir(), "thread-worker-")),
    "worker.mjs",
  );
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
