/**
 * Room declarations and lifecycle of the sample product on the test host:
 * S1, S2, S5 and S45 of 04-TESTING §4.
 */
import { afterEach, describe, expect, test } from "bun:test";
import { V01HostChatCreateRoomRequest } from "@parity/truapi";
import { CHAT_CREATE_ROOM } from "@parity/truapi/wire-table";
import {
  hostStoreConfig,
  productSourceConfig,
  ROOM_NAMES,
  sourcePreview,
  THROWAWAY_NAME,
  THROWAWAY_ROOM,
} from "../../worker/fixtures";
import { startOnTestHost, testHostIsBuilt, texts, until, waitForText, type Running } from "../support/test-host";

const suite = testHostIsBuilt() ? describe : describe.skip;

let running: Running[] = [];
async function start(options: Parameters<typeof startOnTestHost>[0] = {}) {
  const started = await startOnTestHost(options);
  running.push(started);
  return started;
}
afterEach(() => {
  for (const each of running) each.dispose();
  running = [];
});

suite("rooms on the test host", () => {
  test("S1: create_room at the first version gives a host-store room with the defaults", async () => {
    const { transport, host } = await start();
    const { chatUser } = host;
    // The client sends the newest version, so the suite sends `V1` itself:
    // the index byte 0, then `{ roomId, name, icon }`.
    const request = V01HostChatCreateRoomRequest.enc({ roomId: "legacy", name: "Legacy room", icon: "" });
    const payload = new Uint8Array(request.length + 1);
    payload.set(request, 1);
    const response = await transport.request({
      ids: CHAT_CREATE_ROOM,
      payload,
      decodeResponse: (bytes) => ({ success: bytes[0] === 0, value: bytes }) as never,
    });
    expect(response.isOk()).toBe(true);

    const room = host.room("legacy");
    expect(room?.metadata).toEqual({ name: "Legacy room" });
    expect(room?.config).toEqual({
      productSource: false,
      reactionDetails: false,
      participants: false,
      messageActions: false,
      participantActions: false,
      reportSeen: false,
    });
    expect(room?.preview).toBeUndefined();
    expect(room?.footer).toBeUndefined();

    // The host keeps the messages of such a room: a send is stored and posted.
    const sent = await chatUser.send("legacy", "hello");
    expect(sent.tag).toBe("Posted");
    expect(host.feed("legacy").map(({ origin }) => origin.participantId)).toEqual(["user"]);

    // `post_message` of the sample product reaches the store of `host` with a
    // host id, and the host refuses it in the product-source room.
    await chatUser.send("host", "/post legacy");
    const legacy = await waitForText(host, "host", "Legacy message from post_message");
    expect(legacy.messageId).toMatch(/^mock-message:/);
    expect(host.getChatMessageLog().map(({ roomId }) => roomId)).toEqual(["host"]);
    await chatUser.open("source");
    await chatUser.send("source", "/post legacy");
    await until(
      () => host.feed("source").find((message) => message.body.tag === "Text" && message.body.value.text.startsWith("error /post legacy")),
      "the refusal of post_message in source",
    );
  });

  test("S2: create_room V2 keeps the declarations, and a repeat keeps the stored ones", async () => {
    const first = await start();
    const { host } = first;
    expect(host.rooms().map(({ roomId }) => roomId)).toEqual(["host", "source"]);
    expect(host.room("host")).toMatchObject({
      metadata: { name: ROOM_NAMES.host },
      config: hostStoreConfig(),
    });
    expect(host.room("host")?.preview).toBeUndefined();
    expect(host.room("source")).toMatchObject({
      metadata: { name: ROOM_NAMES.source },
      config: productSourceConfig(),
      preview: sourcePreview(),
    });

    // A rename, then a worker restart: `create_room` answers `Exists` and the
    // host keeps what the room declared last.
    await host.chatUser.send("host", "/meta Renamed host");
    await waitForText(host, "host", "ok /meta Renamed host");
    first.dispose();
    running = [];

    const second = await start({ host });
    expect(second.host.room("host")?.metadata.name).toBe("Renamed host");
    expect(second.host.rooms().map(({ roomId }) => roomId)).toEqual(["host", "source"]);
    const boot = await until(() => second.log().find((line) => line.via === "preimage"), "the start line");
    expect(boot.boot).toBe(2);
  });

  test("S5: /room create makes the throwaway room and /room delete removes it", async () => {
    const { host, log } = await start();
    await host.chatUser.send("host", "/room delete");
    await waitForText(host, "host", "error /room delete: no room throwaway");

    await host.chatUser.send("host", "/room create");
    await waitForText(host, "host", "ok /room create");
    expect(host.room(THROWAWAY_ROOM)).toMatchObject({ metadata: { name: THROWAWAY_NAME }, config: hostStoreConfig() });
    expect(host.rooms().map(({ roomId }) => roomId)).toEqual(["host", "source", THROWAWAY_ROOM]);

    // A repeat is idempotent: the host answers `Exists`.
    await host.chatUser.send("host", "/room create");
    await until(() => texts(host, "host").filter((text) => text === "ok /room create").length === 2, "the second create");
    await host.chatUser.open(THROWAWAY_ROOM);

    await host.chatUser.send("host", "/room delete");
    await waitForText(host, "host", "ok /room delete");
    expect(host.room(THROWAWAY_ROOM)).toBeUndefined();
    expect(host.rooms().map(({ roomId }) => roomId)).toEqual(["host", "source"]);
    expect(host.chatUser.isOpen(THROWAWAY_ROOM)).toBe(false);

    await host.chatUser.send("host", "/room delete");
    await until(
      () => texts(host, "host").filter((text) => text === "error /room delete: no room throwaway").length === 2,
      "the second refusal",
    );
    expect(log().filter((line) => line.via === "room").map(({ room, kind }) => [room, kind])).toEqual([
      [THROWAWAY_ROOM, "New"],
      [THROWAWAY_ROOM, "Exists"],
      [THROWAWAY_ROOM, "Deleted"],
    ]);
  });

  test("S5: delete_room removes the room, its messages and declarations, and closes the screen", async () => {
    // `/room delete` deletes only the throwaway room, so the suite deletes
    // `host` with `deleteRoom` on the product's own connection.
    const { client, host } = await start();
    await host.chatUser.send("host", "/post text");
    await waitForText(host, "host", /^Plain text message 0001/);
    await host.chatUser.open("host");
    expect(host.chatUser.isOpen("host")).toBe(true);

    const deleted = await client.chat.deleteRoom({ roomId: "host" });
    expect(deleted.isOk()).toBe(true);
    expect(host.room("host")).toBeUndefined();
    expect(host.feed("host")).toEqual([]);
    expect(host.preview("host")).toBeUndefined();
    expect(host.getChatRooms().map(({ roomId }) => roomId)).toEqual(["source"]);
    expect(host.chatUser.isOpen("host")).toBe(false);
    expect(host.chatUser.notices()).toContainEqual({ roomId: "host", text: "room deleted" });

    // Delete is idempotent, and a room that is gone refuses further
    // declarations.
    expect((await client.chat.deleteRoom({ roomId: "host" })).isOk()).toBe(true);
    expect(
      (await client.chat.setRoomActivity({ roomId: "host", activity: { text: "x" } })).isErr(),
    ).toBe(true);
  });

  test("S45: a query that arrives before the restarted product registers its handler is answered", async () => {
    const first = await start();
    await first.host.chatUser.open("source");
    await first.host.chatUser.send("source", "/history 5");
    await until(() => first.product.store.messages().length === 5, "the history");
    await first.product.idle();
    first.dispose();
    running = [];

    // A new connection for the restarted worker. The host asks for the page
    // before the product code runs, as a host that starts a worker for a
    // query does.
    const { createMockClient } = await import("@parity/truapi-host/testing/client");
    const { PRODUCT } = await import("../support/test-host");
    const mock = await createMockClient({
      host: first.host,
      runtimeConfig: { executionKind: "Worker", productId: PRODUCT },
    });
    const answer = mock.chat.chatMessagesQuery(
      { roomId: "source", anchor: { tag: "Before", value: {} }, pageSize: 40 },
      { timeoutMs: 5_000 },
    );
    await new Promise((resolve) => setTimeout(resolve, 100));
    const { startSampleProduct } = await import("../../worker/product");
    const { loadTestAssets } = await import("../support/test-host");
    const product = startSampleProduct({
      client: mock.client,
      assets: loadTestAssets(),
      logSink: () => {},
      crash: () => {},
    });
    try {
      const page = (await answer)._unsafeUnwrap();
      expect(page.messages.map(({ messageId }) => messageId)).toEqual([
        "source-h-00001",
        "source-h-00002",
        "source-h-00003",
        "source-h-00004",
        "source-h-00005",
      ]);
      await product.idle();
    } finally {
      mock.dispose();
    }
  });
});
