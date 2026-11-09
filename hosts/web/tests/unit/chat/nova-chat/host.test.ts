import { afterEach, beforeEach, describe, expect, test } from "bun:test";
import * as sr25519 from "@scure/sr25519";
import { asProductId } from "@parity/truapi";
import { bytesToHex, type HexString } from "@parity/truapi/scale";
import { utf8 } from "../../../../src/platform/codec.js";
import {
  blake2b256,
  x25519PublicKey,
} from "../../../../src/platform/crypto.js";
import {
  decodeStatement,
  encodeSigned,
} from "../../../../src/platform/statement.js";
import {
  NovaBoundary,
  isRefused,
} from "../../../../src/chat/nova-chat/boundary.js";
import {
  novaHostOver,
  type NovaHostPort,
} from "../../../../src/chat/nova-chat/session.js";
import type { HandleNames } from "../../../../src/contacts/contacts.js";
import { startNovaChatHost } from "../../../../src/invitations/nova-chat-host.js";
import { NotificationStore } from "../../../../src/notifications/notifications.js";
import type { SendPrompt } from "../../../../src/invitations/invitation-service.js";
import type { StatementStoreLike } from "../../../../src/platform/statement-store.js";
import { MemoryStatementStore } from "../../../support/statement-memory.js";
import { MemoryStorage } from "../../../support/test-support.js";
import {
  A,
  A_PEER,
  B_PEER,
  B_PER_CHAT_SECRET,
  NOTIFICATION_SECRET,
  bAcceptance,
  boundaryB,
  chatKeys,
} from "./fixtures.js";

/**
 * Nova Chat's host side alone (`src/invitations/nova-chat-host.ts`): when it
 * posts a request or an acceptance, with which notification account, how it
 * waits for finality and tries again, and what Decline and Block do. B is a
 * key boundary that writes to the in-memory store directly.
 */

const product = { id: asProductId("nova-chat.test"), name: "Nova Chat" };
/** A third person, Carol, so two requests can post at once. */
const C_PEER = bytesToHex(
  sr25519.getPublicKey(sr25519.secretFromSeed(new Uint8Array(32).fill(0x16))),
).toLowerCase();
const C_CHAT = x25519PublicKey(new Uint8Array(32).fill(0x26));
const handle = (account: string) =>
  bytesToHex(blake2b256(utf8(`handle:${account}`))).toLowerCase();
const handles: HandleNames = {
  accountFor: (value) =>
    ([A_PEER, B_PEER, C_PEER].find(
      (account) => handle(account) === value.toLowerCase(),
    ) as HexString | undefined) ?? null,
  handleFor: (account) => handle(account.toLowerCase()) as HexString,
  onChange: () => () => {},
};

let saved: unknown;
beforeEach(() => {
  saved = (globalThis as { localStorage?: unknown }).localStorage;
  Object.assign(globalThis, { localStorage: new MemoryStorage() });
});
afterEach(() => Object.assign(globalThis, { localStorage: saved }));

interface Harness {
  host: ReturnType<typeof startNovaChatHost>;
  port: NovaHostPort;
  notifications: NotificationStore;
  blocked: string[];
  claims: string[];
  submits: Uint8Array[];
  /** Tasks the host scheduled, run by hand. */
  scheduled: (() => void)[];
  settle: { gate: Promise<void> | null };
  maxClaimsInFlight: () => number;
}

function startA(
  store: MemoryStatementStore,
  options: {
    refuseSubmits?: number;
    /** Fail this many claims, after recording the account each was for. */
    failClaims?: number;
    claimMs?: number;
    /** Hold every chain read of a chat key until it resolves. */
    keysGate?: Promise<void>;
    /** Know no name, and hold the chain's name read until it resolves. */
    lookup?: Promise<string | null>;
    /** The user's answer to Send. Defaults to Send. */
    confirmSend?: (prompt: SendPrompt) => Promise<boolean>;
  } = {},
): Harness {
  const notifications = new NotificationStore(new MemoryStorage());
  notifications.setScope("A");
  const blocked: string[] = [];
  const claims: string[] = [];
  const submits: Uint8Array[] = [];
  const scheduled: (() => void)[] = [];
  const settle = { gate: null as Promise<void> | null };
  let inFlight = 0;
  let maxInFlight = 0;
  let refuse = options.refuseSubmits ?? 0;
  let failClaims = options.failClaims ?? 0;
  const connect = (): StatementStoreLike => {
    const inner = store.connect();
    return {
      ...inner,
      subscribe: inner.subscribe.bind(inner),
      close: inner.close.bind(inner),
      submit: async (encoded) => {
        if (refuse > 0) {
          refuse -= 1;
          return { status: "rejected", reason: "test" };
        }
        submits.push(encoded);
        return inner.submit(encoded);
      },
    };
  };
  const host = startNovaChatHost({
    product,
    storageKey: "nova-chat:A",
    boundary: new NovaBoundary(A, async (identity) => {
      await options.keysGate;
      return chatKeys([[C_PEER, C_CHAT]])(identity);
    }),
    handles,
    nameOf: (account) =>
      options.lookup === undefined
        ? { name: account === B_PEER ? "Bob" : "Alice", handle: account }
        : undefined,
    lookupName: () => options.lookup ?? Promise.resolve(null),
    blocked: () => blocked,
    notifications,
    verify: async () => null,
    openStore: async () => connect(),
    notificationSigner: (kind, requestId, period) => {
      const secret = sr25519.secretFromSeed(
        blake2b256(utf8(`${kind}/${requestId}/${period}`)),
      );
      return {
        public: sr25519.getPublicKey(secret),
        sign: (payload) => sr25519.sign(secret, payload),
      };
    },
    claimNotification: async (account) => {
      inFlight += 1;
      maxInFlight = Math.max(maxInFlight, inFlight);
      await new Promise((resolve) => setTimeout(resolve, options.claimMs ?? 5));
      claims.push(bytesToHex(account));
      inFlight -= 1;
      if (failClaims > 0) {
        failClaims -= 1;
        throw new Error(CONNECTION_LOST);
      }
      return { line: "claimed", seq: 1 };
    },
    notificationSettled: () => settle.gate ?? Promise.resolve(),
    accounts: {
      allow: async () => ({ account: "0x00", until: Date.now() + 86_400_000 }),
    },
    confirmSend: options.confirmSend ?? (async () => true),
    confirmAccept: async () => true,
    showChat: () => {},
    log: () => {},
    schedule: (task) => {
      scheduled.push(task);
      return () => {};
    },
  });
  const channel = new MessageChannel();
  host.attachChat(channel.port2);
  return {
    host,
    port: novaHostOver(channel.port1),
    notifications,
    blocked,
    claims,
    submits,
    scheduled,
    settle,
    maxClaimsInFlight: () => maxInFlight,
  };
}

const tick = (ms = 20) => new Promise((resolve) => setTimeout(resolve, ms));
/** How the core reports a claim whose chain connection went silent before it submitted anything. */
const CONNECTION_LOST =
  "state_call: RPC error: client error: json-rpc connection closed: RPC error: client error: no frame for 30s while a request waited";
async function until(check: () => boolean): Promise<void> {
  for (let tries = 0; !check(); tries++) {
    if (tries > 200) throw new Error("timed out");
    await tick(10);
  }
}

/** B's V2 request to A, posted to the store. */
async function bAsks(store: MemoryStatementStore, requestId: string) {
  const sent = await boundaryB().request(
    sr25519.getPublicKey(A.identitySecret),
    {
      requestId,
      nowMs: Date.now(),
      expiry: BigInt(Date.now()),
      welcome: "hello",
    },
  );
  if (isRefused(sent)) throw new Error(sent.refused);
  store.inject(
    encodeSigned(
      sent.statement,
      sr25519.getPublicKey(NOTIFICATION_SECRET),
      (payload) => sr25519.sign(NOTIFICATION_SECRET, payload),
    ),
  );
}

describe("posting requests and acceptances", () => {
  test("a request is answered once it is kept, and submitted only once its notification account's allowance is final", async () => {
    const store = new MemoryStatementStore();
    const a = startA(store);
    let release!: () => void;
    a.settle.gate = new Promise((resolve) => (release = resolve));
    await a.host.start();
    // The product's start query has its own time limit, shorter than the
    // finality wait: the answer must not wait for the post.
    expect(
      await a.port.call({
        call: "request",
        handle: handle(B_PEER),
        welcome: null,
      }),
    ).toEqual({ ok: true });
    await until(() => a.claims.length === 1);
    await tick(50);
    expect(a.submits).toHaveLength(0);
    expect(a.host.sent()).toMatchObject([{ status: { tag: "posting" } }]);
    release();
    await until(() => a.submits.length === 1);
    await tick(50);
    expect(a.submits).toHaveLength(1);
    expect(a.claims).toHaveLength(1);
    expect(a.host.sent()).toMatchObject([{ status: { tag: "waiting" } }]);
    a.host.stop();
  });

  test("a Send the user cancels keeps nothing and claims nothing", async () => {
    const store = new MemoryStatementStore();
    const a = startA(store, { confirmSend: async () => false });
    await a.host.start();
    expect(
      await a.port.call({
        call: "request",
        handle: handle(B_PEER),
        welcome: null,
      }),
    ).toEqual({ ok: false, reason: "You did not send the request." });
    await tick(50);
    expect(a.claims).toEqual([]);
    expect(a.submits).toEqual([]);
    expect(a.host.sent()).toEqual([]);
    expect(a.scheduled).toEqual([]);
    a.host.stop();
  });

  test("a request cancelled under Sent while its allowance becomes final is not submitted", async () => {
    const store = new MemoryStatementStore();
    const a = startA(store);
    let release!: () => void;
    a.settle.gate = new Promise((resolve) => (release = resolve));
    await a.host.start();
    await a.port.call({
      call: "request",
      handle: handle(B_PEER),
      welcome: null,
    });
    await until(() => a.claims.length === 1);
    expect(a.host.cancel(a.host.sent()[0]!.id)).toBe(true);
    release();
    await tick(50);
    expect(a.submits).toEqual([]);
    expect(a.scheduled).toEqual([]);
    expect(a.host.sent()).toEqual([]);
    a.host.stop();
  });

  test("a person blocked while the allowance becomes final gets no request, and it is not tried again", async () => {
    const store = new MemoryStatementStore();
    const a = startA(store);
    let release!: () => void;
    a.settle.gate = new Promise((resolve) => (release = resolve));
    await a.host.start();
    await a.port.call({
      call: "request",
      handle: handle(B_PEER),
      welcome: null,
    });
    await until(() => a.claims.length === 1);
    a.blocked.push(B_PEER);
    release();
    await until(() => a.host.sent()[0]?.status.tag === "paused");
    expect(a.host.sent()).toMatchObject([
      {
        status: {
          tag: "paused",
          reason: "Not posted: You blocked this person.",
        },
      },
    ]);
    expect(a.submits).toEqual([]);
    expect(a.scheduled).toEqual([]);
    a.host.stop();
  });

  test("a request whose allowance never became final is not submitted, and its try again uses the same account", async () => {
    const store = new MemoryStatementStore();
    const a = startA(store);
    a.settle.gate = Promise.reject(new Error("not final after 180 s"));
    a.settle.gate.catch(() => {});
    await a.host.start();
    expect(
      await a.port.call({
        call: "request",
        handle: handle(B_PEER),
        welcome: null,
      }),
    ).toEqual({ ok: true });
    await until(() => a.scheduled.length === 1);
    expect(a.submits).toHaveLength(0);
    expect(a.host.sent()).toMatchObject([
      {
        status: {
          tag: "retrying",
          reason: expect.stringContaining("not final"),
        },
      },
    ]);
    a.settle.gate = null;
    expect(a.scheduled).toHaveLength(1);
    a.scheduled.shift()!();
    await until(() => a.submits.length === 1);
    expect(a.claims).toHaveLength(2);
    expect(a.claims[1]).toBe(a.claims[0]!);
    expect(a.host.sent()).toMatchObject([{ status: { tag: "waiting" } }]);
    a.host.stop();
  });

  test("a claim that lost its chain connection says the request is tried again, and each try claims for the same account", async () => {
    const store = new MemoryStatementStore();
    const a = startA(store, { failClaims: 5 });
    await a.host.start();
    expect(
      await a.port.call({
        call: "request",
        handle: handle(B_PEER),
        welcome: null,
      }),
    ).toEqual({ ok: true });
    await until(() => a.scheduled.length === 1);
    expect(a.host.sent()).toMatchObject([
      {
        status: {
          tag: "retrying",
          reason: `Not posted yet, trying again in 30 s: ${CONNECTION_LOST}`,
        },
      },
    ]);
    for (let tries = 2; tries <= 5; tries++) {
      expect(a.scheduled).toHaveLength(1);
      a.scheduled.shift()!();
      await until(() => a.claims.length === tries);
      await tick();
    }
    expect(a.scheduled).toHaveLength(0);
    expect(a.host.sent()).toMatchObject([
      { status: { tag: "paused", reason: `Not posted: ${CONNECTION_LOST}` } },
    ]);
    expect(new Set(a.claims).size).toBe(1);
    expect(a.submits).toHaveLength(0);
    a.host.stop();
  });

  test("claims run one at a time", async () => {
    const store = new MemoryStatementStore();
    const a = startA(store, { claimMs: 150 });
    await a.host.start();
    await Promise.all([
      a.port.call({ call: "request", handle: handle(B_PEER), welcome: null }),
      a.port.call({ call: "request", handle: handle(C_PEER), welcome: null }),
    ]);
    await until(() => a.submits.length === 2);
    expect(a.claims).toHaveLength(2);
    expect(a.maxClaimsInFlight()).toBe(1);
    a.host.stop();
  });

  test("an Accept tried again after a failed post signs with the same account, from the peer's request id", async () => {
    const store = new MemoryStatementStore();
    const a = startA(store, { refuseSubmits: 1 });
    await a.host.start();
    await bAsks(store, "b-req");
    await tick();
    const item = a.notifications.list()[0]!;
    await expect(a.host.accept(item)).rejects.toThrow("not posted");
    expect(a.notifications.get(item.id)?.state.tag).toBe("pending");
    await a.host.accept(item);
    expect(a.claims).toHaveLength(2);
    expect(a.claims[1]).toBe(a.claims[0]!);
    const signer = sr25519.getPublicKey(
      sr25519.secretFromSeed(
        blake2b256(
          utf8(`chat-accept/b-req/${Math.floor(Date.now() / 86_400_000)}`),
        ),
      ),
    );
    expect(a.claims[0]).toBe(bytesToHex(signer));
    expect(a.notifications.get(item.id)?.state.tag).toBe("accepted");
    a.host.stop();
  });
});

describe("a Send the user agreed to in New chat", () => {
  const agree = (a: Harness, welcome: string) =>
    a.host.chatOptions.start!.confirm!(B_PEER, welcome);
  const ask = (a: Harness, welcome: string | null) =>
    a.port.call({ call: "request", handle: handle(B_PEER), welcome });

  test("is spent by the product's request, which asks nothing more, and a release after it keeps the request", async () => {
    const prompts: SendPrompt[] = [];
    const store = new MemoryStatementStore();
    const a = startA(store, {
      confirmSend: async (prompt) => (prompts.push(prompt), true),
    });
    await a.host.start();
    const release = await agree(a, "hi");
    expect(release).toBeFunction();
    expect(prompts).toMatchObject([{ purpose: "hi" }]);
    expect(await ask(a, "hi")).toEqual({ ok: true });
    // New chat stops waiting once the product answered: the kept request is
    // the user's, and it is posted all the same.
    release!();
    await until(() => a.submits.length === 1);
    expect(prompts).toHaveLength(1);
    expect(a.claims).toHaveLength(1);
    a.host.stop();
  });

  test("Cancel keeps nothing, and a request for another welcome asks again", async () => {
    const answers = [false, true, false];
    const prompts: SendPrompt[] = [];
    const store = new MemoryStatementStore();
    const a = startA(store, {
      confirmSend: async (prompt) => (prompts.push(prompt), answers.shift()!),
    });
    await a.host.start();
    expect(await agree(a, "hi")).toBeNull();
    expect(await agree(a, "hi")).toBeFunction();
    expect(await ask(a, "something else")).toEqual({
      ok: false,
      reason: "You did not send the request.",
    });
    expect(prompts.map((prompt) => prompt.purpose)).toEqual([
      "hi",
      "hi",
      "something else",
    ]);
    await tick(50);
    expect(a.host.sent()).toEqual([]);
    expect(a.claims).toEqual([]);
    a.host.stop();
  });

  test("is not spent once New chat stopped waiting: a late request asks again", async () => {
    const answers = [true, false];
    const store = new MemoryStatementStore();
    const a = startA(store, { confirmSend: async () => answers.shift()! });
    await a.host.start();
    const release = await agree(a, "");
    release!();
    expect(await ask(a, null)).toEqual({
      ok: false,
      reason: "You did not send the request.",
    });
    await tick(50);
    expect(a.host.sent()).toEqual([]);
    expect(a.claims).toEqual([]);
    a.host.stop();
  });

  test("a request that spent it, still building when New chat stopped waiting, keeps nothing and sends nothing", async () => {
    let release!: () => void;
    const store = new MemoryStatementStore();
    const a = startA(store, {
      keysGate: new Promise((resolve) => (release = resolve)),
    });
    await a.host.start();
    const stopWaiting = await agree(a, "hi");
    const answer = ask(a, "hi");
    await tick(50);
    stopWaiting!();
    release();
    expect(await answer).toEqual({
      ok: false,
      reason:
        "New chat stopped waiting before the request was kept. Nothing was sent.",
    });
    await tick(50);
    expect(a.host.sent()).toEqual([]);
    expect(a.claims).toEqual([]);
    a.host.stop();
  });

  test("a stop closes its open prompt, and nothing is kept, claimed or posted", async () => {
    let signal: AbortSignal | undefined;
    const store = new MemoryStatementStore();
    const a = startA(store, {
      confirmSend: (prompt) =>
        new Promise((resolve) => {
          signal = prompt.signal;
          // As the host's prompt does: an abort is a Cancel.
          prompt.signal?.addEventListener("abort", () => resolve(false));
        }),
    });
    await a.host.start();
    const agreeing = agree(a, "hi");
    await until(() => signal !== undefined);
    a.host.stop();
    await expect(agreeing).rejects.toThrow("Nova Chat stopped.");
    expect(signal!.aborted).toBe(true);
    await tick(50);
    expect(a.claims).toEqual([]);
    expect(a.submits).toEqual([]);
    expect(
      JSON.parse(localStorage.getItem("nova-chat:A") ?? `{"outgoing":[]}`)
        .outgoing,
    ).toEqual([]);
  });
});

describe("a copy of the host's state taken at any write", () => {
  test("resumes a pending request under the same request id, so a restore claims no new notification slot", async () => {
    const store = new MemoryStatementStore();
    const live = localStorage as MemoryStorage;
    const cuts: Map<string, string>[] = [];
    const write = live.setItem.bind(live);
    live.setItem = (key, value) => {
      write(key, value);
      cuts.push(
        new Map(live.keys().map((each) => [each, live.getItem(each)!])),
      );
    };
    const a = startA(store);
    await a.host.start();
    expect(
      await a.port.call({
        call: "request",
        handle: handle(B_PEER),
        welcome: null,
      }),
    ).toEqual({ ok: true });
    await until(() => a.host.sent()[0]?.status.tag === "waiting");
    a.host.stop();
    const [account] = a.claims;
    expect(a.claims).toEqual([account!]);
    expect(cuts.length).toBeGreaterThan(1);

    for (const cut of cuts) {
      const copy = new MemoryStorage();
      for (const [key, value] of cut) copy.setItem(key, value);
      Object.assign(globalThis, { localStorage: copy });
      const network = new MemoryStatementStore();
      network.statements = [...store.statements];
      const restored = startA(network);
      await restored.host.start();
      const pending = restored.host.sent();
      if (pending.length > 0 && pending[0]!.status.tag !== "waiting")
        await until(() => restored.submits.length === 1);
      await tick(50);
      // Whatever the copy held, the restored host claims for no other
      // account than the request's own on this day.
      expect(restored.claims.every((claimed) => claimed === account)).toBe(
        true,
      );
      expect(restored.host.sent().map((each) => each.status.tag)).toEqual(
        pending.length === 0 ? [] : ["waiting"],
      );
      restored.host.stop();
    }
  });
});

describe("Decline and Block", () => {
  test("Decline in the Inbox frees the person for a new chat, and a repost of the declined request is not listed again", async () => {
    const store = new MemoryStatementStore();
    const a = startA(store);
    await a.host.start();
    await bAsks(store, "b-req");
    await tick();
    const item = a.notifications.list()[0]!;
    expect(a.notifications.decline(item.id)).toMatchObject({ ok: true });
    expect(
      await a.port.call({
        call: "request",
        handle: handle(B_PEER),
        welcome: null,
      }),
    ).toEqual({ ok: true });
    a.host.cancel(a.host.sent()[0]!.id);
    await bAsks(store, "b-req");
    await tick();
    expect(a.notifications.list()).toHaveLength(1);
    a.host.stop();
  });

  test("an acceptance from someone blocked after the request does not set up the chat", async () => {
    const store = new MemoryStatementStore();
    const a = startA(store);
    await a.host.start();
    await a.port.call({
      call: "request",
      handle: handle(B_PEER),
      welcome: null,
    });
    await until(() => a.submits.length === 1);
    const posted = decodeStatement(a.submits[0]!);
    const sent = await boundaryB().openRequest(posted);
    if (isRefused(sent)) throw new Error(sent.refused);
    a.blocked.push(B_PEER);
    const requestId = sent.requestId;
    store.inject(
      encodeSigned(
        bAcceptance(requestId),
        sr25519.getPublicKey(B_PER_CHAT_SECRET),
        (payload) => sr25519.sign(B_PER_CHAT_SECRET, payload),
      ),
    );
    await tick();
    a.blocked.length = 0;
    expect(
      await a.port.call({ call: "routes", handle: handle(B_PEER) }),
    ).toEqual({
      ok: false,
      reason: "not an accepted contact",
      final: true,
    });
    a.host.stop();
  });
});

describe("a host that stopped changes nothing after", () => {
  const kept = () => localStorage.getItem("nova-chat:A");

  test("a request whose finality wait ends after a stop and a restart does not write over the new host's state", async () => {
    const store = new MemoryStatementStore();
    const first = startA(store);
    let fail!: (error: Error) => void;
    first.settle.gate = new Promise((_, reject) => (fail = reject));
    first.settle.gate.catch(() => {});
    await first.host.start();
    // The stop closes the port, so this call gets no answer.
    void first.port.call({
      call: "request",
      handle: handle(B_PEER),
      welcome: null,
    });
    await until(() => first.claims.length === 1);
    first.host.stop();
    localStorage.setItem("nova-chat:A", "the next wallet's state");
    const next = startA(store);
    await next.host.start();
    fail(new Error("not final"));
    await tick(100);
    expect(kept()).toBe("the next wallet's state");
    expect(first.scheduled).toHaveLength(0);
    expect(first.submits).toHaveLength(0);
    next.host.stop();
  });

  test("a request whose allowance becomes final after a stop is not submitted, and writes nothing", async () => {
    const store = new MemoryStatementStore();
    const a = startA(store);
    let release!: () => void;
    a.settle.gate = new Promise((resolve) => (release = resolve));
    await a.host.start();
    expect(
      await a.port.call({
        call: "request",
        handle: handle(B_PEER),
        welcome: null,
      }),
    ).toEqual({ ok: true });
    await until(() => a.claims.length === 1);
    a.host.stop();
    const before = kept();
    release();
    await tick(100);
    expect(a.submits).toEqual([]);
    expect(a.scheduled).toEqual([]);
    expect(kept()).toBe(before);
  });

  test("a request whose sender's chat key arrives after a stop is not listed", async () => {
    const store = new MemoryStatementStore();
    let release!: () => void;
    const a = startA(store, {
      keysGate: new Promise((resolve) => (release = resolve)),
    });
    await a.host.start();
    await bAsks(store, "b-req");
    await tick();
    a.host.stop();
    const before = kept();
    release();
    await tick(50);
    expect(a.notifications.list()).toEqual([]);
    expect(kept()).toBe(before);
  });

  test("a request whose sender's name arrives after a stop is not listed", async () => {
    const store = new MemoryStatementStore();
    let name!: (value: string | null) => void;
    const a = startA(store, {
      lookup: new Promise((resolve) => (name = resolve)),
    });
    await a.host.start();
    await bAsks(store, "b-req");
    await tick(50);
    a.host.stop();
    name("Bob");
    await tick(50);
    expect(a.notifications.list()).toEqual([]);
  });
});
