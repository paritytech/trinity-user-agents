import { describe, expect, test } from "bun:test";
import type { ProductContext } from "@parity/truapi-host";
import {
  BOTS_UNSUPPORTED,
  NOT_SIGNED_IN,
  ProductChat,
  ROOM_MESSAGE_LIMIT,
  productChatKey,
} from "../../../src/chat/product-chat.js";
import { MemoryStorage } from "../../support/test-support.js";

const worker = (productId: string): ProductContext => ({
  productId,
  executionKind: "Worker",
});
const ICON = "data:image/png;base64,AA==";

function signedIn(storage = new MemoryStorage(), wallet = "w1") {
  const chat = new ProductChat(storage, () => 1_000);
  chat.setScope(productChatKey("paseo", wallet));
  return { chat, storage };
}

/** Read the first `count` items of a host subscription. */
async function firstItems<T>(
  stream: AsyncIterable<{ isOk(): boolean; value?: T }>,
  count: number,
): Promise<T[]> {
  const items: T[] = [];
  const iterator = stream[Symbol.asyncIterator]();
  while (items.length < count) {
    const { value } = await iterator.next();
    items.push((value as { value: T }).value);
  }
  await iterator.return?.();
  return items;
}

describe("product chat", () => {
  test("keeps a product's rooms, its text and custom messages, and its footer", async () => {
    const { chat } = signedIn();
    const platform = chat.platform;
    expect(
      await platform.createChatRoom(worker("game.paseo"), {
        roomId: "lobby",
        name: "Lobby",
        icon: ICON,
      }),
    ).toEqual({ status: "New" });
    expect(
      await platform.createChatRoom(worker("game.paseo"), {
        roomId: "lobby",
        name: "Lobby 2",
        icon: ICON,
      }),
    ).toEqual({ status: "Exists" });
    await platform.postChatMessage(worker("game.paseo"), {
      roomId: "lobby",
      payload: { tag: "Text", value: { text: "hi" } },
    });
    await platform.postChatMessage(worker("game.paseo"), {
      roomId: "lobby",
      payload: {
        tag: "Custom",
        value: { messageType: "game.card", payload: "0x0102" },
      },
      alt: "A card",
    });
    await platform.setChatRoomFooter(worker("game.paseo"), {
      roomId: "lobby",
      footer: "Empty",
    });
    expect(chat.room("game.paseo", "lobby")).toMatchObject({
      name: "Lobby 2",
      footer: "Empty",
    });
    expect(
      chat.messages("game.paseo", "lobby").map((message) => message.content),
    ).toEqual([
      { tag: "Text", text: "hi" },
      {
        tag: "Custom",
        messageType: "game.card",
        payload: "0x0102",
        alt: "A card",
      },
    ]);
  });

  test("refuses bots, other message kinds, other products' rooms and a signed-out host", async () => {
    const { chat } = signedIn();
    const platform = chat.platform;
    await expect(
      platform.registerChatBot(worker("game.paseo"), {
        botId: "b",
        name: "B",
        icon: ICON,
      }),
    ).rejects.toThrow(BOTS_UNSUPPORTED);
    await platform.createChatRoom(worker("game.paseo"), {
      roomId: "lobby",
      name: "Lobby",
      icon: ICON,
    });
    // A reaction names one of the room's messages; a made-up id is refused.
    await expect(
      platform.postChatMessage(worker("game.paseo"), {
        roomId: "lobby",
        payload: { tag: "Reaction", value: { messageId: "m", emoji: "+" } },
      }),
    ).rejects.toThrow("There is no message m in room lobby.");
    // A room is the product's own: another product cannot post into it.
    await expect(
      platform.postChatMessage(worker("other.paseo"), {
        roomId: "lobby",
        payload: { tag: "Text", value: { text: "x" } },
      }),
    ).rejects.toThrow("no room lobby");
    chat.setScope(null);
    await expect(
      platform.createChatRoom(worker("game.paseo"), {
        roomId: "r",
        name: "R",
        icon: ICON,
      }),
    ).rejects.toThrow(NOT_SIGNED_IN);
  });

  test("lists each product only its own rooms, and again when they change", async () => {
    const { chat } = signedIn();
    const platform = chat.platform;
    await platform.createChatRoom(worker("other.paseo"), {
      roomId: "elsewhere",
      name: "E",
      icon: ICON,
    });
    const stream = platform.subscribeChatRooms(worker("game.paseo"));
    const reading = firstItems<{ rooms: unknown[] }>(stream, 2);
    await platform.createChatRoom(worker("game.paseo"), {
      roomId: "lobby",
      name: "Lobby",
      icon: ICON,
    });
    expect(await reading).toEqual([
      { rooms: [] },
      { rooms: [{ roomId: "lobby", participatingAs: "RoomHost" }] },
    ]);
  });

  test("keeps each wallet's rooms apart, and keeps them across a reload", async () => {
    const storage = new MemoryStorage();
    const { chat } = signedIn(storage, "w1");
    await chat.platform.createChatRoom(worker("game.paseo"), {
      roomId: "lobby",
      name: "Lobby",
      icon: ICON,
    });
    chat.setScope(productChatKey("paseo", "w2"));
    expect(chat.rooms()).toEqual([]);
    chat.setScope(productChatKey("testnet", "w1"));
    expect(chat.rooms()).toEqual([]);
    const reloaded = new ProductChat(storage);
    reloaded.setScope(productChatKey("paseo", "w1"));
    expect(reloaded.rooms().map((room) => room.roomId)).toEqual(["lobby"]);
  });

  test("keeps only the newest messages of a room", async () => {
    const { chat } = signedIn();
    await chat.platform.createChatRoom(worker("game.paseo"), {
      roomId: "lobby",
      name: "Lobby",
      icon: ICON,
    });
    for (let index = 0; index < ROOM_MESSAGE_LIMIT + 5; index++)
      await chat.platform.postChatMessage(worker("game.paseo"), {
        roomId: "lobby",
        payload: { tag: "Text", value: { text: String(index) } },
      });
    const kept = chat.messages("game.paseo", "lobby");
    expect(kept.length).toBe(ROOM_MESSAGE_LIMIT);
    expect(kept[0].content).toEqual({ tag: "Text", text: "5" });
  });

  test("keeps rich text, action buttons and file links as the product sent them", async () => {
    const { chat } = signedIn();
    const game = worker("game.paseo");
    await chat.platform.createChatRoom(game, {
      roomId: "lobby",
      name: "Lobby",
      icon: ICON,
    });
    await chat.platform.postChatMessage(game, {
      roomId: "lobby",
      payload: {
        tag: "RichText",
        value: { text: "Look", media: [{ url: ICON }] },
      },
    });
    await chat.platform.postChatMessage(game, {
      roomId: "lobby",
      payload: {
        tag: "Actions",
        value: {
          text: "Play again?",
          actions: [
            { actionId: "yes", title: "Yes" },
            { actionId: "no", title: "No" },
          ],
          layout: "Grid",
        },
      },
    });
    await chat.platform.postChatMessage(game, {
      roomId: "lobby",
      payload: {
        tag: "File",
        value: {
          url: "https://files.invalid/rules.pdf",
          fileName: "rules.pdf",
          mimeType: "application/pdf",
          sizeBytes: 1_536n,
        },
      },
    });
    expect(
      chat.messages("game.paseo", "lobby").map((message) => message.content),
    ).toEqual([
      { tag: "RichText", text: "Look", media: [ICON] },
      {
        tag: "Actions",
        text: "Play again?",
        actions: [
          { actionId: "yes", title: "Yes" },
          { actionId: "no", title: "No" },
        ],
        layout: "Grid",
      },
      {
        tag: "File",
        url: "https://files.invalid/rules.pdf",
        fileName: "rules.pdf",
        mimeType: "application/pdf",
        sizeBytes: 1536,
        text: null,
      },
    ]);
  });

  test("places and removes the product's reactions on its own messages, once per emoji", async () => {
    const { chat, storage } = signedIn();
    const game = worker("game.paseo");
    await chat.platform.createChatRoom(game, {
      roomId: "lobby",
      name: "Lobby",
      icon: ICON,
    });
    const { messageId } = await chat.platform.postChatMessage(game, {
      roomId: "lobby",
      payload: { tag: "Text", value: { text: "gg" } },
    });
    const react = (tag: "Reaction" | "ReactionRemoved", emoji: string) =>
      chat.platform.postChatMessage(game, {
        roomId: "lobby",
        payload: { tag, value: { messageId, emoji } },
      });
    await react("Reaction", "👍");
    await react("Reaction", "🎉");
    await react("Reaction", "👍");
    await react("ReactionRemoved", "🎉");
    await react("ReactionRemoved", "🔥");
    const [message] = chat.messages("game.paseo", "lobby");
    // A reaction is not a message of its own.
    expect(chat.messages("game.paseo", "lobby").length).toBe(1);
    expect(message.reactions).toEqual(["👍"]);
    // Another product cannot react in this product's room.
    await expect(
      chat.platform.postChatMessage(worker("other.paseo"), {
        roomId: "lobby",
        payload: { tag: "Reaction", value: { messageId, emoji: "👎" } },
      }),
    ).rejects.toThrow();
    // Kept with the room, so it is there after a reload.
    const reloaded = new ProductChat(storage, () => 1_000);
    reloaded.setScope(productChatKey("paseo", "w1"));
    expect(reloaded.messages("game.paseo", "lobby")[0].reactions).toEqual([
      "👍",
    ]);
  });
});
