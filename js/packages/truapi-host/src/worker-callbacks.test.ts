import { describe, expect, it } from "bun:test";

import {
  createWorkerRawCallbacks,
  startRawSubscription,
} from "./generated/worker-callbacks.js";
import type { RawCallbacks } from "./generated/host-callbacks-adapter.js";

// The worker proxies an optional capability only when the main thread reports
// the host serves it, so the core sees the same capability set on both sides of
// the boundary. Without that gate a worker host would always look chat-capable
// and the core would route chat calls at a host that cannot answer them.

function stubBridge() {
  const requests: {
    name: string;
    args: readonly unknown[];
    signal?: AbortSignal;
  }[] = [];
  const subscriptions: { name: string; payload: Uint8Array | null }[] = [];
  return {
    requests,
    subscriptions,
    bridge: {
      callbackRequest: async (
        name: string,
        args: readonly unknown[],
        signal?: AbortSignal,
      ) => {
        requests.push(signal ? { name, args, signal } : { name, args });
        return new Uint8Array();
      },
      startSubscription: (name: string, payload: Uint8Array | null) => {
        subscriptions.push({ name, payload });
        return () => {};
      },
      chainConnect: async () => null,
    },
  };
}

describe("worker raw callbacks", () => {
  it("hands a prompt's signal to the bridge, not to the main thread", async () => {
    const { bridge, requests } = stubBridge();
    const callbacks = createWorkerRawCallbacks(
      bridge as unknown as Parameters<typeof createWorkerRawCallbacks>[0],
    ) as unknown as RawCallbacks;
    const product = new Uint8Array([1]);
    const route = new Uint8Array([0]);
    const review = new Uint8Array([2]);
    const { signal } = new AbortController();

    await callbacks.confirmUserAction(product, route, review, { signal });
    await callbacks.readCoreStorage(new Uint8Array([3]));

    expect(requests).toEqual([
      { name: "confirmUserAction", args: [product, route, review], signal },
      { name: "readCoreStorage", args: [new Uint8Array([3])] },
    ]);
    // `toEqual` would accept any signal: the bridge must get the caller's own.
    expect(requests[0]?.signal).toBe(signal);
  });

  it("omits the chat proxies when no chat capability is reported", () => {
    const { bridge } = stubBridge();

    const callbacks = createWorkerRawCallbacks(
      bridge as unknown as Parameters<typeof createWorkerRawCallbacks>[0],
    );

    expect(callbacks.createChatRoom).toBeUndefined();
    expect(callbacks.postChatMessage).toBeUndefined();
    expect(callbacks.subscribeChatRooms).toBeUndefined();
    expect(callbacks.subscribeTheme).toBeDefined();
  });

  it("proxies chat through the bridge when the capability is reported", async () => {
    const { bridge, requests, subscriptions } = stubBridge();

    const callbacks = createWorkerRawCallbacks(
      bridge as unknown as Parameters<typeof createWorkerRawCallbacks>[0],
      { chat: true },
    );

    const product = new Uint8Array([1]);
    await (
      callbacks.createChatRoom as (
        product: Uint8Array,
        request: Uint8Array,
      ) => Promise<unknown>
    )(product, new Uint8Array([2]));
    (
      callbacks.subscribeChatRooms as (
        product: Uint8Array,
        sendItem: () => void,
        sendError: () => void,
      ) => void
    )(
      product,
      () => {},
      () => {},
    );

    expect(requests.map((r) => r.name)).toContain("createChatRoom");
    expect(subscriptions).toEqual([
      { name: "subscribeChatRooms", payload: product },
    ]);
  });

  it("starts no chat room subscription when chat is absent", () => {
    const { bridge, subscriptions } = stubBridge();
    const callbacks = createWorkerRawCallbacks(
      bridge as unknown as Parameters<typeof createWorkerRawCallbacks>[0],
    ) as RawCallbacks;

    const stop = startRawSubscription(
      callbacks,
      "subscribeChatRooms",
      new Uint8Array([1]),
      () => {},
      () => {},
    );

    expect(stop).toBeUndefined();
    expect(subscriptions).toEqual([]);
  });

  it("omits the pocket proxies when no pocket capability is reported", () => {
    const { bridge } = stubBridge();

    const callbacks = createWorkerRawCallbacks(
      bridge as unknown as Parameters<typeof createWorkerRawCallbacks>[0],
    );

    expect(callbacks.subscribePocketCards).toBeUndefined();
    expect(callbacks.removePocketCard).toBeUndefined();
  });

  it("proxies pocket through the bridge when the capability is reported", async () => {
    const { bridge, requests, subscriptions } = stubBridge();

    const callbacks = createWorkerRawCallbacks(
      bridge as unknown as Parameters<typeof createWorkerRawCallbacks>[0],
      { pocket: true },
    );

    const product = new Uint8Array([1]);
    await (
      callbacks.removePocketCard as (
        product: Uint8Array,
        request: Uint8Array,
      ) => Promise<unknown>
    )(product, new Uint8Array([2]));
    (
      callbacks.subscribePocketCards as (
        product: Uint8Array,
        sendItem: () => void,
        sendError: () => void,
      ) => void
    )(
      product,
      () => {},
      () => {},
    );

    expect(requests.map((r) => r.name)).toContain("removePocketCard");
    expect(subscriptions).toEqual([
      { name: "subscribePocketCards", payload: product },
    ]);
  });
});
