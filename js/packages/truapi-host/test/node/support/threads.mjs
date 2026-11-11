// Counts the worker threads this process still has running. Each running
// `ThreadWorker` holds one open `MessagePort`, which keeps the process alive.

import assert from "node:assert/strict";

/** How many `MessagePort`s the process has open. */
export const openPorts = () =>
  process.getActiveResourcesInfo().filter((r) => r === "MessagePort").length;

/** Resolves once no more ports are open than `baseline`, or fails after 2s. */
export async function portsBackTo(baseline) {
  const deadline = Date.now() + 2000;
  while (openPorts() > baseline) {
    if (Date.now() > deadline)
      assert.fail(`${openPorts() - baseline} worker thread(s) still running`);
    await new Promise((resolve) => setTimeout(resolve, 10));
  }
}
