/**
 * Sandbox bootstrap for browser-embedded hosts.
 *
 * Detects whether the app runs inside a TrUAPI host (iframe or webview), builds
 * a client over the host's injected {@link HostTransport} or the matching
 * {@link WireProvider}, and exposes a cached
 * {@link TrUApiClient} via {@link getClientSync}. {@link subscribeConnectionStatus}
 * surfaces a connected / disconnected signal over that client.
 *
 * @module
 */

import {
  ConnectionResetError,
  createMessagePortProvider,
  createWebSocketProvider,
  type MethodIds,
  type WireProvider,
} from "./transport.js";
import { createTransport } from "./client.js";
import { createClient, type TrUApiClient } from "./generated/index.js";
import { tryCreateLegacyIframeProvider } from "./sandbox-legacy.js";

/**
 * Connection lifecycle state. {@link subscribeConnectionStatus} emits
 * `"connecting"` while the client waits for the host channel, `"connected"`
 * once the channel is established, and `"disconnected"` outside a host or
 * after the channel closes.
 */
export type ConnectionStatus = "disconnected" | "connecting" | "connected";

/**
 * Frame channel a webview host injects as `window.__HOST_API_TRANSPORT__`.
 *
 * The host owns the connection to its core and recovers it. The product's own
 * client encodes every frame it sends, so a product speaks the protocol
 * version it was built against whatever version the host was built with.
 */
export interface HostTransport {
  /** Send one encoded frame to the host. */
  postMessage(frame: Uint8Array): void;
  /**
   * Receive the frames addressed to the product. Subscribing opens the
   * connection. The channel serves one client: a later subscriber replaces
   * the earlier one.
   */
  subscribe(callback: (frame: Uint8Array) => void): () => void;
  /** Learn that the connection was replaced, losing the work the host may have started. */
  subscribeReset(callback: (error: Error) => void): () => void;
  /** Resolve once the connection can carry a call to `ids`; reject if it cannot. */
  prepare(ids: MethodIds): Promise<void>;
  /** Observe the connection's status, starting with the current one. */
  subscribeConnectionStatus(
    callback: (status: ConnectionStatus) => void,
  ): () => void;
}

declare global {
  interface Window {
    /** Frame channel supplied by the embedding host. */
    __HOST_API_TRANSPORT__?: HostTransport;
    /** The iframe host's `MessagePort`, which a rebuilt client reuses. */
    __HOST_API_PORT__?: MessagePort;
  }
}

/** Endpoint explicitly selected by {@link connectWebSocketHost}. */
let webSocketEndpoint: string | null = null;

function hostWindow(): Window | null {
  return typeof window === "undefined" ? null : window;
}

/** A closed port a host locked in place, so it survived being dropped. */
let lockedDeadPort: MessagePort | null = null;

/** The injected port a build may adopt, skipping one already closed. */
function liveHostPort(): MessagePort | null {
  const port = hostWindow()?.__HOST_API_PORT__;
  return port && port !== lockedDeadPort ? port : null;
}

/** Drop the injected port, or a rebuild re-adopts the closed one. */
function forgetHostPort(): void {
  const win = hostWindow();
  if (!win) return;
  try {
    delete win.__HOST_API_PORT__;
  } catch {
    // A host may lock the property; the close must still be reported.
  }
  lockedDeadPort = win.__HOST_API_PORT__ ?? null;
}

function isIframe(): boolean {
  try {
    return window !== window.top;
  } catch {
    // A cross-origin parent throws on access, which itself means we're embedded.
    return true;
  }
}

/**
 * Detect whether the SDK can build a client here: a WebSocket endpoint chosen
 * with {@link connectWebSocketHost}, an injected host transport, or an iframe
 * (including a cross-origin parent). Synchronous, so it can gate hot paths.
 */
export function isCorrectEnvironment(): boolean {
  if (webSocketEndpoint !== null) return true;
  const win = hostWindow();
  if (!win) return false;
  if (win.__HOST_API_TRANSPORT__) return true;
  return isIframe();
}

/**
 * Origin used as the `targetOrigin` for iframe bootstrap messages.
 *
 * `ancestorOrigins` names the embedding document whatever this frame loaded
 * before, so it comes first. The referrer is the fallback, but once the
 * product navigates its own frame it names the product's previous page, not
 * the host, and a referrer from this document's own origin is set aside.
 */
function resolveHostOrigin(): string | null {
  // Firefox serializes cross-origin ancestors as "null", which is not a
  // valid postMessage targetOrigin; treat it as an unknown host origin.
  const ancestor = window.location?.ancestorOrigins?.[0];
  if (ancestor && ancestor !== "null") return ancestor;
  if (typeof document !== "undefined" && document.referrer) {
    try {
      const referrer = new URL(document.referrer).origin;
      if (referrer !== window.location?.origin) return referrer;
    } catch {
      // An unparseable referrer names no host.
    }
  }
  return null;
}

const IFRAME_READY_INTERVAL_MS = 50;

/**
 * Create an iframe provider that negotiates the transport from the first valid
 * parent message: modern hosts answer `truapi-ready` with a transferred
 * `MessagePort`; legacy hosts are handled by one removable fallback below.
 * Outbound frames are queued until one path wins.
 * `onEstablished` fires once, when the inner provider is adopted.
 */
function createIframeCompatibilityProvider(
  onEstablished: () => void,
): WireProvider {
  const maybeWin = hostWindow();
  if (!maybeWin) throw new Error("window is unavailable");
  const win = maybeWin;

  const target = win.parent;
  const hostOrigin = resolveHostOrigin();
  let inner: WireProvider | null = null;
  let unsubscribeInner: (() => void) | null = null;
  let unsubscribeInnerClose: (() => void) | null = null;
  let closedError: Error | null = null;
  let cancelReadyRetry: (() => void) | null = null;
  const queued: Uint8Array[] = [];
  const listeners = new Set<(message: Uint8Array) => void>();
  const closeListeners = new Set<(error: Error) => void>();
  const stopReadyRetry = (): void => {
    cancelReadyRetry?.();
    cancelReadyRetry = null;
  };

  const close = (error: Error): void => {
    if (closedError) return;
    closedError = error;
    stopReadyRetry();
    win.removeEventListener("message", onMessage);
    unsubscribeInner?.();
    unsubscribeInnerClose?.();
    for (const listener of [...closeListeners]) listener(error);
    listeners.clear();
    closeListeners.clear();
    queued.length = 0;
  };
  const deliver = (message: Uint8Array): void => {
    if (closedError) return;
    for (const listener of [...listeners]) listener(message);
  };
  const adopt = (provider: WireProvider): void => {
    inner = provider;
    stopReadyRetry();
    win.removeEventListener("message", onMessage);
    unsubscribeInner = provider.subscribe(deliver);
    unsubscribeInnerClose = provider.subscribeClose?.(close) ?? null;
    for (const message of queued.splice(0)) provider.postMessage(message);
    onEstablished();
  };
  const adoptPort = (port: MessagePort): void => {
    win.__HOST_API_PORT__ = port;
    adopt(createMessagePortProvider(port));
  };
  function onMessage(event: MessageEvent): void {
    if (inner !== null || closedError !== null) return;
    if (event.source !== target) return;
    if (hostOrigin !== null && event.origin !== hostOrigin) return;

    if (event.data?.type === "truapi-init") {
      const [port] = event.ports;
      if (!port) {
        close(new Error("truapi-init did not include a MessagePort"));
        return;
      }
      adoptPort(port);
      return;
    }
    // TODO(remove-legacy-host): Delete this fallback and its import once all
    // iframe hosts transfer a MessagePort in `truapi-init`. The modern path
    // above is otherwise independent of legacy transport details.
    const legacy = tryCreateLegacyIframeProvider(win, target, event);
    if (legacy) {
      adopt(legacy.provider);
      deliver(legacy.initialMessage);
    }
  }

  const existing = liveHostPort();
  if (existing) {
    adoptPort(existing);
  } else {
    win.addEventListener("message", onMessage);
    // The host and product load independently. Repeat the data-free ready
    // signal until a valid parent response establishes either transport.
    const postReady = (): void => {
      target.postMessage({ type: "truapi-ready" }, hostOrigin ?? "*");
    };
    const interval = win.setInterval(postReady, IFRAME_READY_INTERVAL_MS);
    cancelReadyRetry = () => win.clearInterval(interval);
    postReady();
  }

  return {
    postMessage(message) {
      if (closedError) throw closedError;
      if (inner) inner.postMessage(message);
      else queued.push(message);
    },
    subscribe(callback) {
      if (closedError) return () => {};
      listeners.add(callback);
      return () => listeners.delete(callback);
    },
    subscribeClose(callback) {
      if (closedError) {
        callback(closedError);
        return () => {};
      }
      closeListeners.add(callback);
      return () => closeListeners.delete(callback);
    },
    dispose() {
      inner?.dispose();
      close(new Error("iframe provider disposed"));
    },
  };
}

/**
 * Build the {@link WireProvider} for a WebSocket endpoint, or for the iframe
 * embedding when there is none. `onEstablished` fires once the host channel is
 * live.
 */
function createSandboxProvider(
  endpoint: string | null,
  onEstablished: () => void,
): WireProvider {
  if (endpoint === null)
    return createIframeCompatibilityProvider(onEstablished);

  // The socket opens off a promise, so the pipe may be dead by then.
  let closed = false;
  const provider = createWebSocketProvider(endpoint);
  provider.subscribeClose?.(() => {
    closed = true;
  });
  provider.opened.then(
    () => {
      if (!closed) onEstablished();
    },
    () => {},
  );
  return provider;
}

/**
 * Build this product's client over the host's transport. The host's errors
 * come from the host's build, so they are rewrapped in this build's
 * {@link ConnectionResetError} for products that check the class.
 */
function createInjectedClient(host: HostTransport): TrUApiClient {
  let reset: ((cause: Error) => void) | undefined;
  return createClient(
    createTransport(
      {
        postMessage: (frame) => host.postMessage(frame),
        subscribe: (callback) => host.subscribe(callback),
        subscribeReset: (callback) => {
          reset = (cause) => callback(new ConnectionResetError({ cause }));
          return host.subscribeReset(reset);
        },
        dispose() {},
      },
      {
        // The host keeps its connection and this client is never rebuilt, so
        // an unreadable frame ends the work in flight instead of the client.
        onProtocolError: (error) => reset?.(error),
        prepare: async (ids) => {
          try {
            await host.prepare(ids);
          } catch (cause) {
            throw new ConnectionResetError({ cause });
          }
        },
      },
    ),
  );
}

let cachedConnection: {
  client: TrUApiClient;
  endpoint: string | null;
  injected: boolean;
} | null = null;
let status: ConnectionStatus = "disconnected";
const statusListeners = new Set<(status: ConnectionStatus) => void>();

function setStatus(next: ConnectionStatus): void {
  if (status === next) return;
  status = next;
  for (const listener of [...statusListeners]) {
    // A listener can change the status, and that call notified everyone.
    if (status !== next) return;
    listener(next);
  }
}

/**
 * Build (or return the cached) {@link TrUApiClient}. Returns `null` outside a
 * host container or if the provider can't be built. A client over an injected
 * host transport retains its identity across connection resets; other closed
 * pipes renegotiate.
 */
export function getClientSync(): TrUApiClient | null {
  if (cachedConnection) return cachedConnection.client;
  if (!isCorrectEnvironment()) return null;
  try {
    const endpoint = webSocketEndpoint;
    const injected =
      endpoint === null ? hostWindow()?.__HOST_API_TRANSPORT__ : undefined;
    if (injected) {
      cachedConnection = {
        client: createInjectedClient(injected),
        endpoint,
        injected: true,
      };
      injected.subscribeConnectionStatus(setStatus);
      return cachedConnection.client;
    }
    const provider = createSandboxProvider(endpoint, () =>
      setStatus("connected"),
    );
    cachedConnection = {
      client: createClient(createTransport(provider)),
      endpoint,
      injected: false,
    };
    provider.subscribeClose?.(() => {
      // Cleared first: a listener may call getClientSync from the notify below.
      cachedConnection = null;
      if (endpoint === null) forgetHostPort();
      setStatus("disconnected");
    });
    return cachedConnection?.client ?? null;
  } catch {
    return null;
  }
}

/**
 * Connect to a host that serves protocol frames over a WebSocket, and return
 * the client for it. From then on this module treats that endpoint as the host:
 * {@link isCorrectEnvironment} reports `true` and {@link getClientSync} returns
 * the same cached client, so product code that already runs inside a webview or
 * an iframe needs no changes.
 *
 * The endpoint is whatever a host exposes on loopback. For local development
 * that is `truapi-host signing-host --frame-listen 127.0.0.1:9955`:
 *
 * ```ts
 * connectWebSocketHost("ws://127.0.0.1:9955");
 * ```
 *
 * Call it before anything else touches the client. It throws while a live
 * client for a different transport exists, because that client is cached and
 * cannot be redirected. Once the pipe closes there is no client to redirect, so
 * a different endpoint is accepted.
 */
export function connectWebSocketHost(url: string): TrUApiClient | null {
  if (cachedConnection !== null && cachedConnection.endpoint !== url) {
    throw new Error(
      "connectWebSocketHost must be called before the TrUAPI client is created",
    );
  }
  webSocketEndpoint = url;
  return getClientSync();
}

/**
 * Subscribe to connection-status changes. The callback fires immediately with
 * the current status and on every transition. Status is `"connecting"` while
 * the client waits for the host channel, `"connected"` once the channel is
 * established (`truapi-init` MessagePort handover or first legacy frame), and
 * `"disconnected"` outside a host container or when the provider reports the
 * pipe closed. Returns an unsubscribe function.
 */
export function subscribeConnectionStatus(
  callback: (status: ConnectionStatus) => void,
): () => void {
  let emitted = false;
  const listener = (next: ConnectionStatus) => {
    emitted = true;
    callback(next);
  };
  statusListeners.add(listener);

  if (status === "disconnected") {
    // Building the client may establish the channel synchronously (an already
    // injected port), in which case the status is already "connected" here.
    const client = getClientSync();
    if (client && !cachedConnection?.injected && status === "disconnected") {
      setStatus("connecting");
    }
  }
  if (!emitted) {
    callback(status);
  }

  return () => {
    statusListeners.delete(listener);
  };
}
