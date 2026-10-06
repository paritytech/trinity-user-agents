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
  const requests: { name: string; args: readonly unknown[] }[] = [];
  const subscriptions: { name: string; args: readonly unknown[] }[] = [];
  return {
    requests,
    subscriptions,
    bridge: {
      callbackRequest: async (name: string, args: readonly unknown[]) => {
        requests.push({ name, args });
        return new Uint8Array();
      },
      startSubscription: (name: string, args: readonly unknown[]) => {
        subscriptions.push({ name, args });
        return () => {};
      },
      chainConnect: async () => null,
      hopConnect: async () => null,
    },
  };
}

describe("worker raw callbacks", () => {
  it("leaves username search unavailable when the host omits its capability", () => {
    const { bridge } = stubBridge();
    const callbacks = createWorkerRawCallbacks(
      bridge as unknown as Parameters<typeof createWorkerRawCallbacks>[0],
    );

    expect(callbacks.identityUsernameCandidates).toBeUndefined();
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

  it("starts no chat room subscription when chat is absent", () => {
    const { bridge, subscriptions } = stubBridge();
    const callbacks = createWorkerRawCallbacks(
      bridge as unknown as Parameters<typeof createWorkerRawCallbacks>[0],
    ) as RawCallbacks;

    const stop = startRawSubscription(
      callbacks,
      "subscribeChatRooms",
      [new Uint8Array([1])],
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
});
