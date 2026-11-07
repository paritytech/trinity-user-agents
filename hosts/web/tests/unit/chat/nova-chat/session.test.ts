import { describe, expect, test } from "bun:test";
import type {
  NovaPeer,
  NovaResult,
  OpenedFrame,
  WireSignedStatement,
  WireStatement,
} from "../../../../src/chat/nova-chat/contract.js";
import {
  NovaSession,
  type NovaHostPort,
} from "../../../../src/chat/nova-chat/session.js";
import { MemoryStorage } from "../../../support/test-support.js";

/**
 * Nova Chat's worker session (`src/chat/nova-chat/session.ts`) against a
 * scripted host: what it keeps, what it acknowledges, and how it posts.
 */

const HANDLE = `0x${"ab".repeat(32)}`;

interface Script {
  /** What `open` answers for the next statement. */
  opened: OpenedFrame[];
  calls: { call: string; [key: string]: unknown }[];
  posted: { index: string; statement: WireStatement }[];
  allowUntil: number;
  /** Hold every post until released. */
  postGate: Promise<void> | null;
  scheduled: { task: () => void; delayMs: number; cancelled: boolean }[];
  /** What `conversationAccount` answers next; then it allows. */
  allowAnswers: NovaResult[];
  /** Push a state change, as the host does. */
  push: (peer: NovaPeer) => void;
  deliver: (statements: WireSignedStatement[]) => void;
  now: number;
}

function session(): { session: NovaSession; script: Script } {
  const script: Script = {
    opened: [],
    calls: [],
    posted: [],
    allowUntil: 10_000,
    postGate: null,
    scheduled: [],
    allowAnswers: [],
    push: () => {},
    deliver: () => {},
    now: 1_000,
  };
  let stateListener: (peer: NovaPeer) => void = () => {};
  const host: NovaHostPort = {
    async call(call): Promise<NovaResult> {
      script.calls.push(call);
      switch (call.call) {
        case "list":
          return {
            ok: true,
            value: [{ handle: HANDLE, state: "established" }],
          };
        case "routes":
          return { ok: true, value: { outgoing: "0x01", incoming: ["0x02"] } };
        case "conversationAccount":
          return (
            script.allowAnswers.shift() ?? {
              ok: true,
              value: { account: "0x03", until: script.allowUntil },
            }
          );
        case "seal":
          return {
            ok: true,
            value: {
              expiry: call.expiry,
              channel: null,
              topics: [],
              data: JSON.stringify(call.frame),
            },
          };
        case "open":
          return { ok: true, value: script.opened.shift() };
        default:
          return { ok: false, reason: "unexpected" };
      }
    },
    onState: (listener) => (stateListener = listener),
  };
  script.push = (peer) => stateListener(peer);
  let ids = 0;
  const value = new NovaSession({
    host,
    statements: {
      follow: (_topics, deliver) => {
        script.deliver = deliver;
        return () => {};
      },
      post: async (index, statement) => {
        if (script.postGate !== null) await script.postGate;
        script.posted.push({ index, statement });
      },
    },
    storage: new MemoryStorage(),
    recordKey: "record",
    now: () => script.now,
    newId: () => `id-${++ids}`,
    indexFor: () => `0x${"cd".repeat(32)}`,
    schedule: (task, delayMs) => {
      const entry = { task, delayMs, cancelled: false };
      script.scheduled.push(entry);
      return () => (entry.cancelled = true);
    },
    encodeText: (id, _timestamp, text) =>
      `0x${Buffer.from(`${id}:${text}`).toString("hex")}`,
    report: () => {},
  });
  return { session: value, script };
}

const tick = () => new Promise((resolve) => setTimeout(resolve, 10));
const statement: WireSignedStatement = {
  expiry: "1",
  channel: null,
  topics: [],
  data: "0x",
  signer: `0x${"00".repeat(32)}`,
  signature: `0x${"00".repeat(64)}`,
};
const framesOf = (script: Script) =>
  script.posted.map(
    (each) => JSON.parse(each.statement.data) as { kind: string },
  );
const expiries = (script: Script, kind: string) =>
  script.posted
    .filter(
      (each) =>
        (JSON.parse(each.statement.data) as { kind: string }).kind === kind,
    )
    .map((each) => BigInt(each.statement.expiry));

describe("Nova Chat's worker session", () => {
  test("a frame of kinds it does not show (the peer's push token) is acknowledged and not listed", async () => {
    const { session: chat, script } = session();
    await chat.run();
    await tick();
    script.opened.push({
      kind: "Request",
      requestId: "r1",
      messages: [
        {
          id: "t1",
          timestamp: "5",
          content: { kind: "Unsupported", index: 1 },
        },
      ],
    });
    script.deliver([statement]);
    await tick();
    expect(chat.messages(HANDLE)).toEqual([]);
    expect(framesOf(script)).toEqual([
      { kind: "Response", requestId: "r1", code: 0 },
    ] as never);
  });

  test("sends and acks at once claim the account once and post with rising expiries per channel", async () => {
    const { session: chat, script } = session();
    await chat.run();
    await tick();
    let release!: () => void;
    script.postGate = new Promise((resolve) => (release = resolve));
    await chat.send(HANDLE, "one");
    await chat.send(HANDLE, "two");
    script.opened.push(
      { kind: "Request", requestId: "p1", messages: [] },
      { kind: "Request", requestId: "p2", messages: [] },
    );
    script.deliver([statement, statement]);
    await tick();
    release();
    for (let i = 0; i < 20 && script.posted.length < 4; i++) await tick();
    expect(
      script.calls.filter((each) => each.call === "conversationAccount"),
    ).toHaveLength(1);
    for (const kind of ["Request", "Response"]) {
      const list = expiries(script, kind);
      expect(list.length).toBe(2);
      expect(list[1]! > list[0]!).toBe(true);
    }
  });

  test("messages still unacknowledged when the allowance ends are posted again with a new one", async () => {
    const { session: chat, script } = session();
    await chat.run();
    await tick();
    await chat.send(HANDLE, "hello");
    await tick();
    expect(script.posted).toHaveLength(1);
    const renewal = script.scheduled.at(-1)!;
    expect(renewal.delayMs).toBe(script.allowUntil - script.now + 1_000);
    script.now = script.allowUntil + 1_000;
    script.allowUntil = script.now + 86_400_000;
    renewal.task();
    for (let i = 0; i < 20 && script.posted.length < 2; i++) await tick();
    expect(
      script.calls.filter((each) => each.call === "conversationAccount"),
    ).toHaveLength(2);
    expect(framesOf(script).map((frame) => frame.kind)).toEqual([
      "Request",
      "Request",
    ]);
  });

  test("an acknowledged message is not posted again", async () => {
    const { session: chat, script } = session();
    await chat.run();
    await tick();
    await chat.send(HANDLE, "hello");
    await tick();
    const sent = JSON.parse(script.posted[0]!.statement.data) as {
      requestId: string;
    };
    script.opened.push({
      kind: "Response",
      requestId: sent.requestId,
      code: 0,
    });
    script.deliver([statement]);
    await tick();
    expect(chat.messages(HANDLE)).toMatchObject([
      { text: "hello", state: "delivered" },
    ]);
    const before = script.posted.length;
    script.scheduled.at(-1)!.task();
    await tick();
    expect(script.posted.length).toBe(before);
  });
  test("a claim that keeps failing is tried again after 30 s, doubling to 30 min, with one timer at a time; a success goes back to the allowance's end", async () => {
    const { session: chat, script } = session();
    await chat.run();
    await tick();
    const busy: NovaResult = { ok: false, reason: "no free slot" };
    for (let i = 0; i < 8; i++) script.allowAnswers.push(busy);
    await chat.send(HANDLE, "hello");
    await tick();
    const delays: number[] = [];
    for (let attempt = 0; attempt < 7; attempt++) {
      const live = script.scheduled.filter((entry) => !entry.cancelled);
      expect(live).toHaveLength(1);
      delays.push(live[0]!.delayMs);
      live[0]!.cancelled = true;
      live[0]!.task();
      await tick();
    }
    expect(delays).toEqual([
      30_000, 60_000, 120_000, 240_000, 480_000, 960_000, 1_800_000,
    ]);
    expect(
      script.calls.filter((each) => each.call === "conversationAccount"),
    ).toHaveLength(8);
    // The scripted failures are used up: the next try succeeds.
    const next = script.scheduled.filter((entry) => !entry.cancelled);
    expect(next.map((entry) => entry.delayMs)).toEqual([1_800_000]);
    next[0]!.cancelled = true;
    next[0]!.task();
    for (let i = 0; i < 20 && script.posted.length === 0; i++) await tick();
    expect(script.posted).toHaveLength(1);
    expect(
      script.scheduled
        .filter((entry) => !entry.cancelled)
        .map((entry) => entry.delayMs),
    ).toEqual([script.allowUntil - script.now + 1_000]);
  });

  test("a refusal that will not change stops the tries until the user sends again or the chat's state changes", async () => {
    const { session: chat, script } = session();
    await chat.run();
    await tick();
    script.allowAnswers.push({
      ok: false,
      reason: "You blocked this person.",
      final: true,
    });
    await chat.send(HANDLE, "hello");
    await tick();
    expect(script.scheduled.filter((entry) => !entry.cancelled)).toHaveLength(
      0,
    );
    expect(chat.messages(HANDLE)).toMatchObject([{ state: "failed" }]);
    script.push({ handle: HANDLE, state: "blocked" });
    script.push({ handle: HANDLE, state: "established" });
    for (let i = 0; i < 20 && script.posted.length === 0; i++) await tick();
    expect(script.posted).toHaveLength(1);
  });

  test("stop clears the timer, and a try that fires after stop does nothing", async () => {
    const { session: chat, script } = session();
    await chat.run();
    await tick();
    script.allowAnswers.push({
      ok: false,
      reason: "the People chain could not be read",
    });
    await chat.send(HANDLE, "hello");
    await tick();
    const timer = script.scheduled.find((entry) => !entry.cancelled)!;
    chat.stop();
    expect(timer.cancelled).toBe(true);
    const before = script.calls.length;
    timer.task();
    await tick();
    expect(script.calls.length).toBe(before);
  });
});
