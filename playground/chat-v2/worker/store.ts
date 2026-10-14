/**
 * The messages of the `source` room, which the product keeps itself. The
 * generated history is not stored: the store makes it again from its count,
 * and keeps only the changes to it.
 */
import type {
  ChatMessage,
  ChatMessageBody,
  ChatMessageStatus,
  ChatPageAnchor,
  ChatReactionDetail,
  ChatReactionSummary,
} from "@parity/truapi";
import { historyId } from "./clock";
import { historyMessage } from "./fixtures";

/** A message and the reactions of each participant on it. */
export interface StoredMessage {
  /** The message; its `reactions` field is computed from `reactions` below. */
  message: ChatMessage;
  reactions: ChatReactionDetail[];
}

/** What the product keeps in local storage for the store. */
export interface StoreSnapshot {
  historyCount: number;
  /** Changed history messages; `null` is a deleted one. */
  historyPatches: Record<string, StoredMessage | null>;
  /** The live messages, in order; `null` is a deleted one. */
  live: (StoredMessage | null)[];
}

export function emptyStoreSnapshot(): StoreSnapshot {
  return { historyCount: 0, historyPatches: {}, live: [] };
}

/** The most messages one page holds, the bound of the RFC. */
export const MAX_PAGE_SIZE = 100;

export interface Page {
  messages: ChatMessage[];
  next?: string;
}

/** A failure of the store, with the reason the answer shows. */
export class StoreError extends Error {}

/** One summary per emoji, in the order of the first reaction. */
export function summarize(reactions: readonly ChatReactionDetail[]): ChatReactionSummary[] {
  const byEmoji = new Map<string, ChatReactionSummary>();
  for (const { participantId, emoji } of reactions) {
    const summary = byEmoji.get(emoji) ?? { emoji, count: 0, reactedByUser: false };
    summary.count += 1;
    if (participantId === "user") summary.reactedByUser = true;
    byEmoji.set(emoji, summary);
  }
  return [...byEmoji.values()];
}

/**
 * Add or remove one reaction. A participant has at most one reaction of an
 * emoji on a message. The result tells if the list changed.
 */
export function applyReaction(
  reactions: ChatReactionDetail[],
  participantId: string,
  emoji: string,
  placed: boolean,
): boolean {
  const index = reactions.findIndex((r) => r.participantId === participantId && r.emoji === emoji);
  if (placed && index < 0) {
    reactions.push({ participantId, emoji });
    return true;
  }
  if (!placed && index >= 0) {
    reactions.splice(index, 1);
    return true;
  }
  return false;
}

export class SourceStore {
  #snapshot: StoreSnapshot;
  /** The slot of each message id: history slots first, then live slots. */
  #slots = new Map<string, number>();

  constructor(snapshot: StoreSnapshot = emptyStoreSnapshot()) {
    this.#snapshot = snapshot;
    this.#index();
  }

  snapshot(): StoreSnapshot {
    return this.#snapshot;
  }

  /** The number of slots: history, then live, deleted ones included. */
  get slotCount(): number {
    return this.#snapshot.historyCount + this.#snapshot.live.length;
  }

  /** Replace every message with `count` generated history messages. */
  replaceHistory(count: number): void {
    this.#snapshot = { historyCount: count, historyPatches: {}, live: [] };
    this.#index();
  }

  add(message: ChatMessage): void {
    if (this.#slots.has(message.messageId)) {
      throw new StoreError(`message ${message.messageId} exists`);
    }
    this.#slots.set(message.messageId, this.slotCount);
    this.#snapshot.live.push({ message: { ...message, reactions: [] }, reactions: [] });
  }

  has(messageId: string): boolean {
    return this.#stored(messageId) !== undefined;
  }

  get(messageId: string): ChatMessage | undefined {
    const stored = this.#stored(messageId);
    return stored ? present(stored) : undefined;
  }

  /** Every message, oldest first. */
  messages(): ChatMessage[] {
    const out: ChatMessage[] = [];
    for (let slot = 0; slot < this.slotCount; slot++) {
      const stored = this.#at(slot);
      if (stored) out.push(present(stored));
    }
    return out;
  }

  edit(messageId: string, body: ChatMessageBody, editedAt: bigint): void {
    this.#update(messageId, (stored) => {
      stored.message = { ...stored.message, body, editedAt };
    });
  }

  setStatus(messageId: string, status: ChatMessageStatus): void {
    this.#update(messageId, (stored) => {
      stored.message = { ...stored.message, status };
    });
  }

  react(messageId: string, participantId: string, emoji: string, placed: boolean): void {
    this.#update(messageId, (stored) => {
      applyReaction(stored.reactions, participantId, emoji, placed);
    });
  }

  delete(messageId: string): void {
    const slot = this.#requireSlot(messageId);
    if (slot < this.#snapshot.historyCount) this.#snapshot.historyPatches[messageId] = null;
    else this.#snapshot.live[slot - this.#snapshot.historyCount] = null;
  }

  reactionDetails(messageId: string): ChatReactionDetail[] {
    const stored = this.#stored(messageId);
    if (!stored) throw new StoreError(`unknown message ${messageId}`);
    return stored.reactions.map((reaction) => ({ ...reaction }));
  }

  /**
   * One page. A cursor is the slot where the next page in the same direction
   * starts (`After`) or ends (`Before`). Deleted slots are skipped, so a page
   * can hold fewer messages than asked and still have a next page.
   */
  page(anchor: ChatPageAnchor, pageSize: number): Page {
    const size = Math.max(1, Math.min(pageSize, MAX_PAGE_SIZE));
    const total = this.slotCount;
    const cursor = anchor.value.cursor === undefined ? undefined : parseCursor(anchor.value.cursor, total);
    if (anchor.tag === "Before") {
      const end = cursor ?? total;
      const start = Math.max(0, end - size);
      return { messages: this.#range(start, end), ...(start > 0 ? { next: String(start) } : {}) };
    }
    const start = cursor ?? 0;
    const end = Math.min(total, start + size);
    return { messages: this.#range(start, end), ...(end < total ? { next: String(end) } : {}) };
  }

  #range(start: number, end: number): ChatMessage[] {
    const out: ChatMessage[] = [];
    for (let slot = start; slot < end; slot++) {
      const stored = this.#at(slot);
      if (stored) out.push(present(stored));
    }
    return out;
  }

  #index(): void {
    this.#slots.clear();
    const { historyCount, live } = this.#snapshot;
    for (let i = 1; i <= historyCount; i++) {
      this.#slots.set(historyId(i), i - 1);
    }
    live.forEach((stored, i) => {
      if (stored) this.#slots.set(stored.message.messageId, historyCount + i);
    });
  }

  #at(slot: number): StoredMessage | null {
    const { historyCount, historyPatches, live } = this.#snapshot;
    if (slot >= historyCount) return live[slot - historyCount] ?? null;
    const patch = historyPatches[historyId(slot + 1)];
    return patch === undefined ? historyMessage(slot + 1, historyCount) : patch;
  }

  #stored(messageId: string): StoredMessage | undefined {
    const slot = this.#slots.get(messageId);
    return slot === undefined ? undefined : (this.#at(slot) ?? undefined);
  }

  #requireSlot(messageId: string): number {
    const slot = this.#slots.get(messageId);
    if (slot === undefined || !this.#at(slot)) throw new StoreError(`unknown message ${messageId}`);
    return slot;
  }

  #update(messageId: string, change: (stored: StoredMessage) => void): void {
    const slot = this.#requireSlot(messageId);
    const current = this.#at(slot);
    if (!current) throw new StoreError(`unknown message ${messageId}`);
    if (slot < this.#snapshot.historyCount) {
      const copy: StoredMessage = { message: { ...current.message }, reactions: [...current.reactions] };
      change(copy);
      this.#snapshot.historyPatches[messageId] = copy;
    } else {
      change(current);
    }
  }
}

function present(stored: StoredMessage): ChatMessage {
  return { ...stored.message, reactions: summarize(stored.reactions) };
}

function parseCursor(cursor: string, total: number): number {
  if (!/^(0|[1-9][0-9]*)$/.test(cursor)) throw new StoreError(`bad cursor ${cursor}`);
  const value = Number(cursor);
  if (value > total) throw new StoreError(`bad cursor ${cursor}`);
  return value;
}
