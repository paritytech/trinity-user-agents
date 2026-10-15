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
