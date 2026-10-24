/**
 * PROTOTYPE. The host's notification list: requests addressed to the user
 * that wait for an answer, kept per wallet and network.
 *
 * It holds two kinds for now, a chat invitation and a profile sharing
 * request, and only the local demo puts any here (`origin: "demo"`). No
 * delivery API feeds it, and no answer leaves the host: accepting or
 * declining changes the item's state in this list and nothing else.
 *
 * Whether the user has seen an item and where it is in its lifecycle are
 * separate fields. Opening an item's detail marks that item seen; only an
 * answer, or the passing of its expiry, moves it out of pending. An item
 * past its expiry is expired, read from the clock, so it reads the same
 * after a reload and can no longer be accepted.
 */

export type NotificationKind = "invitation" | "sharing";

/** Where an item is in its lifecycle, as stored. Expiry is read from the clock. */
export type NotificationState =
  | { tag: "pending" }
  | { tag: "accepted"; at: number }
  | { tag: "declined"; at: number };

export type NotificationStatus =
  | "pending"
  | "accepted"
  | "declined"
  | "expired";

export interface HostNotification {
  id: string;
  /** Who put it here. Only the local demo does. */
  origin: "demo";
  kind: NotificationKind;
  /** The sender as the request names them. Nothing checks it. */
  sender: { name: string; handle: string };
  /** The product the request comes through. */
  product: { id: string; name: string };
  /** What the sender says the request is for, in their words. */
  purpose: string;
  receivedAt: number;
  expiresAt: number;
  /** When the user first opened the item, or null while unseen. */
  seenAt: number | null;
  state: NotificationState;
}

export type Answer =
  | { ok: true; changed: boolean }
  | { ok: false; reason: string };

export const notificationsKey = (network: string, walletId: string): string =>
  `truapi-web-signing-host:network:${network}:notifications:${walletId}:v1`;

/** Where `item` is at `now`. */
export function statusOf(
  item: HostNotification,
  now: number,
): NotificationStatus {
  if (item.state.tag !== "pending") return item.state.tag;
  return now >= item.expiresAt ? "expired" : "pending";
}

const KINDS: readonly NotificationKind[] = ["invitation", "sharing"];

/** Whether `value` is an item this version keeps. Anything else is dropped on load. */
function isNotification(value: unknown): value is HostNotification {
  if (typeof value !== "object" || value === null) return false;
  const item = value as Partial<HostNotification>;
  const state = item.state as Partial<NotificationState> | undefined;
  return (
    typeof item.id === "string" &&
    item.origin === "demo" &&
    KINDS.includes(item.kind as NotificationKind) &&
    typeof item.sender?.name === "string" &&
    typeof item.sender.handle === "string" &&
    typeof item.product?.id === "string" &&
    typeof item.product.name === "string" &&
    typeof item.purpose === "string" &&
    typeof item.receivedAt === "number" &&
    typeof item.expiresAt === "number" &&
    (item.seenAt === null || typeof item.seenAt === "number") &&
    (state?.tag === "pending" ||
      ((state?.tag === "accepted" || state?.tag === "declined") &&
        typeof (state as { at?: unknown }).at === "number"))
  );
}

export class NotificationStore {
  private key: string | null = null;
  private items: HostNotification[] = [];
  private readonly listeners = new Set<() => void>();

  constructor(
    private readonly storage: Storage,
    readonly now: () => number = () => Date.now(),
  ) {}

  /** Follow the signed-in wallet and network, or none. Nothing is written until something changes. */
  setScope(key: string | null): void {
    if (key === this.key) return;
    this.key = key;
    this.items = key === null ? [] : this.load(key);
    this.changed(false);
  }

  scoped(): boolean {
    return this.key !== null;
  }

  private load(key: string): HostNotification[] {
    try {
      const saved: unknown = JSON.parse(this.storage.getItem(key) ?? "[]");
      return Array.isArray(saved) ? saved.filter(isNotification) : [];
    } catch {
      return [];
    }
  }

  private changed(save = true): void {
    if (save && this.key !== null) {
      if (this.items.length === 0) this.storage.removeItem(this.key);
      else this.storage.setItem(this.key, JSON.stringify(this.items));
    }
    for (const listener of [...this.listeners]) listener();
  }

  onChange(listener: () => void): () => void {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  }

  /** Newest first. */
  list(): readonly HostNotification[] {
    return [...this.items].sort((a, b) => b.receivedAt - a.receivedAt);
  }

  get(id: string): HostNotification | undefined {
    return this.items.find((item) => item.id === id);
  }

  /** Items the user has not opened, whatever their state. */
  unseen(): number {
    return this.items.filter((item) => item.seenAt === null).length;
  }

  /** The first time the user opens an item. Its state does not change. */
  markSeen(id: string): void {
    const item = this.get(id);
    if (item === undefined || item.seenAt !== null) return;
    item.seenAt = this.now();
    this.changed();
  }

  accept(id: string): Answer {
    return this.answer(id, "accepted");
  }

  decline(id: string): Answer {
    return this.answer(id, "declined");
  }

  /**
   * Answer a pending item once. The same answer again changes nothing; any
   * other answer, or one after expiry, is refused.
   */
  private answer(id: string, tag: "accepted" | "declined"): Answer {
    const item = this.get(id);
    if (item === undefined)
      return { ok: false, reason: "This notification is gone." };
    const now = this.now();
    const status = statusOf(item, now);
    if (status === tag) return { ok: true, changed: false };
    if (status === "expired")
      return { ok: false, reason: "This request expired." };
    if (status !== "pending")
      return { ok: false, reason: `This request was already ${status}.` };
    item.state = { tag, at: now };
    this.changed();
    return { ok: true, changed: true };
  }

  /** When the next pending item expires, or null when none will. */
  nextExpiry(): number | null {
    const now = this.now();
    const times = this.items
      .filter((item) => statusOf(item, now) === "pending")
      .map((item) => item.expiresAt);
    return times.length === 0 ? null : Math.min(...times);
  }

  /** Put the demo's items here, in place of any it put before. Refused while signed out. */
  seedDemo(items: readonly HostNotification[]): void {
    if (this.key === null) return;
    this.items = [
      ...this.items.filter((item) => item.origin !== "demo"),
      ...items.map((item) => structuredClone(item)),
    ];
    this.changed();
  }

  /** Remove what the demo put here, and nothing else. */
  clearDemo(): void {
    if (this.key === null) return;
    this.items = this.items.filter((item) => item.origin !== "demo");
    this.changed();
  }
}
