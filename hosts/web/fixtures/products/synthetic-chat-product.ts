/**
 * A synthetic chat product, written against the Chat Extension v2 draft
 * bridge's product API (`src/chat-v2/product.ts`). It is a test fixture: its
 * people (Ada, Grace, Lin) and messages are made up and kept in memory, it
 * plays the network with timers, and nothing leaves it. It is not a chat
 * engine and speaks no peer protocol.
 *
 * Two places run it:
 * - `fixtures/products/chat-v2-product-worker.ts`, the product's worker run by the
 *   host's ordinary worker loader, as a `Worker` execution;
 * - `browser/support/chat-screen-harness.ts`, in the page, over a `MessageChannel`.
 *
 * It owns three rooms: a direct chat with Ada (120 messages, call buttons), a
 * group with Ada and Grace, and a direct chat with Lin with three unread
 * messages. A sent message goes from sent to delivered, and in a direct chat
 * the peer reads it and answers. A message with `[fail]` in it fails once
 * and can be retried; `reject me` is refused. The unread badge is the
 * product's, from the newest message the host reported seen.
 */
import { bytesToHex } from "@parity/truapi/scale";
import type {
  ChatMessage,
  ChatMessageBody,
  ChatRoomFeatures,
  HostChatCapabilitiesResponse,
} from "../../src/chat-v2/contract.js";
import type { ChatV2Product } from "../../src/chat-v2/product.js";

const PEOPLE: Record<string, string> = {
  ada: "Ada",
  grace: "Grace",
  lin: "Lin",
};

const PHRASES = [
  "Did the build pass?",
  "Yes, all green.",
  "Can you look at the layout?",
  "Sure, after lunch.",
  "The page loads faster now.",
  "Nice. Ship it?",
];

const NO_FEATURES: ChatRoomFeatures = {
  preimageAttachments: false,
  search: false,
  readAloud: false,
  sealedAttachments: false,
  hopAttachments: false,
};

interface Room {
  name: string;
  description?: string;
  /** The peer who answers, in a direct chat. */
  peer: string | null;
  features: ChatRoomFeatures;
  messages: ChatMessage[];
  /** The newest message the user has seen. */
  readThrough: string | null;
}

/** What the product saw, for a test to read. */
export interface SyntheticSeen {
  ready: boolean;
  capabilities: HostChatCapabilitiesResponse | null;
  rooms: string[];
  queries: { roomId: string; anchor: unknown; pageSize: number }[];
  actions: { roomId: string; action: { tag: string; value?: unknown } }[];
  events: { roomId: string; event: { tag: string; value?: unknown } }[];
  media: { callId: string; offerer: boolean; ok: boolean; error: unknown }[];
  errors: string[];
}

export interface SyntheticChatProduct {
  seen: SyntheticSeen;
  /** Resolves once every room is declared. */
  ready: Promise<void>;
  /** Ada calls the user. */
  ring(roomId: string, video: boolean): Promise<void>;
  /** Ada edits one of her messages. */
  edit(roomId: string, messageId: string, text: string): Promise<void>;
  /** A message is deleted. */
  remove(roomId: string, messageId: string): Promise<void>;
  /** Stop every timer, as when the worker stops. */
  stop(): void;
}

export interface SyntheticOptions {
  /** The gap between one delivery step and the next. */
  stepMs?: number;
  now?(): number;
}

const json = (value: unknown) =>
  JSON.stringify(value, (_key, entry) =>
    typeof entry === "bigint" ? String(entry) : entry,
  );

const text = (value: string): ChatMessageBody => ({
  tag: "Text",
  value: { text: value },
});

export function startSyntheticChatProduct(
  chat: ChatV2Product,
  options: SyntheticOptions = {},
): SyntheticChatProduct {
  const stepMs = options.stepMs ?? 400;
  const now = options.now ?? Date.now;
  const timers = new Set<ReturnType<typeof setTimeout>>();
  let stopped = false;
  const later = (task: () => void | Promise<void>, delayMs = stepMs) => {
    const timer = setTimeout(() => {
      timers.delete(timer);
      if (!stopped) void task();
    }, delayMs);
    timers.add(timer);
  };

  const seen: SyntheticSeen = {
    ready: false,
    capabilities: null,
    rooms: [],
    queries: [],
    actions: [],
    events: [],
    media: [],
    errors: [],
  };
  const record = (
    what: string,
    result: { isErr(): boolean; error?: unknown },
  ) => {
    if (result.isErr()) seen.errors.push(`${what}: ${json(result.error)}`);
  };

  const start = now() - 3 * 24 * 60 * 60 * 1000;
  const history = (
    roomId: string,
    count: number,
    senders: string[],
    spacingMs: number,
    offsetMs = 0,
  ): ChatMessage[] =>
    Array.from({ length: count }, (_, index) => {
      const sender = senders[index % senders.length];
      return {
        messageId: `${roomId}-${index + 1}`,
        origin: { participantId: sender },
        timestamp: BigInt(start + offsetMs + index * spacingMs),
        body: text(`#${index + 1} ${PHRASES[(index + 1) % PHRASES.length]}`),
        status: sender === "user" ? "Read" : "Delivered",
        reactions: [],
      };
    });

  const design = history("design", 9, ["ada", "grace", "user"], 3_600_000);
  design.splice(1, 0, {
    messageId: "design-notice",
    origin: { participantId: "product" },
    timestamp: design[0].timestamp + 1n,
    body: { tag: "Notice", value: { text: "Grace joined the group" } },
    status: "Sent",
    reactions: [],
  });
  const lin = history("lin", 7, ["lin", "user"], 600_000, 2 * 86_400_000);
  const rooms: Record<string, Room> = {
    ada: {
      name: "Ada",
      description: "Made-up contact · synthetic chat product",
      peer: "ada",
      features: { ...NO_FEATURES, calls: { audio: true, video: true } },
      messages: history("ada", 120, ["ada", "user"], 60_000),
      readThrough: "ada-120",
    },
    design: {
      name: "Design group",
      description: "Ada, Grace and you · made-up group",
      peer: null,
      features: NO_FEATURES,
      messages: design,
      readThrough: "design-9",
    },
    lin: {
      name: "Lin",
      description: "Made-up contact · synthetic chat product",
      peer: "lin",
      features: NO_FEATURES,
      messages: lin,
      // The user has not seen Lin's last three messages.
      readThrough: "lin-2",
    },
  };

  const unread = (room: Room) => {
    const at = room.messages.findIndex((m) => m.messageId === room.readThrough);
    return room.messages
      .slice(at + 1)
      .filter((message) => message.origin.participantId !== "user").length;
  };

  const previewLine = (room: Room, message: ChatMessage) => {
    const id = message.origin.participantId;
    const author =
      id === "user"
        ? "You: "
        : room.peer === null && id in PEOPLE
          ? `${PEOPLE[id]}: `
          : "";
    const body = message.body;
    return `${author}${body.tag === "Text" || body.tag === "Notice" ? body.value.text : "Sent a message"}`;
  };

  const preview = async (roomId: string) => {
    const room = rooms[roomId];
    const newest = room.messages.at(-1);
    record(
      `setRoomPreview ${roomId}`,
      await chat.setRoomPreview({
        roomId,
        preview: {
          order: "Timestamp",
          ...(newest === undefined ? {} : { timestamp: newest.timestamp }),
          badge: { tag: "UnreadCount", value: { count: unread(room) } },
          ...(newest === undefined
            ? {}
            : { text: { text: previewLine(room, newest), ongoing: false } }),
        },
      }),
    );
  };

  const feed = async (
    roomId: string,
    update: Parameters<ChatV2Product["updateFeed"]>[0]["update"],
  ) =>
    record(
      `updateFeed ${update.tag}`,
      await chat.updateFeed({ roomId, update }),
    );

  const status = (
    roomId: string,
    message: ChatMessage,
    next: ChatMessage["status"],
  ) => {
    message.status = next;
    return feed(roomId, {
      tag: "StatusChanged",
      value: { messageId: message.messageId, status: next },
    });
  };

  const failedOnce = new Set<string>();
  /** Play the network: sent, then delivered, then the peer reads and answers. */
  const deliver = (roomId: string, message: ChatMessage, said: string) => {
    const room = rooms[roomId];
    later(async () => {
      if (said.includes("[fail]") && !failedOnce.has(message.messageId)) {
        failedOnce.add(message.messageId);
        await status(roomId, message, "Failed");
        return;
      }
      await status(roomId, message, "Sent");
      later(async () => {
        await status(roomId, message, "Delivered");
        if (room.peer === null) return;
        later(async () => {
          for (const mine of room.messages)
            if (
              mine.origin.participantId === "user" &&
              mine.status === "Delivered"
            )
              await status(roomId, mine, "Read");
          const answer: ChatMessage = {
            messageId: `${message.messageId}-answer`,
            origin: { participantId: room.peer! },
            timestamp: BigInt(now()),
            body: text(`Got it: "${said}"`),
            status: "Delivered",
            reactions: [],
          };
          room.messages.push(answer);
          await feed(roomId, { tag: "New", value: answer });
          await preview(roomId);
        });
      });
    });
  };

  chat.onMessagesQuery(({ roomId, anchor, pageSize }, send, interrupt) => {
    seen.queries.push({ roomId, anchor, pageSize });
    const room = rooms[roomId];
    if (room === undefined)
      return interrupt({ tag: "Domain", value: { tag: "UnknownRoom" } });
    const all = room.messages;
    const cursor = anchor.value.cursor;
    const at =
      cursor === undefined ? -1 : all.findIndex((m) => m.messageId === cursor);
    if (cursor !== undefined && at < 0)
      return interrupt({
        tag: "Domain",
        value: { tag: "Unknown", value: { reason: "That page is gone." } },
      });
    if (anchor.tag === "Before") {
      const end = cursor === undefined ? all.length : at;
      const begin = Math.max(0, end - pageSize);
      return send({
        messages: all.slice(begin, end),
        ...(begin > 0 ? { next: all[begin].messageId } : {}),
      });
    }
    const begin = at + 1;
    const end = Math.min(all.length, begin + pageSize);
    send({
      messages: all.slice(begin, end),
      ...(end < all.length ? { next: all[end - 1].messageId } : {}),
    });
  });

  chat.onParticipantsQuery(({ roomId, query }, send) => {
    const room = rooms[roomId];
    const ids = room?.peer === null ? ["ada", "grace"] : [room?.peer ?? ""];
    const entry = (participantId: string) => ({
      participantId,
      participant: {
        tag: "Custom" as const,
        value: { nickname: PEOPLE[participantId] },
      },
    });
    if (query.tag === "Count")
      return send({ tag: "Count", value: { count: ids.length + 1 } });
    if (query.tag === "Page")
      return send({ tag: "Page", value: { participants: ids.map(entry) } });
    send({
      tag: "Resolved",
      value: {
        participants: query.value.participantIds
          .filter((id) => id in PEOPLE)
          .map(entry),
      },
    });
  });

  let sent = 0;
  chat.onUserAction(({ roomId, action }, send, interrupt) => {
    seen.actions.push({ roomId, action });
    const room = rooms[roomId];
    if (room === undefined)
      return interrupt({ tag: "Domain", value: { tag: "UnknownRoom" } });
    switch (action.tag) {
      case "NewMessage": {
        const body = action.value.body;
        const said = body.tag === "Text" ? body.value.text : "a file";
        if (said === "reject me")
          return interrupt({
            tag: "Domain",
            value: {
              tag: "Rejected",
              value: { reason: `${room.name} does not take this message.` },
            },
          });
        const message: ChatMessage = {
          messageId: `${roomId}-you-${++sent}`,
          origin: { participantId: "user" },
          timestamp: BigInt(now()),
          body,
          status: "Preparing",
          reactions: [],
        };
        room.messages.push(message);
        room.readThrough = message.messageId;
        send({ tag: "MessageCreated", value: message });
        void preview(roomId);
        deliver(roomId, message, said);
        return;
      }
      case "Retry": {
        const message = room.messages.find(
          (entry) => entry.messageId === action.value.messageId,
        );
        if (message?.status !== "Failed")
          return interrupt({ tag: "Domain", value: { tag: "UnknownMessage" } });
        send({ tag: "Acknowledged" });
        const body = message.body;
        deliver(roomId, message, body.tag === "Text" ? body.value.text : "");
        return;
      }
      case "Seen": {
        const index = room.messages.findIndex(
          (entry) => entry.messageId === action.value.messageId,
        );
        const current = room.messages.findIndex(
          (entry) => entry.messageId === room.readThrough,
        );
        send({ tag: "Acknowledged" });
        if (index > current) {
          room.readThrough = action.value.messageId;
          void preview(roomId);
        }
        return;
      }
      default:
        send({ tag: "Acknowledged" });
    }
  });

  /** The other side's host, made up: it answers the synthetic engine's offer. */
  const remoteHost = {
    receive(roomId: string, callId: string, signal: string) {
      const said = new TextDecoder().decode(
        Uint8Array.from(signal.slice(2).match(/../g) ?? [], (byte) =>
          parseInt(byte, 16),
        ),
      );
      if (said === `synthetic-offer:${callId}`)
        void this.say(roomId, callId, `synthetic-answer:${callId}`);
    },
    async say(roomId: string, callId: string, said: string) {
      record(
        "callMedia.signal",
        await chat.callMedia.signal({
          roomId,
          callId,
          signal: bytesToHex(new TextEncoder().encode(said)),
        }),
      );
    },
  };

  let calls = 0;
  /** Whether each call Ada rings has video. */
  const ringing = new Map<string, boolean>();
  const startCall = async (
    roomId: string,
    callId: string,
    video: boolean,
    offerer: boolean,
  ) => {
    record(
      "setRoomCall",
      await chat.setRoomCall({
        roomId,
        call: {
          tag: "Active",
          value: { callId, video, startedAt: BigInt(now()) },
        },
      }),
    );
    const started = await chat.callMedia.start({
      roomId,
      callId,
      video,
      offerer,
    });
    seen.media.push({
      callId,
      offerer,
      ok: started.isOk(),
      error: started.isErr() ? started.error : null,
    });
    // The other side makes the offer when this side placed the call.
    if (started.isOk() && !offerer)
      await remoteHost.say(roomId, callId, `synthetic-offer:${callId}`);
  };

  const declare = async () => {
    record(
      "roomEventsSubscribe",
      await chat.roomEventsSubscribe(({ roomId, event }) => {
        seen.events.push({ roomId, event });
        switch (event.tag) {
          case "CallRequested":
            // Ada's side, made up, accepts at once.
            void startCall(roomId, `call-${++calls}`, event.value.video, false);
            return;
          case "CallAnswered":
            if (event.value.accept)
              void startCall(
                roomId,
                event.value.callId,
                ringing.get(event.value.callId) ?? false,
                true,
              );
            else void chat.setRoomCall({ roomId });
            return;
          case "CallEnded":
            void (async () => {
              record(
                "callMedia.stop",
                await chat.callMedia.stop({
                  roomId,
                  callId: event.value.callId,
                }),
              );
              record("setRoomCall", await chat.setRoomCall({ roomId }));
            })();
            return;
          case "CallSignal":
            remoteHost.receive(roomId, event.value.callId, event.value.signal);
            return;
        }
      }),
    );
    const capabilities = await chat.capabilities();
    seen.capabilities = capabilities.isOk() ? capabilities.value : null;
    for (const [roomId, room] of Object.entries(rooms)) {
      const created = await chat.createRoom({
        roomId,
        metadata: {
          name: room.name,
          ...(room.description === undefined
            ? {}
            : { description: room.description }),
        },
        config: {
          productSource: true,
          reactionDetails: false,
          participants: true,
          messageActions: false,
          reportSeen: true,
        },
      });
      record(`createRoom ${roomId}`, created);
      if (created.isErr()) continue;
      record(
        `setRoomFeatures ${roomId}`,
        await chat.setRoomFeatures({ roomId, features: room.features }),
      );
      await preview(roomId);
      seen.rooms.push(roomId);
    }
    seen.ready = true;
  };

  return {
    seen,
    ready: declare(),
    async ring(roomId, video) {
      const callId = `call-${++calls}`;
      ringing.set(callId, video);
      record(
        "setRoomCall",
        await chat.setRoomCall({
          roomId,
          call: {
            tag: "Ringing",
            value: { callId, video, caller: PEOPLE.ada },
          },
        }),
      );
    },
    async edit(roomId, messageId, value) {
      const message = rooms[roomId].messages.find(
        (entry) => entry.messageId === messageId,
      );
      if (message !== undefined) message.body = text(value);
      await feed(roomId, {
        tag: "Edited",
        value: { messageId, body: text(value), editedAt: BigInt(now()) },
      });
    },
    async remove(roomId, messageId) {
      rooms[roomId].messages = rooms[roomId].messages.filter(
        (entry) => entry.messageId !== messageId,
      );
      await feed(roomId, { tag: "Deleted", value: { messageId } });
    },
    stop() {
      stopped = true;
      for (const timer of timers) clearTimeout(timer);
      timers.clear();
    },
  };
}
