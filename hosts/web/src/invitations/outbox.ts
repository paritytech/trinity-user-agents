import type { RequestContext } from "./binding.js";
import type {
  ContactHandle,
  OutgoingStatus,
  RequestHandle,
} from "../../bridges/host-requests/contract.js";

/**
 * The requests a user's product sent, as the sender's host keeps them. Host
 * only: the reply key's secret and the person never reach the product.
 *
 * The reply key's secret is kept, in this host's storage, from before the
 * request is first posted until the request is closed, so a reply opens after
 * a restart. Closing it drops the secret from the list, and a closed request
 * read back with one has it dropped too; then no later reply opens. Dropping
 * it from storage is not a secure erase of what the browser wrote before.
 *
 * Every tab of the user writes the whole list. Before each change a tab takes
 * in what the others wrote since it read it: their new requests, and their
 * closes, which are final. A tab that writes before another tab's write
 * reaches its storage can still undo that write; writes are not atomic
 * across tabs.
 *
 * A request is pending until the product completes it or it is cancelled;
 * the first of these is final. It has no deadline. While the delivery cannot
 * keep it posted it is paused, and still pending. Nothing from the
 * recipient's host closes it: there is no receipt and no decline. Authenticated replies are kept, so a product that restarts gets
 * them again, and `complete` is taken only once one arrived.
 */

export type OutgoingState =
  | { tag: "waiting" }
  | { tag: "completed"; at: number }
  | { tag: "cancelled"; at: number };

/** At most this many distinct replies are kept per request. */
export const REPLIES_MAX = 4;

export interface OutgoingRequest {
  handle: RequestHandle;
  /** The person the user chose, by id. Their identity is in `context.to`. */
  to: string;
  contact: ContactHandle;
  context: RequestContext;
  payload: string;
  /** The digest of the exact request. */
  digest: string;
  /** The sealed request, posted again unchanged. */
  sealed: string;
  /**
   * The reply key's 32-byte secret, `0x` hex: an X-Wing seed or an X25519
   * scalar, as the reply key's length says. Null once the request is closed.
   * Never leaves the host.
   */
  replySecret: string | null;
  /** Authenticated reply payloads, in the order they arrived. */
  replies: { payload: string; at: number }[];
  state: OutgoingState;
  /** Why the delivery cannot keep it posted now, or null. Host only. */
  paused: string | null;
  /** The raw index of the product's account for the conversation this request set up, once it named one. */
  chatIndex?: string;
}

export function outgoingStatus(item: OutgoingRequest): OutgoingStatus {
  if (item.state.tag !== "waiting") return item.state.tag;
  return item.paused === null ? "waiting" : "paused";
}

/** Whether `item` is still pending: waiting or paused. */
export const isPending = (item: OutgoingRequest): boolean =>
  item.state.tag === "waiting";

function isOutgoing(value: unknown): value is OutgoingRequest {
  if (typeof value !== "object" || value === null) return false;
  const item = value as Partial<OutgoingRequest>;
  const state = item.state as Partial<OutgoingState> | undefined;
  return (
    typeof item.handle === "string" &&
    typeof item.to === "string" &&
    typeof item.contact?.bytes === "string" &&
    typeof item.context?.createdAt === "number" &&
    (item.paused === null || typeof item.paused === "string") &&
    typeof item.payload === "string" &&
    typeof item.digest === "string" &&
    typeof item.sealed === "string" &&
    (item.replySecret === null || typeof item.replySecret === "string") &&
    Array.isArray(item.replies) &&
    (state?.tag === "waiting" ||
      state?.tag === "completed" ||
      state?.tag === "cancelled")
  );
}

export class Outbox {
  private items: OutgoingRequest[];

  constructor(
    private readonly storage: Storage,
    private readonly key: string,
    private readonly now: () => number,
  ) {
    this.items = this.load();
  }

  private load(): OutgoingRequest[] {
    let items: OutgoingRequest[];
    try {
      const saved: unknown = JSON.parse(this.storage.getItem(this.key) ?? "[]");
      items = Array.isArray(saved) ? saved.filter(isOutgoing) : [];
    } catch {
      return [];
    }
    const closedWithSecret = items.filter(
      (item) => !isPending(item) && item.replySecret !== null,
    );
    for (const item of closedWithSecret) item.replySecret = null;
    if (closedWithSecret.length > 0) this.write(items);
    return items;
  }

  private save(): void {
    this.write(this.items);
  }

  private write(items: readonly OutgoingRequest[]): void {
    if (items.length === 0) this.storage.removeItem(this.key);
    else this.storage.setItem(this.key, JSON.stringify(items));
  }

  /** Read the list again, after another document of this origin wrote it. */
  reload(): void {
    this.items = this.load();
  }

  /** Oldest first. */
  list(): readonly OutgoingRequest[] {
    return this.items;
  }

  get(handle: RequestHandle): OutgoingRequest | undefined {
    return this.items.find((item) => item.handle === handle);
  }

  /** Every sent request whose replies come on `route`, newest first. */
  withRoute(route: string): OutgoingRequest[] {
    return this.items
      .filter((item) => item.context.returnRoute === route)
      .reverse();
  }

  status(item: OutgoingRequest): OutgoingStatus {
    return outgoingStatus(item);
  }

  /**
   * Take in what other tabs wrote since this list was read: their new
   * requests, and their closes. A request kept here stays the same object, so
   * whoever holds it sees the close.
   */
  private merge(): void {
    for (const saved of this.load()) {
      const item = this.get(saved.handle);
      if (item === undefined) this.items.push(saved);
      else if (isPending(item) && !isPending(saved)) {
        item.state = saved.state;
        item.paused = null;
        item.replySecret = null;
      }
    }
  }

  add(item: OutgoingRequest): void {
    this.merge();
    this.items.push(structuredClone(item));
    this.save();
  }

  /** Requests still pending, paused or not. */
  pending(): OutgoingRequest[] {
    return this.items.filter(isPending);
  }

  /** Record whether the delivery can keep a pending request posted. True when that changed. */
  setPaused(handle: RequestHandle, problem: string | null): boolean {
    this.merge();
    const item = this.get(handle);
    if (item === undefined || !isPending(item) || item.paused === problem)
      return false;
    item.paused = problem;
    this.save();
    return true;
  }

  /** Keep an authenticated reply. False when the request is not pending, it is a repeat, or there is no room. */
  addReply(handle: RequestHandle, payload: string): boolean {
    this.merge();
    const item = this.get(handle);
    if (
      item === undefined ||
      !isPending(item) ||
      item.replies.length >= REPLIES_MAX ||
      item.replies.some((reply) => reply.payload === payload)
    )
      return false;
    item.replies.push({ payload, at: this.now() });
    this.save();
    return true;
  }

  /** Keep the conversation account index the product named for `handle`. */
  setChatIndex(handle: RequestHandle, index: string): void {
    this.merge();
    const item = this.get(handle);
    if (item === undefined || item.chatIndex === index) return;
    item.chatIndex = index;
    this.save();
  }

  /** Close a pending request. False when it is no longer pending. */
  close(
    handle: RequestHandle,
    state: Exclude<OutgoingState, { tag: "waiting" }>,
  ) {
    this.merge();
    const item = this.get(handle);
    if (item === undefined || !isPending(item)) return false;
    item.state = state;
    item.paused = null;
    item.replySecret = null;
    this.save();
    return true;
  }
}
