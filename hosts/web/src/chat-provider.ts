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
 * This is the web host's own seam, not TrUAPI. Three providers implement it:
 * the host's Nova chat engine, the rooms product workers keep through TrUAPI
 * Chat, and the rooms chat products keep themselves over the Chat Extension
 * v2 draft bridge (`src/chat-v2/`).
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
  /** The user pressed one of an `Actions` message's buttons. */
  triggerAction?(
    conversationId: string,
    messageId: string,
    actionId: string,
  ): Promise<void>;
  /**
   * Set the user's reaction on a message, or take it away with `null`. The
   * screen offers it only on messages marked {@link ChatMessage.reactable}.
   * How many emoji one person keeps on a message is the provider's.
   */
  react?(
    conversationId: string,
    messageId: string,
    emoji: string | null,
  ): Promise<void>;
  /** What the user can do to the conversation itself now. */
  controls?(conversationId: string): ConversationControls;
  /** Block the conversation's person. See {@link ConversationControls.block}. */
  block?(conversationId: string): Promise<void>;
  unblock?(conversationId: string): Promise<void>;
  /** The conversations the user blocked, which the list leaves out. */
  blocked?(): readonly ConversationSummary[];
  /** Leave the conversation. See {@link ConversationControls.leave}. */
  leave?(conversationId: string): Promise<void>;
  /** Sending and opening files, for providers that move them. */
  readonly files?: FileTransfer;
  /** Audio and video calls, for providers that place them. */
  readonly calls?: CallSupport;
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
  /**
   * The people the user chats with here, for the host's contact picker.
   * Only a provider whose peers' accounts the host itself established offers
   * them: a contact can become a payment recipient, so a name or account a
   * product asserted is not one.
   */
  contacts?(): readonly ChatContact[];
  subscribe(listener: (event: ProviderEvent) => void): () => void;
  /** Stop the provider. Nothing it started outlives this. */
  dispose(): void;
}

/** What a conversation offers now. A control that is absent or false is not shown. */
export interface ConversationControls {
  /**
   * Blocking is local: the person is not told, their conversation leaves
   * the list and the contact picker, and what they send is not read until
   * they are unblocked.
   */
  block?: boolean;
  /** Leaving tells the person, then takes the conversation off the list. */
  leave?: boolean;
}

/** How a provider moves files. The bytes go through the provider, never a URL a message names. */
export interface FileTransfer {
  /** Why files cannot be sent in the conversation now, or null. */
  blocked(conversationId: string): string | null;
  /** The largest file accepted, in bytes. */
  maxBytes: number;
  /** Send `file` with `caption`. The message shows at once and fails if the upload does. */
  send(conversationId: string, file: File, caption: string): Promise<void>;
  /** Stop sending a message's file. */
  cancel(conversationId: string, messageId: string): void;
  /** Fetch one file of a message, checked and opened. */
  open(
    conversationId: string,
    messageId: string,
    index: number,
    signal: AbortSignal,
  ): Promise<Blob>;
}

/**
 * How a provider places calls. It holds at most one call. The camera and
 * microphone open only from {@link CallSupport.start} or
 * {@link CallSupport.accept}, which the screen calls on a press, and close
 * when the call ends.
 */
export interface CallSupport {
  /** Why the conversation cannot be called now, or null. */
  blocked(conversationId: string): string | null;
  start(conversationId: string, video: boolean): Promise<void>;
  current(): CallView | null;
  accept(): Promise<void>;
  /** Decline an incoming call, cancel an outgoing one, or hang up. */
  hangUp(): void;
  setMuted(muted: boolean): void;
  setCamera(on: boolean): void;
  /** Forget an ended call once the screen has shown how it ended. */
  dismiss(): void;
  subscribe(listener: () => void): () => void;
}

export interface CallView {
  conversationId: string;
  title: string;
  video: boolean;
  direction: "out" | "in";
  state:
    | "calling"
    | "ringing"
    | "incoming"
    | "connecting"
    | "connected"
    | "ended";
  problem: string | null;
  muted: boolean;
  cameraOn: boolean;
  remoteCamera: boolean;
  local: MediaStream | null;
  remote: MediaStream | null;
  /** About the call's relay, such as why there is none. Null when there is nothing to say. */
  relay?: string | null;
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
  /** Text and media. Each media entry is an `https` URL or an inline raster image. */
  | { tag: "RichText"; text: string | null; media: readonly string[] }
  /** Buttons the host draws. A press goes to {@link ChatProvider.triggerAction}. */
  | {
      tag: "Actions";
      text: string | null;
      actions: readonly { actionId: string; title: string }[];
      layout: "Column" | "Grid";
    }
  /** A link to a file. Its name, type and size are the sender's, not checked. */
  | {
      tag: "File";
      url: string;
      fileName: string;
      mimeType: string;
      sizeBytes: number;
      text: string | null;
    }
  | {
      tag: "Custom";
      messageType: string;
      payload: HexString;
      alt: string | null;
    }
  /** Files the provider moves itself, opened through {@link ChatProvider.files}. */
  | { tag: "Attachments"; text: string | null; files: readonly MessageFile[] };

export interface MessageFile {
  /** Known for files this side sent; the wire may not carry one. */
  name: string | null;
  kind: "file" | "image" | "video";
  mimeType: string;
  /** As the sender gave it. */
  sizeBytes: number;
  width?: number;
  height?: number;
  /** Why the file cannot be opened here, or null when it can. */
  unavailable: string | null;
}

/** The reactions on a message, one entry per emoji, as RFC 0002 aggregates them. */
export interface MessageReaction {
  emoji: string;
  count: number;
  /** Whether the user is among those who reacted. */
  self: boolean;
}

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
  reactions?: readonly MessageReaction[];
  /** Whether the user can react to this message now, through {@link ChatProvider.react}. */
  reactable?: boolean;
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

export interface ChatContact {
  /** The person's 32-byte account. */
  account: HexString;
  /** How the host names them in its own UI. Never given to a product. */
  name: string;
  /** When the last message with them was sent or received, when the provider keeps one. */
  lastActiveAt?: number;
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
