import { describe, expect, test } from "bun:test";
import { bytesToHex } from "@parity/truapi/scale";
import { encodeSigned } from "../../../../src/platform/statement.js";
import type { StatementTraffic } from "../../../../src/platform/statement-observer.js";
import {
  StatementLedger,
  ownership,
} from "../../../../tools/statements/ledger.js";
import { peopleTransport } from "../../../../tools/statements/transport.js";
import {
  observedConnection,
  subscriptionKey,
} from "../../../../src/platform/statement-observer.js";
import { LightClientStatements } from "../../../support/light-client-statements.js";

const LIGHT_CLIENT = peopleTransport("light-client");

const NOW_SECONDS = 1_800_000_000;
const SIGNER = new Uint8Array(32).fill(7);
const SIGNER_HEX = bytesToHex(SIGNER);
const TOPIC = bytesToHex(new Uint8Array(32).fill(1));
const CHANNEL = new Uint8Array(32).fill(2);

/** An encoded statement on TOPIC and CHANNEL, `0x` hex, expiring `ahead` seconds after now. */
function statement(
  ahead: number,
  data = 8,
  channel: Uint8Array | null = CHANNEL,
  signer = SIGNER,
) {
  const encoded = encodeSigned(
    {
      expiry: BigInt(NOW_SECONDS + ahead) << 32n,
      channel,
      topics: [new Uint8Array(32).fill(1)],
      data: new Uint8Array(data).fill(ahead & 0xff),
    },
    signer,
    () => new Uint8Array(64),
  );
  return bytesToHex(encoded);
}

/** How this tab's own refusal reads in these tests. */
const REFUSED_HERE = "statement_submit refused here";

/** A validated product-account claim, as the core answers it: it ties `account` to `product`. */
const claimFor = (book: StatementLedger, product: string, account: string) =>
  book.claim(
    { kind: "product", product, index: "0x" + "09".repeat(32), submit: false },
    async () => ({ account, period: 1, outcome: "validated", seq: 1 }),
  );

function ledger(limits = {}, transport = LIGHT_CLIENT) {
  let now = NOW_SECONDS * 1000;
  const book = new StatementLedger(
    transport,
    limits,
    () => (now += 1),
    (error) => error.message === REFUSED_HERE,
  );
  book.startSession("wallet-a", "wallet A");
  const feed = (...events: StatementTraffic[]) => events.forEach(book.traffic);
  feed({ type: "connected", connection: 1, source: "core" });
  let id = 0;
  const submit = (hex: string, result: unknown) => {
    const request = String(++id);
    feed(
      {
        type: "request",
        connection: 1,
        id: request,
        method: "statement_submit",
        params: [hex],
      },
      {
        type: "response",
        connection: 1,
        id: request,
        method: "statement_submit",
        result,
      },
    );
  };
  const subscribe = (remote: number) => {
    const request = String(++id);
    feed(
      {
        type: "request",
        connection: 1,
        id: request,
        method: "statement_subscribeStatement",
        params: [{ matchAny: [TOPIC] }],
      },
      {
        type: "response",
        connection: 1,
        id: request,
        method: "statement_subscribeStatement",
        result: remote,
      },
    );
  };
  const page = (remote: number, statements: unknown[], remaining?: number) =>
    feed({
      type: "notification",
      connection: 1,
      subscription: String(remote),
      method: "statement_statement",
      result: {
        event: "newStatements",
        data: { statements, ...(remaining === undefined ? {} : { remaining }) },
      },
    });
  return { book, feed, submit, subscribe, page };
}

describe("the statement ledger", () => {
  test("follows a submission to the node's answer, and the account's latest observed statements", () => {
    const { book, submit } = ledger();
    submit(statement(100, 20), { status: "new" });
    const [record] = book.submitList();
    expect(record.outcome).toMatchObject({
      kind: "answered",
      answer: { status: "new" },
    });
    expect(book.counts()).toMatchObject({
      submits: 1,
      answers: { new: 1 },
      submittedDataBytes: 20,
    });
    expect(book.accountRecord(SIGNER_HEX)).toMatchObject({
      submits: 1,
      answers: { new: 1 },
    });
    expect(book.latestObserved(SIGNER_HEX)).toEqual({
      statements: 1,
      dataBytes: 20,
      expired: 0,
      lastObservedAt: book.statementList()[0].lastAt,
      dropped: 0,
    });
  });

  test("a topic counts the submissions asked on it apart from the statements observed on it, and when it last saw one", () => {
    const { book, submit, subscribe, page } = ledger();
    // Asked, but refused: a submission, never an observation.
    submit(statement(50), { status: "rejected", reason: "dataTooLarge" });
    expect(book.topicRecord(TOPIC)).toMatchObject({
      submits: 1,
      wentOut: 0,
      statements: 0,
    });
    expect(book.topicRecord(TOPIC)!.lastAt).toBeUndefined();
    subscribe(3);
    page(3, [statement(60)], 0);
    const delivered = book.statementList()[0].lastAt;
    expect(book.topicRecord(TOPIC)).toMatchObject({
      submits: 1,
      statements: 1,
      lastAt: delivered,
    });
    // Seen again: one statement, a later sighting.
    page(3, [statement(60)]);
    expect(book.topicRecord(TOPIC)!.statements).toBe(1);
    expect(book.topicRecord(TOPIC)!.lastAt).toBeGreaterThan(delivered);
  });

  test("a higher expiry on a channel replaces the latest; an older copy seen later does not", () => {
    const { book, submit, subscribe, page } = ledger();
    const first = statement(100);
    const second = statement(200, 30);
    const third = statement(300, 40);
    submit(first, { status: "new" });
    submit(third, { status: "new" });
    subscribe(5);
    // A replay may hand back a statement the newer one replaced, seen here for the first time.
    page(5, [second], 0);
    const [channel] = book.channelList(SIGNER_HEX);
    expect(channel).toMatchObject({
      replacements: 1,
      older: 1,
      latest: book.submitList()[1].meta!.hash,
    });
    // Only the latest observed of the channel counts.
    expect(book.latestObserved(SIGNER_HEX)).toMatchObject({
      statements: 1,
      dataBytes: 40,
      expired: 0,
    });
  });

  // Many accounts use the same channel id: a channel is one account's, so
  // another signer's statements on it are never its versions.
  test("scopes a channel's statements to its account, the latest and the ones it replaced", () => {
    const { book, submit } = ledger({ statements: 3 });
    const other = new Uint8Array(32).fill(8);
    const otherHex = bytesToHex(other);
    const channelHex = bytesToHex(CHANNEL);
    submit(statement(100, 10), { status: "new" });
    submit(statement(100, 11, CHANNEL, other), { status: "new" });
    submit(statement(200, 12), { status: "new" });
    const hashes = book.submitList().map((record) => record.meta!.hash);
    const versions = (account: string) =>
      book
        .channelStatements(
          account.toUpperCase().replace("0X", "0x"),
          channelHex,
          20,
        )
        .map((entry) => entry.meta.hash);
    expect([versions(SIGNER_HEX), versions(otherHex)]).toEqual([
      [hashes[2], hashes[0]],
      [hashes[1]],
    ]);
    expect(book.channelRecord(SIGNER_HEX, channelHex)).toMatchObject({
      latest: hashes[2],
      replacements: 1,
    });
    expect(book.channelRecord(otherHex, channelHex)!.latest).toBe(hashes[1]);
    // A channel this account never posted on has no record, whoever else did.
    expect(
      book.channelRecord(bytesToHex(new Uint8Array(32).fill(9)), channelHex),
    ).toBeUndefined();
    // Past the limit the oldest statement leaves the list; the channel keeps its latest.
    submit(statement(300, 13, new Uint8Array(32).fill(4)), { status: "new" });
    expect(versions(SIGNER_HEX)).toEqual([hashes[2]]);
    expect(book.channelRecord(SIGNER_HEX, channelHex)!.latest).toBe(hashes[2]);
  });

  // A statement first seen long ago and observed again is what this tab saw
  // last: it lists first, once, and the bound drops what was observed
  // longest ago, not what was first seen.
  test("lists statements by last observation, once each, and drops the one observed longest ago", () => {
    const { book, submit, subscribe, page } = ledger({ statements: 3 });
    const other = new Uint8Array(32).fill(8);
    const otherHex = bytesToHex(other);
    const channelHex = bytesToHex(CHANNEL);
    const early = statement(100, 10);
    const middle = statement(100, 11, new Uint8Array(32).fill(5));
    const late = statement(100, 12, CHANNEL, other);
    submit(early, { status: "new" });
    submit(middle, { status: "new" });
    submit(late, { status: "new" });
    const [earlyHash, middleHash, lateHash] = book
      .submitList()
      .map((record) => record.meta!.hash);
    subscribe(7);
    page(7, [early]);
    const recent = (limit: number) =>
      book.recentStatements(() => true, limit).map((entry) => entry.meta.hash);
    expect(recent(20)).toEqual([earlyHash, lateHash, middleHash]);
    expect(book.statement(earlyHash)).toMatchObject({ sightings: 2 });
    // The cap applies after the order, to the matching ones.
    expect(recent(2)).toEqual([earlyHash, lateHash]);
    expect(
      book
        .recentStatements((entry) => entry.meta.signer === SIGNER_HEX, 1)
        .map((entry) => entry.meta.hash),
    ).toEqual([earlyHash]);
    // A channel's list is its account's alone, in the same order.
    expect(
      book.channelStatements(otherHex, channelHex, 20).map((e) => e.meta.hash),
    ).toEqual([lateHash]);
    // Past the limit, the statement observed longest ago goes: the middle
    // one, not the early one seen again.
    submit(statement(300, 13, null), { status: "new" });
    expect(recent(20)).toHaveLength(3);
    expect(recent(20)).not.toContain(middleHash);
    expect(recent(20).slice(1)).toEqual([earlyHash, lateHash]);
    expect(book.dropped()).toMatchObject({ statements: 1 });
  });

  // Silent eviction: what this tab saw is not what the node holds now, nor a
  // bound on it. More latest observed statements than the grant allows is a
  // plain observation, never turned into room used or left.
  test("latest observed is an observation with its time, and may exceed the grant", () => {
    const { book, submit } = ledger();
    const other = (byte: number) => new Uint8Array(32).fill(byte);
    for (const [ahead, channel] of [
      [100, other(4)],
      [200, other(5)],
      [300, other(6)],
    ] as const)
      submit(statement(ahead, 10, channel), { status: "new" });
    book.grants(book.epoch(), "0xblock", [
      { account: SIGNER_HEX, value: { maxCount: 2, maxSize: 512_000 } },
    ]);
    const observed = book.latestObserved(SIGNER_HEX);
    expect(observed).toEqual({
      statements: 3,
      dataBytes: 30,
      expired: 0,
      lastObservedAt: Math.max(...book.statementList().map((s) => s.lastAt)),
      dropped: 0,
    });
    // The ledger keeps the grant apart: nothing subtracts one from the other.
    expect(book.accountRecord(SIGNER_HEX)!.grant).toMatchObject({
      block: "0xblock",
      value: { maxCount: 2 },
    });
  });

  test("keeps a refusal's reason and numbers, and counts a channel's priority refusals", () => {
    const { book, submit } = ledger();
    submit(statement(100), { status: "new" });
    submit(statement(50), {
      status: "rejected",
      reason: "channelPriorityTooLow",
      submitted_expiry: 1,
      min_expiry: 2,
    });
    submit(statement(300, 9000), {
      status: "rejected",
      reason: "dataTooLarge",
      submitted_size: 9000,
      available_size: 4000,
    });
    const account = book.accountRecord(SIGNER_HEX)!;
    expect(account.lastRefusal?.answer).toMatchObject({
      reason: "dataTooLarge",
      fields: { available_size: 4000 },
    });
    expect(book.channelList(SIGNER_HEX)[0].priorityRefusals).toBe(1);
    // Refused statements are not observed as held.
    expect(book.latestObserved(SIGNER_HEX).statements).toBe(1);
  });

  test("knownExpired is shown as the node said it, counted as observed by no one and refused by no one", () => {
    const { book, submit } = ledger();
    submit(statement(100), { status: "knownExpired" });
    expect(book.latestObserved(SIGNER_HEX).statements).toBe(0);
    expect(book.accountRecord(SIGNER_HEX)!.lastRefusal).toBeUndefined();
    const answer = book
      .eventList()
      .find((event) => event.text.includes("knownExpired"));
    expect(answer?.details.map(([label]) => label)).toContain("Note");
  });

  test("tells a refusal in this tab from an RPC error, and a submission its connection dropped", () => {
    const { book, feed } = ledger();
    feed(
      {
        type: "request",
        connection: 1,
        id: "a",
        method: "statement_submit",
        params: [statement(1)],
      },
      {
        type: "response",
        connection: 1,
        id: "a",
        method: "statement_submit",
        error: { message: REFUSED_HERE },
      },
      {
        type: "request",
        connection: 1,
        id: "b",
        method: "statement_submit",
        params: [statement(2)],
      },
      {
        type: "response",
        connection: 1,
        id: "b",
        method: "statement_submit",
        error: { code: -32603, message: "internal" },
      },
      {
        type: "request",
        connection: 1,
        id: "c",
        method: "statement_submit",
        params: [statement(3)],
      },
      { type: "closed", connection: 1, by: "ended" },
    );
    expect(book.submitList().map((record) => record.outcome.kind)).toEqual([
      "rpc-error",
      "rpc-error",
      "unanswered",
    ]);
    expect(book.counts()).toMatchObject({
      refusedLocally: 1,
      rpcErrors: 1,
      unanswered: 1,
    });
    // The account counts every attempt, and which of them never reached the light client.
    expect(book.accountRecord(SIGNER_HEX)).toMatchObject({
      submits: 3,
      refusedLocally: 1,
      rpcErrors: 1,
      answers: {},
      wentOut: 0,
    });
    expect(
      book
        .eventList()
        .filter((event) => event.kind === "submit")
        .map((event) => [event.source, event.text]),
    ).toEqual([
      [
        "core",
        "Submit #1 asked by the core: 8 data bytes, signer 0x070707…0707",
      ],
      ["host", "Submit #1 refused in this tab: it submits nothing"],
      [
        "core",
        "Submit #2 asked by the core: 8 data bytes, signer 0x070707…0707",
      ],
      ["light-client", "Submit #2: RPC error internal"],
      [
        "core",
        "Submit #3 asked by the core: 8 data bytes, signer 0x070707…0707",
      ],
      [
        "inspector",
        "Submit #3: no answer seen (the connection closed before an answer came)",
      ],
    ]);
  });

  test("classifies pages from the node's remaining count, and says when it cannot", () => {
    const { book, subscribe, page } = ledger();
    subscribe(1);
    page(1, [statement(1)], 2);
    page(1, [statement(2), statement(3)], 0);
    page(1, [statement(4)]);
    page(1, [statement(4)]);
    subscribe(2);
    page(2, [statement(5)]);
    const [replayed, unclassified] = book.subscriptionList();
    expect(replayed).toMatchObject({
      phase: "later",
      lastRemaining: 0,
      byDelivery: { initial: 3, later: 2, unclassified: 0 },
      duplicates: 1,
      statements: 5,
    });
    expect(unclassified).toMatchObject({
      phase: "unknown",
      byDelivery: { initial: 0, later: 0, unclassified: 1 },
    });
  });

  test("a page before the answer naming its subscription joins that subscription", () => {
    const { book, feed, page } = ledger();
    feed({
      type: "request",
      connection: 1,
      id: "s",
      method: "statement_subscribeStatement",
      params: [{ matchAll: [TOPIC] }],
    });
    page(9, [statement(1)], 0);
    feed({
      type: "response",
      connection: 1,
      id: "s",
      method: "statement_subscribeStatement",
      result: 9,
    });
    const subscriptions = book.subscriptionList();
    expect(subscriptions).toHaveLength(1);
    expect(subscriptions[0]).toMatchObject({
      filter: { kind: "matchAll", topics: [TOPIC] },
      state: "open",
      pages: 1,
      statements: 1,
    });
    expect(book.statementList()[0].subscriptions).toEqual([
      subscriptions[0].id,
    ]);
  });

  test("an unsubscribe or a closed connection ends a subscription", () => {
    const { book, feed, subscribe } = ledger();
    subscribe(1);
    subscribe(2);
    feed({
      type: "request",
      connection: 1,
      id: "u",
      method: "statement_unsubscribeStatement",
      params: [1],
    });
    feed({ type: "closed", connection: 1, by: "caller" });
    expect(book.subscriptionList().map((record) => record.endedHow)).toEqual([
      "unsubscribed",
      "connection closed",
    ]);
  });

  test("keeps within its limits, counting what it dropped, while counters count everything", () => {
    const { book, submit } = ledger({ events: 5, statements: 3, submits: 4 });
    for (let index = 1; index <= 6; index += 1)
      submit(statement(index, 8, null), { status: "new" });
    expect(book.eventList()).toHaveLength(5);
    expect(book.statementList()).toHaveLength(3);
    expect(book.submitList()).toHaveLength(4);
    expect(book.dropped()).toMatchObject({ statements: 3, submits: 2 });
    expect(book.counts().submits).toBe(6);
  });

  test("records claims as the core answered them; a validation allocates nothing", async () => {
    const { book } = ledger();
    const account = "0x" + "ab".repeat(32);
    await book.claim(
      {
        kind: "product",
        product: "chat.paseo",
        index: "0x" + "01".repeat(32),
        submit: false,
      },
      async () => ({
        account,
        period: 20_000,
        outcome: "validated",
        seq: 3,
        collection: "People",
      }),
    );
    await book.claim(
      {
        kind: "product",
        product: "chat.paseo",
        index: "0x" + "01".repeat(32),
        submit: true,
      },
      async () => ({
        account,
        period: 20_000,
        outcome: "registered",
        seq: 3,
        collection: "People",
        blockHash: "0xbeef",
      }),
    );
    await expect(
      book.claim(
        { kind: "notification", account: "0x" + "cd".repeat(32), submit: true },
        async () => {
          throw new Error("no free slot");
        },
      ),
    ).rejects.toThrow("no free slot");
    expect(book.counts().claims).toEqual({
      validated: 1,
      registered: 1,
      failed: 1,
    });
    const record = book.accountRecord(account)!;
    expect(record.claims.map((claim) => claim.outcome)).toEqual([
      "validated",
      "registered",
    ]);
    expect(record.roles).toEqual([
      {
        kind: "product-account",
        product: "chat.paseo",
        index: "0x" + "01".repeat(32),
      },
    ]);
    const validated = book
      .eventList()
      .find((event) => event.text.includes("validation only"));
    expect(validated?.failed).toBeUndefined();
    expect(book.accountRecord("0x" + "cd".repeat(32))!.claims[0]).toMatchObject(
      {
        outcome: "failed",
        reason: "no free slot",
      },
    );
  });

  test("links a request to its conversation account and its notification accounts", () => {
    const { book } = ledger();
    const handle = "0x" + "11".repeat(16);
    const account = "0x" + "AB".repeat(32);
    book.invitations({ type: "request-sent", product: "chat.paseo", handle });
    book.invitations({
      type: "delivery-account",
      post: handle,
      account: "0x" + "cd".repeat(32),
      period: 1,
    });
    book.invitations({
      type: "chat-account",
      product: "chat.paseo",
      handle,
      index: "0x" + "01".repeat(32),
      outcome: "allowed",
      account,
      until: 5,
    });
    expect(book.requestRecord(handle)).toMatchObject({
      role: "sender",
      account: account.toLowerCase(),
      index: "0x" + "01".repeat(32),
      notificationAccounts: ["0x" + "cd".repeat(32)],
    });
    expect(book.accountRecord(account)!.roles[0]).toEqual({
      kind: "chat-allowance",
      product: "chat.paseo",
      handle,
    });
  });

  test("ties an account to a chat only where the host allowed it, or a product's report names it, one link per chat", () => {
    const { book } = ledger();
    const handle = "0x" + "11".repeat(16);
    const other = "0x" + "22".repeat(16);
    const own = "0x" + "ab".repeat(32);
    const peer = "0x" + "cd".repeat(32);
    const twice = "0x" + "ef".repeat(32);
    const notification = "0x" + "99".repeat(32);
    const allow = (product: string, at: string, account: string) =>
      book.invitations({
        type: "chat-account",
        product,
        handle: at,
        index: "0x" + "01".repeat(32),
        outcome: "allowed",
        account,
        until: 5,
      });
    book.invitations({ type: "request-sent", product: "kettle.paseo", handle });
    book.invitations({
      type: "delivery-account",
      post: handle,
      account: notification,
      period: 1,
    });
    allow("kettle.paseo", handle, own);
    // Allowed before any report: the host's request is the chat.
    expect(book.chatLinks(own)).toEqual([
      {
        key: `request:${handle}`,
        product: "kettle.paseo",
        source: "host",
        text: "allowed for request 0x1111…1111",
      },
    ]);
    book.diagnostics(
      { name: "kettle.paseo", run: 1 },
      {
        conversations: [
          {
            id: "c1",
            protocol: "kettle/6",
            request: handle,
            account: own,
            peers: [peer],
          },
          { id: "c2", protocol: "kettle/6", account: twice },
        ],
      },
    );
    // Reported for that request: the conversation, once, as the host's tie.
    expect(book.chatLinks(own)).toEqual([
      {
        key: "conversation:kettle.paseo:c1",
        product: "kettle.paseo",
        source: "host",
        text: "allowed for request 0x1111…1111",
      },
    ]);
    // The peer only the product names: its word, marked so.
    expect(book.chatLinks(peer)).toEqual([
      {
        key: "conversation:kettle.paseo:c1",
        product: "kettle.paseo",
        source: "product",
        text: "the peer in conversation c1",
      },
    ]);
    // An account tied to two chats lists both, the host's first.
    book.invitations({
      type: "request-sent",
      product: "kettle.paseo",
      handle: other,
    });
    allow("kettle.paseo", other, twice);
    expect(
      book.chatLinks(twice).map((link) => [link.key, link.source]),
    ).toEqual([
      [`request:${other}`, "host"],
      ["conversation:kettle.paseo:c2", "product"],
    ]);
    // A notification account, or one no one tied, links to no chat.
    expect([
      book.chatLinks(notification),
      book.chatLinks("0x" + "77".repeat(32)),
    ]).toEqual([[], []]);
    // A report another product sends for the same request is not that chat.
    book.diagnostics(
      { name: "other.paseo", run: 1 },
      { conversations: [{ id: "x", protocol: "p", request: other }] },
    );
    expect(book.chatLinks(twice)[0]!.key).toBe(`request:${other}`);
  });

  test("keeps a product's reports as reported, and marks them stale once its worker stops", () => {
    const { book } = ledger();
    const account = "0x" + "ab".repeat(32);
    book.diagnostics(
      { name: "chat.paseo", run: 1 },
      {
        conversations: [
          {
            id: "c1",
            protocol: "contact-chat/simple-text/1",
            account,
            peers: ["0x" + "cd".repeat(32)],
            listen: [TOPIC],
            logical: { sent: 2 },
          },
        ],
      },
    );
    expect(book.conversationList()[0]).toMatchObject({ stale: false, run: 1 });
    // A topic a product names is listed; which product names it is read from its report.
    expect(book.topicList().map((topic) => topic.topic)).toEqual([TOPIC]);
    expect(book.accountRecord("0x" + "cd".repeat(32))!.roles[0].kind).toBe(
      "conversation-peer",
    );
    book.diagnostics({ name: "chat.paseo", run: 1 }, null);
    expect(book.conversationList()[0].stale).toBe(true);
  });

  test("captures data only while capture is on, within its budget, and forgets it when turned off", () => {
    const { book, submit } = ledger({
      payloadBytes: 90,
      payloadPerStatement: 40,
    });
    submit(statement(1, 30), { status: "new" });
    expect(book.statementList()[0].payload).toBeUndefined();
    book.setCapture(true);
    submit(statement(2, 60), { status: "new" });
    submit(statement(3, 30), { status: "new" });
    submit(statement(4, 30), { status: "new" });
    const kept = book
      .statementList()
      .map((entry) => entry.payload?.length ?? 0);
    // 40 bytes cut from 60, then 30, then no room for another 30.
    expect(kept).toEqual([0, 2 + 80, 2 + 60, 0]);
    expect(book.statementList()[1].payloadCut).toBe(true);
    expect(book.payloadUse()).toEqual({ bytes: 70, limit: 90, full: true });
    book.setCapture(false);
    expect(
      book.statementList().every((entry) => entry.payload === undefined),
    ).toBe(true);
    expect(book.payloadUse().bytes).toBe(0);
  });

  test("a new session starts empty; the same session keeps what it has", () => {
    const { book, submit } = ledger();
    submit(statement(1), { status: "new" });
    book.startSession("wallet-a", "wallet A");
    expect(book.submitList()).toHaveLength(1);
    book.startSession("wallet-b", "wallet B");
    expect(book.submitList()).toHaveLength(0);
    expect(book.counts().submits).toBe(0);
    expect(book.eventList().map((event) => event.text)).toEqual([
      "Session: wallet B",
    ]);
  });

  test("a claim or grant read that ends after a session change is not recorded in the new one", async () => {
    const { book } = ledger();
    const account = "0x" + "ab".repeat(32);
    let finish: (value: {
      account: string;
      period: number;
      outcome: string;
      seq: number;
    }) => void = () => {};
    const claim = book.claim(
      {
        kind: "product",
        product: "chat.paseo",
        index: "0x" + "01".repeat(32),
        submit: true,
      },
      () =>
        new Promise<{
          account: string;
          period: number;
          outcome: string;
          seq: number;
        }>((resolve) => (finish = resolve)),
    );
    const epoch = book.epoch();
    book.startSession("wallet-b", "wallet B");
    finish({ account, period: 1, outcome: "registered", seq: 1 });
    // The caller still gets the core's answer.
    expect((await claim).outcome).toBe("registered");
    expect(book.grants(epoch, "0xblock", [{ account, value: null }])).toBe(
      false,
    );
    expect(book.grantsFailed(epoch, [account], "gone")).toBe(false);
    expect(book.accountList()).toEqual([]);
    expect(book.counts().claims).toEqual({});
    expect(book.eventList().map((event) => event.text)).toEqual([
      "Session: wallet B",
    ]);
  });

  test("a session change keeps the core's connection and open subscription, as opened earlier, and drops closed ones", () => {
    const { book, feed, subscribe, submit, page } = ledger();
    subscribe(7);
    page(7, [statement(1)], 0);
    feed({ type: "connected", connection: 2, source: "host" });
    feed({
      type: "request",
      connection: 2,
      id: "p",
      method: "statement_subscribeStatement",
      params: [{ matchAny: [TOPIC] }],
    });
    feed({ type: "closed", connection: 2, by: "caller" });
    book.startSession("wallet-b", "wallet B");
    // The core's connection is still the core's, and its subscription still its own.
    submit(statement(2), { status: "new" });
    page(7, [statement(3)]);
    expect(book.connectionList()).toEqual([
      expect.objectContaining({ id: 1, source: "core", earlier: true }),
    ]);
    expect(book.submitList()[0].source).toBe("core");
    const [subscription] = book.subscriptionList();
    expect(subscription).toMatchObject({
      earlier: true,
      state: "open",
      phase: "later",
      filter: { kind: "matchAny", topics: [TOPIC] },
      pages: 1,
      statements: 1,
      byDelivery: { initial: 0, later: 1, unclassified: 0 },
    });
    expect(book.subscriptionList()).toHaveLength(1);
    expect(book.topicList()[0].subscriptions).toEqual([subscription.id]);
  });

  test("an inspector that throws while recording never changes a claim's result", async () => {
    const { book } = ledger();
    book.onChange(() => {
      throw new Error("the view broke");
    });
    const answer = {
      account: "0x" + "ab".repeat(32),
      period: 1,
      outcome: "registered",
      seq: 2,
    };
    expect(
      await book.claim(
        { kind: "notification", account: answer.account, submit: true },
        async () => answer,
      ),
    ).toBe(answer);
    const failure = new Error("no free slot");
    await expect(
      book.claim(
        { kind: "notification", account: answer.account, submit: true },
        async () => {
          throw failure;
        },
      ),
    ).rejects.toBe(failure);
  });

  test("captured data waiting for an answer counts against the budget, and is released when the wait ends", () => {
    const { book, feed } = ledger({
      submits: 3,
      payloadBytes: 100,
      payloadPerStatement: 40,
    });
    book.setCapture(true);
    const send = (id: string, hex: string) =>
      feed({
        type: "request",
        connection: 1,
        id,
        method: "statement_submit",
        params: [hex],
      });
    send("a", statement(1, 60));
    send("b", statement(2, 60));
    // Two cut to 40 bytes wait: 80 of 100. A third would pass the budget.
    send("c", statement(3, 60));
    expect(book.payloadUse()).toEqual({ bytes: 80, limit: 100, full: true });
    // One more request waiting is too many: "a", the oldest, is unanswered
    // now, with why, not pending forever, and its bytes are released.
    feed({
      type: "request",
      connection: 1,
      id: "s",
      method: "statement_subscribeStatement",
      params: [{ matchAny: [TOPIC] }],
    });
    expect(book.payloadUse().bytes).toBe(40);
    expect(book.submitList().map((record) => record.outcome.kind)).toEqual([
      "unanswered",
      "pending",
      "pending",
    ]);
    expect(book.submitList()[0].outcome).toMatchObject({
      why: "the inspector stopped waiting: too many requests were waiting",
    });
    expect(book.dropped()).toMatchObject({ "waiting requests": 1 });
    // A late answer to it changes nothing.
    feed({
      type: "response",
      connection: 1,
      id: "a",
      method: "statement_submit",
      result: { status: "new" },
    });
    expect(book.submitList()[0].outcome.kind).toBe("unanswered");
  });

  test("keeps channels, conversations, requests and roles within bounds", () => {
    const { book, submit } = ledger({
      channels: 2,
      conversations: 2,
      requests: 2,
    });
    for (let index = 1; index <= 4; index += 1)
      submit(statement(index, 4, new Uint8Array(32).fill(10 + index)), {
        status: "new",
      });
    expect(book.channelList()).toHaveLength(2);
    for (let index = 0; index < 5; index += 1)
      book.invitations({
        type: "request-sent",
        product: "chat.paseo",
        handle: `0x${String(index).repeat(32)}`,
      });
    expect(book.requestList()).toHaveLength(2);
    const account = "0x" + "ab".repeat(32);
    book.diagnostics(
      { name: "chat.paseo", run: 1 },
      {
        conversations: Array.from({ length: 20 }, (_, index) => ({
          id: `c${index}`,
          protocol: "p",
          account,
          listen: [TOPIC],
        })),
      },
    );
    expect(book.conversationList()).toHaveLength(2);
    expect(book.accountRecord(account)!.roles).toHaveLength(8);
    for (let index = 0; index < 10; index += 1)
      book.diagnostics(
        { name: `product-${index}.paseo`, run: 1 },
        { conversations: [{ id: "c", protocol: "p", listen: [TOPIC] }] },
      );
    expect(book.topicList()).toHaveLength(1);
    expect(book.dropped()).toMatchObject({
      channels: 2,
      requests: 3,
      conversations: 28,
    });
  });

  test("keeps unfamiliar node answers, pages, filters and errors as shapes: no statement bytes", () => {
    const { book, feed, submit, page } = ledger();
    const secret = "0x" + "5e".repeat(120);
    submit(statement(1), {
      status: "accepted-somehow",
      blob: secret,
      reason: secret,
      note: "plain",
    });
    feed(
      {
        type: "request",
        connection: 1,
        id: "e",
        method: "statement_submit",
        params: [statement(2)],
      },
      {
        type: "response",
        connection: 1,
        id: "e",
        method: "statement_submit",
        error: {
          code: -32000,
          message: `bad statement ${secret}`,
          data: { statement: secret },
        },
      },
      {
        type: "request",
        connection: 1,
        id: "f",
        method: "statement_subscribeStatement",
        params: [{ topics: [secret] }],
      },
      {
        type: "response",
        connection: 1,
        id: "f",
        method: "statement_subscribeStatement",
        error: { message: secret, data: secret },
      },
    );
    page(99, [secret]);
    feed({
      type: "notification",
      connection: 1,
      subscription: "99",
      method: "statement_statement",
      result: { event: "other", payload: [secret] },
    });
    const everything = JSON.stringify({
      submits: book.submitList(),
      subscriptions: book.subscriptionList(),
      events: book.eventList(),
    });
    expect(everything).not.toContain("5e5e5e5e5e5e5e5e");
    const [answered, refused] = book.submitList();
    expect(answered.outcome).toMatchObject({
      answer: {
        status: "unrecognised",
        reason: "<string, 242 chars>",
        shape:
          '{"status":"accepted-somehow","blob":"<string, 242 chars>","reason":"<string, 242 chars>","note":"plain"}',
      },
    });
    expect(refused.outcome).toMatchObject({
      error: {
        code: -32000,
        message: "bad statement 0x…(120 bytes)",
        data: '{"statement":"<string, 242 chars>"}',
      },
    });
  });

  test("cost counts broadcast submissions against messages sent in one window, from the run's first report", async () => {
    const { book, submit } = ledger();
    const signer = SIGNER_HEX;
    const report = (run: number, sent: number) =>
      book.diagnostics(
        { name: "chat.paseo", run },
        {
          conversations: [
            {
              id: "c1",
              protocol: "p",
              account: signer,
              logical: { sent },
            },
          ],
        },
      );
    await claimFor(book, "chat.paseo", signer);
    // Before the window: these do not count.
    submit(statement(1, 10, new Uint8Array(32).fill(21)), { status: "new" });
    report(1, 0);
    const [first] = book.conversationList();
    expect(book.cost(first)).toEqual({
      known: false,
      why: "no message sent since the window started",
    });
    submit(statement(2, 10, new Uint8Array(32).fill(22)), { status: "new" });
    submit(statement(3, 10, new Uint8Array(32).fill(23)), {
      status: "rejected",
      reason: "accountFull",
    });
    submit(statement(4, 30, new Uint8Array(32).fill(24)), { status: "known" });
    report(1, 2);
    expect(book.cost(book.conversationList()[0])).toMatchObject({
      known: true,
      attempts: 3,
      wentOut: 2,
      wentOutDataBytes: 40,
      sent: 2,
    });
    // A new run starts a new window.
    report(2, 5);
    expect(book.cost(book.conversationList()[0])).toEqual({
      known: false,
      why: "no message sent since the window started",
    });
  });

  test("cost is not known once the ledger dropped the account's record since the window started", async () => {
    const { book, submit } = ledger({ accounts: 1 });
    await claimFor(book, "chat.paseo", SIGNER_HEX);
    submit(statement(1), { status: "new" });
    book.diagnostics(
      { name: "chat.paseo", run: 1 },
      {
        conversations: [
          {
            id: "c1",
            protocol: "p",
            account: SIGNER_HEX,
            logical: { sent: 0 },
          },
        ],
      },
    );
    // Another account takes the only place, then this one comes back.
    book.grants(book.epoch(), "0xb", [
      { account: "0x" + "cd".repeat(32), value: null },
    ]);
    submit(statement(2), { status: "new" });
    book.diagnostics(
      { name: "chat.paseo", run: 1 },
      {
        conversations: [
          {
            id: "c1",
            protocol: "p",
            account: SIGNER_HEX,
            logical: { sent: 1 },
          },
        ],
      },
    );
    expect(book.cost(book.conversationList()[0])).toEqual({
      known: false,
      why: "the ledger dropped its account's record since the window started",
    });
  });

  // A product names any account it likes; the host's facts about an account
  // belong to a product only when the host or the core tied them.
  test("an account a product reports is joined with host facts only when tied to that product", async () => {
    const { book, submit } = ledger();
    submit(statement(1), { status: "new" });
    const report = (product: string) =>
      book.diagnostics(
        { name: product, run: 1 },
        {
          conversations: [
            {
              id: "c1",
              protocol: "p",
              account: SIGNER_HEX,
              logical: { sent: 0 },
            },
          ],
        },
      );
    await claimFor(book, "owner.paseo", SIGNER_HEX);
    report("borrower.paseo");
    report("owner.paseo");
    expect(book.owns("owner.paseo", SIGNER_HEX)).toBe(true);
    expect(book.owns("borrower.paseo", SIGNER_HEX)).toBe(false);
    submit(statement(2), { status: "new" });
    const sent = (product: string) =>
      book.diagnostics(
        { name: product, run: 1 },
        {
          conversations: [
            {
              id: "c1",
              protocol: "p",
              account: SIGNER_HEX,
              logical: { sent: 1 },
            },
          ],
        },
      );
    sent("borrower.paseo");
    sent("owner.paseo");
    const card = (product: string) =>
      book.conversationList().find((record) => record.product === product)!;
    expect(book.cost(card("borrower.paseo"))).toEqual({
      known: false,
      why: "the host holds no claim or allowance tying its account to borrower.paseo",
    });
    expect(book.cost(card("owner.paseo"))).toMatchObject({
      known: true,
      wentOut: 1,
      sent: 1,
    });
  });

  test("bounds what a node's own text and ids can hold: request problems, refusal reasons, long subscription ids", () => {
    const { book, feed } = ledger();
    const secret = "0x" + "9c".repeat(200);
    book.invitations({
      type: "request-paused",
      product: "chat.paseo",
      handle: "0x" + "11".repeat(16),
      problem: `refused ${secret} ${"x".repeat(500)}`,
    });
    book.invitations({
      type: "chat-account",
      product: "chat.paseo",
      handle: "0x" + "11".repeat(16),
      index: "0x" + "01".repeat(32),
      outcome: "refused",
      reason: secret,
    });
    book.invitations({
      type: "delivery-problem",
      post: "0x" + "11".repeat(16),
      problem: secret,
    });
    const longId = "s".repeat(5_000);
    feed({
      type: "request",
      connection: 1,
      id: "s",
      method: "statement_subscribeStatement",
      params: [{ matchAny: [TOPIC] }],
    });
    feed({
      type: "response",
      connection: 1,
      id: "s",
      method: "statement_subscribeStatement",
      result: longId,
    });
    feed({
      type: "notification",
      connection: 1,
      subscription: subscriptionKey(JSON.stringify(longId)),
      method: "statement_statement",
      result: { event: "newStatements", data: { statements: [statement(2)] } },
    });
    const kept = JSON.stringify({
      requests: book.requestList(),
      subscriptions: book.subscriptionList(),
      events: book.eventList(),
    });
    expect(kept).not.toContain("9c9c9c9c9c9c9c9c");
    expect(kept).not.toContain("s".repeat(100));
    expect(book.requestList()[0].problem!.length).toBeLessThanOrEqual(240);
    // The long id still matches its pages.
    expect(book.subscriptionList()).toHaveLength(1);
    expect(book.subscriptionList()[0]).toMatchObject({
      statements: 1,
      pages: 1,
    });
  });

  test("drops ended subscriptions before open ones, and a page of a dropped open one says so", () => {
    const { book, feed, subscribe, page } = ledger({ subscriptions: 2 });
    subscribe(1);
    subscribe(2);
    feed({
      type: "request",
      connection: 1,
      id: "u",
      method: "statement_unsubscribeStatement",
      params: [1],
    });
    subscribe(3);
    // The ended one went first: both open ones stay.
    expect(book.subscriptionList().map((record) => record.remote)).toEqual([
      "2",
      "3",
    ]);
    subscribe(4);
    expect(book.dropped()).toMatchObject({
      subscriptions: 1,
      "open subscriptions": 1,
    });
    page(2, [statement(1)]);
    const orphan = book.subscriptionList().at(-1)!;
    expect(orphan.filter).toEqual({
      kind: "unrecognised",
      shape:
        "dropped from the list while open: its filter and earlier counts are gone",
    });
  });

  test("a channel whose latest observed statement left the list is counted as dropped, not as nothing", () => {
    const { book, submit } = ledger({ statements: 1 });
    submit(statement(1, 8), { status: "new" });
    submit(statement(2, 8, null), { status: "new" });
    expect(book.latestObserved(SIGNER_HEX)).toMatchObject({
      statements: 1,
      dropped: 1,
    });
  });
});

describe("whose an account is, as this tab knows it", () => {
  const PRODUCT = "chat.paseo";
  const OWN = "0x" + "a1".repeat(32);
  const NOTIFY = "0x" + "d4".repeat(32);
  const ALLOWED = "0x" + "c3".repeat(32);
  const PEER = "0x" + "b2".repeat(32);
  const kind = (book: StatementLedger, account: string) =>
    ownership(book.accountRecord(account)).kind;

  test("this wallet only where the core or this host derived the account in this session", async () => {
    const { book } = ledger();
    await claimFor(book, PRODUCT, OWN);
    book.invitations({
      type: "delivery-account",
      post: "0x" + "44".repeat(16),
      account: NOTIFY,
      period: 1,
    });
    book.invitations({
      type: "chat-account",
      product: PRODUCT,
      handle: "0x" + "44".repeat(16),
      index: "0x" + "09".repeat(32),
      outcome: "allowed",
      account: ALLOWED,
      until: 1,
    });
    expect(
      [OWN, NOTIFY, ALLOWED].map((account) => kind(book, account)),
    ).toEqual(["this-wallet", "this-wallet", "this-wallet"]);
    expect(ownership(book.accountRecord(OWN))).toEqual({
      kind: "this-wallet",
      evidence: [
        {
          kind: "product-account",
          product: PRODUCT,
          index: "0x" + "09".repeat(32),
        },
      ],
    });
  });

  test("a notification claim alone proves nothing, granted or failed; the delivery's account for its post does", async () => {
    const { book } = ledger();
    const granted = "0x" + "e1".repeat(32);
    const failed = "0x" + "e2".repeat(32);
    await book.claim(
      { kind: "notification", account: granted, submit: true },
      async () => ({ period: 1, outcome: "registered", seq: 3 }),
    );
    await book
      .claim(
        { kind: "notification", account: failed, submit: true },
        async () => {
          throw new Error("no free slot");
        },
      )
      .catch(() => {});
    expect([granted, failed].map((account) => kind(book, account))).toEqual([
      "unknown",
      "unknown",
    ]);
    book.invitations({
      type: "delivery-account",
      post: "0x" + "44".repeat(16),
      account: granted,
      period: 1,
    });
    expect(kind(book, granted)).toBe("this-wallet");
  });

  test("a product account claim that throws records no account", async () => {
    const { book } = ledger();
    await book
      .claim(
        {
          kind: "product",
          product: PRODUCT,
          index: "0x" + "09".repeat(32),
          submit: true,
        },
        async () => {
          throw new Error("refused");
        },
      )
      .catch(() => {});
    expect(book.accountList()).toEqual([]);
  });

  test("an account dropped to keep the limits, and seen again, is unknown", async () => {
    const { book, submit } = ledger({ accounts: 1 });
    await claimFor(book, PRODUCT, OWN);
    expect(kind(book, OWN)).toBe("this-wallet");
    submit(statement(60), { status: "new" });
    expect(book.accountRecord(OWN)).toBeUndefined();
    const own = new Uint8Array(32).fill(0xa1);
    const again = bytesToHex(
      encodeSigned(
        {
          expiry: BigInt(NOW_SECONDS + 70) << 32n,
          channel: CHANNEL,
          topics: [new Uint8Array(32).fill(1)],
          data: new Uint8Array(4),
        },
        own,
        () => new Uint8Array(64),
      ),
    );
    submit(again, { status: "new" });
    expect(kind(book, OWN)).toBe("unknown");
  });

  test("Clear forgets whose an account was", async () => {
    const { book } = ledger();
    await claimFor(book, PRODUCT, OWN);
    book.clear();
    expect(kind(book, OWN)).toBe("unknown");
  });

  test("a product's report names an account but never makes it this wallet's, and cannot push out what the host derived", async () => {
    const { book } = ledger();
    await claimFor(book, PRODUCT, OWN);
    const report = (
      name: string,
      id: string,
      account: string,
      peers: string[],
    ) =>
      book.diagnostics(
        { name, run: 1 },
        { conversations: [{ id, protocol: "p/1", account, peers }] },
      );
    report("other.paseo", "x", PEER, []);
    expect(kind(book, PEER)).toBe("reported");
    // Reports naming the wallet's own account, more than the roles it keeps.
    for (let index = 0; index < 12; index += 1)
      report(`spoof-${index}.paseo`, `c${index}`, OWN, []);
    expect(kind(book, OWN)).toBe("this-wallet");
    expect(book.accountRecord(OWN)!.roles).toHaveLength(8);
    // A stopped worker's report changes nothing.
    book.diagnostics({ name: "other.paseo", run: 1 }, null);
    expect(kind(book, PEER)).toBe("reported");
  });

  test("a signer only seen, or a request addressed to this wallet, says nothing of whose the signer is", () => {
    const { book, subscribe, page } = ledger();
    subscribe(5);
    // A statement on a topic this host reads, signed by an account it never derived:
    // the same as one this wallet signed in another browser.
    page(5, [statement(60)], 0);
    book.invitations({
      type: "request-received",
      product: PRODUCT,
      handle: "0x" + "55".repeat(16),
    });
    expect(kind(book, SIGNER_HEX)).toBe("unknown");
    expect(ownership(undefined)).toEqual({ kind: "unknown" });
  });

  test("a new session forgets whose an account was: its record goes, and seen again it is unknown", async () => {
    const { book, submit } = ledger();
    await claimFor(book, PRODUCT, SIGNER_HEX);
    submit(statement(60), { status: "new" });
    expect(kind(book, SIGNER_HEX)).toBe("this-wallet");
    book.startSession("wallet-b", "wallet B");
    expect(book.accountRecord(SIGNER_HEX)).toBeUndefined();
    submit(statement(61), { status: "new" });
    expect(kind(book, SIGNER_HEX)).toBe("unknown");
  });
});

describe("the statement ledger over the light client", () => {
  /** The ledger fed by the real observer on a connection answered as the light client answers. */
  async function overLightClient() {
    const client = new LightClientStatements();
    client.nowSeconds = NOW_SECONDS;
    const book = new StatementLedger(
      LIGHT_CLIENT,
      {},
      () => NOW_SECONDS * 1000,
    );
    book.startSession("s", "session");
    const connection = observedConnection(
      client.connect(),
      "core",
      book.traffic,
    );
    const answers: string[] = [];
    void (async () => {
      for await (const text of connection.responses()) answers.push(text);
    })();
    let id = 0;
    const call = async (method: string, params: unknown[]) => {
      connection.send(
        JSON.stringify({ jsonrpc: "2.0", id: ++id, method, params }),
      );
      await new Promise((resolve) => setTimeout(resolve, 0));
    };
    return { client, book, call, answers };
  }

  test("the empty first page ends the initial batch; what peers send later is 'later', not proven new, and seen once", async () => {
    const { client, book, call } = await overLightClient();
    await call("statement_subscribeStatement", [{ matchAny: [TOPIC] }]);
    expect(book.subscriptionList()[0]).toMatchObject({
      state: "open",
      phase: "later",
      lastRemaining: 0,
      pages: 1,
      statements: 0,
    });
    // A statement peers held for a day arrives now, after the empty batch.
    const old = statement(-1_000 + 90_000, 8, null);
    client.peersSend([old]);
    client.peersSend([old]);
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(book.subscriptionList()[0]).toMatchObject({
      pages: 2,
      statements: 1,
      byDelivery: { initial: 0, later: 1, unclassified: 0 },
    });
    expect(book.statementList()[0].via).toEqual(["later"]);
    const texts = book.eventList().map((event) => event.text);
    expect(texts).toContain(
      "Subscription S1: an empty page (the initial batch, empty: the light client holds no statements)",
    );
    expect(texts).toContain(
      "Subscription S1: 1 statement (after the initial batch: what peers sent, new or long stored)",
    );
    expect(
      book.eventList().every((event) => event.source !== ("node" as never)),
    ).toBe(true);
  });

  test("an answer new is a broadcast that went out, not a store's acceptance, and says peers' refusals do not reach this tab", async () => {
    const { book, call } = await overLightClient();
    await call("statement_submit", [statement(100, 12)]);
    const [record] = book.submitList();
    expect(record.outcome).toMatchObject({
      kind: "answered",
      answer: { status: "new" },
    });
    expect(book.statementList()[0].via).toEqual(["broadcast"]);
    expect(book.accountRecord(SIGNER_HEX)).toMatchObject({
      wentOut: 1,
      wentOutDataBytes: 12,
    });
    const event = book
      .eventList()
      .find(
        (entry) => entry.refs.submit === 1 && entry.source === "light-client",
      )!;
    expect(event.text).toBe(
      "Submit #1: broadcast (new): at least one peer took it; not shown stored",
    );
    expect(event.details.map(([label]) => label)).toContain("Note");
  });

  test("the light client's own checks and broadcast failures, each as it gives them", async () => {
    const { client, book, call } = await overLightClient();
    await call("statement_submit", [statement(-10)]);
    await call("statement_submit", ["0x0408"]);
    client.peers = { connected: 0, accepting: 0 };
    await call("statement_submit", [statement(100)]);
    client.peers = { connected: 5, accepting: 0 };
    await call("statement_submit", [statement(101)]);
    const outcomes = book.submitList().map((record) => record.outcome);
    expect(outcomes[0]).toMatchObject({
      kind: "answered",
      answer: { status: "invalid", reason: "alreadyExpired" },
    });
    expect(outcomes.slice(1)).toEqual([
      expect.objectContaining({
        kind: "rpc-error",
        local: false,
        error: {
          code: 7001,
          message: "Statement store error: Error decoding statement",
        },
        meaning: "the light client could not decode it",
      }),
      expect.objectContaining({
        kind: "rpc-error",
        meaning: "not broadcast: the light client had no connected peer",
      }),
      expect.objectContaining({
        kind: "rpc-error",
        meaning: "not broadcast: none of the 5 connected peers took it",
      }),
    ]);
    // None went out: nothing is observed as broadcast.
    expect(book.statementList()).toEqual([]);
    expect(book.counts()).toMatchObject({
      rpcErrors: 3,
      answers: { invalid: 1 },
    });
  });
});

describe("the statement ledger over People's RPC node", () => {
  const NODE = peopleTransport("rpc");

  // A node keeps a store: its "new" means it took the statement in, not that
  // a peer took a broadcast, and its facts carry the node's source tag.
  test("reads a node's answers as the node's, not as a light client broadcast", () => {
    const { book, submit } = ledger({}, NODE);
    submit(statement(100, 20), { status: "new" });
    const event = book
      .eventList()
      .find((entry) => entry.kind === "submit" && entry.source !== "core")!;
    expect([event.source, event.text]).toEqual([
      "node",
      "Submit #1: accepted (new): in the node's store; other nodes may still refuse it",
    ]);
    expect(event.details).toContainEqual(["Note", NODE.wentOutNote]);
  });

  // The light client's error messages mean nothing coming from a node, so a
  // node's RPC error keeps its own message.
  test("does not read a node's RPC error as a light client refusal", () => {
    const { book, feed } = ledger({}, NODE);
    feed(
      {
        type: "request",
        connection: 1,
        id: "1",
        method: "statement_submit",
        params: [statement(100)],
      },
      {
        type: "response",
        connection: 1,
        id: "1",
        method: "statement_submit",
        error: { code: 7001, message: "No connected peers to broadcast" },
      },
    );
    expect(book.submitList()[0].outcome).toEqual(
      expect.not.objectContaining({ meaning: expect.anything() }),
    );
  });

  // A node replays what it holds, so a first batch that ends with
  // statements in it is complete, not empty.
  test("says where a node's replay ends, and when it held nothing", () => {
    const { book, subscribe, page } = ledger({}, NODE);
    subscribe(9);
    page(9, [statement(100)], 0);
    subscribe(10);
    page(10, [], 0);
    expect(
      book
        .eventList()
        .filter((event) => event.kind === "page")
        .map((event) => [event.source, event.text]),
    ).toEqual([
      ["node", "Subscription S1: 1 statement (the initial batch, complete)"],
      [
        "node",
        "Subscription S2: an empty page (the initial batch, empty: the node holds none on this filter)",
      ],
    ]);
  });
});
