// Copyright 2026 Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: MIT
/**
 * Shared harness for the tests around `createWebWorkerPairingHostRuntime`.
 *
 * Lives outside the `.test.ts` files so more than one of them can drive a fake
 * worker: the runtime's tests were all in `worker-provider.test.ts`, which made
 * the coverage for `create-worker-host-runtime.ts` unfindable from the file it
 * covers. Excluded from the build in `tsconfig.json`, like `test-support.ts`.
 *
 * @module
 */

import { expect } from "bun:test";

import { makeHostCallbacks } from "../test-support.js";
import type { ProductRuntimeConfig } from "../runtime.js";
import { createWebWorkerPairingHostRuntime } from "./index.js";
import type { CreateWebWorkerPairingHostRuntimeOptions } from "./index.js";

export type WorkerMessage = Record<string, unknown>;

/** Minimal `Worker` stand-in that records posted messages and lets a test
 *  drive the `message`/`error`/`messageerror` events by hand. */
export class FakeWorker {
  listeners = new Map<string, Set<(event: unknown) => void>>();
  messages: WorkerMessage[] = [];
  terminated = false;

  addEventListener(name: string, fn: (event: unknown) => void) {
    const listeners = this.listeners.get(name) ?? new Set();
    listeners.add(fn);
    this.listeners.set(name, listeners);
  }

  removeEventListener(name: string, fn: (event: unknown) => void) {
    this.listeners.get(name)?.delete(fn);
  }

  postMessage(message: WorkerMessage) {
    this.messages.push(message);
  }

  terminate() {
    this.terminated = true;
  }

  emit(message: WorkerMessage) {
    for (const listener of this.listeners.get("message") ?? []) {
      listener({ data: message });
    }
  }

  emitError(message: string) {
    for (const listener of this.listeners.get("error") ?? []) {
      listener({ message });
    }
  }

  emitMessageError() {
    for (const listener of this.listeners.get("messageerror") ?? []) {
      listener({ data: null });
    }
  }
}

/** Coerce the `FakeWorker` to the `Worker` shape the provider expects. */
export function asWorker(worker: FakeWorker): Worker {
  return worker as unknown as Worker;
}

export function runtimeConfig(
  overrides: Partial<ProductRuntimeConfig> = {},
): ProductRuntimeConfig {
  return {
    productId: "dotli.dot",
    host: {
      name: "Polkadot Web",
      icon: "https://dot.li/dotli.png",
      version: "0.5.0",
    },
    platform: {
      type: "node",
      version: process.versions.node,
    },
    people: {
      genesisHash:
        "0xa22a2424d2cbf561eaecf7da8b1b548fa9d1939f60265e942b1049616a012f71",
    },
    bulletin: {
      genesisHash:
        "0xbbcccc1cbe333151b8ed63b17e9e0dec61ee53b57296f1fbe2d161ae3e6fb4dc",
    },
    assetHub: {
      genesisHash:
        "0xcccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc",
    },
    pairing: {
      deeplinkScheme: "polkadotapp",
    },
    ...overrides,
  };
}

export function hostConfigFromRuntimeConfig(
  config: ProductRuntimeConfig,
): CreateWebWorkerPairingHostRuntimeOptions["hostConfig"] {
  const {
    productId: _productId,
    executionKind: _executionKind,
    ...hostConfig
  } = config;
  return hostConfig;
}

export function lastMessageOfKind(
  worker: FakeWorker,
  kind: string,
): WorkerMessage {
  const message = [...worker.messages].reverse().find((m) => m.kind === kind);
  expect(message).toBeDefined();
  return message!;
}

export async function readyRuntime(
  worker: FakeWorker,
  options: { debugger?: string | null } = {},
) {
  const runtimePromise = createWebWorkerPairingHostRuntime(
    asWorker(worker),
    makeHostCallbacks(),
    { hostConfig: hostConfigFromRuntimeConfig(runtimeConfig()), ...options },
  );
  worker.emit({ kind: "loaded" });
  worker.emit({ kind: "ready" });
  return runtimePromise;
}
