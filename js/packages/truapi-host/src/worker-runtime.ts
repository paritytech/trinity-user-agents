/// <reference lib="webworker" />
// Worker entrypoint. Loads the web-targeted truapi WASM bundle and
// bridges every host callback over postMessage. The main thread keeps the
// state that needs DOM access (localStorage, prompts) while the core dispatcher
// runs here off the page main thread.
//
// Importing this module starts the worker: it loads the WASM glue and posts
// `loaded` once. Test files share one module registry under `bun test`, so only
// `worker-signing-host.test.ts` imports it; anything testable on its own lives
// in a side-effect-free module such as `worker-debugger-link.ts`.

import type { ProductId } from "@parity/truapi";
import type {
  MainToWorker,
  SubscriptionName,
  WorkerToMain,
} from "./worker-protocol.js";
import type { GenericError } from "@parity/truapi";
import {
  coreWireSchemaHash,
  createDebuggerLink,
} from "./worker-debugger-link.js";
import {
  createWorkerRawCallbacks,
  type CallbackName,
  type OptionalCapabilities,
} from "./generated/worker-callbacks.js";
import {
  handleGetPermissionAuthorizationStatus,
  handleGetPermissionAuthorizationStatuses,
  handleSetPermissionAuthorizationStatus,
} from "./worker-permission-authorization.js";
import type {
  WasmModuleShape,
  WorkerPairingHostRuntime,
  WorkerSigningHostRuntime,
  WorkerProductRuntime,
  WorkerTransition,
} from "./wasm-module.js";
import { errorMessage } from "./error.js";
import {
  CHAT_ACTION_ENTRY_POINT,
  RENDERER_ACTION_ENTRY_POINT,
  handlePublishAction,
} from "./worker-actions.js";
import {
  handleRenderStart,
  stopRender,
  stopRendersForCore,
  type RenderSubscriptions,
} from "./worker-renderer.js";
import {
  dispatchChainResponse,
  dispatchSubscriptionError,
  dispatchSubscriptionItem,
  type SubscriptionListeners,
} from "./worker-dispatch.js";
import {
  dispatchFrame,
  disposeAwaitingFrames,
} from "./worker-core-registry.js";

// A literal specifier so bundlers resolve the glue statically and emit it
// alongside `truapi_server_bg.wasm`. It is typed by the ambient declaration in
// `src/wasm/web/`, and resolves against `dist/worker-runtime.js` at runtime,
// where `make wasm` puts the artifact.
const wasmModulePromise: Promise<WasmModuleShape> =
  import("./wasm/web/truapi_server.js");

const ctx = self as unknown as DedicatedWorkerGlobalScope;

function postToMain(msg: WorkerToMain): void {
  ctx.postMessage(msg);
}

let nextRequestId = 0;
const pendingCallbacks = new Map<
  number,
  (result: { ok: true; value: unknown } | { ok: false; error: string }) => void
>();

let nextSubId = 0;
const subscriptionListeners = new Map<number, SubscriptionListeners>();

let nextConnId = 0;
type ChainConnectAck = { ok: true } | { ok: false; error: string };
const chainConnectAcks = new Map<number, (ack: ChainConnectAck) => void>();
const chainResponseListeners = new Map<number, (json: string) => void>();

function callbackRequest(
  name: CallbackName,
  args: readonly unknown[],
): Promise<unknown> {
  return new Promise((resolve, reject) => {
    const requestId = ++nextRequestId;
    pendingCallbacks.set(requestId, (r) => {
      if (r.ok) resolve(r.value);
      else reject(new Error(r.error));
    });
    postToMain({ kind: "callbackRequest", requestId, name, args });
  });
}

function startSubscription<T>(
  name: SubscriptionName,
  payload: Uint8Array | string | null,
  sendItem: (value: T) => void,
  sendError: (error: GenericError) => void,
): () => void {
  const subId = ++nextSubId;
  subscriptionListeners.set(subId, {
    sendItem: sendItem as (value: unknown) => void,
    sendError: (error) => sendError({ reason: error }),
  });
  postToMain({ kind: "subscriptionStart", subId, name, payload });
  return () => {
    subscriptionListeners.delete(subId);
    postToMain({ kind: "subscriptionStop", subId });
  };
}

interface WorkerChainConnection {
  send(request: string): void;
  close(): void;
}

/**
 * Worker-side half of the host chain-connect bridge.
 *
 * The Rust core runs in this worker but owns no socket. When it needs chain
 * access (chainHead v1 for dotNS identity on Asset Hub / statement-store SSO) it
 * calls this; the actual transport lives on the host main thread and is reached
 * over postMessage. The data crossing here is JSON-RPC strings, not SCALE: only
 * the product<->core wire is SCALE.
 *
 *   per-tab / sandboxed          core-owned (this Web Worker)       host-owned (main thread)
 *   +-------------------+  SCALE  +--------------------------+      +--------------------------------+
 *   | Product (iframe)  |<------->| truapi WASM core         |      | host.connect() (ChainProvider) |
 *   | speaks TrUAPI     |  frames | chainHead v1, SSO,       |      | host-owned JSON-RPC transport  |
 *   | never sees chains |         | dotNS identity (AH)      |      | remote RPC, native client, ... |
 *   +-------------------+         +--------------------------+      +--------------------------------+
 *                                      |   ^  JSON-RPC strings (not SCALE)        ^   |
 *                       chainConnect() |   | onResponse(json)           connect   |   | responses()
 *                         (this fn)    v   |                                      |   v
 *                 worker-runtime.ts  <======== postMessage ========>  create-worker-host-runtime.ts
 *                 chainConnectStart / chainSend / chainClose   -->   handleChainConnect* -> host.connect()
 *                 chainConnectAck   / chainResponse            <--   (pumped from connection.responses())
 *
 * Allocates a `connId`, posts `chainConnectStart`, and resolves a
 * `{ send, close }` handle once the main thread acks. `send` posts `chainSend`,
 * `close` posts `chainClose`, and every `chainResponse` for this `connId` is
 * delivered to `onResponse`.
 */
function chainConnect(
  genesisHash: string,
  onResponse: (json: string) => void,
): Promise<WorkerChainConnection | null> {
  const connId = ++nextConnId;
  return new Promise((resolve, reject) => {
    chainConnectAcks.set(connId, (ack) => {
      if (!ack.ok) {
        chainResponseListeners.delete(connId);
        reject(new Error(ack.error));
        return;
      }
      resolve({
        send(request: string) {
          postToMain({ kind: "chainSend", connId, request });
        },
        close() {
          chainResponseListeners.delete(connId);
          postToMain({ kind: "chainClose", connId });
        },
      });
    });
    chainResponseListeners.set(connId, onResponse);
    postToMain({ kind: "chainConnectStart", connId, genesisHash });
  });
}

/** Build the host-level callback object passed to the WASM runtime. */
function buildRawCallbacks(capabilities: OptionalCapabilities) {
  return {
    ...createWorkerRawCallbacks(
      {
        callbackRequest,
        startSubscription,
        chainConnect,
      },
      capabilities,
    ),
    /**
     * Demand on a product's worker crossed zero. Every transition arrives
     * here in ledger order, whether this thread asked for it through
     * `acquireWorker`/`releaseWorker` or the core took the reference itself
     * for an open render.
     */
    requestWorkerTransition(
      productId: ProductId,
      transition: WorkerTransition,
    ): void {
      postToMain({
        kind: "workerDemandChanged",
        productId,
        wanted: transition === "Start",
      });
    },
  };
}

let debuggerLink: ReturnType<typeof createDebuggerLink> | null = null;

function buildCoreCallbacks(coreId: number) {
  const callbacks = {
    emitFrame(frame: Uint8Array): void {
      postToMain({ kind: "frame", coreId, bytes: frame });
    },
    dispose(): void {
      // Main thread owns lifecycle and disposes explicitly.
    },
  };
  if (!debuggerLink) return callbacks;
  // Adding `debugEmit` is what makes the Rust host install its debug sink. The
  // link is created once, from `init`, and never replaced, so a core either has
  // a tap for its whole life or never has one - there is no window in which this
  // decision and the link disagree.
  return {
    ...callbacks,
    debugEmit(channelId: string, dir: string, frame: Uint8Array): void {
      debuggerLink?.emit(channelId, dir, frame);
    },
  };
}

/** The runtime `init` built, tagged with the role it was built for. */
type HostRuntime =
  | { role: "pairing"; runtime: WorkerPairingHostRuntime }
  | { role: "signing"; runtime: WorkerSigningHostRuntime };

let host: HostRuntime | null = null;
const cores = new Map<number, WorkerProductRuntime>();
// Outstanding receiveFrame calls per core. wasm-bindgen holds a borrow of the
// core for the whole duration of an async method, so `free()` throws while one
// is in flight. `disposeCore` aborts these then awaits them before freeing.
const inFlightFrames = new Map<number, Set<Promise<void>>>();
/** Live render subscriptions, keyed by main-thread render id. */
const renders: RenderSubscriptions = new Map();
let wasm: WasmModuleShape | null = null;

(async () => {
  try {
    wasm = await wasmModulePromise;
    await wasm.default();
    postToMain({ kind: "loaded" });
  } catch (err) {
    postToMain({ kind: "fatalError", error: errorMessage(err) });
  }
})();

ctx.addEventListener("message", (ev: MessageEvent<MainToWorker>) => {
  const msg = ev.data;
  switch (msg.kind) {
    case "init":
      if (!wasm) {
        postToMain({
          kind: "fatalError",
          error: "init received before WASM loaded",
        });
        break;
      }
      if (host) {
        postToMain({
          kind: "fatalError",
          error: "init: runtime already initialized",
        });
        break;
      }
      wasm.setLogLevel?.(msg.logLevel);
      if (msg.debuggerUrl && !debuggerLink) {
        // The hash comes from the core that will encode the frames, not from this
        // package's client constant: they are separate artifacts and the WASM
        // bundle is built by hand.
        debuggerLink = createDebuggerLink(msg.debuggerUrl, {
          schema: coreWireSchemaHash(wasm),
        });
      }
      try {
        if (msg.role === "signing") {
          // Only the `testing` bundle carries a signing host; the production
          // `web` one is built without it, so say that rather than let an
          // undefined constructor surface as a generic type error.
          const SigningRuntime = wasm.WasmSigningHostRuntime;
          if (!SigningRuntime) {
            postToMain({
              kind: "fatalError",
              error:
                "init: this WASM bundle has no signing host. Use the " +
                "`testing` bundle, which is built with `wasm-signing-host`.",
            });
            break;
          }
          host = {
            role: "signing",
            runtime: new SigningRuntime(
              buildRawCallbacks(msg.capabilities),
              msg.hostConfig,
            ),
          };
        } else {
          host = {
            role: "pairing",
            runtime: new wasm.WasmPairingHostRuntime(
              buildRawCallbacks(msg.capabilities),
              msg.hostConfig,
            ),
          };
        }
        postToMain({ kind: "ready", schema: coreWireSchemaHash(wasm) });
      } catch (err) {
        postToMain({ kind: "fatalError", error: `init: ${errorMessage(err)}` });
      }
      break;
    case "createCore":
      if (!host) {
        postToMain({
          kind: "coreError",
          coreId: msg.coreId,
          error: "createCore received before runtime is ready",
        });
        break;
      }
      try {
        const core = host.runtime.productRuntime(
          msg.product,
          buildCoreCallbacks(msg.coreId),
        );
        cores.set(msg.coreId, core);
        postToMain({ kind: "coreReady", coreId: msg.coreId });
      } catch (err) {
        postToMain({
          kind: "coreError",
          coreId: msg.coreId,
          error: errorMessage(err),
        });
      }
      break;
    case "setLogLevel":
      wasm?.setLogLevel?.(msg.level);
      break;
    case "frame":
      void handleFrame(msg.coreId, msg.bytes);
      break;
    case "disconnectSession":
      void handleDisconnectSession(msg.requestId);
      break;
    case "cancelPairing":
      // A signing host has no pairing to cancel.
      if (host?.role === "pairing") host.runtime.cancelPairing();
      break;
    case "getSessionChatIdentityKey":
      handleGetSessionChatIdentityKey(msg.requestId);
      break;
    case "getDeviceStatementKey":
      handleGetDeviceStatementKey(msg.requestId);
      break;
    case "getDeviceEncryptionKey":
      void handleGetDeviceEncryptionKey(msg.requestId);
      break;
    case "parseProductId":
      try {
        if (!wasm) throw new Error("runtime is not ready");
        postToMain({
          kind: "parseProductIdResponse",
          requestId: msg.requestId,
          ok: true,
          value: wasm.parseProductId(msg.value),
        });
      } catch (error) {
        postToMain({
          kind: "parseProductIdResponse",
          requestId: msg.requestId,
          ok: false,
          error: errorMessage(error),
        });
      }
      break;
    case "getProductSubtreePublicKey":
      void handleGetProductSubtreePublicKey(
        msg.requestId,
        msg.productId,
        msg.timeoutMs,
      );
      break;
    case "notifySessionStoreChanged":
      // Only a pairing host restores its session from the store.
      if (host?.role === "pairing") host.runtime.notifySessionStoreChanged();
      break;
    case "notifyContactsChanged":
      host?.runtime.notifyContactsChanged();
      break;
    case "acquireWorker":
      host?.runtime.acquireWorker(msg.productId);
      break;
    case "releaseWorker":
      host?.runtime.releaseWorker(msg.productId);
      break;
    case "activateStoredSession":
      void handleSessionActivation(
        msg.requestId,
        "activateStoredSession",
        (booted) =>
          pairingRuntime(
            booted,
            "activateStoredSession",
          ).activateStoredSession(),
      );
      break;
    case "activateExternalSession": {
      const { blob } = msg;
      void handleSessionActivation(
        msg.requestId,
        "activateExternalSession",
        (booted) =>
          pairingRuntime(
            booted,
            "activateExternalSession",
          ).activateExternalSession(blob),
      );
      break;
    }
    case "activateLocalSession": {
      const { secret, liteUsername } = msg;
      void handleSessionActivation(
        msg.requestId,
        "activateLocalSession",
        (booted) => {
          const signing = signingRuntime(booted, "activateLocalSession");
          // Activating with a name is a separate core entry point. Fall back
          // when the name is absent, or when a core predating it is loaded.
          if (
            liteUsername !== undefined &&
            typeof signing.activateLocalSessionWithIdentity === "function"
          ) {
            return signing.activateLocalSessionWithIdentity(
              secret,
              liteUsername,
            );
          }
          return signing.activateLocalSession(secret);
        },
      );
      break;
    }
    case "setGrantAllowancesUnchecked": {
      const { granted } = msg;
      void handleSessionActivation(
        msg.requestId,
        "setGrantAllowancesUnchecked",
        (booted) => {
          const signing = signingRuntime(booted, "setGrantAllowancesUnchecked");
          if (typeof signing.setGrantAllowancesUnchecked !== "function") {
            return Promise.reject(
              new Error(
                "setGrantAllowancesUnchecked needs a signing host built with " +
                  "`wasm-signing-host`; this core does not carry it",
              ),
            );
          }
          signing.setGrantAllowancesUnchecked(granted);
          return Promise.resolve();
        },
      );
      break;
    }
    case "setSubmitPreimagesLocally": {
      const { local } = msg;
      void handleSessionActivation(
        msg.requestId,
        "setSubmitPreimagesLocally",
        ({ runtime }) => {
          if (typeof runtime.setSubmitPreimagesLocally !== "function") {
            return Promise.reject(
              new Error(
                "setSubmitPreimagesLocally needs a core built with " +
                  "`test-host`; this core does not carry it",
              ),
            );
          }
          runtime.setSubmitPreimagesLocally(local);
          return Promise.resolve();
        },
      );
      break;
    }
    case "setWithheldResources": {
      const { tags } = msg;
      void handleSessionActivation(
        msg.requestId,
        "setWithheldResources",
        (booted) => {
          const signing = signingRuntime(booted, "setWithheldResources");
          if (typeof signing.setWithheldResources !== "function") {
            return Promise.reject(
              new Error(
                "setWithheldResources needs a signing host built with " +
                  "`wasm-signing-host`; this core does not carry it",
              ),
            );
          }
          signing.setWithheldResources(tags);
          return Promise.resolve();
        },
      );
      break;
    }
    case "resetSessionState":
      void handleSessionActivation(
        msg.requestId,
        "resetSessionState",
        (booted) =>
          pairingRuntime(booted, "resetSessionState").resetSessionState(),
      );
      break;
    case "getPermissionAuthorizationStatus":
      void handleGetPermissionAuthorizationStatus(
        host?.runtime ?? null,
        postToMain,
        msg.productId,
        msg.requestId,
        msg.request,
      );
      break;
    case "getPermissionAuthorizationStatuses":
      void handleGetPermissionAuthorizationStatuses(
        host?.runtime ?? null,
        postToMain,
        msg.productId,
        msg.requestId,
        msg.requests,
      );
      break;
    case "setPermissionAuthorizationStatus":
      void handleSetPermissionAuthorizationStatus(
        host?.runtime ?? null,
        postToMain,
        msg.productId,
        msg.requestId,
        msg.request,
        msg.status,
      );
      break;
    case "callbackResponse": {
      const cb = pendingCallbacks.get(msg.requestId);
      if (cb) {
        pendingCallbacks.delete(msg.requestId);
        cb(
          msg.ok
            ? { ok: true, value: msg.value }
            : { ok: false, error: msg.error },
        );
      }
      break;
    }
    case "subscriptionItem": {
      dispatchSubscriptionItem(
        msg.subId,
        msg.value,
        subscriptionListeners,
        postToMain,
      );
      break;
    }
    case "subscriptionError": {
      dispatchSubscriptionError(
        msg.subId,
        msg.error,
        subscriptionListeners,
        postToMain,
      );
      break;
    }
    case "chainConnectAck": {
      const cb = chainConnectAcks.get(msg.connId);
      if (cb) {
        chainConnectAcks.delete(msg.connId);
        cb(msg.ok ? { ok: true } : { ok: false, error: msg.error });
      }
      break;
    }
    case "chainResponse": {
      dispatchChainResponse(
        msg.connId,
        msg.json,
        chainResponseListeners,
        postToMain,
      );
      break;
    }
    case "publishChatAction":
      handlePublishAction(
        CHAT_ACTION_ENTRY_POINT,
        cores.get(msg.coreId),
        postToMain,
        msg.coreId,
        msg.requestId,
        msg.action,
      );
      break;
    case "publishRendererAction":
      handlePublishAction(
        RENDERER_ACTION_ENTRY_POINT,
        cores.get(msg.coreId),
        postToMain,
        msg.coreId,
        msg.requestId,
        msg.action,
      );
      break;
    case "renderStart":
      handleRenderStart(
        cores.get(msg.coreId),
        postToMain,
        renders,
        msg.coreId,
        msg.renderId,
        msg.request,
      );
      break;
    case "renderStop":
      stopRender(renders, msg.renderId);
      break;
    case "disposeCore":
      void disposeCore(msg.coreId);
      break;
    case "dispose": {
      // Null the runtime synchronously so a message arriving mid-disposal takes
      // its `if (!host)` path instead of calling into a runtime being torn
      // down; free the captured handle after the cores finish disposing.
      const disposing = host?.runtime;
      host = null;
      void (async () => {
        try {
          await Promise.all(
            [...cores.keys()].map((coreId) => disposeCore(coreId)),
          );
          disposing?.free();
        } catch (err) {
          postToMain({ kind: "disposeError", error: errorMessage(err) });
        }
      })();
      break;
    }
    default: {
      const { kind } = msg as { kind?: unknown };
      console.warn(
        `[truapi worker-runtime] unknown message kind: ${String(kind)}`,
      );
    }
  }
});

async function disposeCore(coreId: number): Promise<void> {
  const core = cores.get(coreId);
  if (!core) return;
  cores.delete(coreId);
  // A render subscription outliving its core would call into freed wasm.
  stopRendersForCore(renders, coreId);
  try {
    await disposeAwaitingFrames(core, coreId, inFlightFrames);
  } catch (err) {
    postToMain({ kind: "disposeError", error: errorMessage(err) });
  }
}

function pairingRuntime(
  booted: HostRuntime,
  label: string,
): WorkerPairingHostRuntime {
  if (booted.role === "signing") {
    throw new Error(
      `${label} needs a pairing host; this runtime is a signing host`,
    );
  }
  return booted.runtime;
}

function signingRuntime(
  booted: HostRuntime,
  label: string,
): WorkerSigningHostRuntime {
  if (booted.role === "pairing") {
    throw new Error(
      `${label} needs a signing host; this runtime is a pairing host ` +
        '(pass role: "signing" to init)',
    );
  }
  return booted.runtime;
}

async function handleSessionActivation(
  requestId: number,
  label: string,
  activate: (booted: HostRuntime) => Promise<void>,
): Promise<void> {
  if (!host) {
    postToMain({
      kind: "sessionActivationResponse",
      requestId,
      ok: false,
      error: `${label} received before runtime is ready`,
    });
    return;
  }
  try {
    await activate(host);
    postToMain({ kind: "sessionActivationResponse", requestId, ok: true });
  } catch (err) {
    postToMain({
      kind: "sessionActivationResponse",
      requestId,
      ok: false,
      error: errorMessage(err),
    });
  }
}

async function handleDisconnectSession(requestId: number): Promise<void> {
  if (!host) {
    postToMain({
      kind: "disconnectSessionResponse",
      requestId,
      ok: false,
      error: "disconnectSession received before runtime is ready",
    });
    return;
  }
  try {
    await host.runtime.disconnectSession();
    postToMain({ kind: "disconnectSessionResponse", requestId, ok: true });
  } catch (err) {
    postToMain({
      kind: "disconnectSessionResponse",
      requestId,
      ok: false,
      error: errorMessage(err),
    });
  }
}

function handleGetSessionChatIdentityKey(requestId: number): void {
  if (!host) {
    postToMain({
      kind: "sessionChatIdentityKeyResponse",
      requestId,
      ok: false,
      error: "getSessionChatIdentityKey received before runtime is ready",
    });
    return;
  }
  try {
    postToMain({
      kind: "sessionChatIdentityKeyResponse",
      requestId,
      ok: true,
      key: pairingRuntime(
        host,
        "getSessionChatIdentityKey",
      ).sessionChatIdentityKey(),
    });
  } catch (err) {
    postToMain({
      kind: "sessionChatIdentityKeyResponse",
      requestId,
      ok: false,
      error: errorMessage(err),
    });
  }
}

function handleGetDeviceStatementKey(requestId: number): void {
  if (!host) {
    postToMain({
      kind: "deviceStatementKeyResponse",
      requestId,
      ok: false,
      error: "getDeviceStatementKey received before runtime is ready",
    });
    return;
  }
  try {
    postToMain({
      kind: "deviceStatementKeyResponse",
      requestId,
      ok: true,
      key: pairingRuntime(host, "getDeviceStatementKey").deviceStatementKey(),
    });
  } catch (err) {
    postToMain({
      kind: "deviceStatementKeyResponse",
      requestId,
      ok: false,
      error: errorMessage(err),
    });
  }
}

async function handleGetProductSubtreePublicKey(
  requestId: number,
  productId: ProductId,
  timeoutMs: number | undefined,
): Promise<void> {
  if (!host) {
    postToMain({
      kind: "productSubtreePublicKeyResponse",
      requestId,
      ok: false,
      error: "getProductSubtreePublicKey received before runtime is ready",
    });
    return;
  }
  try {
    postToMain({
      kind: "productSubtreePublicKeyResponse",
      requestId,
      ok: true,
      key: await host.runtime.productSubtreePublicKey(productId, timeoutMs),
    });
  } catch (err) {
    postToMain({
      kind: "productSubtreePublicKeyResponse",
      requestId,
      ok: false,
      error: errorMessage(err),
    });
  }
}

async function handleGetDeviceEncryptionKey(requestId: number): Promise<void> {
  if (!host) {
    postToMain({
      kind: "deviceEncryptionKeyResponse",
      requestId,
      ok: false,
      error: "getDeviceEncryptionKey received before runtime is ready",
    });
    return;
  }
  try {
    postToMain({
      kind: "deviceEncryptionKeyResponse",
      requestId,
      ok: true,
      key: await pairingRuntime(
        host,
        "getDeviceEncryptionKey",
      ).deviceEncryptionKey(),
    });
  } catch (err) {
    postToMain({
      kind: "deviceEncryptionKeyResponse",
      requestId,
      ok: false,
      error: errorMessage(err),
    });
  }
}

async function handleFrame(coreId: number, bytes: Uint8Array): Promise<void> {
  const core = cores.get(coreId);
  if (!core) {
    postToMain({
      kind: "frameError",
      coreId,
      error: `frame received for unknown core ${coreId}`,
    });
    return;
  }
  try {
    await dispatchFrame(core, coreId, bytes, inFlightFrames);
  } catch (err) {
    postToMain({
      kind: "frameError",
      coreId,
      error: errorMessage(err),
    });
  }
}
