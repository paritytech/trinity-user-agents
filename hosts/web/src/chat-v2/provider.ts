import { bytesToHex, hexToBytes } from "@parity/truapi/scale";
import {
  Listeners,
  PAGE_SIZE,
  type ChatMessage,
  type ChatProvider,
  type Composer,
  type ConversationSummary,
  type FileTransfer,
  type MessageContent,
  type MessageFile,
  type MessageStatus,
  type ProviderEvent,
  type ProviderStatus,
  type TimelinePage,
} from "../chat-provider.js";
import { V2Calls } from "./calls.js";
import {
  INLINE_MEDIA_MAX_BYTES,
  type ChatMediaItem,
  type ChatMessage as V2Message,
  type ChatMessageBody,
  type ChatMessageStatus,
  type ChatParticipant,
  type ChatResource,
} from "./contract.js";
import type { ChatV2Change, ChatV2Host, V2Room } from "./host.js";

/**
 * DRAFT, NOT TRUAPI. The rooms one product declared over the Chat Extension
 * v2 draft bridge, as a chat provider of its own. It implements the screen's
 * `ChatProvider` interface as it is, and maps the contract onto it:
 *
 * - pages come from `messages_query` (`Before`, with the product's cursor);
 * - a sent message shows as pending under a local id, and the product's
 *   `MessageCreated` answer is drawn under that same id, so the screen sees
 *   one message change, as it does for a status change;
 * - `update_feed` changes are drawn as changed messages; a delete makes the
 *   screen read the newest page again, which drops what the page no longer
 *   holds;
 * - `Seen` goes out only for a room that sets `reportSeen`;
 * - `"user"` is the user, `"product"` the product, and other participant ids
 *   are named through `participants_query` `ByIds`.
 *
 * The provider never touches the product rooms TrUAPI Chat keeps, or the Nova
 * chat.
 */

export const chatV2ProviderId = (productId: string): string =>
  `chat-v2:${productId}`;

const STATUS: Record<ChatMessageStatus, MessageStatus> = {
  Preparing: "pending",
  Sent: "sent",
  Delivered: "delivered",
  Read: "read",
  Failed: "failed",
};

const RASTER = /^image\/(?:png|jpeg|gif|webp)$/;
const LOCAL = "local-";

/** What a refusal of the product says, for the screen. */
function refusalText(error: { tag: string; value?: unknown }): string {
  if (error.tag === "Domain") {
    const inner = error.value as { tag: string; value?: { reason?: string } };
    return inner.value?.reason ?? inner.tag;
  }
  if (error.tag === "Timeout") return "The product did not answer in time.";
  if (error.tag === "Cancelled") return "Sending stopped.";
  if (error.tag === "HostFailure")
    return (error.value as { reason: string }).reason;
  return `The product refused: ${error.tag}.`;
}

function dataUrl(resource: ChatResource | undefined): string | null {
  if (resource?.tag !== "Inline" || !RASTER.test(resource.value.mimeType))
    return null;
  let binary = "";
  for (const byte of hexToBytes(resource.value.bytes))
    binary += String.fromCharCode(byte);
  return `data:${resource.value.mimeType};base64,${btoa(binary)}`;
}

/** Why the host cannot open a resource, or null when it holds the bytes. */
function unavailable(resource: ChatResource): string | null {
  switch (resource.tag) {
    case "Inline":
      return null;
    case "Preimage":
      return "This web host does not fetch preimage media.";
    case "Sealed":
      return "This web host does not open sealed media.";
    case "Hop":
      return "This web host does not fetch HOP files.";
  }
}

function fileOf(item: ChatMediaItem): MessageFile {
  const { resource, meta } = item.value;
  const inline =
    resource.tag === "Inline" ? (resource.value.bytes.length - 2) / 2 : 0;
  return {
    name: item.tag === "File" ? item.value.fileName : (meta.alt ?? null),
    kind:
      item.tag === "Image" ? "image" : item.tag === "Video" ? "video" : "file",
    mimeType: meta.mimeType,
    sizeBytes: meta.sizeBytes === undefined ? inline : Number(meta.sizeBytes),
    ...(meta.width === undefined ? {} : { width: meta.width }),
    ...(meta.height === undefined ? {} : { height: meta.height }),
    unavailable: unavailable(resource),
  };
}

/**
 * The screen's content for a message body. A `Custom` body shows its
 * preview text: this host lists no `custom_bodies`, so a product that reads
 * the capabilities sends none.
 */
export function contentOf(body: ChatMessageBody): MessageContent {
  switch (body.tag) {
    case "Text":
      return { tag: "Text", text: body.value.text };
    case "Notice":
      return { tag: "System", text: body.value.text };
    case "RichText":
      return body.value.media.length === 0
        ? { tag: "Text", text: body.value.text ?? "" }
        : {
            tag: "Attachments",
            text: body.value.text ?? null,
            files: body.value.media.map(fileOf),
          };
    case "Actions":
      return {
        tag: "Actions",
        text: body.value.text ?? null,
        actions: body.value.actions.map(({ actionId, title }) => ({
          actionId,
          title,
        })),
        layout: body.value.layout,
      };
    case "Custom":
      return {
        tag: "Text",
        text: body.value.previewText ?? "Sent a message",
      };
  }
}

function previewText(body: ChatMessageBody): string {
  switch (body.tag) {
    case "Text":
    case "Notice":
      return body.value.text;
    case "RichText":
      return body.value.text ?? "Media";
    case "Actions":
      return body.value.text ?? "Sent a message";
    case "Custom":
      return body.value.previewText ?? "Sent a message";
  }
}

function participantLabel(participant: ChatParticipant): string {
  switch (participant.tag) {
    case "Custom":
      return participant.value.nickname;
    case "Product":
      return participant.value.productId;
    case "Person":
      return `Person ${participant.value.personId}`;
    case "LitePerson":
      return `${participant.value.account.slice(0, 10)}…`;
    case "Contact":
      return "Contact";
  }
}

/** A message the user sent that the product has not stored. */
interface Unsent {
  message: ChatMessage;
  body: ChatMessageBody;
  controller: AbortController | null;
}

/** Per room: names, the local ids of stored messages, and what is unsent. */
interface RoomState {
  labels: Map<string, string>;
  /** Product message id to the local id the screen first drew it under. */
  shownAs: Map<string, string>;
  unsent: Map<string, Unsent>;
  seen: string | null;
}

export class ChatV2Provider implements ChatProvider {
  readonly id: string;
  readonly label: string;
  readonly calls: V2Calls;
  readonly files: FileTransfer;
  private readonly listeners = new Listeners<ProviderEvent>();
  private readonly state = new Map<string, RoomState>();
  private readonly stop: () => void;
  private generation = 0;
  private nextLocal = 1;

  constructor(
    readonly productId: string,
    private readonly host: ChatV2Host,
    private readonly now: () => number = Date.now,
  ) {
    this.id = chatV2ProviderId(productId);
    this.label = `${productId} (Chat v2 draft)`;
    this.calls = new V2Calls(productId, host, (roomId) => this.title(roomId));
    this.files = this.fileTransfer();
    this.stop = host.onChange((change) => this.follow(change));
  }

  private emit(event: ProviderEvent): void {
    this.listeners.emit(event);
  }

  private follow(change: ChatV2Change): void {
    if (change.productId !== this.productId) return;
    switch (change.type) {
      case "rooms":
        this.emit({ type: "conversations" });
        return;
      case "link":
        for (const room of this.state.values())
          for (const entry of room.unsent.values()) entry.controller?.abort();
        this.state.clear();
        this.generation += 1;
        this.emit({ type: "status" });
        this.emit({ type: "conversations" });
        return;
      case "call":
        this.calls.changed();
        this.emit({ type: "conversations" });
        return;
      case "feed": {
        const { roomId, message, removed } = change;
        if (message === undefined) {
          if (removed !== undefined)
            this.roomState(roomId).shownAs.delete(removed);
          // The screen reads the newest page again and drops what it no longer holds.
          this.emit({ type: "messages", conversationId: roomId });
          return;
        }
        const epoch = this.generation;
        void this.named(roomId, [message]).then(([view]) => {
          if (epoch === this.generation)
            this.emit({
              type: "messages",
              conversationId: roomId,
              message: view,
            });
        });
        return;
      }
    }
  }

  private room(roomId: string): V2Room | undefined {
    return this.host.room(this.productId, roomId);
  }

  private roomState(roomId: string): RoomState {
    let state = this.state.get(roomId);
    if (state === undefined) {
      state = {
        labels: new Map(),
        shownAs: new Map(),
        unsent: new Map(),
        seen: null,
      };
      this.state.set(roomId, state);
    }
    return state;
  }

  /** The product's id for a message the screen names. */
  private productIdOf(roomId: string, shownId: string): string {
    for (const [stored, shown] of this.roomState(roomId).shownAs)
      if (shown === shownId) return stored;
    return shownId;
  }

  private title(roomId: string): string {
    return this.room(roomId)?.metadata.name ?? roomId;
  }

  status(): ProviderStatus {
    return this.host.connected(this.productId)
      ? { state: "ready" }
      : { state: "offline", reason: "The product's worker is not running." };
  }

  epoch(): number {
    return this.generation;
  }

  conversations(): ConversationSummary[] {
    return this.host.roomsOf(this.productId).map((room) => {
      const newest = this.host.cached(this.productId, room.roomId).at(-1);
      const badge = room.preview?.badge;
      return {
        id: room.roomId,
        title: room.metadata.name,
        subtitle: room.metadata.description ?? this.productId,
        icon: dataUrl(room.metadata.avatar),
        seed: this.productId,
        preview:
          room.preview?.text?.text ??
          (newest === undefined ? "" : previewText(newest.body)),
        updatedAt: Number(room.preview?.timestamp ?? newest?.timestamp ?? 0),
        ...(badge?.tag === "UnreadCount"
          ? { unread: badge.value.count }
          : badge?.tag === "Hidden"
            ? { unread: 0 }
            : {}),
      };
    });
  }

  composer(roomId: string): Composer {
    if (!this.host.connected(this.productId))
      return { kind: "blocked", reason: "The product's worker is not running" };
    if (!this.room(roomId)?.config.productSource)
      return {
        kind: "blocked",
        reason: "This draft host keeps no messages for this room",
      };
    return { kind: "text" };
  }

  /** Put names to the senders of `messages`, then draw them for the screen. */
  private async named(
    roomId: string,
    messages: V2Message[],
  ): Promise<ChatMessage[]> {
    const { labels } = this.roomState(roomId);
    const ids = [
      ...new Set(messages.map((message) => message.origin.participantId)),
    ].filter((id) => id !== "user" && id !== "product" && !labels.has(id));
    if (ids.length > 0 && this.room(roomId)?.config.participants) {
      const answer = await this.host.query(
        this.productId,
        "participantsQuery",
        {
          roomId,
          query: { tag: "ByIds", value: { participantIds: ids } },
        },
      );
      if (answer.isOk() && answer.value.tag !== "Count")
        for (const entry of answer.value.value.participants)
          labels.set(entry.participantId, participantLabel(entry.participant));
    }
    return messages.map((message) => this.view(roomId, message));
  }

  private view(roomId: string, message: V2Message): ChatMessage {
    const state = this.roomState(roomId);
    const id = message.origin.participantId;
    const self = id === "user";
    const label = self
      ? "You"
      : id === "product"
        ? this.productId
        : (state.labels.get(id) ?? id);
    return {
      id: state.shownAs.get(message.messageId) ?? message.messageId,
      at: Number(message.timestamp),
      sender: { id, label, self },
      status: self ? STATUS[message.status] : "received",
      content: contentOf(message.body),
      ...(message.reactions.length === 0
        ? {}
        : {
            reactions: message.reactions.map((reaction) => ({
              emoji: reaction.emoji,
              count: reaction.count,
              self: reaction.reactedByUser,
            })),
          }),
      ...(message.body.tag !== "Notice" && this.host.connected(this.productId)
        ? { reactable: true }
        : {}),
    };
  }

  async timeline(roomId: string, before?: string): Promise<TimelinePage> {
    if (!this.room(roomId)?.config.productSource) return { messages: [] };
    const epoch = this.generation;
    const answer = await this.host.query(this.productId, "messagesQuery", {
      roomId,
      anchor: {
        tag: "Before",
        value: before === undefined ? {} : { cursor: before },
      },
      pageSize: PAGE_SIZE,
    });
    if (answer.isErr())
      throw new Error(
        `The product did not give its messages: ${refusalText(answer.error)}`,
      );
    const messages = await this.named(roomId, answer.value.messages);
    const unsent =
      before === undefined && epoch === this.generation
        ? [...this.roomState(roomId).unsent.values()].map(
            (entry) => entry.message,
          )
        : [];
    return {
      messages: [...messages, ...unsent],
      ...(answer.value.next === undefined ? {} : { before: answer.value.next }),
    };
  }

  /** Text the user typed, sent as `user_action` `NewMessage`. */
  send(roomId: string, text: string): Promise<void> {
    return this.submit(roomId, { tag: "Text", value: { text } });
  }

  /**
   * Show `body` as pending at once under a local id, then ask the product to
   * store it. Its `MessageCreated` answer is drawn under the same id. Any
   * refusal, a `Rejected` one included, leaves it failed with the reason and
   * a retry.
   */
  private async submit(
    roomId: string,
    body: ChatMessageBody,
    localId = `${LOCAL}${this.nextLocal++}`,
  ): Promise<void> {
    const state = this.roomState(roomId);
    const controller = new AbortController();
    const message: ChatMessage = {
      id: localId,
      at: state.unsent.get(localId)?.message.at ?? this.now(),
      sender: { id: "user", label: "You", self: true },
      status: "pending",
      content: contentOf(body),
    };
    state.unsent.set(localId, { message, body, controller });
    this.emit({ type: "messages", conversationId: roomId, message });
    const answer = await this.host.query(
      this.productId,
      "userAction",
      { roomId, action: { tag: "NewMessage", value: { body } } },
      { signal: controller.signal },
    );
    if (state.unsent.get(localId)?.controller !== controller) return;
    if (answer.isOk() && answer.value.tag === "MessageCreated") {
      state.unsent.delete(localId);
      state.shownAs.set(answer.value.value.messageId, localId);
      const [stored] = await this.named(roomId, [answer.value.value]);
      this.emit({ type: "messages", conversationId: roomId, message: stored });
      return;
    }
    const problem = answer.isErr()
      ? refusalText(answer.error)
      : "The product did not store the message.";
    const failed: ChatMessage = { ...message, status: "failed", problem };
    state.unsent.set(localId, { message: failed, body, controller: null });
    this.emit({ type: "messages", conversationId: roomId, message: failed });
  }

  /** Send a failed message again: one the product never stored, or one it marked `Failed`. */
  async retry(roomId: string, messageId: string): Promise<void> {
    const unsent = this.roomState(roomId).unsent.get(messageId);
    if (unsent !== undefined)
      return this.submit(roomId, unsent.body, messageId);
    const answer = await this.host.query(this.productId, "userAction", {
      roomId,
      action: {
        tag: "Retry",
        value: { messageId: this.productIdOf(roomId, messageId) },
      },
    });
    if (answer.isErr()) throw new Error(refusalText(answer.error));
  }

  /** Tell the product the user saw this message, when the room asks for it. */
  markRead(roomId: string, throughMessageId: string): void {
    if (!this.room(roomId)?.config.reportSeen) return;
    const state = this.roomState(roomId);
    if (state.unsent.has(throughMessageId)) return;
    const messageId = this.productIdOf(roomId, throughMessageId);
    if (state.seen === messageId) return;
    state.seen = messageId;
    void this.host.query(this.productId, "userAction", {
      roomId,
      action: { tag: "Seen", value: { messageId } },
    });
  }

  /**
   * Set the user's one emoji on a message, or take it off. The change shows
   * at once and is taken back, with the reason, when the product refuses it.
   */
  async react(
    roomId: string,
    shownId: string,
    emoji: string | null,
  ): Promise<void> {
    const messageId = this.productIdOf(roomId, shownId);
    const before = this.host
      .cached(this.productId, roomId)
      .find((message) => message.messageId === messageId);
    if (before === undefined) throw new Error("This message is not loaded.");
    const current =
      before.reactions.find((reaction) => reaction.reactedByUser)?.emoji ??
      null;
    const placing = emoji === current ? null : emoji;
    const actions = [
      ...(placing === null
        ? []
        : [{ tag: "ReactionPlaced" as const, emoji: placing }]),
      ...(current === null
        ? []
        : [{ tag: "ReactionRemoved" as const, emoji: current }]),
    ];
    let after = before;
    for (const action of actions)
      after = withUserReaction(
        after,
        action.emoji,
        action.tag === "ReactionPlaced",
      );
    this.show(roomId, after);
    for (const action of actions) {
      const answer = await this.host.query(this.productId, "userAction", {
        roomId,
        action: { tag: action.tag, value: { messageId, emoji: action.emoji } },
      });
      if (answer.isErr()) {
        this.show(roomId, before);
        throw new Error(
          `The reaction was taken back: ${refusalText(answer.error)}`,
        );
      }
    }
  }

  private show(roomId: string, message: V2Message): void {
    this.host.remember(this.productId, roomId, message);
    this.emit({
      type: "messages",
      conversationId: roomId,
      message: this.view(roomId, message),
    });
  }

  /** Files the user sends inline, inside the bound every v2 host keeps. */
  private fileTransfer(): FileTransfer {
    return {
      blocked: (roomId) => {
        if (!this.host.connected(this.productId))
          return "The product's worker is not running.";
        return this.room(roomId)?.config.productSource
          ? null
          : "This draft host keeps no messages for this room.";
      },
      maxBytes: INLINE_MEDIA_MAX_BYTES,
      send: async (roomId, file, caption) => {
        if (file.size > INLINE_MEDIA_MAX_BYTES)
          throw new Error("The file is larger than this host sends inline.");
        const bytes = new Uint8Array(await file.arrayBuffer());
        const resource: ChatResource = {
          tag: "Inline",
          value: {
            mimeType: file.type || "application/octet-stream",
            bytes: bytesToHex(bytes),
          },
        };
        const meta = {
          mimeType: resource.value.mimeType,
          sizeBytes: BigInt(bytes.length),
        };
        const item: ChatMediaItem = file.type.startsWith("image/")
          ? { tag: "Image", value: { resource, meta } }
          : file.type.startsWith("video/")
            ? { tag: "Video", value: { resource, meta } }
            : { tag: "File", value: { resource, meta, fileName: file.name } };
        await this.submit(roomId, {
          tag: "RichText",
          value: {
            ...(caption.trim() === "" ? {} : { text: caption.trim() }),
            media: [item],
          },
        });
      },
      cancel: (roomId, messageId) =>
        this.roomState(roomId).unsent.get(messageId)?.controller?.abort(),
      open: async (roomId, shownId, index) => {
        const messageId = this.productIdOf(roomId, shownId);
        const body =
          this.roomState(roomId).unsent.get(shownId)?.body ??
          this.host
            .cached(this.productId, roomId)
            .find((message) => message.messageId === messageId)?.body;
        const item =
          body?.tag === "RichText" ? body.value.media[index] : undefined;
        if (item === undefined) throw new Error("This file is not loaded.");
        const resource = item.value.resource;
        if (resource.tag !== "Inline")
          throw new Error(unavailable(resource) ?? "Not available here.");
        return new Blob([new Uint8Array(hexToBytes(resource.value.bytes))], {
          type: resource.value.mimeType,
        });
      },
    };
  }

  subscribe(listener: (event: ProviderEvent) => void): () => void {
    return this.listeners.add(listener);
  }

  /** Stop following the rooms. The worker and its link belong to `ProductWorkers`. */
  dispose(): void {
    this.stop();
    this.calls.dispose();
    for (const room of this.state.values())
      for (const entry of room.unsent.values()) entry.controller?.abort();
    this.listeners.clear();
  }
}

/** A message with the user's reaction placed or removed, as the screen shows it at once. */
function withUserReaction(
  message: V2Message,
  emoji: string,
  placed: boolean,
): V2Message {
  const reactions = message.reactions.map((reaction) => ({ ...reaction }));
  const entry = reactions.find((reaction) => reaction.emoji === emoji);
  if (placed && entry === undefined)
    reactions.push({ emoji, count: 1, reactedByUser: true });
  else if (placed && entry !== undefined && !entry.reactedByUser) {
    entry.count += 1;
    entry.reactedByUser = true;
  } else if (!placed && entry?.reactedByUser) {
    entry.count -= 1;
    entry.reactedByUser = false;
  }
  return {
    ...message,
    reactions: reactions.filter((reaction) => reaction.count > 0),
  };
}
