// Copyright 2026 Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: AGPL-3.0-only

// The loopback wire: product client <-> core, in one process.
//
// This is the structural difference from the web host: no iframe, no
// postMessage, no separate origin. The product and the core share a process,
// so the wire is a plain in-memory pipe.

import type { WireProvider } from "@parity/truapi-host";

/** The callbacks the core hands its frames to. */
export interface CoreWireCallbacks {
  emitFrame(frame: Uint8Array): void;
  dispose(): void;
}

/** The slice of a product core the loopback needs. */
export interface FrameReceiver {
  receiveFrame(frame: Uint8Array): Promise<void>;
}

/**
 * Wire a product core to a `WireProvider` over an in-process pipe.
 *
 * `receiveFrame` is invoked SYNCHRONOUSLY per posted frame so frames enter the
 * core in post order (the tier-2 spike measured a pipe with its own queueing
 * manufacturing the exact operation-ordering inversion under test). Its
 * returned promise is only observed for errors. A frame like `requestLogin`
 * can stay pending for minutes, so completion must not gate later frames.
 */
export function createLoopbackProvider<C extends FrameReceiver>(
  makeCore: (callbacks: CoreWireCallbacks) => C,
  options: { onReceiveError?: (error: unknown) => void } = {},
): { provider: WireProvider; core: C } {
  const subscribers = new Set<(frame: Uint8Array) => void>();
  const core = makeCore({
    emitFrame(frame) {
      for (const callback of [...subscribers]) {
        callback(frame);
      }
    },
    dispose() {},
  });
  const provider: WireProvider = {
    postMessage(frame: Uint8Array): void {
      core.receiveFrame(frame).catch((error: unknown) => {
        options.onReceiveError?.(error);
      });
    },
    subscribe(callback) {
      subscribers.add(callback);
      return () => subscribers.delete(callback);
    },
    dispose() {
      subscribers.clear();
    },
  };
  return { provider, core };
}
