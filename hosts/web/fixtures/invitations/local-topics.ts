import {
  readTopicRelayCall,
  type TopicRelayEvent,
} from "../../src/chat/draft/topic-relay.js";
import { digest, toHex } from "../../src/invitations/sealing.js";
import type { SharedStore } from "./shared-store.js";

/**
 * DEV ONLY. LOCAL SIMULATED TOPICS: the draft topic relay's host end
 * (`src/chat/draft/topic-relay.ts`) between tabs of this browser, in place of
 * the Statement Store, which test users cannot reach.
 *
 * A post is a product's opaque bytes kept in a store the tabs share, under
 * its topic, until it lapses, as a statement would be. A watcher gets what
 * is kept on its topic and every post that arrives after. The host never
 * reads a post.
 *
 * Simulated: topics are in the clear in a store any script of this origin
 * reads, nothing signs or limits a post, and posts lapse on a dev lifetime,
 * not a statement expiry.
 */

/** How long a post stays readable. A dev choice, longer than an invitation lives. */
export const TOPIC_POST_LIFETIME_MS = 7 * 24 * 60 * 60_000;

export const topicsPrefix = (network: string): string =>
  `truapi-web-signing-host:network:${network}:dev:topics:v1:`;

interface Kept {
  data: string;
  storedUntil: number;
}

function readKept(value: string | null): Kept | null {
  if (value === null) return null;
  try {
    const kept = JSON.parse(value) as Partial<Kept> | null;
    return typeof kept?.data === "string" &&
      typeof kept.storedUntil === "number"
      ? { data: kept.data, storedUntil: kept.storedUntil }
      : null;
  } catch {
    return null;
  }
}

export interface LocalTopicsOptions {
  network: string;
  store: SharedStore;
  now(): number;
  lifetimeMs?: number;
}

export class LocalTopics {
  private readonly prefix: string;

  constructor(private readonly options: LocalTopicsOptions) {
    this.prefix = topicsPrefix(options.network);
  }

  /** Serve one product's port until it closes or the returned stop. */
  serve(port: MessagePort): () => void {
    const watched = new Set<string>();
    const send = (event: TopicRelayEvent) => port.postMessage(event);
    const kept = (topic: string): string[] => {
      const now = this.options.now();
      return this.options.store
        .keys(`${this.prefix}${topic}:`)
        .map((key) => readKept(this.options.store.get(key)))
        .filter((post): post is Kept => post !== null && post.storedUntil > now)
        .map((post) => post.data);
    };
    const unlisten = this.options.store.onChange((key) => {
      if (key === null) {
        for (const topic of watched) {
          const data = kept(topic);
          if (data.length > 0) send({ kind: "data", topic, data });
        }
        return;
      }
      if (!key.startsWith(this.prefix)) return;
      const topic = key.slice(this.prefix.length).split(":")[0];
      if (!watched.has(topic)) return;
      const post = readKept(this.options.store.get(key));
      if (post !== null && post.storedUntil > this.options.now())
        send({ kind: "data", topic, data: [post.data] });
    });
    port.onmessage = (event: MessageEvent<unknown>) => {
      const call = readTopicRelayCall(event.data);
      if (call === null) return;
      switch (call.kind) {
        case "post":
          this.prune();
          this.options.store.set(
            `${this.prefix}${call.topic}:${toHex(digest("topic-post", call.data)).slice(2, 34)}`,
            JSON.stringify({
              data: call.data,
              storedUntil:
                this.options.now() +
                (this.options.lifetimeMs ?? TOPIC_POST_LIFETIME_MS),
            } satisfies Kept),
          );
          send({ kind: "result", id: call.id, ok: true });
          return;
        case "watch": {
          watched.add(call.topic);
          send({ kind: "result", id: call.id, ok: true });
          const data = kept(call.topic);
          if (data.length > 0) send({ kind: "data", topic: call.topic, data });
          return;
        }
        case "unwatch":
          watched.delete(call.topic);
          return;
      }
    };
    port.start();
    return () => {
      unlisten();
      watched.clear();
      port.close();
    };
  }

  /** Remove every post that lapsed, as a store forgets a statement. */
  private prune(): void {
    const now = this.options.now();
    for (const key of this.options.store.keys(this.prefix)) {
      const post = readKept(this.options.store.get(key));
      if (post === null || post.storedUntil <= now)
        this.options.store.remove(key);
    }
  }
}
