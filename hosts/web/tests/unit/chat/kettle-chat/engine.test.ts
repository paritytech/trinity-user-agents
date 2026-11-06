import { describe, expect, test } from "bun:test";
import type {
  CallResult,
  HostMessage,
} from "../../../../src/invitations/contract.js";
import type {
  HostRequests,
  RequestCall,
} from "../../../../src/chat/contact-chat/requests.js";
import { KettleChat } from "../../../../src/chat/kettle-chat/engine.js";
import type { IrohRelays } from "../../../../src/chat/kettle-chat/route.js";
import type {
  KettleEvent,
  KettleModule,
  KettleStartOptions,
  KettleNode,
} from "../../../../src/chat/kettle-chat/ports.js";

const PEER = `0x${"be".repeat(32)}`;
const CONTACT = `0x${"c0".repeat(32)}`;
const UNTIL = 2_000_000_000_000;

/** What a test can do with an allowance the host has not answered yet. */
interface Pending {
  resolve(result: CallResult): void;
}

/** A node the scripted Kettle started: what it was told and what the test can tell it. */
interface StartedNode {
  account: string;
  emit(event: KettleEvent): void;
  left: string[];
  stopped: boolean;
  freed: boolean;
}

/** One product's world: its storage, its host's request service and Kettle, all scripted. */
function world(
  storage = new Map<string, string>(),
  options: {
    seed?: number;
    holdRequests?: boolean;
    holdAccounts?: boolean;
    holdRemoves?: boolean;
    /** Hold the first flush of the product's storage. */
    holdFlush?: boolean;
    /**
     * Hold every reply, as a host does whose answer is slow. Released, the
     * host takes the first and refuses the rest: a request is answered once.
     */
    holdReplies?: boolean;
    leaveFails?: boolean;
    iroh?: IrohRelays;
  } = {},
) {
  const calls: RequestCall[] = [];
  const pending: Pending[] = [];
  const heldRequests: (() => void)[] = [];
  const heldAccounts: (() => void)[] = [];
  const heldRemoves: (() => void)[] = [];
  const heldFlushes: (() => void)[] = [];
  const heldReplies: ((answer: CallResult) => void)[] = [];
  let flushes = 0;
  const hold = (held: (() => void)[], on: boolean | undefined) =>
    on === true
      ? new Promise<void>((resolve) => held.push(resolve))
      : Promise.resolve();
  let listener: (message: HostMessage) => void = () => {};
  let allowance: "hold" | CallResult = "hold";
  const sent: { peer: string; text: string }[] = [];
  const started: string[] = [];
  const startedWith: KettleStartOptions[] = [];
  const nodes: StartedNode[] = [];
  const removed: string[] = [];
  const reports: string[] = [];
  let counter = options.seed ?? 0;
  let requests = 0;
  const host: HostRequests = {
    call: (call) => {
      calls.push(call);
      if (call.call === "chatAccount") {
        if (allowance !== "hold") return Promise.resolve(allowance);
        return new Promise((resolve) => pending.push({ resolve }));
      }
      if (call.call === "request") {
        requests += 1;
        const answer = {
          ok: true,
          request: { handle: `0xa${requests}` },
        } as never;
        if (options.holdRequests !== true) return Promise.resolve(answer);
        return new Promise((resolve) =>
          heldRequests.push(() => resolve(answer)),
        );
      }
      if (call.call === "reply" && options.holdReplies === true)
        return new Promise((resolve) => heldReplies.push(resolve));
      return Promise.resolve({ ok: true });
    },
    onMessage: (next) => {
      listener = next;
    },
  };
  const kettle: KettleModule = {
    revision: "test",
    protocol: 6,
    start: async (account, _ports, onEvent, startOptions) => {
      started.push(account);
      startedWith.push(startOptions);
      const record: StartedNode = {
        account,
        emit: onEvent,
        left: [],
        stopped: false,
        freed: false,
      };
      nodes.push(record);
      return {
        account: () => account,
        iroh: () => ({ state: "off" }),
        sendText: async (peer, text) => {
          sent.push({ peer, text });
          return {
            group: `0x${"99".repeat(32)}`,
            number: sent.length,
            timeMs: 0,
          };
        },
        directGroup: async () => null,
        leaveGroup: async (group) => {
          record.left.push(group);
          if (options.leaveFails === true) throw new Error("no such group");
        },
        saveState: async () => {},
        inspect: async () => null,
        stop: async () => {
          record.stopped = true;
        },
        free: () => {
          record.freed = true;
        },
      } satisfies KettleNode;
    },
    removeState: async (account) => {
      await hold(heldRemoves, options.holdRemoves);
      removed.push(account);
    },
  };
  const engine = new KettleChat({
    requests: host,
    kettle,
    account: async (index) => {
      await hold(heldAccounts, options.holdAccounts);
      return index;
    },
    ports: () => ({}) as never,
    storage: {
      getItem: (key) => storage.get(key) ?? null,
      setItem: (key, value) => void storage.set(key, value),
      flush: async () => {
        flushes += 1;
        if (flushes === 1) await hold(heldFlushes, options.holdFlush);
      },
    },
    recordKey: "record",
    random: (length) => {
      counter += 1;
      return new Uint8Array(length).fill(counter);
    },
    now: () => 1_000,
    report: (reason) => reports.push(reason),
    ...(options.iroh === undefined ? {} : { iroh: options.iroh }),
  });
  engine.run();
  return {
    engine,
    storage,
    calls,
    sent,
    started,
    startedWith,
    nodes,
    removed,
    reports,
    /** Answer every `chatAccount` from now on with `result`, and the ones waiting too. */
    answer(result: CallResult) {
      allowance = result;
      for (const each of pending.splice(0)) each.resolve(result);
    },
    /** Answer the `request` calls held so far, oldest first. */
    releaseRequests: () => {
      for (const release of heldRequests.splice(0)) release();
    },
    /** Answer the `account` derivations held so far, oldest first. */
    releaseAccounts: () => {
      for (const release of heldAccounts.splice(0)) release();
    },
    /** Let the held first flush finish. */
    releaseFlush: () => {
      for (const release of heldFlushes.splice(0)) release();
    },
    /** Answer the held replies: the host takes the first and refuses the others. */
    releaseReplies: () => {
      const [first, ...others] = heldReplies.splice(0);
      first?.({ ok: true });
      for (const refuse of others)
        refuse({
          ok: false,
          reason: "Refused",
          detail: "This request was already answered.",
        });
    },
    /** Let the state removals held so far finish, oldest first. */
    releaseRemoves: () => {
      for (const release of heldRemoves.splice(0)) release();
    },
    tell: (message: HostMessage) => listener(message),
    record: () =>
      JSON.parse(storage.get("record")!) as {
        conversations: {
          status: string;
          request?: string;
          account?: string;
          allowedUntil?: number;
        }[];
      },
    names: () => calls.map((call) => call.call),
  };
}

const settle = async () => {
  for (let round = 0; round < 10; round += 1)
    await new Promise((resolve) => setTimeout(resolve, 0));
};

const allowed: CallResult = {
  ok: true,
  allowance: { account: "", until: UNTIL },
};

/** Alice sent a request with a first message, and her host delivered Bob's authenticated reply. */
async function replied(w: ReturnType<typeof world>) {
  const id = await w.engine.start(
    { handle: { bytes: CONTACT as never } },
    "Hello Bob",
  );
  await settle();
  w.tell({
    kind: "response",
    handle: "0xa1",
    payload: `0x4b06${PEER.slice(2)}`,
  });
  await settle();
  return id;
}

describe("Kettle Chat's setup with a slow allowance", () => {
  test("the sender completes the request and shows its first message as sending while the allowance is not final", async () => {
    const alice = world();
    const id = await replied(alice);
    // The allowance has not answered, however long that takes.
    expect(alice.names()).toEqual(["request", "complete", "chatAccount"]);
    expect(
      alice.engine.messages(id).map(({ author, text, state }) => ({
        author: author.kind,
        text,
        state,
      })),
    ).toEqual([
      {
        author: "system",
        text: "You sent a chat request: Hello Bob",
        state: "received",
      },
      {
        author: "system",
        text: "They accepted your chat request. Setting up the encrypted chat…",
        state: "received",
      },
      { author: "self", text: "Hello Bob", state: "sending" },
    ]);
    const [shown] = alice.engine.conversations();
    expect([shown!.subtitle, shown!.composer]).toEqual([
      "Setting up the encrypted chat",
      { allowed: true },
    ]);
    expect([alice.started, alice.sent]).toEqual([[], []]);
    // Nothing that names the allowance is kept before the host answered.
    expect(alice.record().conversations[0]!.allowedUntil).toBeUndefined();

    alice.answer(allowed);
    await settle();
    expect(alice.sent).toEqual([{ peer: PEER, text: "Hello Bob" }]);
    expect(alice.engine.conversations()[0]!.subtitle).toBe(
      "Connected · end-to-end encrypted (Kettle MLS)",
    );
    expect(alice.record().conversations[0]!.allowedUntil).toBe(UNTIL);
  });

  test("an allowance that never finalizes fails the first message, leaves the request completed, and Retry sends it once", async () => {
    const alice = world();
    const id = await replied(alice);
    alice.answer({
      ok: false,
      reason: "Refused",
      detail: "it was not final after 180 s",
    });
    await settle();
    expect(alice.names()).toEqual(["request", "complete", "chatAccount"]);
    const failed = alice.engine
      .messages(id)
      .find((message) => message.author.kind === "self")!;
    expect([failed.text, failed.state]).toEqual(["Hello Bob", "failed"]);
    expect(alice.engine.messages(id).at(-1)!.text).toBe(
      "Kettle Chat cannot run this chat: your host did not let it post. it was not final after 180 s",
    );
    expect(alice.engine.conversations()[0]!.subtitle).toBe(
      "Setting up the encrypted chat",
    );
    expect(alice.sent).toEqual([]);

    alice.answer(allowed);
    await alice.engine.retry(id, failed.id);
    await settle();
    expect(alice.sent).toEqual([{ peer: PEER, text: "Hello Bob" }]);
    await expect(alice.engine.retry(id, failed.id)).rejects.toThrow(
      "Only a message that failed is sent again.",
    );
    expect(alice.sent).toHaveLength(1);
  });

  test("the recipient replies at Accept before its allowance answers, so a wait past the host's 30 s Accept window does not lose the reply", async () => {
    const bob = world();
    bob.tell({
      kind: "acceptIntent",
      request: {
        handle: "0xb1",
        payload: `0x4b06${PEER.slice(2)}`,
        sender: { bytes: CONTACT },
      },
    } as never);
    await settle();
    expect(bob.names()).toEqual(["reply", "chatAccount"]);
    expect(bob.started).toEqual([]);
    expect(bob.engine.conversations()[0]!.subtitle).toBe(
      "Setting up the encrypted chat · waiting for their first message",
    );
  });

  test("a reload during setup asks the host again before any post: a kept conversation does not skip the allowance", async () => {
    const alice = world();
    const id = await replied(alice);
    const again = world(alice.storage);
    await settle();
    // The new run's node asks for the allowance again and starts nothing until it answers.
    expect(again.names()).toEqual(["chatAccount"]);
    expect([again.started, again.sent]).toEqual([[], []]);
    // The first message was never queued in Kettle: it is offered again, not sent twice.
    expect(again.engine.messages(id).at(-1)!.state).toBe("failed");
    again.answer(allowed);
    await settle();
    expect(again.started).toHaveLength(1);
    expect(again.sent).toEqual([]);
  });
});

const ALICE_HANDLE = `0x${"a0".repeat(32)}`;
const BOB_HANDLE = `0x${"b0".repeat(32)}`;
const OTHER = `0x${"0e".repeat(32)}`;
const GROUP = `0x${"99".repeat(32)}`;

/** The host's Accept of request `handle` from `sender`, naming the conversation account `account`. */
const acceptIntent = (
  handle: string,
  sender: string,
  account: string,
): HostMessage =>
  ({
    kind: "acceptIntent",
    request: {
      handle,
      payload: `0x4b06${account.slice(2)}`,
      sender: { bytes: sender },
    },
  }) as never;

const lastCall = (w: ReturnType<typeof world>) => w.calls.at(-1)!;

describe("Kettle Chat with a second request between the same two people", () => {
  test("an Accept from someone this side already chats with is refused, and the chat keeps running", async () => {
    const alice = world();
    alice.answer(allowed);
    const id = await replied(alice);
    expect(alice.engine.conversations()[0]!.subtitle).toBe(
      "Connected · end-to-end encrypted (Kettle MLS)",
    );
    alice.tell(acceptIntent("0xb9", CONTACT, OTHER));
    await settle();
    expect(lastCall(alice)).toEqual({
      call: "refuse",
      handle: "0xb9",
      reason: "You already have a Kettle chat with this person.",
    });
    expect([
      alice.engine.conversations()[0]!.subtitle,
      alice.record().conversations[0]!.request,
      alice.nodes.map(({ stopped, freed }) => ({ stopped, freed })),
      alice.removed,
    ]).toEqual([
      "Connected · end-to-end encrypted (Kettle MLS)",
      "0xa1",
      [{ stopped: false, freed: false }],
      [],
    ]);
    expect(alice.engine.messages(id).some((m) => m.text === "Hello Bob")).toBe(
      true,
    );
  });

  test("when both sent a request and both accept, exactly one side accepts: the request of the lower account, whose sender withdraws nothing", async () => {
    const alice = world(new Map(), { seed: 0x10 });
    const bob = world(new Map(), { seed: 0x20 });
    await alice.engine.start({ handle: { bytes: BOB_HANDLE as never } }, "");
    await bob.engine.start({ handle: { bytes: ALICE_HANDLE as never } }, "");
    await settle();
    const aliceAccount = alice.record().conversations[0]!.account!;
    const bobAccount = bob.record().conversations[0]!.account!;
    expect(aliceAccount < bobAccount).toBe(true);

    alice.tell(acceptIntent("0xb1", BOB_HANDLE, bobAccount));
    bob.tell(acceptIntent("0xc1", ALICE_HANDLE, aliceAccount));
    await settle();
    // Alice keeps her own request, which has the lower account.
    expect([alice.names(), lastCall(alice)]).toEqual([
      ["request", "refuse"],
      {
        call: "refuse",
        handle: "0xb1",
        reason:
          "You asked them to chat too. Kettle Chat keeps your request: the chat starts when they accept it.",
      },
    ]);
    // Bob accepts hers and withdraws his own, so it is not posted again.
    expect(bob.calls.slice(1, 3)).toEqual([
      { call: "cancel", handle: "0xa1" },
      {
        call: "reply",
        handle: "0xc1",
        payload: `0x4b06${bob.record().conversations[0]!.account!.slice(2)}`,
        open: true,
      },
    ]);
    expect([bob.removed, bob.record().conversations[0]!.status]).toEqual([
      [bobAccount],
      "joining",
    ]);
    const [shown] = bob.engine.conversations();
    expect(
      bob.engine.messages(shown!.id).map((message) => message.text),
    ).toContain("You accepted their chat request, so yours was withdrawn");

    // Bob's reply reaches Alice as the answer to her request: one chat, on both sides.
    alice.tell({
      kind: "response",
      handle: "0xa1",
      payload: `0x4b06${bob.record().conversations[0]!.account!.slice(2)}`,
    });
    await settle();
    expect(alice.record().conversations[0]!.status).toBe("setting-up");
    expect(alice.names()).toEqual([
      "request",
      "refuse",
      "complete",
      "chatAccount",
    ]);
  });

  test("an Accept of the same request again, once replied to, is refused and changes nothing", async () => {
    const bob = world();
    bob.tell(acceptIntent("0xb1", CONTACT, PEER));
    await settle();
    const before = bob.record();
    bob.tell(acceptIntent("0xb1", CONTACT, PEER));
    await settle();
    expect([lastCall(bob), bob.record()]).toEqual([
      {
        call: "refuse",
        handle: "0xb1",
        reason: "You already accepted this request.",
      },
      before,
    ]);
  });
});

describe("Kettle Chat's late answers", () => {
  test("a request the host answers after a newer start is withdrawn, and the conversation keeps the newer request and account", async () => {
    const alice = world(new Map(), { holdRequests: true });
    await alice.engine.start({ handle: { bytes: CONTACT as never } }, "one");
    await settle();
    await alice.engine.start({ handle: { bytes: CONTACT as never } }, "two");
    await settle();
    const newer = alice.record().conversations[0]!.account!;
    alice.releaseRequests();
    await settle();
    expect(alice.calls).toEqual([
      expect.objectContaining({ call: "request" }),
      expect.objectContaining({ call: "request" }),
      { call: "cancel", handle: "0xa1" },
    ]);
    expect(alice.calls[1]).toMatchObject({
      request: { payload: `0x4b06${newer.slice(2)}` },
    });
    const [conversation] = alice.record().conversations;
    expect([
      conversation!.status,
      conversation!.request,
      conversation!.account,
    ]).toEqual(["waiting", "0xa2", newer]);
  });
});

describe("Kettle Chat's direct group", () => {
  /** Bob accepted Alice's request and his node runs, waiting for her Welcome. */
  async function joining() {
    const bob = world();
    bob.answer(allowed);
    bob.tell(acceptIntent("0xb1", CONTACT, PEER));
    await settle();
    const account = bob.record().conversations[0]!.account!;
    return { bob, account, node: bob.nodes[0]! };
  }

  test("a group from a Welcome is kept only when it is direct, its two members are this side and the peer, and the peer made it", async () => {
    const { bob, account, node } = await joining();
    const joined = (group: string, fields: Record<string, unknown>) =>
      node.emit({
        type: "joined",
        group,
        committer: PEER,
        members: [account, PEER],
        direct: true,
        ...fields,
      });
    const wrong = [
      `0x${"01".repeat(32)}`,
      `0x${"02".repeat(32)}`,
      `0x${"03".repeat(32)}`,
      `0x${"04".repeat(32)}`,
    ];
    joined(wrong[0]!, { members: [account, PEER, OTHER] });
    joined(wrong[1]!, { committer: OTHER, members: [account, OTHER] });
    joined(wrong[2]!, { direct: false });
    joined(wrong[3]!, { committer: account });
    await settle();
    expect([node.left, bob.record().conversations[0]!.status]).toEqual([
      wrong,
      "joining",
    ]);

    joined(GROUP, {});
    node.emit({
      type: "message",
      group: GROUP,
      sender: PEER,
      text: "Hello Bob",
      id: "m1",
    });
    await settle();
    const [shown] = bob.engine.conversations();
    expect([
      node.left,
      bob.record().conversations[0]!.status,
      bob.engine.messages(shown!.id).at(-1)!.text,
    ]).toEqual([wrong, "connected", "Hello Bob"]);
  });

  test("a leave that fails is reported, not thrown", async () => {
    const bob = world(new Map(), { leaveFails: true });
    bob.answer(allowed);
    bob.tell(acceptIntent("0xb1", CONTACT, PEER));
    await settle();
    const account = bob.record().conversations[0]!.account!;
    bob.nodes[0]!.emit({
      type: "joined",
      group: GROUP,
      committer: OTHER,
      members: [account, OTHER],
      direct: true,
    });
    await settle();
    expect(bob.reports.filter((line) => line.startsWith("leave"))).toEqual([
      `leave ${GROUP}: no such group`,
    ]);
  });

  test("on reload, a group this side made is kept while the peer is not in it yet", async () => {
    const alice = world();
    alice.answer(allowed);
    await replied(alice);
    const again = world(alice.storage);
    again.answer(allowed);
    await settle();
    const account = again.record().conversations[0]!.account!;
    const node = again.nodes[0]!;
    node.emit({
      type: "restored",
      group: GROUP,
      committer: account,
      members: [account],
      direct: PEER,
      active: true,
    });
    await settle();
    expect(node.left).toEqual([]);
  });
});

describe("Kettle Chat's nodes and their state", () => {
  test("stopping the engine stops and frees every node", async () => {
    const alice = world();
    alice.answer(allowed);
    await replied(alice);
    await alice.engine.stop();
    expect(
      alice.nodes.map(({ stopped, freed }) => ({ stopped, freed })),
    ).toEqual([{ stopped: true, freed: true }]);
  });

  test("a node still waiting for its allowance when the engine stops is never started", async () => {
    const bob = world();
    bob.tell(acceptIntent("0xb1", CONTACT, PEER));
    await settle();
    await bob.engine.stop();
    bob.answer(allowed);
    await settle();
    expect(bob.started).toEqual([]);
  });

  test("a cancelled request sent again removes the old account's Kettle state and keeps the history", async () => {
    const alice = world();
    const id = await alice.engine.start(
      { handle: { bytes: CONTACT as never } },
      "Hello Bob",
    );
    await settle();
    const old = alice.record().conversations[0]!.account!;
    alice.tell({
      kind: "outgoing",
      request: { handle: "0xa1", status: "cancelled" },
    } as never);
    await settle();
    await alice.engine.start({ handle: { bytes: CONTACT as never } }, "Again");
    await settle();
    expect([alice.removed, alice.record().conversations[0]!.request]).toEqual([
      [old],
      "0xa2",
    ]);
    expect(alice.engine.messages(id).map((message) => message.text)).toEqual([
      "You sent a chat request: Hello Bob",
      "You cancelled your chat request",
      "You sent a chat request: Again",
    ]);
  });
});

describe("Kettle Chat's operations that finish after a reset", () => {
  test("a request the host answers while a newer start still removes the old state is withdrawn, never kept", async () => {
    const alice = world(new Map(), { holdRequests: true, holdRemoves: true });
    await alice.engine.start({ handle: { bytes: CONTACT as never } }, "one");
    await settle();
    const second = alice.engine.start(
      { handle: { bytes: CONTACT as never } },
      "two",
    );
    await settle();
    // The old account's state is still being removed; its request is answered now.
    alice.releaseRequests();
    await settle();
    alice.releaseRemoves();
    await second;
    await settle();
    alice.releaseRequests();
    await settle();
    expect(alice.calls).toEqual([
      expect.objectContaining({ call: "request" }),
      { call: "cancel", handle: "0xa1" },
      expect.objectContaining({ call: "request" }),
    ]);
    const [conversation] = alice.record().conversations;
    expect([conversation!.status, conversation!.request]).toEqual([
      "waiting",
      "0xa2",
    ]);
    expect(
      alice.engine
        .messages(alice.engine.conversations()[0]!.id)
        .map((message) => message.text),
    ).toEqual(["You sent a chat request: two"]);
  });

  test("an Accept that crosses this side's request while it is still being sent withdraws that request once it is answered", async () => {
    // Her account is the higher one, so she accepts theirs.
    const alice = world(new Map(), {
      holdRequests: true,
      holdRemoves: true,
      seed: 0xd0,
    });
    await alice.engine.start({ handle: { bytes: CONTACT as never } }, "one");
    await settle();
    alice.tell(acceptIntent("0xb1", CONTACT, PEER));
    await settle();
    // Her request is answered while the Accept still removes the old state.
    alice.releaseRequests();
    await settle();
    alice.releaseRemoves();
    await settle();
    expect(alice.calls.slice(0, 3)).toEqual([
      expect.objectContaining({ call: "request" }),
      { call: "cancel", handle: "0xa1" },
      expect.objectContaining({ call: "reply", handle: "0xb1" }),
    ]);
    const [conversation] = alice.record().conversations;
    expect([conversation!.status, conversation!.request]).toEqual([
      "joining",
      "0xb1",
    ]);
  });

  test("an Accept of the same request again, while the first still derives its account, waits for that account and replies with it, refusing nothing", async () => {
    const bob = world(new Map(), { holdAccounts: true });
    bob.tell(acceptIntent("0xb1", CONTACT, PEER));
    await settle();
    // The host's Accept timed out; the user chose Accept again.
    bob.tell(acceptIntent("0xb1", CONTACT, PEER));
    await settle();
    bob.releaseAccounts();
    await settle();
    expect(sameAccountReplies(bob)).toEqual({ replies: 2, refused: [] });
    expect([bob.record().conversations[0]!.status, bob.removed]).toEqual([
      "joining",
      [],
    ]);
  });
});

/** What `w` replied and refused: replies all name the conversation's one account. */
function sameAccountReplies(w: ReturnType<typeof world>) {
  const account = w.record().conversations[0]!.account!;
  const replies = w.calls.filter((call) => call.call === "reply");
  expect(replies.map((call) => call.payload)).toEqual(
    replies.map(() => `0x4b06${account.slice(2)}`),
  );
  return {
    replies: replies.length,
    refused: w.calls.filter((call) => call.call === "refuse"),
  };
}

describe("Kettle Chat with the same request accepted again", () => {
  test("an earlier Accept still saving its account keeps it: the later one resets nothing, and both reply with it", async () => {
    const bob = world(new Map(), { holdFlush: true, holdRemoves: true });
    bob.tell(acceptIntent("0xb1", CONTACT, PEER));
    await settle();
    const first = bob.record().conversations[0]!;
    bob.tell(acceptIntent("0xb1", CONTACT, PEER));
    await settle();
    bob.releaseFlush();
    await settle();
    bob.releaseRemoves();
    await settle();
    expect(sameAccountReplies(bob)).toEqual({ replies: 2, refused: [] });
    const [conversation] = bob.record().conversations;
    expect([
      conversation!.account,
      conversation!.request,
      conversation!.status,
      bob.removed,
    ]).toEqual([first.account, "0xb1", "joining", []]);
  });

  test("an earlier Accept's reply still in flight: whichever the host takes names the one account this side runs", async () => {
    const bob = world(new Map(), { holdReplies: true, holdRemoves: true });
    bob.answer(allowed);
    bob.tell(acceptIntent("0xb1", CONTACT, PEER));
    await settle();
    // The host's Accept timed out while the reply was on its way; the user
    // chose Accept again.
    bob.tell(acceptIntent("0xb1", CONTACT, PEER));
    await settle();
    bob.releaseReplies();
    await settle();
    bob.releaseRemoves();
    await settle();
    expect(sameAccountReplies(bob)).toEqual({ replies: 2, refused: [] });
    const [conversation] = bob.record().conversations;
    expect([
      conversation!.status,
      bob.removed,
      bob.started,
      bob.reports.filter((line) => line.startsWith("reply")),
    ]).toEqual([
      "joining",
      [],
      [conversation!.account!],
      ["reply: This request was already answered."],
    ]);
  });

  test("after a reload, an Accept again of a request whose reply was never answered replies with the account kept", async () => {
    const bob = world(new Map(), { holdReplies: true });
    bob.tell(acceptIntent("0xb1", CONTACT, PEER));
    await settle();
    const kept = bob.record().conversations[0]!.account!;
    const again = world(bob.storage, { seed: 0x40 });
    again.tell(acceptIntent("0xb1", CONTACT, PEER));
    await settle();
    expect([
      again.calls.filter((call) => call.call === "reply"),
      again.removed,
      again.record().conversations[0]!.status,
    ]).toEqual([
      [
        {
          call: "reply",
          handle: "0xb1",
          payload: `0x4b06${kept.slice(2)}`,
          open: true,
        },
      ],
      [],
      "joining",
    ]);
  });
});

describe("Kettle Chat's recipient writing before the sender's group", () => {
  /** Bob accepted, his node runs, and he waits for Alice's Welcome. */
  async function joiningBob(storage = new Map<string, string>()) {
    const bob = world(storage);
    bob.answer(allowed);
    bob.tell(acceptIntent("0xb1", CONTACT, PEER));
    await settle();
    const conversation = bob.engine.conversations()[0]!;
    return { bob, id: conversation.id };
  }
  const joined = (
    bob: ReturnType<typeof world>,
    fields: Record<string, unknown> = {},
  ) => {
    const account = bob.record().conversations[0]!.account!;
    bob.nodes.at(-1)!.emit({
      type: "joined",
      group: GROUP,
      committer: PEER,
      members: [account, PEER],
      direct: true,
      ...fields,
    });
  };
  const own = (bob: ReturnType<typeof world>, id: string) =>
    bob.engine
      .messages(id)
      .filter((message) => message.author.kind === "self")
      .map(({ text, state }) => [text, state]);

  test("holds what the recipient writes, hands nothing to Kettle, and sends it in order, once, after the sender's group is joined", async () => {
    const { bob, id } = await joiningBob();
    expect(bob.engine.conversations()[0]!.composer).toEqual({
      allowed: true,
    });
    await bob.engine.send(id, "one");
    await bob.engine.send(id, "two");
    await settle();
    expect([bob.sent, own(bob, id)]).toEqual([
      [],
      [
        ["one", "sending"],
        ["two", "sending"],
      ],
    ]);

    // A group that is not this chat's direct group releases nothing.
    joined(bob, { committer: OTHER, members: [OTHER, PEER] });
    await settle();
    expect(bob.sent).toEqual([]);

    joined(bob);
    await settle();
    // The same group told again, and restored, send nothing twice.
    joined(bob);
    bob.nodes.at(-1)!.emit({
      type: "restored",
      group: GROUP,
      committer: PEER,
      members: [bob.record().conversations[0]!.account!, PEER],
      direct: PEER,
      active: true,
    });
    await settle();
    expect(bob.sent).toEqual([
      { peer: PEER, text: "one" },
      { peer: PEER, text: "two" },
    ]);
    expect(bob.engine.conversations()[0]!.subtitle).toBe(
      "Connected · end-to-end encrypted (Kettle MLS)",
    );
    // Written once joined, a message goes at once.
    await bob.engine.send(id, "three");
    await settle();
    expect(bob.sent.map((each) => each.text)).toEqual(["one", "two", "three"]);
  });

  test("a message that failed before is held by Retry too, and a reload keeps what is held until the group is joined", async () => {
    const { bob, id } = await joiningBob();
    await bob.engine.send(id, "held");
    await settle();
    // A message an earlier build refused while joining, kept as failed.
    const record = JSON.parse(bob.storage.get("record")!) as {
      conversations: { messages: Record<string, unknown>[] }[];
    };
    record.conversations[0]!.messages.push({
      id: "0xfa11",
      author: "self",
      text: "oii",
      at: 1_000,
      state: "failed",
    });
    bob.storage.set("record", JSON.stringify(record));

    const again = world(bob.storage);
    again.answer(allowed);
    await settle();
    expect(own(again, id)).toEqual([
      ["held", "sending"],
      ["oii", "failed"],
    ]);
    await again.engine.retry(id, "0xfa11");
    await expect(again.engine.retry(id, "0xfa11")).rejects.toThrow(
      "Only a message that failed is sent again.",
    );
    await settle();
    expect([again.sent, own(again, id)]).toEqual([
      [],
      [
        ["held", "sending"],
        ["oii", "sending"],
      ],
    ]);

    joined(again);
    await settle();
    expect(again.sent).toEqual([
      { peer: PEER, text: "held" },
      { peer: PEER, text: "oii" },
    ]);
  });

  test("Retry before the chat is set up anywhere is refused, not sent", async () => {
    // The Accept is kept, still accepting, when the tab stops.
    const bob = world(new Map(), { holdFlush: true });
    bob.tell(acceptIntent("0xb1", CONTACT, PEER));
    await settle();
    const record = JSON.parse(bob.storage.get("record")!) as {
      conversations: { status: string; messages: Record<string, unknown>[] }[];
    };
    record.conversations[0]!.messages.push({
      id: "0xfa12",
      author: "self",
      text: "too early",
      at: 1_000,
      state: "failed",
    });
    bob.storage.set("record", JSON.stringify(record));
    const again = world(bob.storage);
    await settle();
    const [conversation] = again.engine.conversations();
    expect(again.record().conversations[0]!.status).toBe("accepting");
    await expect(
      again.engine.retry(conversation!.id, "0xfa12"),
    ).rejects.toThrow("The chat is not connected.");
    expect(again.sent).toEqual([]);
  });
});

describe("Kettle Chat's log recovery setting", () => {
  test("each node starts with the relays the worker was given, and with none when it was given none", async () => {
    const custom = {
      relays: "custom",
      relayUrl: "http://127.0.0.1:3340/",
    } as const;
    const worlds = [
      world(new Map(), { iroh: { relays: "public" } }),
      world(new Map(), { iroh: custom }),
      world(),
    ];
    for (const w of worlds) {
      w.answer(allowed);
      await replied(w);
    }
    expect(worlds.map((w) => w.startedWith)).toEqual([
      [{ fast: false, iroh: { relays: "public" } }],
      [{ fast: false, iroh: custom }],
      [{ fast: false }],
    ]);
  });
});

describe("Kettle Chat's sender before its group exists", () => {
  /** Alice sent a request with no message, Bob replied, and her node runs. */
  async function emptyAccepted(storage = new Map<string, string>()) {
    const alice = world(storage);
    alice.answer(allowed);
    const id = await alice.engine.start(
      { handle: { bytes: CONTACT as never } },
      "",
    );
    await settle();
    alice.tell({
      kind: "response",
      handle: "0xa1",
      payload: `0x4b06${PEER.slice(2)}`,
    });
    await settle();
    return { alice, id };
  }
  const setUpLines = (w: ReturnType<typeof world>, id: string) =>
    w.engine
      .messages(id)
      .filter((message) => message.text === "The encrypted chat is set up")
      .length;

  test("an accepted request with no message is not set up when the node runs: it asks for a message, and the first one makes the group", async () => {
    const { alice, id } = await emptyAccepted();
    const [shown] = alice.engine.conversations();
    expect([
      alice.started.length,
      alice.sent,
      alice.record().conversations[0]!.status,
      shown!.subtitle,
      shown!.composer,
      setUpLines(alice, id),
    ]).toEqual([
      1,
      [],
      "setting-up",
      "Write a message to start the encrypted chat",
      { allowed: true },
      0,
    ]);
    await alice.engine.send(id, "hi");
    await settle();
    expect([
      alice.sent,
      alice.record().conversations[0]!.status,
      alice.engine.conversations()[0]!.subtitle,
      setUpLines(alice, id),
    ]).toEqual([
      [{ peer: PEER, text: "hi" }],
      "connected",
      "Connected · end-to-end encrypted (Kettle MLS)",
      1,
    ]);
  });

  test("a first message still on its way is not asked for again", async () => {
    const alice = world();
    await replied(alice);
    // The allowance has not answered: the request's message waits, sending.
    expect(alice.engine.conversations()[0]!.subtitle).toBe(
      "Setting up the encrypted chat",
    );
  });

  test("only Kettle's direct group with the peer sets the chat up, once, whether made or restored", async () => {
    const { alice, id } = await emptyAccepted();
    const node = alice.nodes.at(-1)!;
    node.emit({ type: "created", group: GROUP, direct: OTHER });
    await settle();
    expect(alice.record().conversations[0]!.status).toBe("setting-up");
    node.emit({ type: "created", group: GROUP, direct: PEER });
    node.emit({ type: "created", group: GROUP, direct: PEER });
    await settle();
    expect([
      alice.record().conversations[0]!.status,
      setUpLines(alice, id),
    ]).toEqual(["connected", 1]);

    // A reload restores the group this side made, and stays set up.
    const again = world(alice.storage);
    again.answer(allowed);
    await settle();
    const account = again.record().conversations[0]!.account!;
    again.nodes.at(-1)!.emit({
      type: "restored",
      group: GROUP,
      committer: account,
      members: [account, PEER],
      direct: PEER,
      active: true,
    });
    await settle();
    expect([
      again.record().conversations[0]!.status,
      setUpLines(again, id),
    ]).toEqual(["connected", 1]);
  });

  test("a chat an earlier build saved as connected with no group sets up again on reopen, its history kept", async () => {
    const { alice, id } = await emptyAccepted();
    // What an earlier build kept: connected at the node's start, no group.
    const record = JSON.parse(alice.storage.get("record")!) as {
      conversations: {
        status: string;
        messages: { id: string; author: string; text: string }[];
      }[];
    };
    record.conversations[0]!.status = "connected";
    record.conversations[0]!.messages.push({
      id: "0xold",
      author: "system",
      text: "The encrypted chat is set up",
    } as never);
    alice.storage.set("record", JSON.stringify(record));
    const history = record.conversations[0]!.messages.map(
      (message) => message.text,
    );

    const again = world(alice.storage);
    again.answer(allowed);
    await settle();
    expect([
      again.record().conversations[0]!.status,
      again.engine.messages(id).map((message) => message.text),
      again.engine.conversations()[0]!.subtitle,
      again.started.length,
    ]).toEqual([
      "setting-up",
      history,
      "Write a message to start the encrypted chat",
      1,
    ]);
    await again.engine.send(id, "hi");
    await settle();
    expect([again.sent, again.record().conversations[0]!.status]).toEqual([
      [{ peer: PEER, text: "hi" }],
      "connected",
    ]);
  });

  test("the recipient's node running does not set its chat up either: it waits for the Welcome", async () => {
    const bob = world();
    bob.answer(allowed);
    bob.tell(acceptIntent("0xb1", CONTACT, PEER));
    await settle();
    expect([
      bob.started.length,
      bob.record().conversations[0]!.status,
      bob.engine.conversations()[0]!.subtitle,
    ]).toEqual([
      1,
      "joining",
      "Setting up the encrypted chat · waiting for their first message",
    ]);
  });
});
