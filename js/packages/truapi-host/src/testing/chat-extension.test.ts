// The Chat Extension v2 surface of the test host, against the real core: the
// declarations reach the mock host, the host asks the six queries, and the
// events reach the product. The sample product of `playground/chat-v2` has
// its own end-to-end suite over this host; this suite covers what that
// product cannot show, such as a subscriber at the first protocol version.
import { describe, expect, it } from "bun:test";

import type {
  ChatRoomConfig,
  HostChatActionSubscribeItem,
  ProductChatMessagesQueryRequest,
  TrUApiClient,
  V01HostChatActionSubscribeItem,
} from "@parity/truapi";
import { V01HostChatActionSubscribeItem as V01ItemCodec } from "@parity/truapi";
import { CHAT_ACTION_SUBSCRIBE } from "@parity/truapi/wire-table";

import { createMockClient, type MockClient } from "./create-mock-client.js";
import { wasmIsBuilt } from "./require-wasm.js";

const suite = wasmIsBuilt("testing/truapi_server.js")
  ? describe
  : describe.skip;

const SOURCE_CONFIG: ChatRoomConfig = {
  productSource: true,
  reactionDetails: false,
  participants: false,
  messageActions: false,
  participantActions: false,
  reportSeen: false,
};

const HOST_CONFIG: ChatRoomConfig = { ...SOURCE_CONFIG, productSource: false };

const NEWEST_PAGE: ProductChatMessagesQueryRequest = {
  roomId: "source",
  anchor: { tag: "Before", value: {} },
  pageSize: 40,
};

/** A test client that runs as a Worker, the execution kind chat needs. */
function workerClient(options: { host?: MockClient["host"] } = {}) {
  return createMockClient({
    ...options,
    runtimeConfig: { executionKind: "Worker" },
  });
}

async function createRoom(
  client: TrUApiClient,
  roomId: string,
  config: ChatRoomConfig,
): Promise<string> {
  const created = await client.chat.createRoom({
    roomId,
    metadata: { name: `Room ${roomId}` },
    config,
  });
  return created._unsafeUnwrap().status;
}

const delay = (ms: number) =>
  new Promise<void>((resolve) => setTimeout(resolve, ms));

suite("chat extension v2 on the test host", () => {
  it("keeps the declarations of a new room and keeps them on a repeat (S2)", async () => {
    const { client, host, dispose } = await workerClient();
    try {
      const preview = { order: "PinToTop" as const };
      const first = await client.chat.createRoom({
        roomId: "source",
        metadata: { name: "Source", description: "A room" },
        config: SOURCE_CONFIG,
        preview,
      });
      expect(first._unsafeUnwrap().status).toBe("New");
      const again = await client.chat.createRoom({
        roomId: "source",
        metadata: { name: "Other name" },
        config: HOST_CONFIG,
      });
      expect(again._unsafeUnwrap().status).toBe("Exists");

      const room = host.room("source");
      expect(room?.metadata).toEqual({ name: "Source", description: "A room" });
      expect(room?.config).toEqual(SOURCE_CONFIG);
      expect(room?.preview).toEqual(preview);
      expect(host.rooms().map(({ roomId }) => roomId)).toEqual(["source"]);
    } finally {
      dispose();
    }
  });

  it("answers a query sent before the product registered its handler (P2.6, O4)", async () => {
    const { client, chat, dispose } = await workerClient();
    try {
      expect(await createRoom(client, "source", SOURCE_CONFIG)).toBe("New");
      let settled = false;
      const answer = chat
        .chatMessagesQuery(NEWEST_PAGE, { timeoutMs: 5_000 })
        .then((result) => {
          settled = true;
          return result;
        });
      // The core reads the room configuration, then sends `Start`. The client
      // keeps the frame, because no handler exists yet.
      await delay(200);
      expect(settled).toBe(false);

      client.chat.onMessagesQuery((request, send) => {
        send({ messages: [], next: `after-${request.roomId}` });
      });
      const result = await answer;
      expect(result._unsafeUnwrap()).toEqual({
        messages: [],
        next: "after-source",
      });
    } finally {
      dispose();
    }
  });

  it("gives the typed refusals of a query", async () => {
    const { client, chat, dispose } = await workerClient();
    try {
      await createRoom(client, "source", SOURCE_CONFIG);
      await createRoom(client, "host", HOST_CONFIG);

      // The core answers for a room that does not declare the handler, and
      // the product is not asked.
      let asked = 0;
      client.chat.onReactionsQuery(() => {
        asked++;
      });
      const undeclared = await chat.chatReactionsQuery({
        roomId: "source",
        messageId: "m1",
      });
      expect(undeclared._unsafeUnwrapErr()).toEqual({
        tag: "Domain",
        value: { tag: "NotAvailable" },
      });
      expect(asked).toBe(0);

      // A user action in a room the host stores is refused by the core.
      const hostStore = await chat.chatUserAction({
        roomId: "host",
        action: { tag: "Seen", value: { messageId: "m1" } },
      });
      expect(hostStore.isErr()).toBe(true);

      // The interrupt of the product arrives as its domain error.
      client.chat.onUserAction((_request, _send, interrupt) => {
        interrupt({
          tag: "Domain",
          value: {
            tag: "V1",
            value: { tag: "Rejected", value: { reason: "no" } },
          },
        });
      });
      const rejected = await chat.chatUserAction({
        roomId: "source",
        action: {
          tag: "NewMessage",
          value: { body: { tag: "Text", value: { text: "hi" } } },
        },
      });
      expect(rejected._unsafeUnwrapErr()).toEqual({
        tag: "Domain",
        value: { tag: "Rejected", value: { reason: "no" } },
      });

      // A product that never answers gives `Timeout` after the limit.
      client.chat.onMessagesQuery(() => {});
      const silent = await chat.chatMessagesQuery(NEWEST_PAGE, {
        timeoutMs: 100,
      });
      expect(silent._unsafeUnwrapErr()).toEqual({ tag: "Timeout" });

      // The host stops a query with its signal.
      const controller = new AbortController();
      const stopped = chat.chatMessagesQuery(NEWEST_PAGE, {
        signal: controller.signal,
      });
      controller.abort();
      expect((await stopped)._unsafeUnwrapErr()).toEqual({ tag: "Cancelled" });
    } finally {
      dispose();
    }
  });

  it("gives a subscriber at the first version only the events it can carry (S42)", async () => {
    const { client, transport, chat, dispose } = await workerClient();
    try {
      await createRoom(client, "host", HOST_CONFIG);
      // A connection has one action subscriber, so this suite opens only the
      // subscription at the first version.
      const first: V01HostChatActionSubscribeItem[] = [];
      transport.subscribeRaw({
        ids: CHAT_ACTION_SUBSCRIBE,
        // `V1` of the request, which carries no payload.
        payload: new Uint8Array([0]),
        onReceive: (payload) => {
          // `V1` of the item: the index byte, then the V1 struct.
          expect(payload[0]).toBe(0);
          // A copy, not a view: the decoder reads from the start of the buffer.
          first.push(V01ItemCodec.dec(payload.slice(1)));
        },
      });
      await delay(100);

      const body = { tag: "Text" as const, value: { text: "hello" } };
      const events: HostChatActionSubscribeItem["event"][] = [
        { tag: "MessagePosted", value: { messageId: "m1", body } },
        { tag: "ActionTriggered", value: { messageId: "m1", actionId: "yes" } },
        { tag: "Command", value: { command: "help", payload: "" } },
        { tag: "UserEdited", value: { messageId: "m1", body } },
        { tag: "UserDeleted", value: { messageId: "m1" } },
        {
          tag: "UserReacted",
          value: { messageId: "m1", emoji: "👍", placed: true },
        },
        { tag: "Seen", value: { messageId: "m1" } },
        { tag: "ToolbarActionTriggered", value: { actionId: "tb-1" } },
        { tag: "RoomOpened" },
      ];
      for (const event of events) {
        await chat.publishChatEvent({ roomId: "host", peer: "native", event });
      }
      await delay(200);

      expect(first.map(({ payload }) => payload.tag)).toEqual([
        "MessagePosted",
        "ActionTriggered",
        "Command",
      ]);
    } finally {
      dispose();
    }
  });

  it("keeps the feed of a room the host stores and refuses what it cannot apply", async () => {
    const { client, host, dispose } = await workerClient();
    try {
      await createRoom(client, "host", HOST_CONFIG);
      const message = {
        messageId: "p-1",
        origin: { participantId: "product" },
        timestamp: 1_000n,
        body: { tag: "Text" as const, value: { text: "one" } },
        status: "Sent" as const,
        reactions: [],
      };
      const posted = await client.chat.updateFeed({
        roomId: "host",
        update: { tag: "New", value: message },
      });
      expect(posted.isOk()).toBe(true);
      const duplicate = await client.chat.updateFeed({
        roomId: "host",
        update: { tag: "New", value: message },
      });
      // The WASM bridge gives every host refusal as `Unknown` with the
      // reason, so the variant name arrives as the reason.
      expect(duplicate.isErr()).toBe(true);
      expect(JSON.stringify(duplicate._unsafeUnwrapErr())).toContain(
        "DuplicateMessage",
      );
      // The product cannot write as the user in a room the host stores.
      const forged = await client.chat.updateFeed({
        roomId: "host",
        update: {
          tag: "New",
          value: {
            ...message,
            messageId: "u-1",
            origin: { participantId: "user" },
          },
        },
      });
      expect(forged._unsafeUnwrapErr()).toEqual({
        tag: "Domain",
        value: { tag: "V1", value: { tag: "NotOwner" } },
      });

      expect(host.feed("host").map(({ messageId }) => messageId)).toEqual([
        "p-1",
      ]);
      expect(host.room("host")?.updates).toEqual([
        { update: { tag: "New", value: message }, persist: true },
      ]);
      expect(host.preview("host")).toMatchObject({
        text: "one",
        timestamp: 1_000n,
        badge: { tag: "UnreadCount", value: { count: 1 } },
      });
    } finally {
      dispose();
    }
  });

  it("serves a restarted product from the rooms the host kept (S45)", async () => {
    const first = await workerClient();
    await createRoom(first.client, "source", SOURCE_CONFIG);
    first.dispose();

    const second = await workerClient({ host: first.host });
    try {
      // The query goes out before the restarted product registers anything.
      const answer = second.chat.chatMessagesQuery(NEWEST_PAGE, {
        timeoutMs: 5_000,
      });
      await delay(100);
      second.client.chat.onMessagesQuery((_request, send) => {
        send({ messages: [] });
      });
      expect((await answer)._unsafeUnwrap()).toEqual({ messages: [] });
    } finally {
      second.dispose();
    }
  });
});
