import { hexBytes } from "../../src/invitations/contract.js";
import type {
  DeliveryReceiver,
  HostDelivery,
  IdentityKeys,
} from "../../src/invitations/ports.js";
import {
  digest,
  toHex,
  type RandomBytes,
} from "../../src/invitations/sealing.js";
import type { SharedStore } from "./shared-store.js";

/**
 * DEV ONLY. LOCAL SIMULATED DELIVERY: a `HostDelivery` between tabs of this
 * browser, in place of a notification transport and the Statement Store,
 * which test users cannot reach.
 *
 * A post is sealed bytes kept in a store the tabs share, with a time it
 * lapses at, as a statement would be. Every host has an inbox, under a label
 * derived from its agreement key; requests go there. Where a reply goes is
 * this delivery's choice, set by `returnRoutes`:
 *
 * - `"inbox"` (the default): a return route is the sender's inbox and a
 *   random tag. A reply lands in the sender's inbox with the tag, which
 *   names the request it answers. A recipient's host posts a reply only to
 *   the inbox of the person who sent the request.
 * - `"topic"`: a return route is a random topic of its own, which the sender
 *   watches.
 *
 * A host posts each of its posts again on a cadence until it is withdrawn or
 * its `until` passes, and reads what is posted to it when it starts and as
 * posts arrive.
 *
 * Simulated: inbox labels are stable and anyone can link them, a reply's tag
 * is in the clear, the store is readable by any script of this origin, no
 * notification account signs a post and nothing limits who posts. What is
 * posted is sealed and authenticated for real (`src/invitations/binding.ts`).
 * The cadence and lifetime below are dev choices, not the design's.
 */

/** How often a post is posted again. A dev choice. */
export const REPOST_INTERVAL_MS = 60_000;
/** How long one post stays readable. A dev choice. */
export const POST_LIFETIME_MS = 5 * 60_000;

/** What a return route is, in this delivery. */
export type ReturnRoutes = "inbox" | "topic";

interface Post {
  sealed: string;
  storedUntil: number;
  /** For a reply in an inbox: the return route it answers. */
  route?: string;
}

/** The label of the inbox of the holder of `agreementPublic`. */
export const inboxOf = (agreementPublic: string): string =>
  toHex(digest("truapi-web-host/dev-fixture/inbox/1", agreementPublic)).slice(
    2,
    34,
  );

export const deliveryPrefix = (network: string): string =>
  `truapi-web-signing-host:network:${network}:dev:delivery:v3:`;

const INBOX_ROUTE = /^inbox:([0-9a-f]{32}):0x[0-9a-f]{32}$/;
const TOPIC_ROUTE = /^topic:(0x[0-9a-f]{64})$/;

function readPost(value: string | null): Post | null {
  if (value === null) return null;
  try {
    const post = JSON.parse(value) as Partial<Post> | null;
    if (
      hexBytes(post?.sealed) === null ||
      typeof post?.storedUntil !== "number" ||
      (post.route !== undefined && typeof post.route !== "string")
    )
      return null;
    return {
      sealed: post.sealed as string,
      storedUntil: post.storedUntil,
      ...(post.route === undefined ? {} : { route: post.route }),
    };
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
  random: RandomBytes;
  log(line: string): void;
  returnRoutes?: ReturnRoutes;
  repostIntervalMs?: number;
  postLifetimeMs?: number;
}

export class LocalDelivery implements HostDelivery {
  /** What this host posts and posts again, by id. */
  private readonly posts = new Map<
    string,
    { stem: string; post: Omit<Post, "storedUntil">; until: number }
  >();
  /** The return routes of this host's that it reads replies on. */
  private readonly routes = new Set<string>();
  private receiver: DeliveryReceiver | null = null;
  private unlisten = () => {};
  private cancelRepost = () => {};
  private closed = false;
  private readonly prefix: string;
  private readonly inbox: string;

  constructor(private readonly options: LocalDeliveryOptions) {
    this.prefix = deliveryPrefix(options.network);
    this.inbox = inboxOf(options.self);
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

  returnRoute(): string {
    const { random } = this.options;
    return (this.options.returnRoutes ?? "inbox") === "inbox"
      ? `inbox:${this.inbox}:${toHex(random(16))}`
      : `topic:${toHex(random(32))}`;
  }

  isReturnRoute(route: string, requester: IdentityKeys): boolean {
    const inbox = INBOX_ROUTE.exec(route);
    if (inbox !== null) return inbox[1] === inboxOf(requester.agreementPublic);
    return TOPIC_ROUTE.test(route);
  }

  postRequest(to: IdentityKeys, id: string, sealed: string, until: number) {
    this.add(id, `inbox:${inboxOf(to.agreementPublic)}`, { sealed }, until);
  }

  postReply(route: string, id: string, sealed: string, until: number) {
    const inbox = INBOX_ROUTE.exec(route);
    const topic = TOPIC_ROUTE.exec(route);
    if (inbox !== null)
      this.add(id, `inbox:${inbox[1]}`, { sealed, route }, until);
    else if (topic !== null)
      this.add(id, `topic:${topic[1]}`, { sealed }, until);
    else this.options.log("Not posted: not a return route of this delivery.");
  }

  watch(route: string): void {
    if (this.closed || this.routes.has(route)) return;
    this.routes.add(route);
    const topic = TOPIC_ROUTE.exec(route);
    const stem = topic === null ? `inbox:${this.inbox}` : `topic:${topic[1]}`;
    for (const key of this.options.store.keys(`${this.prefix}${stem}:`))
      this.read(key);
  }

  unwatch(route: string): void {
    this.routes.delete(route);
  }

  withdraw(id: string): void {
    this.posts.delete(id);
  }

  close(): void {
    if (this.closed) return;
    this.closed = true;
    this.posts.clear();
    this.routes.clear();
    this.unlisten();
    this.cancelRepost();
    this.receiver = null;
  }

  private add(
    id: string,
    stem: string,
    post: Omit<Post, "storedUntil">,
    until: number,
  ): void {
    if (this.closed) return;
    this.posts.set(id, { stem, post, until });
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
    const name = toHex(digest("post", entry.post.sealed)).slice(2, 34);
    this.options.store.set(
      `${this.prefix}${entry.stem}:${name}`,
      JSON.stringify({
        ...entry.post,
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
    for (const key of this.options.store.keys(
      `${this.prefix}inbox:${this.inbox}:`,
    ))
      this.read(key);
    for (const route of this.routes) {
      const topic = TOPIC_ROUTE.exec(route);
      if (topic !== null)
        for (const key of this.options.store.keys(
          `${this.prefix}topic:${topic[1]}:`,
        ))
          this.read(key);
    }
  }

  private read(key: string): void {
    const receiver = this.receiver;
    if (receiver === null || !key.startsWith(this.prefix)) return;
    const [kind, label] = key.slice(this.prefix.length).split(":");
    const mine = kind === "inbox" && label === this.inbox;
    const route = kind === "topic" ? `topic:${label}` : null;
    if (!mine && (route === null || !this.routes.has(route))) return;
    const post = readPost(this.options.store.get(key));
    if (post === null || post.storedUntil <= this.options.now()) return;
    if (route !== null) receiver.reply(route, post.sealed);
    else if (post.route === undefined) receiver.request(post.sealed);
    else if (this.routes.has(post.route))
      receiver.reply(post.route, post.sealed);
  }
}
