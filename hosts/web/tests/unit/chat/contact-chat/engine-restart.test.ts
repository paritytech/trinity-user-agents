import { describe, expect, test } from "bun:test";
import { bytesToHex, hexToBytes } from "@parity/truapi/scale";
import type {
  CallResult,
  HostMessage,
} from "../../../../src/invitations/contract.js";
import { cryptoRandom, toHex } from "../../../../src/invitations/sealing.js";
import { settle } from "../../../support/test-support.js";
import { KeptStorage } from "../../../support/product-storage.js";
import {
  ContactChat,
  START_OVER,
} from "../../../../src/chat/contact-chat/engine.js";
import type { RequestCall } from "../../../../src/chat/contact-chat/requests.js";
import {
  simpleText,
  type Session,
} from "../../../../src/chat/contact-chat/simple-text.js";
import { MemoryChatStatements } from "../../../support/chat-statements.js";

/**
 * Contact Chat's engine across a restart that falls between the engine and
 * its host: the host acted on a call whose answer the engine never kept.
 */

const CONTACT = `0x${"aa".repeat(32)}` as const;
const HANDLE = `0x${"bb".repeat(16)}`;

/** An invitation from some other product's account. */
const invitation = () =>
  simpleText.invite(toHex(cryptoRandom(32)), cryptoRandom).payload;

/** A host that answers each call with `answer`, or never when it is null, and tells the engine what a test hands it. */
function host(answer: (call: RequestCall) => CallResult | null) {
  let listener: (message: HostMessage) => void = () => {};
  const calls: RequestCall[] = [];
  return {
    calls,
    tell: (message: HostMessage) => listener(message),
    requests: {
      call: (call: RequestCall) => {
        calls.push(call);
        const result = answer(call);
        return result === null
          ? new Promise<CallResult>(() => {})
          : Promise.resolve(result);
      },
      onMessage: (next: (message: HostMessage) => void) => (listener = next),
    },
  };
}

/** A run of the engine over what the host keeps of its storage, as a worker that starts. */
async function engine(
  storage: KeptStorage,
  requests: ReturnType<typeof host>,
  reports: string[] = [],
  now: () => number = () => 1,
) {
  return new ContactChat({
    requests: requests.requests,
    protocol: simpleText,
    statements: new MemoryChatStatements().statements("contact-chat.test"),
    storage: await storage.open(["record"]),
    report: (reason) => reports.push(reason),
    recordKey: "record",
    random: cryptoRandom,
    now,
  });
}

describe("Contact Chat across a restart", () => {
  test("a conversation kept by an earlier version hides its stored encryption notice, and keeps every other line", async () => {
    const notice =
      "Messages here are not encrypted: anyone who reads the Statement Store can read them. The network keeps only each side's last 20 messages, for a short time; this app keeps your history.";
    const line = (id: number, author: string, text: string) => ({
      id: `0x${String(id).padStart(32, "0")}`,
      author,
      text,
      at: id,
      state: author === "self" ? "sent" : "received",
    });
    const messages = [
      line(1, "system", "They accepted your chat request"),
      line(2, "system", notice),
      line(3, "self", notice),
      line(4, "peer", "hello"),
      line(5, "system", "Your message was not sent: no slot"),
    ];
    const storage = new KeptStorage();
    storage.kept.set(
      "record",
      bytesToHex(
        new TextEncoder().encode(
          JSON.stringify({
            version: 3,
            conversations: [
              {
                id: "0x" + "c0".repeat(16),
                contact: CONTACT,
                role: "sender",
                status: "connected",
                createdAt: 1,
                messages,
              },
            ],
          }),
        ),
      ),
    );
    const chat = await engine(
      storage,
      host(() => ({ ok: true })),
    );
    expect(
      chat
        .messages("0x" + "c0".repeat(16))
        .map((message) => [message.author.kind, message.text]),
    ).toEqual([
      ["system", "They accepted your chat request"],
      ["self", notice],
      ["participant", "hello"],
      ["system", "Your message was not sent: no slot"],
    ]);
    // A connection kept from before Start over existed offers it in its menu.
    expect(chat.conversations()[0]!.actions).toEqual([
      { id: START_OVER, label: "Start over" },
    ]);
    // Nothing is taken out of what is kept.
    expect(chat.stored()[0]!.messages).toHaveLength(5);
  });

  test("a request the host sent while the engine had no handle for it is matched again by its contact", async () => {
    const storage = new KeptStorage();
    const silent = host(() => null);
    const first = await engine(storage, silent);
    first.run();
    await first.start({ handle: { bytes: CONTACT } }, "");
    await settle();
    expect([first.stored()[0]!.status, silent.calls.length]).toEqual([
      "sending",
      1,
    ]);

    // The engine ended before the host's answer; the host replays the request.
    const replayed = host(() => ({ ok: true }));
    const second = await engine(storage, replayed);
    second.run();
    replayed.tell({
      kind: "outgoing",
      request: {
        handle: HANDLE,
        contact: { bytes: CONTACT },
        purpose: "Hi",
        createdAt: 1,
        status: "waiting",
      },
    });
    expect([second.stored()[0]!.request, second.stored()[0]!.status]).toEqual([
      HANDLE,
      "waiting",
    ]);
  });

  test("an Accept keeps its connection before the reply leaves, and connects once the host says it took the reply", async () => {
    const storage = new KeptStorage();
    /** What the host kept of the product's storage when the reply reached it. */
    let keptAtReply = "";
    const first = host((call) => {
      if (call.call !== "reply") return { ok: true };
      keptAtReply = new TextDecoder().decode(
        hexToBytes(storage.kept.get("record") ?? "0x"),
      );
      return null;
    });
    const bob = await engine(storage, first);
    bob.run();
    first.tell({
      kind: "acceptIntent",
      request: {
        handle: HANDLE,
        sender: { bytes: CONTACT },
        payload: invitation(),
      },
    });
    await settle();
    expect(first.calls.map((call) => call.call)).toEqual(["reply"]);
    expect(keptAtReply).toContain('"status":"accepting"');
    expect(keptAtReply).toContain('"session":');
    const kept = bob.stored()[0]!;
    expect([kept.status, kept.session === undefined]).toEqual([
      "accepting",
      false,
    ]);

    // The engine ended before the host answered; the host replays that it replied.
    const replayed = host(() => ({ ok: true }));
    const again = await engine(storage, replayed);
    again.run();
    replayed.tell({
      kind: "replied",
      request: { handle: HANDLE, sender: { bytes: CONTACT }, repliedAt: 1 },
    });
    await settle();
    expect(again.stored()[0]!.status).toBe("connected");
  });
  /**
   * The setup's result is the product's: each side keeps its session before
   * its host acts (Bob's reply, Alice's complete), and the two sessions
   * match, so either side reads and writes after a restart.
   */
  test("both sides keep matching sessions before the host replies or completes, and a restart reads them", async () => {
    const aliceStorage = new KeptStorage();
    const bobStorage = new KeptStorage();
    const keptRecord = (storage: KeptStorage) =>
      JSON.parse(
        new TextDecoder().decode(hexToBytes(storage.kept.get("record")!)),
      ) as { conversations: { session?: unknown }[] };
    let invitation = "";
    let response = "";
    let keptAtReply: unknown;
    let keptAtComplete: unknown;
    const aliceHost = host((call) => {
      if (call.call === "complete") {
        keptAtComplete = keptRecord(aliceStorage).conversations[0]!.session;
        return { ok: true };
      }
      if (call.call !== "request") return { ok: true };
      invitation = (call.request as { payload: string }).payload;
      return {
        ok: true,
        request: {
          handle: HANDLE,
          contact: { bytes: CONTACT },
          purpose: "Hi",
          createdAt: 1,
          status: "waiting",
        },
      };
    });
    const bobHost = host((call) => {
      if (call.call !== "reply") return { ok: true };
      keptAtReply = keptRecord(bobStorage).conversations[0]!.session;
      response = call.payload;
      return { ok: true };
    });
    const alice = await engine(aliceStorage, aliceHost);
    const bob = await engine(bobStorage, bobHost);
    alice.run();
    bob.run();
    await alice.start({ handle: { bytes: CONTACT } }, "");
    await settle();
    bobHost.tell({
      kind: "acceptIntent",
      request: {
        handle: HANDLE,
        sender: { bytes: CONTACT },
        payload: invitation,
      },
    });
    await settle();
    aliceHost.tell({ kind: "response", handle: HANDLE, payload: response });
    await settle();

    const aliceSide = alice.stored()[0]!.session as Session;
    const bobSide = bob.stored()[0]!.session as Session;
    expect([alice.stored()[0]!.status, bob.stored()[0]!.status]).toEqual([
      "connected",
      "connected",
    ]);
    expect([bobSide.listen, bobSide.send]).toEqual([
      aliceSide.send,
      aliceSide.listen,
    ]);
    expect([keptAtReply, keptAtComplete]).toEqual([bobSide, aliceSide]);

    const [aliceAgain, bobAgain] = await Promise.all([
      engine(
        aliceStorage,
        host(() => null),
      ),
      engine(
        bobStorage,
        host(() => null),
      ),
    ]);
    expect([
      aliceAgain.stored()[0]!.session,
      bobAgain.stored()[0]!.session,
    ]).toEqual([aliceSide, bobSide]);
  });

  /**
   * A call the host acts on must follow a save the host kept, or a restart
   * finds the host acting on a conversation the engine lost: a reply without
   * its keys, or a request without its secrets.
   */
  test("no request or reply reaches the host while saving fails", async () => {
    const storage = new KeptStorage();
    storage.failing = "the disk is full";
    const calls = host(() => ({ ok: true }));
    const reports: string[] = [];
    const bob = await engine(storage, calls, reports);
    bob.run();
    await bob.start({ handle: { bytes: CONTACT } }, "");
    calls.tell({
      kind: "acceptIntent",
      request: {
        handle: HANDLE,
        sender: { bytes: `0x${"cc".repeat(32)}` },
        payload: invitation(),
      },
    });
    await settle();

    expect({
      calls: calls.calls.map((call) => call.call),
      statuses: bob.stored().map((conversation) => conversation.status),
      kept: storage.kept.size,
      reports: [...new Set(reports)],
    }).toEqual({
      calls: ["refuse"],
      statuses: ["not-sent", "accepting"],
      kept: 0,
      reports: [
        'save: The worker\'s storage did not keep a write: {"tag":"Unknown","value":{"reason":"the disk is full"}}',
      ],
    });
  });

  test("a save kept once the storage recovers lets the next call through", async () => {
    const storage = new KeptStorage();
    storage.failing = "the disk is full";
    const calls = host(() => null);
    const alice = await engine(storage, calls);
    alice.run();
    await alice.start({ handle: { bytes: CONTACT } }, "");
    await settle();
    storage.failing = null;
    await alice.start({ handle: { bytes: CONTACT } }, "");
    await settle();

    expect([calls.calls.map((call) => call.call), storage.kept.size]).toEqual([
      ["request"],
      1,
    ]);
  });
  test("a checked reply is not completed while saving its connection fails", async () => {
    const storage = new KeptStorage();
    let invitation = "";
    const calls = host((call) => {
      if (call.call !== "request") return { ok: true };
      invitation = (call.request as { payload: string }).payload;
      return {
        ok: true,
        request: {
          handle: HANDLE,
          contact: { bytes: CONTACT },
          purpose: "Hi",
          createdAt: 1,
          status: "waiting",
        },
      };
    });
    const alice = await engine(storage, calls);
    alice.run();
    await alice.start({ handle: { bytes: CONTACT } }, "");
    await settle();
    storage.failing = "the disk is full";
    calls.tell({
      kind: "response",
      handle: HANDLE,
      payload: simpleText.respond(
        invitation,
        toHex(cryptoRandom(32)),
        cryptoRandom,
      )!.response,
    });
    await settle();

    expect([
      calls.calls.map((call) => call.call),
      alice.stored()[0]!.status,
    ]).toEqual([["request"], "connected"]);
  });

  test("a message still sending when the engine stopped is failed after a restart, for the user to send again", async () => {
    const storage = new KeptStorage();
    let invited = "";
    let response = "";
    const sender = host((call) => {
      if (call.call === "chatAccount") return null;
      if (call.call !== "request") return { ok: true };
      invited = (call.request as { payload: string }).payload;
      return {
        ok: true,
        request: {
          handle: HANDLE,
          contact: { bytes: CONTACT },
          purpose: "Hi",
          createdAt: 1,
          status: "waiting",
        },
      };
    });
    const alice = await engine(storage, sender);
    alice.run();
    const roomId = await alice.start({ handle: { bytes: CONTACT } }, "");
    await settle();
    response = simpleText.respond(
      invited,
      toHex(cryptoRandom(32)),
      cryptoRandom,
    )!.response;
    sender.tell({ kind: "response", handle: HANDLE, payload: response });
    await settle();
    const id = await alice.send(roomId, "Hello");
    await settle();
    // The host never answered the allowance: the message is still sending when the engine stops.
    expect(alice.messages(roomId).find((m) => m.id === id)!.state).toBe(
      "sending",
    );
    alice.stop();
    const again = await engine(
      storage,
      host(() => null),
    );
    expect(again.messages(roomId).find((m) => m.id === id)!.state).toBe(
      "failed",
    );
  });

  describe("Start over", () => {
    const NEW_HANDLE = `0x${"dd".repeat(16)}`;
    const view = (handle: string, createdAt: number) => ({
      handle,
      contact: { bytes: CONTACT },
      purpose: "Hi",
      createdAt,
      status: "waiting" as const,
    });

    /** Alice connected to the contact over `storage`, as her engine kept it, and what her side would answer. */
    async function connectedAlice(storage: KeptStorage) {
      let invited = "";
      const setup = host((call) => {
        if (call.call !== "request") return { ok: true };
        invited = (call.request as { payload: string }).payload;
        return { ok: true, request: view(HANDLE, 1) };
      });
      const alice = await engine(storage, setup);
      alice.run();
      const roomId = await alice.start({ handle: { bytes: CONTACT } }, "");
      await settle();
      const response = simpleText.respond(
        invited,
        toHex(cryptoRandom(32)),
        cryptoRandom,
      )!.response;
      setup.tell({ kind: "response", handle: HANDLE, payload: response });
      await settle();
      alice.stop();
      return { roomId, response };
    }

    test("a request the host took while the engine stopped is matched from its replay; until then the connection stays, and a late reply of the old request changes nothing", async () => {
      const storage = new KeptStorage();
      const { roomId, response } = await connectedAlice(storage);
      const asking = host(() => null);
      const first = await engine(storage, asking);
      first.run();
      const before = structuredClone(first.stored()[0]!);
      void first.act(roomId, START_OVER);
      await settle();
      // The host is asking the user: nothing has changed but the kept candidate.
      expect(asking.calls.map((call) => call.call)).toEqual(["request"]);
      const { restart, ...kept } = first.stored()[0]!;
      expect(kept).toEqual(before);
      expect(restart).toBeDefined();
      first.stop();

      // The engine ended there; the host took the request and replays it.
      const replayed = host(() => ({ ok: true }));
      const second = await engine(storage, replayed);
      expect(second.stored()[0]!.status).toBe("connected");
      second.run();
      replayed.tell({ kind: "outgoing", request: view(NEW_HANDLE, 1) });
      await settle();
      const after = second.stored()[0]!;
      expect({
        status: after.status,
        request: after.request,
        index: after.index,
        session: after.session,
        restart: after.restart,
      }).toEqual({
        status: "waiting",
        request: NEW_HANDLE,
        index: restart!.index,
        session: undefined,
        restart: undefined,
      });

      // The old request's reply, told again, does not reconnect the old session.
      replayed.tell({ kind: "response", handle: HANDLE, payload: response });
      replayed.tell({
        kind: "replied",
        request: { handle: HANDLE, sender: { bytes: CONTACT }, repliedAt: 1 },
      });
      await settle();
      expect([second.stored()[0]!.status, second.stored()[0]!.session]).toEqual(
        ["waiting", undefined],
      );
    });

    test("a request the host refuses says why and keeps the connection; a candidate the host never took is replaced by the next Start over", async () => {
      const storage = new KeptStorage();
      const { roomId } = await connectedAlice(storage);
      const refusing = host((call) =>
        call.call === "request"
          ? { ok: false, reason: "Refused", detail: "No free slot" }
          : { ok: true },
      );
      const alice = await engine(storage, refusing);
      alice.run();
      const before = structuredClone(alice.stored()[0]!);
      await alice.act(roomId, START_OVER);
      const { messages, ...kept } = alice.stored()[0]!;
      const { messages: earlier, ...was } = before;
      expect(kept).toEqual(was);
      expect(messages.slice(earlier.length).map((m) => m.text)).toEqual([
        "Start over did not happen: No free slot. This chat is as it was.",
      ]);
      // Only Start over is offered, and only while connected.
      await expect(alice.act(roomId, "block")).rejects.toThrow(
        "This option is not offered any more.",
      );
      alice.stop();

      // A candidate kept from a run whose request never reached the host.
      const silent = host(() => null);
      const second = await engine(storage, silent);
      second.run();
      void second.act(roomId, START_OVER);
      await settle();
      second.stop();
      const sending = host((call) =>
        call.call === "request"
          ? { ok: true, request: view(NEW_HANDLE, 1) }
          : { ok: true },
      );
      const third = await engine(storage, sending);
      third.run();
      const stale = third.stored()[0]!.restart!;
      await third.act(roomId, START_OVER);
      const after = third.stored()[0]!;
      expect([
        after.status,
        after.request,
        after.index === stale.index,
        after.restart,
      ]).toEqual(["waiting", NEW_HANDLE, false, undefined]);
    });

    test("a replayed request made before the Start over is not taken for it; one made at or after it is", async () => {
      const storage = new KeptStorage();
      const { roomId } = await connectedAlice(storage);
      const first = await engine(
        storage,
        host(() => null),
        [],
        () => 100,
      );
      first.run();
      void first.act(roomId, START_OVER);
      await settle();
      expect(first.stored()[0]!.restart!.at).toBe(100);
      first.stop();

      const replayed = host(() => ({ ok: true }));
      const second = await engine(storage, replayed, [], () => 100);
      second.run();
      replayed.tell({ kind: "outgoing", request: view(NEW_HANDLE, 99) });
      await settle();
      expect([
        second.stored()[0]!.status,
        second.stored()[0]!.request,
        second.stored()[0]!.restart?.at,
      ]).toEqual(["connected", HANDLE, 100]);
      replayed.tell({ kind: "outgoing", request: view(NEW_HANDLE, 100) });
      await settle();
      expect([second.stored()[0]!.status, second.stored()[0]!.request]).toEqual(
        ["waiting", NEW_HANDLE],
      );
    });

    test("while a Start over waits for the user the menu does not offer it again, and an Accept meanwhile replaces it before any request leaves", async () => {
      const storage = new KeptStorage();
      const { roomId } = await connectedAlice(storage);
      const asking = host(() => null);
      const alice = await engine(storage, asking);
      alice.run();
      void alice.act(roomId, START_OVER);
      await settle();
      expect(alice.conversations()[0]!.actions).toBeUndefined();
      // A second choice from a menu drawn earlier sends nothing, and the room says why.
      await alice.act(roomId, START_OVER);
      expect([
        asking.calls.map((call) => call.call),
        alice.messages(roomId).at(-1)!.text,
      ]).toEqual([
        ["request"],
        "Start over is already waiting for your answer.",
      ]);
      alice.stop();

      // A run where their request is accepted while the Start over is being saved.
      const calls = host(() => ({ ok: true }));
      const again = await engine(storage, calls);
      again.run();
      const before = again.stored()[0]!.restart;
      void again.act(roomId, START_OVER);
      calls.tell({
        kind: "acceptIntent",
        request: {
          handle: NEW_HANDLE,
          sender: { bytes: CONTACT },
          payload: invitation(),
        },
      });
      await settle();
      const after = again.stored()[0]!;
      expect({
        calls: calls.calls.map((call) => call.call),
        status: after.status,
        request: after.request,
        restart: after.restart,
        replacedEarlier: before !== undefined,
      }).toEqual({
        calls: ["reply"],
        status: "connected",
        request: NEW_HANDLE,
        restart: undefined,
        replacedEarlier: true,
      });
    });

    test("an Accept that replaces a connection fails this side's messages still sending in it; they stay in the history and are not sent again", async () => {
      const storage = new KeptStorage();
      const { roomId } = await connectedAlice(storage);
      const calls = host((call) =>
        call.call === "chatAccount" ? null : { ok: true },
      );
      const alice = await engine(storage, calls);
      alice.run();
      const id = await alice.send(roomId, "Still there?");
      await settle();
      expect(alice.messages(roomId).find((m) => m.id === id)!.state).toBe(
        "sending",
      );
      calls.tell({
        kind: "acceptIntent",
        request: {
          handle: NEW_HANDLE,
          sender: { bytes: CONTACT },
          payload: invitation(),
        },
      });
      await settle();
      const after = alice.stored()[0]!;
      expect([
        after.status,
        alice.messages(roomId).find((m) => m.id === id)!.state,
        after.windowStart,
      ]).toEqual(["connected", "failed", after.messages.length - 1]);
      await expect(alice.retry(roomId, id)).rejects.toThrow(
        "This message was written in an earlier connection.",
      );
    });
  });
});
