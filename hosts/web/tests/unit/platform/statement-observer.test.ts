import { describe, expect, test } from "bun:test";
import type { JsonRpcConnection } from "@parity/truapi-host";
import {
  OBSERVER_MEMORY,
  observeStatements,
  observedConnection,
  subscriptionKey,
  type StatementTraffic,
} from "../../../src/platform/statement-observer.js";

/** A connection whose answers a test pushes, recording what was sent and whether reading was stopped. */
function fakeConnection() {
  const sent: string[] = [];
  const queue: string[] = [];
  let wake = () => {};
  let ended = false;
  let failure: Error | null = null;
  const state = { returned: false, closed: 0 };
  const connection: JsonRpcConnection = {
    send: (text) => void sent.push(text),
    responses: () => ({
      [Symbol.asyncIterator]() {
        return {
          async next(): Promise<IteratorResult<string>> {
            for (;;) {
              if (queue.length > 0)
                return { value: queue.shift()!, done: false };
              if (failure !== null) throw failure;
              if (ended) return { value: undefined, done: true };
              await new Promise<void>((resolve) => (wake = resolve));
            }
          },
          async return(): Promise<IteratorResult<string>> {
            state.returned = true;
            return { value: undefined, done: true };
          },
        };
      },
    }),
    close: () => void (state.closed += 1),
  };
  return {
    connection,
    sent,
    state,
    push(...texts: string[]) {
      queue.push(...texts);
      wake();
    },
    end() {
      ended = true;
      wake();
    },
    fail(error: Error) {
      failure = error;
      wake();
    },
  };
}

async function read(
  connection: JsonRpcConnection,
  count: number,
): Promise<string[]> {
  const out: string[] = [];
  for await (const text of connection.responses()) {
    out.push(text);
    if (out.length === count) break;
  }
  return out;
}

const FOLLOW = JSON.stringify({
  jsonrpc: "2.0",
  id: 1,
  method: "chainHead_v1_follow",
  params: [true],
});
const FOLLOW_EVENT = JSON.stringify({
  jsonrpc: "2.0",
  method: "chainHead_v1_followEvent",
  params: { subscription: "f1", result: { event: "bestBlockChanged", id: 7 } },
});
const SUBMIT = JSON.stringify({
  jsonrpc: "2.0",
  id: 7,
  method: "statement_submit",
  params: ["0x00"],
});
const SUBMIT_ANSWER = '{"jsonrpc":"2.0","id":7,"result":{"status":"new"}}';
const SUBSCRIBE = JSON.stringify({
  jsonrpc: "2.0",
  id: "s",
  method: "statement_subscribeStatement",
  params: [{ matchAny: ["0x" + "11".repeat(32)] }],
});
const SUBSCRIBED = '{"jsonrpc":"2.0","id":"s","result":42}';
const PAGE = JSON.stringify({
  jsonrpc: "2.0",
  method: "statement_statement",
  params: {
    subscription: 42,
    result: { event: "newStatements", data: { statements: [], remaining: 0 } },
  },
});

describe("the statement traffic observer", () => {
  test("hands every request and answer on unchanged, in order, and reports only statement traffic", async () => {
    const fake = fakeConnection();
    const events: StatementTraffic[] = [];
    const observed = observedConnection(fake.connection, "core", (event) =>
      events.push(event),
    );
    observed.send(FOLLOW);
    observed.send(SUBMIT);
    observed.send(SUBSCRIBE);
    expect(fake.sent).toEqual([FOLLOW, SUBMIT, SUBSCRIBE]);
    const answers = [FOLLOW_EVENT, SUBMIT_ANSWER, SUBSCRIBED, PAGE];
    fake.push(...answers);
    expect(await read(observed, answers.length)).toEqual(answers);
    const connection =
      events[0].type === "connected" ? events[0].connection : -1;
    expect(events).toEqual([
      { type: "connected", connection, source: "core" },
      {
        type: "request",
        connection,
        id: "7",
        method: "statement_submit",
        params: ["0x00"],
      },
      {
        type: "request",
        connection,
        id: '"s"',
        method: "statement_subscribeStatement",
        params: [{ matchAny: ["0x" + "11".repeat(32)] }],
      },
      {
        type: "response",
        connection,
        id: "7",
        method: "statement_submit",
        result: { status: "new" },
      },
      {
        type: "response",
        connection,
        id: '"s"',
        method: "statement_subscribeStatement",
        result: 42,
      },
      {
        type: "notification",
        connection,
        subscription: "42",
        method: "statement_statement",
        result: {
          event: "newStatements",
          data: { statements: [], remaining: 0 },
        },
      },
    ]);
  });

  test("a sink that throws changes nothing the connection does", async () => {
    const fake = fakeConnection();
    const observed = observedConnection(fake.connection, "host", () => {
      throw new Error("the inspector broke");
    });
    observed.send(SUBMIT);
    expect(fake.sent).toEqual([SUBMIT]);
    fake.push(SUBMIT_ANSWER, "not json at all", FOLLOW_EVENT);
    expect(await read(observed, 3)).toEqual([
      SUBMIT_ANSWER,
      "not json at all",
      FOLLOW_EVENT,
    ]);
    observed.close();
    expect(fake.state.closed).toBe(1);
  });

  test("stopping the read stops the inner read, and a connection with no statement traffic reports nothing", async () => {
    const fake = fakeConnection();
    const events: StatementTraffic[] = [];
    const observed = observedConnection(fake.connection, "core", (event) =>
      events.push(event),
    );
    observed.send(FOLLOW);
    fake.push(FOLLOW_EVENT, FOLLOW_EVENT);
    expect(await read(observed, 1)).toEqual([FOLLOW_EVENT]);
    expect(fake.state.returned).toBe(true);
    observed.close();
    expect(events).toEqual([]);
  });

  test("takes only the answer to its own pending request, once", async () => {
    const fake = fakeConnection();
    const events: StatementTraffic[] = [];
    const observed = observedConnection(fake.connection, "core", (event) =>
      events.push(event),
    );
    observed.send(SUBMIT);
    // Answers that name other ids, or no pending one, are passed by.
    fake.push('{"jsonrpc":"2.0","id":8,"result":{"status":"new"}}');
    fake.push(SUBMIT_ANSWER, SUBMIT_ANSWER);
    expect((await read(observed, 3)).length).toBe(3);
    expect(events.filter((event) => event.type === "response")).toHaveLength(1);
  });

  test("reports how a connection ended: closed by its opener once, or its answers ended or failed", async () => {
    const closedBy = async (
      end: (fake: ReturnType<typeof fakeConnection>) => void,
    ) => {
      const fake = fakeConnection();
      const events: StatementTraffic[] = [];
      const observed = observedConnection(fake.connection, "host", (event) =>
        events.push(event),
      );
      observed.send(SUBMIT);
      const reading = (async () => {
        try {
          for await (const _ of observed.responses()) void _;
        } catch {
          // A failed read is reported, then thrown on.
        }
      })();
      end(fake);
      await reading;
      observed.close();
      observed.close();
      return events.filter((event) => event.type === "closed");
    };
    expect(await closedBy((fake) => fake.end())).toEqual([
      expect.objectContaining({ type: "closed", by: "ended" }),
    ]);
    expect(await closedBy((fake) => fake.fail(new Error("gone")))).toEqual([
      expect.objectContaining({ type: "closed", by: "failed", reason: "gone" }),
    ]);
  });

  test("a page that comes before the answer naming its subscription is still reported", async () => {
    const fake = fakeConnection();
    const events: StatementTraffic[] = [];
    const observed = observedConnection(fake.connection, "core", (event) =>
      events.push(event),
    );
    observed.send(SUBSCRIBE);
    fake.push(PAGE, SUBSCRIBED);
    await read(observed, 2);
    expect(events.map((event) => event.type)).toEqual([
      "connected",
      "request",
      "notification",
      "response",
    ]);
  });

  test("remembers a bounded number of unanswered requests, the oldest forgotten first", async () => {
    const fake = fakeConnection();
    const events: StatementTraffic[] = [];
    const observed = observedConnection(fake.connection, "core", (event) =>
      events.push(event),
    );
    for (let id = 0; id < OBSERVER_MEMORY; id += 1)
      observed.send(
        JSON.stringify({ id, method: "statement_submit", params: ["0x00"] }),
      );
    observed.send(SUBSCRIBE);
    fake.push(
      // The oldest request was forgotten: its answer is passed by unread.
      '{"id":0,"result":{"status":"new"}}',
      '{"id":1,"result":{"status":"new"}}',
      SUBSCRIBED,
      PAGE,
    );
    await read(observed, 4);
    const responses = events.filter((event) => event.type === "response");
    expect(
      responses.map((event) => event.type === "response" && event.id),
    ).toEqual(["1", '"s"']);
  });

  test("names a long subscription id the node chose by a bounded hash, and still matches its pages", async () => {
    const fake = fakeConnection();
    const events: StatementTraffic[] = [];
    const observed = observedConnection(fake.connection, "core", (event) =>
      events.push(event),
    );
    const longId = "n".repeat(10_000);
    observed.send(SUBSCRIBE);
    const answer = JSON.stringify({ jsonrpc: "2.0", id: "s", result: longId });
    const longPage = JSON.stringify({
      jsonrpc: "2.0",
      method: "statement_statement",
      params: { subscription: longId, result: {} },
    });
    fake.push(answer, longPage);
    // Handed on unchanged, long id and all.
    expect(await read(observed, 2)).toEqual([answer, longPage]);
    const notification = events.find((event) => event.type === "notification");
    expect(notification).toMatchObject({
      subscription: subscriptionKey(JSON.stringify(longId)),
    });
    expect(
      notification?.type === "notification" && notification.subscription.length,
    ).toBe(65);
  });

  test("only People chain connections are observed", async () => {
    const people = "0x" + "aa".repeat(32);
    const opened: JsonRpcConnection[] = [];
    const chain = {
      async connect() {
        const fake = fakeConnection();
        opened.push(fake.connection);
        return fake.connection;
      },
    };
    const observed = observeStatements(chain, people, "host", () => {});
    const other = await observed.connect(new Uint8Array(32).fill(0xbb));
    const peopleConnection = await observed.connect(
      new Uint8Array(32).fill(0xaa),
    );
    expect(other).toBe(opened[0]);
    expect(peopleConnection).not.toBe(opened[1]);
  });
});
