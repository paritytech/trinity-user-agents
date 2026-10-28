import type { RequestContext } from "./binding.js";
import type {
  ContactHandle,
  OutgoingStatus,
  RequestHandle,
} from "./contract.js";

/**
 * The requests a user's product sent, as the sender's host keeps them. Host
 * only: the reply key's secret and the person never reach the product.
 *
 * A request waits until the product completes it, the product cancels it, or
 * its deadline passes (read from the clock). The first of these is final.
 * Nothing from the recipient's host closes it: there is no receipt and no
 * decline. Authenticated replies are kept, so a product that restarts gets
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
  /** The reply key's secret, `0x` hex. Never leaves the host. */
  replySecret: string;
  /** Authenticated reply payloads, in the order they arrived. */
  replies: { payload: string; at: number }[];
  state: OutgoingState;
}

export function outgoingStatus(
  item: OutgoingRequest,
  now: number,
): OutgoingStatus {
  if (item.state.tag !== "waiting") return item.state.tag;
  return now >= item.context.deadline ? "expired" : "waiting";
}

function isOutgoing(value: unknown): value is OutgoingRequest {
  if (typeof value !== "object" || value === null) return false;
  const item = value as Partial<OutgoingRequest>;
  const state = item.state as Partial<OutgoingState> | undefined;
  return (
    typeof item.handle === "string" &&
    typeof item.to === "string" &&
    typeof item.contact?.bytes === "string" &&
    typeof item.context?.deadline === "number" &&
    typeof item.payload === "string" &&
    typeof item.digest === "string" &&
    typeof item.sealed === "string" &&
    typeof item.replySecret === "string" &&
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
    try {
      const saved: unknown = JSON.parse(this.storage.getItem(this.key) ?? "[]");
      return Array.isArray(saved) ? saved.filter(isOutgoing) : [];
    } catch {
      return [];
    }
  }

  private save(): void {
    if (this.items.length === 0) this.storage.removeItem(this.key);
    else this.storage.setItem(this.key, JSON.stringify(this.items));
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

  byRoute(route: string): OutgoingRequest | undefined {
    return this.items.find((item) => item.context.returnRoute === route);
  }

  status(item: OutgoingRequest): OutgoingStatus {
    return outgoingStatus(item, this.now());
  }

  add(item: OutgoingRequest): void {
    this.items.push(structuredClone(item));
    this.save();
  }

  /** Requests still waiting. */
  waiting(): OutgoingRequest[] {
    return this.items.filter((item) => this.status(item) === "waiting");
  }

  /** Keep an authenticated reply. False when it is a repeat or there is no room. */
  addReply(handle: RequestHandle, payload: string): boolean {
    const item = this.get(handle);
    if (
      item === undefined ||
      item.replies.length >= REPLIES_MAX ||
      item.replies.some((reply) => reply.payload === payload)
    )
      return false;
    item.replies.push({ payload, at: this.now() });
    this.save();
    return true;
  }

  /** Close a waiting request. False when it no longer waits. */
  close(
    handle: RequestHandle,
    state: Exclude<OutgoingState, { tag: "waiting" }>,
  ) {
    const item = this.get(handle);
    if (item === undefined || this.status(item) !== "waiting") return false;
    item.state = state;
    this.save();
    return true;
  }

  /** When the next waiting request's deadline passes, or null. */
  nextDeadline(): number | null {
    const times = this.waiting().map((item) => item.context.deadline);
    return times.length === 0 ? null : Math.min(...times);
  }
}
