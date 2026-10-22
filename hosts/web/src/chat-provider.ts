import type { HexString, RendererNode } from "@parity/truapi";
import type { RenderHooks } from "./product-render.js";

/**
 * The boundary between the host's Chat screen and what supplies its chats.
 *
 * The screen draws one inbox and one conversation view and talks to every
 * source through this interface. A provider owns everything behind it: its
 * protocol, storage, delivery, retries, ordering and read receipts. The
 * screen only reports what the user did (sent text, saw messages up to one)
 * and draws what the provider says.
 *
 * Reads are cheap and synchronous where the screen needs speed: the inbox is
 * drawn from {@link ChatProvider.conversations}, a snapshot the provider keeps
 * in memory. Message history is read in pages, newest page first. Events are
 * nudges: after one, the screen reads the provider again, unless the event
 * carries the changed message.
 *
 * This is the web host's own seam, not TrUAPI. Two providers implement it:
 * the host's Nova chat engine and the rooms product workers keep through
 * TrUAPI Chat.
 */
export interface ChatProvider {
  /** Unique on the screen, such as `nova` or `product:<productId>`. */
  readonly id: string;
  /** What the provider is called on the screen. */
  readonly label: string;
  status(): ProviderStatus;
  /**
   * Bumped whenever earlier reads and render streams no longer hold, such as
   * when the source behind the provider restarts. The screen drops replies
   * that were asked for under an older epoch.
   */
  epoch(): number;
  /** The provider's conversations as it knows them now. */
  conversations(): readonly ConversationSummary[];
  /**
   * One page of a conversation, oldest message first. Without `before` it is
   * the newest page; with the `before` of a page, it is the page before it.
   */
  timeline(conversationId: string, before?: string): Promise<TimelinePage>;
  /** What the conversation shows below its messages. */
  composer(conversationId: string): Composer;
  /** Send text. The provider delivers it and reports the message through an event. */
  send(conversationId: string, text: string): Promise<void>;
  /** Send a failed message again, as the same message. */
  retry?(conversationId: string, messageId: string): Promise<void>;
  /**
   * The user has seen the conversation up to and including this message. The
   * screen calls it only for messages that were on screen; opening a
   * conversation alone is not reading it. What follows, such as an unread
   * count or a receipt to the peer, is the provider's.
   */
  markRead?(conversationId: string, throughMessageId: string): void;
  /** Draw a `Custom` message. Returns what stops the drawing. */
  renderCustom?(
    conversationId: string,
    message: ChatMessage,
    view: CustomView,
  ): () => void;
  /** Starting a conversation, for providers that can. */
  readonly start?: StartChat;
  /** Requests from people who want to chat, for providers that have them. */
  readonly requests?: RequestInbox;
  /** The provider's own account and notes about it, for providers that have one. */
  account?(): ProviderAccount;
  subscribe(listener: (event: ProviderEvent) => void): () => void;
  /** Stop the provider. Nothing it started outlives this. */
  dispose(): void;
}

export interface ProviderStatus {
  state: "starting" | "ready" | "offline" | "failed";
  /** Why the provider is offline or failed. */
  reason?: string;
}

export interface ConversationSummary {
  /** Unique within its provider. */
  id: string;
  title: string;
  /** A line under the title in the conversation's header. */
  subtitle: string;
  /** An `https` or inline raster image, or null to show an initial. */
  icon: string | null;
  /** What picks the initial's color. */
  seed: string;
  preview: string;
  /** Milliseconds since the epoch of the last activity. The inbox is ordered by it. */
  updatedAt: number;
  /** Messages the user has not seen. Absent when the provider does not count them. */
  unread?: number;
}

export interface ChatSender {
  id: string;
  label: string;
  /** Whether the user sent it, from this device or another. */
  self: boolean;
}

export type MessageStatus =
  | "pending"
  | "sent"
  | "delivered"
  | "read"
  | "failed"
  | "received";

export type MessageContent =
  | { tag: "Text"; text: string; label?: string }
  | { tag: "System"; text: string }
  | {
      tag: "Custom";
      messageType: string;
      payload: HexString;
      alt: string | null;
    };

export interface ChatMessage {
  /** Unique within its conversation, and stable: a change to a message keeps its id. */
  id: string;
  /** Milliseconds since the epoch. */
  at: number;
  sender: ChatSender;
  status: MessageStatus;
  /** Why the message failed, when it did. */
  problem?: string;
  content: MessageContent;
}

export interface TimelinePage {
  /** Oldest first. */
  messages: ChatMessage[];
  /** Opaque. Pass it back for the page before this one; absent when there is none. */
  before?: string;
}

export type Composer =
  | { kind: "text" }
  | { kind: "none" }
  | { kind: "blocked"; reason: string };

export type ProviderEvent =
  | { type: "conversations" }
  | { type: "status" }
  /** A conversation's messages changed. `message` is the changed one, when the provider knows it. */
  | { type: "messages"; conversationId: string; message?: ChatMessage };

/** Where a provider draws a `Custom` message. Each tree replaces the last. */
export interface CustomView {
  draw(tree: RendererNode, hooks: RenderHooks): void;
  /** Show the message's alt text, and why the card is not drawn. */
  fail(reason: string): void;
}

export interface StartChat {
  /** What the form asks for, such as a username. */
  placeholder: string;
  /** Why a conversation cannot be started now, or null. */
  blocked(): string | null;
  /** Start a conversation with `target`. Resolves to its id. */
  start(target: string, welcome: string): Promise<string>;
}

export interface ChatRequest {
  id: string;
  title: string;
  seed: string;
  preview: string;
  at: number;
  /** Why it cannot be accepted, when it cannot. */
  problem: string | null;
}

export interface RequestInbox {
  list(): readonly ChatRequest[];
  /** Accept a request. Resolves to the id of the conversation it opened. */
  accept(requestId: string): Promise<string>;
  decline(requestId: string): void;
}

export interface ProviderAccount {
  /** Who the user is to this provider, as shown. */
  name: string;
  /** More about it, for the tooltip. */
  title: string;
  /** Things the user should know, such as a missing allowance. */
  notes: readonly string[];
  /** Input the provider saw and set aside. */
  setAside: readonly string[];
}

/** Messages in one page of a provider's history. */
export const PAGE_SIZE = 30;

/**
 * A page of `items`, kept oldest first, ending just before the item with id
 * `before`, or at the newest item. The cursor is the id of the page's oldest
 * item, so pages stay put as newer items arrive.
 */
export function pageBefore<T extends { id: string }>(
  items: readonly T[],
  before: string | undefined,
  size = PAGE_SIZE,
): { items: T[]; before?: string } {
  const end =
    before === undefined
      ? items.length
      : items.findIndex((item) => item.id === before);
  if (end < 0) return { items: [] };
  const start = Math.max(0, end - size);
  return start > 0
    ? { items: items.slice(start, end), before: items[start].id }
    : { items: items.slice(start, end) };
}

/** A small set of listeners, for providers to tell the screen about changes. */
export class Listeners<T> {
  private readonly listeners = new Set<(value: T) => void>();

  add(listener: (value: T) => void): () => void {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  }

  emit(value: T): void {
    for (const listener of [...this.listeners]) listener(value);
  }

  clear(): void {
    this.listeners.clear();
  }
}
