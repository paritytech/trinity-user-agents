import { bytesToHex } from "@parity/truapi/scale";
import { blake2b256 } from "../../bridges/primitives/crypto.js";
import type { ChainProvider, JsonRpcConnection } from "@parity/truapi-host";

/**
 * A passive view of the Statement Store traffic on People chain connections,
 * for the inspector's Statements view (`tools/statements/`).
 *
 * {@link observeStatements} wraps a chain provider. Each People chain
 * connection it opens hands on every request and every answer exactly as it
 * gets them, in the same order, and reads only `statement_*` traffic: the
 * requests whose method starts `statement_`, the answers to those requests,
 * and the notifications of the subscriptions they opened. Everything else on
 * the connection is passed by without being parsed. The sink runs after the
 * observer has taken what it needs and before the text is handed on; a sink
 * or a parse that throws is ignored, so the connection behaves the same with
 * the observer as without it.
 *
 * Events hold what crossed the connection: methods, ids, parameters and
 * results, statements included. The sink decides what it keeps.
 */

/** Who opened a connection: the core, or the host page's own features. */
export type TrafficSource = "core" | "host";

/** An error a JSON-RPC answer carried. */
export interface RpcErrorValue {
  code?: number;
  message?: string;
  data?: unknown;
}

/** Statement Store traffic on one connection, in the order it crossed it. */
export type StatementTraffic =
  /** The first `statement_*` message on a connection. Connections that carry none are never reported. */
  | { type: "connected"; connection: number; source: TrafficSource }
  | {
      type: "request";
      connection: number;
      /** The request id as JSON, so `1` and `"1"` stay apart. */
      id: string;
      method: string;
      params: unknown;
    }
  | {
      type: "response";
      connection: number;
      id: string;
      /** The method of the request it answers. */
      method: string;
      result?: unknown;
      error?: RpcErrorValue;
    }
  | {
      type: "notification";
      connection: number;
      /** The subscription id, as {@link subscriptionKey} names it. */
      subscription: string;
      method: string;
      result: unknown;
    }
  | {
      type: "closed";
      connection: number;
      /** `caller`: the opener closed it; `ended`: the answers ended; `failed`: reading them threw. */
      by: "caller" | "ended" | "failed";
      reason?: string;
    };

export type StatementTrafficSink = (event: StatementTraffic) => void;

const STATEMENT = '"statement_';
const SUBSCRIBE = "statement_subscribeStatement";
const UNSUBSCRIBE = "statement_unsubscribeStatement";
/** The most unanswered requests and open subscriptions one connection remembers; the oldest go first. */
export const OBSERVER_MEMORY = 256;
/** Every top-level-looking `"id": <value>` in a message, to skip parsing answers to other requests. */
const ID_TOKEN = /"id"\s*:\s*("(?:[^"\\]|\\.)*"|-?\d+)/g;

let nextConnection = 1;

/**
 * `chain`, with every connection to `peopleGenesis` observed for `source`.
 * Connections to other chains are handed back untouched.
 */
export function observeStatements(
  chain: ChainProvider,
  peopleGenesis: string,
  source: TrafficSource,
  sink: StatementTrafficSink,
): ChainProvider {
  const people = peopleGenesis.toLowerCase();
  return {
    async connect(genesisHash) {
      const inner = await chain.connect(genesisHash);
      return bytesToHex(genesisHash).toLowerCase() === people
        ? observedConnection(inner, source, sink)
        : inner;
    },
  };
}

/** `inner`, unchanged, with its Statement Store traffic reported to `sink`. */
export function observedConnection(
  inner: JsonRpcConnection,
  source: TrafficSource,
  sink: StatementTrafficSink,
): JsonRpcConnection {
  const connection = nextConnection++;
  /** Methods of the `statement_*` requests not answered yet, by id. */
  const pending = new Map<string, string>();
  /** Subscriptions the answers to `statement_subscribeStatement` opened. */
  const subscriptions = new Map<string, true>();
  let seen = false;
  let closed = false;

  const report = (event: StatementTraffic) => {
    try {
      if (!seen) {
        seen = true;
        sink({ type: "connected", connection, source });
      }
      sink(event);
    } catch {
      // The inspector never changes what the connection does.
    }
  };
  const close = (by: "caller" | "ended" | "failed", reason?: string) => {
    if (closed) return;
    closed = true;
    if (seen)
      try {
        sink({
          type: "closed",
          connection,
          by,
          ...(reason === undefined ? {} : { reason }),
        });
      } catch {
        // As above.
      }
  };

  const outgoing = (text: string) => {
    if (!text.includes(STATEMENT)) return;
    const message = parse(text);
    if (message === null || typeof message.method !== "string") return;
    if (!message.method.startsWith("statement_")) return;
    const id = message.id === undefined ? null : JSON.stringify(message.id);
    if (id !== null) remember(pending, id, message.method);
    if (message.method === UNSUBSCRIBE && Array.isArray(message.params))
      subscriptions.delete(subscriptionKey(JSON.stringify(message.params[0])));
    report({
      type: "request",
      connection,
      id: id ?? "null",
      method: message.method,
      params: message.params,
    });
  };

  const incoming = (text: string) => {
    const mentionsStatement = text.includes(STATEMENT);
    if (!mentionsStatement && !answersPending(text, pending)) return;
    const message = parse(text);
    if (message === null) return;
    if (message.id !== undefined && message.id !== null) {
      const id = JSON.stringify(message.id);
      const method = pending.get(id);
      if (method === undefined) return;
      pending.delete(id);
      if (
        method === SUBSCRIBE &&
        message.error === undefined &&
        message.result !== undefined
      )
        remember(
          subscriptions,
          subscriptionKey(JSON.stringify(message.result)),
          true,
        );
      report({
        type: "response",
        connection,
        id,
        method,
        ...(message.error === undefined
          ? { result: message.result }
          : { error: rpcError(message.error) }),
      });
      return;
    }
    const params = message.params as
      | { subscription?: unknown; result?: unknown }
      | undefined;
    if (
      params?.subscription === undefined ||
      typeof message.method !== "string"
    )
      return;
    const subscription = subscriptionKey(JSON.stringify(params.subscription));
    // A notification may arrive before the answer that names its subscription.
    if (
      !subscriptions.has(subscription) &&
      !message.method.startsWith("statement_")
    )
      return;
    report({
      type: "notification",
      connection,
      subscription,
      method: message.method,
      result: params.result,
    });
  };

  return {
    send(request) {
      try {
        outgoing(request);
      } catch {
        // As above.
      }
      inner.send(request);
    },
    async *responses() {
      let finished = false;
      try {
        for await (const text of inner.responses()) {
          try {
            incoming(text);
          } catch {
            // As above.
          }
          yield text;
        }
        finished = true;
      } catch (error) {
        close("failed", error instanceof Error ? error.message : String(error));
        throw error;
      } finally {
        if (finished) close("ended");
      }
    },
    close() {
      close("caller");
      inner.close();
    },
  };
}

/** The longest subscription id, as JSON, kept as it is. */
const SUBSCRIPTION_KEY_MAX = 64;

/**
 * A bounded name for a subscription id the node chose, `id` as JSON: the
 * JSON itself when short, else `#` and its BLAKE2b-256 hash. Two ids get the
 * same name only when they are the same id, and a long id costs no memory.
 */
export function subscriptionKey(id: string): string {
  return id.length <= SUBSCRIPTION_KEY_MAX
    ? id
    : `#${bytesToHex(blake2b256(new TextEncoder().encode(id))).slice(2)}`;
}

/** Keep `key` in `map`, forgetting the oldest past {@link OBSERVER_MEMORY}. */
function remember<V>(map: Map<string, V>, key: string, value: V): void {
  map.delete(key);
  map.set(key, value);
  if (map.size > OBSERVER_MEMORY) map.delete(map.keys().next().value as string);
}

interface RpcMessage {
  id?: unknown;
  method?: unknown;
  params?: unknown;
  result?: unknown;
  error?: unknown;
}

function parse(text: string): RpcMessage | null {
  const value: unknown = JSON.parse(text);
  return typeof value === "object" && value !== null && !Array.isArray(value)
    ? (value as RpcMessage)
    : null;
}

/** Whether `text` may be the answer to a pending request: it names one of their ids. */
function answersPending(text: string, pending: Map<string, string>): boolean {
  if (pending.size === 0) return false;
  for (const match of text.matchAll(ID_TOKEN))
    if (pending.has(match[1])) return true;
  return false;
}

function rpcError(value: unknown): RpcErrorValue {
  if (typeof value !== "object" || value === null)
    return { message: String(value) };
  const { code, message, data } = value as Record<string, unknown>;
  return {
    ...(typeof code === "number" ? { code } : {}),
    ...(typeof message === "string" ? { message } : {}),
    ...(data === undefined ? {} : { data }),
  };
}
