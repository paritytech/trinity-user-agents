// Wire format between the main thread (`createWebWorkerPairingHostRuntime`) and the
// Web Worker that hosts the truapi WASM runtime.
//
//   Main window / host JS
//   ┌─────────────────────────────────────────────────────────────────┐
//   │ createWebWorkerPairingHostRuntime                               │
//   │ host callbacks: storage, DOM prompts, chain provider, logging   │
//   └───────────────┬─────────────────────────────────────────────────┘
//                   │ MainToWorker: init, createCore, frame,
//                   │               callbackResponse, subscriptionItem,
//                   │               chainResponse
//                   v
//   Dedicated Worker
//   ┌─────────────────────────────────────────────────────────────────┐
//   │ shared truapi WASM PairingHostRuntime + product runtimes        │
//   │ generated raw-callback proxy                                    │
//   └───────────────┬─────────────────────────────────────────────────┘
//                   │ WorkerToMain: coreReady, frame, callbackRequest,
//                   │               subscriptionStart, chainConnect
//                   v
//   Main window dispatches those requests to the actual host callbacks.
//
// Frames (`kind: 'frame'`) carry SCALE-encoded `ProtocolMessage` bytes
// untouched in either direction. Everything else is a control message
// for callback dispatch, subscription bookkeeping, or chain connections.
//
// Frame bytes cross the boundary by structured clone, deliberately not as
// transferables: the sender keeps using its buffer (the worker side posts
// views into WASM memory) and frames are small, so the copy is the simpler
// safe choice.

import type { OptionalCapabilities } from "./generated/worker-callbacks.js";
import type { LogLevel, PermissionAuthorizationStatus } from "./runtime.js";
import type {
  CallbackName,
  SubscriptionName,
} from "./generated/worker-callbacks.js";
/**
 * Generated callback-name unions used by the worker transport. They keep the
 * hand-written protocol aligned with the Rust platform callback catalog.
 */
export type {
  CallbackName,
  SubscriptionName,
} from "./generated/worker-callbacks.js";

/**
 * Positional arguments for a callback. The wasm core calls each callback
 * at a fixed arity; a uniform `unknown[]` keeps the wire protocol simple.
 */
export type CallbackArgs = readonly unknown[];

/**
 * Messages posted by the main window to the WASM worker. These either control
 * worker/core lifecycle, forward encoded TrUAPI frames into the core, or return
 * host callback/subscription/chain responses requested by the worker.
 */
/** Which host role a worker runtime plays. */
export type HostRole = "pairing" | "signing";

export type MainToWorker =
  | {
      kind: "init";
      logLevel: LogLevel;
      hostConfig: unknown;
      /**
       * Optional capabilities the main-thread host serves. The worker proxies
       * only these, so the core sees the same capability set on both sides of
       * the boundary.
       */
      capabilities: OptionalCapabilities;
      // Dev-only: when set, the worker dials this debugger and streams tapped
      // frames to it. Null in production, so the host tap stays inert.
      debuggerUrl: string | null;
      /**
       * Which host role the worker constructs. Omitted means `"pairing"`, so a
       * host written before this existed behaves exactly as it did.
       *
       * `"signing"` needs the `testing` WASM bundle, which is the only one
       * built with a signing host in it.
       */
      role?: HostRole;
    }
  | { kind: "createCore"; coreId: number; product: unknown }
  | { kind: "disposeCore"; coreId: number }
  | { kind: "setLogLevel"; level: LogLevel }
  | { kind: "frame"; coreId: number; bytes: Uint8Array }
  | { kind: "disconnectSession"; requestId: number }
  | { kind: "cancelPairing" }
  | { kind: "notifySessionStoreChanged" }
  | { kind: "notifyContactsChanged" }
  | { kind: "acquireWorker"; productId: string }
  | { kind: "releaseWorker"; productId: string }
  | { kind: "activateStoredSession"; requestId: number }
  | { kind: "activateExternalSession"; requestId: number; blob: Uint8Array }
  /**
   * Establish a session from host-held entropy. Signing hosts only: a pairing
   * host has no local secret and answers this with an error.
   */
  | {
      kind: "activateLocalSession";
      requestId: number;
      secret: Uint8Array;
      /**
       * Display name to activate the session under. Absent activates without
       * one, which leaves the session with no primary username -- and
       * `account.get_user_id` answers `Unknown` rather than a name.
       */
      liteUsername?: string;
    }
  | {
      kind: "setGrantAllowancesUnchecked";
      requestId: number;
      /** Answer allocation as granted without performing it. */
      granted: boolean;
    }
  | {
      kind: "setSubmitPreimagesLocally";
      requestId: number;
      /** Keep preimage submissions in the core instead of the Bulletin chain. */
      local: boolean;
    }
  | {
      kind: "setWithheldResources";
      requestId: number;
      /** Resource tags answered as refused, replacing any earlier set. */
      tags: string[];
    }
  | { kind: "resetSessionState"; requestId: number }
  | {
      kind: "getPermissionAuthorizationStatus";
      productId: string;
      requestId: number;
      request: Uint8Array;
    }
  | {
      kind: "getPermissionAuthorizationStatuses";
      productId: string;
      requestId: number;
      requests: Uint8Array[];
    }
  | {
      kind: "setPermissionAuthorizationStatus";
      productId: string;
      requestId: number;
      request: Uint8Array;
      status: PermissionAuthorizationStatus;
    }
  | { kind: "getSessionChatIdentityKey"; requestId: number }
  | { kind: "getDeviceStatementKey"; requestId: number }
  | { kind: "getDeviceEncryptionKey"; requestId: number }
  | {
      kind: "getProductSubtreePublicKey";
      requestId: number;
      productId: string;
      timeoutMs: number | undefined;
    }
  | {
      kind: "publishChatAction";
      coreId: number;
      requestId: number;
      /** SCALE-encoded `HostChatActionSubscribeItem`. */
      action: Uint8Array;
    }
  | {
      kind: "publishRendererAction";
      coreId: number;
      requestId: number;
      /** SCALE-encoded `HostRendererActionSubscribeItem`. */
      action: Uint8Array;
    }
  | {
      kind: "renderStart";
      coreId: number;
      renderId: number;
      /** SCALE-encoded `ProductRendererRenderRequest`. */
      request: Uint8Array;
    }
  | { kind: "renderStop"; renderId: number }
  | { kind: "callbackResponse"; requestId: number; ok: true; value: unknown }
  | { kind: "callbackResponse"; requestId: number; ok: false; error: string }
  | { kind: "subscriptionItem"; subId: number; value: unknown }
  | { kind: "subscriptionError"; subId: number; error: string }
  | { kind: "chainConnectAck"; connId: number; ok: true }
  | { kind: "chainConnectAck"; connId: number; ok: false; error: string }
  | { kind: "chainResponse"; connId: number; json: string }
  | { kind: "dispose" };

/**
 * Messages posted by the WASM worker back to the main window. These either
 * report worker lifecycle/errors, emit encoded TrUAPI frames from the core, or
 * request host callbacks, subscriptions, and chain-provider operations.
 */
export type WorkerToMain =
  | { kind: "loaded" }
  | {
      kind: "ready";
      /**
       * The encoding core's wire-schema hash, when it reports one. The page needs
       * it to stamp an in-host debugger tap with the same identity a dialing host
       * puts on a standalone envelope; without it a tap is grouped but not decoded.
       */
      schema?: string;
    }
  | { kind: "coreReady"; coreId: number }
  | { kind: "coreError"; coreId: number; error: string }
  | { kind: "fatalError"; error: string }
  | { kind: "frameError"; coreId: number; error: string }
  | { kind: "disposeError"; error: string }
  | { kind: "frame"; coreId: number; bytes: Uint8Array }
  | { kind: "disconnectSessionResponse"; requestId: number; ok: true }
  | {
      kind: "disconnectSessionResponse";
      requestId: number;
      ok: false;
      error: string;
    }
  /**
   * Shared reply for `activateStoredSession`, `activateExternalSession` and
   * `resetSessionState`: all three settle as a bare success or a failure
   * reason.
   */
  | { kind: "sessionActivationResponse"; requestId: number; ok: true }
  | {
      kind: "sessionActivationResponse";
      requestId: number;
      ok: false;
      error: string;
    }
  | {
      kind: "permissionAuthorizationStatusResponse";
      requestId: number;
      ok: true;
      status: PermissionAuthorizationStatus;
    }
  | {
      kind: "permissionAuthorizationStatusResponse";
      requestId: number;
      ok: false;
      error: string;
    }
  | {
      kind: "permissionAuthorizationStatusesResponse";
      requestId: number;
      ok: true;
      statuses: PermissionAuthorizationStatus[];
    }
  | {
      kind: "permissionAuthorizationStatusesResponse";
      requestId: number;
      ok: false;
      error: string;
    }
  | {
      kind: "setPermissionAuthorizationStatusResponse";
      requestId: number;
      ok: true;
    }
  | {
      kind: "setPermissionAuthorizationStatusResponse";
      requestId: number;
      ok: false;
      error: string;
    }
  | {
      kind: "sessionChatIdentityKeyResponse";
      requestId: number;
      ok: true;
      key: Uint8Array | undefined;
    }
  | {
      kind: "sessionChatIdentityKeyResponse";
      requestId: number;
      ok: false;
      error: string;
    }
  | {
      kind: "deviceStatementKeyResponse";
      requestId: number;
      ok: true;
      key: Uint8Array | undefined;
    }
  | {
      kind: "deviceStatementKeyResponse";
      requestId: number;
      ok: false;
      error: string;
    }
  | {
      kind: "productSubtreePublicKeyResponse";
      requestId: number;
      ok: true;
      key: Uint8Array | undefined;
    }
  | {
      kind: "productSubtreePublicKeyResponse";
      requestId: number;
      ok: false;
      error: string;
    }
  | {
      kind: "deviceEncryptionKeyResponse";
      requestId: number;
      ok: true;
      key: Uint8Array;
    }
  | {
      kind: "deviceEncryptionKeyResponse";
      requestId: number;
      ok: false;
      error: string;
    }
  | { kind: "publishChatActionResponse"; requestId: number; ok: true }
  | {
      kind: "publishChatActionResponse";
      requestId: number;
      ok: false;
      error: string;
    }
  /**
   * Demand on one product's worker crossed zero. Posted in ledger order, so
   * the latest message for a product is its current level.
   */
  | { kind: "workerDemandChanged"; productId: string; wanted: boolean }
  | { kind: "publishRendererActionResponse"; requestId: number; ok: true }
  | {
      kind: "publishRendererActionResponse";
      requestId: number;
      ok: false;
      error: string;
    }
  /** One replacement tree, as a SCALE-encoded `RendererNode`. */
  | { kind: "renderItem"; renderId: number; node: Uint8Array }
  /** The product ended the render stream; no further items follow. */
  | { kind: "renderComplete"; renderId: number }
  | { kind: "renderError"; renderId: number; error: string }
  | {
      kind: "callbackRequest";
      requestId: number;
      name: CallbackName;
      args: CallbackArgs;
    }
  | {
      kind: "subscriptionStart";
      subId: number;
      name: SubscriptionName;
      payload: Uint8Array | string | null;
    }
  | { kind: "subscriptionStop"; subId: number }
  | { kind: "chainConnectStart"; connId: number; genesisHash: string }
  | { kind: "chainSend"; connId: number; request: string }
  | { kind: "chainClose"; connId: number };

/**
 * Is `url` a `ws://` URL on a loopback host? The tap forwards every frame
 * verbatim, key material included, and redacts nothing, so loopback is the whole
 * confinement story.
 *
 * `ws://` only, matching the native sink. A loopback socket has no path for TLS
 * to defend, so `wss://` would buy nothing and cost a certificate `localhost`
 * cannot get from a real CA.
 *
 * `WsDebugSink::connect` resolves the host and checks every address; this matches
 * the normalized hostname. A Worker has no resolver and needs none, since this
 * same string is handed to `new WebSocket`, so the native "validate one string,
 * dial another" gap cannot open here.
 *
 * Accepts what the native sink accepts: `localhost`, 127.0.0.0/8 and `::1`. An
 * IPv4-mapped literal is refused in both, since `Ipv6Addr::is_loopback` matches
 * only `::1`.
 */
export function isLoopbackWsUrl(url: string): boolean {
  try {
    const u = new URL(url);
    if (u.protocol !== "ws:") return false;
    const host = u.hostname.replace(/^\[|\]$/g, "").toLowerCase();
    return (
      host === "localhost" ||
      host === "::1" ||
      /^127\.\d{1,3}\.\d{1,3}\.\d{1,3}$/.test(host)
    );
  } catch {
    return false;
  }
}
