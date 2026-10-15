/**
 * The Chat Extension v2 additions of the sample product on the test host:
 * S49 to S52 of 04-TESTING §4.
 */
import { afterEach, describe, expect, test } from "bun:test";
import { createMockHost, MOCK_CHAT_FEATURE_NAMES } from "@parity/truapi-host/testing";
import { capsText } from "../../worker/execute";
import { startOnTestHost, testHostIsBuilt, until, waitForText, type Running } from "../support/test-host";

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

/** The names the core cannot back, so it removes them from any list. */
const CORE_REMOVES = new Set(["room_calls", "payment_button"]);

suite("chat additions on the test host", () => {
  test("S49: /caps lists what the host serves less what the core cannot back, and both bounds", async () => {
    const { host } = await start();
    await host.chatUser.send("host", "/caps");

    const served = MOCK_CHAT_FEATURE_NAMES.filter((name) => !CORE_REMOVES.has(name));
    await waitForText(host, "host", capsText(served, 1_048_576, 8_388_608));
  });

  test("S49: a host that lists no feature gets caps: none", async () => {
    const { host } = await start({ host: createMockHost({ chatFeatures: [] }) });
    await host.chatUser.send("host", "/caps");
    await waitForText(host, "host", "caps: none; inline 1048576 B; preimage 0 B");
  });

  test("S51: features survive a restart, and a feature the host does not serve is kept", async () => {
    const first = await start();
    const { host } = first;
    await host.chatUser.send("host", "/features preimage on");
    await waitForText(host, "host", "ok /features preimage on");
    await host.chatUser.send("host", "/features readaloud on");
    await waitForText(host, "host", "ok /features readaloud on");
    expect(host.room("host")?.features).toEqual({ preimageAttachments: true, search: false, readAloud: true });
    await first.product.idle();
    first.dispose();
    running = [];

    const second = await start({ host });
    expect(second.host.room("host")?.features).toEqual({ preimageAttachments: true, search: false, readAloud: true });
    // The restarted product changes one field and keeps the two it declared.
    await host.chatUser.send("host", "/features calls video");
    await waitForText(host, "host", "ok /features calls video");
    expect(host.room("host")?.features).toEqual({
      preimageAttachments: true,
      search: false,
      readAloud: true,
      calls: { audio: false, video: true },
    });
  });

  test("S52: the room events start with the muted and archived rooms, and the v2 commands work on", async () => {
    const first = await start();
    const { host } = first;
    await host.setRoomUserState("host", { archived: true });
    await until(
      () => first.log().find((line) => line.via === "room_events" && line.kind === "UserStateChanged"),
      "the live user state",
    );
    first.dispose();
    running = [];

    // A restarted product gets the state first, in the snapshot.
    const second = await start({ host });
    const snapshot = await until(
      () => second.log().find((line) => line.via === "room_events"),
      "the snapshot of the user state",
    );
    expect(snapshot).toMatchObject({ room: "host", kind: "UserStateChanged", data: { archived: true } });

    // Every v2 call goes on as before beside the additions.
    await host.chatUser.send("host", "/post text");
    await waitForText(host, "host", /^Plain text message/);
    await host.chatUser.send("host", "/preview pin");
    await waitForText(host, "host", "ok /preview pin");
    expect(host.preview("host")?.order).toBe("PinToTop");
  });
});

suite("bot personas on the test host", () => {
  test("S53: register_bot answers New then Exists, a persona speaks and types, an unknown one is refused", async () => {
    const { host } = await start();
    await host.chatUser.send("host", "/bot add helper Helper");
    await waitForText(host, "host", "bot helper New");
    await host.chatUser.send("host", "/bot add helper Helper Two");
    await waitForText(host, "host", "bot helper Exists");
    // A repeat replaces the name and the icon of the persona.
    expect(host.getChatBots()).toEqual([
      { botId: "helper", name: "Helper Two", icon: expect.stringMatching(/^data:image\/png;base64,/) },
    ]);

    await host.chatUser.send("host", "/bot post helper Hello from the persona");
    const spoken = await waitForText(host, "host", "Hello from the persona");
    expect(spoken.origin).toEqual({ participantId: "bot:helper" });
    await host.chatUser.send("host", "/bot typing helper");
    await waitForText(host, "host", "ok /bot typing helper");
    expect(host.room("host")?.activity).toEqual({ text: "Sample is typing", participantId: "bot:helper" });

    // The core refuses a persona the product did not register.
    await host.chatUser.send("host", "/bot post ghost Boo");
    await waitForText(host, "host", "error /bot post ghost Boo: update_feed Unknown (unknown bot ghost)");
    await host.chatUser.send("host", "/bot typing ghost");
    await waitForText(host, "host", "error /bot typing ghost: set_room_activity Unknown (unknown bot ghost)");
  });

  test("S53: a persona speaks in the room the product keeps", async () => {
    const { host } = await start();
    await host.chatUser.open("source");
    await host.chatUser.send("source", "/bot add helper Helper");
    await waitForText(host, "source", "bot helper New");
    await host.chatUser.send("source", "/bot post helper In the source room");
    const spoken = await waitForText(host, "source", "In the source room");
    expect(spoken.origin).toEqual({ participantId: "bot:helper" });
  });
});

suite("mute and archive on the test host", () => {
  const HOUR_MS = 60 * 60 * 1000;
  const notifies = (host: Running["host"]) =>
    host.feed("host").filter((message) => message.body.tag === "Text" && message.body.value.text === "ok /notify");

  test("S54: a muted room gets no /notify, the product hears UserStateChanged, unmute shows the next one", async () => {
    const { host } = await start();
    await host.chatUser.send("host", "/echo");
    await waitForText(host, "host", "ok /echo");
    await host.setRoomUserState("host", { mutedUntil: BigInt(Date.now() + HOUR_MS), archived: false });
    await waitForText(host, "host", /^echo UserStateChanged muted=\d+ archived=false$/);

    host.clearNotificationLog();
    await host.chatUser.send("host", "/notify");
    await until(() => notifies(host).length === 1, "the first ok /notify");
    expect(host.getNotificationLog()).toEqual([]);

    await host.setRoomUserState("host", { archived: false });
    await waitForText(host, "host", "echo UserStateChanged muted=none archived=false");
    await host.chatUser.send("host", "/notify");
    await until(() => notifies(host).length === 2, "the second ok /notify");
    expect(host.getNotificationLog()).toEqual([
      expect.objectContaining({ text: "Sample notification from host", deeplink: "polkadot://sample.dot/-/chat/host" }),
    ]);
  });

  test("S55: an archived room stays archived when a message arrives, unarchives, and is still deleted", async () => {
    const { host, client } = await start();
    await host.setRoomUserState("host", { archived: true });
    await host.chatUser.send("host", "/post text");
    await waitForText(host, "host", /^Plain text message/);
    expect(host.roomUserState("host")).toEqual({ archived: true });

    await host.setRoomUserState("host", { archived: false });
    expect(host.roomUserState("host")).toBeUndefined();

    await host.chatUser.send("host", "/placeholder Nothing here");
    await waitForText(host, "host", "ok /placeholder Nothing here");
    await host.setRoomUserState("host-empty", { archived: true });
    const deleted = await client.chat.deleteRoom({ roomId: "host-empty" });
    expect(deleted.isOk()).toBe(true);
    expect(host.room("host-empty")).toBeUndefined();
    expect(host.roomUserState("host-empty")).toBeUndefined();
  });

  test("S56: a restarted worker hears every muted or archived room first", async () => {
    const first = await start();
    const { host } = first;
    await host.setRoomUserState("host", { mutedUntil: BigInt(Date.now() + HOUR_MS), archived: false });
    await host.setRoomUserState("source", { archived: true });
    await first.product.idle();
    first.dispose();
    running = [];

    const second = await start({ host });
    await host.chatUser.publishRoomEvent("host", { tag: "CallRequested", value: { video: false } });
    const roomEvents = await until(() => {
      const lines = second.log().filter((line) => line.via === "room_events");
      return lines.length === 3 ? lines : undefined;
    }, "the snapshot and the live event");
    expect(roomEvents.map((line) => [line.room, line.kind])).toEqual([
      ["host", "UserStateChanged"],
      ["source", "UserStateChanged"],
      ["host", "CallRequested"],
    ]);
    expect(roomEvents[1].data).toEqual({ mutedUntil: null, archived: true });
  });
});

suite("preimage attachments on the test host", () => {
  const MIB = 1024 * 1024;
  const bytes = (size: number) => new Uint8Array(size).map((_, index) => index % 251);
  const fetched = (running: Running, size: number) =>
    until(
      () => running.log().find((line) => line.via === "preimage" && line.kind === "Fetched" && (line.data as { size: number }).size === size),
      `the product read back ${size} bytes`,
    );

  test("S57: a 3 MiB photo and a 6 MiB file arrive as preimages in both modes, and the product reads them", async () => {
    const running = await start();
    const { host } = running;
    for (const room of ["host", "source"]) {
      if (room === "source") await host.chatUser.open("source");
      await host.chatUser.send(room, "/features preimage on");
      await waitForText(host, room, "ok /features preimage on");
      await host.chatUser.send(room, "/echo");
      await waitForText(host, room, "ok /echo");

      const photo = await host.chatUser.sendAttachment(room, { kind: "Image", mimeType: "image/png", bytes: bytes(3 * MIB) });
      expect(photo.tag).toBe("Posted");
      await waitForText(host, room, `echo media Image ${3 * MIB} Preimage`);
      const file = await host.chatUser.sendAttachment(room, {
        kind: "File",
        mimeType: "application/pdf",
        bytes: bytes(6 * MIB),
        fileName: "big.pdf",
      });
      expect(file.tag).toBe("Posted");
      await waitForText(host, room, `echo media File ${6 * MIB} Preimage`);
      // A small one still travels inline.
      await host.chatUser.sendAttachment(room, { kind: "Image", mimeType: "image/png", bytes: bytes(1024) });
      await waitForText(host, room, "echo media Image 1024 Inline");
      await host.chatUser.send(room, "/echo");
      await waitForText(host, room, "ok /echo");
    }
    await fetched(running, 3 * MIB);
    await fetched(running, 6 * MIB);
    expect(running.log().filter((line) => line.via === "preimage" && line.kind === "Fetched")).toHaveLength(4);
  });

  test("S58: without the flag, above 8 MiB or with PreimageSubmit refused, nothing is uploaded or sent", async () => {
    const running = await start();
    const { host } = running;
    const sent = () => host.feed("host").filter((message) => message.body.tag === "RichText" && message.origin.participantId === "user");
    const refused = async (size: number) => {
      const outcome = await host.chatUser.sendAttachment("host", { kind: "File", mimeType: "application/pdf", bytes: bytes(size) });
      return outcome.tag === "Refused" ? outcome.reason : `sent ${outcome.tag}`;
    };

    expect(await refused(3 * MIB)).toContain("does not declare preimage attachments");
    await host.chatUser.send("host", "/features preimage on");
    await waitForText(host, "host", "ok /features preimage on");
    expect(await refused(8 * MIB + 1)).toContain("larger than 8388608 bytes");
    host.revokePermission("PreimageSubmit");
    expect(await refused(3 * MIB)).toContain("refused preimage submission");
    expect(sent()).toEqual([]);
    expect(running.log().filter((line) => line.via === "media")).toEqual([]);
  });
});

suite("search on the test host", () => {
  test("S62: a room the product keeps pages search_query hits; na shows not available; off shows no search", async () => {
    const running = await start();
    const { host } = running;
    await host.chatUser.open("source");
    await host.chatUser.send("source", "/history 30");
    await until(() => running.product.store.messages().length === 30, "the history");
    await host.chatUser.send("source", "/search on");
    await waitForText(host, "source", "ok /search on");

    const first = await host.chatUser.search("source", "Message 0001");
    expect(first?.hits.map((hit) => hit.messageId)).toEqual([
      "source-h-00019",
      "source-h-00018",
      "source-h-00017",
      "source-h-00016",
      "source-h-00015",
      "source-h-00014",
      "source-h-00013",
      "source-h-00012",
      "source-h-00011",
      "source-h-00010",
    ]);
    expect(first?.next).toBeUndefined();
    expect(running.log().filter((line) => line.via === "search_query")).toHaveLength(1);

    await host.chatUser.send("source", "/search na");
    await waitForText(host, "source", "ok /search na");
    expect(await host.chatUser.search("source", "message")).toBeUndefined();
    await host.chatUser.send("source", "/search off");
    await waitForText(host, "source", "ok /search off");
    const asked = running.log().filter((line) => line.via === "search_query").length;
    expect(await host.chatUser.search("source", "message")).toBeUndefined();
    // Without `search` the host shows no search and asks nothing.
    expect(running.log().filter((line) => line.via === "search_query")).toHaveLength(asked);
  });

  test("S61: the host searches a room it stores itself, case ignored, and asks the product nothing", async () => {
    const running = await start();
    const { host } = running;
    await host.chatUser.send("host", "/post text");
    await waitForText(host, "host", /^Plain text message/);
    const found = await host.chatUser.search("host", "PLAIN TEXT");
    expect(found?.hits.map((hit) => hit.body.tag === "Text" && hit.body.value.text.startsWith("Plain text"))).toEqual([true]);
    expect(running.log().filter((line) => line.via === "search_query")).toEqual([]);
  });
});

suite("threads and branches on the test host", () => {
  test("S64: /thread opens a thread room of three replies with the root summary; deleting the parent deletes it", async () => {
    const running = await start();
    const { host, client } = running;
    await host.chatUser.send("host", "/bot add helper Helper");
    await waitForText(host, "host", /^bot helper (New|Exists)$/);
    await host.chatUser.send("host", "/post text");
    const root = await waitForText(host, "host", /^Plain text message/);
    await host.chatUser.send("host", `/thread ${root.messageId}`);
    await waitForText(host, "host", `ok /thread ${root.messageId}`);

    const thread = `thread-host-${root.messageId}`;
    expect(host.room(thread)?.features.threadOf).toEqual({ roomId: "host", messageId: root.messageId });
    expect(host.feed(thread).map((message) => message.origin.participantId)).toEqual(["p1", "p2", "bot:helper"]);
    expect(host.room("host")?.meta[root.messageId]?.thread).toEqual({
      roomId: thread,
      replyCount: 3,
      lastReplyAt: expect.any(BigInt),
      participantIds: ["p1", "p2", "bot:helper"],
    });

    expect((await client.chat.deleteRoom({ roomId: "host" })).isOk()).toBe(true);
    expect(host.room(thread)).toBeUndefined();
  });

  test("S65: a branch arrow sends BranchSelected and the product answers the alternative", async () => {
    const running = await start();
    const { host } = running;
    await host.chatUser.send("host", "/post text");
    const message = await waitForText(host, "host", /^Plain text message/);
    await host.chatUser.send("host", `/branches ${message.messageId} 3`);
    await waitForText(host, "host", `ok /branches ${message.messageId} 3`);
    expect(host.room("host")?.meta[message.messageId]?.branch).toEqual({ index: 1, count: 3 });

    await host.chatUser.selectBranch("host", message.messageId, 2);
    await waitForText(host, "host", "Branch 2 of 3");
    await until(() => host.room("host")?.meta[message.messageId]?.branch?.index === 2, "the new position");
  });
});
