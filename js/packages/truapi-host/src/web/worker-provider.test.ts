import { afterEach, describe, expect, it } from "bun:test";
import { err, ok } from "neverthrow";

import {
  HostPushNotificationRequest,
  HostPushNotificationResponse,
  RendererNode,
} from "@parity/truapi";
import { bytesToHex } from "@parity/truapi/scale";
import type {
  GenericError,
  ProductRendererRenderRequest,
  Result,
  ThemeVariant,
} from "@parity/truapi";

import { createWasmRawCallbacks } from "../generated/host-callbacks-adapter.js";
import {
  AuthState,
  CoreStorageKey,
  ProductContext,
} from "../generated/host-callbacks.js";
import type {
  AuthState as AuthStateValue,
  PreimageHost,
} from "../generated/host-callbacks.js";
import type {
  ProductRuntimeConfig,
  TrUApiProductProvider,
  WorkerDemandChange,
} from "../runtime.js";
import { makeHostCallbacks, settle } from "../test-support.js";
import {
  asWorker,
  FakeWorker,
  hostConfigFromRuntimeConfig,
  lastMessageOfKind,
  readyRuntime,
  runtimeConfig,
} from "./worker-test-harness.js";
import type { WorkerMessage } from "./worker-test-harness.js";
import { createWebWorkerPairingHostRuntime } from "./index.js";
import type { CreateWebWorkerPairingHostRuntimeOptions } from "./index.js";

function renderRequest(): ProductRendererRenderRequest {
  return {
    context: { tag: "PocketCard", value: { cardId: "card" } },
    payload: "0x",
  };
}

/** Position of the last message of `kind` in post order, or -1 if never sent. */
function indexOfKind(worker: FakeWorker, kind: string): number {
  let index = -1;
  worker.messages.forEach((message, at) => {
    if (message.kind === kind) index = at;
  });
  return index;
}

async function finishProviderReady(
  worker: FakeWorker,
  providerPromise: Promise<TrUApiProductProvider>,
) {
  await settle();
  const createCore = lastMessageOfKind(worker, "createCore");
  worker.emit({ kind: "coreReady", coreId: createCore.coreId });
  return providerPromise;
}

type ReadyOptions = Partial<
  Omit<CreateWebWorkerPairingHostRuntimeOptions, "hostConfig">
> & {
  createWebWorkerPairingHostRuntime?: typeof createWebWorkerPairingHostRuntime;
  runtimeConfig?: ProductRuntimeConfig;
};

async function createProviderFromRuntime(
  worker: Worker,
  host: ReturnType<typeof makeHostCallbacks>,
  options: ReadyOptions,
): Promise<TrUApiProductProvider> {
  const {
    createWebWorkerPairingHostRuntime:
      createRuntime = createWebWorkerPairingHostRuntime,
    runtimeConfig: cfg = runtimeConfig(),
    ...runtimeOptions
  } = options;
  const runtime = await createRuntime(worker, host, {
    ...runtimeOptions,
    hostConfig: hostConfigFromRuntimeConfig(cfg),
  });
  const provider = await runtime.createProvider(
    cfg.executionKind === undefined
      ? { productId: cfg.productId }
      : { productId: cfg.productId, executionKind: cfg.executionKind },
  );
  return {
    ...provider,
    dispose(): void {
      provider.dispose();
      runtime.dispose();
    },
  };
}

async function readyProvider(worker: FakeWorker, options: ReadyOptions = {}) {
  const providerPromise = createProviderFromRuntime(
    asWorker(worker),
    makeHostCallbacks(),
    options,
  );
  worker.emit({ kind: "loaded" });
  worker.emit({ kind: "ready" });
  return finishProviderReady(worker, providerPromise);
}

/** Typed view of the dev console the worker runtime publishes on `globalThis`. */
type TruapiDevConsole = {
  setLogLevel(level: string): void;
  getLogLevel(): string | null;
};
const devGlobal = globalThis as typeof globalThis & {
  __truapi?: TruapiDevConsole;
};

describe("createWebWorkerPairingHostRuntime", () => {
  it("initializes the worker without a callback manifest", async () => {
    const worker = new FakeWorker();
    const config = runtimeConfig();
    const providerPromise = createProviderFromRuntime(
      asWorker(worker),
      makeHostCallbacks(),
      {
        logLevel: "debug",
        runtimeConfig: config,
      },
    );

    worker.emit({ kind: "loaded" });
    expect(worker.messages.length).toBe(1);
    expect(worker.messages[0]).toEqual({
      kind: "init",
      logLevel: "debug",
      hostConfig: hostConfigFromRuntimeConfig(config),
      capabilities: {
        chat: false,
        permissionStatus: false,
        pocket: false,
        game: false,
        contacts: false,
      },
      // Null under `bun test`: the `import.meta.env.DEV` gate reads undefined,
      // so no dial resolves and the worker builds no tap.
      debuggerUrl: null,
    });

    worker.emit({ kind: "ready" });
    await settle();
    const createCore = lastMessageOfKind(worker, "createCore");
    expect(createCore).toEqual({
      kind: "createCore",
      coreId: 1,
      product: { productId: "dotli.dot" },
    });
    worker.emit({ kind: "coreReady", coreId: 1 });
    const provider = await providerPromise;
    expect(typeof provider.disconnectSession).toBe("function");

    provider.dispose();
  });

  it("reports the chat capability to the worker when the host serves it", async () => {
    const worker = new FakeWorker();
    void createWebWorkerPairingHostRuntime(
      asWorker(worker),
      makeHostCallbacks({
        chat: { createChatRoom: async () => ({ status: "New" }) },
      }),
      { hostConfig: hostConfigFromRuntimeConfig(runtimeConfig()) },
    );

    worker.emit({ kind: "loaded" });

    expect(lastMessageOfKind(worker, "init").capabilities).toEqual({
      chat: true,
      permissionStatus: false,
      pocket: false,
      game: false,
      contacts: false,
    });
  });

  it("reports the pocket capability to the worker when the host serves it", async () => {
    const worker = new FakeWorker();
    void createWebWorkerPairingHostRuntime(
      asWorker(worker),
      makeHostCallbacks({
        pocket: { removePocketCard: async () => {} },
      }),
      { hostConfig: hostConfigFromRuntimeConfig(runtimeConfig()) },
    );

    worker.emit({ kind: "loaded" });

    // Without this the worker never builds the pocket callbacks, so a host
    // that serves Pocket is answered `Unsupported` anyway.
    expect(lastMessageOfKind(worker, "init").capabilities).toEqual({
      chat: false,
      permissionStatus: false,
      pocket: true,
      game: false,
      contacts: false,
    });
  });

  it("reports the game capability to the worker when the host serves it", async () => {
    const worker = new FakeWorker();
    void createWebWorkerPairingHostRuntime(
      asWorker(worker),
      makeHostCallbacks({
        game: {
          scheduleGameReminder: async () => {},
          cancelGameReminder: async () => {},
        },
      }),
      { hostConfig: hostConfigFromRuntimeConfig(runtimeConfig()) },
    );

    worker.emit({ kind: "loaded" });

    // Without this the worker never builds the game callbacks, so a host that
    // holds reminders is answered `Unsupported` anyway.
    expect(lastMessageOfKind(worker, "init").capabilities).toEqual({
      chat: false,
      permissionStatus: false,
      pocket: false,
      game: true,
      contacts: false,
    });
  });

  it("creates multiple product cores on one worker runtime", async () => {
    const worker = new FakeWorker();
    const config = runtimeConfig();
    const runtimePromise = createWebWorkerPairingHostRuntime(
      asWorker(worker),
      makeHostCallbacks(),
      {
        hostConfig: hostConfigFromRuntimeConfig(config),
      },
    );
    worker.emit({ kind: "loaded" });
    worker.emit({ kind: "ready" });
    const runtime = await runtimePromise;

    const firstPromise = runtime.createProvider({ productId: "first.dot" });
    const secondPromise = runtime.createProvider({ productId: "second.dot" });

    expect(worker.messages.at(-2)).toEqual({
      kind: "createCore",
      coreId: 1,
      product: { productId: "first.dot" },
    });
    expect(worker.messages.at(-1)).toEqual({
      kind: "createCore",
      coreId: 2,
      product: { productId: "second.dot" },
    });

    worker.emit({ kind: "coreReady", coreId: 1 });
    worker.emit({ kind: "coreReady", coreId: 2 });
    const first = await firstPromise;
    const second = await secondPromise;

    const firstFrames: Uint8Array[] = [];
    const secondFrames: Uint8Array[] = [];
    first.subscribe((frame) => firstFrames.push(frame));
    second.subscribe((frame) => secondFrames.push(frame));

    worker.emit({ kind: "frame", coreId: 2, bytes: new Uint8Array([2]) });
    worker.emit({ kind: "frame", coreId: 1, bytes: new Uint8Array([1]) });
    expect(firstFrames).toEqual([new Uint8Array([1])]);
    expect(secondFrames).toEqual([new Uint8Array([2])]);

    first.postMessage(new Uint8Array([9]));
    expect(worker.messages.at(-1)).toEqual({
      kind: "frame",
      coreId: 1,
      bytes: new Uint8Array([9]),
    });

    first.dispose();
    expect(worker.messages.at(-1)).toEqual({ kind: "disposeCore", coreId: 1 });

    worker.emit({ kind: "frame", coreId: 2, bytes: new Uint8Array([3]) });
    expect(firstFrames).toEqual([new Uint8Array([1])]);
    expect(secondFrames).toEqual([new Uint8Array([2]), new Uint8Array([3])]);

    runtime.dispose();
    expect(worker.messages.at(-1)).toEqual({ kind: "dispose" });
  });

  it("binds a host-selected execution kind to the product core", async () => {
    const worker = new FakeWorker();
    const config = runtimeConfig({ executionKind: "Worker" });
    const providerPromise = createProviderFromRuntime(
      asWorker(worker),
      makeHostCallbacks(),
      { runtimeConfig: config },
    );

    worker.emit({ kind: "loaded" });
    worker.emit({ kind: "ready" });
    await settle();

    const createCore = lastMessageOfKind(worker, "createCore");
    expect(createCore).toEqual({
      kind: "createCore",
      coreId: 1,
      product: { productId: "dotli.dot", executionKind: "Worker" },
    });
    worker.emit({ kind: "coreReady", coreId: 1 });
    (await providerPromise).dispose();
  });

  it("dev global setLogLevel updates every live worker provider", async () => {
    const previous = devGlobal.__truapi;
    delete devGlobal.__truapi;
    const firstWorker = new FakeWorker();
    const secondWorker = new FakeWorker();
    const first = await readyProvider(firstWorker);
    const second = await readyProvider(secondWorker);

    devGlobal.__truapi!.setLogLevel("debug");

    expect(firstWorker.messages.at(-1)).toEqual({
      kind: "setLogLevel",
      level: "debug",
    });
    expect(secondWorker.messages.at(-1)).toEqual({
      kind: "setLogLevel",
      level: "debug",
    });
    expect(devGlobal.__truapi!.getLogLevel()).toBe("debug");

    devGlobal.__truapi!.setLogLevel("off");
    first.dispose();
    second.dispose();
    if (previous === undefined) {
      delete devGlobal.__truapi;
    } else {
      devGlobal.__truapi = previous;
    }
  });

  it("dev global setLogLevel applies to providers created later", async () => {
    const previous = devGlobal.__truapi;
    delete devGlobal.__truapi;
    const moduleUrl = `./create-worker-host-runtime.js?dev-global-${Date.now()}`;
    const {
      createWebWorkerPairingHostRuntime: freshCreateWebWorkerPairingHostRuntime,
    } = (await import(
      moduleUrl
    )) as typeof import("./create-worker-host-runtime.js");

    expect(typeof devGlobal.__truapi!.setLogLevel).toBe("function");
    devGlobal.__truapi!.setLogLevel("trace");

    const firstWorker = new FakeWorker();
    const first = await readyProvider(firstWorker, {
      createWebWorkerPairingHostRuntime: freshCreateWebWorkerPairingHostRuntime,
    });
    first.dispose();

    const secondWorker = new FakeWorker();
    const second = await readyProvider(secondWorker, {
      createWebWorkerPairingHostRuntime: freshCreateWebWorkerPairingHostRuntime,
    });

    expect(secondWorker.messages[0].kind).toBe("init");
    expect(secondWorker.messages[0].logLevel).toBe("trace");
    expect(
      secondWorker.messages.some((message) => {
        return message.kind === "setLogLevel" && message.level === "trace";
      }),
    ).toBe(true);

    second.dispose();
    devGlobal.__truapi!.setLogLevel("off");
    if (previous === undefined) {
      delete devGlobal.__truapi;
    } else {
      devGlobal.__truapi = previous;
    }
  });

  it("dev global setLogLevel persists the level to localStorage", async () => {
    const previousGlobal = devGlobal.__truapi;
    const previousStorage = globalThis.localStorage;
    delete devGlobal.__truapi;
    const store = new Map<string, string>();
    globalThis.localStorage = {
      getItem: (key: string) => (store.has(key) ? store.get(key)! : null),
      setItem: (key: string, value: string) => store.set(key, String(value)),
    } as unknown as Storage;

    const worker = new FakeWorker();
    const provider = await readyProvider(worker);

    devGlobal.__truapi!.setLogLevel("debug");
    expect(store.get("truapi:logLevel")).toBe("debug");

    devGlobal.__truapi!.setLogLevel("off");
    expect(store.get("truapi:logLevel")).toBe("off");

    provider.dispose();
    globalThis.localStorage = previousStorage;
    if (previousGlobal === undefined) {
      delete devGlobal.__truapi;
    } else {
      devGlobal.__truapi = previousGlobal;
    }
  });

  it("resolves disconnect responses", async () => {
    const worker = new FakeWorker();
    const providerPromise = createProviderFromRuntime(
      asWorker(worker),
      makeHostCallbacks(),
      {
        runtimeConfig: runtimeConfig(),
      },
    );
    worker.emit({ kind: "loaded" });
    worker.emit({ kind: "ready" });
    const provider = await finishProviderReady(worker, providerPromise);

    const disconnect = provider.disconnectSession();
    const msg = worker.messages.at(-1)!;
    expect(msg.kind).toBe("disconnectSession");
    expect(typeof msg.requestId).toBe("number");

    worker.emit({
      kind: "disconnectSessionResponse",
      requestId: msg.requestId,
      ok: true,
    });
    await disconnect;

    provider.dispose();
  });

  it("returns the session chat identity key as hex, and undefined when absent", async () => {
    const worker = new FakeWorker();
    const providerPromise = createProviderFromRuntime(
      asWorker(worker),
      makeHostCallbacks(),
      {
        runtimeConfig: runtimeConfig(),
      },
    );
    worker.emit({ kind: "loaded" });
    worker.emit({ kind: "ready" });
    const provider = await finishProviderReady(worker, providerPromise);

    const keyBytes = new Uint8Array(32).fill(0x77);
    const pending = provider.getSessionChatIdentityKey();
    const msg = worker.messages.at(-1)!;
    expect(msg.kind).toBe("getSessionChatIdentityKey");

    worker.emit({
      kind: "sessionChatIdentityKeyResponse",
      requestId: msg.requestId,
      ok: true,
      key: keyBytes,
    });
    expect(await pending).toBe(bytesToHex(keyBytes));

    // A session that predates retention reports absence rather than an error.
    const missing = provider.getSessionChatIdentityKey();
    worker.emit({
      kind: "sessionChatIdentityKeyResponse",
      requestId: worker.messages.at(-1)!.requestId,
      ok: true,
      key: undefined,
    });
    expect(await missing).toBeUndefined();

    provider.dispose();
  });

  it("returns the device statement key, and undefined when absent", async () => {
    const worker = new FakeWorker();
    const provider = await readyProvider(worker, {
      runtimeConfig: runtimeConfig(),
    });

    const keyBytes = new Uint8Array(64).fill(0x5a);
    const pending = provider.getDeviceStatementKey();
    const msg = lastMessageOfKind(worker, "getDeviceStatementKey");

    worker.emit({
      kind: "deviceStatementKeyResponse",
      requestId: msg.requestId,
      ok: true,
      key: keyBytes,
    });
    expect(await pending).toEqual(keyBytes);

    // No active session means no advertised account, so absence is not an error.
    const missing = provider.getDeviceStatementKey();
    worker.emit({
      kind: "deviceStatementKeyResponse",
      requestId: lastMessageOfKind(worker, "getDeviceStatementKey").requestId,
      ok: true,
      key: undefined,
    });
    expect(await missing).toBeUndefined();

    provider.dispose();
  });

  it("returns a product subtree key as hex and surfaces a wallet deadline", async () => {
    const worker = new FakeWorker();
    const providerPromise = createProviderFromRuntime(
      asWorker(worker),
      makeHostCallbacks(),
      {
        runtimeConfig: runtimeConfig(),
      },
    );
    worker.emit({ kind: "loaded" });
    worker.emit({ kind: "ready" });
    const provider = await finishProviderReady(worker, providerPromise);

    const keyBytes = new Uint8Array(32).fill(0x31);
    const pending = provider.getProductSubtreePublicKey("myapp.dot", 5000);
    const msg = worker.messages.at(-1)!;
    expect(msg.kind).toBe("getProductSubtreePublicKey");
    expect(msg.productId).toBe("myapp.dot");
    expect(msg.timeoutMs).toBe(5000);

    worker.emit({
      kind: "productSubtreePublicKeyResponse",
      requestId: msg.requestId,
      ok: true,
      key: keyBytes,
    });
    expect(await pending).toBe(bytesToHex(keyBytes));

    // A wallet that never answers ends at the deadline, and that is an error
    // rather than an absent key, so a host can tell it apart from having no
    // session at all.
    const late = provider.getProductSubtreePublicKey("slow.dot", 1);
    worker.emit({
      kind: "productSubtreePublicKeyResponse",
      requestId: worker.messages.at(-1)!.requestId,
      ok: false,
      error: "Account authority request timed out after 1ms",
    });
    await expect(late).rejects.toThrow("timed out");

    provider.dispose();
  });

  it("returns the device encryption key as hex and fails once disposed", async () => {
    const worker = new FakeWorker();
    const providerPromise = createProviderFromRuntime(
      asWorker(worker),
      makeHostCallbacks(),
      {
        runtimeConfig: runtimeConfig(),
      },
    );
    worker.emit({ kind: "loaded" });
    worker.emit({ kind: "ready" });
    const provider = await finishProviderReady(worker, providerPromise);

    const keyBytes = new Uint8Array(32).fill(0x5a);
    const pending = provider.getDeviceEncryptionKey();
    const msg = worker.messages.at(-1)!;
    expect(msg.kind).toBe("getDeviceEncryptionKey");

    worker.emit({
      kind: "deviceEncryptionKeyResponse",
      requestId: msg.requestId,
      ok: true,
      key: keyBytes,
    });
    expect(await pending).toBe(bytesToHex(keyBytes));

    const failing = provider.getDeviceEncryptionKey();
    worker.emit({
      kind: "deviceEncryptionKeyResponse",
      requestId: worker.messages.at(-1)!.requestId,
      ok: false,
      error: "no device key",
    });
    await expect(failing).rejects.toThrow("no device key");

    // A key has no safe empty value, so a closed connection rejects.
    provider.dispose();
    await expect(provider.getDeviceEncryptionKey()).rejects.toThrow(
      "product connection is closed",
    );
  });

  it("forwards session activation calls and resolves their responses", async () => {
    const worker = new FakeWorker();
    const runtime = await readyRuntime(worker);
    const blob = new Uint8Array([1, 2, 3]);

    for (const [kind, call] of [
      ["activateStoredSession", () => runtime.activateStoredSession()],
      ["activateExternalSession", () => runtime.activateExternalSession(blob)],
      ["resetSessionState", () => runtime.resetSessionState()],
    ] as const) {
      const pending = call();
      const msg = lastMessageOfKind(worker, kind);
      expect(typeof msg.requestId).toBe("number");
      worker.emit({
        kind: "sessionActivationResponse",
        requestId: msg.requestId,
        ok: true,
      });
      await pending;
    }

    expect(lastMessageOfKind(worker, "activateExternalSession").blob).toEqual(
      blob,
    );

    runtime.dispose();
  });

  it("rejects a session activation the core could not complete", async () => {
    const worker = new FakeWorker();
    const runtime = await readyRuntime(worker);

    const pending = runtime.activateStoredSession();
    const msg = lastMessageOfKind(worker, "activateStoredSession");
    worker.emit({
      kind: "sessionActivationResponse",
      requestId: msg.requestId,
      ok: false,
      error: "no stored session",
    });

    await expect(pending).rejects.toThrow("no stored session");

    runtime.dispose();
  });

  it("rejects a session activation still in flight when the worker faults", async () => {
    const worker = new FakeWorker();
    const runtime = await readyRuntime(worker);

    const pending = runtime.activateStoredSession();
    worker.emitError("boom");

    await expect(pending).rejects.toThrow(/boom/);
  });

  it("rejects session activation calls made after the runtime is gone", async () => {
    const worker = new FakeWorker();
    const runtime = await readyRuntime(worker);
    const blob = new Uint8Array([1, 2, 3]);
    worker.emitError("boom");

    // Resolving here would tell a host at boot that the activation ran and
    // found no session, when nothing was ever sent to the worker.
    await expect(runtime.activateStoredSession()).rejects.toThrow(/boom/);
    await expect(runtime.activateExternalSession(blob)).rejects.toThrow(/boom/);
    await expect(runtime.resetSessionState()).rejects.toThrow(/boom/);

    runtime.dispose();
    await expect(runtime.activateStoredSession()).rejects.toThrow();
  });

  it("dispatches callback requests to host hooks", async () => {
    const worker = new FakeWorker();
    let clears = 0;
    const authSessionKey = CoreStorageKey.enc({ tag: "AuthSession" });
    const providerPromise = createProviderFromRuntime(
      asWorker(worker),
      makeHostCallbacks({
        coreStorage: {
          clearCoreStorage: async (key) => {
            expect(key).toEqual({ tag: "AuthSession", value: undefined });
            clears += 1;
          },
        },
      }),
      { runtimeConfig: runtimeConfig() },
    );
    worker.emit({ kind: "loaded" });
    worker.emit({ kind: "ready" });
    const provider = await finishProviderReady(worker, providerPromise);

    worker.emit({
      kind: "callbackRequest",
      requestId: 7,
      name: "clearCoreStorage",
      args: [authSessionKey],
    });
    await settle();

    expect(clears).toBe(1);
    expect(worker.messages.at(-1)).toEqual({
      kind: "callbackResponse",
      requestId: 7,
      ok: true,
      value: undefined,
    });

    provider.dispose();
  });

  it("reports unknown callback requests", async () => {
    const worker = new FakeWorker();
    const providerPromise = createProviderFromRuntime(
      asWorker(worker),
      makeHostCallbacks(),
      {
        runtimeConfig: runtimeConfig(),
      },
    );
    worker.emit({ kind: "loaded" });
    worker.emit({ kind: "ready" });
    const provider = await finishProviderReady(worker, providerPromise);

    worker.emit({
      kind: "callbackRequest",
      requestId: 11,
      name: "someFutureCallback",
      args: [new Uint8Array([1, 2, 3])],
    });
    await settle();

    expect(worker.messages.at(-1)).toEqual({
      kind: "callbackResponse",
      requestId: 11,
      ok: false,
      error: "unknown callback: someFutureCallback",
    });

    provider.dispose();
  });

  it("forwards authStateChanged callback requests", async () => {
    const worker = new FakeWorker();
    const states: AuthStateValue[] = [];
    const providerPromise = createProviderFromRuntime(
      asWorker(worker),
      makeHostCallbacks({
        auth: {
          authStateChanged: (state) => {
            states.push(state);
          },
        },
      }),
      { runtimeConfig: runtimeConfig() },
    );
    worker.emit({ kind: "loaded" });
    worker.emit({ kind: "ready" });
    const provider = await finishProviderReady(worker, providerPromise);
    const publicKeyBytes = new Uint8Array(32);
    publicKeyBytes.set([1, 2]);
    const publicKey = bytesToHex(publicKeyBytes);

    worker.emit({
      kind: "callbackRequest",
      requestId: 3,
      name: "authStateChanged",
      args: [
        AuthState.enc({
          tag: "Connected",
          value: {
            publicKey,
            liteUsername: "alice",
          },
        }),
      ],
    });
    await settle();

    expect(states).toEqual([
      {
        tag: "Connected",
        value: {
          publicKey,
          liteUsername: "alice",
        },
      },
    ]);
    expect(worker.messages.at(-1)).toEqual({
      kind: "callbackResponse",
      requestId: 3,
      ok: true,
      value: undefined,
    });

    provider.dispose();
  });

  it("posts cancelPairing to the worker", async () => {
    const worker = new FakeWorker();
    const config = runtimeConfig();
    const runtimePromise = createWebWorkerPairingHostRuntime(
      asWorker(worker),
      makeHostCallbacks(),
      {
        hostConfig: hostConfigFromRuntimeConfig(config),
      },
    );
    worker.emit({ kind: "loaded" });
    worker.emit({ kind: "ready" });
    const runtime = await runtimePromise;

    runtime.cancelPairing();

    expect(worker.messages.at(-1)).toEqual({ kind: "cancelPairing" });
    runtime.dispose();
  });

  it("posts notifySessionStoreChanged to the worker", async () => {
    const worker = new FakeWorker();
    const config = runtimeConfig();
    const runtimePromise = createWebWorkerPairingHostRuntime(
      asWorker(worker),
      makeHostCallbacks(),
      {
        hostConfig: hostConfigFromRuntimeConfig(config),
      },
    );
    worker.emit({ kind: "loaded" });
    worker.emit({ kind: "ready" });
    const runtime = await runtimePromise;

    runtime.notifySessionStoreChanged();

    expect(worker.messages.at(-1)).toEqual({
      kind: "notifySessionStoreChanged",
    });
    runtime.dispose();
  });

  it("posts notifyContactsChanged to the worker", async () => {
    const worker = new FakeWorker();
    const config = runtimeConfig();
    const runtimePromise = createWebWorkerPairingHostRuntime(
      asWorker(worker),
      makeHostCallbacks(),
      {
        hostConfig: hostConfigFromRuntimeConfig(config),
      },
    );
    worker.emit({ kind: "loaded" });
    worker.emit({ kind: "ready" });
    const runtime = await runtimePromise;

    runtime.notifyContactsChanged();

    expect(worker.messages.at(-1)).toEqual({ kind: "notifyContactsChanged" });
    runtime.dispose();
  });

  it("worker fault terminates the worker and runs the full teardown", async () => {
    const worker = new FakeWorker();
    let subscriptionDisposes = 0;
    let chainResponseStops = 0;
    let chainCloses = 0;
    const providerPromise = createProviderFromRuntime(
      asWorker(worker),
      makeHostCallbacks({
        // Manual async iterables whose `return()` records disposal; the
        // provider disposes subscriptions and closes chain connections
        // on a worker fault.
        theme: {
          subscribeTheme: () =>
            ({
              [Symbol.asyncIterator]() {
                return this;
              },
              next: () => new Promise(() => {}),
              return: async () => {
                subscriptionDisposes += 1;
                return { done: true, value: undefined };
              },
            }) as unknown as AsyncIterable<Result<ThemeVariant, GenericError>>,
        },
        chain: {
          connect: async () => ({
            send() {},
            responses: () =>
              ({
                [Symbol.asyncIterator]() {
                  return this;
                },
                next: () => new Promise(() => {}),
                return: async () => {
                  chainResponseStops += 1;
                  return { done: true, value: undefined };
                },
              }) as unknown as AsyncIterable<string>,
            close() {
              chainCloses += 1;
            },
          }),
        },
      }),
      { runtimeConfig: runtimeConfig() },
    );
    worker.emit({ kind: "loaded" });
    worker.emit({ kind: "ready" });
    const provider = await finishProviderReady(worker, providerPromise);

    worker.emit({
      kind: "subscriptionStart",
      subId: 1,
      name: "subscribeTheme",
      payload: null,
    });
    worker.emit({
      kind: "chainConnectStart",
      connId: 1,
      genesisHash: "0xab",
    });
    await settle();

    const closes: Error[] = [];
    provider.subscribeClose!((error) => closes.push(error));

    worker.emitError("boom");
    await settle();

    expect(worker.terminated).toBe(true);
    expect(subscriptionDisposes).toBe(1);
    expect(chainResponseStops).toBe(1);
    expect(chainCloses).toBe(1);
    expect(closes.length).toBe(1);
    expect(closes[0].message).toMatch(/boom/);

    // The fault teardown is terminal; a second fault is a no-op.
    worker.emitError("again");
    expect(closes.length).toBe(1);

    let lateClose: Error | null = null;
    provider.subscribeClose!((error) => {
      lateClose = error;
    });
    expect(lateClose).toBeInstanceOf(Error);
    expect(lateClose!.message).toMatch(/boom/);
  });

  it("worker fatalError during init rejects provider creation", async () => {
    const worker = new FakeWorker();
    const providerPromise = createProviderFromRuntime(
      asWorker(worker),
      makeHostCallbacks(),
      {
        runtimeConfig: runtimeConfig(),
      },
    );

    worker.emit({ kind: "fatalError", error: "bad wasm" });

    await expect(providerPromise).rejects.toThrow(
      /worker init reported error: bad wasm/,
    );
    expect(worker.terminated).toBe(true);
  });

  it("worker frameError after init closes the provider", async () => {
    const worker = new FakeWorker();
    const provider = await readyProvider(worker);
    const closes: Error[] = [];
    provider.subscribeClose!((error) => closes.push(error));

    worker.emit({ kind: "frameError", coreId: 1, error: "bad frame" });

    expect(worker.terminated).toBe(false);
    expect(worker.messages.at(-1)).toEqual({ kind: "disposeCore", coreId: 1 });
    expect(closes.length).toBe(1);
    expect(closes[0].message).toMatch(/worker frame error: bad frame/);

    let lateClose: Error | null = null;
    provider.subscribeClose!((error) => {
      lateClose = error;
    });
    expect(lateClose).toBeInstanceOf(Error);
    provider.dispose();
  });

  it("keeps the worker alive until pending operations end", async () => {
    const worker = new FakeWorker();
    const provider = await readyProvider(worker, {
      runtimeConfig: runtimeConfig({ executionKind: "Worker" }),
    });

    const product = ProductContext.enc({
      productId: "dotli.dot",
      executionKind: "Worker",
    });

    worker.emit({
      kind: "callbackRequest",
      requestId: 1,
      name: "beginOperation",
      args: [product, "funding"],
    });
    await settle();

    provider.dispose();
    await settle();
    expect(worker.terminated).toBe(false);

    // The deferred teardown terminates the worker on a zero-delay timer.
    worker.emit({
      kind: "callbackRequest",
      requestId: 2,
      name: "endOperation",
      args: [product, 1],
    });
    await settle();
    await new Promise((resolve) => setTimeout(resolve, 10));
    expect(worker.terminated).toBe(true);
  });

  it("stops waiting for an operation that never ends", async () => {
    const worker = new FakeWorker();
    const provider = await readyProvider(worker, {
      runtimeConfig: runtimeConfig({ executionKind: "Worker" }),
      operationGraceMs: 10,
    });

    const product = ProductContext.enc({
      productId: "dotli.dot",
      executionKind: "Worker",
    });
    worker.emit({
      kind: "callbackRequest",
      requestId: 1,
      name: "beginOperation",
      args: [product, "funding"],
    });
    await settle();

    provider.dispose();
    await settle();
    expect(worker.terminated).toBe(false);

    // The operation never ends, so the grace period is what tears the worker
    // down rather than leaving the core running for a product that is gone.
    await new Promise((resolve) => setTimeout(resolve, 60));
    expect(worker.terminated).toBe(true);
  });

  it("ending an unknown or already-ended operation releases no other hold", async () => {
    const worker = new FakeWorker();
    let nextId = 1;
    const providerPromise = createProviderFromRuntime(
      asWorker(worker),
      makeHostCallbacks({
        productOperations: { beginOperation: async () => ({ id: nextId++ }) },
      }),
      { runtimeConfig: runtimeConfig({ executionKind: "Worker" }) },
    );
    worker.emit({ kind: "loaded" });
    worker.emit({ kind: "ready" });
    const provider = await finishProviderReady(worker, providerPromise);

    const product = ProductContext.enc({
      productId: "dotli.dot",
      executionKind: "Worker",
    });
    const begin = (requestId: number) =>
      worker.emit({
        kind: "callbackRequest",
        requestId,
        name: "beginOperation",
        args: [product, ""],
      });
    const end = (requestId: number, id: number) =>
      worker.emit({
        kind: "callbackRequest",
        requestId,
        name: "endOperation",
        args: [product, id],
      });

    begin(1);
    begin(2);
    await settle();
    provider.dispose();

    // Operation 1 ends twice and an unknown id ends once. Operation 2 still
    // holds the worker.
    end(3, 1);
    end(4, 1);
    end(5, 99);
    await settle();
    await new Promise((resolve) => setTimeout(resolve, 10));
    expect(worker.terminated).toBe(false);

    end(6, 2);
    await settle();
    await new Promise((resolve) => setTimeout(resolve, 10));
    expect(worker.terminated).toBe(true);
  });

  it("keeps two products' operation holds apart when their ids collide", async () => {
    const worker = new FakeWorker();
    const providerPromise = createProviderFromRuntime(
      asWorker(worker),
      // `OperationId` is unique per product, so a host may hand the same id to
      // every product it serves.
      makeHostCallbacks({
        productOperations: { beginOperation: async () => ({ id: 1 }) },
      }),
      { runtimeConfig: runtimeConfig({ executionKind: "Worker" }) },
    );
    worker.emit({ kind: "loaded" });
    worker.emit({ kind: "ready" });
    const provider = await finishProviderReady(worker, providerPromise);

    const productFor = (productId: string) =>
      ProductContext.enc({ productId, executionKind: "Worker" });
    const first = productFor("first.dot");
    const second = productFor("second.dot");

    worker.emit({
      kind: "callbackRequest",
      requestId: 1,
      name: "beginOperation",
      args: [first, ""],
    });
    worker.emit({
      kind: "callbackRequest",
      requestId: 2,
      name: "beginOperation",
      args: [second, ""],
    });
    await settle();
    provider.dispose();

    worker.emit({
      kind: "callbackRequest",
      requestId: 3,
      name: "endOperation",
      args: [first, 1],
    });
    await settle();
    await new Promise((resolve) => setTimeout(resolve, 10));
    expect(worker.terminated).toBe(false);

    worker.emit({
      kind: "callbackRequest",
      requestId: 4,
      name: "endOperation",
      args: [second, 1],
    });
    await settle();
    await new Promise((resolve) => setTimeout(resolve, 10));
    expect(worker.terminated).toBe(true);
  });

  it("routes payload-carrying subscriptions by name", async () => {
    const worker = new FakeWorker();
    const keys: Uint8Array[] = [];
    const providerPromise = createProviderFromRuntime(
      asWorker(worker),
      makeHostCallbacks({
        preimage: {
          lookupPreimage: async function* (key) {
            keys.push(key);
            yield ok(new Uint8Array([1]));
          },
        },
      }),
      { runtimeConfig: runtimeConfig() },
    );
    worker.emit({ kind: "loaded" });
    worker.emit({ kind: "ready" });
    const provider = await finishProviderReady(worker, providerPromise);

    worker.emit({
      kind: "subscriptionStart",
      subId: 4,
      name: "lookupPreimage",
      payload: new Uint8Array([9, 9]),
    });

    await settle();
    expect(keys).toEqual([new Uint8Array([9, 9])]);
    expect(worker.messages.at(-1)).toEqual({
      kind: "subscriptionItem",
      subId: 4,
      value: new Uint8Array([1]),
    });

    provider.dispose();
  });

  it("propagates host subscription stream errors to the worker", async () => {
    const worker = new FakeWorker();
    const providerPromise = createProviderFromRuntime(
      asWorker(worker),
      makeHostCallbacks({
        theme: {
          subscribeTheme: async function* () {
            yield err<ThemeVariant, GenericError>({
              reason: "theme stream failed",
            });
          },
        },
      }),
      { runtimeConfig: runtimeConfig() },
    );
    worker.emit({ kind: "loaded" });
    worker.emit({ kind: "ready" });
    const provider = await finishProviderReady(worker, providerPromise);

    worker.emit({
      kind: "subscriptionStart",
      subId: 7,
      name: "subscribeTheme",
      payload: null,
    });

    await settle();

    expect(worker.messages.at(-1)).toEqual({
      kind: "subscriptionError",
      subId: 7,
      error: "theme stream failed",
    });

    provider.dispose();
  });

  it("never falls through unknown subscription names to another callback", async () => {
    const worker = new FakeWorker();
    let preimageStarts = 0;
    const providerPromise = createProviderFromRuntime(
      asWorker(worker),
      makeHostCallbacks({
        preimage: {
          lookupPreimage: (() => {
            preimageStarts += 1;
            return () => {};
          }) as unknown as PreimageHost["lookupPreimage"],
        },
      }),
      { runtimeConfig: runtimeConfig() },
    );
    worker.emit({ kind: "loaded" });
    worker.emit({ kind: "ready" });
    const provider = await finishProviderReady(worker, providerPromise);

    worker.emit({
      kind: "subscriptionStart",
      subId: 5,
      name: "someFutureSubscribe",
      payload: new Uint8Array([1, 2, 3]),
    });

    expect(preimageStarts).toBe(0);
    expect(worker.messages.some((m) => m.kind === "subscriptionItem")).toBe(
      false,
    );

    provider.dispose();
  });

  it("does not dispatch a payload-carrying subscription without payload", async () => {
    const worker = new FakeWorker();
    let preimageStarts = 0;
    const providerPromise = createProviderFromRuntime(
      asWorker(worker),
      makeHostCallbacks({
        preimage: {
          lookupPreimage: (() => {
            preimageStarts += 1;
            return () => {};
          }) as unknown as PreimageHost["lookupPreimage"],
        },
      }),
      { runtimeConfig: runtimeConfig() },
    );
    worker.emit({ kind: "loaded" });
    worker.emit({ kind: "ready" });
    const provider = await finishProviderReady(worker, providerPromise);

    worker.emit({
      kind: "subscriptionStart",
      subId: 6,
      name: "lookupPreimage",
      payload: null,
    });

    expect(preimageStarts).toBe(0);

    provider.dispose();
  });

  it("rejects when init times out", async () => {
    const worker = new FakeWorker();
    const providerPromise = createProviderFromRuntime(
      asWorker(worker),
      makeHostCallbacks(),
      {
        runtimeConfig: runtimeConfig(),
        initTimeoutMs: 20,
      },
    );
    worker.emit({ kind: "loaded" });
    await expect(providerPromise).rejects.toThrow(
      /worker init timed out after 20ms/,
    );
    expect(worker.terminated).toBe(true);
  });

  it("rejects on messageerror during init", async () => {
    const worker = new FakeWorker();
    const providerPromise = createProviderFromRuntime(
      asWorker(worker),
      makeHostCallbacks(),
      {
        runtimeConfig: runtimeConfig(),
      },
    );
    worker.emitMessageError();
    await expect(providerPromise).rejects.toThrow(/could not be deserialized/);
    expect(worker.terminated).toBe(true);
  });

  it("decodes raw v01 push notification payloads", async () => {
    let notification: HostPushNotificationRequest | undefined;
    const callbacks = createWasmRawCallbacks(
      makeHostCallbacks({
        notifications: {
          pushNotification: async (request) => {
            notification = request;
            return { id: 42 };
          },
        },
      }),
    );

    const encoded = await callbacks.pushNotification!(
      HostPushNotificationRequest.enc({
        text: "Hello!",
        deeplink: undefined,
        scheduledAt: undefined,
      }),
    );

    expect(HostPushNotificationResponse.dec(encoded).id).toBe(42);
    expect(notification).toEqual({
      text: "Hello!",
      deeplink: undefined,
      scheduledAt: undefined,
    });
  });
  it("ends a render whose tree cannot be decoded instead of stranding it", async () => {
    const worker = new FakeWorker();
    const provider = await readyProvider(worker);

    const errors: Error[] = [];
    let completed = 0;
    provider.render!(renderRequest(), {
      onUpdate: () => {},
      onComplete: () => completed++,
      onError: (error) => errors.push(error),
    });
    const { renderId } = lastMessageOfKind(worker, "renderStart");

    // 0xff is not a RendererNode discriminant.
    expect(() =>
      worker.emit({
        kind: "renderItem",
        renderId,
        node: new Uint8Array([0xff]),
      }),
    ).not.toThrow();

    expect(errors).toHaveLength(1);
    expect(completed).toBe(0);
    // The worker must be told to stop, or its wasm subscription leaks.
    expect(lastMessageOfKind(worker, "renderStop").renderId).toBe(renderId);
  });

  it("fails a render the codec rejects without registering it", async () => {
    const worker = new FakeWorker();
    const provider = await readyProvider(worker);

    const errors: Error[] = [];
    const stop = provider.render!(
      // Odd-length hex: a `HexString` the codec cannot turn into bytes.
      {
        context: { tag: "PocketCard", value: { cardId: "card" } },
        payload: "0xabc",
      },
      { onUpdate: () => {}, onError: (error) => errors.push(error) },
    );

    expect(errors).toHaveLength(1);
    // Nothing was registered, so nothing is left for the worker to stop.
    expect(indexOfKind(worker, "renderStart")).toBe(-1);
    stop();
    expect(indexOfKind(worker, "renderStop")).toBe(-1);
  });

  it("settles a render exactly once when postMessage throws for renderStart", async () => {
    const worker = new FakeWorker();
    const provider = await readyProvider(worker);

    const post = worker.postMessage.bind(worker);
    worker.postMessage = (message: WorkerMessage) => {
      if (message.kind === "renderStart") {
        throw new Error("worker gone");
      }
      post(message);
    };

    const errors: Error[] = [];
    const stop = provider.render!(renderRequest(), {
      onUpdate: () => {},
      onError: (error) => errors.push(error),
    });

    expect(errors).toHaveLength(1);
    expect(errors[0].message).toMatch(/worker gone/);
    // The post never reached the worker, so there is nothing to stop.
    expect(indexOfKind(worker, "renderStart")).toBe(-1);
    expect(indexOfKind(worker, "renderStop")).toBe(-1);

    // The returned disposer is a safe no-op: the ledger entry is already gone.
    expect(() => stop()).not.toThrow();
    expect(indexOfKind(worker, "renderStop")).toBe(-1);
    expect(errors).toHaveLength(1);

    // A later core disposal must not settle the already-settled sink again.
    provider.dispose();
    expect(errors).toHaveLength(1);
  });

  it("fails every open render of a core the worker reported a frame error for", async () => {
    const worker = new FakeWorker();
    const provider = await readyProvider(worker);

    const errors: Error[] = [];
    provider.render!(renderRequest(), {
      onUpdate: () => {},
      onError: (error) => errors.push(error),
    });
    const { coreId } = lastMessageOfKind(worker, "renderStart");

    worker.emit({ kind: "frameError", coreId, error: "bad frame" });

    // The worker cancels the subscription with the core, so the sink's only
    // terminal is this one.
    expect(errors).toHaveLength(1);
    expect(errors[0].message).toMatch(/worker frame error: bad frame/);

    provider.dispose();
    expect(errors).toHaveLength(1);
  });

  it("keeps a throwing render sink from breaking the worker listener", async () => {
    const worker = new FakeWorker();
    const provider = await readyProvider(worker);

    provider.render!(renderRequest(), {
      onUpdate: () => {
        throw new Error("renderer exploded");
      },
      onError: () => {
        throw new Error("and so did onError");
      },
    });
    const { renderId } = lastMessageOfKind(worker, "renderStart");

    expect(() =>
      worker.emit({
        kind: "renderItem",
        renderId,
        node: RendererNode.enc({ tag: "Nil", value: undefined }),
      }),
    ).not.toThrow();

    // Still live: a later frame must still reach the provider.
    expect(() =>
      worker.emit({ kind: "frame", coreId: 0, bytes: new Uint8Array([1]) }),
    ).not.toThrow();
  });

  it("leaves the render's worker reference to the core", async () => {
    const worker = new FakeWorker();
    const runtime = await readyRuntime(worker);
    const provider = await finishProviderReady(
      worker,
      runtime.createProvider({ productId: "dotli.dot" }),
    );
    const seen: WorkerDemandChange[] = [];
    runtime.subscribeWorkerDemand((change) => seen.push(change));

    const stop = provider.render!(renderRequest(), { onUpdate: () => {} });

    // The core holds the reference, so this thread asks for none of its own.
    expect(indexOfKind(worker, "acquireWorker")).toBe(-1);
    // The demand the core's own reference caused still reaches the host.
    worker.emit({
      kind: "workerDemandChanged",
      productId: "dotli.dot",
      wanted: true,
    });
    expect(seen).toEqual([{ productId: "dotli.dot", wanted: true }]);

    stop();
    expect(indexOfKind(worker, "releaseWorker")).toBe(-1);
    worker.emit({
      kind: "workerDemandChanged",
      productId: "dotli.dot",
      wanted: false,
    });
    expect(seen).toEqual([
      { productId: "dotli.dot", wanted: true },
      { productId: "dotli.dot", wanted: false },
    ]);
    runtime.dispose();
  });

  it("binds a method-valued onUpdate to its own receiver", async () => {
    const worker = new FakeWorker();
    const provider = await readyProvider(worker);

    class RecordingSink {
      nodes: RendererNode[] = [];
      onUpdate(node: RendererNode) {
        this.nodes.push(node);
      }
    }
    const sink = new RecordingSink();

    provider.render!(renderRequest(), sink);
    const { renderId } = lastMessageOfKind(worker, "renderStart");

    expect(() =>
      worker.emit({
        kind: "renderItem",
        renderId,
        node: RendererNode.enc({ tag: "Nil", value: undefined }),
      }),
    ).not.toThrow();

    expect(sink.nodes).toHaveLength(1);
    expect(sink.nodes[0]).toEqual({ tag: "Nil", value: undefined });
  });

  it("publishes a renderer action and settles on the worker's response", async () => {
    const worker = new FakeWorker();
    const provider = await readyProvider(worker);

    const published = provider.publishRendererAction!({
      context: { tag: "PocketCard", value: { cardId: "card" } },
      actionId: "confirm",
      payload: "0x",
    });
    const { requestId } = lastMessageOfKind(worker, "publishRendererAction");
    worker.emit({ kind: "publishRendererActionResponse", requestId, ok: true });

    await expect(published).resolves.toBeUndefined();
  });

  it("publishes a chat action and settles on the worker's response", async () => {
    const worker = new FakeWorker();
    const provider = await readyProvider(worker);

    const published = provider.publishChatAction!({
      roomId: "room",
      peer: "peer",
      payload: {
        tag: "ActionTriggered",
        value: { messageId: "message", actionId: "confirm" },
      },
    });
    const { requestId } = lastMessageOfKind(worker, "publishChatAction");
    worker.emit({ kind: "publishChatActionResponse", requestId, ok: true });

    await expect(published).resolves.toBeUndefined();
  });

  it("rejects a chat action the worker could not publish", async () => {
    const worker = new FakeWorker();
    const provider = await readyProvider(worker);

    const published = provider.publishChatAction!({
      roomId: "room",
      peer: "peer",
      payload: {
        tag: "ActionTriggered",
        value: { messageId: "message", actionId: "confirm" },
      },
    });
    const { requestId } = lastMessageOfKind(worker, "publishChatAction");
    worker.emit({
      kind: "publishChatActionResponse",
      requestId,
      ok: false,
      error: "Denied",
    });

    await expect(published).rejects.toThrow("Denied");
  });
});

describe("worker demand", () => {
  it("forwards references and fans the wanted level out", async () => {
    const worker = new FakeWorker();
    const runtime = await readyRuntime(worker);
    const seen: WorkerDemandChange[] = [];

    runtime.acquireWorker("a.dot");
    expect(worker.messages.at(-1)).toEqual({
      kind: "acquireWorker",
      productId: "a.dot",
    });
    worker.emit({
      kind: "workerDemandChanged",
      productId: "a.dot",
      wanted: true,
    });

    // A late subscriber first learns what is wanted right now.
    const unsubscribe = runtime.subscribeWorkerDemand((change) =>
      seen.push(change),
    );
    expect(seen).toEqual([{ productId: "a.dot", wanted: true }]);

    runtime.acquireWorker("b.dot");
    worker.emit({
      kind: "workerDemandChanged",
      productId: "b.dot",
      wanted: true,
    });

    runtime.releaseWorker("a.dot");
    expect(worker.messages.at(-1)).toEqual({
      kind: "releaseWorker",
      productId: "a.dot",
    });
    worker.emit({
      kind: "workerDemandChanged",
      productId: "a.dot",
      wanted: false,
    });
    expect(seen).toEqual([
      { productId: "a.dot", wanted: true },
      { productId: "b.dot", wanted: true },
      { productId: "a.dot", wanted: false },
    ]);

    unsubscribe();
    worker.emit({
      kind: "workerDemandChanged",
      productId: "b.dot",
      wanted: false,
    });
    expect(seen).toHaveLength(3);
    runtime.dispose();
  });

  it("reports every wanted worker as unwanted when the runtime goes away", async () => {
    const worker = new FakeWorker();
    const runtime = await readyRuntime(worker);
    const seen: WorkerDemandChange[] = [];
    runtime.subscribeWorkerDemand((change) => seen.push(change));
    worker.emit({
      kind: "workerDemandChanged",
      productId: "a.dot",
      wanted: true,
    });

    runtime.dispose();
    expect(seen).toEqual([
      { productId: "a.dot", wanted: true },
      { productId: "a.dot", wanted: false },
    ]);

    // Nothing is posted to a disposed worker, and a late subscriber sees nothing.
    const before = worker.messages.length;
    runtime.acquireWorker("a.dot");
    expect(worker.messages.length).toBe(before);
    const late: WorkerDemandChange[] = [];
    runtime.subscribeWorkerDemand((change) => late.push(change));
    expect(late).toEqual([]);
  });

  it("ignores a level that was already in flight when the runtime went away", async () => {
    const worker = new FakeWorker();
    const runtime = await readyRuntime(worker);
    runtime.dispose();

    // The worker posted this before it saw the dispose, so it lands afterwards.
    worker.emit({
      kind: "workerDemandChanged",
      productId: "a.dot",
      wanted: true,
    });

    const seen: WorkerDemandChange[] = [];
    runtime.subscribeWorkerDemand((change) => seen.push(change));
    expect(seen).toEqual([]);
  });

  it("delivers one change once to a listener that subscribes from a listener", async () => {
    const worker = new FakeWorker();
    const runtime = await readyRuntime(worker);
    const nested: WorkerDemandChange[] = [];
    runtime.subscribeWorkerDemand(() => {
      if (nested.length === 0) {
        runtime.subscribeWorkerDemand((change) => nested.push(change));
      }
    });

    worker.emit({
      kind: "workerDemandChanged",
      productId: "a.dot",
      wanted: true,
    });

    // The replay tells it the product is wanted; the delivery still running
    // must not tell it again.
    expect(nested).toEqual([{ productId: "a.dot", wanted: true }]);
    runtime.dispose();
  });

  it("keeps delivering after a listener throws, in replay and in updates", async () => {
    const worker = new FakeWorker();
    const runtime = await readyRuntime(worker);
    worker.emit({
      kind: "workerDemandChanged",
      productId: "a.dot",
      wanted: true,
    });

    // A throw during the initial replay must still return the unsubscribe,
    // otherwise the listener is registered with no way to remove it.
    const thrower = (): never => {
      throw new Error("listener exploded");
    };
    const unsubscribe = runtime.subscribeWorkerDemand(thrower);
    expect(typeof unsubscribe).toBe("function");

    const seen: WorkerDemandChange[] = [];
    runtime.subscribeWorkerDemand((change) => seen.push(change));
    worker.emit({
      kind: "workerDemandChanged",
      productId: "b.dot",
      wanted: true,
    });
    expect(seen).toEqual([
      { productId: "a.dot", wanted: true },
      { productId: "b.dot", wanted: true },
    ]);

    unsubscribe();
    runtime.dispose();
  });
});

// The worker gained an optional host role so a test host can run a signing
// host in it. The property that makes that safe is that it is additive: a host
// that does not ask for a role must put exactly the same thing on the wire as
// it did before, and the worker must read an absent role as "pairing". These
// assert that rather than trusting it.
describe("worker host role", () => {
  it("omits the role for a host that does not ask for one", async () => {
    const worker = new FakeWorker();
    const config = runtimeConfig();
    const providerPromise = createProviderFromRuntime(
      asWorker(worker),
      makeHostCallbacks(),
      { logLevel: "debug", runtimeConfig: config },
    );

    worker.emit({ kind: "loaded" });
    const init = worker.messages[0] as { kind: string; role?: unknown };
    expect(init.kind).toBe("init");
    // Not merely "not signing": absent, so the message a pairing host sends is
    // byte-for-byte what it sent before the field existed.
    expect(init.role).toBeUndefined();
    expect(Object.hasOwn(init, "role") ? init.role : undefined).toBeUndefined();

    worker.emit({ kind: "ready" });
    await settle();
    await finishProviderReady(worker, providerPromise).catch(() => {});
  });

  it("carries the role through when a host asks for a signing host", async () => {
    const worker = new FakeWorker();
    const config = runtimeConfig();
    const providerPromise = createProviderFromRuntime(
      asWorker(worker),
      makeHostCallbacks(),
      {
        logLevel: "debug",
        runtimeConfig: config,
        createWebWorkerPairingHostRuntime: (w, h, o) =>
          createWebWorkerPairingHostRuntime(w, h, { ...o, role: "signing" }),
      },
    );

    worker.emit({ kind: "loaded" });
    expect((worker.messages[0] as { role?: unknown }).role).toBe("signing");

    worker.emit({ kind: "ready" });
    await settle();
    await finishProviderReady(worker, providerPromise).catch(() => {});
  });
});
