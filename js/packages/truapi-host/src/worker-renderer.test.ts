import { describe, expect, it } from "bun:test";
import { ProductRendererRenderRequest } from "@parity/truapi";

import {
  handleRenderStart,
  stopRender,
  stopRendersForCore,
  type RenderSubscriptions,
} from "./worker-renderer.js";
import type {
  WorkerRendererSubscription,
  WorkerProductRuntime,
} from "./wasm-module.js";
import type { WorkerToMain } from "./worker-protocol.js";

const renderRequest = ProductRendererRenderRequest.enc({
  context: { tag: "PocketCard", value: { cardId: "card" } },
  payload: "0x",
});

function fakeSubscription(log: string[]): WorkerRendererSubscription {
  return {
    cancel: () => log.push("cancel"),
    free: () => log.push("free"),
  };
}

/** Core stub that captures the render callbacks so a test can drive them. */
function fakeCore(
  log: string[],
  onStart?: (emit: {
    update: (node: Uint8Array) => void;
    complete: () => void;
    fail: (reason: string) => void;
  }) => void,
): WorkerProductRuntime {
  return {
    receiveFrame: async () => {},
    dispose: () => {},
    free: () => {},
    publishChatAction: (action) => log.push(`chat:${action.join(",")}`),
    publishRendererAction: (item) => log.push(`renderer:${item.join(",")}`),
    render: (_request, onUpdate, onComplete, onError) => {
      onStart?.({ update: onUpdate, complete: onComplete, fail: onError });
      return fakeSubscription(log);
    },
  };
}

describe("worker render subscription", () => {
  it("streams render items and releases the subscription on complete", () => {
    const messages: WorkerToMain[] = [];
    const log: string[] = [];
    const renders: RenderSubscriptions = new Map();
    let emit!: {
      update: (node: Uint8Array) => void;
      complete: () => void;
      fail: (reason: string) => void;
    };

    handleRenderStart(
      fakeCore(log, (e) => (emit = e)),
      (msg) => messages.push(msg),
      renders,
      1,
      5,
      renderRequest,
    );
    expect(renders.has(5)).toBe(true);

    emit.update(new Uint8Array([2, 3]));
    emit.complete();

    expect(messages).toEqual([
      { kind: "renderItem", renderId: 5, node: new Uint8Array([2, 3]) },
      { kind: "renderComplete", renderId: 5 },
    ]);
    // Completing must free the wasm handle, not just stop delivering.
    expect(log).toEqual(["cancel", "free"]);
    expect(renders.has(5)).toBe(false);
  });

  it("reports a render for an unknown core as a render error", () => {
    const messages: WorkerToMain[] = [];
    const renders: RenderSubscriptions = new Map();

    handleRenderStart(
      undefined,
      (msg) => messages.push(msg),
      renders,
      7,
      2,
      renderRequest,
    );

    expect(messages).toEqual([
      {
        kind: "renderError",
        renderId: 2,
        error: "render received for unknown core 7",
      },
    ]);
    expect(renders.size).toBe(0);
  });

  it("cancels only the renders belonging to the disposed core", () => {
    const log: string[] = [];
    const renders: RenderSubscriptions = new Map([
      [1, { coreId: 10, subscription: fakeSubscription(log) }],
      [2, { coreId: 11, subscription: fakeSubscription(log) }],
    ]);

    stopRendersForCore(renders, 10);

    expect([...renders.keys()]).toEqual([2]);
    expect(log).toEqual(["cancel", "free"]);
  });

  it("makes stopRender idempotent so a double dispose cannot double-free", () => {
    const log: string[] = [];
    const renders: RenderSubscriptions = new Map([
      [1, { coreId: 10, subscription: fakeSubscription(log) }],
    ]);

    stopRender(renders, 1);
    stopRender(renders, 1);

    expect(log).toEqual(["cancel", "free"]);
  });

  it("reports a declined render as an error, not a completion", () => {
    const messages: WorkerToMain[] = [];
    const log: string[] = [];
    const renders: RenderSubscriptions = new Map();
    let emit!: {
      update: (node: Uint8Array) => void;
      complete: () => void;
      fail: (reason: string) => void;
    };

    handleRenderStart(
      fakeCore(log, (e) => (emit = e)),
      (msg) => messages.push(msg),
      renders,
      1,
      6,
      renderRequest,
    );

    emit.update(new Uint8Array([9]));
    emit.fail("product interrupted the host-initiated subscription");

    // The partial tree must be followed by an error, never a completion.
    expect(messages).toEqual([
      { kind: "renderItem", renderId: 6, node: new Uint8Array([9]) },
      {
        kind: "renderError",
        renderId: 6,
        error: "product interrupted the host-initiated subscription",
      },
    ]);
    expect(renders.has(6)).toBe(false);
    expect(log).toEqual(["cancel", "free"]);
  });
});
