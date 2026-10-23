import { bytesToHex } from "@parity/truapi/scale";
import type { HexString, ImageSource, RenderContext } from "@parity/truapi";
import type { TrUApiProductProvider } from "@parity/truapi-host";
import {
  Listeners,
  pageBefore,
  type ChatMessage,
  type ChatProvider,
  type Composer,
  type ConversationSummary,
  type CustomView,
  type MessageReaction,
  type ProviderEvent,
  type ProviderStatus,
  type TimelinePage,
} from "./chat-provider.js";
import type {
  ProductChat,
  ProductMessage,
  ProductRoom,
} from "./product-chat.js";
import { bulletinImageUrl } from "./product-render.js";
import type { WorkerState } from "./product-workers.js";

/**
 * The rooms one product's worker keeps through TrUAPI Chat, as a chat
 * provider. The host stores these rooms (`src/product-chat.ts`); this file
 * reads them, sends what the user types to the worker as a chat action, and
 * asks the worker to draw `Custom` messages through `Renderer`.
 */

/** The peer a user's message and actions come from, as the native hosts name it. */
export const USER_PEER = "native";

export const productProviderId = (productId: string): string =>
  `product:${productId}`;

/** The calls a room makes on the worker's channel to the core. */
export type RoomChannel = Pick<
  TrUApiProductProvider,
  "render" | "publishRendererAction" | "publishChatAction"
>;

export interface ProductRoomDeps {
  chat: ProductChat;
  /** The running worker's channel, or null when it is not running. */
  channel(productId: string): RoomChannel | null;
  /** The worker's state, or null when nothing runs it. */
  workerState(productId: string): WorkerState | null;
  /** Calls back when a worker starts, stops or changes state. */
  onWorkersChange(listener: () => void): () => void;
  archiveImage(productId: string, path: string): Promise<string | null>;
  gateway(): string;
  log(text: string): void;
}

const ICON = /^(?:https:\/\/|data:image\/(?:png|jpeg|gif|webp);base64,)/;

const errorText = (error: unknown): string =>
  error instanceof Error ? error.message : String(error);

function previewOf(
  message: ProductMessage | undefined,
  fallback: string,
): string {
  const content = message?.content;
  switch (content?.tag) {
    case undefined:
      return fallback;
    case "Text":
      return content.text;
    case "RichText":
      return content.text ?? "Media";
    case "Actions":
      return (
        content.text ??
        content.actions.map((action) => action.title).join(" · ")
      );
    case "File":
      return content.fileName;
    case "Custom":
      return content.alt ?? fallback;
  }
}

/** The product's emoji and the user's, one entry per emoji. */
function reactionsOf(message: ProductMessage): {
  reactions?: MessageReaction[];
} {
  const byEmoji = new Map<string, MessageReaction>();
  const add = (emoji: string, self: boolean) => {
    const known = byEmoji.get(emoji) ?? { emoji, count: 0, self: false };
    known.count += 1;
    known.self ||= self;
    byEmoji.set(emoji, known);
  };
  for (const emoji of message.reactions ?? []) add(emoji, false);
  if (message.userReaction !== undefined) add(message.userReaction, true);
  return byEmoji.size === 0 ? {} : { reactions: [...byEmoji.values()] };
}

export class ProductRoomProvider implements ChatProvider {
  readonly id: string;
  readonly label: string;
  private readonly listeners = new Listeners<ProviderEvent>();
  private readonly stops: (() => void)[];
  private channel: RoomChannel | null;
  private generation = 0;

  constructor(
    readonly productId: string,
    private readonly deps: ProductRoomDeps,
  ) {
    this.id = productProviderId(productId);
    this.label = productId;
    this.channel = deps.channel(productId);
    this.stops = [
      deps.chat.onChange(() => {
        for (const room of this.rooms())
          this.listeners.emit({
            type: "messages",
            conversationId: room.roomId,
          });
        this.listeners.emit({ type: "conversations" });
      }),
      deps.onWorkersChange(() => {
        const channel = deps.channel(productId);
        if (channel !== this.channel) {
          this.channel = channel;
          this.generation += 1;
        }
        this.listeners.emit({ type: "status" });
        this.listeners.emit({ type: "conversations" });
      }),
    ];
  }

  private rooms(): ProductRoom[] {
    return this.deps.chat
      .rooms()
      .filter((room) => room.productId === this.productId);
  }

  private room(roomId: string): ProductRoom | undefined {
    return this.deps.chat.room(this.productId, roomId);
  }

  status(): ProviderStatus {
    const state = this.deps.workerState(this.productId);
    if (state === null || this.channel === null)
      return state?.state === "starting"
        ? { state: "starting" }
        : state?.state === "failed"
          ? { state: "failed", reason: state.reason }
          : {
              state: "offline",
              reason: "The product's worker is not running.",
            };
    return { state: "ready" };
  }

  /** A new worker channel is a new epoch: render streams on the old one are gone. */
  epoch(): number {
    return this.generation;
  }

  conversations(): ConversationSummary[] {
    return this.rooms().map((room) => {
      const last = this.deps.chat.messages(this.productId, room.roomId).at(-1);
      return {
        id: room.roomId,
        title: room.name,
        subtitle: this.productId,
        icon: ICON.test(room.icon) ? room.icon : null,
        seed: this.productId,
        preview: previewOf(last, this.productId),
        updatedAt: last?.at ?? room.createdAt,
      };
    });
  }

  async timeline(roomId: string, before?: string): Promise<TimelinePage> {
    const room = this.room(roomId);
    if (room === undefined) return { messages: [] };
    const page = pageBefore(
      this.deps.chat
        .messages(this.productId, roomId)
        .map((message) => ({ ...message, id: message.messageId })),
      before,
    );
    return {
      messages: page.items.map((message) => this.message(room, message)),
      before: page.before,
    };
  }

  private message(room: ProductRoom, message: ProductMessage): ChatMessage {
    const out = message.direction === "out";
    const content = message.content;
    return {
      id: message.messageId,
      at: message.at,
      sender: out
        ? { id: USER_PEER, label: "You", self: true }
        : { id: this.productId, label: room.name, self: false },
      status: out ? "sent" : "received",
      content,
      ...reactionsOf(message),
      ...(out || this.channel === null ? {} : { reactable: true }),
    };
  }

  /**
   * The user's reaction on one of the product's messages, through the
   * existing TrUAPI types: `MessagePosted(Reaction)` from the user's peer,
   * naming the id the product got from `post_message`. A new emoji is sent
   * first, then the old one is removed with `MessagePosted(ReactionRemoved)`;
   * the same emoji again removes it. The user keeps one emoji per message.
   */
  async react(
    roomId: string,
    messageId: string,
    emoji: string | null,
  ): Promise<void> {
    const channel = this.channel;
    const publish = channel?.publishChatAction;
    if (channel === null || publish === undefined)
      throw new Error("The product's worker is not running.");
    const current =
      this.deps.chat
        .messages(this.productId, roomId)
        .find((message) => message.messageId === messageId)?.userReaction ??
      null;
    const placing = emoji === current ? null : emoji;
    this.deps.chat.setUserReaction(this.productId, roomId, messageId, placing);
    const post = (tag: "Reaction" | "ReactionRemoved", value: string) =>
      publish.call(channel, {
        roomId,
        peer: USER_PEER,
        payload: {
          tag: "MessagePosted",
          value: { tag, value: { messageId, emoji: value } },
        },
      });
    if (placing !== null) await post("Reaction", placing);
    if (current !== null) await post("ReactionRemoved", current);
  }

  composer(roomId: string): Composer {
    const room = this.room(roomId);
    if (room === undefined || room.footer === "Empty") return { kind: "none" };
    return this.channel === null
      ? { kind: "blocked", reason: "The product's worker is not running" }
      : { kind: "text" };
  }

  /** Keep the text in the room, as it shows it, and hand it to the worker. */
  async send(roomId: string, text: string): Promise<void> {
    const channel = this.channel;
    if (channel?.publishChatAction === undefined)
      throw new Error("The product's worker is not running.");
    this.deps.chat.addOutgoing(this.productId, roomId, text);
    void channel
      .publishChatAction({
        roomId,
        peer: USER_PEER,
        payload: {
          tag: "MessagePosted",
          value: { tag: "Text", value: { text } },
        },
      })
      .catch((error: unknown) =>
        this.deps.log(
          `${this.productId}: message not delivered: ${errorText(error)}`,
        ),
      );
  }

  /** A press on a button the host drew for an `Actions` message, as `ActionTriggered`. */
  async triggerAction(
    roomId: string,
    messageId: string,
    actionId: string,
  ): Promise<void> {
    const channel = this.channel;
    if (channel?.publishChatAction === undefined)
      throw new Error("The product's worker is not running.");
    await channel.publishChatAction({
      roomId,
      peer: USER_PEER,
      payload: { tag: "ActionTriggered", value: { messageId, actionId } },
    });
  }

  renderCustom(
    roomId: string,
    message: ChatMessage,
    view: CustomView,
  ): () => void {
    const content = message.content;
    const channel = this.channel;
    if (content.tag !== "Custom") return () => {};
    if (channel?.render === undefined) {
      view.fail("The product's worker is not running.");
      return () => {};
    }
    const context: RenderContext = {
      tag: "ChatMessage",
      value: {
        roomId,
        messageId: message.id,
        messageType: content.messageType,
      },
    };
    const hooks = {
      action: (actionId: string, payload: Uint8Array) => {
        void channel
          .publishRendererAction?.({
            context,
            actionId,
            payload: bytesToHex(payload) as HexString,
          })
          .catch((error: unknown) =>
            this.deps.log(
              `${this.productId}: action ${actionId} not delivered: ${errorText(error)}`,
            ),
          );
      },
      image: (source: ImageSource) =>
        source.tag === "Archive"
          ? this.deps.archiveImage(this.productId, source.value)
          : bulletinImageUrl(source.value, this.deps.gateway()),
      unsupported: (what: string) =>
        this.deps.log(`${this.productId}: this host does not draw ${what}`),
    };
    return channel.render(
      { context, payload: content.payload },
      {
        onUpdate: (tree) => view.draw(tree, hooks),
        onError: (error) =>
          view.fail(`Could not draw this message: ${errorText(error)}`),
      },
    );
  }

  subscribe(listener: (event: ProviderEvent) => void): () => void {
    return this.listeners.add(listener);
  }

  /** Stop following the rooms. The worker itself belongs to `ProductWorkers`. */
  dispose(): void {
    for (const stop of this.stops) stop();
    this.listeners.clear();
  }
}
