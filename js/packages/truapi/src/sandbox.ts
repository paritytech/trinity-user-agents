/**
 * Sandbox bootstrap for browser-embedded hosts.
 *
 * Detects whether the app runs inside a TrUAPI host (iframe or webview), adopts
 * its client or builds the matching {@link WireProvider}, and exposes a cached
 * {@link TrUApiClient} via {@link getClientSync}. {@link subscribeConnectionStatus}
 * surfaces a connected / disconnected signal over that client.
 *
 * @module
 */

import {
  createMessagePortProvider,
  createWebSocketProvider,
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

declare global {
  interface Window {
    /** Public client and connection status supplied by the embedding host. */
    __HOST_API_CLIENT__?: {
      readonly client: TrUApiClient;
      subscribeConnectionStatus(
        callback: (status: ConnectionStatus) => void,
      ): () => void;
    };
    /** Set by webview hosts (Polkadot Desktop / Mobile) to mark the embedding. */
    __HOST_WEBVIEW_MARK__?: boolean;
    /** Injected by webview hosts to carry the host-side `MessagePort`. */
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
 * Detect whether the app is running inside a TrUAPI host container: an iframe
 * (including a cross-origin parent), a marked webview, or a window carrying an
 * injected host client or message port. Synchronous, so it can gate hot paths.
 */
export function isCorrectEnvironment(): boolean {
  if (webSocketEndpoint !== null) return true;
  const win = hostWindow();
  if (!win) return false;
  if (win.__HOST_API_CLIENT__) return true;
  if (isIframe()) return true;
  if (win.__HOST_WEBVIEW_MARK__ === true) return true;
  if (win.__HOST_API_PORT__ != null) return true;
  return false;
}

/**
 * Origin used as the `targetOrigin` for iframe bootstrap messages.
 */
function resolveHostOrigin(): string | null {
  if (typeof document !== "undefined" && document.referrer) {
    try {
      return new URL(document.referrer).origin;
    } catch {
      // Fall through to ancestorOrigins.
    }
  }
  // Firefox serializes cross-origin ancestors as "null", which is not a
  // valid postMessage targetOrigin; treat it as an unknown host origin.
  const ancestor = window.location?.ancestorOrigins?.[0];
  if (ancestor && ancestor !== "null") return ancestor;
  return null;
}

const HOST_PORT_TIMEOUT_MS = 20_000;
const IFRAME_READY_INTERVAL_MS = 50;

/**
 * Resolve the host-injected `MessagePort`, polling `window.__HOST_API_PORT__`
 * until it appears or the timeout elapses. Rejects on timeout or abort.
 *
 * TODO(cleanup): this polling is defensive against the port not being injected
 * yet. Once hosts guarantee the iframe / `__HOST_API_PORT__` is wired before any
 * product code runs, read `window.__HOST_API_PORT__` directly and drop the
 * poll loop + timeout.
 */
async function waitForWebviewPort(
  signal?: AbortSignal,
  timeoutMs = HOST_PORT_TIMEOUT_MS,
): Promise<MessagePort> {
  const start = Date.now();
  while (Date.now() - start < timeoutMs) {
    if (signal?.aborted) throw new Error("waitForWebviewPort aborted");
    const port = liveHostPort();
    if (port) return port;
    await new Promise((resolve) => setTimeout(resolve, 50));
  }
  throw new Error(
    `Timed out waiting for window.__HOST_API_PORT__ (${timeoutMs}ms)`,
  );
}

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
 * Build the {@link WireProvider} matching the detected environment (iframe or
 * webview). `onEstablished` fires once the host channel is live.
 */
function createSandboxProvider(
  endpoint: string | null,
  onEstablished: () => void,
): WireProvider {
  // Both branches settle off a promise, so the pipe may be dead by then.
  let closed = false;
  const established = () => {
    if (!closed) onEstablished();
  };
  const watchClose = <T extends WireProvider>(provider: T): T => {
    provider.subscribeClose?.(() => {
      closed = true;
    });
    return provider;
  };

  if (endpoint !== null) {
    const provider = watchClose(createWebSocketProvider(endpoint));
    provider.opened.then(established, () => {});
    return provider;
  }
  if (isIframe()) return createIframeCompatibilityProvider(established);

  const portController = new AbortController();
  const portPromise = waitForWebviewPort(portController.signal);
  const provider = watchClose(createMessagePortProvider(portPromise));
  portPromise.then(established, () => {});
  const baseDispose = provider.dispose;
  provider.dispose = () => {
    portController.abort();
    baseDispose?.();
  };
  return provider;
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
 * host container or if the provider can't be built. Host-injected clients retain
 * their identity across connection resets; other closed pipes renegotiate.
 */
export function getClientSync(): TrUApiClient | null {
  if (cachedConnection) return cachedConnection.client;
  if (!isCorrectEnvironment()) return null;
  try {
    const endpoint = webSocketEndpoint;
    const injected =
      endpoint === null ? hostWindow()?.__HOST_API_CLIENT__ : undefined;
    if (injected) {
      cachedConnection = { client: injected.client, endpoint, injected: true };
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
 * established (`truapi-init` MessagePort handover, first legacy frame, or
 * webview port), and `"disconnected"` outside a host container or when the
 * provider reports the pipe closed. Returns an unsubscribe function.
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
