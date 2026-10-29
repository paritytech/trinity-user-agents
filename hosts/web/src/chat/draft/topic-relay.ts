/**
 * DRAFT, NOT TRUAPI. A stand-in for the Statement Store topics a chat product
 * posts on and watches, for a host whose session reaches no chain, such as a
 * test user of the invitation demo (`tools/demos/dev-tools.ts`).
 *
 * The product frame posts {@link TopicRelayConnect} to its parent with one
 * `MessagePort`. Over it the product posts opaque bytes on a topic, and
 * watches a topic for what is kept on it and what arrives. The host keeps a
 * post for a while, as a store keeps a statement, and never reads it.
 *
 * It stands in for TrUAPI's `statementStore`, not for a design: there is no
 * signer, channel, proof or allowance here, so it says nothing about which
 * account signs a product's chat statements. Topics are 32 bytes and data
 * at most {@link TOPIC_DATA_MAX_BYTES}, both as `0x` hex.
 */

/** `type` of the window message a product frame sends to hand the host its port. */
export const TOPIC_RELAY_CONNECT = "truapi-topic-relay-draft";
/** Version of the messages on that port. */
export const TOPIC_RELAY_DRAFT = "truapi-topic-relay-draft/1";

/** The largest post, as a statement's data is bounded. */
export const TOPIC_DATA_MAX_BYTES = 16 * 1024;

export interface TopicRelayConnect {
  type: typeof TOPIC_RELAY_CONNECT;
  protocol: typeof TOPIC_RELAY_DRAFT;
}

/** Product to host. `post` and `watch` are answered with one `result` of their `id`. */
export type TopicRelayCall =
  | { kind: "post"; id: number; topic: string; data: string }
  | { kind: "watch"; id: number; topic: string }
  | { kind: "unwatch"; topic: string };

/** Host to product. */
export type TopicRelayEvent =
  | { kind: "result"; id: number; ok: true }
  | { kind: "result"; id: number; ok: false; reason: string }
  /** What is kept on a watched topic, or arrived on it. A post may come more than once. */
  | { kind: "data"; topic: string; data: string[] };

const TOPIC = /^0x[0-9a-f]{64}$/;
const DATA = /^0x(?:[0-9a-f]{2})*$/;

/** Whether `data` is a hello with one port. */
export function isTopicRelayHello(
  data: unknown,
  ports: readonly MessagePort[],
): boolean {
  const hello = data as Partial<TopicRelayConnect> | null;
  return (
    hello?.type === TOPIC_RELAY_CONNECT &&
    hello.protocol === TOPIC_RELAY_DRAFT &&
    ports.length === 1
  );
}

/** A product's call as the host reads it, or null when it is not one. */
export function readTopicRelayCall(value: unknown): TopicRelayCall | null {
  const call = value as {
    kind?: unknown;
    id?: unknown;
    topic?: unknown;
    data?: unknown;
  } | null;
  if (typeof call?.topic !== "string" || !TOPIC.test(call.topic)) return null;
  if (call.kind === "unwatch") return { kind: "unwatch", topic: call.topic };
  if (typeof call.id !== "number" || !Number.isSafeInteger(call.id))
    return null;
  if (call.kind === "watch")
    return { kind: "watch", id: call.id, topic: call.topic };
  if (
    call.kind === "post" &&
    typeof call.data === "string" &&
    DATA.test(call.data) &&
    (call.data.length - 2) / 2 <= TOPIC_DATA_MAX_BYTES
  )
    return { kind: "post", id: call.id, topic: call.topic, data: call.data };
  return null;
}

/** The product's end of the relay. */
export interface TopicRelay {
  post(topic: string, data: string): Promise<void>;
  /** Hand `onData` what is kept on `topic` and what arrives, until the returned stop. */
  watch(topic: string, onData: (data: string[]) => void): Promise<() => void>;
  close(): void;
}

/** The relay over `port`, from the product's side. */
export function topicRelayOver(port: MessagePort): TopicRelay {
  let nextId = 1;
  const waiting = new Map<
    number,
    { resolve(): void; reject(error: Error): void }
  >();
  const watchers = new Map<string, Set<(data: string[]) => void>>();
  port.onmessage = (event: MessageEvent<TopicRelayEvent>) => {
    const message = event.data;
    if (message?.kind === "result") {
      const call = waiting.get(message.id);
      waiting.delete(message.id);
      if (message.ok) call?.resolve();
      else call?.reject(new Error(message.reason));
    } else if (message?.kind === "data")
      for (const listener of watchers.get(message.topic) ?? [])
        listener(message.data);
  };
  port.start();
  const call = (
    fields:
      | { kind: "post"; topic: string; data: string }
      | { kind: "watch"; topic: string },
  ): Promise<void> => {
    const id = nextId++;
    return new Promise((resolve, reject) => {
      waiting.set(id, { resolve, reject });
      port.postMessage({ ...fields, id });
    });
  };
  return {
    post: (topic, data) => call({ kind: "post", topic, data }),
    async watch(topic, onData) {
      let listeners = watchers.get(topic);
      const first = listeners === undefined;
      if (listeners === undefined) {
        listeners = new Set();
        watchers.set(topic, listeners);
      }
      listeners.add(onData);
      if (first) await call({ kind: "watch", topic });
      return () => {
        const current = watchers.get(topic);
        current?.delete(onData);
        if (current?.size === 0) {
          watchers.delete(topic);
          port.postMessage({
            kind: "unwatch",
            topic,
          } satisfies TopicRelayCall);
        }
      };
    },
    close() {
      port.close();
      for (const pending of waiting.values())
        pending.reject(new Error("The topic relay is closed."));
      waiting.clear();
      watchers.clear();
    },
  };
}

/** Hand the host this frame's end of the relay, and connect over it. */
export function connectTopicRelay(
  parent: Window = window.parent,
  hostOrigin: string = location.origin,
): TopicRelay {
  const channel = new MessageChannel();
  parent.postMessage(
    {
      type: TOPIC_RELAY_CONNECT,
      protocol: TOPIC_RELAY_DRAFT,
    } satisfies TopicRelayConnect,
    hostOrigin,
    [channel.port2],
  );
  return topicRelayOver(channel.port1);
}
