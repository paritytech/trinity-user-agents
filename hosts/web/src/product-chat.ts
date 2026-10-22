import type { ChatPlatform, ProductContext } from "@parity/truapi-host";
import type {
  ChatRoomFooter,
  HexString,
  HostChatListSubscribeItem,
  HostChatPostMessageRequest,
  HostChatPostMessageResponse,
} from "@parity/truapi";
import { subscription } from "./subscription.js";

/**
 * The rooms product workers keep in this host's Chat, and the host side of
 * TrUAPI Chat that writes them.
 *
 * A product's worker creates its rooms and posts into them through the core,
 * which checks the caller is a worker and screens what it sends: text, rich
 * text with media, action buttons, file links, custom messages, and its own
 * reactions to its messages. These rooms
 * are the products' own: they are kept apart from the person-to-person
 * conversations of the host's Nova chat, per network and wallet, in this
 * browser's `localStorage`.
 */

export interface ProductRoom {
  productId: string;
  roomId: string;
  name: string;
  /** As the product gave it: an `https` URL or an inline raster image. */
  icon: string;
  footer: ChatRoomFooter;
  createdAt: number;
}

export type ProductMessageContent =
  | { tag: "Text"; text: string }
  | { tag: "RichText"; text: string | null; media: string[] }
  | {
      tag: "Actions";
      text: string | null;
      actions: { actionId: string; title: string }[];
      layout: "Column" | "Grid";
    }
  | {
      tag: "File";
      url: string;
      fileName: string;
      mimeType: string;
      /** As the product gave it, not checked against the file. */
      sizeBytes: number;
      text: string | null;
    }
  | {
      tag: "Custom";
      messageType: string;
      payload: HexString;
      alt: string | null;
    };

export interface ProductMessage {
  productId: string;
  roomId: string;
  messageId: string;
  /** `in` from the product, `out` typed by the user in the room. */
  direction: "in" | "out";
  at: number;
  content: ProductMessageContent;
  /** The emoji the product reacted to this message with, in the order it placed them. */
  reactions?: string[];
}

interface Record {
  rooms: ProductRoom[];
  messages: ProductMessage[];
}

/** Messages kept per room; older ones are dropped first. */
export const ROOM_MESSAGE_LIMIT = 200;

export const productChatKey = (network: string, walletId: string): string =>
  `truapi-web-signing-host:network:${network}:product-chat:${walletId}:v1`;

export const NOT_SIGNED_IN = "Chat needs a signed-in wallet.";
export const BOTS_UNSUPPORTED =
  "This host does not register chat bots. Create a room instead.";

/** What the room keeps of a message the product posted. */
function contentOf(
  payload: Exclude<
    HostChatPostMessageRequest["payload"],
    { tag: "Reaction" | "ReactionRemoved" }
  >,
  alt: string | null,
): ProductMessageContent {
  switch (payload.tag) {
    case "Text":
      return { tag: "Text", text: payload.value.text };
    case "RichText":
      return {
        tag: "RichText",
        text: payload.value.text ?? null,
        media: payload.value.media.map((item) => item.url),
      };
    case "Actions":
      return {
        tag: "Actions",
        text: payload.value.text ?? null,
        actions: payload.value.actions.map(({ actionId, title }) => ({
          actionId,
          title,
        })),
        layout: payload.value.layout,
      };
    case "File":
      return {
        tag: "File",
        url: payload.value.url,
        fileName: payload.value.fileName,
        mimeType: payload.value.mimeType,
        sizeBytes: Number(payload.value.sizeBytes),
        text: payload.value.text ?? null,
      };
    case "Custom":
      return {
        tag: "Custom",
        messageType: payload.value.messageType,
        payload: payload.value.payload,
        alt,
      };
  }
}

const sameRoom =
  (productId: string, roomId: string) =>
  (entry: { productId: string; roomId: string }): boolean =>
    entry.productId === productId && entry.roomId === roomId;

export class ProductChat {
  private key: string | null = null;
  private record: Record = { rooms: [], messages: [] };
  private readonly listeners = new Set<() => void>();
  private readonly roomListeners = new Map<string, Set<() => void>>();
  private nextId = 1;

  constructor(
    private readonly storage: Storage,
    private readonly now: () => number = Date.now,
  ) {}

  /** Follow the signed-in wallet and network, or none. */
  setScope(key: string | null): void {
    if (key === this.key) return;
    this.key = key;
    this.record = key === null ? { rooms: [], messages: [] } : this.load(key);
    this.changed(null);
  }

  private load(key: string): Record {
    const raw = this.storage.getItem(key);
    if (raw === null) return { rooms: [], messages: [] };
    try {
      const parsed = JSON.parse(raw) as Partial<Record>;
      if (!Array.isArray(parsed.rooms) || !Array.isArray(parsed.messages))
        throw new Error("not a record");
      return { rooms: parsed.rooms, messages: parsed.messages };
    } catch {
      this.storage.setItem(`${key}.unreadable`, raw);
      return { rooms: [], messages: [] };
    }
  }

  /** Save, and tell whoever watches. `productId` names the product whose rooms changed. */
  private changed(productId: string | null): void {
    if (this.key !== null)
      this.storage.setItem(this.key, JSON.stringify(this.record));
    for (const listener of this.listeners) listener();
    const products =
      productId === null ? [...this.roomListeners.keys()] : [productId];
    for (const product of products)
      for (const listener of this.roomListeners.get(product) ?? []) listener();
  }

  onChange(listener: () => void): () => void {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  }

  rooms(): readonly ProductRoom[] {
    return this.record.rooms;
  }

  room(productId: string, roomId: string): ProductRoom | undefined {
    return this.record.rooms.find(sameRoom(productId, roomId));
  }

  messages(productId: string, roomId: string): ProductMessage[] {
    return this.record.messages.filter(sameRoom(productId, roomId));
  }

  private requireScope(): void {
    if (this.key === null) throw new Error(NOT_SIGNED_IN);
  }

  private append(message: ProductMessage): void {
    this.record.messages.push(message);
    const inRoom = this.record.messages.filter(
      sameRoom(message.productId, message.roomId),
    );
    if (inRoom.length > ROOM_MESSAGE_LIMIT) {
      const drop = new Set(inRoom.slice(0, inRoom.length - ROOM_MESSAGE_LIMIT));
      this.record.messages = this.record.messages.filter(
        (entry) => !drop.has(entry),
      );
    }
  }

  /** Keep what the user typed in a room, as the room shows it. */
  addOutgoing(productId: string, roomId: string, text: string): void {
    this.requireScope();
    this.append({
      productId,
      roomId,
      messageId: `user-${this.now()}-${this.nextId++}`,
      direction: "out",
      at: this.now(),
      content: { tag: "Text", text },
    });
    this.changed(null);
  }

  /** Forget every room and message of a product, as when its worker is removed. */
  forgetProduct(productId: string): void {
    this.record.rooms = this.record.rooms.filter(
      (room) => room.productId !== productId,
    );
    this.record.messages = this.record.messages.filter(
      (message) => message.productId !== productId,
    );
    this.changed(productId);
  }

  /**
   * Place or remove the product's reaction on one of the room's messages.
   * The product learns only its own messages' ids, so those are the ones it
   * can name; any other id is refused rather than kept for a message that
   * is not here.
   */
  private react(
    productId: string,
    roomId: string,
    messageId: string,
    emoji: string,
    placed: boolean,
  ): void {
    const message = this.record.messages.find(
      (entry) =>
        sameRoom(productId, roomId)(entry) && entry.messageId === messageId,
    );
    if (message === undefined)
      throw new Error(`There is no message ${messageId} in room ${roomId}.`);
    const reactions = (message.reactions ?? []).filter(
      (entry) => entry !== emoji,
    );
    message.reactions = placed ? [...reactions, emoji] : reactions;
    this.changed(null);
  }

  private roomsOf(productId: string): HostChatListSubscribeItem {
    return {
      rooms: this.record.rooms
        .filter((room) => room.productId === productId)
        .map((room) => ({ roomId: room.roomId, participatingAs: "RoomHost" })),
    };
  }

  /** The host side of TrUAPI Chat, for `createHostCallbacks`. */
  readonly platform: Required<ChatPlatform> = {
    createChatRoom: async (product: ProductContext, request) => {
      this.requireScope();
      const existing = this.room(product.productId, request.roomId);
      if (existing !== undefined) {
        existing.name = request.name;
        existing.icon = request.icon;
        this.changed(product.productId);
        return { status: "Exists" };
      }
      this.record.rooms.push({
        productId: product.productId,
        roomId: request.roomId,
        name: request.name,
        icon: request.icon,
        footer: "TextInput",
        createdAt: this.now(),
      });
      this.changed(product.productId);
      return { status: "New" };
    },
    registerChatBot: async () => {
      throw new Error(BOTS_UNSUPPORTED);
    },
    postChatMessage: async (
      product,
      request,
    ): Promise<HostChatPostMessageResponse> => {
      this.requireScope();
      if (this.room(product.productId, request.roomId) === undefined)
        throw new Error(`There is no room ${request.roomId} for this product.`);
      const payload = request.payload;
      const messageId = `m-${this.now().toString(36)}-${(this.nextId++).toString(36)}`;
      if (payload.tag === "Reaction" || payload.tag === "ReactionRemoved") {
        this.react(
          product.productId,
          request.roomId,
          payload.value.messageId,
          payload.value.emoji,
          payload.tag === "Reaction",
        );
        return { messageId };
      }
      this.append({
        productId: product.productId,
        roomId: request.roomId,
        messageId,
        direction: "in",
        at: this.now(),
        content: contentOf(payload, request.alt ?? null),
      });
      this.changed(null);
      return { messageId };
    },
    setChatRoomFooter: async (product, request) => {
      this.requireScope();
      const room = this.room(product.productId, request.roomId);
      if (room === undefined)
        throw new Error(`There is no room ${request.roomId} for this product.`);
      room.footer = request.footer;
      this.changed(product.productId);
    },
    subscribeChatRooms: (product) =>
      subscription((emit) => {
        const send = () => emit(this.roomsOf(product.productId));
        let listeners = this.roomListeners.get(product.productId);
        if (listeners === undefined) {
          listeners = new Set();
          this.roomListeners.set(product.productId, listeners);
        }
        listeners.add(send);
        send();
        return () => listeners.delete(send);
      }),
  };
}
