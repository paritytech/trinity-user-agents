/**
 * PROTOTYPE. The host's notification list: requests addressed to the user
 * that wait for an answer, kept per user and network.
 *
 * It holds two kinds for now, a chat invitation and a profile sharing
 * request. It does not know where a request came from: a feature adds its
 * requests and follows their answers with `onAnswer`.
 *
 * Whether the user has seen an item, where it is in its lifecycle, and
 * whether the user dismissed it are separate fields. An item is seen once it
 * shows in the list; only an answer, or the passing of its expiry, moves it
 * out of pending. An item past its expiry is expired, read from the clock, so
 * it reads the same after a reload and can no longer be accepted. Dismissing
 * takes an item off the list on this device only: it is not an answer, and
 * the item stays stored as it was.
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
  /** Chosen by the feature that adds it, such as an invitation id. */
  id: string;
  kind: NotificationKind;
  /** The sender as the request names them. Nothing checks it. */
  sender: { name: string; handle: string };
  /** The product the request comes through. */
  product: { id: string; name: string };
  /** What the sender says the request is for, in their words. */
  purpose: string;
  receivedAt: number;
  expiresAt: number;
  /** When the item first showed to the user, or null while unseen. */
  seenAt: number | null;
  state: NotificationState;
  /** When the user dismissed it from the list. Absent or null while listed. */
  dismissedAt?: number | null;
}

export type Answer =
  | { ok: true; changed: boolean }
  | { ok: false; reason: string };

export const notificationsKey = (network: string, walletId: string): string =>
  `truapi-web-signing-host:network:${network}:notifications:${walletId}:v1`;

const isDismissed = (item: HostNotification): boolean =>
  typeof item.dismissedAt === "number";

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
    KINDS.includes(item.kind as NotificationKind) &&
    typeof item.sender?.name === "string" &&
    typeof item.sender.handle === "string" &&
    typeof item.product?.id === "string" &&
    typeof item.product.name === "string" &&
    typeof item.purpose === "string" &&
    typeof item.receivedAt === "number" &&
    typeof item.expiresAt === "number" &&
    (item.seenAt === null || typeof item.seenAt === "number") &&
    (item.dismissedAt === undefined ||
      item.dismissedAt === null ||
      typeof item.dismissedAt === "number") &&
    (state?.tag === "pending" ||
      ((state?.tag === "accepted" || state?.tag === "declined") &&
        typeof (state as { at?: unknown }).at === "number"))
  );
}

export class NotificationStore {
  private key: string | null = null;
  private items: HostNotification[] = [];
  private readonly listeners = new Set<() => void>();
  private readonly answerListeners = new Set<
    (item: HostNotification) => void
  >();

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

  /** Read the list again, after another document of this origin wrote it. */
  reload(): void {
    if (this.key === null) return;
    this.items = this.load(this.key);
    this.changed(false);
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

  /** Calls back once for each answer that changes an item, after it is saved. */
  onAnswer(listener: (item: HostNotification) => void): () => void {
    this.answerListeners.add(listener);
    return () => this.answerListeners.delete(listener);
  }

  /**
   * Add `item`. An item with the same id already here is kept as it is,
   * answered or not, and returned. Refused while signed out.
   */
  add(
    item: HostNotification,
  ): { added: boolean; item: HostNotification } | null {
    if (this.key === null) return null;
    const existing = this.get(item.id);
    if (existing !== undefined) return { added: false, item: existing };
    const copy = structuredClone(item);
    this.items.push(copy);
    this.changed();
    return { added: true, item: copy };
  }

  /** Every item, dismissed or not, newest first. */
  list(): readonly HostNotification[] {
    return [...this.items].sort((a, b) => b.receivedAt - a.receivedAt);
  }

  /** The items the list shows: those not dismissed, newest first. */
  listed(): readonly HostNotification[] {
    return this.list().filter((item) => !isDismissed(item));
  }

  get(id: string): HostNotification | undefined {
    return this.items.find((item) => item.id === id);
  }

  /** Listed items the user has not seen, whatever their state. */
  unseen(): number {
    return this.items.filter(
      (item) => item.seenAt === null && !isDismissed(item),
    ).length;
  }

  /** The first time these items show to the user. Their state does not change. */
  markSeen(...ids: string[]): void {
    const now = this.now();
    let marked = false;
    for (const id of ids) {
      const item = this.get(id);
      if (item === undefined || item.seenAt !== null) continue;
      item.seenAt = now;
      marked = true;
    }
    if (marked) this.changed();
  }

  /**
   * Take an item off the list, on this device only. It is not an answer:
   * nothing is reported, its state is kept and it still expires.
   */
  dismiss(id: string): void {
    const item = this.get(id);
    if (item === undefined || isDismissed(item)) return;
    const now = this.now();
    item.dismissedAt = now;
    item.seenAt ??= now;
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
    for (const listener of [...this.answerListeners]) listener(item);
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

  /** Remove the items with these ids, answered or not. */
  remove(ids: readonly string[]): void {
    if (this.key === null) return;
    const before = this.items.length;
    this.items = this.items.filter((item) => !ids.includes(item.id));
    if (this.items.length !== before) this.changed();
  }
}
