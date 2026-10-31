import { describe, expect, test } from "bun:test";
import { hexToBytes } from "@parity/truapi/scale";
import type {
  CallResult,
  HostMessage,
} from "../../../../src/invitations/contract.js";
import { cryptoRandom } from "../../../../src/invitations/sealing.js";
import { settle } from "../../../support/test-support.js";
import { KeptStorage } from "../../../support/product-storage.js";
import {
  createInvitation,
  respond,
} from "../../../../fixtures/products/contact-chat/setup.js";
import { ContactChat } from "../../../../fixtures/products/contact-chat/engine.js";
import type { RequestCall } from "../../../../fixtures/products/contact-chat/requests.js";

/**
 * Contact Chat's engine across a restart that falls between the engine and
 * its host: the host acted on a call whose answer the engine never kept.
 */

const CONTACT = `0x${"aa".repeat(32)}` as const;
const HANDLE = `0x${"bb".repeat(16)}`;

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
) {
  return new ContactChat({
    requests: requests.requests,
    storage: await storage.open(["record"]),
    report: (reason) => reports.push(reason),
    recordKey: "record",
    random: cryptoRandom,
    now: () => 1,
  });
}

describe("Contact Chat across a restart", () => {
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
        payload: createInvitation(cryptoRandom).payload,
      },
    });
    await settle();
    expect(first.calls.map((call) => call.call)).toEqual(["reply"]);
    expect(keptAtReply).toContain('"status":"accepting"');
    expect(keptAtReply).toContain('"connection":');
    const kept = bob.stored()[0]!;
    expect([kept.status, kept.connection === undefined]).toEqual([
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
   * The setup's result is the product's: each side keeps the topics and the
   * agreed secret before its host acts (Bob's reply, Alice's complete), and
   * both sides hold the same, so a later transport can start from either
   * after a restart.
   */
  test("both sides keep the same topics and agreed secret before the host replies or completes, and a restart reads them", async () => {
    const aliceStorage = new KeptStorage();
    const bobStorage = new KeptStorage();
    const keptRecord = (storage: KeptStorage) =>
      JSON.parse(
        new TextDecoder().decode(hexToBytes(storage.kept.get("record")!)),
      ) as { conversations: { connection?: unknown }[] };
    let invitation = "";
    let response = "";
    let keptAtReply: unknown;
    let keptAtComplete: unknown;
    const aliceHost = host((call) => {
      if (call.call === "complete") {
        keptAtComplete = keptRecord(aliceStorage).conversations[0]!.connection;
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
      keptAtReply = keptRecord(bobStorage).conversations[0]!.connection;
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

    const aliceSide = alice.stored()[0]!.connection!;
    expect([alice.stored()[0]!.status, bob.stored()[0]!.status]).toEqual([
      "connected",
      "connected",
    ]);
    expect(bob.stored()[0]!.connection).toEqual(aliceSide);
    expect([keptAtReply, keptAtComplete]).toEqual([aliceSide, aliceSide]);
    expect(aliceSide.secret).toMatch(/^0x[0-9a-f]{64}$/);

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
      aliceAgain.stored()[0]!.connection,
      bobAgain.stored()[0]!.connection,
    ]).toEqual([aliceSide, aliceSide]);
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
        payload: createInvitation(cryptoRandom).payload,
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
      payload: respond(invitation, cryptoRandom)!.response,
    });
    await settle();

    expect([
      calls.calls.map((call) => call.call),
      alice.stored()[0]!.status,
    ]).toEqual([["request"], "connected"]);
  });
});
