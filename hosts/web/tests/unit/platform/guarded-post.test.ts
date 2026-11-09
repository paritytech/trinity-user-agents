import { describe, expect, test } from "bun:test";
import * as sr25519 from "@scure/sr25519";
import type { ClaimedAccount } from "../../../src/platform/allowance-finality.js";
import type {
  AllowanceGuard,
  GuardRequest,
} from "../../../src/platform/allowance-usable.js";
import {
  NO_ALLOWANCE_INTERVAL_MS,
  NO_ALLOWANCE_SUBMITS,
  postWhenUsable,
  submitUntilAllowed,
  type GuardedPost,
} from "../../../src/platform/guarded-post.js";
import {
  decodeStatement,
  encodeSigned,
  verifyStatement,
} from "../../../src/platform/statement.js";
import type { SubmitResult } from "../../../src/platform/statement-store.js";
import { toHex } from "../../../src/invitations/sealing.js";
import { usable } from "../../support/allowance.js";

/**
 * A statement the host posts itself after a claim. The bytes are signed once
 * and every submit sends them; a claim that may still land is never followed
 * by a claim for another account; a best-block post whose claim did not
 * become final is recovered once, with the same account and the same bytes.
 */

const NOW = Date.UTC(2026, 10, 9, 17, 0, 0);
const secret = sr25519.secretFromSeed(new Uint8Array(32).fill(7));
const ACCOUNT = sr25519.getPublicKey(secret);
const CLAIM: ClaimedAccount = {
  account: toHex(ACCOUNT),
  period: Math.floor(NOW / 86_400_000),
  seq: 2,
  collection: "LitePeople",
  blockHash: "0xb10c",
};

/** One signed statement that expires `expiresInMs` from {@link NOW}. */
function signed(expiresInMs = 3_600_000): Uint8Array {
  const expires = Math.floor((NOW + expiresInMs) / 1000);
  return encodeSigned(
    {
      expiry: (BigInt(expires) << 32n) | BigInt(Math.floor(NOW / 1000)),
      channel: null,
      topics: [new Uint8Array(32).fill(1)],
      data: new Uint8Array([1, 2, 3]),
    },
    ACCOUNT,
    (payload) => sr25519.sign(secret, payload),
  );
}

/**
 * A post over a node that answers `answers` in turn, the last one ever
 * after, and a guard that answers `usableAt` with `notFinal` behind it.
 */
function scene(options: {
  answers?: SubmitResult[];
  usableAt?: "best" | "finalized";
  notFinal?: string | null;
  claims?: (ClaimedAccount | Error)[];
  bytes?: Uint8Array;
  finalRefusal?: string;
  /** Hold the check behind the post until the test calls `release`. */
  held?: boolean;
}) {
  const answers = options.answers ?? [{ status: "new" }];
  const claims = options.claims ?? [CLAIM];
  const log = {
    claims: 0,
    guard: [] as (GuardRequest["wait"] | "rule")[],
    submits: [] as string[],
    slept: [] as number[],
  };
  let unwanted: string | null = null;
  let running = true;
  let release!: () => void;
  const behind = options.held
    ? new Promise<void>((resolve) => (release = resolve))
    : Promise.resolve();
  const guard: AllowanceGuard = async (claim, request = {}) => {
    log.guard.push(request.wait ?? "rule");
    if (request.wait === "finalized" && options.finalRefusal !== undefined)
      throw new Error(options.finalRefusal);
    const answer = usable(
      claim,
      request.wait ?? options.usableAt ?? "best",
      behind.then(() => options.notFinal ?? null),
    );
    answer.running = () => running;
    return answer;
  };
  const post: GuardedPost = {
    claim: async () => {
      const claim = claims[Math.min(log.claims, claims.length - 1)]!;
      log.claims += 1;
      if (claim instanceof Error) throw claim;
      return claim;
    },
    bytes: options.bytes ?? signed(),
    sign: (payload) => sr25519.sign(secret, payload),
    submit: async (bytes) => {
      log.submits.push(toHex(bytes));
      return answers[Math.min(log.submits.length - 1, answers.length - 1)]!;
    },
    unwanted: () => unwanted,
  };
  let clock = NOW;
  const deps = {
    guard,
    sleep: async (ms: number) => {
      log.slept.push(ms);
      clock += ms;
    },
    now: () => clock,
  };
  return {
    post,
    deps,
    log,
    want: (why: string | null) => (unwanted = why),
    advance: (ms: number) => (clock += ms),
    end: () => (running = false),
    release: () => release(),
  };
}

const NO_ALLOWANCE: SubmitResult = {
  status: "rejected",
  reason: "noAllowance",
};

describe("submitting while the node does not see the claim yet", () => {
  test("sends the same bytes again, 2 s apart, until the node keeps them", async () => {
    const { post, deps, log } = scene({
      answers: [NO_ALLOWANCE, NO_ALLOWANCE, { status: "new" }],
    });
    expect(await submitUntilAllowed(post, deps.sleep)).toEqual({
      result: { status: "new" },
      submits: 3,
    });
    expect(new Set(log.submits).size).toBe(1);
    expect(log.slept).toEqual([
      NO_ALLOWANCE_INTERVAL_MS,
      NO_ALLOWANCE_INTERVAL_MS,
    ]);
  });

  test("stops after a bounded number of submits, and never retries another answer", async () => {
    const stuck = scene({ answers: [NO_ALLOWANCE] });
    expect(await submitUntilAllowed(stuck.post, stuck.deps.sleep)).toEqual({
      result: NO_ALLOWANCE,
      submits: NO_ALLOWANCE_SUBMITS,
    });
    const full = scene({
      answers: [{ status: "rejected", reason: "accountFull" }],
    });
    expect((await submitUntilAllowed(full.post, full.deps.sleep)).submits).toBe(
      1,
    );
  });
});

describe("a guarded post", () => {
  test("a session-ended final watch cannot invoke recovery even if the caller still wants the post", async () => {
    const s = scene({ held: true, notFinal: "the session ended" });
    const posted = await postWhenUsable(s.post, s.deps);
    s.end();
    s.release();
    expect(await posted.final).toEqual({
      tag: "unconfirmed",
      why: "the session ended",
    });
    expect([s.log.claims, s.log.submits.length]).toEqual([1, 1]);
  });

  test("knownExpired is not held, and never triggers recovery or a new proof", async () => {
    const s = scene({
      answers: [{ status: "knownExpired" }],
      notFinal: "not final",
    });
    const posted = await postWhenUsable(s.post, s.deps);
    await posted.final;
    expect([s.log.claims, s.log.submits.length, s.log.guard]).toEqual([
      1,
      1,
      ["rule"],
    ]);
  });
  test("at a best block: claims once, waits for the guard, submits, and ends final once the claim is", async () => {
    const { post, deps, log } = scene({});
    const posted = await postWhenUsable(post, deps);
    expect([posted.result, posted.usable.usableAt, await posted.final]).toEqual(
      [{ status: "new" }, "best", { tag: "final" }],
    );
    expect([log.claims, log.guard, log.submits.length]).toEqual([
      1,
      ["rule"],
      1,
    ]);
  });

  // The claim may still land: a claim for another account could hold a
  // second slot, and nothing is posted on a claim the core did not confirm.
  test("an uncertain claim posts nothing and claims nothing else", async () => {
    const { post, deps, log } = scene({
      claims: [
        new Error("notification claim for period 20766 seq 3 is uncertain"),
      ],
    });
    await expect(postWhenUsable(post, deps)).rejects.toThrow("is uncertain");
    expect([log.claims, log.guard, log.submits]).toEqual([1, [], []]);
  });

  test("a post no longer wanted while it waited is not submitted", async () => {
    const s = scene({});
    const guarded = s.deps.guard;
    s.deps.guard = async (claim, request) => {
      const answer = await guarded(claim, request);
      s.want("the request was cancelled");
      return answer;
    };
    await expect(postWhenUsable(s.post, s.deps)).rejects.toThrow(
      "the request was cancelled",
    );
    expect(s.log.submits).toEqual([]);
  });

  test("an unconfirmed best-block post replaces only the outer expiry and proof after finalized allowance, with no recovery claim", async () => {
    const { post, deps, log } = scene({
      notFinal:
        "its allowance was not final after 180 s: finalized block 0xf1 has no allowance for it yet",
      answers: [{ status: "new" }, { status: "known" }],
    });
    const posted = await postWhenUsable(post, deps);
    expect(await posted.final).toEqual({
      tag: "reposted",
      why: "its allowance was not final after 180 s: finalized block 0xf1 has no allowance for it yet",
      result: { status: "known" },
    });
    expect({
      claims: log.claims,
      guard: log.guard,
      submits: log.submits.length,
      sameBytes: new Set(log.submits).size,
    }).toEqual({
      claims: 1,
      guard: ["rule", "finalized"],
      submits: 2,
      sameBytes: 2,
    });
    const [first, next] = log.submits.map((hex) =>
      decodeStatement(new Uint8Array(Buffer.from(hex.slice(2), "hex"))),
    );
    expect({
      ...next!,
      expiry: first!.expiry,
      signature: first!.signature,
    }).toEqual(first!);
    expect(next!.expiry).toBe(first!.expiry + 1n);
    expect(verifyStatement(next!)).toBe(true);
  });

  test("a recovery that cannot reach the finalized head leaves the post unconfirmed and submits nothing more", async () => {
    const { post, deps, log } = scene({
      notFinal: "its allowance was not final after 180 s",
      finalRefusal:
        "This chat's account has its slot, but it was not final after 180 s",
    });
    const posted = await postWhenUsable(post, deps);
    expect(await posted.final).toEqual({
      tag: "unconfirmed",
      why: "its allowance was not final after 180 s; This chat's account has its slot, but it was not final after 180 s",
    });
    expect(log.submits.length).toBe(1);
  });

  test("the replacement fits the same one-statement quota and escapes an evicted-hash ban", async () => {
    const s = scene({ notFinal: "claim orphaned", held: true });
    const banned = new Set<string>();
    let held: Uint8Array | undefined;
    const sent: Uint8Array[] = [];
    s.post.submit = async (bytes) => {
      const hash = toHex(bytes);
      if (banned.has(hash)) return { status: "knownExpired" };
      if (
        held !== undefined &&
        decodeStatement(held).expiry >= decodeStatement(bytes).expiry
      )
        return { status: "rejected", reason: "accountFull" };
      held = bytes;
      sent.push(bytes);
      return { status: "new" };
    };
    const posted = await postWhenUsable(s.post, s.deps);
    banned.add(toHex(held!));
    s.release();
    expect(await posted.final).toMatchObject({
      tag: "reposted",
      result: { status: "new" },
    });
    expect(sent).toHaveLength(2);
    expect(held).toBe(sent[1]);
    expect(decodeStatement(sent[1]!).data).toEqual(
      decodeStatement(sent[0]!).data,
    );
    expect(s.log.claims).toBe(1);
  });

  test("no recovery once the day changed, the statement expired, or the post is no longer wanted", async () => {
    const dayChanged = scene({
      notFinal: "not final",
      held: true,
      bytes: signed(2 * 86_400_000),
    });
    const changed = await postWhenUsable(dayChanged.post, dayChanged.deps);
    dayChanged.advance(86_400_000);
    dayChanged.release();
    expect(await changed.final).toEqual({
      tag: "unconfirmed",
      why: "not final; the day changed, so it was not posted again",
    });

    const expiring = scene({
      notFinal: "not final",
      bytes: signed(60_000),
      held: true,
    });
    const expired = await postWhenUsable(expiring.post, expiring.deps);
    expiring.advance(120_000);
    expiring.release();
    expect(await expired.final).toEqual({
      tag: "unconfirmed",
      why: "not final; the statement has expired",
    });

    const cancelled = scene({ notFinal: "not final", held: true });
    const unwanted = await postWhenUsable(cancelled.post, cancelled.deps);
    cancelled.want("the request was cancelled");
    cancelled.release();
    expect(await unwanted.final).toEqual({
      tag: "unconfirmed",
      why: "not final; the request was cancelled",
    });

    for (const { log } of [dayChanged, expiring, cancelled])
      expect(log.submits.length).toBe(1);
    expect([
      dayChanged.log.claims,
      expiring.log.claims,
      cancelled.log.claims,
    ]).toEqual([1, 1, 1]);
  });

  test("a post that waited for the finalized head is final at once and is not followed", async () => {
    const { post, deps, log } = scene({
      usableAt: "finalized",
      notFinal: "never asked",
    });
    const posted = await postWhenUsable(post, deps);
    expect(await posted.final).toEqual({ tag: "final" });
    expect([log.claims, log.submits.length]).toEqual([1, 1]);
  });
});
