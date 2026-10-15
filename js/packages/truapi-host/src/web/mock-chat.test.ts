// The chat model of the mock host, without the core: the callbacks keep the
// declarations and the messages, and the user half follows the host rules
// against a scripted product runtime.
import { describe, expect, it } from "bun:test";
import { err, ok } from "neverthrow";

import type {
  ChatMessage,
  ChatRoomConfig,
  HostChatActionSubscribeItem,
  HostChatRoomEventsSubscribeItem,
} from "@parity/truapi";

import type {
  ChatHostRuntime,
  ChatQueryName,
  ChatWakeReason,
} from "../chat-queries.js";
import { ProductContext } from "../generated/host-callbacks.js";
import { createMockHost } from "./create-mock-host.js";

const product: ProductContext = {
  productId: "chat.dot",
  executionKind: "Worker",
};

const CONFIG: ChatRoomConfig = {
  productSource: false,
  reactionDetails: false,
  participants: false,
  messageActions: false,
  participantActions: false,
  reportSeen: false,
};

function message(id: string, timestamp: bigint, text = id): ChatMessage {
  return {
    messageId: id,
    origin: { participantId: "product" },
    timestamp,
    body: { tag: "Text", value: { text } },
    status: "Sent",
    reactions: [],
  };
}

type Answer = (request: unknown) => unknown;

/** A product runtime whose answers a test writes per query. */
function scripted(answers: Partial<Record<ChatQueryName, Answer>>) {
  const published: HostChatActionSubscribeItem[] = [];
  const roomEvents: HostChatRoomEventsSubscribeItem[] = [];
  const wakes: ChatWakeReason[] = [];
  const asked: { name: ChatQueryName; request: unknown }[] = [];
  const query =
    (name: ChatQueryName) =>
    async (request: unknown): Promise<never> => {
      asked.push({ name, request });
      const answer = answers[name];
      if (!answer) throw new Error(`no answer for ${name}`);
      return answer(request) as never;
    };
  const runtime: ChatHostRuntime = {
    chatMessagesQuery: query("chatMessagesQuery"),
    chatUserAction: query("chatUserAction"),
    chatReactionsQuery: query("chatReactionsQuery"),
    chatParticipantsQuery: query("chatParticipantsQuery"),
    chatMessageActionsQuery: query("chatMessageActionsQuery"),
    chatParticipantActionsQuery: query("chatParticipantActionsQuery"),
    chatSearchQuery: query("chatSearchQuery"),
    publishChatEvent: async (item) => {
      published.push(item);
    },
    publishChatRoomEvent: async (item) => {
      roomEvents.push(item);
    },
    wakeProduct: async (reason) => {
      wakes.push(reason);
      return { tag: "Held", value: { untilMs: 30_000 } };
    },
    stageChatAttachment: async () => {
      throw new Error("not staged");
    },
  };
  return { runtime, published, roomEvents, wakes, asked };
}

async function hostWithRoom(
  roomId: string,
  config: Partial<ChatRoomConfig> = {},
) {
  const host = createMockHost();
  await host.callbacks.chat.createChatRoom(
    product,
    { roomId, metadata: { name: roomId }, config: { ...CONFIG, ...config } },
    "",
  );
  return host;
}

describe("mock host chat declarations", () => {
  it("keeps every declaration and drops them with the room", async () => {
    const host = await hostWithRoom("host");
    const chat = host.callbacks.chat;
    await chat.setChatRoomMetadata(product, {
      roomId: "host",
      metadata: { name: "Renamed", description: "text" },
    });
    await chat.setChatRoomToolbar(product, {
      roomId: "host",
      actions: [
        {
          actionId: "tb-1",
          label: "One",
          icon: {
            tag: "Inline",
            value: { mimeType: "image/png", bytes: "0x01" },
          },
          handler: { tag: "Callback" },
        },
      ],
    });
    await chat.setChatRoomSuggestions(product, {
      roomId: "host",
      suggestions: [{ suggestionId: "sg", text: "Hi", sendsText: true }],
    });
    await chat.setChatRoomCommands(product, {
      roomId: "host",
      commands: [{ name: "help", description: "Help" }],
    });
    await chat.setChatRoomActivity(product, {
      roomId: "host",
      activity: { text: "typing" },
    });
    await chat.setChatRoomFooter(product, {
      roomId: "host",
      footer: { tag: "Empty" },
    });

    expect(host.room("host")).toMatchObject({
      metadata: { name: "Renamed", description: "text" },
      toolbar: [{ actionId: "tb-1" }],
      suggestions: [{ suggestionId: "sg" }],
      commands: [{ name: "help" }],
      activity: { text: "typing" },
      footer: { tag: "Empty" },
    });

    await chat.setChatRoomActivity(product, { roomId: "host" });
    expect(host.room("host")?.activity).toBeUndefined();

    await chat.deleteChatRoom(product, { roomId: "host" });
    // Delete is idempotent.
    await chat.deleteChatRoom(product, { roomId: "host" });
    expect(host.room("host")).toBeUndefined();
    expect(host.rooms()).toEqual([]);
    expect(host.feed("host")).toEqual([]);
  });

  it("stores the feed of a host-store room and refuses what it cannot apply", async () => {
    const host = await hostWithRoom("host");
    const chat = host.callbacks.chat;
    const update = (u: Parameters<typeof chat.updateChatFeed>[1]["update"]) =>
      chat.updateChatFeed(product, { roomId: "host", update: u }, true);

    await update({ tag: "New", value: message("p-1", 2n) });
    await update({ tag: "New", value: message("p-2", 1n) });
    await expect(
      update({ tag: "New", value: message("p-1", 3n) }),
    ).rejects.toThrow("DuplicateMessage");
    await expect(
      update({ tag: "Deleted", value: { messageId: "nope" } }),
    ).rejects.toThrow("UnknownMessage");
    await update({
      tag: "ReactionPlaced",
      value: { messageId: "p-1", participantId: "p1", emoji: "👍" },
    });
    await update({
      tag: "StatusChanged",
      value: { messageId: "p-2", status: "Read" },
    });

    // A room the host stores keeps the arrival order.
    expect(host.feed("host").map(({ messageId }) => messageId)).toEqual([
      "p-1",
      "p-2",
    ]);
    expect(host.feed("host")[0]!.reactions).toEqual([
      { emoji: "👍", count: 1, reactedByUser: false },
    ]);
    expect(host.feed("host")[1]!.status).toBe("Read");

    await update({ tag: "Deleted", value: { messageId: "p-2" } });
    // The list line follows the newest message that is left.
    expect(host.preview("host")).toMatchObject({
      text: "p-1",
      order: "Timestamp",
    });
  });

  it("applies the feed of a product-source room to the screen in time order", async () => {
    const host = await hostWithRoom("source", { productSource: true });
    const chat = host.callbacks.chat;
    const update = (u: Parameters<typeof chat.updateChatFeed>[1]["update"]) =>
      chat.updateChatFeed(product, { roomId: "source", update: u }, false);

    await update({ tag: "New", value: message("b", 2n) });
    await update({ tag: "New", value: message("a", 1n) });
    // A message the screen does not show is not an error.
    await update({ tag: "Deleted", value: { messageId: "not-on-screen" } });

    expect(host.feed("source").map(({ messageId }) => messageId)).toEqual([
      "a",
      "b",
    ]);
    expect(host.room("source")?.updates.map(({ persist }) => persist)).toEqual([
      false,
      false,
      false,
    ]);
    // A product-source room has no host unread count.
    expect(host.preview("source")?.badge).toBeUndefined();

    // A change of mode starts the room with no messages.
    await chat.setChatRoomConfig(product, { roomId: "source", config: CONFIG });
    expect(host.feed("source")).toEqual([]);
  });
});

describe("mock host chat user", () => {
  it("needs an attached product", async () => {
    const host = await hostWithRoom("host");
    await expect(host.chatUser.send("host", "hi")).rejects.toThrow(
      "no product is attached",
    );
  });

  it("stores a message in a host-store room and sends MessagePosted", async () => {
    const host = await hostWithRoom("host");
    const product = scripted({});
    host.chatUser.attach(product.runtime);

    const outcome = await host.chatUser.send("host", "hello");

    expect(outcome.tag).toBe("Posted");
    expect(host.feed("host")).toHaveLength(1);
    expect(product.published.map(({ event }) => event.tag)).toEqual([
      "MessagePosted",
    ]);
    expect(host.events()).toEqual(product.published);
  });

  it("asks once more after NotAvailable, then uses the host default", async () => {
    const host = await hostWithRoom("source", { productSource: true });
    host.chatUser.setTimings({ retryDelay: 1 });
    let pages = 0;
    const product = scripted({
      chatMessagesQuery: () =>
        pages++ === 0
          ? err({ tag: "Domain", value: { tag: "NotAvailable" } })
          : ok({ messages: [message("m", 1n)] }),
    });
    host.chatUser.attach(product.runtime);

    expect(
      (await host.chatUser.open("source")).map(({ messageId }) => messageId),
    ).toEqual(["m"]);
    expect(product.asked.map(({ name }) => name)).toEqual([
      "chatMessagesQuery",
      "chatMessagesQuery",
    ]);
  });

  it("removes a rejected message and shows the reason, and marks a silent one Failed", async () => {
    const host = await hostWithRoom("source", { productSource: true });
    let answer: unknown = err({
      tag: "Domain",
      value: { tag: "Rejected", value: { reason: "nope" } },
    });
    const product = scripted({ chatUserAction: () => answer });
    host.chatUser.attach(product.runtime);

    expect(await host.chatUser.send("source", "one")).toEqual({
      tag: "Rejected",
      reason: "nope",
    });
    expect(host.feed("source")).toEqual([]);
    expect(host.chatUser.notices()).toEqual([
      { roomId: "source", text: "nope" },
    ]);

    answer = err({ tag: "Timeout" });
    const failed = await host.chatUser.send("source", "two");
    expect(failed.tag).toBe("Failed");
    expect(host.feed("source").map(({ status }) => status)).toEqual(["Failed"]);
  });

  it("shows the host menu on a refusal and the product menu otherwise", async () => {
    const host = await hostWithRoom("host", { messageActions: true });
    await host.callbacks.chat.updateChatFeed(
      product,
      { roomId: "host", update: { tag: "New", value: message("p-1", 1n) } },
      true,
    );
    let answer: unknown = err({ tag: "Timeout" });
    const runtime = scripted({ chatMessageActionsQuery: () => answer });
    host.chatUser.attach(runtime.runtime);

    expect(await host.chatUser.messageMenu("host", "p-1")).toEqual({
      builtIn: ["Copy", "Reply", "React"],
      actions: [],
      source: "default",
    });

    const pin = {
      actionId: "act-pin",
      label: "Pin",
      destructive: false,
      handler: { tag: "Callback" as const },
    };
    answer = ok({
      menu: {
        tag: "ExtendBuiltIn",
        value: { actions: [pin], hidden: ["Reply"] },
      },
    });
    expect(await host.chatUser.messageMenu("host", "p-1")).toEqual({
      builtIn: ["Copy", "React"],
      actions: [pin],
      source: "product",
    });
  });
});

describe("mock host chat additions", () => {
  it("lists every feature name, or the names its configuration gives", async () => {
    expect(
      (await createMockHost().callbacks.chat.chatCapabilities(product)).names,
    ).toContain("call_buttons");
    const host = createMockHost({ chatFeatures: ["search"] });
    expect(await host.callbacks.chat.chatCapabilities(product)).toEqual({
      names: ["search"],
    });
  });

  it("keeps the features, the meta and the call of a room", async () => {
    const host = await hostWithRoom("main");
    const { chat } = host.callbacks;
    expect(await chat.readChatRoomFeatures(product, "main")).toEqual({
      preimageAttachments: false,
      search: false,
      readAloud: false,
    });
    const features = {
      preimageAttachments: true,
      search: false,
      calls: { audio: true, video: false },
      readAloud: true,
    };
    await chat.setChatRoomFeatures(product, { roomId: "main", features });
    expect(await chat.readChatRoomFeatures(product, "main")).toEqual(features);

    // A room the host stores holds no message `m1` yet.
    await expect(
      chat.setChatMessageMeta(product, {
        roomId: "main",
        messageId: "m1",
        meta: {},
      }),
    ).rejects.toThrow("UnknownMessage");
    await chat.updateChatFeed(
      product,
      { roomId: "main", update: { tag: "New", value: message("m1", 1n) } },
      true,
    );
    const meta = { branch: { index: 1, count: 2 } };
    await chat.setChatMessageMeta(product, {
      roomId: "main",
      messageId: "m1",
      meta,
    });
    const call = {
      tag: "Ringing" as const,
      value: { callId: "c1", video: false },
    };
    await chat.setChatRoomCall(product, { roomId: "main", call });

    expect(host.room("main")).toMatchObject({
      features,
      meta: { m1: meta },
      call,
    });
    await chat.setChatRoomCall(product, { roomId: "main" });
    expect(host.room("main")?.call).toBeUndefined();
  });

  it("keeps one persona for each bot id, and a repeat replaces it", async () => {
    const { chat } = createMockHost().callbacks;
    const register = (name: string) =>
      chat.registerChatBot(product, { botId: "helper", name, icon: "" });
    expect(await register("Helper")).toEqual({ status: "New" });
    expect(await register("Helper 2")).toEqual({ status: "Exists" });
    expect(await chat.readChatBots(product)).toEqual({ botIds: ["helper"] });
  });

  it("keeps the user state of a room and tells an attached product", async () => {
    const host = await hostWithRoom("main");
    await host.setRoomUserState("quiet", {
      mutedUntil: 18_446_744_073_709_551_615n,
      archived: false,
    });
    const runtime = scripted({});
    host.chatUser.attach(runtime.runtime);
    await host.setRoomUserState("main", { archived: true });

    expect(await host.callbacks.chat.readChatRoomUserStates(product)).toEqual({
      rooms: [
        { roomId: "main", archived: true },
        {
          roomId: "quiet",
          mutedUntil: 18_446_744_073_709_551_615n,
          archived: false,
        },
      ],
    });
    expect(runtime.roomEvents).toEqual([
      {
        roomId: "main",
        event: { tag: "UserStateChanged", value: { archived: true } },
      },
    ]);
    await host.setRoomUserState("main", { archived: false });
    expect(
      (await host.callbacks.chat.readChatRoomUserStates(product)).rooms.map(
        ({ roomId }) => roomId,
      ),
    ).toEqual(["quiet"]);
  });

  it("plays the call buttons, the call bar and the branch control", async () => {
    const host = await hostWithRoom("main");
    const { chat } = host.callbacks;
    const runtime = scripted({});
    host.chatUser.attach(runtime.runtime);

    await expect(host.chatUser.pressCall("main", false)).rejects.toThrow(
      "no audio call button",
    );
    await chat.setChatRoomFeatures(product, {
      roomId: "main",
      features: {
        preimageAttachments: false,
        search: false,
        calls: { audio: true, video: true },
        readAloud: false,
      },
    });
    await host.chatUser.pressCall("main", true);
    await chat.setChatRoomCall(product, {
      roomId: "main",
      call: { tag: "Ringing", value: { callId: "c1", video: true } },
    });
    await host.chatUser.answerCall("main", true);
    await chat.setChatRoomCall(product, {
      roomId: "main",
      call: {
        tag: "Active",
        value: { callId: "c1", video: true, startedAt: 1n },
      },
    });
    await host.chatUser.openCall("main");
    await host.chatUser.endCall("main");

    await chat.updateChatFeed(
      product,
      { roomId: "main", update: { tag: "New", value: message("m1", 1n) } },
      true,
    );
    await chat.setChatMessageMeta(product, {
      roomId: "main",
      messageId: "m1",
      meta: { branch: { index: 1, count: 3 } },
    });
    await expect(host.chatUser.selectBranch("main", "m1", 4)).rejects.toThrow(
      "no alternative 4",
    );
    await host.chatUser.selectBranch("main", "m1", 3);

    expect(runtime.roomEvents.map(({ event }) => event)).toEqual([
      { tag: "CallRequested", value: { video: true } },
      { tag: "CallAnswered", value: { callId: "c1", accept: true } },
      { tag: "CallOpened", value: { callId: "c1" } },
      { tag: "CallEnded", value: { callId: "c1" } },
      { tag: "BranchSelected", value: { messageId: "m1", index: 3 } },
    ]);
    expect(host.roomEvents()).toEqual(runtime.roomEvents);
  });

  it("searches a room it stores itself and asks a room the product keeps", async () => {
    const host = await hostWithRoom("main");
    const { chat } = host.callbacks;
    await chat.createChatRoom(
      product,
      {
        roomId: "source",
        metadata: { name: "source" },
        config: { ...CONFIG, productSource: true },
      },
      "",
    );
    for (const [id, text] of [
      ["m1", "Hello there"],
      ["m2", "nothing"],
      ["m3", "hello again"],
    ] as const) {
      await chat.updateChatFeed(
        product,
        {
          roomId: "main",
          update: { tag: "New", value: message(id, 1n, text) },
        },
        true,
      );
    }
    let answer: unknown = ok({ hits: [message("s1", 5n)], next: "1" });
    const runtime = scripted({ chatSearchQuery: () => answer });
    host.chatUser.attach(runtime.runtime);

    const stored = await host.chatUser.search("main", "HELLO");
    expect(stored?.hits.map(({ messageId }) => messageId)).toEqual([
      "m3",
      "m1",
    ]);

    // A room the product keeps shows search only when it declares it.
    expect(await host.chatUser.search("source", "hi")).toBeUndefined();
    expect(runtime.asked).toEqual([]);
    await chat.setChatRoomFeatures(product, {
      roomId: "source",
      features: { preimageAttachments: false, search: true, readAloud: false },
    });
    expect(await host.chatUser.search("source", "hi")).toEqual({
      hits: [message("s1", 5n)],
      next: "1",
    });
    expect(runtime.asked).toEqual([
      {
        name: "chatSearchQuery",
        request: { roomId: "source", text: "hi", pageSize: 30 },
      },
    ]);

    answer = err({ tag: "Domain", value: { tag: "NotAvailable" } });
    expect(await host.chatUser.search("source", "hi")).toBeUndefined();
    expect(host.chatUser.notices()).toEqual([
      { roomId: "source", text: "Search is not available" },
    ]);
  });

  it("wakes the product through the attached runtime", async () => {
    const host = createMockHost();
    const runtime = scripted({});
    host.chatUser.attach(runtime.runtime);
    const reason: ChatWakeReason = {
      tag: "Notification",
      value: { roomId: "main", watchId: "dev" },
    };
    expect(await host.chatUser.wake(reason)).toEqual({
      tag: "Held",
      value: { untilMs: 30_000 },
    });
    expect(runtime.wakes).toEqual([reason]);
  });
});
