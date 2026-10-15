/**
 * The commands and the start of the Chat Extension v2 additions, against the
 * fake client: `/caps`, `/features` and `room_events_subscribe`.
 */
import { describe, expect, test } from "bun:test";
import type { HostChatSetRoomFeaturesRequest, HostChatUpdateFeedRequest } from "@parity/truapi";
import { bytesToHex } from "../../worker/bytes";
import { capsText, isUnavailable } from "../../worker/execute";
import type { Assets } from "../../worker/fixtures";
import { startSampleProduct } from "../../worker/product";
import { decodeState, encodeState, initialState } from "../../worker/state";
import { createFakeClient, type FakeClient, type FakeClientOptions } from "../support/fake-client";

const tiny = bytesToHex(new Uint8Array([1, 2, 3]));
const assets: Assets = {
  avatar: tiny,
  avatarAlt: tiny,
  badge: tiny,
  icons: [tiny, tiny, tiny, tiny, tiny, tiny],
  samplePng: tiny,
  samplePdf: tiny,
  sampleMp4: tiny,
};

function start(options: FakeClientOptions = {}, fake: FakeClient = createFakeClient(options)) {
  const lines: string[] = [];
  const product = startSampleProduct({
    client: fake.client,
    assets,
    product: "sample.dot",
    logSink: (line) => lines.push(line),
    crash: () => {},
  });
  return { fake, product, log: () => lines.map((line) => JSON.parse(line) as Record<string, unknown>) };
}

/** Post a command in `host` and wait until the product ran it. */
async function command(running: ReturnType<typeof start>, text: string) {
  running.fake.emitEvent({
    roomId: "host",
    peer: "native",
    event: { tag: "MessagePosted", value: { messageId: `host-${text}`, body: { tag: "Text", value: { text } } } },
  });
  await running.product.idle();
}

/** The bodies of the messages the product posted in `host`, as text. */
function posted(fake: FakeClient): string[] {
  return (fake.callsOf("updateFeed") as HostChatUpdateFeedRequest[]).flatMap(({ roomId, update }) => {
    if (roomId !== "host" || update.tag !== "New") return [];
    const { body } = update.value;
    if (body.tag === "Text") return [body.value.text];
    if (body.tag === "RichText") return [body.value.text ?? ""];
    return [];
  });
}

describe("room events", () => {
  test("the Worker opens room_events_subscribe at start and logs each item", async () => {
    const running = start();
    await running.product.idle();
    running.fake.emitRoomEvent({ roomId: "host", event: { tag: "CallRequested", value: { video: true } } });
    await running.product.idle();

    expect(running.log().filter((line) => line.via === "room_events")).toEqual([
      expect.objectContaining({ room: "host", kind: "CallRequested", data: { video: true } }),
    ]);
  });
});

describe("/caps", () => {
  test("posts the names and both bounds as RichText (S49)", async () => {
    const running = start({
      capabilities: { features: ["search", "call_buttons"], inlineAttachmentMaxBytes: 1_048_576, preimageAttachmentMaxBytes: 0 },
    });
    await command(running, "/caps");

    expect(posted(running.fake)).toEqual(["caps: search, call_buttons; inline 1048576 B; preimage 0 B"]);
    const [request] = (running.fake.callsOf("updateFeed") as HostChatUpdateFeedRequest[]).filter(
      ({ update }) => update.tag === "New",
    );
    expect(request?.update.tag === "New" && request.update.value.body.tag).toBe("RichText");
  });

  test("says none for a host that lists no feature", () => {
    expect(capsText([], 1_048_576, 0)).toBe("caps: none; inline 1048576 B; preimage 0 B");
  });

  test("posts caps unavailable on a host before the additions, which keeps working (S50, S52)", async () => {
    const running = start({ unknown: new Set(["capabilities", "setRoomFeatures", "roomEventsSubscribe"]) });
    await command(running, "/caps");
    await command(running, "/features search on");
    await command(running, "/post text");
    await command(running, "/preview pin");

    expect(posted(running.fake)).toEqual([
      "caps unavailable",
      "error /features search on: set_room_features Unsupported",
      expect.stringContaining("Plain text"),
      "ok /preview pin",
    ]);
    // A refused declaration is not kept.
    expect(running.product.state.rooms.host.features).toEqual({
      preimageAttachments: false,
      search: false,
      readAloud: false,
    });
  });

  test("tells an unknown call from another failure", () => {
    expect(isUnavailable({ tag: "Unsupported" })).toBe(true);
    expect(isUnavailable({ tag: "HostFailure", value: { reason: "unavailable" } })).toBe(true);
    expect(isUnavailable({ tag: "HostFailure", value: { reason: "the host is down" } })).toBe(false);
    expect(isUnavailable({ tag: "Denied" })).toBe(false);
  });
});

describe("/features", () => {
  test("changes one field and keeps the others (S51)", async () => {
    const running = start();
    for (const text of ["/features readaloud on", "/features calls both", "/features preimage on", "/features calls off"]) {
      await command(running, text);
    }

    const declared = (running.fake.callsOf("setRoomFeatures") as HostChatSetRoomFeaturesRequest[]).map(
      ({ features }) => features,
    );
    expect(declared).toEqual([
      { preimageAttachments: false, search: false, readAloud: true },
      { preimageAttachments: false, search: false, readAloud: true, calls: { audio: true, video: true } },
      { preimageAttachments: true, search: false, readAloud: true, calls: { audio: true, video: true } },
      { preimageAttachments: true, search: false, readAloud: true },
    ]);
    expect(posted(running.fake)).toEqual([
      "ok /features readaloud on",
      "ok /features calls both",
      "ok /features preimage on",
      "ok /features calls off",
    ]);
  });

  test("answers the usage for a name or a value it does not know", async () => {
    const running = start();
    for (const text of ["/features", "/features bogus on", "/features search maybe", "/features calls loud"]) {
      await command(running, text);
    }

    expect(running.fake.callsOf("setRoomFeatures")).toEqual([]);
    for (const line of posted(running.fake)) {
      expect(line).toContain("usage: /features preimage|search|readaloud on|off, calls audio|video|both|off");
    }
  });

  test("keeps the features of the room across a worker restart", async () => {
    const storage = new Map<string, `0x${string}`>();
    const first = start({}, createFakeClient({ storage }));
    await command(first, "/features search on");
    await first.product.idle();

    const second = start({}, createFakeClient({ storage }));
    await command(second, "/features readaloud on");
    expect((second.fake.callsOf("setRoomFeatures") as HostChatSetRoomFeaturesRequest[]).at(-1)?.features).toEqual({
      preimageAttachments: false,
      search: true,
      readAloud: true,
    });
  });

  test("a state saved before /features loads with the default features", () => {
    const state = initialState();
    const saved = JSON.parse(encodeState(state)) as { rooms: Record<string, Record<string, unknown>> };
    for (const room of Object.values(saved.rooms)) delete room.features;

    expect(decodeState(JSON.stringify(saved)).rooms.source.features).toEqual({
      preimageAttachments: false,
      search: false,
      readAloud: false,
    });
  });
});

describe("/bot", () => {
  test("registers a persona, posts and types as it (S53)", async () => {
    const running = start();
    for (const text of ["/bot add helper Helper", "/bot add helper Helper Two", "/bot post helper Hello there", "/bot typing helper"]) {
      await command(running, text);
    }

    expect(running.fake.callsOf("registerBot")).toEqual([
      { botId: "helper", name: "Helper", icon: expect.stringMatching(/^data:image\/png;base64,/) },
      { botId: "helper", name: "Helper Two", icon: expect.stringMatching(/^data:image\/png;base64,/) },
    ]);
    expect(posted(running.fake)).toEqual(["bot helper New", "bot helper Exists", "Hello there", "ok /bot typing helper"]);
    const origins = (running.fake.callsOf("updateFeed") as HostChatUpdateFeedRequest[]).flatMap(({ update }) =>
      update.tag === "New" ? [update.value.origin.participantId] : [],
    );
    expect(origins).toEqual(["product", "product", "bot:helper", "product"]);
    expect(running.fake.callsOf("setRoomActivity")).toEqual([
      { roomId: "host", activity: { text: "Sample is typing", participantId: "bot:helper" } },
    ]);
  });

  test("answers the usage without an id or a text", async () => {
    const running = start();
    for (const text of ["/bot", "/bot add helper", "/bot post helper", "/bot wave helper"]) {
      await command(running, text);
    }

    expect(running.fake.callsOf("registerBot")).toEqual([]);
    expect(posted(running.fake).every((line) => line.includes("usage: /bot"))).toBe(true);
  });
});

describe("/commands", () => {
  test("declares the v2 catalogue or the additions catalogue", async () => {
    const running = start();
    await command(running, "/commands");
    await command(running, "/commands additions");
    await command(running, "/commands off");

    const names = (running.fake.callsOf("setRoomCommands") as { commands: { name: string }[] }[]).map(({ commands }) =>
      commands.map((spec) => spec.name),
    );
    expect(names[0]).toContain("history");
    expect(names[0]).not.toContain("bot");
    expect(names[1]).toEqual(expect.arrayContaining(["help", "commands", "caps", "features", "bot"]));
    expect(names[1]).not.toContain("history");
    expect(names[2]).toEqual([]);
  });
});

describe("/notify and the user state", () => {
  test("sends a notification that opens the room, now or later (S54)", async () => {
    const running = start();
    await command(running, "/notify");
    const before = Date.now();
    await command(running, "/notify 60");

    const [now, later] = running.fake.callsOf("sendPushNotification") as {
      text: string;
      deeplink: string;
      scheduledAt?: bigint;
    }[];
    expect(now).toEqual({ text: "Sample notification from host", deeplink: "polkadot://sample.dot/-/chat/host" });
    expect(later.deeplink).toBe("polkadot://sample.dot/-/chat/host");
    expect(Number(later.scheduledAt) - before).toBeGreaterThanOrEqual(60_000);
    expect(posted(running.fake)).toEqual(["ok /notify", "ok /notify 60"]);
  });

  test("echoes UserStateChanged while /echo is on (S54, S55)", async () => {
    const running = start();
    await running.product.idle();
    running.fake.emitRoomEvent({ roomId: "host", event: { tag: "UserStateChanged", value: { archived: true } } });
    await running.product.idle();
    await command(running, "/echo");
    running.fake.emitRoomEvent({
      roomId: "host",
      event: { tag: "UserStateChanged", value: { mutedUntil: 2n ** 64n - 1n, archived: false } },
    });
    await running.product.idle();

    expect(posted(running.fake).filter((line) => line.startsWith("echo UserStateChanged"))).toEqual([
      "echo UserStateChanged muted=forever archived=false",
    ]);
  });
});

describe("media from the user", () => {
  const KEY = `0x${"cd".repeat(32)}` as const;

  test("logs each item, echoes it and reads a preimage back (S57)", async () => {
    const running = start({ preimages: new Map([[KEY, "0x010203"]]) });
    await command(running, "/echo");
    running.fake.emitEvent({
      roomId: "host",
      peer: "native",
      event: {
        tag: "MessagePosted",
        value: {
          messageId: "m-media",
          body: {
            tag: "RichText",
            value: {
              media: [
                { tag: "Image", value: { resource: { tag: "Inline", value: { mimeType: "image/png", bytes: "0x0102" } }, meta: { mimeType: "image/png" } } },
                { tag: "File", value: { resource: { tag: "Preimage", value: { key: KEY } }, meta: { mimeType: "application/pdf", sizeBytes: 6_291_456n }, fileName: "big.pdf" } },
              ],
            },
          },
        },
      },
    });
    await running.product.idle();
    await new Promise((resolve) => setTimeout(resolve, 20));

    expect(posted(running.fake).filter((line) => line.startsWith("echo media"))).toEqual([
      "echo media Image 2 Inline",
      "echo media File 6291456 Preimage",
    ]);
    expect(running.log().filter((line) => line.via === "media").map((line) => (line.data as { line: string }).line)).toEqual([
      "Image 2 Inline",
      "File 6291456 Preimage",
    ]);
    expect(running.log().filter((line) => line.via === "preimage" && line.kind === "Fetched")).toEqual([
      expect.objectContaining({ room: "host", data: { key: KEY, size: 3 } }),
    ]);
  });
});

describe("/search", () => {
  /** Send a command in `source` as the host does: a user action. */
  async function sourceCommand(running: ReturnType<typeof start>, text: string) {
    await running.product.ready;
    const { outcome } = running.fake.ask("user_action", {
      roomId: "source",
      action: { tag: "NewMessage", value: { body: { tag: "Text", value: { text } } } },
    });
    await outcome;
    await running.product.idle();
  }

  test("declares search and pages the stored messages, or answers NotAvailable (S62)", async () => {
    const running = start();
    await sourceCommand(running, "/history 30");
    await sourceCommand(running, "/search on");
    expect((running.fake.callsOf("setRoomFeatures") as HostChatSetRoomFeaturesRequest[]).at(-1)).toEqual({
      roomId: "source",
      features: { preimageAttachments: false, search: true, readAloud: false },
    });

    const found = await running.fake.ask("search_query", { roomId: "source", text: "message 0001", pageSize: 3 }).outcome;
    expect((found.item as { hits: { messageId: string }[] }).hits.map((hit) => hit.messageId)).toEqual([
      "source-h-00019",
      "source-h-00018",
      "source-h-00017",
    ]);
    expect((found.item as { next?: string }).next).toBe("16");

    await sourceCommand(running, "/search na");
    const na = await running.fake.ask("search_query", { roomId: "source", text: "message", pageSize: 3 }).outcome;
    expect(na.interrupt).toEqual({ tag: "Domain", value: { tag: "V1", value: { tag: "NotAvailable" } } });
    // `na` keeps search declared.
    expect((running.fake.callsOf("setRoomFeatures") as HostChatSetRoomFeaturesRequest[]).at(-1)?.features.search).toBe(true);
  });

  test("runs only in the room the product keeps", async () => {
    const running = start();
    await command(running, "/search on");
    expect(posted(running.fake)).toEqual(["error /search on: only in source"]);
  });
});

describe("threads, branches and reasoning", () => {
  test("/thread creates the thread room, three replies and the root summary (S64)", async () => {
    const running = start();
    await command(running, "/bot add helper Helper");
    await command(running, "/post text");
    const root = (running.fake.callsOf("updateFeed") as HostChatUpdateFeedRequest[]).flatMap(({ update }) =>
      update.tag === "New" ? [update.value.messageId] : [],
    )[1];
    await command(running, `/thread ${root}`);

    const thread = `thread-host-${root}`;
    expect(running.fake.callsOf("createRoom").at(-1)).toEqual(expect.objectContaining({ roomId: thread }));
    expect(running.fake.callsOf("setRoomFeatures").at(-1)).toEqual({
      roomId: thread,
      features: { preimageAttachments: false, search: false, readAloud: false, threadOf: { roomId: "host", messageId: root } },
    });
    const replies = (running.fake.callsOf("updateFeed") as HostChatUpdateFeedRequest[]).flatMap(({ roomId, update }) =>
      roomId === thread && update.tag === "New" ? [update.value.origin.participantId] : [],
    );
    expect(replies).toEqual(["p1", "p2", "bot:helper"]);
    expect(running.fake.callsOf("setMessageMeta")).toEqual([
      {
        roomId: "host",
        messageId: root,
        meta: {
          thread: { roomId: thread, replyCount: 3, lastReplyAt: expect.any(BigInt), participantIds: ["p1", "p2", "bot:helper"] },
        },
      },
    ]);
    expect(posted(running.fake).at(-1)).toBe(`ok /thread ${root}`);
  });

  test("a thread room takes commands and has no thread of its own", async () => {
    const running = start();
    await command(running, "/post text");
    await command(running, "/thread last");
    const thread = (running.fake.callsOf("createRoom").at(-1) as { roomId: string }).roomId;
    running.fake.emitEvent({
      roomId: thread,
      peer: "native",
      event: { tag: "MessagePosted", value: { messageId: "t1", body: { tag: "Text", value: { text: "/thread last" } } } },
    });
    await running.product.idle();
    const replies = (running.fake.callsOf("updateFeed") as HostChatUpdateFeedRequest[]).flatMap(({ roomId, update }) =>
      roomId === thread && update.tag === "New" && update.value.body.tag === "Text" ? [update.value.body.value.text] : [],
    );
    expect(replies.at(-1)).toBe("error /thread last: a thread has no thread of its own");
  });

  test("/branches sets the position and BranchSelected answers with the alternative (S65)", async () => {
    const running = start();
    await command(running, "/post text");
    await command(running, "/branches last 3");
    const id = (running.fake.callsOf("setMessageMeta")[0] as { messageId: string }).messageId;
    running.fake.emitRoomEvent({ roomId: "host", event: { tag: "BranchSelected", value: { messageId: id, index: 2 } } });
    await running.product.idle();

    expect(running.fake.callsOf("setMessageMeta")).toEqual([
      { roomId: "host", messageId: id, meta: { branch: { index: 1, count: 3 } } },
      { roomId: "host", messageId: id, meta: { branch: { index: 2, count: 3 } } },
    ]);
    const edits = (running.fake.callsOf("updateFeed") as HostChatUpdateFeedRequest[]).flatMap(({ update }) =>
      update.tag === "Edited" && update.value.body.tag === "Text" ? [update.value.body.value.text] : [],
    );
    expect(edits).toEqual(["Branch 2 of 3"]);
  });

  test("/reason streams a reasoning row and ends it with its duration (S66)", async () => {
    const running = start();
    await command(running, "/reason");

    const updates = (running.fake.callsOf("updateFeed") as HostChatUpdateFeedRequest[]).filter(({ update }) => update.tag !== "New" || update.value.status === "Preparing");
    expect(updates.filter(({ update }) => update.tag === "Edited")).toHaveLength(20);
    expect(updates.at(-1)?.update).toEqual({ tag: "StatusChanged", value: { messageId: expect.any(String), status: "Sent" } });
    expect(running.fake.callsOf("setMessageMeta").map((call) => (call as { meta: unknown }).meta)).toEqual([
      { reasoning: {} },
      { reasoning: { durationMs: 4000 } },
    ]);
  });

  test("/menu thread offers Reply in thread, which creates and opens the thread (S64)", async () => {
    const running = start();
    await command(running, "/post text");
    await command(running, "/menu thread");
    const root = (running.fake.callsOf("updateFeed") as HostChatUpdateFeedRequest[]).flatMap(({ update }) =>
      update.tag === "New" ? [update.value.messageId] : [],
    )[1];
    const menu = await running.fake.ask("message_actions_query", {
      roomId: "host",
      message: { messageId: root, origin: { participantId: "product" }, timestamp: 1n, body: { tag: "Text", value: { text: "x" } }, status: "Sent", reactions: [] },
    }).outcome;
    expect(menu.item).toEqual({
      menu: { tag: "ExtendBuiltIn", value: { actions: [expect.objectContaining({ actionId: "act-thread", label: "Reply in thread" })], hidden: [] } },
    });
    running.fake.emitEvent({
      roomId: "host",
      peer: "native",
      event: { tag: "MessageActionTriggered", value: { messageId: root, actionId: "act-thread" } },
    });
    await running.product.idle();
    expect(running.fake.callsOf("navigateTo")).toEqual([{ url: `polkadot://sample.dot/-/chat/thread-host-${root}` }]);
  });
});

describe("wakes", () => {
  test("Woken refreshes the preview, posts woken and notifies the room (S70)", async () => {
    const running = start();
    await running.product.idle();
    running.fake.emitRoomEvent({ roomId: "host", event: { tag: "Woken", value: { watchId: "dev" } } });
    await running.product.idle();

    expect((running.fake.callsOf("setRoomPreview") as { preview: { text?: { text: string; ongoing: boolean } } }[]).at(-1)?.preview.text).toEqual({
      text: "Woken by dev",
      ongoing: false,
    });
    expect(posted(running.fake)).toEqual(["woken"]);
    expect(running.fake.callsOf("sendPushNotification")).toEqual([
      { text: "Sample woken in host by dev", deeplink: "polkadot://sample.dot/-/chat/host" },
    ]);
  });

  test("/wake watch answers that the host has no receiving API", async () => {
    const running = start();
    await command(running, "/wake watch");
    expect(posted(running.fake)).toEqual(["error /wake watch: this host has no receiving API for a watch"]);
  });
});

describe("/call", () => {
  test("rings, shows the bar, ends, and answers the call events (S72 to S74)", async () => {
    const running = start();
    await command(running, "/call ring video");
    running.fake.emitRoomEvent({ roomId: "host", event: { tag: "CallAnswered", value: { callId: "call-0001", accept: true } } });
    await running.product.idle();
    running.fake.emitRoomEvent({ roomId: "host", event: { tag: "CallEnded", value: { callId: "call-0001" } } });
    await running.product.idle();
    running.fake.emitRoomEvent({ roomId: "host", event: { tag: "CallRequested", value: { video: false } } });
    await running.product.idle();
    await command(running, "/call end");

    expect(running.fake.callsOf("setRoomCall")).toEqual([
      { roomId: "host", call: { tag: "Ringing", value: { callId: "call-0001", video: true, caller: "p1" } } },
      { roomId: "host", call: { tag: "Active", value: { callId: "call-0001", video: true, startedAt: expect.any(BigInt) } } },
      { roomId: "host" },
      { roomId: "host", call: { tag: "Active", value: { callId: "call-0002", video: false, startedAt: expect.any(BigInt) } } },
      { roomId: "host" },
    ]);
    expect(posted(running.fake)).toEqual(["ok /call ring video", "call requested video=false", "ok /call end"]);
  });

  test("a declined call is cleared", async () => {
    const running = start();
    await command(running, "/call ring");
    running.fake.emitRoomEvent({ roomId: "host", event: { tag: "CallAnswered", value: { callId: "call-0001", accept: false } } });
    await running.product.idle();
    expect(running.fake.callsOf("setRoomCall").at(-1)).toEqual({ roomId: "host" });
  });
});
