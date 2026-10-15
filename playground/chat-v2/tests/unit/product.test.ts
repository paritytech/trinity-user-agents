import { afterEach, describe, expect, test } from "bun:test";
import type { HostChatUpdateFeedRequest } from "@parity/truapi";
import { bytesToHex } from "../../worker/bytes";
import { EPOCH_MS } from "../../worker/clock";
import { FLOOD_SETTLE_MS } from "../../worker/execute";
import { hostStoreConfig, MISSING_PREIMAGE_KEY, type Assets } from "../../worker/fixtures";
import { startSampleProduct, type SampleProduct } from "../../worker/product";
import { createFakeClient, domainError, type FakeClient } from "../support/fake-client";

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

function start(fake: FakeClient = createFakeClient()) {
  const lines: string[] = [];
  const product = startSampleProduct({
    client: fake.client,
    assets,
    product: "sample.dot",
    logSink: (line) => lines.push(line),
    crash: () => {},
  });
  return { fake, product, lines, log: () => lines.map((line) => JSON.parse(line) as Record<string, unknown>) };
}

function feedUpdates(fake: FakeClient): HostChatUpdateFeedRequest[] {
  return fake.callsOf("updateFeed") as HostChatUpdateFeedRequest[];
}

function newTexts(fake: FakeClient, roomId: string): string[] {
  return feedUpdates(fake).flatMap(({ roomId: room, update }) =>
    room === roomId && update.tag === "New" && update.value.body.tag === "Text" ? [update.value.body.value.text] : [],
  );
}

function post(fake: FakeClient, roomId: string, text: string) {
  fake.emitEvent({
    roomId,
    peer: "native",
    event: { tag: "MessagePosted", value: { messageId: `host-${text}`, body: { tag: "Text", value: { text } } } },
  });
}

async function send(fake: FakeClient, product: SampleProduct, text: string) {
  const asked = fake.ask("user_action", {
    roomId: "source",
    action: { tag: "NewMessage", value: { body: { tag: "Text", value: { text } } } },
  });
  const outcome = await asked.outcome;
  await product.idle();
  return outcome;
}

describe("start", () => {
  test("registers every handler before the first await and creates the two rooms", async () => {
    const { fake, product } = start();
    expect([...fake.handlers.keys()].sort()).toEqual([
      "message_actions_query",
      "messages_query",
      "participant_actions_query",
      "participants_query",
      "reactions_query",
      "render",
      "user_action",
    ]);
    expect(fake.callsOf("createRoom")).toEqual([
      expect.objectContaining({ roomId: "host", config: expect.objectContaining({ productSource: false }) }),
      expect.objectContaining({
        roomId: "source",
        config: expect.objectContaining({ productSource: true, reportSeen: true }),
        preview: { order: "Timestamp", badge: { tag: "UnreadCount", value: { count: 0 } } },
      }),
    ]);
    await product.idle();
    expect(fake.callsOf("preimageSubmit")).toEqual([tiny]);
  });
});

describe("source room", () => {
  test("stores a user message, answers MessageCreated, then runs the command", async () => {
    const { fake, product, log } = start();
    const outcome = await send(fake, product, "/post text");
    expect(outcome.item).toEqual({
      tag: "MessageCreated",
      value: {
        messageId: "source-u-0001",
        origin: { participantId: "user" },
        timestamp: EPOCH_MS + 1_000n,
        body: { tag: "Text", value: { text: "/post text" } },
        status: "Sent",
        reactions: [],
      },
    });
    const [update] = feedUpdates(fake);
    expect(update.roomId).toBe("source");
    expect(update.update.tag).toBe("New");
    if (update.update.tag === "New") {
      expect(update.update.value.messageId).toBe("source-p-0001");
      expect(update.update.value.timestamp).toBe(EPOCH_MS + 2_000n);
    }
    expect(product.store.messages().map((m) => m.messageId)).toEqual(["source-u-0001", "source-p-0001"]);
    const lines = log().filter((line) => line.via !== "preimage");
    expect(lines.map((line) => [line.via, line.kind, line.re])).toEqual([
      ["user_action", "NewMessage", undefined],
      ["answer", "MessageCreated", lines[0].seq],
    ]);
  });

  test("pages the stored messages and answers the last page", async () => {
    const { fake, product } = start();
    await send(fake, product, "/history 120");
    expect(newTexts(fake, "source")).toEqual([]);
    const first = await fake.ask("messages_query", { roomId: "source", anchor: { tag: "Before", value: {} }, pageSize: 50 }).outcome;
    expect(first.item).toEqual(expect.objectContaining({ next: "70" }));
    const last = await fake.ask("messages_query", { roomId: "source", anchor: { tag: "Before", value: { cursor: "20" } }, pageSize: 50 }).outcome;
    const page = last.item as { messages: { messageId: string }[]; next?: string };
    expect(page.next).toBeUndefined();
    expect(page.messages.map((m) => m.messageId).at(0)).toBe("source-h-00001");
  });

  test("/fail timeout leaves the next user action without an answer and logs Timeout", async () => {
    const { fake, product, log } = start();
    await send(fake, product, "/fail timeout");
    const asked = fake.ask("user_action", {
      roomId: "source",
      action: { tag: "NewMessage", value: { body: { tag: "Text", value: { text: "lost" } } } },
    });
    await product.idle();
    asked.stop();
    await product.idle();
    expect(log().at(-1)).toEqual(expect.objectContaining({ via: "answer", kind: "Timeout" }));
    expect(product.state.fail.timeout).toBe(false);
  });

  test("/fail reject answers Rejected once", async () => {
    const { fake, product } = start();
    await send(fake, product, "/fail reject");
    const rejected = await send(fake, product, "hello");
    expect(rejected.interrupt).toEqual({
      tag: "Domain",
      value: { tag: "V1", value: { tag: "Rejected", value: { reason: "Rejected by /fail" } } },
    });
    expect((await send(fake, product, "again")).item).toEqual(expect.objectContaining({ tag: "MessageCreated" }));
  });

  test("/fail unavailable interrupts the next query with NotAvailable", async () => {
    const { fake, product } = start();
    await send(fake, product, "/fail unavailable");
    const first = await fake.ask("participants_query", { roomId: "source", query: { tag: "Count" } }).outcome;
    expect(first.interrupt).toEqual({ tag: "Domain", value: { tag: "V1", value: { tag: "NotAvailable" } } });
    const second = await fake.ask("participants_query", { roomId: "source", query: { tag: "Count" } }).outcome;
    expect(second.item).toEqual({ tag: "Count", value: { count: 4 } });
  });

  test("/echo echoes each user action, and not a Seen of an echo", async () => {
    const { fake, product } = start();
    await send(fake, product, "/echo");
    await send(fake, product, "hi");
    const echoes = newTexts(fake, "source").filter((text) => text.startsWith("echo"));
    expect(echoes).toEqual(["echo NewMessage hi"]);
    const echoId = product.store.messages().at(-1)?.messageId;
    await fake.ask("user_action", { roomId: "source", action: { tag: "Seen", value: { messageId: echoId } } }).outcome;
    await product.idle();
    expect(newTexts(fake, "source").filter((text) => text.startsWith("echo"))).toEqual(echoes);
  });
});

describe("host room", () => {
  test("runs a posted command and answers ok", async () => {
    const { fake, product } = start();
    post(fake, "host", "/meta Renamed room");
    await product.idle();
    expect(fake.callsOf("setRoomMetadata")).toEqual([
      { roomId: "host", metadata: { name: "Renamed room", avatar: { tag: "Inline", value: { mimeType: "image/png", bytes: tiny } }, description: "Renamed by /meta" } },
    ]);
    expect(newTexts(fake, "host")).toEqual(["ok /meta Renamed room"]);
  });

  test("answers error and makes no call for a source-only command", async () => {
    const { fake, product } = start();
    post(fake, "host", "/history 5");
    await product.idle();
    expect(newTexts(fake, "host")).toEqual(["error /history 5: only in source"]);
  });

  test("reports a refused call as an error reply", async () => {
    const { fake, product } = start(createFakeClient({ refuse: new Set(["setRoomToolbar"]) }));
    post(fake, "host", "/toolbar 2");
    await product.idle();
    expect(newTexts(fake, "host")).toEqual(["error /toolbar 2: set_room_toolbar Unknown (refused by the fake)"]);
  });

  test("edits an Actions message when a button is pressed", async () => {
    const { fake, product } = start();
    post(fake, "host", "/post actions");
    await product.idle();
    fake.emitEvent({ roomId: "host", peer: "native", event: { tag: "ActionTriggered", value: { messageId: "host-p-0001", actionId: "act-no" } } });
    await product.idle();
    expect(feedUpdates(fake).at(-1)?.update).toEqual({
      tag: "Edited",
      value: { messageId: "host-p-0001", body: { tag: "Actions", value: { text: "Picked No", actions: [], layout: "Column" } }, editedAt: EPOCH_MS + 1_000n },
    });
  });

  test("/participants declares the participants and answers their count", async () => {
    const { fake, product } = start();
    post(fake, "host", "/participants 10000");
    await product.idle();
    expect(fake.callsOf("setRoomConfig")).toEqual([
      { roomId: "host", config: expect.objectContaining({ participants: true, participantActions: true }) },
    ]);
    const count = await fake.ask("participants_query", { roomId: "host", query: { tag: "Count" } }).outcome;
    expect(count.item).toEqual({ tag: "Count", value: { count: 10_004 } });
  });

  test("/stream sends New, forty edits and the Sent status", async () => {
    const { fake, product } = start();
    post(fake, "host", "/stream");
    await product.idle();
    const updates: string[] = feedUpdates(fake).map(({ update }) => update.tag);
    expect(updates).toEqual(["New", ...new Array<string>(40).fill("Edited"), "StatusChanged"]);
  });
});

describe("restart", () => {
  test("a second start continues the counters and counts the boot", async () => {
    const before = createFakeClient();
    const first = start(before);
    post(before, "host", "/post text");
    await first.product.idle();
    const after = createFakeClient({ storage: before.storage });
    const second = start(after);
    post(after, "host", "/post text");
    await second.product.idle();
    const [update] = feedUpdates(after);
    expect(update.update.tag === "New" && update.update.value.messageId).toBe("host-p-0002");
    expect(update.update.tag === "New" && update.update.value.timestamp).toBe(EPOCH_MS + 2_000n);
    expect(second.log()[0].boot).toBe(2);
    expect(after.callsOf("preimageSubmit")).toEqual([]);
  });
});

describe("/room", () => {
  test("creates the throwaway room, deletes it, and refuses a delete with no room", async () => {
    const { fake, product, log } = start();
    post(fake, "host", "/room delete");
    await product.idle();
    expect(fake.callsOf("deleteRoom")).toEqual([]);

    post(fake, "source", "/room create");
    await product.idle();
    expect(fake.callsOf("createRoom").at(-1)).toEqual({
      roomId: "throwaway",
      metadata: expect.objectContaining({ name: "Sample: throwaway" }),
      config: hostStoreConfig(),
    });
    expect(product.state.throwawayCreated).toBe(true);

    post(fake, "host", "/room delete");
    await product.idle();
    expect(fake.callsOf("deleteRoom")).toEqual([{ roomId: "throwaway" }]);
    expect(product.state.throwawayCreated).toBe(false);

    post(fake, "host", "/room delete");
    post(fake, "host", "/room host");
    await product.idle();
    expect(fake.callsOf("deleteRoom")).toHaveLength(1);
    expect(newTexts(fake, "host")).toEqual([
      "error /room delete: no room throwaway",
      "ok /room delete",
      "error /room delete: no room throwaway",
      "error /room host: usage: /room create|delete",
    ]);
    expect(newTexts(fake, "source")).toEqual(["ok /room create"]);
    expect(log().filter((line) => line.via === "room").map(({ room, kind, data }) => [room, kind, data])).toEqual([
      ["throwaway", "New", { roomId: "throwaway" }],
      ["throwaway", "Deleted", { roomId: "throwaway" }],
    ]);
  });
});

describe("/flood", () => {
  /** A fake core that accepts 20 edits and refuses the rest with `RateLimited`. */
  function rateLimited(): FakeClient {
    let edits = 0;
    return createFakeClient({
      fail: (method, _index, request) => {
        if (method !== "updateFeed" || (request as HostChatUpdateFeedRequest).update.tag !== "Edited") return undefined;
        edits += 1;
        return edits > 20 ? { error: domainError({ tag: "RateLimited" }) } : undefined;
      },
    });
  }

  test(
    "sends the edits at once, counts the refused ones, and mirrors the accepted ones in source",
    async () => {
      const { fake, product, log } = start(rateLimited());
      await send(fake, product, "/flood");
      const updates: string[] = feedUpdates(fake).map(({ update }) => update.tag);
      expect(updates).toEqual(["New", ...new Array<string>(25).fill("Edited"), "New"]);
      expect(newTexts(fake, "source")).toEqual(["Flood target", "ok /flood accepted=20 refused=5"]);
      const target = product.store.messages().find((message) => message.messageId === "source-p-0001");
      expect(target?.body).toEqual({ tag: "Text", value: { text: "Flood of 25 edits" } });
      expect(target?.editedAt).toBe(EPOCH_MS + 2_000n);
      const flood = log().filter((line) => line.via === "flood");
      expect(flood).toEqual([
        expect.objectContaining({ room: "source", kind: "Done", data: { sent: 25, accepted: 20, refused: 5 } }),
      ]);
    },
    4 * FLOOD_SETTLE_MS + 1_000,
  );

  test(
    "takes a count, and answers a refused count with usage",
    async () => {
      const { fake, product } = start(rateLimited());
      post(fake, "host", "/flood 3");
      post(fake, "host", "/flood many");
      await product.idle();
      expect(newTexts(fake, "host")).toEqual([
        "Flood target",
        "ok /flood 3 accepted=3 refused=0",
        "error /flood many: usage: /flood [n], 0 to 200",
      ]);
    },
    4 * FLOOD_SETTLE_MS + 1_000,
  );
});

describe("preimage", () => {
  const unhandled: unknown[] = [];
  const onUnhandled = (reason: unknown) => unhandled.push(reason);
  afterEach(() => {
    process.off("unhandledRejection", onUnhandled);
    unhandled.length = 0;
  });

  /** The preimage keys of the `/post rich` messages of a room, in order. */
  function preimageKeys(fake: FakeClient, roomId: string): string[] {
    return feedUpdates(fake).flatMap(({ roomId: room, update }) => {
      if (room !== roomId || update.tag !== "New" || update.value.body.tag !== "RichText") return [];
      return update.value.body.value.media.flatMap((item) => {
        const resource = (item.value as { resource?: { tag: string; value: { key?: string } } }).resource;
        return resource?.tag === "Preimage" && resource.value.key ? [resource.value.key] : [];
      });
    });
  }

  test("a submit that rejects leaves no unhandled rejection and keeps no key", async () => {
    process.on("unhandledRejection", onUnhandled);
    const fake = createFakeClient({
      fail: (method) => (method === "preimageSubmit" ? { reject: new Error("the client is closed") } : undefined),
    });
    const { product, log } = start(fake);
    await product.idle();
    await new Promise((resolve) => setTimeout(resolve, 10));
    expect(unhandled).toEqual([]);
    expect(product.state.preimageKey).toBeUndefined();
    expect(log().filter((line) => line.via === "preimage").map((line) => line.kind)).toEqual(["Missing"]);

    post(fake, "host", "/post rich");
    await product.idle();
    await new Promise((resolve) => setTimeout(resolve, 10));
    expect(unhandled).toEqual([]);
    expect(preimageKeys(fake, "host")).toEqual([MISSING_PREIMAGE_KEY]);
  });

  test("a failed submit is tried again at each /post rich until one succeeds", async () => {
    const key: `0x${string}` = `0x${"ab".repeat(32)}`;
    const fake = createFakeClient({
      fail: (method, index) =>
        method === "preimageSubmit" && index <= 2 ? { error: domainError({ tag: "Unknown", value: { reason: "no preimages" } }) } : undefined,
    });
    const { product, log } = start(fake);
    await product.idle();
    post(fake, "host", "/post rich");
    await product.idle();
    post(fake, "host", "/post rich");
    await product.idle();
    post(fake, "host", "/post rich");
    await product.idle();
    expect(fake.callsOf("preimageSubmit")).toHaveLength(3);
    expect(preimageKeys(fake, "host")).toEqual([MISSING_PREIMAGE_KEY, key, key]);
    expect(product.state.preimageKey).toBe(key);
    expect(log().filter((line) => line.via === "preimage").map((line) => line.kind)).toEqual(["Missing", "Missing", "Submitted"]);
  });

  test("a restart submits again when the saved state holds the missing key", async () => {
    const before = createFakeClient({ fail: (method) => (method === "preimageSubmit" ? { error: domainError({ tag: "Unknown" }) } : undefined) });
    const first = start(before);
    await first.product.idle();
    // A state that an older Worker saved after a failed submit.
    first.product.state.preimageKey = MISSING_PREIMAGE_KEY;
    post(before, "host", "/post text");
    await first.product.idle();
    const after = createFakeClient({ storage: before.storage });
    const second = start(after);
    await second.product.idle();
    expect(after.callsOf("preimageSubmit")).toHaveLength(1);
    expect(second.product.state.preimageKey).toBe(`0x${"ab".repeat(32)}` as const);
  });
});
