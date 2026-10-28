import type { HexString } from "@parity/truapi/scale";
import type {
  ChatRoomContact,
  ChatMessage,
  ChatMessageBody,
  ChatMessageStatus,
  ChatRoomPreview,
} from "./contract.js";
import type { ChatV2Product } from "./product.js";

/**
 * The seam between a chat engine and a chat product's worker.
 *
 * A {@link ConversationEngine} owns its protocol, its keys or key access,
 * and its state; it answers in conversations and text messages. The worker
 * hands any such engine to {@link serveConversations}, which shows it through
 * the Chat Extension v2 draft bridge (`product.ts`): one room per
 * conversation, pages of history on request, and the user's sends, retries
 * and reads back to the engine.
 *
 * The Nova engine is one implementation (`../worker/nova-conversations.ts`). An
 * engine over libsignal or OpenMLS would be another, with its own identity
 * and its own idea of who is in a conversation; nothing here assumes Nova's
 * keys, handshake or pairwise sessions.
 */

/** Who wrote a message. */
export type Author =
  | { kind: "self" }
  | { kind: "participant"; id: string }
  /** A line about the conversation, such as "Ada accepted the request". */
  | { kind: "system" };

export type MessageState =
  | "sending"
  | "sent"
  | "delivered"
  | "read"
  | "failed"
  | "received";

export interface ConversationMessage {
  /** Unique in its conversation and stable for the message's life. */
  id: string;
  author: Author;
  text: string;
  /** Milliseconds since the Unix epoch. */
  at: number;
  state: MessageState;
}

export interface Participant {
  id: string;
  /** What the engine calls them, shown only when the host has no name for them. */
  name: string;
  /** Their public account, when the engine's protocol addresses people by one. The host names them from it. */
  account?: string;
}

/** Where a conversation with one person stands; the names of the v2 draft's contact states. */
export type ContactState =
  | "Outgoing"
  | "Incoming"
  | "Established"
  | "Blocked"
  | "PeerLeft";

export interface Conversation {
  id: string;
  title: string;
  subtitle?: string;
  participants: Participant[];
  /** Whether the user can write here now, and why not. */
  composer: { allowed: true } | { allowed: false; reason: string };
  /**
   * The one person a conversation is with, for an engine that has
   * person-to-person conversations. The host names and draws them from the
   * account; the engine's title is then only a fallback.
   */
  contact?: { account: string; state: ContactState };
}

export interface ConversationEngine {
  conversations(): Conversation[];
  /** Oldest first. */
  messages(conversationId: string): ConversationMessage[];
  /** Send text; resolves to the new message's id once the engine holds it. */
  send(conversationId: string, text: string): Promise<string>;
  /** Send a failed message again. */
  retry(conversationId: string, messageId: string): Promise<void>;
  /**
   * Start a conversation with the person `contact` refers to: an opaque
   * reference the host made when the user chose someone. What the engine
   * does to reach them is its own: resolve the reference with a service of
   * the host, and send a request. Resolves to the conversation's id.
   * Absent: the engine starts no conversations.
   */
  start?(contact: string, welcome: string): Promise<string>;
  onChange(listener: () => void): () => void;
}

/** Where read marks are kept: the worker's own storage. */
export interface ReadMarks {
  get(conversationId: string): string | null;
  set(conversationId: string, messageId: string): void;
}

export interface ServeOptions {
  reads: ReadMarks;
  /** Told of every bridge call that failed, for the worker's status. */
  onError?(what: string, reason: string): void;
}

/** A running adapter, until {@link stop}. */
export interface ServedConversations {
  /** Resolves once every conversation is declared. */
  ready: Promise<void>;
  stop(): void;
}

const STATUS: Record<MessageState, ChatMessageStatus> = {
  sending: "Preparing",
  sent: "Sent",
  delivered: "Delivered",
  read: "Read",
  failed: "Failed",
  received: "Delivered",
};

function toV2(message: ConversationMessage): ChatMessage {
  const author = message.author;
  return {
    messageId: message.id,
    origin: {
      participantId:
        author.kind === "self"
          ? "user"
          : author.kind === "system"
            ? "product"
            : author.id,
    },
    timestamp: BigInt(message.at),
    body:
      author.kind === "system"
        ? { tag: "Notice", value: { text: message.text } }
        : { tag: "Text", value: { text: message.text } },
    status: STATUS[message.state],
    reactions: [],
  };
}

const contactOf = (contact: {
  account: string;
  state: ContactState;
}): ChatRoomContact => ({
  account: contact.account as HexString,
  state: contact.state,
});

const json = (value: unknown) =>
  JSON.stringify(value, (_key, entry) =>
    typeof entry === "bigint" ? String(entry) : entry,
  );

/** What was last declared of a room, to send only what changed. */
interface Declared {
  metadata: string;
  preview: string;
  contact: string;
  states: Map<string, MessageState>;
}

/**
 * Show `engine` through the v2 draft bridge until {@link ServedConversations.stop}.
 * Rooms follow the engine's conversations; their history is read from the
 * engine on each page request, not copied into the host.
 */
export function serveConversations(
  chat: ChatV2Product,
  engine: ConversationEngine,
  options: ServeOptions,
): ServedConversations {
  const declared = new Map<string, Declared>();
  let stopped = false;
  let queue: Promise<void> = Promise.resolve();
  let pending = false;

  const failed = (
    what: string,
    result: { isErr(): boolean; error?: unknown },
  ) => {
    if (result.isErr()) options.onError?.(what, json(result.error));
    return result.isErr();
  };

  const find = (conversationId: string) =>
    engine.conversations().find((entry) => entry.id === conversationId);

  /** Others' messages after the read mark, or after the user's own newest message. */
  const unread = (conversationId: string): number => {
    const messages = engine.messages(conversationId);
    const mark = options.reads.get(conversationId);
    const marked =
      mark === null ? -1 : messages.findIndex((m) => m.id === mark);
    const answered = messages.findLastIndex((m) => m.author.kind === "self");
    return messages
      .slice(Math.max(marked, answered) + 1)
      .filter((message) => message.author.kind === "participant").length;
  };

  const previewOf = (conversation: Conversation): ChatRoomPreview => {
    const newest = engine.messages(conversation.id).at(-1);
    return {
      order: "Timestamp",
      ...(newest === undefined ? {} : { timestamp: BigInt(newest.at) }),
      badge: {
        tag: "UnreadCount",
        value: { count: unread(conversation.id) },
      },
      ...(newest === undefined
        ? {}
        : {
            text: {
              text: `${newest.author.kind === "self" ? "You: " : ""}${newest.text}`,
              ongoing: false,
            },
          }),
    };
  };

  const metadataOf = (conversation: Conversation) => ({
    name: conversation.title,
    ...(conversation.subtitle === undefined
      ? {}
      : { description: conversation.subtitle }),
  });

  /** Bring the declared rooms and their feeds in line with the engine. */
  const sync = async (): Promise<void> => {
    const conversations = engine.conversations();
    const live = new Set(conversations.map((entry) => entry.id));
    for (const roomId of [...declared.keys()])
      if (!live.has(roomId)) {
        declared.delete(roomId);
        failed(`deleteRoom ${roomId}`, await chat.deleteRoom({ roomId }));
      }
    for (const conversation of conversations) {
      if (stopped) return;
      const roomId = conversation.id;
      const metadata = metadataOf(conversation);
      const preview = previewOf(conversation);
      let room = declared.get(roomId);
      if (room === undefined) {
        const created = await chat.createRoom({
          roomId,
          metadata,
          config: {
            productSource: true,
            reactionDetails: false,
            participants: true,
            messageActions: false,
            reportSeen: true,
            ...(conversation.composer.allowed
              ? {}
              : { placeholder: conversation.composer.reason }),
          },
          preview,
        });
        if (failed(`createRoom ${roomId}`, created)) continue;
        if (conversation.contact !== undefined)
          failed(
            `setRoomContact ${roomId}`,
            await chat.setRoomContact({
              roomId,
              contact: contactOf(conversation.contact),
            }),
          );
        room = {
          metadata: json(metadata),
          preview: json(preview),
          contact: json(conversation.contact),
          states: new Map(
            engine
              .messages(roomId)
              .map((message) => [message.id, message.state]),
          ),
        };
        declared.set(roomId, room);
        continue;
      }
      if (json(metadata) !== room.metadata) {
        room.metadata = json(metadata);
        failed(
          `setRoomMetadata ${roomId}`,
          await chat.setRoomMetadata({ roomId, metadata }),
        );
      }
      if (
        conversation.contact !== undefined &&
        json(conversation.contact) !== room.contact
      ) {
        room.contact = json(conversation.contact);
        failed(
          `setRoomContact ${roomId}`,
          await chat.setRoomContact({
            roomId,
            contact: contactOf(conversation.contact),
          }),
        );
      }
      const messages = engine.messages(roomId);
      const present = new Set(messages.map((message) => message.id));
      for (const id of [...room.states.keys()])
        if (!present.has(id)) {
          room.states.delete(id);
          failed(
            `updateFeed Deleted ${roomId}`,
            await chat.updateFeed({
              roomId,
              update: { tag: "Deleted", value: { messageId: id } },
            }),
          );
        }
      for (const message of messages) {
        const known = room.states.get(message.id);
        if (known === message.state) continue;
        // The user's own send reaches the host as its MessageCreated answer.
        if (
          known === undefined &&
          message.author.kind === "self" &&
          message.state === "sending"
        )
          continue;
        room.states.set(message.id, message.state);
        failed(
          `updateFeed ${roomId}`,
          await chat.updateFeed({
            roomId,
            update:
              known === undefined
                ? { tag: "New", value: toV2(message) }
                : {
                    tag: "StatusChanged",
                    value: {
                      messageId: message.id,
                      status: STATUS[message.state],
                    },
                  },
          }),
        );
      }
      if (json(preview) !== room.preview) {
        room.preview = json(preview);
        failed(
          `setRoomPreview ${roomId}`,
          await chat.setRoomPreview({ roomId, preview }),
        );
      }
    }
  };

  /** Run one sync after the current one, however many changes asked for it. */
  const schedule = (): Promise<void> => {
    if (pending) return queue;
    pending = true;
    queue = queue
      .then(() => {
        pending = false;
        return stopped ? undefined : sync();
      })
      .catch((error: unknown) =>
        options.onError?.(
          "sync",
          error instanceof Error ? error.message : String(error),
        ),
      );
    return queue;
  };

  chat.onMessagesQuery(({ roomId, anchor, pageSize }, send, interrupt) => {
    if (find(roomId) === undefined)
      return interrupt({ tag: "Domain", value: { tag: "UnknownRoom" } });
    const all = engine.messages(roomId);
    const cursor = anchor.value.cursor;
    const at =
      cursor === undefined ? -1 : all.findIndex((m) => m.id === cursor);
    if (cursor !== undefined && at < 0)
      return interrupt({
        tag: "Domain",
        value: { tag: "Unknown", value: { reason: "That page is gone." } },
      });
    if (anchor.tag === "Before") {
      const end = cursor === undefined ? all.length : at;
      const begin = Math.max(0, end - pageSize);
      return send({
        messages: all.slice(begin, end).map(toV2),
        ...(begin > 0 ? { next: all[begin].id } : {}),
      });
    }
    const begin = at + 1;
    const end = Math.min(all.length, begin + pageSize);
    send({
      messages: all.slice(begin, end).map(toV2),
      ...(end < all.length ? { next: all[end - 1].id } : {}),
    });
  });

  chat.onParticipantsQuery(({ roomId, query }, send, interrupt) => {
    const conversation = find(roomId);
    if (conversation === undefined)
      return interrupt({ tag: "Domain", value: { tag: "UnknownRoom" } });
    const entry = (participant: Participant) => ({
      participantId: participant.id,
      participant:
        participant.account === undefined
          ? {
              tag: "Custom" as const,
              value: { nickname: participant.name },
            }
          : {
              tag: "LitePerson" as const,
              value: { account: participant.account as HexString },
            },
    });
    const people = conversation.participants;
    if (query.tag === "Count")
      return send({ tag: "Count", value: { count: people.length + 1 } });
    if (query.tag === "Page")
      return send({ tag: "Page", value: { participants: people.map(entry) } });
    const wanted = new Set(query.value.participantIds);
    send({
      tag: "Resolved",
      value: {
        participants: people
          .filter((person) => wanted.has(person.id))
          .map(entry),
      },
    });
  });

  const rejected = (reason: string) => ({
    tag: "Domain" as const,
    value: { tag: "Rejected" as const, value: { reason } },
  });

  const start = engine.start;
  if (start !== undefined)
    chat.onStartConversation(async ({ contact, welcome }, send, interrupt) => {
      let roomId: string;
      try {
        roomId = await start(contact, welcome ?? "");
      } catch (error) {
        return interrupt(
          rejected(error instanceof Error ? error.message : String(error)),
        );
      }
      // The room exists, with its contact, before the host opens it.
      await schedule();
      send({ roomId });
    });

  chat.onUserAction(async ({ roomId, action }, send, interrupt) => {
    const conversation = find(roomId);
    if (conversation === undefined)
      return interrupt({ tag: "Domain", value: { tag: "UnknownRoom" } });
    switch (action.tag) {
      case "NewMessage": {
        const body: ChatMessageBody = action.value.body;
        if (body.tag !== "Text")
          return interrupt(rejected("This chat sends text only for now."));
        let id: string;
        try {
          id = await engine.send(roomId, body.value.text);
        } catch (error) {
          return interrupt(
            rejected(error instanceof Error ? error.message : String(error)),
          );
        }
        const message = engine
          .messages(roomId)
          .find((entry) => entry.id === id);
        if (message === undefined)
          return interrupt(rejected("The message was not kept."));
        options.reads.set(roomId, id);
        const room = declared.get(roomId);
        room?.states.set(id, message.state);
        send({ tag: "MessageCreated", value: toV2(message) });
        void schedule();
        return;
      }
      case "Retry":
        try {
          await engine.retry(roomId, action.value.messageId);
        } catch (error) {
          return interrupt(
            rejected(error instanceof Error ? error.message : String(error)),
          );
        }
        send({ tag: "Acknowledged" });
        return;
      case "Seen": {
        const messages = engine.messages(roomId);
        const index = messages.findIndex(
          (entry) => entry.id === action.value.messageId,
        );
        const mark = options.reads.get(roomId);
        const current =
          mark === null ? -1 : messages.findIndex((entry) => entry.id === mark);
        if (index > current) options.reads.set(roomId, action.value.messageId);
        send({ tag: "Acknowledged" });
        void schedule();
        return;
      }
      default:
        return interrupt(rejected("This chat does not support that yet."));
    }
  });

  const unsubscribe = engine.onChange(() => void schedule());
  return {
    ready: schedule(),
    stop() {
      stopped = true;
      unsubscribe();
    },
  };
}
