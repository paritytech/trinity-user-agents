import type { Envelope } from "./binding.js";

/**
 * The invitations a user's host received, by the host's own id for each:
 * who sent it, the envelope, and the response the product accepted it with.
 * The Notifications list shows the request; this keeps what only the
 * invitation feature needs. It is kept so a repeat of an invitation is
 * recognised after a reload, and dismissing one keeps it here.
 */

export interface ReceivedInvitation {
  invitationId: string;
  /** The sender's person id and signing key. Host only. */
  from: { id: string; signingPublic: string };
  envelope: Envelope;
  /** The response digest the host issued its proof for, once accepted. */
  accepted: { response: string; at: number } | null;
}

function isReceived(value: unknown): value is ReceivedInvitation {
  if (typeof value !== "object" || value === null) return false;
  const item = value as Partial<ReceivedInvitation>;
  return (
    typeof item.invitationId === "string" &&
    typeof item.from?.id === "string" &&
    typeof item.from.signingPublic === "string" &&
    typeof item.envelope?.invitation?.requestId === "string" &&
    (item.accepted === null || typeof item.accepted?.response === "string")
  );
}

export class ReceivedInvitations {
  private items: ReceivedInvitation[];

  constructor(
    private readonly storage: Storage,
    private readonly key: string,
  ) {
    this.items = this.load();
  }

  private load(): ReceivedInvitation[] {
    try {
      const saved: unknown = JSON.parse(this.storage.getItem(this.key) ?? "[]");
      return Array.isArray(saved) ? saved.filter(isReceived) : [];
    } catch {
      return [];
    }
  }

  private save(): void {
    this.storage.setItem(this.key, JSON.stringify(this.items));
  }

  /** Read the list again, after another document of this origin wrote it. */
  reload(): void {
    this.items = this.load();
  }

  list(): readonly ReceivedInvitation[] {
    return this.items;
  }

  get(invitationId: string): ReceivedInvitation | undefined {
    return this.items.find((item) => item.invitationId === invitationId);
  }

  add(item: ReceivedInvitation): void {
    this.items.push(structuredClone(item));
    this.save();
  }

  markAccepted(invitationId: string, response: string, at: number): void {
    const item = this.get(invitationId);
    if (item === undefined || item.accepted !== null) return;
    item.accepted = { response, at };
    this.save();
  }
}
