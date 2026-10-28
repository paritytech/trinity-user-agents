import type { JsonRpcConnection } from "@parity/truapi-host";

/**
 * How many `chainHead_v1_follow` subscriptions one light-client connection
 * holds at once. The JSON-RPC specification lets a server stop at two per
 * client, and the embedded light client does: a third is refused with
 * "too many active follow subscriptions".
 */
export const FOLLOWS_PER_CONNECTION = 2;

/** A light-client connection and the follows it holds. */
interface Lane {
  /** Settles with the connection, or with `null` when it could not be opened. */
  ready: Promise<JsonRpcConnection | null>;
  connection: JsonRpcConnection | null;
  /** Follows the server accepted and has not ended. */
  follows: number;
  /** Follows asked for and not answered yet. */
  asking: number;
}

interface Request {
  id?: unknown;
  method?: unknown;
  params?: unknown;
}

interface Message {
  id?: unknown;
  result?: unknown;
  method?: unknown;
  params?: { subscription?: unknown; result?: { event?: unknown } };
}

const parse = <T>(text: string): T | null => {
  try {
    const value: unknown = JSON.parse(text);
    return typeof value === "object" && value !== null ? (value as T) : null;
  } catch {
    return null;
  }
};

/**
 * One JSON-RPC connection for the core that spreads its `chainHead_v1`
 * follows over as many light-client connections as they need.
 *
 * The core keeps one connection per chain and opens a follow on it for each
 * product that follows the chain, and for some of its own reads. Two follows
 * fill a light-client connection, so a third product would be refused. Each
 * follow is opened on a connection with a free slot, a new one when none has,
 * and every `chainHead_v1_*` call that names the follow's subscription goes to
 * the connection that holds it. Everything else goes to the first connection.
 * Connections share the light client's sync, so a new one answers at once.
 *
 * Request ids come from one caller, so the merged responses keep them unique.
 */
export function spreadFollows(
  first: JsonRpcConnection,
  open: () => Promise<JsonRpcConnection>,
): JsonRpcConnection {
  const lanes: Lane[] = [
    { ready: Promise.resolve(first), connection: first, follows: 0, asking: 0 },
  ];
  const subscriptions = new Map<string, Lane>();
  const followRequests = new Map<string, Lane>();
  const queued: string[] = [];
  let wake: (() => void) | null = null;
  let closed = false;

  const emit = (response: string): void => {
    if (closed) return;
    queued.push(response);
    wake?.();
  };

  const release = (lane: Lane): void => {
    lane.follows = Math.max(0, lane.follows - 1);
  };

  const observe = (lane: Lane, response: string): void => {
    const message = parse<Message>(response);
    if (message === null) return;
    if (message.id !== undefined) {
      const key = JSON.stringify(message.id);
      if (followRequests.get(key) === lane) {
        followRequests.delete(key);
        lane.asking -= 1;
        if (typeof message.result === "string") {
          lane.follows += 1;
          subscriptions.set(message.result, lane);
        }
      }
      return;
    }
    const subscription = message.params?.subscription;
    if (
      message.method === "chainHead_v1_followEvent" &&
      message.params?.result?.event === "stop" &&
      typeof subscription === "string" &&
      subscriptions.get(subscription) === lane
    ) {
      subscriptions.delete(subscription);
      release(lane);
    }
  };

  const pump = async (lane: Lane): Promise<void> => {
    const connection = await lane.ready;
    if (connection === null) return;
    for await (const response of connection.responses()) {
      observe(lane, response);
      emit(response);
    }
  };

  const openLane = (): Lane => {
    const lane: Lane = {
      ready: Promise.resolve(null),
      connection: null,
      follows: 0,
      asking: 0,
    };
    lane.ready = open().then(
      (connection) => {
        if (closed) {
          connection.close();
          return null;
        }
        lane.connection = connection;
        return connection;
      },
      () => null,
    );
    lanes.push(lane);
    void pump(lane);
    return lane;
  };

  /** A lane with a free follow slot, opening one when every lane is full. */
  const laneForFollow = (): Lane =>
    lanes.find((lane) => lane.follows + lane.asking < FOLLOWS_PER_CONNECTION) ??
    openLane();

  const sendOn = (lane: Lane, request: string, id: unknown): void => {
    if (lane.connection !== null) {
      lane.connection.send(request);
      return;
    }
    void lane.ready.then((connection) => {
      if (connection !== null) connection.send(request);
      else if (id !== undefined)
        emit(
          JSON.stringify({
            jsonrpc: "2.0",
            id,
            error: {
              code: -32603,
              message: "the light client could not open another connection",
            },
          }),
        );
    });
  };

  void pump(lanes[0]!);

  return {
    send(request) {
      if (closed) return;
      const parsed = parse<Request>(request);
      const method = typeof parsed?.method === "string" ? parsed.method : "";
      if (method === "chainHead_v1_follow") {
        const lane = laneForFollow();
        lane.asking += 1;
        if (parsed?.id !== undefined)
          followRequests.set(JSON.stringify(parsed.id), lane);
        sendOn(lane, request, parsed?.id);
        return;
      }
      const subscription = Array.isArray(parsed?.params)
        ? parsed.params[0]
        : undefined;
      const lane =
        method.startsWith("chainHead_v1_") && typeof subscription === "string"
          ? subscriptions.get(subscription)
          : undefined;
      if (lane === undefined) {
        first.send(request);
        return;
      }
      if (method === "chainHead_v1_unfollow") {
        subscriptions.delete(subscription as string);
        release(lane);
      }
      sendOn(lane, request, parsed?.id);
    },
    async *responses() {
      while (!closed) {
        const next = queued.shift();
        if (next !== undefined) {
          yield next;
          continue;
        }
        await new Promise<void>((resolve) => (wake = resolve));
        wake = null;
      }
    },
    close() {
      if (closed) return;
      closed = true;
      wake?.();
      for (const lane of lanes)
        void lane.ready.then((connection) => connection?.close());
    },
  };
}
