import type { Envelope, PeerKeys } from "./binding.js";
import type { ContactRef, OutgoingStatus } from "./contract.js";

/**
 * The invitations a user's product sent, as the sender's host keeps them.
 *
 * An invitation waits until the product completes it with a response the
 * host verified, the product cancels it, or it expires (read from the clock).
 * The first of these is final. Nothing from the recipient's host changes it:
 * there is no delivery receipt and no decline.
 */

export type OutgoingState =
  | { tag: "waiting" }
  | { tag: "completed"; at: number; response: string }
  | { tag: "cancelled"; at: number };

export interface OutgoingInvitation {
  /** The product's request id. */
  requestId: string;
  /** The person the user chose, as the host knew them at sending. Host only. */
  to: { id: string; keys: PeerKeys };
  contact: ContactRef;
  envelope: Envelope;
  /** The sealed envelope, posted again unchanged. */
  sealed: string;
  /** Digests of the responses whose proof the host verified. */
  verified: string[];
  state: OutgoingState;
}

export function outgoingStatus(
  item: OutgoingInvitation,
  now: number,
): OutgoingStatus {
  if (item.state.tag !== "waiting") return item.state.tag;
  return now >= item.envelope.invitation.expiresAt ? "expired" : "waiting";
}

function isOutgoing(value: unknown): value is OutgoingInvitation {
  if (typeof value !== "object" || value === null) return false;
  const item = value as Partial<OutgoingInvitation>;
  const state = item.state as Partial<OutgoingState> | undefined;
  return (
    typeof item.requestId === "string" &&
    typeof item.to?.id === "string" &&
    typeof item.to.keys?.signingPublic === "string" &&
    typeof item.contact === "string" &&
    typeof item.envelope?.invitation?.expiresAt === "number" &&
    typeof item.sealed === "string" &&
    Array.isArray(item.verified) &&
    (state?.tag === "waiting" ||
      state?.tag === "completed" ||
      state?.tag === "cancelled")
  );
}

export class Outbox {
  private items: OutgoingInvitation[];

  constructor(
    private readonly storage: Storage,
    private readonly key: string,
    private readonly now: () => number,
  ) {
    this.items = this.load();
  }

  private load(): OutgoingInvitation[] {
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
  list(): readonly OutgoingInvitation[] {
    return this.items;
  }

  get(requestId: string): OutgoingInvitation | undefined {
    return this.items.find((item) => item.requestId === requestId);
  }

  status(item: OutgoingInvitation): OutgoingStatus {
    return outgoingStatus(item, this.now());
  }

  add(item: OutgoingInvitation): void {
    this.items.push(structuredClone(item));
    this.save();
  }

  /** Invitations still waiting. */
  waiting(): OutgoingInvitation[] {
    return this.items.filter((item) => this.status(item) === "waiting");
  }

  /** Note a verified response of a waiting invitation. */
  verified(requestId: string, response: string): void {
    const item = this.get(requestId);
    if (item === undefined || item.verified.includes(response)) return;
    item.verified.push(response);
    this.save();
  }

  /** Close a waiting invitation. False when it no longer waits. */
  close(requestId: string, state: Exclude<OutgoingState, { tag: "waiting" }>) {
    const item = this.get(requestId);
    if (item === undefined || this.status(item) !== "waiting") return false;
    item.state = state;
    this.save();
    return true;
  }

  /** When the next waiting invitation expires, or null. */
  nextExpiry(): number | null {
    const times = this.waiting().map(
      (item) => item.envelope.invitation.expiresAt,
    );
    return times.length === 0 ? null : Math.min(...times);
  }
}
