import { describe, expect, it } from "bun:test";

import { createCallbackRequests } from "./worker-callback-requests.js";
import type { WorkerToMain } from "./worker-protocol.js";

function channel() {
  const posted: WorkerToMain[] = [];
  return {
    posted,
    requests: createCallbackRequests((msg) => posted.push(msg)),
  };
}

describe("worker callback requests", () => {
  it("marks a prompt withdrawable and a plain callback not", async () => {
    const { posted, requests } = channel();

    const prompt = requests.request(
      "confirmUserAction",
      [1],
      new AbortController().signal,
    );
    const plain = requests.request("readCoreStorage", [2]);

    expect(posted).toEqual([
      {
        kind: "callbackRequest",
        requestId: 1,
        name: "confirmUserAction",
        args: [1],
        withdrawable: true,
      },
      {
        kind: "callbackRequest",
        requestId: 2,
        name: "readCoreStorage",
        args: [2],
      },
    ]);
    requests.settle(1, { ok: true, value: "yes" });
    requests.settle(2, { ok: false, error: "no storage" });
    expect(await prompt).toBe("yes");
    await expect(plain).rejects.toThrow("no storage");
  });

  it("withdraws a prompt when its signal aborts and ignores a late answer", async () => {
    const { posted, requests } = channel();
    const withdrawal = new AbortController();

    const prompt = requests.request("confirmPermission", [], withdrawal.signal);
    withdrawal.abort();

    await expect(prompt).rejects.toThrow("withdrawn");
    expect(posted.at(-1)).toEqual({ kind: "callbackAbort", requestId: 1 });
    // The main thread's answer may already be in flight. It must not reach
    // the next request, which is the one still waiting.
    const next = requests.request(
      "confirmPermission",
      [],
      new AbortController().signal,
    );
    requests.settle(1, { ok: true, value: "stale" });
    requests.settle(2, { ok: true, value: "fresh" });
    expect(await next).toBe("fresh");
  });

  it("does not withdraw a prompt that was already answered", async () => {
    const { posted, requests } = channel();
    const withdrawal = new AbortController();

    const prompt = requests.request("devicePermission", [], withdrawal.signal);
    requests.settle(1, { ok: true, value: "AllowOnce" });
    withdrawal.abort();

    expect(await prompt).toBe("AllowOnce");
    expect(posted.filter((msg) => msg.kind === "callbackAbort")).toEqual([]);
  });
});
