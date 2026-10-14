/**
 * Feed updates of the product. In `source` each change goes to the store
 * first, and then to the host; in a host-store room it goes to the host only.
 */
import type { ChatMessage, ChatMessageBody, ChatMessageStatus } from "@parity/truapi";
import type { Clock, RoomId } from "./clock";
import type { ChatSurface } from "./context";
import { call, CommandError } from "./errors";
import type { RoomSettings } from "./state";
import { applyReaction, type SourceStore } from "./store";

/** How many echo message ids a room keeps. */
const ECHO_MEMORY = 50;

export interface FeedDeps {
  chat: ChatSurface;
  clock: () => Clock;
  store: () => SourceStore;
  settings: (room: RoomId) => RoomSettings;
}

export interface PostOptions {
  status?: ChatMessageStatus;
  /** `true`: the message is an echo, so a `Seen` of it makes no echo. */
  echo?: boolean;
}

export class Feed {
  readonly #deps: FeedDeps;

  constructor(deps: FeedDeps) {
    this.#deps = deps;
  }

  /**
   * Post a new product message. The body can use the message counter, as the
   * `Custom` card does.
   */
  async post(
    room: RoomId,
    body: ChatMessageBody | ((counter: number) => ChatMessageBody),
    options: PostOptions = {},
  ): Promise<string> {
    const { chat } = this.#deps;
    const clock = this.#deps.clock();
    const { id, counter } = clock.nextProductId(room);
    const message: ChatMessage = {
      messageId: id,
      origin: { participantId: "product" },
      timestamp: clock.nextTimestamp(),
      body: typeof body === "function" ? body(counter) : body,
      status: options.status ?? "Sent",
      reactions: [],
    };
    const store = this.#deps.store();
    if (room === "source") store.add(message);
    try {
      await call("update_feed", chat.updateFeed({ roomId: room, update: { tag: "New", value: message } }));
    } catch (error) {
      if (room === "source") store.delete(id);
      throw error;
    }
    const settings = this.#deps.settings(room);
    settings.productMessages.push(id);
    if (options.echo) {
      settings.echoMessages.push(id);
      settings.echoMessages.splice(0, Math.max(0, settings.echoMessages.length - ECHO_MEMORY));
    }
    return id;
  }

  /** Post a `Text` reply. A failure goes to the console only. */
  async reply(room: RoomId, text: string, options: PostOptions = {}): Promise<void> {
    try {
      await this.post(room, { tag: "Text", value: { text } }, options);
    } catch (error) {
      console.error(`chat-v2: reply in ${room} failed`, error);
    }
  }

  async edit(room: RoomId, messageId: string, body: ChatMessageBody): Promise<void> {
    this.#requireKnown(room, messageId);
    const editedAt = this.#deps.clock().now();
    await call(
      "update_feed",
      this.#deps.chat.updateFeed({ roomId: room, update: { tag: "Edited", value: { messageId, body, editedAt } } }),
    );
    if (room === "source") this.#deps.store().edit(messageId, body, editedAt);
  }

  async remove(room: RoomId, messageId: string): Promise<void> {
    this.#requireKnown(room, messageId);
    await call("update_feed", this.#deps.chat.updateFeed({ roomId: room, update: { tag: "Deleted", value: { messageId } } }));
    if (room === "source") this.#deps.store().delete(messageId);
    const settings = this.#deps.settings(room);
    settings.productMessages = settings.productMessages.filter((id) => id !== messageId);
  }

  async react(room: RoomId, messageId: string, participantId: string, emoji: string, placed: boolean): Promise<void> {
    this.#requireKnown(room, messageId);
    const value = { messageId, participantId, emoji };
    await call(
      "update_feed",
      this.#deps.chat.updateFeed({
        roomId: room,
        update: placed ? { tag: "ReactionPlaced", value } : { tag: "ReactionRemoved", value },
      }),
    );
    if (room === "source") this.#deps.store().react(messageId, participantId, emoji, placed);
    else this.recordReaction(room, messageId, participantId, emoji, placed);
  }

  /** Keep a reaction of a host-store room for `reactions_query`. */
  recordReaction(room: RoomId, messageId: string, participantId: string, emoji: string, placed: boolean): void {
    const book = this.#deps.settings(room).reactionBook;
    const reactions = book[messageId] ?? [];
    applyReaction(reactions, participantId, emoji, placed);
    if (reactions.length > 0) book[messageId] = reactions;
    else delete book[messageId];
  }

  async status(room: RoomId, messageId: string, status: ChatMessageStatus): Promise<void> {
    this.#requireKnown(room, messageId);
    await call(
      "update_feed",
      this.#deps.chat.updateFeed({ roomId: room, update: { tag: "StatusChanged", value: { messageId, status } } }),
    );
    if (room === "source") this.#deps.store().setStatus(messageId, status);
  }

  /**
   * The product knows every message of `source`, so an unknown id there
   * fails with no call. In a host-store room the host decides.
   */
  #requireKnown(room: RoomId, messageId: string): void {
    if (room === "source" && !this.#deps.store().has(messageId)) {
      throw new CommandError(`unknown message ${messageId}`);
    }
  }
}
