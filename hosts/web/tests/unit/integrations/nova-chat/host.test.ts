import { afterEach, beforeEach, describe, expect, test } from "bun:test";
import * as sr25519 from "@scure/sr25519";
import { asProductId } from "@parity/truapi";
import { bytesToHex, hexToBytes, type HexString } from "@parity/truapi/scale";
import { utf8 } from "../../../../bridges/primitives/codec.js";
import {
  blake2b256,
  x25519PublicKey,
} from "../../../../bridges/primitives/crypto.js";
import {
  decodeStatement,
  encodeSigned,
} from "../../../../bridges/primitives/statement.js";
import {
  NovaBoundary,
  isRefused,
} from "../../../../src/integrations/nova-chat/boundary.js";
import {
  novaHostOver,
  type NovaHostPort,
} from "../../../../apps/nova-chat/session.js";
import type { HandleNames } from "../../../../src/contacts/contacts.js";
import { startNovaChatHost } from "../../../../src/integrations/nova-chat/host.js";
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
import { usable } from "../../../support/allowance.js";
import type { AllowanceGuard } from "../../../../src/platform/allowance-usable.js";

/**
 * Nova Chat's host side alone (`src/integrations/nova-chat/host.ts`): when it
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
  attempted: Uint8Array[];
  /** Tasks the host scheduled, run by hand. */
  scheduled: (() => void)[];
  settle: Settle;
  maxClaimsInFlight: () => number;
}

/** The allowance guard's answers: when, where, and what its check behind a post says. */
interface Settle {
  gate: Promise<void> | null;
  usableAt: "best" | "finalized";
  notFinal: string | null;
  /** When set, a guard asked for the finalized head fails with it. */
  finalRefusal: string | null;
  /** What the guard was asked: `rule`, or the finalized head. */
  asked: string[];
  final: Promise<string | null> | null;
}

function startA(
  store: MemoryStatementStore,
  options: {
    refuseSubmits?: number;
    uncertainSubmits?: number;
    now?: () => number;
    guard?: AllowanceGuard;
    claimBlock?: string;
    /** Fail this many claims, after recording the account each was for. */
    failClaims?: number;
    claimMs?: number;
    /** Hold every chain read of a chat key until it resolves. */
    keysGate?: Promise<void>;
    /** Know no name, and hold the chain's name read until it resolves. */
    lookup?: Promise<string | null>;
    /** The user's answer to Send. Defaults to Send. */
    confirmSend?: (prompt: SendPrompt) => Promise<boolean>;
    notifications?: NotificationStore;
    supportsBest?: boolean;
    noAllowance?: number;
  } = {},
): Harness {
  const notifications =
    options.notifications ?? new NotificationStore(new MemoryStorage());
  notifications.setScope("A");
  const blocked: string[] = [];
  const claims: string[] = [];
  const submits: Uint8Array[] = [];
  const attempted: Uint8Array[] = [];
  const scheduled: (() => void)[] = [];
  const settle: Settle = {
    gate: null,
    usableAt: "finalized",
    notFinal: null,
    finalRefusal: null,
    asked: [],
    final: null,
  };
  let inFlight = 0;
  let maxInFlight = 0;
  let refuse = options.refuseSubmits ?? 0;
  let failClaims = options.failClaims ?? 0;
  let uncertain = options.uncertainSubmits ?? 0;
  let noAllowance = options.noAllowance ?? 0;
  const connect = (): StatementStoreLike => {
    const inner = store.connect();
    return {
      ...inner,
      supportsBestBlockPosting: async () => options.supportsBest !== false,
      subscribe: inner.subscribe.bind(inner),
      close: inner.close.bind(inner),
      submit: async (encoded) => {
        attempted.push(encoded);
        if (noAllowance > 0) {
          noAllowance -= 1;
          return { status: "rejected", reason: "noAllowance" };
        }
        if (uncertain > 0) {
          uncertain -= 1;
          throw new Error("submit answer lost");
        }
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
    now: options.now,
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
      return { line: "claimed", seq: 1, blockHash: options.claimBlock };
    },
    notificationGuard: async (claim, request = {}) => {
      await (settle.gate ?? Promise.resolve());
      settle.asked.push(request.wait ?? "rule");
      if (options.guard !== undefined) return options.guard(claim, request);
      if (request.wait === "finalized" && settle.finalRefusal !== null)
        throw new Error(settle.finalRefusal);
      return usable(
        claim,
        request.wait ?? settle.usableAt,
        settle.final ?? Promise.resolve(settle.notFinal),
      );
    },
    accounts: {
      allow: async () => ({ account: "0x00", until: Date.now() + 86_400_000 }),
    },
    confirmSend: options.confirmSend ?? (async () => true),
    confirmAccept: async () => true,
    showChat: () => {},
    log: () => {},
    schedule: (task) => {
      scheduled.push(task);
      return () => {
        const index = scheduled.indexOf(task);
        if (index >= 0) scheduled.splice(index, 1);
      };
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
    attempted,
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
  const sender = boundaryB();
  const sent = await sender.request(sr25519.getPublicKey(A.identitySecret), {
    requestId,
    nowMs: Date.now(),
    expiry: BigInt(Date.now()),
    welcome: "hello",
  });
  if (isRefused(sent)) throw new Error(sent.refused);
  store.inject(
    encodeSigned(
      sent.statement,
      sr25519.getPublicKey(NOTIFICATION_SECRET),
      (payload) => sr25519.sign(NOTIFICATION_SECRET, payload),
    ),
  );
  return sender;
}

describe("posting requests and acceptances", () => {
  test("a failed finalized acceptance stays incoming and pending, then retries without a new claim or proof", async () => {
    const store = new MemoryStatementStore();
    const a = startA(store, { supportsBest: false });
    a.settle.usableAt = "best";
    a.settle.finalRefusal = "claim not on finalized ancestry";
    await a.host.start();
    await bAsks(store, "b-final-fail");
    await tick();
    const item = a.notifications.list()[0]!;
    await expect(a.host.accept(item)).rejects.toThrow(
      "not on finalized ancestry",
    );
    expect(a.notifications.get(item.id)?.state.tag).toBe("pending");
    expect(await a.port.call({ call: "list" })).toMatchObject({
      ok: true,
      value: [{ state: "incoming" }],
    });
    expect(a.attempted).toHaveLength(0);
    a.settle.finalRefusal = null;
    await a.host.accept(item);
    expect(a.claims).toHaveLength(1);
    expect(a.notifications.get(item.id)?.state.tag).toBe("accepted");
    a.host.stop();
  });

  test("best-ready Accept establishes locally before finality, deduplicates concurrent Accepts and keeps native kind 20/account/topic binding", async () => {
    const store = new MemoryStatementStore();
    const a = startA(store, { claimBlock: "0xclaim" });
    let finish!: (why: string | null) => void;
    a.settle.usableAt = "best";
    a.settle.final = new Promise((resolve) => {
      finish = resolve;
    });
    await a.host.start();
    const sender = await bAsks(store, "native-best-accept");
    await until(() => a.notifications.list().length === 1);
    const item = a.notifications.list()[0]!;
    await Promise.all([a.host.accept(item), a.host.accept(item)]);
    expect([a.claims.length, a.submits.length, a.settle.asked]).toEqual([
      1,
      1,
      ["rule"],
    ]);
    expect(a.notifications.get(item.id)?.state.tag).toBe("pending");
    expect(await a.port.call({ call: "list" })).toMatchObject({
      ok: true,
      value: [{ state: "established" }],
    });
    expect(
      sender.openAcceptance(A_PEER, decodeStatement(a.submits[0]!)),
    ).toEqual({ established: true });
    expect(a.claims[0]).toBe(bytesToHex(decodeStatement(a.submits[0]!).signer));
    finish(null);
    await until(() => a.notifications.get(item.id)?.state.tag === "accepted");
    a.host.stop();
  });

  test("a late acceptance reorg is visible, explicit retry keeps its kind 20 payload and account, and late callbacks cannot undo success", async () => {
    const store = new MemoryStatementStore();
    const a = startA(store, { claimBlock: "0xclaim" });
    let finish!: (why: string | null) => void;
    a.settle.usableAt = "best";
    a.settle.final = new Promise((resolve) => {
      finish = resolve;
    });
    await a.host.start();
    await bAsks(store, "native-reorg-accept");
    await until(() => a.notifications.list().length === 1);
    const item = a.notifications.list()[0]!;
    await a.host.accept(item);
    a.settle.finalRefusal = "claim orphaned and no finalized allowance";
    finish("claim orphaned");
    await until(
      () =>
        a.notifications
          .get(item.id)
          ?.postingProblem?.includes("Select Accept") === true,
    );
    expect(a.notifications.get(item.id)?.state.tag).toBe("pending");
    expect(await a.port.call({ call: "list" })).toMatchObject({
      value: [{ state: "established" }],
    });
    a.settle.finalRefusal = null;
    await a.host.accept(item);
    await until(() => a.notifications.get(item.id)?.state.tag === "accepted");
    const [first, replacement] = a.submits.map(decodeStatement);
    expect(replacement).toMatchObject({
      signer: first!.signer,
      data: first!.data,
      topics: first!.topics,
      channel: first!.channel,
      expiry: first!.expiry + 1n,
    });
    expect(a.claims).toHaveLength(1);
    a.host.stop();
  });

  test("a restarted acceptance follows its saved inclusion without another claim or submit", async () => {
    const store = new MemoryStatementStore();
    const now = Date.UTC(2026, 10, 10, 12);
    const first = startA(store, { claimBlock: "0xclaim", now: () => now });
    let finish!: (why: string | null) => void;
    first.settle.usableAt = "best";
    first.settle.final = new Promise((resolve) => {
      finish = resolve;
    });
    await first.host.start();
    await bAsks(store, "native-restart-accept");
    await until(() => first.notifications.list().length === 1);
    const item = first.notifications.list()[0]!;
    await first.host.accept(item);
    first.host.stop();
    const next = startA(store, {
      notifications: first.notifications,
      now: () => now + 20_000,
    });
    await next.host.start();
    await until(
      () => next.notifications.get(item.id)?.state.tag === "accepted",
    );
    expect([
      next.claims.length,
      next.attempted.length,
      next.settle.asked,
    ]).toEqual([0, 0, ["finalized"]]);
    const kept = localStorage.getItem("nova-chat:A");
    finish("old host reorg");
    await tick();
    expect(localStorage.getItem("nova-chat:A")).toBe(kept);
    next.host.stop();
  });

  test("acceptance restart deadlines, blocks and later days do not authorize network work", async () => {
    for (const why of ["deadline", "block", "day"] as const) {
      localStorage.clear();
      const store = new MemoryStatementStore();
      const now = Date.UTC(2026, 10, 10, 12);
      const first = startA(store, { claimBlock: "0xclaim", now: () => now });
      first.settle.usableAt = "best";
      let finish!: (why: string | null) => void;
      first.settle.final = new Promise((resolve) => {
        finish = resolve;
      });
      await first.host.start();
      await bAsks(store, `native-${why}-accept`);
      await until(() => first.notifications.list().length === 1);
      const item = first.notifications.list()[0]!;
      await first.host.accept(item);
      first.host.stop();
      const next = startA(store, {
        notifications: first.notifications,
        now: () =>
          now +
          (why === "day" ? 86_400_000 : why === "deadline" ? 180_001 : 20_000),
      });
      if (why === "block") next.blocked.push(B_PEER);
      await next.host.start();
      await tick();
      expect([
        next.claims.length,
        next.attempted.length,
        next.settle.asked.length,
      ]).toEqual([0, 0, 0]);
      expect(next.notifications.get(item.id)?.state.tag).toBe("pending");
      next.host.stop();
      finish(null);
    }
  });

  test("a crash before the best-ready acceptance submit answer resumes only its exact saved bytes", async () => {
    const store = new MemoryStatementStore();
    const now = Date.UTC(2026, 10, 10, 12);
    const first = startA(store, {
      claimBlock: "0xclaim",
      uncertainSubmits: 1,
      now: () => now,
    });
    let finish!: (why: string | null) => void;
    first.settle.usableAt = "best";
    first.settle.final = new Promise((resolve) => {
      finish = resolve;
    });
    await first.host.start();
    await bAsks(store, "native-uncertain-accept");
    await until(() => first.notifications.list().length === 1);
    const item = first.notifications.list()[0]!;
    await expect(first.host.accept(item)).rejects.toThrow("answer lost");
    first.host.stop();
    const next = startA(store, {
      notifications: first.notifications,
      now: () => now + 10_000,
    });
    await next.host.start();
    await until(
      () => next.notifications.get(item.id)?.state.tag === "accepted",
    );
    expect(next.claims).toHaveLength(0);
    expect(next.attempted).toEqual(first.attempted);
    next.host.stop();
    finish(null);
  });

  test("a resumed acceptance uses the original remaining deadline and ignores a guard that finishes after it", async () => {
    const store = new MemoryStatementStore();
    let now = Date.UTC(2026, 10, 10, 12);
    const first = startA(store, { claimBlock: "0xclaim", now: () => now });
    let finish!: (why: string | null) => void;
    first.settle.usableAt = "best";
    first.settle.final = new Promise((resolve) => {
      finish = resolve;
    });
    await first.host.start();
    await bAsks(store, "native-deadline-accept");
    await until(() => first.notifications.list().length === 1);
    const item = first.notifications.list()[0]!;
    await first.host.accept(item);
    first.host.stop();
    now += 170_000;
    let signal: AbortSignal | undefined;
    let release!: () => void;
    const next = startA(store, {
      notifications: first.notifications,
      now: () => now,
      guard: async (claim, request) => {
        signal = request?.signal;
        await new Promise<void>((resolve) => {
          release = resolve;
        });
        return usable(claim);
      },
    });
    await next.host.start();
    await until(() => signal !== undefined);
    now += 10_001;
    next.scheduled.shift()!();
    await until(
      () =>
        next.notifications
          .get(item.id)
          ?.postingProblem?.includes("Select Accept") === true,
    );
    expect(signal!.aborted).toBe(true);
    expect(next.notifications.get(item.id)?.state.tag).toBe("pending");
    const kept = localStorage.getItem("nova-chat:A");
    release();
    await tick();
    expect(localStorage.getItem("nova-chat:A")).toBe(kept);
    expect([next.claims.length, next.attempted.length]).toEqual([0, 0]);
    next.host.stop();
    finish(null);
  });

  test("an older Inbox item cannot accept a newer native request from the same identity", async () => {
    const store = new MemoryStatementStore();
    const a = startA(store);
    await a.host.start();
    await bAsks(store, "native-earlier");
    await until(() => a.notifications.list().length === 1);
    const earlier = a.notifications.list()[0]!;
    await bAsks(store, "native-newer");
    await until(() => a.notifications.list().length === 2);
    await expect(a.host.accept(earlier)).rejects.toThrow("no longer pending");
    expect([a.claims.length, a.submits.length]).toEqual([0, 0]);
    a.host.stop();
  });

  test("a stale acceptance callback cannot downgrade a newer explicit success", async () => {
    const store = new MemoryStatementStore();
    const a = startA(store, { claimBlock: "0xclaim" });
    let finish!: (why: string | null) => void;
    a.settle.usableAt = "best";
    a.settle.final = new Promise((resolve) => {
      finish = resolve;
    });
    await a.host.start();
    await bAsks(store, "native-late-callback");
    await until(() => a.notifications.list().length === 1);
    const item = a.notifications.list()[0]!;
    await a.host.accept(item);
    await a.host.accept(item);
    await until(() => a.notifications.get(item.id)?.state.tag === "accepted");
    const kept = localStorage.getItem("nova-chat:A");
    finish("late orphaned claim");
    await tick();
    expect(a.notifications.get(item.id)).toMatchObject({
      state: { tag: "accepted" },
    });
    expect(a.notifications.get(item.id)?.postingProblem).toBeUndefined();
    expect(localStorage.getItem("nova-chat:A")).toBe(kept);
    expect(a.claims).toHaveLength(1);
    a.host.stop();
  });

  test("blocking or declining during an acceptance wait prevents submission", async () => {
    for (const why of ["block", "decline"] as const) {
      localStorage.clear();
      const store = new MemoryStatementStore();
      const a = startA(store);
      let release!: () => void;
      a.settle.gate = new Promise((resolve) => {
        release = resolve;
      });
      await a.host.start();
      await bAsks(store, `native-wait-${why}`);
      await until(() => a.notifications.list().length === 1);
      const item = a.notifications.list()[0]!;
      const pending = a.host.accept(item);
      await until(() => a.claims.length === 1);
      if (why === "block") a.blocked.push(B_PEER);
      else a.notifications.decline(item.id);
      release();
      await expect(pending).rejects.toThrow("not posted");
      expect(a.attempted).toHaveLength(0);
      a.host.stop();
    }
  });

  test("an ambiguous acceptance claim is never blindly submitted again", async () => {
    const store = new MemoryStatementStore();
    const a = startA(store, { failClaims: 1 });
    await a.host.start();
    await bAsks(store, "ambiguous-accept");
    await until(() => a.notifications.list().length === 1);
    const item = a.notifications.list()[0]!;
    await expect(a.host.accept(item)).rejects.toThrow("not posted");
    await expect(a.host.accept(item)).rejects.toThrow("no recovery claim");
    expect([a.claims.length, a.submits.length]).toEqual([1, 0]);
    a.host.stop();
  });

  test("transient noAllowance acceptance retries preserve identical signed bytes", async () => {
    const store = new MemoryStatementStore();
    const a = startA(store, { noAllowance: 1 });
    await a.host.start();
    await bAsks(store, "retry-accept");
    await until(() => a.notifications.list().length === 1);
    const posted = a.host.accept(a.notifications.list()[0]!);
    await until(() => a.scheduled.length === 1);
    a.scheduled.shift()!();
    await posted;
    expect(a.attempted).toHaveLength(2);
    expect(a.attempted[0]).toEqual(a.attempted[1]);
    expect(a.claims).toHaveLength(1);
    a.host.stop();
  });

  test("a lost submit answer preserves exact bytes across restart instead of generating another invitation", async () => {
    const store = new MemoryStatementStore();
    const first = startA(store, { uncertainSubmits: 1 });
    await first.host.start();
    await first.port.call({
      call: "request",
      handle: handle(B_PEER),
      welcome: "hello",
    });
    await until(() => first.scheduled.length === 1);
    first.host.stop();
    const next = startA(store);
    await next.host.start();
    await until(() => next.submits.length === 1);
    expect(next.attempted[0]).toEqual(first.attempted[0]);
    expect(next.claims).toHaveLength(0);
    next.host.stop();
  });

  test("late request failures stop after five attempts without spending another claim", async () => {
    const a = startA(new MemoryStatementStore());
    a.settle.usableAt = "best";
    a.settle.notFinal = "claim orphaned";
    a.settle.finalRefusal = "no finalized grant";
    await a.host.start();
    await a.port.call({
      call: "request",
      handle: handle(B_PEER),
      welcome: null,
    });
    for (let attempt = 1; attempt < 5; attempt++) {
      await until(() => a.scheduled.length === 1);
      a.scheduled.shift()!();
      await tick();
    }
    await until(() => a.host.sent()[0]?.status.tag === "paused");
    expect([a.claims.length, a.submits.length, a.scheduled.length]).toEqual([
      1, 1, 0,
    ]);
    a.host.stop();
  });

  test("a request's recovery timer cannot claim on a later day", async () => {
    let now = Date.now();
    const a = startA(new MemoryStatementStore(), {
      uncertainSubmits: 1,
      now: () => now,
    });
    await a.host.start();
    await a.port.call({
      call: "request",
      handle: handle(B_PEER),
      welcome: null,
    });
    await until(() => a.scheduled.length === 1);
    now += 86_400_000;
    a.scheduled.shift()!();
    await until(() => a.host.sent()[0]?.status.tag === "paused");
    expect(a.claims).toHaveLength(1);
    expect(a.attempted).toHaveLength(1);
    a.host.stop();
  });

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

  test("an unconfirmed best-block request gets one fresh outer proof after finalized allowance without another claim", async () => {
    const store = new MemoryStatementStore();
    const a = startA(store);
    a.settle.usableAt = "best";
    a.settle.notFinal = "its allowance was not final after 180 s";
    await a.host.start();
    expect(
      await a.port.call({
        call: "request",
        handle: handle(B_PEER),
        welcome: null,
      }),
    ).toEqual({ ok: true });
    await until(() => a.submits.length === 2);
    await tick(50);
    expect({
      claims: a.claims.length,
      sameAccount:
        bytesToHex(decodeStatement(a.submits[1]!).signer) === a.claims[0],
      asked: a.settle.asked,
      sameBytes: bytesToHex(a.submits[1]!) === bytesToHex(a.submits[0]!),
    }).toEqual({
      claims: 1,
      sameAccount: true,
      asked: ["rule", "finalized"],
      sameBytes: false,
    });
    const [first, replacement] = a.submits.map(decodeStatement);
    expect({
      ...replacement!,
      expiry: first!.expiry,
      signature: first!.signature,
    }).toEqual(first!);
    expect(a.host.sent()).toMatchObject([{ status: { tag: "waiting" } }]);
    a.host.stop();
  });

  test("when that recovery fails too, Sent says the request went at a best block and its claim is not final", async () => {
    const store = new MemoryStatementStore();
    const a = startA(store);
    a.settle.usableAt = "best";
    a.settle.notFinal = "its allowance was not final after 180 s";
    a.settle.finalRefusal = "the People chain could not be read";
    await a.host.start();
    await a.port.call({
      call: "request",
      handle: handle(B_PEER),
      welcome: null,
    });
    await until(() => a.host.sent()[0]?.status.tag === "retrying");
    expect(a.host.sent()).toMatchObject([
      {
        status: {
          tag: "retrying",
          reason: expect.stringContaining(
            "Posted at a best block, but its allowance was not final",
          ),
        },
      },
    ]);
    expect([a.submits.length, a.claims.length]).toEqual([1, 1]);
    a.settle.finalRefusal = null;
    a.scheduled.shift()!();
    await until(() => a.host.sent()[0]?.status.tag === "waiting");
    expect([a.submits.length, a.claims.length]).toEqual([2, 1]);
    expect(bytesToHex(decodeStatement(a.submits[0]!).data)).toBe(
      bytesToHex(decodeStatement(a.submits[1]!).data),
    );
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
    expect(a.claims).toHaveLength(1);
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
    expect(a.claims).toHaveLength(1);
    expect(a.settle.asked).toEqual(["rule", "rule"]);
    expect(a.attempted[1]).toEqual(a.attempted[0]);
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

describe("an unfinished provisional request after restart", () => {
  const NOW = Date.UTC(2026, 10, 9, 12);
  const BLOCK = `0x${"0c".repeat(32)}`;
  const savedState = () =>
    JSON.parse(localStorage.getItem("nova-chat:A")!) as {
      outgoing: {
        requestId: string;
        problem: string | null;
        attemptsLeft: number;
      }[];
      posts: Record<
        string,
        {
          period: number;
          bytes: string;
          confirmation?: { until: number; submitted: boolean };
          claim: { seq: number; blockHash: string };
          renewed?: boolean;
        }
      >;
    };
  async function provisional() {
    const store = new MemoryStatementStore();
    const writes: string[] = [];
    const storage = localStorage as MemoryStorage;
    const write = storage.setItem.bind(storage);
    storage.setItem = (key, value) => {
      write(key, value);
      if (key === "nova-chat:A") writes.push(value);
    };
    const first = startA(store, { now: () => NOW, claimBlock: BLOCK });
    first.settle.usableAt = "best";
    let finish!: (why: string | null) => void;
    first.settle.final = new Promise((resolve) => {
      finish = resolve;
    });
    await first.host.start();
    await first.port.call({
      call: "request",
      handle: handle(B_PEER),
      welcome: "hello",
    });
    await until(() => first.host.sent()[0]?.status.tag === "waiting");
    first.host.stop();
    storage.setItem = write;
    const saved = savedState();
    const key = `chat-request:${saved.outgoing[0]!.requestId}`;
    expect(saved.posts[key]).toMatchObject({
      claim: { blockHash: BLOCK },
      confirmation: { until: NOW + 180_000, submitted: true },
    });
    return { store, first, finish, key, writes };
  }

  test("malformed saved budgets pause only this request, preserve its public context and ciphertext, and never autorun", async () => {
    const { store, finish, key } = await provisional();
    const original = savedState();
    const invalid: unknown[] = [
      -1,
      0.5,
      NaN,
      Infinity,
      "NaN",
      "5",
      null,
      {},
      [],
      true,
      6,
      1e30,
    ];
    for (const attemptsLeft of invalid)
      for (const provisional of [true, false]) {
        const saved = structuredClone(original);
        const outgoing = {
          ...saved.outgoing[0]!,
          attemptsLeft,
          problem: provisional ? null : "retry failed",
        };
        if (!provisional) delete saved.posts[key]!.confirmation;
        localStorage.setItem(
          "nova-chat:A",
          JSON.stringify({ ...saved, outgoing: [outgoing] }),
        );
        const next = startA(store, { now: () => NOW + 20_000 });
        await next.host.start();
        await tick();
        expect(next.host.sent()).toMatchObject([
          {
            status: {
              tag: "paused",
              reason: expect.stringContaining(
                "invalid saved retry budget or confirmation",
              ),
            },
          },
        ]);
        expect([
          next.settle.asked.length,
          next.claims.length,
          next.attempted.length,
          next.scheduled.length,
        ]).toEqual([0, 0, 0, 0]);
        const paused = savedState();
        expect(paused.outgoing[0]!.attemptsLeft).toBe(0);
        expect(paused.posts[key]).toMatchObject({
          bytes: original.posts[key]!.bytes,
          claim: original.posts[key]!.claim,
          needsFinal: true,
        });
        expect(paused.posts[key]!.confirmation).toBeUndefined();
        const {
          outgoing: _outgoing,
          posts: _posts,
          ...unrelated
        } = JSON.parse(localStorage.getItem("nova-chat:A")!);
        const {
          outgoing: _originalOutgoing,
          posts: _originalPosts,
          ...originalUnrelated
        } = original;
        expect(unrelated).toEqual(originalUnrelated);
        next.host.stop();
        const again = startA(store, { now: () => NOW + 30_000 });
        await again.host.start();
        await tick();
        expect([
          again.settle.asked.length,
          again.claims.length,
          again.scheduled.length,
        ]).toEqual([0, 0, 0]);
        again.host.stop();
      }
    finish(null);
  });

  test("valid zero and maximum saved budgets keep exactly their remaining retries", async () => {
    const { store, finish, key } = await provisional();
    const original = savedState();
    for (const attemptsLeft of [0, 5]) {
      const saved = structuredClone(original);
      saved.outgoing[0]!.attemptsLeft = attemptsLeft;
      saved.outgoing[0]!.problem = "no finalized allowance";
      delete saved.posts[key]!.confirmation;
      localStorage.setItem("nova-chat:A", JSON.stringify(saved));
      const next = startA(store, {
        now: () => NOW + 20_000,
        guard: async () => {
          throw new Error("no finalized allowance");
        },
      });
      await next.host.start();
      for (let used = 1; used <= attemptsLeft; used++) {
        await until(
          () =>
            next.settle.asked.length === used &&
            next.host.sent()[0]?.status.tag ===
              (used < attemptsLeft ? "retrying" : "paused"),
        );
        expect(savedState().outgoing[0]!.attemptsLeft).toBe(
          attemptsLeft - used,
        );
        if (used < attemptsLeft) next.scheduled.shift()!();
      }
      await tick();
      expect([
        next.settle.asked.length,
        next.claims.length,
        next.attempted.length,
        next.scheduled.length,
      ]).toEqual([attemptsLeft, 0, 0, 0]);
      expect(next.host.sent()[0]?.status.tag).toBe("paused");
      expect(savedState().outgoing[0]!.problem).not.toContain("invalid saved");
      next.host.stop();
    }
    finish(null);
  });

  test("invalid saved confirmation deadlines cannot bypass the bounded watch", async () => {
    const { store, finish, key } = await provisional();
    const original = savedState();
    for (const until of [NaN, Infinity, null, "later", -1, 0.5, 1e30]) {
      const saved = structuredClone(original);
      saved.posts[key]!.confirmation = {
        ...saved.posts[key]!.confirmation!,
        until: until as number,
      };
      localStorage.setItem("nova-chat:A", JSON.stringify(saved));
      const next = startA(store, { now: () => NOW + 20_000 });
      await next.host.start();
      await tick();
      expect(next.host.sent()).toMatchObject([
        {
          status: {
            tag: "paused",
            reason: expect.stringContaining(
              "invalid saved retry budget or confirmation",
            ),
          },
        },
      ]);
      expect([
        next.settle.asked.length,
        next.claims.length,
        next.attempted.length,
        next.scheduled.length,
      ]).toEqual([0, 0, 0, 0]);
      next.host.stop();
    }
    finish(null);
  });

  test("a crash before the provisional submit answer retries only the identical signed ciphertext after confirmation", async () => {
    const { store, first, finish, key, writes } = await provisional();
    const beforeAnswer = writes.find(
      (text) =>
        JSON.parse(text).posts?.[key]?.confirmation?.submitted === false,
    )!;
    expect(beforeAnswer).toBeDefined();
    localStorage.setItem("nova-chat:A", beforeAnswer);
    const next = startA(store, {
      now: () => NOW + 20_000,
      uncertainSubmits: 1,
    });
    await next.host.start();
    await until(() => next.host.sent()[0]?.status.tag === "retrying");
    expect(next.attempted).toEqual(first.attempted);
    expect(next.claims).toHaveLength(0);
    expect(next.settle.asked).toEqual(["finalized"]);
    next.scheduled.shift()!();
    await until(() => next.host.sent()[0]?.status.tag === "waiting");
    expect(next.attempted).toEqual([first.attempted[0]!, first.attempted[0]!]);
    expect(savedState().posts[key]?.renewed).toBeUndefined();
    expect(savedState().posts[key]?.confirmation).toBeUndefined();
    expect(next.host.sent()[0]?.status.tag).toBe("waiting");
    next.host.stop();
    finish(null);
  });

  test("an old Waiting record without an explicit pending confirmation is never assumed provisional", async () => {
    const { store, finish, key } = await provisional();
    const saved = savedState();
    delete saved.posts[key]!.confirmation;
    localStorage.setItem("nova-chat:A", JSON.stringify(saved));
    const next = startA(store, { now: () => NOW + 20_000 });
    await next.host.start();
    await tick();
    expect([
      next.settle.asked.length,
      next.claims.length,
      next.attempted.length,
    ]).toEqual([0, 0, 0]);
    expect(next.host.sent()[0]?.status.tag).toBe("waiting");
    next.host.stop();
    finish(null);
  });

  test("resumes the known inclusion watch without reposting, then saves confirmation and leaves confirmed Waiting alone", async () => {
    const { store, first, finish, key } = await provisional();
    const next = startA(store, { now: () => NOW + 20_000 });
    await next.host.start();
    await until(() => savedState().posts[key]?.confirmation === undefined);
    expect(next.settle.asked).toEqual(["finalized"]);
    expect([next.claims.length, next.attempted.length]).toEqual([0, 0]);
    expect(next.host.sent()[0]?.status.tag).toBe("waiting");
    const confirmed = localStorage.getItem("nova-chat:A");
    finish("claim lost after old tab closed");
    await tick();
    expect(localStorage.getItem("nova-chat:A")).toBe(confirmed);
    expect(first.attempted).toHaveLength(1);
    next.host.stop();
    const last = startA(store, { now: () => NOW + 40_000 });
    await last.host.start();
    await tick();
    expect([
      last.settle.asked.length,
      last.claims.length,
      last.attempted.length,
    ]).toEqual([0, 0, 0]);
    last.host.stop();
  });

  test("a reorg resumes bounded recovery with one replacement, the same ciphertext and recipient dedupe, never another claim", async () => {
    const { store, first, finish, key } = await provisional();
    const checks: (string | undefined)[] = [];
    const next = startA(store, {
      now: () => NOW + 20_000,
      guard: async (claim) => {
        checks.push(claim.blockHash);
        if (claim.blockHash !== undefined)
          throw new Error("claim not on finalized ancestry");
        return usable(claim);
      },
    });
    await next.host.start();
    await until(() => next.host.sent()[0]?.status.tag === "retrying");
    expect(next.scheduled).toHaveLength(1);
    next.scheduled.shift()!();
    await until(() => next.host.sent()[0]?.status.tag === "waiting");
    expect(checks).toEqual([BLOCK, undefined]);
    expect([next.claims.length, next.attempted.length]).toEqual([0, 1]);
    const original = decodeStatement(first.attempted[0]!);
    const replacement = decodeStatement(next.attempted[0]!);
    expect({
      ...replacement,
      expiry: original.expiry,
      signature: original.signature,
    }).toEqual(original);
    expect(replacement.expiry).toBe(original.expiry + 1n);
    expect(savedState().posts[key]).toMatchObject({ renewed: true });
    expect(savedState().posts[key]?.confirmation).toBeUndefined();
    const receiver = boundaryB();
    expect(await receiver.openRequest(original)).toMatchObject({
      duplicate: false,
    });
    expect(await receiver.openRequest(replacement)).toMatchObject({
      duplicate: true,
    });
    next.host.stop();
    finish(null);
  });

  test("absent finalized allowance stays visible and keeps its attempt budget across repeated restarts", async () => {
    const { store, finish } = await provisional();
    for (let remaining = 4; remaining >= 0; remaining--) {
      const next = startA(store, {
        now: () => NOW + 20_000,
        guard: async () => {
          throw new Error("no finalized allowance");
        },
      });
      await next.host.start();
      await until(
        () =>
          next.host.sent()[0]?.status.tag ===
          (remaining > 0 ? "retrying" : "paused"),
      );
      expect(savedState().outgoing[0]!.attemptsLeft).toBe(remaining);
      expect(savedState().outgoing[0]!.problem).toContain(
        "no finalized allowance",
      );
      expect([next.claims.length, next.attempted.length]).toEqual([0, 0]);
      next.host.stop();
    }
    const paused = startA(store, { now: () => NOW + 20_000 });
    await paused.host.start();
    await tick();
    expect([
      paused.settle.asked.length,
      paused.attempted.length,
      paused.scheduled.length,
    ]).toEqual([0, 0, 0]);
    paused.host.stop();
    finish(null);
  });

  test("elapsed deadlines, later days, expired bytes and blocked people cannot resume network work", async () => {
    const { store, finish, key } = await provisional();
    const original = localStorage.getItem("nova-chat:A")!;
    for (const reason of ["deadline", "day", "expiry", "block"]) {
      localStorage.setItem("nova-chat:A", original);
      if (reason === "expiry") {
        const saved = savedState();
        const cached = saved.posts[key]!;
        const secret = sr25519.secretFromSeed(
          blake2b256(
            utf8(
              `chat-request/${saved.outgoing[0]!.requestId}/${cached.period}`,
            ),
          ),
        );
        const statement = decodeStatement(hexToBytes(cached.bytes));
        cached.bytes = bytesToHex(
          encodeSigned(
            {
              ...statement,
              expiry: BigInt(Math.floor((NOW - 1_000) / 1000)) << 32n,
            },
            statement.signer,
            (payload) => sr25519.sign(secret, payload),
          ),
        );
        localStorage.setItem("nova-chat:A", JSON.stringify(saved));
      }
      const next = startA(store, {
        now: () =>
          NOW +
          (reason === "deadline"
            ? 180_001
            : reason === "day"
              ? 86_400_000
              : 20_000),
      });
      if (reason === "block") next.blocked.push(B_PEER);
      await next.host.start();
      await until(() => next.host.sent()[0]?.status.tag !== "waiting");
      await tick();
      expect([
        next.settle.asked.length,
        next.claims.length,
        next.attempted.length,
      ]).toEqual([0, 0, 0]);
      expect(savedState().outgoing[0]!.problem).not.toBeNull();
      next.host.stop();
    }
    finish(null);
  });

  test("the original remaining deadline aborts a resumed read without extending on restart", async () => {
    const { store, finish, key } = await provisional();
    let signal: AbortSignal | undefined;
    let release!: () => void;
    let now = NOW + 170_000;
    const next = startA(store, {
      now: () => now,
      guard: async (claim, request) => {
        signal = request?.signal;
        await new Promise<void>((resolve) => {
          release = resolve;
        });
        return usable(claim);
      },
    });
    await next.host.start();
    await until(() => signal !== undefined);
    expect(savedState().posts[key]!.confirmation!.until).toBe(NOW + 180_000);
    now = NOW + 180_001;
    next.scheduled.shift()!();
    await until(() => next.host.sent()[0]?.status.tag === "paused");
    expect(signal!.aborted).toBe(true);
    expect(next.attempted).toHaveLength(0);
    expect(next.claims).toHaveLength(0);
    const terminal = localStorage.getItem("nova-chat:A");
    release();
    await tick();
    expect(localStorage.getItem("nova-chat:A")).toBe(terminal);
    next.host.stop();
    finish(null);
  });

  test("session end, cancel and block during a resumed read prevent recovery and stale writes", async () => {
    const { store, finish } = await provisional();
    const original = localStorage.getItem("nova-chat:A")!;
    for (const reason of ["stop", "cancel", "block"]) {
      localStorage.setItem("nova-chat:A", original);
      let signal: AbortSignal | undefined;
      let fail!: () => void;
      const next = startA(store, {
        now: () => NOW + 20_000,
        guard: async (_claim, request) => {
          signal = request?.signal;
          await new Promise<void>((_, reject) => {
            fail = () => reject(new Error("claim lost"));
          });
          throw new Error("unreachable");
        },
      });
      await next.host.start();
      await until(() => signal !== undefined);
      if (reason === "stop") next.host.stop();
      if (reason === "cancel") next.host.cancel(next.host.sent()[0]!.id);
      if (reason === "block") next.blocked.push(B_PEER);
      const before = localStorage.getItem("nova-chat:A");
      fail();
      await tick();
      expect([
        next.claims.length,
        next.attempted.length,
        next.scheduled.length,
      ]).toEqual([0, 0, 0]);
      if (reason !== "block") {
        expect(signal!.aborted).toBe(true);
        expect(localStorage.getItem("nova-chat:A")).toBe(before);
      } else expect(next.host.sent()[0]?.status.tag).toBe("paused");
      next.host.stop();
    }
    finish(null);
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
