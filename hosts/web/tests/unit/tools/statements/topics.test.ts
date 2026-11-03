import { describe, expect, test } from "bun:test";
import { bytesToHex } from "@parity/truapi/scale";
import { encodeSigned } from "../../../../src/platform/statement.js";
import type { StatementTraffic } from "../../../../src/platform/statement-observer.js";
import {
  inboxTopic,
  periodOf,
} from "../../../../src/invitations/statement-delivery.js";
import { StatementLedger } from "../../../../tools/statements/ledger.js";
import { topicViews } from "../../../../tools/statements/topics.js";

const NOW = 1_800_000_000_000;
const NETWORK = "testnet";
const TODAY = bytesToHex(inboxTopic(NETWORK, periodOf(NOW)));
const YESTERDAY = bytesToHex(inboxTopic(NETWORK, periodOf(NOW) - 1));
const READS = "0x" + "22".repeat(32);
const POSTS = "0x" + "11".repeat(32);
const OTHER = "0x" + "99".repeat(32);
const ACCOUNT = "0x" + "a1".repeat(32);

function statementOn(topics: string[]): string {
  return bytesToHex(
    encodeSigned(
      {
        expiry: BigInt(NOW / 1000 + 3600) << 32n,
        channel: null,
        topics: topics.map(
          (topic) =>
            new Uint8Array(
              (topic.slice(2).match(/../g) ?? []).map((pair) =>
                parseInt(pair, 16),
              ),
            ),
        ),
        data: new Uint8Array(4),
      },
      new Uint8Array(32).fill(7),
      () => new Uint8Array(64),
    ),
  );
}

/** How this tab's own no-submit refusal reads in these tests. */
const REFUSED_HERE = "refused here";

function setup() {
  const book = new StatementLedger(
    {},
    () => NOW,
    (error) => error.message === REFUSED_HERE,
  );
  book.startSession("wallet", "wallet");
  const feed = (...events: StatementTraffic[]) => events.forEach(book.traffic);
  feed({ type: "connected", connection: 1, source: "host" });
  let id = 0;
  const subscribe = (topics: string[], remote: string) => {
    const request = String(++id);
    feed(
      {
        type: "request",
        connection: 1,
        id: request,
        method: "statement_subscribeStatement",
        params: [{ matchAny: topics }],
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
  /** Submit a statement on `topics`; with no answer, it stays waiting. */
  const submit = (
    topics: string[],
    answer?: { result: unknown } | { error: { code: number; message: string } },
  ) => {
    const request = String(++id);
    feed({
      type: "request",
      connection: 1,
      id: request,
      method: "statement_submit",
      params: [statementOn(topics)],
    });
    if (answer !== undefined)
      feed({
        type: "response",
        connection: 1,
        id: request,
        method: "statement_submit",
        ...answer,
      });
  };
  const page = (remote: string, statements: string[]) =>
    feed({
      type: "notification",
      connection: 1,
      subscription: remote,
      method: "statement_statement",
      result: { event: "newStatements", data: { statements } },
    });
  const report = (name: string, conversation: object | null) =>
    book.diagnostics(
      { name, run: 1 },
      conversation === null ? null : { conversations: [conversation as never] },
    );
  const views = (network = NETWORK) =>
    Object.fromEntries(
      topicViews(book, network, NOW).map((view) => [
        view.record.topic,
        {
          name:
            view.name === null
              ? null
              : `${view.name.source}: ${view.name.text}`,
          role: view.role,
          reportedOnly: view.reportedOnly,
          open: view.open.map((entry) => entry.source),
        },
      ]),
    );
  return { book, subscribe, submit, page, report, views };
}

describe("the Topics tab's model", () => {
  test("names the host's day inbox from the network and the day, and an open subscription makes it Listening", () => {
    const { subscribe, views } = setup();
    subscribe([YESTERDAY, TODAY], "inbox");
    expect(views()).toEqual({
      [YESTERDAY]: {
        name: "derived: Shared setup inbox · yesterday UTC",
        role: "Listening",
        reportedOnly: { listening: false, publishing: false },
        open: ["host"],
      },
      [TODAY]: {
        name: "derived: Shared setup inbox · today UTC",
        role: "Listening",
        reportedOnly: { listening: false, publishing: false },
        open: ["host"],
      },
    });
    // On another network the same topics mean nothing known: hex only.
    expect(Object.values(views("othernet")).map((view) => view.name)).toEqual([
      null,
      null,
    ]);
  });

  test("a product's report gives a reported name and role, never an open subscription or a submission", () => {
    const { report, views } = setup();
    report("chat.test", {
      id: "0x" + "c0".repeat(16),
      protocol: "p/1",
      account: ACCOUNT,
      listen: [READS],
      post: { topics: [POSTS] },
    });
    expect(views()).toEqual({
      [READS]: {
        name: "product: chat.test · incoming",
        role: "Listening",
        reportedOnly: { listening: true, publishing: false },
        open: [],
      },
      [POSTS]: {
        name: "product: chat.test · outgoing",
        role: "Publishing",
        reportedOnly: { listening: false, publishing: true },
        open: [],
      },
    });
    // A stopped worker's report no longer holds: the role goes, and the name says so.
    report("chat.test", null);
    expect(views()[READS]).toMatchObject({
      name: "product: chat.test · incoming · worker stopped",
      role: "Other observed",
    });
  });

  test("a submission makes a topic Publishing only once the light client answered it new; one refused, failed or unanswered is only Tried", () => {
    const { submit, report, views } = setup();
    const refusedHere = "0x" + "31".repeat(32);
    const rpcError = "0x" + "32".repeat(32);
    const rejected = "0x" + "33".repeat(32);
    const waiting = "0x" + "34".repeat(32);
    const broadcast = "0x" + "35".repeat(32);
    submit([refusedHere], { error: { code: -32000, message: REFUSED_HERE } });
    submit([rpcError], {
      error: { code: 7001, message: "none of the 3 connected peers accepted" },
    });
    submit([rejected], { result: { status: "invalid", reason: "badProof" } });
    submit([waiting]);
    submit([broadcast], { result: { status: "new" } });
    const roles = views();
    expect(
      [refusedHere, rpcError, rejected, waiting, broadcast].map(
        (topic) => roles[topic]!.role,
      ),
    ).toEqual(["Tried", "Tried", "Tried", "Tried", "Publishing"]);
    // A product saying it posts on a topic nothing went out on is its word only.
    report("chat.test", {
      id: "c",
      protocol: "p/1",
      post: { topics: [refusedHere] },
    });
    expect(views()[refusedHere]).toMatchObject({
      role: "Publishing",
      reportedOnly: { listening: false, publishing: true },
    });
  });

  test("a product naming the host's inbox does not rename it, and a topic only seen on statements is Other observed", () => {
    const { subscribe, page, report, views } = setup();
    subscribe([TODAY], "inbox");
    page("inbox", [statementOn([TODAY, OTHER])]);
    report("other.test", {
      id: "x",
      protocol: "p/1",
      post: { topics: [TODAY] },
    });
    expect(views()[TODAY]).toEqual({
      name: "derived: Shared setup inbox · today UTC",
      role: "Both",
      reportedOnly: { listening: false, publishing: true },
      open: ["host"],
    });
    expect(views()[OTHER]).toEqual({
      name: null,
      role: "Other observed",
      reportedOnly: { listening: false, publishing: false },
      open: [],
    });
  });
});
