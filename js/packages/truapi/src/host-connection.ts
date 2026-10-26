import { createTransport } from "./client.js";
import { createClient, type TrUApiClient } from "./generated/client.js";
import {
  createInternalClient,
  type InternalTrUApiClient,
} from "./generated/internal-client.js";
import { SYSTEM_HANDSHAKE } from "./generated/wire-table.js";
import type { ConnectionStatus } from "./sandbox.js";
import {
  ConnectionResetError,
  createWebSocketProviderFactory,
  decodeWireMessage,
  type WebSocketWireProvider,
} from "./transport.js";

/** One native host connection shared by product calls and browser authorization. */
export interface HostConnection {
  /** Stable public client. Reading it also connects passive host-initiated handlers. */
  readonly client: TrUApiClient;
  /** Private generated authorization methods on the same transport. */
  readonly internal: InternalTrUApiClient;
  /** Startup compatibility for SDKs predating the injected client. */
  readonly legacyPort: MessagePort;
  /** Observe connection readiness without replacing the client. */
  subscribeConnectionStatus(
    callback: (status: ConnectionStatus) => void,
  ): () => void;
  /** End this execution's connection permanently. */
  dispose(): void;
}

interface Connection {
  provider: WebSocketWireProvider;
  verified: boolean;
  checkedAt: number;
  checking?: Promise<void>;
}

// A host can refuse a reconnect for a moment while it rebinds its listener. A
// visible page redials after these pauses, then waits for its next call.
const VISIBLE_RETRY_DELAYS_MS = [250, 1_000, 4_000];

/** Creates a client whose interrupted operations fail and whose later calls reconnect. */
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
  let legacy: { receive(frame: Uint8Array): void; close(): void } | undefined;
  let legacyPort: MessagePort | undefined;
  let status: ConnectionStatus = "disconnected";
  let retries = 0;
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
    const oldLegacy = legacy;
    legacy = undefined;
    oldLegacy?.close();
    try {
      reset?.(new ConnectionResetError({ cause }));
    } catch {
      /* Other callers must still recover. */
    }
    connection.provider.dispose();
    if (!current) setStatus("disconnected");
    if (stopped) return;
    const delay = VISIBLE_RETRY_DELAYS_MS[retries];
    if (connection.verified) setTimeout(activate, 0);
    else if (delay !== undefined && page?.visibilityState === "visible") {
      retries += 1;
      setTimeout(activate, delay);
    }
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
    const connection = { provider, verified: false, checkedAt: 0 };
    current = connection;
    provider.subscribe((frame) => {
      if (current !== connection) return;
      connection.checkedAt = now();
      if (legacy) {
        const decoded = decodeWireMessage(frame);
        if (decoded.isErr()) return retire(connection, decoded.error);
        if (!decoded.value.requestId.startsWith("host:"))
          return legacy.receive(frame);
      }
      receive?.(frame);
    });
    provider.subscribeClose?.((error) => retire(connection, error));
    setStatus("connecting");
    return connection;
  }

  function ready(connection: Connection): Promise<void> {
    if (connection.verified && now() - connection.checkedAt < 10_000)
      return Promise.resolve();
    return (connection.checking ??= Promise.resolve(handshake())
      .then((result) => {
        if (result.isErr()) throw result.error;
        if (current !== connection) throw new ConnectionResetError();
        connection.verified = true;
        connection.checkedAt = now();
        retries = 0;
        setStatus("connected");
      })
      .catch((error) => {
        retire(connection, error);
        throw error;
      })
      .finally(() => {
        connection.checking = undefined;
      }));
  }

  function activate(): void {
    try {
      void ready(open()).catch(() => {});
    } catch {
      /* A later call can try again. */
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

  const transport = createTransport(
    {
      postMessage(frame) {
        const connection = current;
        if (!connection) return;
        try {
          connection.provider.postMessage(frame);
        } catch (error) {
          retire(connection, error);
        }
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
      prepare(ids) {
        const connection = open();
        return ids.trait === SYSTEM_HANDSHAKE.trait &&
          ids.method === SYSTEM_HANDSHAKE.method
          ? connection.provider.opened
          : ready(connection);
      },
    },
  );
  const client = createClient(transport);
  const handshake = client.system.handshake.bind(client.system);

  return {
    get client() {
      activate();
      return client;
    },
    internal: createInternalClient(transport),
    get legacyPort() {
      if (legacyPort) return legacyPort;
      const { port1: product, port2: host } = new MessageChannel();
      legacyPort = product;
      const adapter = {
        receive(frame: Uint8Array) {
          host.postMessage(frame);
        },
        close() {
          host.close();
        },
      };
      legacy = adapter;
      const onMessage = (event: MessageEvent) => {
        if (legacy !== adapter || !current?.verified) return;
        const { data: frame } = event;
        if (!(frame instanceof Uint8Array)) return;
        const decoded = decodeWireMessage(frame);
        if (decoded.isErr() || decoded.value.requestId.startsWith("host:"))
          return;
        try {
          current.provider.postMessage(frame);
        } catch (error) {
          if (current) retire(current, error);
        }
      };
      try {
        void ready(open()).then(
          () => {
            if (legacy !== adapter) return;
            host.addEventListener("message", onMessage);
            host.start();
          },
          () => adapter.close(),
        );
      } catch {
        adapter.close();
      }
      return product;
    },
    subscribeConnectionStatus(callback) {
      listeners.add(callback);
      callback(status);
      return () => {
        listeners.delete(callback);
      };
    },
    dispose() {
      try {
        stop();
      } finally {
        transport.dispose();
      }
    },
  };
}
