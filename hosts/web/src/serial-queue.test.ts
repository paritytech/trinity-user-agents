import { describe, expect, test } from "bun:test";
import { SerialQueue } from "./serial-queue.js";

function deferred() {
  let resolve!: () => void;
  let reject!: (error: Error) => void;
  const promise = new Promise<void>((done, fail) => {
    resolve = done;
    reject = fail;
  });
  return { promise, resolve, reject };
}

describe("SerialQueue", () => {
  // Opening a product awaits the core. A second open or a wallet switch that
  // started meanwhile would find no product to close and leave the first one
  // connected, so a task must not start before the one ahead has finished.
  test("starts a task only after the one ahead of it has finished", async () => {
    const queue = new SerialQueue(
      () => {},
      () => {},
    );
    const events: string[] = [];
    const first = deferred();
    const one = queue.run(async () => {
      events.push("first started");
      await first.promise;
      events.push("first finished");
    });
    const two = queue.run(async () => {
      events.push("second started");
    });

    await Promise.resolve();
    expect(events).toEqual(["first started"]);
    first.resolve();
    await Promise.all([one, two]);
    expect(events).toEqual([
      "first started",
      "first finished",
      "second started",
    ]);
  });

  test("keeps running after a task fails, and reports the failure", async () => {
    const errors: unknown[] = [];
    const queue = new SerialQueue(
      () => {},
      (error) => errors.push(error),
    );
    const failing = deferred();
    const one = queue.run(() => failing.promise);
    let ranAfter = false;
    const two = queue.run(async () => {
      ranAfter = true;
    });
    failing.reject(new Error("boom"));
    await Promise.all([one, two]);
    expect({ ranAfter, errors: errors.map(String) }).toEqual({
      ranAfter: true,
      errors: ["Error: boom"],
    });
  });

  test("is busy from the moment a task is queued until the last one ends", async () => {
    const seen: boolean[] = [];
    const queue: SerialQueue = new SerialQueue(
      () => seen.push(queue.busy),
      () => {},
    );
    const gate = deferred();
    const one = queue.run(() => gate.promise);
    const two = queue.run(async () => {});
    expect(queue.busy).toBe(true);
    gate.resolve();
    await Promise.all([one, two]);
    expect(seen).toEqual([true, true, true, false]);
  });
});
