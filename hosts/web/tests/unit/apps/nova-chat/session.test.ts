import { describe, expect, test } from "bun:test";
import type {
  NovaPeer,
  NovaResult,
  OpenedFrame,
  WireSignedStatement,
  WireStatement,
} from "../../../../bridges/nova-chat/contract.js";
import {
  NovaSession,
  type NovaHostPort,
} from "../../../../apps/nova-chat/session.js";
import {
  DIAGNOSTICS_PROTOCOL,
  PRODUCT_DIAGNOSTICS,
  readDiagnosticsMessage,
} from "../../../../bridges/diagnostics.js";
import { MemoryStorage } from "../../../support/test-support.js";

/**
 * Nova Chat's worker session (`apps/nova-chat/session.ts`) against a
 * scripted host: what it keeps, what it acknowledges, and how it posts.
 */

const HANDLE = `0x${"ab".repeat(32)}`;
const OUTGOING = `0x${"01".repeat(32)}`;
const INCOMING = `0x${"02".repeat(32)}`;
const REQUEST_CHANNEL = `0x${"0a".repeat(32)}`;
const RESPONSE_CHANNEL = `0x${"0b".repeat(32)}`;
const OWN_ACCOUNT = `0x${"0c".repeat(32)}`;

interface Script {
  /** What `list` answers. */
  listed: NovaPeer[];
  /** What `open` answers for the next statement. */
  opened: OpenedFrame[];
  calls: { call: string; [key: string]: unknown }[];
  posted: { index: string; statement: WireStatement }[];
  allowUntil: number;
  /** Hold every post until released. */
  postGate: Promise<void> | null;
  postProblem: string | null;
  scheduled: { task: () => void; delayMs: number; cancelled: boolean }[];
  /** What `conversationAccount` answers next; then it allows. */
  allowAnswers: NovaResult[];
  /** Push a state change, as the host does. */
  push: (peer: NovaPeer) => void;
  deliver: (statements: WireSignedStatement[]) => void;
  now: number;
}

function session(storage = new MemoryStorage()): {
  session: NovaSession;
  script: Script;
} {
  const script: Script = {
    listed: [{ handle: HANDLE, state: "established" }],
    opened: [],
    calls: [],
    posted: [],
    allowUntil: 10_000,
    postGate: null,
    postProblem: null,
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
          return { ok: true, value: script.listed };
        case "routes":
          return {
            ok: true,
            value: { outgoing: OUTGOING, incoming: [INCOMING] },
          };
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
              channel:
                (call.frame as { kind: string }).kind === "Request"
                  ? REQUEST_CHANNEL
                  : RESPONSE_CHANNEL,
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
        if (script.postProblem !== null) throw new Error(script.postProblem);
        script.posted.push({ index, statement });
      },
    },
    storage,
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
    account: async () => OWN_ACCOUNT.toUpperCase().replace("0X", "0x"),
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
  test("provisional renewal preserves logical message ids, uses a higher outer frame and rechecks the same conversation account", async () => {
    const { session: chat, script } = session();
    await chat.run();
    await tick();
    script.allowAnswers.push({
      ok: true,
      value: { account: OWN_ACCOUNT, until: 1_100, provisional: true },
    });
    await chat.send(HANDLE, "provisional first");
    await tick();
    const first = script.posted[0]!;
    script.now = 1_101;
    script.scheduled.find((entry) => !entry.cancelled)!.task();
    await tick();
    const second = script.posted[1]!;
    expect(second.index).toBe(first.index);
    expect(JSON.parse(second.statement.data).messages).toEqual(
      JSON.parse(first.statement.data).messages,
    );
    expect(
      BigInt(second.statement.expiry) > BigInt(first.statement.expiry),
    ).toBe(true);
    expect(
      script.calls.filter((call) => call.call === "conversationAccount"),
    ).toHaveLength(2);
    expect(chat.messages(HANDLE)).toHaveLength(1);
    chat.stop();
  });

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

describe("Nova Chat's diagnostics", () => {
  const PEER = `0x${"ee".repeat(32)}`;
  const fromPeer = { ...statement, signer: PEER };
  const peerText = (id: string, text: string): OpenedFrame => ({
    kind: "Request",
    requestId: `r-${id}`,
    messages: [{ id, timestamp: "5", content: { kind: "Text", text } }],
  });
  /** The report as the host keeps it: what survives its reader. */
  const hostRead = (chat: NovaSession) =>
    readDiagnosticsMessage({
      type: PRODUCT_DIAGNOSTICS,
      protocol: DIAGNOSTICS_PROTOCOL,
      report: { conversations: chat.diagnostics() },
    })!.conversations;
  const fact = (chat: NovaSession, label: string) =>
    chat.diagnostics()[0]!.facts!.find((entry) => entry.label === label)?.value;

  test("a successful send, failed retransmissions and successful retry count the logical message once this run", async () => {
    const storage = new MemoryStorage();
    const { session: chat, script } = session(storage);
    const runScheduled = async () => {
      const next = script.scheduled.findLast((entry) => !entry.cancelled)!;
      next.cancelled = true;
      next.task();
      await tick();
    };
    await chat.run();
    await tick();
    await chat.send(HANDLE, "private text");
    await tick();
    expect(chat.diagnostics()[0]!.logical).toEqual({
      sent: 1,
      received: 0,
      duplicates: 0,
      failed: 0,
      posts: 1,
    });
    script.now = script.allowUntil + 1_000;
    script.allowUntil = script.now + 10_000;
    script.postProblem = "synthetic refusal";
    await runScheduled();
    await runScheduled();
    expect(chat.diagnostics()[0]!.logical).toEqual({
      sent: 1,
      received: 0,
      duplicates: 0,
      failed: 1,
      posts: 1,
    });
    expect(fact(chat, "Delivery (this run)")).toContain(
      "2 failed tries in a row",
    );
    script.postProblem = null;
    await runScheduled();
    expect(chat.diagnostics()[0]!.logical).toEqual({
      sent: 1,
      received: 0,
      duplicates: 0,
      failed: 1,
      posts: 2,
    });
    expect(fact(chat, "Stored history")).toBe(
      "1 yours · 0 theirs · 1 awaiting ack",
    );
    const request = JSON.parse(script.posted.at(-1)!.statement.data) as {
      requestId: string;
    };
    script.opened.push({
      kind: "Response",
      requestId: request.requestId,
      code: 0,
    });
    script.deliver([fromPeer]);
    await tick();
    expect(fact(chat, "Acknowledged (this run)")).toBe("1 of yours");
    expect(chat.messages(HANDLE)).toHaveLength(1);
    expect(hostRead(chat)[0]!.logical!.sent).toBe(1);
    expect(JSON.stringify(chat.diagnostics())).not.toContain("private text");
    expect(storage.getItem("record")).not.toContain("sentIds");
    chat.stop();
    const restarted = session(storage);
    await restarted.session.run();
    await tick();
    expect(restarted.session.diagnostics()[0]!.logical).toEqual({
      sent: 0,
      received: 0,
      duplicates: 0,
      failed: 0,
      posts: 0,
    });
    expect(fact(restarted.session, "Stored history")).toBe(
      "1 yours · 0 theirs · 0 awaiting ack",
    );
    restarted.session.stop();
  });

  test("an established conversation reports its state, accounts, topics, channel and this run's counts, all kept by the host's reader", async () => {
    const { session: chat, script } = session();
    await chat.run();
    await tick();
    await chat.send(HANDLE, "secret words");
    await tick();
    const sent = JSON.parse(script.posted[0]!.statement.data) as {
      requestId: string;
    };
    script.opened.push(
      { kind: "Response", requestId: sent.requestId, code: 0 },
      peerText("m1", "their words"),
    );
    script.deliver([fromPeer, fromPeer]);
    await tick();
    const reported = chat.diagnostics();
    expect(hostRead(chat)).toEqual(reported);
    expect(reported).toEqual([
      {
        id: HANDLE,
        protocol: "nova-apps/chat-session",
        state: "established",
        account: OWN_ACCOUNT,
        peers: [PEER],
        listen: [INCOMING],
        post: { topics: [OUTGOING], channel: REQUEST_CHANNEL },
        logical: { sent: 1, received: 1, duplicates: 0, failed: 0, posts: 2 },
        facts: [
          {
            label: "Stored history",
            value: "1 yours · 1 theirs · 0 awaiting ack",
          },
          { label: "Acknowledged (this run)", value: "1 of yours" },
          {
            label: "Delivery (this run)",
            value: "following · next post scheduled",
          },
          { label: "Allowance (stored)", value: "until 1970-01-01 00:00 UTC" },
        ],
      },
    ]);
    // Nothing anyone wrote, encoded or not.
    const text = JSON.stringify(reported);
    for (const words of ["secret words", "their words"]) {
      expect(text).not.toContain(words);
      expect(text).not.toContain(Buffer.from(words).toString("hex"));
    }
  });

  test("what the run has not learned is left out, not reported as empty", async () => {
    const { session: chat, script } = session();
    script.listed = [{ handle: HANDLE, state: "outgoing" }];
    await chat.run();
    await tick();
    expect(chat.diagnostics()).toEqual([
      expect.objectContaining({ id: HANDLE, state: "outgoing" }),
    ]);
    const [entry] = chat.diagnostics();
    for (const key of ["account", "peers", "listen", "post", "request"])
      expect(entry).not.toHaveProperty(key);
    expect(fact(chat, "Allowance (stored)")).toBe("not asked yet");
    expect(fact(chat, "Delivery (this run)")).toBe("not following");
  });

  test("topics are reported only while the conversation is followed", async () => {
    const { session: chat, script } = session();
    await chat.run();
    await tick();
    expect(chat.diagnostics()[0]).toHaveProperty("listen", [INCOMING]);
    script.push({ handle: HANDLE, state: "blocked" });
    const [entry] = chat.diagnostics();
    expect(entry).toMatchObject({ state: "blocked", account: OWN_ACCOUNT });
    expect(entry).not.toHaveProperty("listen");
    expect(entry).not.toHaveProperty("post");
  });

  test("a message read again counts as a duplicate, and a new run counts from zero over the stored history", async () => {
    const storage = new MemoryStorage();
    const first = session(storage);
    await first.session.run();
    await tick();
    first.script.opened.push(peerText("m1", "hi"), peerText("m1", "hi"));
    first.script.deliver([fromPeer, fromPeer]);
    await tick();
    expect(first.session.diagnostics()[0]!.logical).toMatchObject({
      received: 1,
      duplicates: 1,
    });
    first.session.stop();

    const second = session(storage);
    await second.session.run();
    await tick();
    const [entry] = second.session.diagnostics();
    expect(entry!.logical).toEqual({
      sent: 0,
      received: 0,
      duplicates: 0,
      failed: 0,
      posts: 0,
    });
    expect(entry!.facts![0]).toEqual({
      label: "Stored history",
      value: "0 yours · 1 theirs · 0 awaiting ack",
    });
    expect(entry).not.toHaveProperty("peers");
  });

  test("a failed send counts once per message, says why and how delivery stands, and listeners hear it until they leave", async () => {
    const { session: chat, script } = session();
    let heard = 0;
    const leave = chat.onDiagnostics(() => heard++);
    await chat.run();
    await tick();
    for (let i = 0; i < 2; i++)
      script.allowAnswers.push({ ok: false, reason: "no free slot" });
    await chat.send(HANDLE, "hello");
    await tick();
    script.scheduled.find((entry) => !entry.cancelled)!.task();
    await tick();
    expect(chat.diagnostics()[0]!.logical).toMatchObject({
      sent: 0,
      failed: 1,
      posts: 0,
    });
    expect(fact(chat, "Last problem (this run)")).toBe("send: no free slot");
    expect(fact(chat, "Delivery (this run)")).toBe(
      "following · 2 failed tries in a row · next post scheduled",
    );
    expect(fact(chat, "Stored history")).toBe(
      "1 yours · 0 theirs · 1 awaiting ack",
    );
    expect(heard).toBeGreaterThan(0);
    leave();
    const before = heard;
    await chat.send(HANDLE, "again");
    await tick();
    expect(heard).toBe(before);
  });
});
