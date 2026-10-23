import { describe, expect, test } from "bun:test";
import { bytesToHex } from "@parity/truapi/scale";
import {
  messagePortBridge,
  type ChatMessage,
  type HostChatRoomEventsSubscribeItem,
} from "./contract.js";
import {
  ChatV2Host,
  type CallMediaEngine,
  type CallMediaEvents,
  type ChatV2Change,
} from "./host.js";
import { chatV2Over, type ChatV2Product } from "./product.js";

const PRODUCT = "chat.paseo";
const OTHER = "other.paseo";

const message = (id: string, from: string, at: number): ChatMessage => ({
  messageId: id,
  origin: { participantId: from },
  timestamp: BigInt(at),
  body: { tag: "Text", value: { text: id } },
  status: "Sent",
  reactions: [],
});

/** A host call engine that opens no device: it trades made-up signals. */
function fakeMedia() {
  const started: { offerer: boolean; signals: string[]; stopped: boolean }[] =
    [];
  const engine: CallMediaEngine = {
    name: "fake",
    start(call, events: CallMediaEvents) {
      const entry = {
        offerer: call.offerer,
        signals: [] as string[],
        stopped: false,
      };
      started.push(entry);
      if (call.offerer) events.signal(new TextEncoder().encode("offer"));
      return {
        state: "connecting",
        local: null,
        remote: null,
        signal: (bytes) => entry.signals.push(new TextDecoder().decode(bytes)),
        setMuted: () => {},
        setCamera: () => {},
        stop: () => (entry.stopped = true),
      };
    },
  };
  return { engine, started };
}

/** A host and one connected product. */
function setup(
  options: { media?: CallMediaEngine | null; now?: () => number } = {},
) {
  const changes: ChatV2Change[] = [];
  const host = new ChatV2Host({
    media: () => options.media ?? null,
    now: options.now,
    log: () => {},
  });
  host.onChange((change) => changes.push(change));
  const connect = (productId: string): ChatV2Product => {
    const channel = new MessageChannel();
    host.connect(productId, messagePortBridge(channel.port2));
    return chatV2Over(messagePortBridge(channel.port1));
  };
  return { host, changes, connect, product: connect(PRODUCT) };
}

const CONFIG = {
  productSource: true,
  reactionDetails: false,
  participants: true,
  messageActions: false,
  reportSeen: true,
};

async function room(product: ChatV2Product, roomId = "ada") {
  const created = await product.createRoom({
    roomId,
    metadata: { name: "Ada" },
    config: CONFIG,
  });
  expect(created._unsafeUnwrap()).toEqual({ status: "New" });
}

describe("chat v2 draft bridge", () => {
  test("a product configures only rooms it declared", async () => {
    const { host, product } = setup();
    await room(product);
    expect(host.room(PRODUCT, "ada")?.metadata.name).toBe("Ada");
    expect(
      (
        await product.createRoom({
          roomId: "ada",
          metadata: { name: "Renamed" },
          config: { ...CONFIG, productSource: false },
        })
      )._unsafeUnwrap(),
    ).toEqual({ status: "Exists" });
    // `Exists` keeps what the room was created with.
    expect(host.room(PRODUCT, "ada")?.config.productSource).toBe(true);

    expect(
      (
        await product.setRoomMetadata({
          roomId: "elsewhere",
          metadata: { name: "x" },
        })
      )._unsafeUnwrapErr(),
    ).toEqual({ tag: "Domain", value: { tag: "UnknownRoom" } });
  });

  test("another product cannot reach a room it did not create", async () => {
    const { host, product, connect } = setup();
    await room(product);
    const other = connect(OTHER);
    expect(
      (
        await other.updateFeed({
          roomId: "ada",
          update: { tag: "New", value: message("m1", "product", 1) },
        })
      )._unsafeUnwrapErr(),
    ).toEqual({ tag: "Domain", value: { tag: "UnknownRoom" } });
    expect(host.cached(PRODUCT, "ada")).toEqual([]);
  });

  test("the host pages the product's history before and after a cursor", async () => {
    const { host, product } = setup();
    await room(product);
    const history = Array.from({ length: 45 }, (_, index) =>
      message(`m${index}`, index % 2 ? "user" : "ada", index),
    );
    const asked: unknown[] = [];
    product.onMessagesQuery(({ anchor, pageSize }, send) => {
      asked.push(anchor);
      const cursor =
        anchor.value.cursor === undefined
          ? undefined
          : Number(anchor.value.cursor);
      if (anchor.tag === "Before") {
        const end = cursor ?? history.length;
        const start = Math.max(0, end - pageSize);
        send({
          messages: history.slice(start, end),
          ...(start > 0 ? { next: String(start) } : {}),
        });
      } else {
        const start = cursor === undefined ? 0 : cursor + 1;
        const end = Math.min(history.length, start + pageSize);
        send({
          messages: history.slice(start, end),
          ...(end < history.length ? { next: String(end - 1) } : {}),
        });
      }
    });
    const newest = await host.query(PRODUCT, "messagesQuery", {
      roomId: "ada",
      anchor: { tag: "Before", value: {} },
      pageSize: 30,
    });
    expect(newest._unsafeUnwrap().messages.map((m) => m.messageId)[0]).toBe(
      "m15",
    );
    expect(newest._unsafeUnwrap().next).toBe("15");
    const older = await host.query(PRODUCT, "messagesQuery", {
      roomId: "ada",
      anchor: { tag: "Before", value: { cursor: "15" } },
      pageSize: 30,
    });
    expect(older._unsafeUnwrap().messages).toHaveLength(15);
    expect(older._unsafeUnwrap().next).toBeUndefined();
    const after = await host.query(PRODUCT, "messagesQuery", {
      roomId: "ada",
      anchor: { tag: "After", value: { cursor: "40" } },
      pageSize: 30,
    });
    expect(after._unsafeUnwrap().messages.map((m) => m.messageId)).toEqual([
      "m41",
      "m42",
      "m43",
      "m44",
    ]);
    expect(asked).toHaveLength(3);
    expect(host.cached(PRODUCT, "ada")).toHaveLength(45);
  });

  test("a sent message is the product's stored message, and feed updates change it", async () => {
    const { host, product, changes } = setup();
    await room(product);
    product.onUserAction(({ action }, send) => {
      if (action.tag !== "NewMessage") return send({ tag: "Acknowledged" });
      send({
        tag: "MessageCreated",
        value: { ...message("u1", "user", 5), body: action.value.body },
      });
    });
    const created = await host.query(PRODUCT, "userAction", {
      roomId: "ada",
      action: {
        tag: "NewMessage",
        value: { body: { tag: "Text", value: { text: "hi" } } },
      },
    });
    expect(created._unsafeUnwrap()).toMatchObject({
      tag: "MessageCreated",
      value: { messageId: "u1", origin: { participantId: "user" } },
    });
    const update = (
      value: Parameters<ChatV2Product["updateFeed"]>[0]["update"],
    ) => product.updateFeed({ roomId: "ada", update: value });
    expect(
      (
        await update({
          tag: "StatusChanged",
          value: { messageId: "u1", status: "Read" },
        })
      ).isOk(),
    ).toBe(true);
    await update({
      tag: "ReactionPlaced",
      value: { messageId: "u1", participantId: "ada", emoji: "👍" },
    });
    await update({
      tag: "Edited",
      value: {
        messageId: "u1",
        body: { tag: "Text", value: { text: "hi!" } },
        editedAt: 9n,
      },
    });
    expect(host.cached(PRODUCT, "ada")[0]).toMatchObject({
      status: "Read",
      body: { tag: "Text", value: { text: "hi!" } },
      editedAt: 9n,
      reactions: [{ emoji: "👍", count: 1, reactedByUser: false }],
    });
    expect(
      (
        await update({ tag: "New", value: message("u1", "user", 6) })
      )._unsafeUnwrapErr(),
    ).toEqual({ tag: "Domain", value: { tag: "DuplicateMessage" } });
    await update({ tag: "Deleted", value: { messageId: "u1" } });
    expect(host.cached(PRODUCT, "ada")).toEqual([]);
    expect(changes).toContainEqual({
      type: "feed",
      productId: PRODUCT,
      roomId: "ada",
      removed: "u1",
    });
  });

  test("queries follow the room's configuration", async () => {
    const { host, product } = setup();
    await product.createRoom({
      roomId: "store",
      metadata: { name: "Stored" },
      config: { ...CONFIG, productSource: false, participants: false },
    });
    expect(
      (
        await host.query(PRODUCT, "participantsQuery", {
          roomId: "store",
          query: { tag: "Count" },
        })
      )._unsafeUnwrapErr(),
    ).toEqual({ tag: "Domain", value: { tag: "NotAvailable" } });
    expect(
      (
        await host.query(PRODUCT, "userAction", {
          roomId: "store",
          action: { tag: "Seen", value: { messageId: "m" } },
        })
      )._unsafeUnwrapErr().tag,
    ).toBe("Domain");
    // A product with no handler answers Unsupported.
    await room(product);
    expect(
      (
        await host.query(PRODUCT, "participantsQuery", {
          roomId: "ada",
          query: { tag: "Count" },
        })
      )._unsafeUnwrapErr(),
    ).toEqual({ tag: "Unsupported" });
  });

  test("a stale query is cancelled and its late answer reaches nothing", async () => {
    const { host, product } = setup();
    await room(product);
    let late: (() => void) | null = null;
    product.onMessagesQuery((_request, send) => {
      late = () => send({ messages: [message("late", "ada", 1)] });
    });
    const aborted = new AbortController();
    const asked = host.query(
      PRODUCT,
      "messagesQuery",
      { roomId: "ada", anchor: { tag: "Before", value: {} }, pageSize: 30 },
      { signal: aborted.signal },
    );
    await Bun.sleep(5);
    aborted.abort();
    expect((await asked)._unsafeUnwrapErr()).toEqual({ tag: "Cancelled" });
    late!();
    const timedOut = await host.query(
      PRODUCT,
      "messagesQuery",
      { roomId: "ada", anchor: { tag: "Before", value: {} }, pageSize: 30 },
      { timeoutMs: 10 },
    );
    expect(timedOut._unsafeUnwrapErr()).toEqual({ tag: "Timeout" });
    await Bun.sleep(5);
    expect(host.cached(PRODUCT, "ada")).toEqual([]);
  });

  test("a worker that stops fails its open queries, and a new link starts over", async () => {
    const { host, product, connect } = setup();
    await room(product);
    product.onMessagesQuery(() => {});
    const before = host.epoch(PRODUCT);
    const asked = host.query(PRODUCT, "messagesQuery", {
      roomId: "ada",
      anchor: { tag: "Before", value: {} },
      pageSize: 30,
    });
    await Bun.sleep(5);
    host.disconnect(PRODUCT);
    expect((await asked)._unsafeUnwrapErr().tag).toBe("HostFailure");
    expect(host.connected(PRODUCT)).toBe(false);
    expect(
      (
        await host.query(PRODUCT, "messagesQuery", {
          roomId: "ada",
          anchor: { tag: "Before", value: {} },
          pageSize: 30,
        })
      )._unsafeUnwrapErr().tag,
    ).toBe("HostFailure");
    // Declarations outlive the worker, as the RFC keeps them across restarts.
    expect(host.room(PRODUCT, "ada")).toBeDefined();
    const again = connect(PRODUCT);
    expect(host.epoch(PRODUCT)).toBeGreaterThan(before + 1);
    expect((await again.capabilities())._unsafeUnwrap().features).toContain(
      "product_source_rooms",
    );
  });

  test("capabilities list only what this host serves", async () => {
    const { product } = setup();
    const caps = (await product.capabilities())._unsafeUnwrap();
    expect(caps.features).not.toContain("room_calls");
    expect(caps.features).not.toContain("preimage_attachments");
    expect(caps.features).not.toContain("sealed_attachments");
    expect(caps.preimageAttachmentMaxBytes).toBe(0);
    expect(caps.sealedAttachmentMaxBytes).toBe(0);
  });

  test("room events queue until the product subscribes", async () => {
    const { host, product } = setup();
    await room(product);
    host.roomEvent(PRODUCT, "ada", {
      tag: "CallRequested",
      value: { video: false },
    });
    const seen: HostChatRoomEventsSubscribeItem[] = [];
    await product.roomEventsSubscribe((item) => seen.push(item));
    await Bun.sleep(5);
    host.roomEvent(PRODUCT, "ada", {
      tag: "CallEnded",
      value: { callId: "c" },
    });
    await Bun.sleep(5);
    expect(seen.map((item) => item.event.tag)).toEqual([
      "CallRequested",
      "CallEnded",
    ]);
  });

  test("call state is the product's, media is the host's, and media needs a press", async () => {
    let now = 1_000;
    const { engine, started } = fakeMedia();
    const { host, product } = setup({ media: engine, now: () => now });
    await room(product);
    const signals: string[] = [];
    await product.roomEventsSubscribe(({ event }) => {
      if (event.tag === "CallSignal") signals.push(event.value.signal);
    });
    const active = {
      roomId: "ada",
      call: {
        tag: "Active" as const,
        value: { callId: "c1", video: false, startedAt: 1n },
      },
    };
    expect((await product.setRoomCall(active)).isOk()).toBe(true);
    const start = { roomId: "ada", callId: "c1", video: false, offerer: true };
    // No press in the room: no microphone.
    expect((await product.callMedia.start(start))._unsafeUnwrapErr()).toEqual({
      tag: "Domain",
      value: { tag: "NotAllowed" },
    });
    host.roomEvent(PRODUCT, "ada", {
      tag: "CallRequested",
      value: { video: false },
    });
    // A signal of the other side that comes first is kept for the start.
    await product.callMedia.signal({
      roomId: "ada",
      callId: "c1",
      signal: bytesToHex(new TextEncoder().encode("early")),
    });
    expect((await product.callMedia.start(start)).isOk()).toBe(true);
    await Bun.sleep(5);
    expect(started[0].signals).toEqual(["early"]);
    expect(signals).toEqual([bytesToHex(new TextEncoder().encode("offer"))]);
    // A media session of the Media service is not served here.
    expect(
      (
        await product.setRoomCall({
          roomId: "ada",
          call: {
            tag: "Active",
            value: {
              ...active.call.value,
              mediaSession: `0x${"00".repeat(32)}`,
            },
          },
        })
      )._unsafeUnwrapErr().tag,
    ).toBe("Domain");
    await product.setRoomCall({ roomId: "ada" });
    expect(started[0].stopped).toBe(true);
    expect(host.heldMedia(PRODUCT)).toBeNull();
    // The press is old now.
    now += 181_000;
    await product.setRoomCall(active);
    expect((await product.callMedia.start(start))._unsafeUnwrapErr()).toEqual({
      tag: "Domain",
      value: { tag: "NotAllowed" },
    });
  });

  test("a host without call media answers Unsupported", async () => {
    const { host, product } = setup({ media: null });
    await room(product);
    await product.setRoomCall({
      roomId: "ada",
      call: {
        tag: "Active",
        value: { callId: "c1", video: false, startedAt: 1n },
      },
    });
    host.roomEvent(PRODUCT, "ada", {
      tag: "CallRequested",
      value: { video: false },
    });
    expect(
      (
        await product.callMedia.start({
          roomId: "ada",
          callId: "c1",
          video: false,
          offerer: true,
        })
      )._unsafeUnwrapErr(),
    ).toEqual({ tag: "Unsupported" });
  });

  test("delete_room removes the room", async () => {
    const { host, product } = setup();
    await room(product);
    expect((await product.deleteRoom({ roomId: "ada" })).isOk()).toBe(true);
    expect(host.room(PRODUCT, "ada")).toBeUndefined();
    // Deleting a room that is gone succeeds.
    expect((await product.deleteRoom({ roomId: "ada" })).isOk()).toBe(true);
  });
});
