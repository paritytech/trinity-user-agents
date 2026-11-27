import {
  ConnectionResetError,
  createClient,
  createTransport,
  decodeWireMessage,
  type MethodIds,
  type WebSocketWireProvider,
} from "@parity/truapi";
import {
  createInternalClient,
  createWebSocketProviderFactory,
  type InternalTrUApiClient,
} from "@parity/truapi/internal";
import type { ConnectionStatus, HostTransport } from "@parity/truapi/sandbox";
import { SYSTEM_HANDSHAKE } from "@parity/truapi/wire-table";
import { createLegacyPort, type LegacyPort } from "./legacy-port.js";

/** One native host connection shared by product calls and browser authorization. */
export interface HostConnection {
  /** Frame channel for the product's own client, which survives connection resets. */
  readonly transport: HostTransport;
  /** Private generated authorization methods on the same transport. */
  readonly internal: InternalTrUApiClient;
  /** Frame channel for SDKs that predate the injected transport. */
  readonly legacyPort: MessagePort;
  /** Observe the connection's status, starting with the current one. */
  subscribeConnectionStatus(
    callback: (status: ConnectionStatus) => void,
  ): () => void;
  /** End this execution's connection permanently. */
  dispose(): void;
}

interface Connection {
  provider: WebSocketWireProvider;
  opening?: { wait?: Promise<void>; finish?: (error?: unknown) => void };
  verified: boolean;
  checkedAt: number;
  checking?: Promise<void>;
}

// A host can refuse a reconnect for a moment while it rebinds its listener. A
// visible page redials after these pauses, then waits for its next call.
const VISIBLE_RETRY_DELAYS_MS = [250, 1_000, 4_000];
const OPEN_WAIT_MS = 10_000;

/**
 * Creates a connection that fails the operations a lost connection carried and
 * sends the ones still waiting on its replacement.
 */
export function createHostConnection(
  url: string,
  createProvider: (
    url: string,
  ) => WebSocketWireProvider = createWebSocketProviderFactory(),
): HostConnection {
  const now = performance.now.bind(performance);
  let current: Connection | undefined;
  let stopped = false;
  let receive: ((frame: Uint8Array) => void) | undefined;
  let reset: ((error: Error) => void) | undefined;
  let productReceive: ((frame: Uint8Array) => void) | undefined;
  let productReset: ((error: Error) => void) | undefined;
  let legacy: LegacyPort | undefined;
  let status: ConnectionStatus = "disconnected";
  let retries = 0;
  let nextDial: Promise<void> | undefined;
  const listeners = new Set<(status: ConnectionStatus) => void>();

  function setStatus(next: ConnectionStatus): void {
    if (status === next) return;
    status = next;
    for (const listener of [...listeners]) {
      if (status !== next) return;
      try {
        listener(next);
      } catch {
        /* Product observers cannot stop connection recovery. */
      }
    }
  }

  function retire(connection: Connection, cause?: unknown): void {
    if (current !== connection) return;
    current = undefined;
    connection.opening?.finish?.(cause ?? new ConnectionResetError());
    legacy?.lost();
    for (const notify of [reset, productReset]) {
      try {
        notify?.(new ConnectionResetError({ cause }));
      } catch {
        /* Other callers must still recover. */
      }
    }
    connection.provider.dispose();
    if (!current) setStatus("disconnected");
    nextDial = undefined;
    if (stopped) return;
    console.warn(`[truapi] host link lost: ${describeCause(cause)}`);
    const delay = VISIBLE_RETRY_DELAYS_MS[retries];
    if (connection.verified) scheduleDial(0);
    else if (delay !== undefined && page?.visibilityState === "visible") {
      retries += 1;
      scheduleDial(delay);
    }
  }

  function scheduleDial(delay: number): void {
    nextDial = new Promise((resolve) =>
      setTimeout(() => {
        activate();
        resolve();
      }, delay),
    );
  }

  function open(): Connection {
    if (stopped) throw new ConnectionResetError();
    if (current) return current;
    let provider: WebSocketWireProvider;
    try {
      provider = createProvider(url);
    } catch (error) {
      setStatus("disconnected");
      throw error;
    }
    const connection: Connection = {
      provider,
      opening: {},
      verified: false,
      checkedAt: 0,
    };
    current = connection;
    provider.subscribe((frame) => {
      if (current !== connection) return;
      connection.checkedAt = now();
      const decoded = decodeWireMessage(frame);
      if (decoded.isErr()) return retire(connection, decoded.error);
      if (decoded.value.requestId.startsWith("host:")) return receive?.(frame);
      productReceive?.(frame);
      legacy?.receive(frame, decoded.value);
    });
    provider.subscribeClose?.((error) => retire(connection, error));
    // A suspended host can accept the upgrade after its callers have timed out.
    provider.opened.then(
      () => {
        if (current !== connection) return;
        const finish = connection.opening?.finish;
        connection.opening = undefined;
        finish?.();
        activate();
      },
      (error) => retire(connection, error),
    );
    setStatus("connecting");
    return connection;
  }

  function waitForOpen(connection: Connection): Promise<void> {
    const opening = connection.opening;
    if (!opening) return Promise.resolve();
    if (opening.wait) return opening.wait;
    // A pending upgrade gets the time a visible page would spend on its remaining retries.
    let budget = OPEN_WAIT_MS;
    if (page?.visibilityState === "visible") {
      for (const delay of VISIBLE_RETRY_DELAYS_MS.slice(retries))
        budget += delay + OPEN_WAIT_MS;
    }
    return (opening.wait = new Promise((resolve, reject) => {
      const deadline = setTimeout(() => {
        if (current === connection) legacy?.lost();
        opening.finish?.(new Error("WebSocket opening timed out"));
      }, budget);
      opening.finish = (error) => {
        opening.wait = undefined;
        opening.finish = undefined;
        clearTimeout(deadline);
        if (error === undefined) resolve();
        else reject(error);
      };
    }));
  }

  async function verify(connection: Connection): Promise<void> {
    if (current !== connection) throw new ConnectionResetError();
    try {
      const result = await handshake();
      if (result.isErr()) throw result.error;
      if (current !== connection) throw new ConnectionResetError();
      connection.verified = true;
      connection.checkedAt = now();
      retries = 0;
      setStatus("connected");
      legacy?.connected();
    } catch (error) {
      retire(connection, error);
      throw error;
    }
  }

  function ready(connection: Connection): Promise<void> {
    if (connection.verified && now() - connection.checkedAt < 10_000)
      return Promise.resolve();
    return (connection.checking ??= waitForOpen(connection)
      .then(() => verify(connection))
      .finally(() => {
        connection.checking = undefined;
      }));
  }

  function activate(): void {
    try {
      void ready(open()).catch(() => {});
    } catch {
      // A later call can try again, but nothing will carry the frames
      // waiting on the port.
      legacy?.lost();
    }
  }

  const page = typeof document === "undefined" ? undefined : document;
  const onVisibilityChange = () => {
    if (page?.visibilityState !== "visible") return;
    retries = 0;
    activate();
  };
  page?.addEventListener("visibilitychange", onVisibilityChange);

  function stop(): void {
    page?.removeEventListener("visibilitychange", onVisibilityChange);
    stopped = true;
    if (current) retire(current);
  }

  function prepare(ids: MethodIds): Promise<void> {
    const connection = open();
    if (
      ids.trait === SYSTEM_HANDSHAKE.trait &&
      ids.method === SYSTEM_HANDSHAKE.method
    )
      return waitForOpen(connection);
    return ready(connection).catch((error) => {
      if (current !== connection && nextDial)
        return nextDial.then(() => prepare(ids));
      throw error instanceof ConnectionResetError
        ? error
        : new ConnectionResetError({ cause: error });
    });
  }

  const transport = createTransport(
    {
      postMessage(frame) {
        const connection = current;
        // A call made ready on a connection that retired before its frame was
        // written has nowhere to go, so it fails now instead of at its deadline.
        if (!connection) return reset?.(new ConnectionResetError());
        write(connection, frame);
      },
      subscribe(callback) {
        receive = callback;
        return () => {
          receive = undefined;
        };
      },
      subscribeReset(callback) {
        reset = callback;
        return () => {
          reset = undefined;
        };
      },
      dispose: stop,
    },
    {
      requestIdPrefix: "host:",
      onProtocolError(error) {
        if (current) retire(current, error);
      },
      prepare,
    },
  );
  const client = createClient(transport);
  const handshake = client.system.handshake.bind(client.system);

  function write(connection: Connection, frame: Uint8Array): void {
    try {
      connection.provider.postMessage(frame);
    } catch (error) {
      retire(connection, error);
    }
  }

  function sendFromPort(frame: Uint8Array): boolean {
    if (!current?.verified) return false;
    write(current, frame);
    return true;
  }

  function subscribeConnectionStatus(
    callback: (status: ConnectionStatus) => void,
  ): () => void {
    listeners.add(callback);
    callback(status);
    return () => {
      listeners.delete(callback);
    };
  }

  return {
    transport: {
      postMessage(product) {
        // A copy, so the bytes checked are the bytes sent whatever the
        // product put on its own object.
        const frame = new Uint8Array(product);
        const decoded = decodeWireMessage(frame);
        if (decoded.isErr() || decoded.value.requestId.startsWith("host:"))
          return;
        if (!current) return productReset?.(new ConnectionResetError());
        write(current, frame);
      },
      subscribe(callback) {
        productReceive = callback;
        activate();
        return () => {
          if (productReceive === callback) productReceive = undefined;
        };
      },
      subscribeReset(callback) {
        productReset = callback;
        return () => {
          if (productReset === callback) productReset = undefined;
        };
      },
      prepare,
      subscribeConnectionStatus,
    },
    internal: createInternalClient(transport),
    get legacyPort() {
      if (!legacy) {
        legacy = createLegacyPort({ send: sendFromPort, dial: activate });
        activate();
      }
      return legacy.port;
    },
    subscribeConnectionStatus,
    dispose() {
      try {
        stop();
      } finally {
        transport.dispose();
      }
    },
  };
}

// The iOS host prints each console argument as JSON, which shows an Error as {}.
function describeCause(cause: unknown): string {
  if (cause instanceof Error) return cause.message;
  try {
    return JSON.stringify(cause);
  } catch {
    return String(cause);
  }
}
