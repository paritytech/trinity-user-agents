import type { PeerKeys } from "../invitations/binding.js";
import { hexBytes } from "../invitations/contract.js";
import type { InvitationDelivery } from "../invitations/ports.js";
import { digest, toHex } from "../invitations/sealing.js";
import type { SharedStore } from "./shared-store.js";

/**
 * DEV ONLY. LOCAL SIMULATED DELIVERY: an `InvitationDelivery` between tabs of
 * this browser, in place of a Contacts delivery that does not exist yet.
 *
 * A post is a sealed invitation kept in a store the tabs share, under a route
 * derived from the recipient's inbox key, with a time it lapses at, as a
 * statement would. The sender's host posts it again on a cadence until it is
 * withdrawn or its `until` passes. A recipient's host reads what is posted to
 * its route when it starts and as posts arrive. It writes nothing: no hello,
 * receipt, answer or decline, so the sender learns nothing of the recipient.
 *
 * Simulated: the route is a stable label anyone can link, the store is
 * readable by any script of this origin, and nothing limits who posts. What
 * is posted is sealed and signed for real (`src/invitations/binding.ts`).
 * The cadence and lifetime below are dev choices, not the design's.
 */

/** How often a waiting invitation is posted again. A dev choice. */
export const REPOST_INTERVAL_MS = 60_000;
/** How long one post stays readable. A dev choice. */
export const POST_LIFETIME_MS = 5 * 60_000;

interface Post {
  sealed: string;
  storedUntil: number;
}

/** The label posts for the holder of `inboxPublic` are kept under. */
export const routeOf = (inboxPublic: string): string =>
  toHex(
    digest("truapi-web-host/dev-fixture/mailbox-route/0", inboxPublic),
  ).slice(2, 34);

export const mailboxPrefix = (network: string): string =>
  `truapi-web-signing-host:network:${network}:dev:mailbox:`;

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
  /** This host's inbox public key, as `0x` hex. */
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

export class LocalDelivery implements InvitationDelivery {
  /** What this host posts and posts again, by id. */
  private readonly posts = new Map<
    string,
    { route: string; sealed: string; until: number }
  >();
  private receive: ((sealed: string) => void) | null = null;
  private unlisten = () => {};
  private cancelRepost = () => {};
  private closed = false;
  private readonly prefix: string;
  private readonly inbox: string;

  constructor(private readonly options: LocalDeliveryOptions) {
    this.prefix = mailboxPrefix(options.network);
    this.inbox = `${this.prefix}${routeOf(options.self)}:`;
  }

  listen(receive: (sealed: string) => void): void {
    if (this.closed) return;
    this.receive = receive;
    this.unlisten = this.options.store.onChange((key) => {
      if (key === null) this.readInbox();
      else if (key.startsWith(this.inbox)) this.deliver(key);
    });
    this.readInbox();
    this.scheduleRepost();
  }

  post(to: PeerKeys, id: string, sealed: string, until: number): void {
    if (this.closed) return;
    this.posts.set(id, { route: routeOf(to.inboxPublic), sealed, until });
    this.write(id);
  }

  withdraw(id: string): void {
    this.posts.delete(id);
  }

  close(): void {
    if (this.closed) return;
    this.closed = true;
    this.posts.clear();
    this.unlisten();
    this.cancelRepost();
    this.receive = null;
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
    const key = `${this.prefix}${entry.route}:${toHex(digest("post", entry.sealed)).slice(2, 34)}`;
    this.options.store.set(
      key,
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

  private readInbox(): void {
    for (const key of this.options.store.keys(this.inbox)) this.deliver(key);
  }

  private deliver(key: string): void {
    const post = readPost(this.options.store.get(key));
    if (post === null || post.storedUntil <= this.options.now()) return;
    this.receive?.(post.sealed);
  }
}
