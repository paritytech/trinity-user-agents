import { describe, expect, test } from "bun:test";
import type { ProviderEvent } from "../chat-provider.js";
import {
  messagePortBridge,
  type ChatMessage,
  type ChatUserAction,
  type HostChatRoomEventsSubscribeItem,
  type ProductChatUserActionItem,
} from "./contract.js";
import { ChatV2Host } from "./host.js";
import { chatV2Over, type ChatV2Product } from "./product.js";
import { ChatV2Provider } from "./provider.js";

const PRODUCT = "chat.paseo";

const message = (
  id: string,
  from: string,
  at: number,
  extra: Partial<ChatMessage> = {},
): ChatMessage => ({
  messageId: id,
  origin: { participantId: from },
  timestamp: BigInt(at),
  body: { tag: "Text", value: { text: id } },
  status: "Sent",
  reactions: [],
  ...extra,
});

const settle = () => Bun.sleep(5);

type Answer = ProductChatUserActionItem | { refuse: unknown };

const FEATURES = {
  preimageAttachments: false,
  search: false,
  readAloud: false,
  sealedAttachments: false,
  hopAttachments: false,
};

/**
 * A host with the bridge, one synthetic product on it with a room `ada` that
 * keeps three messages in the product, and the provider over them.
 */
async function setup() {
  const host = new ChatV2Host({ media: () => null, log: () => {} });
  const channel = new MessageChannel();
  host.connect(PRODUCT, messagePortBridge(channel.port2));
  const product: ChatV2Product = chatV2Over(messagePortBridge(channel.port1));
  await product.createRoom({
    roomId: "ada",
    metadata: { name: "Ada", description: "Direct" },
    config: {
      productSource: true,
      reactionDetails: false,
      participants: true,
      messageActions: false,
      reportSeen: true,
    },
    preview: {
      order: "Timestamp",
      badge: { tag: "UnreadCount", value: { count: 2 } },
    },
  });
  const stored = [
    message("m1", "ada", 10),
    message("m2", "user", 20, { status: "Read" }),
    message("m3", "product", 30, {
      body: { tag: "Notice", value: { text: "Ada joined" } },
    }),
  ];
  const actions: ChatUserAction[] = [];
  let answer: (action: ChatUserAction) => Answer = (action) =>
    action.tag === "NewMessage"
      ? {
          tag: "MessageCreated",
          value: message(`u${actions.length}`, "user", 100, {
            body: action.value.body,
          }),
        }
      : { tag: "Acknowledged" };
  product.onMessagesQuery(({ anchor, pageSize }, send) => {
    if (anchor.tag !== "Before") return send({ messages: [] });
    send({ messages: stored.slice(-pageSize) });
  });
  product.onParticipantsQuery(({ query }, send) => {
    if (query.tag !== "ByIds")
      return send({ tag: "Count", value: { count: 2 } });
    send({
      tag: "Resolved",
      value: {
        participants: query.value.participantIds
          .filter((id) => id === "ada")
          .map((participantId) => ({
            participantId,
            participant: { tag: "Custom", value: { nickname: "Ada L." } },
          })),
      },
    });
  });
  product.onUserAction(({ action }, send, interrupt) => {
    actions.push(action);
    const item = answer(action);
    if ("refuse" in item) interrupt(item.refuse as never);
    else send(item);
  });
  const events: HostChatRoomEventsSubscribeItem[] = [];
  await product.roomEventsSubscribe((item) => events.push(item));
  const provider = new ChatV2Provider(PRODUCT, host);
  const seen: ProviderEvent[] = [];
  provider.subscribe((event) => seen.push(event));
  return {
    host,
    product,
    provider,
    actions,
    events,
    seen,
    stored,
    setAnswer: (next: typeof answer) => (answer = next),
  };
}

const refuse = (tag: string, reason: string): Answer => ({
  refuse: { tag: "Domain", value: { tag, value: { reason } } },
});

describe("the chat v2 draft provider", () => {
  test("lists what the product declared", async () => {
    const { provider } = await setup();
    await provider.timeline("ada");
    expect(provider.conversations()).toEqual([
      {
        id: "ada",
        title: "Ada",
        subtitle: "Direct",
        icon: null,
        seed: PRODUCT,
        preview: "Ada joined",
        updatedAt: 30,
        unread: 2,
      },
    ]);
  });

  test("pages come from the product, with its participants and the user as self", async () => {
    const { provider } = await setup();
    const page = await provider.timeline("ada");
    expect(
      page.messages.map((m) => [m.id, m.sender, m.status, m.content.tag]),
    ).toEqual([
      ["m1", { id: "ada", label: "Ada L.", self: false }, "received", "Text"],
      ["m2", { id: "user", label: "You", self: true }, "read", "Text"],
      [
        "m3",
        { id: "product", label: PRODUCT, self: false },
        "received",
        "System",
      ],
    ]);
  });

  test("a sent message is pending, then the product's stored message under the same id", async () => {
    const { provider, seen, actions, product } = await setup();
    await provider.send("ada", "hello");
    expect(actions.at(-1)).toEqual({
      tag: "NewMessage",
      value: { body: { tag: "Text", value: { text: "hello" } } },
    });
    const events = seen.filter((event) => event.type === "messages");
    expect(events.map((event) => event.message?.id)).toEqual([
      "local-1",
      "local-1",
    ]);
    expect(events.map((event) => event.message?.status)).toEqual([
      "pending",
      "sent",
    ]);
    // The product's later changes name its own id; the screen still sees one message.
    await product.updateFeed({
      roomId: "ada",
      update: {
        tag: "StatusChanged",
        value: { messageId: "u1", status: "Read" },
      },
    });
    await settle();
    expect(seen.at(-1)).toMatchObject({
      message: { id: "local-1", status: "read" },
    });
    provider.markRead("ada", "local-1");
    await settle();
    expect(actions.at(-1)).toEqual({
      tag: "Seen",
      value: { messageId: "u1" },
    });
  });

  test("a refused message stays failed with the reason, and is sent again as the same message", async () => {
    const { provider, seen, setAnswer, actions } = await setup();
    setAnswer(() => refuse("Rejected", "Blocked by Ada"));
    await provider.send("ada", "no");
    expect(seen.at(-1)).toMatchObject({
      message: { id: "local-1", status: "failed", problem: "Blocked by Ada" },
    });
    expect((await provider.timeline("ada")).messages.at(-1)?.id).toBe(
      "local-1",
    );
    setAnswer((action) =>
      action.tag === "NewMessage"
        ? {
            tag: "MessageCreated",
            value: message("u9", "user", 200, { body: action.value.body }),
          }
        : { tag: "Acknowledged" },
    );
    await provider.retry("ada", "local-1");
    expect(seen.at(-1)).toMatchObject({
      message: { id: "local-1", status: "sent" },
    });
    expect(
      actions.filter((action) => action.tag === "NewMessage"),
    ).toHaveLength(2);
    // A message the product marked failed is retried by the product.
    await provider.retry("ada", "m2");
    expect(actions.at(-1)).toEqual({
      tag: "Retry",
      value: { messageId: "m2" },
    });
  });

  test("Seen goes out once per newest message, only for a room that asks", async () => {
    const { provider, product, actions } = await setup();
    await product.createRoom({
      roomId: "quiet",
      metadata: { name: "Quiet" },
      config: {
        productSource: true,
        reactionDetails: false,
        participants: false,
        messageActions: false,
        reportSeen: false,
      },
    });
    await provider.timeline("ada");
    provider.markRead("ada", "m3");
    provider.markRead("ada", "m3");
    provider.markRead("quiet", "m1");
    await settle();
    expect(actions).toEqual([{ tag: "Seen", value: { messageId: "m3" } }]);
  });

  test("a reaction shows at once and is taken back when the product refuses it", async () => {
    const { provider, seen, actions, setAnswer } = await setup();
    await provider.timeline("ada");
    await provider.react("ada", "m1", "👍");
    expect(actions.at(-1)).toEqual({
      tag: "ReactionPlaced",
      value: { messageId: "m1", emoji: "👍" },
    });
    await provider.react("ada", "m1", "🎉");
    expect(actions.slice(-2).map((action) => action.tag)).toEqual([
      "ReactionPlaced",
      "ReactionRemoved",
    ]);
    setAnswer(() => refuse("Unknown", "no"));
    await expect(provider.react("ada", "m1", null)).rejects.toThrow(
      "taken back",
    );
    expect(seen.at(-1)).toMatchObject({
      message: { id: "m1", reactions: [{ emoji: "🎉", count: 1, self: true }] },
    });
  });

  test("feed changes reach the screen as changed messages; a delete makes it read again", async () => {
    const { provider, product, seen, stored } = await setup();
    await provider.timeline("ada");
    await product.updateFeed({
      roomId: "ada",
      update: { tag: "New", value: message("m4", "ada", 40) },
    });
    stored.shift();
    await product.updateFeed({
      roomId: "ada",
      update: { tag: "Deleted", value: { messageId: "m1" } },
    });
    await settle();
    expect(seen.filter((event) => event.type === "messages")).toEqual([
      {
        type: "messages",
        conversationId: "ada",
        message: expect.objectContaining({
          id: "m4",
          sender: { id: "ada", label: "Ada L.", self: false },
        }),
      },
      { type: "messages", conversationId: "ada" },
    ]);
    expect((await provider.timeline("ada")).messages.map((m) => m.id)).toEqual([
      "m2",
      "m3",
    ]);
  });

  test("calls follow the room's features, and the product owns the call", async () => {
    const { provider, product, events } = await setup();
    const calls = provider.calls;
    expect(calls.blocked("ada")).toBe("This room has no calls.");
    await product.setRoomFeatures({
      roomId: "ada",
      features: { ...FEATURES, calls: { audio: true, video: false } },
    });
    expect(calls.blocked("ada")).toBeNull();
    await expect(calls.start("ada", true)).rejects.toThrow("no video calls");
    await calls.start("ada", false);
    expect(calls.current()?.state).toBe("calling");
    await settle();
    expect(events.at(-1)?.event).toEqual({
      tag: "CallRequested",
      value: { video: false },
    });
    await product.setRoomCall({
      roomId: "ada",
      call: {
        tag: "Active",
        value: { callId: "c1", video: false, startedAt: 1n },
      },
    });
    expect(calls.current()).toMatchObject({
      state: "connected",
      direction: "out",
      local: null,
    });
    calls.hangUp();
    await settle();
    expect(events.at(-1)?.event).toEqual({
      tag: "CallEnded",
      value: { callId: "c1" },
    });
    await product.setRoomCall({ roomId: "ada" });
    expect(calls.current()?.state).toBe("ended");
    calls.dismiss();
    expect(calls.current()).toBeNull();

    await product.setRoomCall({
      roomId: "ada",
      call: {
        tag: "Ringing",
        value: { callId: "c2", video: false, caller: "Ada" },
      },
    });
    expect(calls.current()).toMatchObject({ state: "incoming", title: "Ada" });
    await calls.accept();
    await settle();
    expect(events.at(-1)?.event).toEqual({
      tag: "CallAnswered",
      value: { callId: "c2", accept: true },
    });
  });

  test("a file goes inline as rich-text media, and opens from its bytes", async () => {
    const { provider, actions } = await setup();
    const file = new File([new Uint8Array([1, 2, 3])], "a.png", {
      type: "image/png",
    });
    await provider.files.send("ada", file, "look");
    expect(actions.at(-1)).toEqual({
      tag: "NewMessage",
      value: {
        body: {
          tag: "RichText",
          value: {
            text: "look",
            media: [
              {
                tag: "Image",
                value: {
                  resource: {
                    tag: "Inline",
                    value: { mimeType: "image/png", bytes: "0x010203" },
                  },
                  meta: { mimeType: "image/png", sizeBytes: 3n },
                },
              },
            ],
          },
        },
      },
    });
    const blob = await provider.files.open(
      "ada",
      "local-1",
      0,
      new AbortController().signal,
    );
    expect([...new Uint8Array(await blob.arrayBuffer())]).toEqual([1, 2, 3]);
  });

  test("a worker restart is a new epoch, and the room waits for the worker", async () => {
    const { provider, host } = await setup();
    const epoch = provider.epoch();
    host.disconnect(PRODUCT);
    expect(provider.epoch()).toBeGreaterThan(epoch);
    expect(provider.status().state).toBe("offline");
    expect(provider.composer("ada")).toEqual({
      kind: "blocked",
      reason: "The product's worker is not running",
    });
    await expect(provider.timeline("ada")).rejects.toThrow("not running");
  });

  test("delete_room takes the room off the list", async () => {
    const { provider, product } = await setup();
    await product.deleteRoom({ roomId: "ada" });
    expect(provider.conversations()).toEqual([]);
  });
});
