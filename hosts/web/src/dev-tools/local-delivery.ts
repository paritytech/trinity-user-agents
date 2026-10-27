import { hexBytes } from "../invitations/contract.js";
import type {
  DeliveryReceiver,
  HostDelivery,
  IdentityKeys,
} from "../invitations/ports.js";
import { digest, toHex } from "../invitations/sealing.js";
import type { SharedStore } from "./shared-store.js";

/**
 * DEV ONLY. LOCAL SIMULATED DELIVERY: a `HostDelivery` between tabs of this
 * browser, in place of a notification transport and the Statement Store,
 * which test users cannot reach.
 *
 * A post is sealed bytes kept in a store the tabs share, with a time it
 * lapses at, as a statement would be. A request is kept under a route
 * derived from the recipient's agreement key; a reply under its return
 * topic. A host posts each of its posts again on a cadence until it is
 * withdrawn or its `until` passes. A host reads what is posted to its route,
 * and to the topics it watches, when it starts and as posts arrive.
 *
 * Simulated: the route is a stable label anyone can link, the store is
 * readable by any script of this origin, no notification account signs a
 * post and nothing limits who posts. What is posted is sealed and
 * authenticated for real (`src/invitations/binding.ts`). The cadence and
 * lifetime below are dev choices, not the design's.
 */

/** How often a post is posted again. A dev choice. */
export const REPOST_INTERVAL_MS = 60_000;
/** How long one post stays readable. A dev choice. */
export const POST_LIFETIME_MS = 5 * 60_000;

interface Post {
  sealed: string;
  storedUntil: number;
}

/** The label requests for the holder of `agreementPublic` are kept under. */
export const routeOf = (agreementPublic: string): string =>
  toHex(
    digest("truapi-web-host/dev-fixture/mailbox-route/1", agreementPublic),
  ).slice(2, 34);

export const deliveryPrefix = (network: string): string =>
  `truapi-web-signing-host:network:${network}:dev:delivery:v2:`;

function readPost(value: string | null): Post | null {
  if (value === null) return null;
  try {
    const post = JSON.parse(value) as Partial<Post>;
    return hexBytes(post.sealed) !== null &&
      typeof post.storedUntil === "number"
      ? { sealed: post.sealed as string, storedUntil: post.storedUntil }
      : null;
  } catch {
    return null;
  }
}

export interface LocalDeliveryOptions {
  /** This host's agreement public key, as `0x` hex. */
  self: string;
  network: string;
  store: SharedStore;
  now(): number;
  /** Run `task` after `delayMs`. Returns a function that cancels it. */
  schedule(task: () => void, delayMs: number): () => void;
  log(line: string): void;
  repostIntervalMs?: number;
  postLifetimeMs?: number;
}

export class LocalDelivery implements HostDelivery {
  /** What this host posts and posts again, by id: the store key's stem and the bytes. */
  private readonly posts = new Map<
    string,
    { stem: string; sealed: string; until: number }
  >();
  private readonly topics = new Set<string>();
  private receiver: DeliveryReceiver | null = null;
  private unlisten = () => {};
  private cancelRepost = () => {};
  private closed = false;
  private readonly prefix: string;
  private readonly inbox: string;

  constructor(private readonly options: LocalDeliveryOptions) {
    this.prefix = deliveryPrefix(options.network);
    this.inbox = `${this.prefix}request:${routeOf(options.self)}:`;
  }

  listen(receiver: DeliveryReceiver): void {
    if (this.closed) return;
    this.receiver = receiver;
    this.unlisten = this.options.store.onChange((key) => {
      if (key === null) this.readAll();
      else this.read(key);
    });
    this.readAll();
    this.scheduleRepost();
  }

  postRequest(to: IdentityKeys, id: string, sealed: string, until: number) {
    this.add(id, `request:${routeOf(to.agreementPublic)}`, sealed, until);
  }

  postReply(topic: string, id: string, sealed: string, until: number) {
    this.add(id, `reply:${topic}`, sealed, until);
  }

  watch(topic: string): void {
    if (this.closed || this.topics.has(topic)) return;
    this.topics.add(topic);
    for (const key of this.options.store.keys(`${this.prefix}reply:${topic}:`))
      this.read(key);
  }

  unwatch(topic: string): void {
    this.topics.delete(topic);
  }

  withdraw(id: string): void {
    this.posts.delete(id);
  }

  close(): void {
    if (this.closed) return;
    this.closed = true;
    this.posts.clear();
    this.topics.clear();
    this.unlisten();
    this.cancelRepost();
    this.receiver = null;
  }

  private add(id: string, stem: string, sealed: string, until: number): void {
    if (this.closed) return;
    this.posts.set(id, { stem, sealed, until });
    this.write(id);
  }

  private write(id: string): void {
    const entry = this.posts.get(id);
    if (entry === undefined) return;
    const now = this.options.now();
    if (now >= entry.until) {
      this.posts.delete(id);
      return;
    }
    const lifetime = this.options.postLifetimeMs ?? POST_LIFETIME_MS;
    const name = toHex(digest("post", entry.sealed)).slice(2, 34);
    this.options.store.set(
      `${this.prefix}${entry.stem}:${name}`,
      JSON.stringify({
        sealed: entry.sealed,
        storedUntil: Math.min(now + lifetime, entry.until),
      } satisfies Post),
    );
  }

  private scheduleRepost(): void {
    this.cancelRepost = this.options.schedule(() => {
      if (this.closed) return;
      for (const id of [...this.posts.keys()]) this.write(id);
      this.prune();
      this.scheduleRepost();
    }, this.options.repostIntervalMs ?? REPOST_INTERVAL_MS);
  }

  /** Remove every post that lapsed, as a store forgets a statement. */
  private prune(): void {
    const now = this.options.now();
    for (const key of this.options.store.keys(this.prefix)) {
      const post = readPost(this.options.store.get(key));
      if (post === null || post.storedUntil <= now)
        this.options.store.remove(key);
    }
  }

  private readAll(): void {
    for (const key of this.options.store.keys(this.inbox)) this.read(key);
    for (const topic of this.topics)
      for (const key of this.options.store.keys(
        `${this.prefix}reply:${topic}:`,
      ))
        this.read(key);
  }

  private read(key: string): void {
    const receiver = this.receiver;
    if (receiver === null || !key.startsWith(this.prefix)) return;
    const [kind, label] = key.slice(this.prefix.length).split(":");
    const isRequest = key.startsWith(this.inbox);
    if (!isRequest && !(kind === "reply" && this.topics.has(label!))) return;
    const post = readPost(this.options.store.get(key));
    if (post === null || post.storedUntil <= this.options.now()) return;
    if (isRequest) receiver.request(post.sealed);
    else receiver.reply(label!, post.sealed);
  }
}
