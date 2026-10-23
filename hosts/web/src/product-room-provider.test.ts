import { describe, expect, test } from "bun:test";
import type {
  HostChatActionSubscribeItem,
  HostRendererActionSubscribeItem,
  ProductRendererRenderRequest,
} from "@parity/truapi";
import type { ProductContext, RenderSink } from "@parity/truapi-host";
import type { ProviderEvent } from "./chat-provider.js";
import { ProductChat, productChatKey } from "./product-chat.js";
import {
  ProductRoomProvider,
  USER_PEER,
  type RoomChannel,
} from "./product-room-provider.js";
import type { WorkerState } from "./product-workers.js";
import { MemoryStorage } from "./test-support.js";

const PRODUCT = "game.paseo";
const ICON = "data:image/png;base64,AA==";
const worker: ProductContext = { productId: PRODUCT, executionKind: "Worker" };
const TREE = {
  tag: "Text",
  value: { modifiers: [], props: {}, children: [] },
} as never;

/** A worker channel that records what the host asks of it. */
function fakeChannel() {
  const chatActions: HostChatActionSubscribeItem[] = [];
  const rendererActions: HostRendererActionSubscribeItem[] = [];
  const renders: { request: ProductRendererRenderRequest; stopped: boolean }[] =
    [];
  const channel: RoomChannel = {
    render: (request: ProductRendererRenderRequest, sink: RenderSink) => {
      const entry = { request, stopped: false };
      renders.push(entry);
      sink.onUpdate(TREE);
      return () => (entry.stopped = true);
    },
    publishChatAction: async (action) => void chatActions.push(action),
    publishRendererAction: async (item) => void rendererActions.push(item),
  };
  return { channel, chatActions, rendererActions, renders };
}

async function setup() {
  let now = 1_000;
  const chat = new ProductChat(new MemoryStorage(), () => (now += 1));
  chat.setScope(productChatKey("paseo", "w1"));
  await chat.platform.createChatRoom(worker, {
    roomId: "lobby",
    name: "Lobby",
    icon: ICON,
  });
  await chat.platform.postChatMessage(worker, {
    roomId: "lobby",
    payload: { tag: "Text", value: { text: "welcome" } },
  });
  await chat.platform.postChatMessage(worker, {
    roomId: "lobby",
    payload: {
      tag: "Custom",
      value: { messageType: "game.card", payload: "0x01" },
    },
    alt: "A card",
  });
  const fake = fakeChannel();
  let channel: RoomChannel | null = fake.channel;
  let state: WorkerState | null = {
    state: "running",
    manifest: {} as never,
    record: `worker.${PRODUCT}`,
    cid: "cid",
  };
  const workerListeners = new Set<() => void>();
  const logs: string[] = [];
  const provider = new ProductRoomProvider(PRODUCT, {
    chat,
    channel: () => channel,
    workerState: () => state,
    onWorkersChange: (listener) => {
      workerListeners.add(listener);
      return () => workerListeners.delete(listener);
    },
    archiveImage: async () => null,
    gateway: () => "https://gateway.invalid",
    log: (text) => logs.push(text),
  });
  const events: ProviderEvent[] = [];
  provider.subscribe((event) => events.push(event));
  const setWorker = (
    next: RoomChannel | null,
    nextState: WorkerState | null,
  ) => {
    channel = next;
    state = nextState;
    for (const listener of workerListeners) listener();
  };
  return { chat, provider, fake, events, setWorker, logs };
}

describe("product room provider", () => {
  test("lists the product's rooms with the last message as the preview", async () => {
    const { provider } = await setup();
    expect(provider.conversations()).toEqual([
      {
        id: "lobby",
        title: "Lobby",
        subtitle: PRODUCT,
        icon: ICON,
        seed: PRODUCT,
        preview: "A card",
        updatedAt: expect.any(Number),
      },
    ]);
    expect(provider.status()).toEqual({ state: "ready" });
  });

  test("reads the room's messages, the product's as received and the user's as self", async () => {
    const { provider } = await setup();
    await provider.send("lobby", "hi");
    const { messages, before } = await provider.timeline("lobby");
    expect(before).toBeUndefined();
    expect(
      messages.map((message) => [
        message.sender,
        message.status,
        message.content.tag,
      ]),
    ).toEqual([
      [{ id: PRODUCT, label: "Lobby", self: false }, "received", "Text"],
      [{ id: PRODUCT, label: "Lobby", self: false }, "received", "Custom"],
      [{ id: USER_PEER, label: "You", self: true }, "sent", "Text"],
    ]);
  });

  test("keeps what the user sends in the room and hands it to the worker as a chat action", async () => {
    const { provider, fake, events } = await setup();
    await provider.send("lobby", "hi");
    expect(fake.chatActions).toEqual([
      {
        roomId: "lobby",
        peer: USER_PEER,
        payload: {
          tag: "MessagePosted",
          value: { tag: "Text", value: { text: "hi" } },
        },
      },
    ]);
    expect(events).toContainEqual({
      type: "messages",
      conversationId: "lobby",
    });
  });

  test("draws a custom message through the worker's renderer, and sends its actions back", async () => {
    const { provider, fake } = await setup();
    const card = (await provider.timeline("lobby")).messages[1];
    const drawn: unknown[] = [];
    let hooks: { action(id: string, payload: Uint8Array): void } | undefined;
    const stop = provider.renderCustom("lobby", card, {
      draw: (tree, given) => {
        drawn.push(tree);
        hooks = given;
      },
      fail: (reason) => drawn.push(reason),
    });
    expect(drawn).toEqual([TREE]);
    expect(fake.renders[0].request).toEqual({
      context: {
        tag: "ChatMessage",
        value: {
          roomId: "lobby",
          messageId: card.id,
          messageType: "game.card",
        },
      },
      payload: "0x01",
    });
    hooks!.action("bump", Uint8Array.of(7));
    await Promise.resolve();
    expect(fake.rendererActions).toEqual([
      {
        context: fake.renders[0].request.context,
        actionId: "bump",
        payload: "0x07",
      },
    ]);
    stop();
    expect(fake.renders[0].stopped).toBe(true);
  });

  test("a stopped worker blocks the composer, and a new worker is a new epoch", async () => {
    const { provider, setWorker, fake } = await setup();
    expect(provider.composer("lobby")).toEqual({ kind: "text" });
    setWorker(null, null);
    expect(provider.status()).toEqual({
      state: "offline",
      reason: "The product's worker is not running.",
    });
    expect(provider.composer("lobby")).toEqual({
      kind: "blocked",
      reason: "The product's worker is not running",
    });
    const card = (await provider.timeline("lobby")).messages[1];
    const failures: string[] = [];
    provider.renderCustom("lobby", card, {
      draw: () => {},
      fail: (reason) => failures.push(reason),
    });
    expect(failures).toEqual(["The product's worker is not running."]);
    await expect(provider.send("lobby", "hi")).rejects.toThrow("not running");
    expect(provider.epoch()).toBe(1);
    setWorker(fake.channel, { state: "starting" });
    expect(provider.epoch()).toBe(2);
  });

  test("a room with an empty footer shows nothing below its messages", async () => {
    const { chat, provider } = await setup();
    await chat.platform.setChatRoomFooter(worker, {
      roomId: "lobby",
      footer: "Empty",
    });
    expect(provider.composer("lobby")).toEqual({ kind: "none" });
  });

  test("a button press goes to the worker as ActionTriggered for that message", async () => {
    const { chat, provider, fake } = await setup();
    const { messageId } = await chat.platform.postChatMessage(worker, {
      roomId: "lobby",
      payload: {
        tag: "Actions",
        value: {
          actions: [{ actionId: "yes", title: "Yes" }],
          layout: "Column",
        },
      },
    });
    const [, , buttons] = (await provider.timeline("lobby")).messages;
    expect(buttons.content).toEqual({
      tag: "Actions",
      text: null,
      actions: [{ actionId: "yes", title: "Yes" }],
      layout: "Column",
    });
    expect(provider.conversations()[0].preview).toBe("Yes");
    await provider.triggerAction("lobby", messageId, "yes");
    expect(fake.chatActions).toEqual([
      {
        roomId: "lobby",
        peer: USER_PEER,
        payload: {
          tag: "ActionTriggered",
          value: { messageId, actionId: "yes" },
        },
      },
    ]);
  });

  test("shows the product's reactions on its message, one per emoji, not as the user's", async () => {
    const { chat, provider } = await setup();
    const [welcome] = (await provider.timeline("lobby")).messages;
    await chat.platform.postChatMessage(worker, {
      roomId: "lobby",
      payload: {
        tag: "Reaction",
        value: { messageId: welcome.id, emoji: "👍" },
      },
    });
    const [again] = (await provider.timeline("lobby")).messages;
    expect(again.reactions).toEqual([{ emoji: "👍", count: 1, self: false }]);
    expect(welcome.reactions).toBeUndefined();
  });

  test("the user's reaction goes to the worker as MessagePosted(Reaction), and a new emoji replaces it", async () => {
    const { provider, fake } = await setup();
    await provider.send("lobby", "hi");
    const [welcome, , mine] = (await provider.timeline("lobby")).messages;
    // Only the product's own messages take a reaction: their ids are the ones it knows.
    expect(welcome.reactable).toBe(true);
    expect(mine.reactable).toBeUndefined();
    fake.chatActions.length = 0;
    await provider.react("lobby", welcome.id, "👍");
    await provider.react("lobby", welcome.id, "🎉");
    await provider.react("lobby", welcome.id, "🎉");
    const posted = (
      tag: "Reaction" | "ReactionRemoved",
      emoji: string,
    ): HostChatActionSubscribeItem => ({
      roomId: "lobby",
      peer: USER_PEER,
      payload: {
        tag: "MessagePosted",
        value: { tag, value: { messageId: welcome.id, emoji } },
      },
    });
    expect(fake.chatActions).toEqual([
      posted("Reaction", "👍"),
      posted("Reaction", "🎉"),
      posted("ReactionRemoved", "👍"),
      posted("ReactionRemoved", "🎉"),
    ]);
    const [after] = (await provider.timeline("lobby")).messages;
    expect(after.reactions).toBeUndefined();
    await provider.react("lobby", welcome.id, "👍");
    const [placed] = (await provider.timeline("lobby")).messages;
    expect(placed.reactions).toEqual([{ emoji: "👍", count: 1, self: true }]);
    await expect(provider.react("lobby", mine.id, "👍")).rejects.toThrow(
      "no product message",
    );
  });
});
