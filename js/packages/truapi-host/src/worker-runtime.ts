/// <reference lib="webworker" />
// Worker entrypoint. Loads the web-targeted truapi WASM bundle and
// bridges every host callback over postMessage. The main thread keeps the
// state that needs DOM access (localStorage, prompts) while the core dispatcher
// runs here off the page main thread.

import { isLoopbackWsUrl } from "./worker-protocol.js";
import type {
  MainToWorker,
  SubscriptionName,
  WorkerToMain,
} from "./worker-protocol.js";
import type { GenericError } from "@parity/truapi";
import { TRUAPI_CODEC_VERSION } from "@parity/truapi";
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
    workerDemandChanged(productId: string, transition: WorkerTransition): void {
      postToMain({
        kind: "workerDemandChanged",
        productId,
        wanted: transition === "Start",
      });
    },
  };
}

/** Encode raw frame bytes as base64 (JSON can't carry binary over the WS). */
function toBase64(bytes: Uint8Array): string {
  let binary = "";
  for (let i = 0; i < bytes.length; i++)
    binary += String.fromCharCode(bytes[i]);
  return btoa(binary);
}

/**
 * Envelope version stamped on each frame, mirroring the debugger's
 * `WIRE_ENVELOPE_VERSION`. Kept in sync by hand (a value constant, not a shared
 * dep, to avoid truapi-host depending on the debugger package).
 */
const WIRE_ENVELOPE_VERSION = 1;

/**
 * The wire-contract fingerprint of the core that *encodes* the frames, or
 * `undefined` when this build of the core does not report one.
 *
 * The debugger decodes each frame against a `frameId → method` table, so the
 * `schema` an envelope carries has to be the fingerprint of the table the bytes
 * were encoded with. That is the WASM core's, not `@parity/truapi`'s: the client
 * and the core are separate artifacts, and `dist/wasm/web/` is gitignored and
 * built by hand (`make wasm`), so a stale core beside a fresh client is the
 * everyday case rather than an exotic one. Stamping the client's hash there would
 * make the debugger *confirm* identity on frames from a different table and decode
 * them into the wrong methods and values, silently.
 *
 * When the core does not report a hash, the envelope carries none. The debugger
 * treats an unstamped frame as unconfirmed: it still groups the op, but refuses
 * to decode values. Losing decode until `make wasm` is rerun is the honest
 * outcome; a confident wrong decode is not.
 */
export function coreWireSchemaHash(module: {
  wireSchemaHash?: () => string;
}): string | undefined {
  let hash: unknown;
  try {
    hash = module.wireSchemaHash?.();
  } catch {
    hash = undefined;
  }
  if (typeof hash === "string" && hash.length > 0) return hash;
  console.warn(
    "[truapi] wire debugger: this WASM core does not report its wire-schema hash — frames will stream without a `schema` stamp and the debugger will group them but refuse to decode values (rebuild the core with `make wasm`)",
  );
  return undefined;
}

/**
 * The socket surface the debugger link uses. A `WebSocket` satisfies it; tests
 * substitute a fake to drive backpressure and reconnect timing without a network.
 */
export interface DebuggerSocket {
  /** Bytes handed to the socket that it has not yet put on the wire. */
  readonly bufferedAmount: number;
  send(data: string): void;
  close(): void;
  addEventListener(
    type: "open" | "close" | "error",
    listener: () => void,
  ): void;
}

/** Construction options for {@link createDebuggerLink}. */
export interface DebuggerLinkOptions {
  /**
   * The encoding core's wire-schema hash, from {@link coreWireSchemaHash}. When
   * omitted, envelopes carry no `schema` and the debugger refuses value decode
   * rather than trusting a hash the core never vouched for.
   */
  schema?: string;
  /** Socket factory. Defaults to a real `WebSocket`; tests inject a fake. */
  createSocket?: (url: string) => DebuggerSocket;
  /** Deferred scheduler for reconnect backoff. Defaults to `setTimeout`. */
  schedule?: (run: () => void, delayMs: number) => void;
}

/** Initial reconnect delay; doubles per failed dial up to {@link RECONNECT_MAX_MS}. */
const RECONNECT_BASE_MS = 200;

/** Cap on the reconnect backoff. Mirrors the native sink's `MAX_BACKOFF`. */
const RECONNECT_MAX_MS = 5000;

/**
 * Ceiling on the socket's *own* unflushed send buffer before frames are shed.
 *
 * The queue caps below only bound what this module holds while the socket is
 * down. A socket that is open but whose peer has stopped reading keeps
 * `readyState === OPEN` while `bufferedAmount` grows without limit, and that
 * growth is charged to the observed session's worker: handing frames to it
 * unchecked is the same unbounded buffering the queue caps exist to prevent, one
 * layer lower. Over this ceiling, frames are shed into the counted `dropped`
 * instead.
 */
const MAX_SOCKET_BUFFERED_BYTES = 8 * 1024 * 1024;

/**
 * Ceiling on a SINGLE encoded message, enforced on the live path and again when
 * the backlog drains.
 *
 * `MAX_SOCKET_BUFFERED_BYTES` bounds the socket's cumulative backlog, which one
 * oversized frame passes straight through on an otherwise idle socket. The
 * debugger closes the connection on an over-cap message rather than dropping it
 * (Bun: close 1006, "Received too big message"), so an unshed frame costs the
 * whole stream. Base64 inflates 4/3, so this sits below the server's own limit
 * with room for the envelope's other fields.
 */
const MAX_MESSAGE_BYTES = 6 * 1024 * 1024;

/**
 * Dev-only link to the debugger the host dials. Fire-and-forget by construction:
 * it opens lazily, buffers a bounded backlog until the socket is up, retries a
 * dropped connection with capped backoff, sheds frames (counted) rather than
 * buffering without bound at either layer, and swallows every error - a slow,
 * absent, or crashed debugger only loses the trace, it can never throw into the
 * frame path.
 */
export function createDebuggerLink(
  url: string,
  options: DebuggerLinkOptions = {},
): {
  emit(channelId: string, dir: string, frame: Uint8Array): void;
} {
  // Loopback-only, dev-only: a non-loopback (or non-ws://) debugger URL yields an
  // inert link rather than streaming frames across the network. Warn so a
  // mistyped value reads as "misconfigured", not "the debugger doesn't work".
  if (!isLoopbackWsUrl(url)) {
    console.warn(
      `[truapi] wire debugger URL rejected (must be ws:// on a loopback host): ${url}`,
    );
    return { emit() {} };
  }
  const createSocket =
    options.createSocket ?? ((target: string) => new WebSocket(target));
  const schedule =
    options.schedule ??
    ((run: () => void, delayMs: number) => {
      setTimeout(run, delayMs);
    });
  const schema = options.schema;
  let socket: DebuggerSocket | null = null;
  let open = false;
  const queue: string[] = [];
  // Count *and* byte caps: each queued item is a base64 ProtocolMessage (storage
  // writes, RPC responses - up to MBs each), so a count-only cap would let a slow
  // or absent debugger buffer unbounded RSS on the observed session. Whichever
  // ceiling hits first drops the frame (counted), never blocking the frame path.
  const MAX_QUEUE = 1000;
  const MAX_QUEUE_BYTES = 8 * 1024 * 1024;
  let queuedBytes = 0;
  let droppedSinceSend = 0;
  let reconnectDelayMs = RECONNECT_BASE_MS;
  let reconnectScheduled = false;

  /**
   * Dial again after the current backoff, at most one dial in flight.
   *
   * Without the delay this ran once per frame: a busy session with no debugger
   * listening dialed loopback hundreds of times a second (each refused
   * immediately, each logging a console error), because every emit found
   * `socket === null` and redialled. The native sink has always backed off; this
   * mirrors it.
   */
  function scheduleReconnect(): void {
    if (socket !== null || reconnectScheduled) return;
    reconnectScheduled = true;
    const delayMs = reconnectDelayMs;
    reconnectDelayMs = Math.min(reconnectDelayMs * 2, RECONNECT_MAX_MS);
    try {
      schedule(() => {
        reconnectScheduled = false;
        if (socket === null) connect();
      }, delayMs);
    } catch {
      // No timer available: fall back to redialling on the next emit.
      reconnectScheduled = false;
    }
  }

  /** Drain the backlog onto a freshly opened socket. */
  function flush(): void {
    const pending = queue.splice(0);
    queuedBytes = 0;
    // Deliver drops accumulated while disconnected by stamping the count on the
    // first drained frame - a bare marker without channelId/dir/frame wouldn't
    // parse server-side. Drops only happen once the queue is full, so when the
    // count is nonzero there is always a pending frame to carry it; if not, it
    // rides the next live emit.
    // The count is only cleared once a frame carrying it is actually handed to
    // the socket. Clearing it up front lost the whole gap whenever the drain
    // failed - 1100 shed frames reported as a clean session.
    let carried = 0;
    if (pending.length > 0 && droppedSinceSend > 0) {
      try {
        const first = JSON.parse(pending[0]) as Record<string, unknown>;
        first.dropped = droppedSinceSend;
        pending[0] = JSON.stringify(first);
        carried = droppedSinceSend;
      } catch {
        // Leave the frame as-is; the count rides the next live emit.
      }
    }
    for (const [index, message] of pending.entries()) {
      // The drained path is subject to the same two ceilings as the live one: a
      // wedged socket must not be force-fed the backlog, and an over-cap message
      // closes the debugger's connection and costs every frame after it.
      const open = socket;
      if (
        open === null ||
        open.bufferedAmount > MAX_SOCKET_BUFFERED_BYTES ||
        message.length > MAX_MESSAGE_BYTES
      ) {
        shed();
        continue;
      }
      if (send(message)) {
        if (index === 0) droppedSinceSend -= carried;
      } else {
        shed();
      }
    }
  }

  function connect(): void {
    let dialed: DebuggerSocket;
    try {
      dialed = createSocket(url);
    } catch {
      socket = null;
      scheduleReconnect();
      return;
    }
    socket = dialed;
    dialed.addEventListener("open", () => {
      open = true;
      // A dial that reached the debugger earns the short delay back, so a
      // debugger that restarts is picked up promptly rather than after the cap.
      reconnectDelayMs = RECONNECT_BASE_MS;
      flush();
    });
    dialed.addEventListener("close", () => {
      open = false;
      if (socket === dialed) socket = null;
    });
    dialed.addEventListener("error", () => {
      // A socket that fired `error` is dead: close it explicitly (tidiness), then
      // null it so the next emit schedules a redial. Without the null, a runtime
      // that fires `error` without a following `close` would leave `socket`
      // non-null and frames would buffer then drop.
      open = false;
      if (socket === dialed) socket = null;
      try {
        dialed.close();
      } catch {
        // already closed / closing
      }
    });
  }

  function send(message: string): boolean {
    // A null socket is NOT a success: returning true there would clear the drop
    // count against a frame that went nowhere. Note the residual limit - per
    // WHATWG, `WebSocket.send()` on a CLOSING/CLOSED socket discards silently
    // without throwing, so a `true` here means "handed over", not "delivered".
    const live = socket;
    if (live === null) return false;
    try {
      live.send(message);
      return true;
    } catch {
      // A dead socket must never break the frame path. The caller keeps its
      // pending drop count rather than clearing it against a send that failed -
      // otherwise a gap the host really did cause is reported as no gap at all.
      return false;
    }
  }

  connect();

  let warnedDrop = false;
  /** Shed one frame into the counted backlog gap. */
  function shed(): void {
    droppedSinceSend += 1;
    if (!warnedDrop) {
      // The link buffers a bounded backlog while the debugger is absent/slow, and
      // stops handing frames to a socket that is not draining. Warn once so the
      // gap is attributable to the link, not the host.
      warnedDrop = true;
      console.warn(
        "[truapi] wire debugger link is not keeping up — dropping frames (counted in `dropped`) until it drains",
      );
    }
  }

  return {
    emit(channelId, dir, frame) {
      // A debug tap must never throw into the observed frame path: toBase64 /
      // JSON.stringify can raise on a pathological frame (btoa or V8 string-length
      // limits), and only send() swallows its own errors. Losing a trace is fine;
      // breaking dispatch is not.
      try {
        const live = open ? socket : null;
        // Checked before encoding, so a shed frame costs no base64 either.
        if (live !== null && live.bufferedAmount > MAX_SOCKET_BUFFERED_BYTES) {
          shed();
          return;
        }
        const base = {
          v: WIRE_ENVELOPE_VERSION,
          codec: TRUAPI_CODEC_VERSION,
          // Only when the core vouched for it: see coreWireSchemaHash.
          ...(schema !== undefined ? { schema } : {}),
          channelId,
          dir,
          // The producer is the only party that knows when the frame crossed. The
          // debugger's own clock is the flush instant for anything that waited in
          // the queue below, which collapses every duration in a backlog to 0ms
          // and pulls ops minutes apart into one retry-storm window.
          observedAt: Date.now(),
          frame: toBase64(frame),
        };
        if (live !== null) {
          // Piggyback any frames dropped while the link was down onto the next
          // live frame, so the debugger attributes the gap to the link, not the
          // host.
          const message =
            droppedSinceSend > 0
              ? JSON.stringify({ ...base, dropped: droppedSinceSend })
              : JSON.stringify(base);
          // One over-cap message closes the debugger's socket, taking the whole
          // stream with it. Shedding this frame keeps the rest.
          if (message.length > MAX_MESSAGE_BYTES) {
            shed();
            return;
          }
          if (send(message)) {
            droppedSinceSend = 0;
          } else {
            // The frame just handed over is lost as well, not only the earlier
            // ones: counting the prior gap but not this frame under-reports by
            // exactly the frames whose send failed.
            shed();
          }
          return;
        }
        // Nothing leaves the queue except through flush(), so everything that
        // enters it is by definition replayed rather than live: mark it here and
        // the debugger can tell a backlog gap from a quiet session. Its
        // `observedAt` above is already the real crossing time, so the marker is
        // provenance, not a correction.
        const message = JSON.stringify({ ...base, buffered: true });
        if (
          queue.length < MAX_QUEUE &&
          queuedBytes + message.length <= MAX_QUEUE_BYTES
        ) {
          queue.push(message);
          queuedBytes += message.length;
        } else {
          shed();
        }
        scheduleReconnect();
      } catch {
        // Swallow: never let the tap disturb the frame path.
      }
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

let runtime: WorkerPairingHostRuntime | null = null;
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
      if (runtime) {
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
          runtime = new SigningRuntime(
            buildRawCallbacks(msg.capabilities),
            msg.hostConfig,
          );
        } else {
          runtime = new wasm.WasmPairingHostRuntime(
            buildRawCallbacks(msg.capabilities),
            msg.hostConfig,
          );
        }
        postToMain({ kind: "ready", schema: coreWireSchemaHash(wasm) });
      } catch (err) {
        postToMain({ kind: "fatalError", error: `init: ${errorMessage(err)}` });
      }
      break;
    case "createCore":
      if (!runtime) {
        postToMain({
          kind: "coreError",
          coreId: msg.coreId,
          error: "createCore received before runtime is ready",
        });
        break;
      }
      try {
        const core = runtime.productRuntime(
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
      runtime?.cancelPairing();
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
    case "getProductSubtreePublicKey":
      void handleGetProductSubtreePublicKey(
        msg.requestId,
        msg.productId,
        msg.timeoutMs,
      );
      break;
    case "notifySessionStoreChanged":
      runtime?.notifySessionStoreChanged();
      break;
    case "notifyContactsChanged":
      runtime?.notifyContactsChanged();
      break;
    case "acquireWorker":
      runtime?.acquireWorker(msg.productId);
      break;
    case "releaseWorker":
      runtime?.releaseWorker(msg.productId);
      break;
    case "activateStoredSession":
      void handleSessionActivation(
        msg.requestId,
        "activateStoredSession",
        (rt) => rt.activateStoredSession(),
      );
      break;
    case "activateExternalSession": {
      const { blob } = msg;
      void handleSessionActivation(
        msg.requestId,
        "activateExternalSession",
        (rt) => rt.activateExternalSession(blob),
      );
      break;
    }
    case "activateLocalSession": {
      const { secret, liteUsername } = msg;
      void handleSessionActivation(
        msg.requestId,
        "activateLocalSession",
        (rt) => {
          const signing = rt as Partial<WorkerSigningHostRuntime>;
          if (typeof signing.activateLocalSession !== "function") {
            // A pairing host has no local secret to activate; saying so beats
            // a TypeError about an undefined function.
            return Promise.reject(
              new Error(
                "activateLocalSession needs a signing host; this runtime is " +
                  'a pairing host (pass role: "signing" to init)',
              ),
            );
          }
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
        (rt) => {
          const signing = rt as Partial<WorkerSigningHostRuntime>;
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
        (rt) => {
          if (typeof rt.setSubmitPreimagesLocally !== "function") {
            return Promise.reject(
              new Error(
                "setSubmitPreimagesLocally needs a core built with " +
                  "`test-host`; this core does not carry it",
              ),
            );
          }
          rt.setSubmitPreimagesLocally(local);
          return Promise.resolve();
        },
      );
      break;
    }
    case "setWithheldResources": {
      const { tags } = msg;
      void handleSessionActivation(msg.requestId, "setWithheldResources", (rt) => {
        const signing = rt as Partial<WorkerSigningHostRuntime>;
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
      });
      break;
    }
    case "resetSessionState":
      void handleSessionActivation(msg.requestId, "resetSessionState", (rt) =>
        rt.resetSessionState(),
      );
      break;
    case "getPermissionAuthorizationStatus":
      void handleGetPermissionAuthorizationStatus(
        runtime,
        postToMain,
        msg.productId,
        msg.requestId,
        msg.request,
      );
      break;
    case "getPermissionAuthorizationStatuses":
      void handleGetPermissionAuthorizationStatuses(
        runtime,
        postToMain,
        msg.productId,
        msg.requestId,
        msg.requests,
      );
      break;
    case "setPermissionAuthorizationStatus":
      void handleSetPermissionAuthorizationStatus(
        runtime,
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
      // its `if (!runtime)` path instead of calling into a runtime being torn
      // down; free the captured handle after the cores finish disposing.
      const disposing = runtime;
      runtime = null;
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

async function handleSessionActivation(
  requestId: number,
  label: string,
  activate: (runtime: WorkerPairingHostRuntime) => Promise<void>,
): Promise<void> {
  if (!runtime) {
    postToMain({
      kind: "sessionActivationResponse",
      requestId,
      ok: false,
      error: `${label} received before runtime is ready`,
    });
    return;
  }
  try {
    await activate(runtime);
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
  if (!runtime) {
    postToMain({
      kind: "disconnectSessionResponse",
      requestId,
      ok: false,
      error: "disconnectSession received before runtime is ready",
    });
    return;
  }
  try {
    await runtime.disconnectSession();
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
  if (!runtime) {
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
      key: runtime.sessionChatIdentityKey(),
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
  if (!runtime) {
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
      key: runtime.deviceStatementKey(),
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
  productId: string,
  timeoutMs: number | undefined,
): Promise<void> {
  if (!runtime) {
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
      key: await runtime.productSubtreePublicKey(productId, timeoutMs),
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
  if (!runtime) {
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
      key: await runtime.deviceEncryptionKey(),
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
