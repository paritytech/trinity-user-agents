// The chat model of the mock host, without the core: the callbacks keep the
// declarations and the messages, and the user half follows the host rules
// against a scripted product runtime.
import { describe, expect, it } from "bun:test";
import { err, ok } from "neverthrow";

import type {
  ChatMessage,
  ChatRoomConfig,
  HostChatActionSubscribeItem,
} from "@parity/truapi";

import type { ChatHostRuntime, ChatQueryName } from "../chat-queries.js";
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
    publishChatEvent: async (item) => {
      published.push(item);
    },
  };
  return { runtime, published, asked };
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
