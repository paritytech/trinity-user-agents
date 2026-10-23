// A session that stops anywhere must finish when a restarted worker is
// assigned it again and resumes from the state it stored, and must not move
// funds twice on the way. Each case stops a session at one of its kill points,
// then runs it again from the state it stored, against a fake host and chain
// that answer a repeated action the way the real ones do. The fake host orders reports as
// the core does (host_logic/funding.rs `report` and `follows`).

import { beforeEach, expect, mock, test } from "bun:test";
import type {
  FundingAssignment,
  FundingCancelAnswer,
  FundingUpdate,
} from "@parity/truapi";

process.env.FUNDTEST_CARD_ARRIVAL_MS = "0";
process.env.FUNDTEST_PAYOUT_MS = "0";
process.env.FUNDTEST_REPORT_RETRY_MS = "1";

const ONE_CASH = 1_000_000n;
const DEPOSIT = 1_000n;

type Answer = {
  match<A, B>(
    ok: (value: unknown) => A,
    err: (error: unknown) => B,
  ): Promise<A | B>;
};
const ok = (value?: unknown): Answer => ({
  match: async (okay) => okay(value),
});
const refused = (tag: string): Answer => ({
  match: async (_okay, err) => err({ tag }),
});
const never: Answer = { match: () => new Promise(() => {}) };
const stream = (items: unknown[]) => ({
  subscribe(observer: { next: (item: unknown) => void }) {
    for (const item of items) observer.next(item);
    return { unsubscribe() {} };
  },
});

/** What the fake core keeps about the one session under test. */
interface Core {
  /** Rank of the last ranked update, as `update_rank` gives it. */
  lastRank: number;
  /** Top-up and payment ids the reports named. */
  named: string[];
  failed: boolean;
  /** The payment completed, so the session is released. */
  released: boolean;
  converting: boolean;
  payout?: "paid" | "failed";
  refunded: boolean;
  /** Where the user's cancel stands, as the core's `FundingCancel` does. */
  cancel?: "requested" | "offered" | "refused" | "accepted" | "declined";
  /** What the user paid on an outbound session, which a refund may not
   *  exceed. */
  debited: bigint;
}

/** What the fake host, chain and faucet saw. */
interface World {
  /** Every update the host took, in order. */
  reports: string[];
  core: Core;
  /** The state the session last stored. */
  saved: string | undefined;
  /** The world just before each store of the session's state, to resume
   *  from as if the worker stopped there. */
  checkpoints: Omit<World, "checkpoints">[];
  storage: Map<string, string>;
  topUps: string[];
  requests: string[];
  /** Answers the host gives a report instead of taking it, in order, by tag. */
  reportFaults: { tag: string; answer: string }[];
  frame: "closes" | "stays-open";
  drips: number;
  dripLands: "at-once" | "on-repeat";
  dripped: boolean;
  pas: bigint;
  ticksBeforeDone: number;
  /** Whether the user's payment completes, is claimed in part, or stays
   *  pending. */
  paymentStatus: "completed" | "partial" | "pending";
  /** Planning the conversion throws a programming error. */
  planFails: boolean;
}

const freshCore = (): Core => ({
  lastRank: -1,
  named: [],
  failed: false,
  released: false,
  converting: false,
  refunded: false,
  debited: 0n,
});

let world: World;

beforeEach(() => {
  world = {
    reports: [],
    core: freshCore(),
    saved: undefined,
    checkpoints: [],
    storage: new Map(),
    topUps: [],
    requests: [],
    reportFaults: [],
    frame: "closes",
    drips: 0,
    dripLands: "at-once",
    dripped: false,
    pas: 0n,
    ticksBeforeDone: 0,
    paymentStatus: "completed",
    planFails: false,
  };
});

/** What a partly claimed payment takes. */
const PARTIAL = 20_000_000n;

const reportKey = (update: FundingUpdate) =>
  JSON.stringify(update, (_key, value) =>
    typeof value === "bigint" ? value.toString() : value,
  );

/** The rank `update_rank` gives an update, or none for those outside the
 *  order. */
function rankOf(update: FundingUpdate): number | undefined {
  switch (update.tag) {
    case "AwaitingPayment":
      return 0;
    case "PaymentReceived":
      return update.value.finalized ? 2 : 1;
    case "Converting":
      return 3;
    case "Crediting":
    case "Collecting":
      return 4;
    case "Delivered":
      return 5;
    case "Failed":
      return 6;
    default:
      return undefined;
  }
}

/** The core's answer to `update` on an outbound session it released. */
function reportReleased(update: FundingUpdate): string | undefined {
  const { core } = world;
  if (!core.released) return core.failed ? "NotFound" : "OutOfOrder";
  const owedRefund = core.payout === "failed" || core.cancel === "accepted";
  if (update.tag === "Refunding") {
    if (!owedRefund || core.refunded || update.value.amount > core.debited)
      return "OutOfOrder";
    if (!world.topUps.includes(update.value.topUpId)) return "NotStarted";
    core.refunded = true;
    return undefined;
  }
  if (
    core.payout ||
    core.cancel === "accepted" ||
    (update.tag === "Converting" && core.converting)
  )
    return "OutOfOrder";
  if (update.tag === "Converting") core.converting = true;
  if (update.tag === "Payout")
    core.payout = update.value.outcome.tag === "Failed" ? "failed" : "paid";
  return undefined;
}

/** The core's answer to `update` on an open session. */
function reportOpen(update: FundingUpdate): string | undefined {
  const { core } = world;
  if (core.failed || core.released) return "NotFound";
  const moving = core.named.length > 0;
  const started =
    moving || world.topUps.length > 0 || world.requests.length > 0;
  const rank = rankOf(update);
  const follows =
    update.tag === "Details"
      ? true
      : update.tag === "Failed"
        ? !started
        : update.tag === "Deposit"
          ? !moving
          : update.tag === "Crediting"
            ? core.lastRank <= 4 &&
              core.named.length < 3 &&
              !core.named.includes(update.value.topUpId)
            : update.tag === "Delivered"
              ? core.lastRank < 5 && moving
              : rank !== undefined && core.lastRank < rank;
  if (!follows) return "OutOfOrder";
  if (update.tag === "Crediting") {
    if (!world.topUps.includes(update.value.topUpId)) return "NotStarted";
    core.named.push(update.value.topUpId);
  }
  if (update.tag === "Collecting") {
    if (!world.requests.includes(update.value.paymentId)) return "NotStarted";
    core.named.push(update.value.paymentId);
    // The fake payment completes at once, which releases the session.
    core.released = true;
    core.debited =
      world.paymentStatus === "partial" ? PARTIAL : update.value.amount;
  }
  if (update.tag === "Failed") core.failed = true;
  if (rank !== undefined) core.lastRank = rank;
  return undefined;
}

const client = {
  fundingProvider: {
    report: ({ intent, update }: { intent: string; update: FundingUpdate }) => {
      const fault = world.reportFaults.find((each) => each.tag === update.tag);
      if (fault) {
        world.reportFaults.splice(world.reportFaults.indexOf(fault), 1);
        return refused(fault.answer);
      }
      const afterRelease =
        update.tag === "Payout" ||
        update.tag === "Refunding" ||
        update.tag === "PayoutRetrying" ||
        (update.tag === "Converting" && intent.startsWith("out"));
      const refusal = afterRelease
        ? reportReleased(update)
        : reportOpen(update);
      if (refusal) return refused(refusal);
      world.reports.push(reportKey(update));
      return ok();
    },
    presentFrame: () =>
      world.frame === "closes" ? ok({ outcome: { tag: "Closed" } }) : never,
  },
  payment: {
    topUp: ({ id }: { id: string }) => {
      const repeat = world.topUps.includes(id);
      world.topUps.push(id);
      return repeat ? refused("AlreadyExists") : ok();
    },
    topUpStatusSubscribe: () =>
      stream([{ tag: "Claimed", value: { finalized: true } }]),
    request: ({ id }: { id: string }) => {
      const repeat = world.requests.includes(id);
      world.requests.push(id);
      return repeat ? refused("AlreadyExists") : ok();
    },
    statusSubscribe: () =>
      stream(
        world.paymentStatus === "completed"
          ? [{ tag: "Completed" }]
          : world.paymentStatus === "partial"
            ? [
                {
                  tag: "ClaimedPartially",
                  value: { actualClaimed: PARTIAL.toString() },
                },
              ]
            : [],
      ),
  },
  localStorage: {
    read: ({ key }: { key: string }) => ok({ value: world.storage.get(key) }),
    write: ({ key, value }: { key: string; value: string }) => {
      if (key.startsWith("funding:")) {
        const { checkpoints: _, ...now } = world;
        world.checkpoints.push(structuredClone(now));
        world.saved = value;
      }
      world.storage.set(key, value);
      return ok();
    },
    clear: ({ key }: { key: string }) => {
      world.storage.delete(key);
      return ok();
    },
    subscribe: () => stream([]),
  },
  permissions: { requestRemotePermission: () => ok({ granted: true }) },
  worker: { beginOperation: () => ok({ id: 1 }), endOperation: () => ok() },
};

// getcash's packages are only reached through the chain modules faked below,
// apart from these few names the session itself imports.
mock.module("@getsome/funding", () => ({
  FundingHeldError: class extends Error {},
  FundingShortfallError: class extends Error {},
  PASEO_ASSET_HUB_PARA_ID: 1500,
}));
mock.module("@getsome/core", () => ({
  TOKENS: { PAS: { symbol: "PAS", decimals: 10 } },
}));

const host = await import("../worker/host");
mock.module("../worker/host", () => ({ ...host, truapi: async () => client }));

const faucet = await import("../worker/faucet");
mock.module("../worker/faucet", () => ({
  ...faucet,
  requestDrip: async () => {
    world.drips += 1;
    if (world.dripped) {
      world.pas = DEPOSIT;
      throw new faucet.FaucetRefused("already dripped today", true);
    }
    world.dripped = true;
    if (world.dripLands === "at-once") world.pas = DEPOSIT;
    return "0xd1";
  },
}));

mock.module("../worker/chain", () => ({
  onTestnet: () => true,
  withChains: <T>(work: (chains: unknown) => Promise<T>) =>
    work({ assetHub: {}, people: {} }),
}));

mock.module("../worker/onchain", () => ({
  CLAIM_UNIT: 10_000n,
  depositToken: () => ({ symbol: "PAS", decimals: 10 }),
  displaySymbol: () => "PAS",
  planConversion: async () => {
    if (world.planFails) throw new TypeError("no route for this amount");
    return {
      route: { tier: "pool" },
      depositSymbol: "PAS",
      deposit: DEPOSIT.toString(),
      remoteFeeBuffer: "0",
      keepNativeForFees: "0",
      slippagePct: 1,
    };
  },
  depositBalance: async () => world.pas,
  sweepSurplus: async () => 0n,
  convertOnce: async () => {
    if (world.ticksBeforeDone === 0) return "done";
    world.ticksBeforeDone -= 1;
    return "submitted";
  },
  claimable: async () => 25n * ONE_CASH,
}));

const accounts = await import("../worker/accounts");
const fakeAccount = (intent: string) => ({
  address: `account-of-${intent}`,
  publicKey: new TextEncoder().encode(intent.padEnd(32, ".")).slice(0, 32),
  secretKey: new Uint8Array(64),
  signer: { signBytes: async () => new Uint8Array(64) },
});
mock.module("../worker/accounts", () => ({
  ...accounts,
  sessionAccount: async (intent: string) => fakeAccount(intent),
  reserveAccount: async () => fakeAccount("collection"),
  topUpSecret: () => "0x00",
}));

const { Session: Running, MAX_RESTARTS } = await import("../worker/session");
const { actionKey, toStored } = await import("../shared/channel");
const { quoteIdOf } = await import("../worker/quotes");
const { writeSaved } = await import("../worker/saved");
type Saved = import("../worker/saved").Saved;

/** An assignment, with the state the session stored before, if any. */
type Handed = FundingAssignment & { saved?: string };

/** A session run from what it was handed. */
class Session extends Running {
  constructor(handed: Handed, restarts = 0) {
    super(handed, handed.saved, restarts);
  }
}

/** Cents `.5N` stops the session at kill point N. */
const amountStoppingAt = (point: number) =>
  25n * ONE_CASH + BigInt(50 + point) * (ONE_CASH / 100n);

function assignment(
  direction: "In" | "Out",
  amount: bigint,
  saved?: string,
): Handed {
  return {
    intent: `${direction.toLowerCase()}-session`,
    direction,
    amount,
    expiresAt: 0n,
    ...(saved === undefined ? {} : { saved }),
  };
}

/** Runs a session to its kill point, then again as a restarted worker would,
 *  from the state it stored. */
async function stopAndResume(direction: "In" | "Out", point: number) {
  const amount = amountStoppingAt(point);
  const first = assignment(direction, amount);
  world.storage.set(
    actionKey(first.intent),
    toStored(
      direction === "In"
        ? { kind: "card-paid" }
        : { kind: "payout-confirmed", destination: "DE89370400440532013000" },
    ),
  );
  expect(await new Session(first).run()).toBe("killed");
  expect(
    await new Session(assignment(direction, amount, world.saved)).run(),
  ).toBe("ended");
}

const tags = () => world.reports.map((key) => JSON.parse(key).tag as string);
const distinct = (ids: string[]) => [...new Set(ids)];

test("a top-up that resumes after a stop is credited once, under one id", async () => {
  for (const point of [1, 2, 3, 4]) {
    world = {
      ...world,
      reports: [],
      core: freshCore(),
      checkpoints: [],
      topUps: [],
      drips: 0,
      dripped: false,
      pas: 0n,
      saved: undefined,
      storage: new Map(),
      ticksBeforeDone: point === 2 ? 1 : 0,
    };
    await stopAndResume("In", point);
    expect({ point, tags: tags() }).toEqual({
      point,
      tags: [
        "AwaitingPayment",
        "Details",
        "PaymentReceived",
        "PaymentReceived",
        "Converting",
        "Crediting",
        "Delivered",
      ],
    });
    expect(distinct(world.topUps)).toHaveLength(1);
  }
});

test("a drip asked for before a stop is not asked for again once it landed", async () => {
  await stopAndResume("In", 1);
  expect(world.drips).toBe(1);
});

test("a drip whose answer was lost is asked for again and the session waits for the first", async () => {
  world.dripLands = "on-repeat";
  await stopAndResume("In", 1);
  expect(world.drips).toBe(2);
  expect(tags().at(-1)).toBe("Delivered");
});

test("a top-up repeated after a stop is answered as already made", async () => {
  await stopAndResume("In", 3);
  expect(world.topUps).toHaveLength(2);
  expect(distinct(world.topUps)).toHaveLength(1);
});

test("a withdrawal that resumes after a stop asks the user to pay once", async () => {
  for (const point of [1, 2, 3]) {
    world = {
      ...world,
      reports: [],
      core: freshCore(),
      checkpoints: [],
      requests: [],
      saved: undefined,
      storage: new Map(),
    };
    await stopAndResume("Out", point);
    expect(distinct(world.requests)).toEqual([
      accounts.paymentIdFor("out-session"),
    ]);
    expect({ point, tags: tags() }).toEqual({
      point,
      tags: ["Details", "Collecting", "Converting", "Payout"],
    });
  }
});

test("a payout retried on purpose is reported between the conversion and the outcome", async () => {
  const session = assignment("Out", 25n * ONE_CASH + 63n * (ONE_CASH / 100n));
  world.storage.set(
    actionKey(session.intent),
    toStored({
      kind: "payout-confirmed",
      destination: "DE89370400440532013000",
    }),
  );
  expect(await new Session(session).run()).toBe("ended");
  expect(tags()).toEqual([
    "Details",
    "Collecting",
    "Converting",
    "PayoutRetrying",
    "PayoutRetrying",
    "Payout",
  ]);
});

/** An amount with `cents` after 25 CASH, picking a trigger. */
const withCents = (cents: number) =>
  25n * ONE_CASH + BigInt(cents) * (ONE_CASH / 100n);

const PAYOUT = {
  kind: "payout-confirmed",
  destination: "DE89370400440532013000",
} as const;

const lastUpdate = () =>
  JSON.parse(world.reports.at(-1)!) as { tag: string; value?: unknown };

// A failed payout leaves the user without the CASH they paid until the
// provider refunds it, and the core keeps the session open until it does.
test("a failed payout is refunded from the reserve account and the top-up is named", async () => {
  const session = assignment("Out", withCents(13));
  world.storage.set(actionKey(session.intent), toStored(PAYOUT));
  expect(await new Session(session).run()).toBe("ended");
  const refundId = accounts.refundIdFor(session.intent);
  expect(tags()).toEqual([
    "Details",
    "Collecting",
    "Converting",
    "Payout",
    "Refunding",
  ]);
  expect(lastUpdate().value).toEqual({
    topUpId: refundId,
    amount: withCents(13).toString(),
  });
  expect(world.topUps).toEqual([refundId]);
  expect(world.core.refunded).toBe(true);
});

// Stopping just before any save leaves the actions since the last save made
// but not recorded, the window a crash can hit.
test("a refund resumes from every checkpoint without refunding twice", async () => {
  const session = assignment("Out", withCents(13));
  world.storage.set(actionKey(session.intent), toStored(PAYOUT));
  expect(await new Session(session).run()).toBe("ended");
  const { checkpoints: stops, ...end } = world;
  const refundId = accounts.refundIdFor(session.intent);
  for (const [index, stop] of [...stops, structuredClone(end)].entries()) {
    world = { ...structuredClone(stop), checkpoints: [] };
    const resumed = assignment("Out", withCents(13), stop.saved);
    expect({ index, outcome: await new Session(resumed).run() }).toEqual({
      index,
      outcome: "ended",
    });
    // The core takes `Details` again, so a resume may repeat it; every other
    // update it takes once.
    expect({ index, tags: tags().filter((tag) => tag !== "Details") }).toEqual({
      index,
      tags: ["Collecting", "Converting", "Payout", "Refunding"],
    });
    expect({ index, refunds: distinct(world.topUps) }).toEqual({
      index,
      refunds: [refundId],
    });
    expect(distinct(world.requests)).toHaveLength(1);
  }
});

test("a card payment declined on purpose fails before any money moves", async () => {
  const session = assignment("In", withCents(43));
  world.storage.set(actionKey(session.intent), toStored({ kind: "card-paid" }));
  expect(await new Session(session).run()).toBe("ended");
  expect(tags()).toEqual(["AwaitingPayment", "Details", "Failed"]);
  expect(lastUpdate().value).toEqual({ reason: { tag: "Declined" } });
  expect(world.topUps).toEqual([]);
  expect(world.drips).toBe(0);
});

test("a cancel while the user is on the provider's screen stops the session", async () => {
  world.frame = "stays-open";
  const running = new Session(assignment("In", 25n * ONE_CASH));
  const outcome = running.run();
  await Bun.sleep(20);
  expect(await running.cancel()).toEqual({ tag: "Stopped" });
  expect(await outcome).toBe("ended");
  // The core ends the session on the answer; the worker reports nothing more.
  expect(tags()).toEqual(["AwaitingPayment", "Details"]);
  expect(world.storage.has("funding:in-session")).toBe(false);
});

// A report the host fails to take must not leave a top-up it made unnamed:
// the core could not settle it.
test("a report the host fails to take for a while is made again", async () => {
  world.reportFaults = [
    { tag: "Crediting", answer: "HostFailure" },
    { tag: "Crediting", answer: "HostFailure" },
  ];
  const session = assignment("In", 25n * ONE_CASH);
  world.storage.set(actionKey(session.intent), toStored({ kind: "card-paid" }));
  expect(await new Session(session).run()).toBe("ended");
  expect(tags().at(-2)).toBe("Crediting");
  expect(tags().at(-1)).toBe("Delivered");
});

test("a session a refused report stops is resumed and credits once", async () => {
  world.reportFaults = [{ tag: "Crediting", answer: "NotStarted" }];
  const first = assignment("In", 25n * ONE_CASH);
  world.storage.set(actionKey(first.intent), toStored({ kind: "card-paid" }));
  expect(await new Session(first).run()).toBe("retry");
  expect(tags()).not.toContain("Crediting");
  const resumed = assignment("In", 25n * ONE_CASH, world.saved);
  expect(await new Session(resumed).run()).toBe("ended");
  expect(tags().at(-1)).toBe("Delivered");
  expect(distinct(world.topUps)).toHaveLength(1);
});

// A slow bank payment takes days; a restart must not start that wait again.
test("a session resumed after its payment was due does not wait again", async () => {
  const amount = withCents(33);
  const session: FundingAssignment = {
    ...assignment(
      "In",
      amount,
      writeSaved({
        v: 1,
        amount: amount.toString(),
        inbound: { phase: "fiat-sent", arrivesAt: Date.now() - 1, claims: [] },
      }),
    ),
    quote: {
      quoteId: quoteIdOf(
        { direction: "In", rail: "Bank", asset: "EUR", amount },
        Date.now() + 60_000,
      ),
    } as FundingAssignment["quote"],
  };
  expect(await new Session(session).run()).toBe("ended");
  expect(tags().at(-1)).toBe("Delivered");
});

test("only failures that may clear by themselves are retried", () => {
  const answer = (tag: string) =>
    new host.HostCallError("call", { tag: "Domain", value: { tag } });
  expect(host.isTransient(answer("HostFailure"))).toBe(true);
  expect(host.isTransient(answer("Unknown"))).toBe(true);
  expect(host.isTransient(answer("TopUpNotFound"))).toBe(true);
  expect(host.isTransient(new Error("socket closed"))).toBe(true);
  expect(host.isTransient(answer("NotStarted"))).toBe(false);
  expect(host.isTransient(answer("InvalidAmount"))).toBe(false);
  expect(host.isTransient(new TypeError("x is undefined"))).toBe(false);
});

// The ids are what make a repeat safe, so they must not depend on anything a
// restarted worker would compute differently.
test("payment and top-up ids are fixed by the session and differ between sessions", () => {
  expect(accounts.paymentIdFor("a")).toBe(accounts.paymentIdFor("a"));
  expect(accounts.paymentIdFor("a")).not.toBe(accounts.paymentIdFor("b"));
  const key = new Uint8Array(32).fill(7);
  expect(accounts.topUpIdFor(key, 1)).toBe(accounts.topUpIdFor(key, 1));
  expect(accounts.topUpIdFor(key, 1)).not.toBe(accounts.topUpIdFor(key, 2));
  expect(accounts.topUpIdFor(key, 0)).not.toBe(accounts.topUpIdFor(key, 1));
});

// Cancelling after money moved -----------------------------------------------

/** Waits until `ready` holds, as the worker's answers arrive asynchronously. */
async function until(ready: () => boolean) {
  for (let tries = 0; !ready(); tries++) {
    if (tries > 1_000) throw new Error("waited too long");
    await Bun.sleep(2);
  }
}

/** A withdrawal the core released, its payout due in `payoutInMs`. */
function releasedWithdrawal(
  amount: bigint,
  {
    payoutInMs = 60_000,
    cancel,
    debited = amount,
  }: { payoutInMs?: number; cancel?: Saved["cancel"]; debited?: bigint } = {},
): Handed {
  const paymentId = accounts.paymentIdFor("out-session");
  world.requests = [paymentId];
  world.core = {
    ...freshCore(),
    named: [paymentId],
    released: true,
    lastRank: 4,
    debited,
  };
  // What the session stored before the worker stopped.
  world.saved = writeSaved({
    v: 1,
    amount: amount.toString(),
    ...(cancel === undefined ? {} : { cancel }),
    outbound: {
      phase: "paying-out",
      paymentId,
      debited: debited.toString(),
      payoutAt: Date.now() + payoutInMs,
    },
  });
  return assignment("Out", amount, world.saved);
}

/** The user cancels, the core asks the worker, and takes its answer as the
 *  core's `answer_cancel` does. */
async function userCancels(
  session: InstanceType<typeof Session>,
): Promise<FundingCancelAnswer> {
  world.core.cancel = "requested";
  const answer = await session.cancel();
  if (answer.tag === "Offer") world.core.cancel = "offered";
  if (answer.tag === "Refused") world.core.cancel = "refused";
  return answer;
}

/** The user answers the worker's offer, and the core settles the cancel
 *  with the worker. */
async function userAnswers(
  session: InstanceType<typeof Session>,
  accept: boolean,
) {
  expect(world.core.cancel).toBe("offered");
  world.core.cancel = accept ? "accepted" : "declined";
  if (accept) await session.cancelAccepted();
  else await session.cancelDeclined();
}

/** The terms an answer offers, as the core would show them. */
const offerOf = (answer: FundingCancelAnswer) =>
  answer.tag === "Offer"
    ? {
        fee: answer.value.fee.toString(),
        refund: answer.value.refund.toString(),
      }
    : answer;

const updateOf = (tag: string) =>
  world.reports.map((key) => JSON.parse(key)).find((each) => each.tag === tag)
    ?.value;

// 25 CASH: 1% would be 0.25 CASH, under the 0.50 CASH floor.
const CANCEL_FEE = ONE_CASH / 2n;

test("a cancel after the release is offered for a fee, and an accepted one refunds the rest", async () => {
  const amount = 25n * ONE_CASH;
  const session = new Session(releasedWithdrawal(amount));
  const outcome = session.run();
  await until(() => tags().includes("Converting"));
  const answer = await userCancels(session);
  await userAnswers(session, true);
  expect(await outcome).toBe("ended");
  const refundId = accounts.refundIdFor("out-session");
  expect(tags()).toEqual(["Converting", "Refunding"]);
  expect(offerOf(answer)).toEqual({
    fee: CANCEL_FEE.toString(),
    refund: (amount - CANCEL_FEE).toString(),
  });
  expect(lastUpdate().value).toEqual({
    topUpId: refundId,
    amount: (amount - CANCEL_FEE).toString(),
  });
  expect(world.topUps).toEqual([refundId]);
  expect(world.core.refunded).toBe(true);
});

// A crash between the user's answer and the refund must neither lose the
// refund nor make it twice.
test("an accepted cancel resumes from every checkpoint without refunding twice", async () => {
  const amount = 25n * ONE_CASH;
  const first = new Session(releasedWithdrawal(amount));
  const outcome = first.run();
  await until(() => tags().includes("Converting"));
  await userCancels(first);
  await userAnswers(first, true);
  expect(await outcome).toBe("ended");
  const { checkpoints: stops, ...end } = world;
  const refundId = accounts.refundIdFor("out-session");
  for (const [index, stop] of [...stops, structuredClone(end)].entries()) {
    world = { ...structuredClone(stop), checkpoints: [] };
    const resumed = new Session(assignment("Out", amount, stop.saved));
    const run = resumed.run();
    // The core repeats what the worker has not answered about the cancel.
    if (world.core.cancel === "accepted") await resumed.cancelAccepted();
    else {
      await userCancels(resumed);
      await userAnswers(resumed, true);
    }
    expect({ index, outcome: await run }).toEqual({ index, outcome: "ended" });
    expect({ index, refunds: distinct(world.topUps) }).toEqual({
      index,
      refunds: [refundId],
    });
    expect({
      index,
      refunded: tags().filter((tag) => tag === "Refunding").length,
    }).toEqual({ index, refunded: 1 });
  }
});

test("a declined cancel offer lets the payout go through", async () => {
  const session = new Session(
    releasedWithdrawal(25n * ONE_CASH, { payoutInMs: 30 }),
  );
  const outcome = session.run();
  await until(() => tags().includes("Converting"));
  await userCancels(session);
  await userAnswers(session, false);
  expect(await outcome).toBe("ended");
  expect(tags()).toEqual(["Converting", "Payout"]);
  expect(world.topUps).toEqual([]);
});

// The core decides when an offer lapsed, on its own clock, and tells the
// worker: a worker trusting its own clock could act on an offer the user
// accepted at the last moment.
test("an open cancel offer holds the payout until the core answers, even past its expiry", async () => {
  world.core.cancel = "offered";
  const session = new Session(
    releasedWithdrawal(25n * ONE_CASH, {
      payoutInMs: 0,
      cancel: {
        state: "offered",
        expiresAt: Date.now() - 1,
        fee: CANCEL_FEE.toString(),
        refund: (25n * ONE_CASH - CANCEL_FEE).toString(),
      },
    }),
  );
  world.core.cancel = "offered";
  const outcome = session.run();
  await until(() => tags().includes("Converting"));
  await Bun.sleep(50);
  expect(tags()).not.toContain("Payout");
  world.core.cancel = "declined";
  await session.cancelDeclined();
  expect(await outcome).toBe("ended");
  expect(tags()).toEqual(["Converting", "Payout"]);
});

test("cents .73 refuse a cancel and the payout goes through", async () => {
  const session = new Session(
    releasedWithdrawal(withCents(73), { payoutInMs: 30 }),
  );
  const outcome = session.run();
  await until(() => tags().includes("Converting"));
  const answer = await userCancels(session);
  expect(await outcome).toBe("ended");
  expect(answer.tag).toBe("Refused");
  expect(tags()).toEqual(["Converting", "Payout"]);
});

test("a cancel while the user's payment is still completing is refused", async () => {
  world.paymentStatus = "pending";
  const amount = 25n * ONE_CASH;
  const paymentId = accounts.paymentIdFor("out-session");
  world.requests = [paymentId];
  world.core = { ...freshCore(), named: [paymentId] };
  const session = new Session(
    assignment(
      "Out",
      amount,
      writeSaved({
        v: 1,
        amount: amount.toString(),
        outbound: { phase: "collecting", paymentId },
      }),
    ),
  );
  void session.run();
  expect(await userCancels(session)).toEqual({
    tag: "Refused",
    value: {
      reason: "Your payment is still completing. Try again once it has",
    },
  });
});

test("an inbound cancel after the payment arrived is offered, and an accepted one ends refunded", async () => {
  const amount = 25n * ONE_CASH;
  world.ticksBeforeDone = 1_000;
  world.core = { ...freshCore(), lastRank: 3 };
  const session = new Session(
    assignment(
      "In",
      amount,
      writeSaved({
        v: 1,
        amount: amount.toString(),
        inbound: {
          phase: "converting",
          plan: {
            route: { tier: "pool" },
            depositSymbol: "PAS",
            deposit: DEPOSIT.toString(),
            remoteFeeBuffer: "0",
            keepNativeForFees: "0",
            slippagePct: 1,
          },
          claims: [],
        },
      }),
    ),
  );
  const outcome = session.run();
  expect((await userCancels(session)).tag).toBe("Offer");
  await userAnswers(session, true);
  expect(await outcome).toBe("ended");
  expect(tags()).toEqual(["Failed"]);
  expect(lastUpdate().value).toEqual({ reason: { tag: "Refunded" } });
  expect(world.topUps).toEqual([]);
});

// A payment the host claimed only in part moved less than was asked, and the
// core refuses a refund above what moved: refunding the asked amount would
// leave the refund unnamed and the session open for good.
test("a failed payout of a partly claimed payment refunds what was taken", async () => {
  world.paymentStatus = "partial";
  const session = assignment("Out", withCents(13));
  world.storage.set(actionKey(session.intent), toStored(PAYOUT));
  expect(await new Session(session).run()).toBe("ended");
  expect(lastUpdate()).toEqual({
    tag: "Refunding",
    value: {
      topUpId: accounts.refundIdFor(session.intent),
      amount: PARTIAL.toString(),
    },
  });
  expect(world.core.refunded).toBe(true);
});

test("a cancel of a partly claimed withdrawal offers what was taken, less the fee", async () => {
  const session = new Session(
    releasedWithdrawal(25n * ONE_CASH, { debited: PARTIAL }),
  );
  const outcome = session.run();
  await until(() => tags().includes("Converting"));
  const answer = await userCancels(session);
  await userAnswers(session, true);
  expect(await outcome).toBe("ended");
  expect(offerOf(answer)).toEqual({
    fee: CANCEL_FEE.toString(),
    refund: (PARTIAL - CANCEL_FEE).toString(),
  });
  expect(updateOf("Refunding")).toMatchObject({
    amount: (PARTIAL - CANCEL_FEE).toString(),
  });
  expect(world.core.refunded).toBe(true);
});

const SAVED_PLAN = {
  route: { tier: "pool" as const },
  depositSymbol: "PAS",
  deposit: DEPOSIT.toString(),
  remoteFeeBuffer: "0",
  keepNativeForFees: "0",
  slippagePct: 1,
};

// Once a top-up started the provider cannot undo the credit, and a refund it
// claims instead would be refused while money moves. A late acceptance, with
// no offer open, is ignored.
test("a cancel accepted after the credit started is ignored and the credit completes", async () => {
  const amount = 25n * ONE_CASH;
  const account = (await import("../worker/accounts")).topUpIdFor;
  const claimId = account(
    new TextEncoder().encode("in-session".padEnd(32, ".")).slice(0, 32),
    0,
  );
  world.core = { ...freshCore(), lastRank: 3, cancel: "accepted" };
  const session = new Session(
    assignment(
      "In",
      amount,
      writeSaved({
        v: 1,
        amount: amount.toString(),
        inbound: {
          phase: "crediting",
          plan: SAVED_PLAN,
          claims: [{ id: claimId, amount: amount.toString(), attempt: 0 }],
        },
      }),
    ),
  );
  await session.cancelAccepted();
  expect(await session.run()).toBe("ended");
  expect(tags()).toEqual(["Crediting", "Delivered"]);
  expect(tags()).not.toContain("Failed");
});

// The core asks again about a cancel it has no answer for after a restart:
// the worker must make the same offer again, not withdraw it.
test("a cancel asked again while its offer is open makes the same offer", async () => {
  const amount = 25n * ONE_CASH;
  world.ticksBeforeDone = 1_000;
  world.core = { ...freshCore(), lastRank: 3, cancel: "offered" };
  const offer = {
    state: "offered" as const,
    expiresAt: Date.now() + 60_000,
    fee: CANCEL_FEE.toString(),
    refund: (amount - CANCEL_FEE).toString(),
  };
  const session = new Session(
    assignment(
      "In",
      amount,
      writeSaved({
        v: 1,
        amount: amount.toString(),
        cancel: offer,
        inbound: { phase: "crediting", plan: SAVED_PLAN, claims: [] },
      }),
    ),
  );
  const outcome = session.run();
  expect(await session.cancel()).toEqual({
    tag: "Offer",
    value: {
      fee: BigInt(offer.fee),
      refund: BigInt(offer.refund),
      expiresAt: BigInt(offer.expiresAt),
    },
  });
  // Nothing was credited while the offer stood.
  expect(world.topUps).toEqual([]);
  world.core.cancel = "declined";
  await session.cancelDeclined();
  expect(await outcome).toBe("ended");
  expect(tags().at(-1)).toBe("Delivered");
});

// The deposit already reached the provider, so a cancel is a request; this
// provider cannot send crypto back and refuses it.
test("a crypto session whose deposit was seen refuses a cancel after a restart", async () => {
  const amount = 25n * ONE_CASH;
  process.env.FUNDTEST_DEMO_CRYPTO = "0";
  world.core = { ...freshCore(), lastRank: 0 };
  const session = new Session({
    ...assignment(
      "In",
      amount,
      writeSaved({
        v: 1,
        amount: amount.toString(),
        inbound: {
          phase: "awaiting-user",
          paymentSeen: true,
          plan: SAVED_PLAN,
          depositExpiresAt: Date.now() + 60_000,
          claims: [],
        },
      }),
    ),
    quote: {
      quoteId: quoteIdOf(
        { direction: "In", rail: "Crypto", asset: "PAS", amount },
        Date.now() + 60_000,
      ),
    } as FundingAssignment["quote"],
  });
  void session.run();
  expect(await userCancels(session)).toEqual({
    tag: "Refused",
    value: { reason: "This provider cannot send a crypto deposit back" },
  });
  expect(tags()).not.toContain("Failed");
});

test("a session an error keeps stopping gives up once no money moved for it", async () => {
  world.planFails = true;
  const session = assignment("In", 25n * ONE_CASH);
  world.storage.set(actionKey(session.intent), toStored({ kind: "card-paid" }));
  expect(await new Session(session).run()).toBe("retry");
  world.reports = [];
  world.core = freshCore();
  expect(await new Session(session, MAX_RESTARTS).run()).toBe("ended");
  expect(lastUpdate()).toMatchObject({
    tag: "Failed",
    value: { reason: { tag: "Other", value: { code: "provider_stuck" } } },
  });
});
