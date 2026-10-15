// A session that stops anywhere must finish when a restarted worker is handed
// it again with what it saved, and must not move funds twice on the way. Each
// case stops a session at one of its kill points, then runs it again from the
// host's copy of its saved state, against a fake host and chain that answer a
// repeated action the way the real ones do. The fake host orders reports as
// the core does (host_logic/funding.rs `report` and `follows`).

import { beforeEach, expect, mock, test } from "bun:test";
import type { FundingAssignment, FundingUpdate } from "@parity/truapi";

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
}

/** What the fake host, chain and faucet saw. */
interface World {
  /** Every update the host took, in order. */
  reports: string[];
  core: Core;
  saved: string | undefined;
  /** The host's state just before each save, to resume from as if the worker
   *  stopped there. */
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
}

const freshCore = (): Core => ({
  lastRank: -1,
  named: [],
  failed: false,
  released: false,
  converting: false,
  refunded: false,
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
  };
});

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
  if (update.tag === "Refunding") {
    if (core.payout !== "failed" || core.refunded) return "OutOfOrder";
    if (!world.topUps.includes(update.value.topUpId)) return "NotStarted";
    core.refunded = true;
    return undefined;
  }
  if (core.payout || (update.tag === "Converting" && core.converting))
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
  const rank = rankOf(update);
  const follows =
    update.tag === "Details"
      ? true
      : update.tag === "Failed" || update.tag === "Deposit"
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
    save: ({ state }: { state: string }) => {
      const { checkpoints: _, ...now } = world;
      world.checkpoints.push(structuredClone(now));
      world.saved = state;
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
    statusSubscribe: () => stream([{ tag: "Completed" }]),
  },
  localStorage: {
    read: ({ key }: { key: string }) => ok({ value: world.storage.get(key) }),
    write: ({ key, value }: { key: string; value: string }) => {
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
  planConversion: async () => ({
    route: { tier: "pool" },
    depositSymbol: "PAS",
    deposit: DEPOSIT.toString(),
    remoteFeeBuffer: "0",
    keepNativeForFees: "0",
    slippagePct: 1,
  }),
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

const { Session } = await import("../worker/session");
const { actionKey, toStored } = await import("../shared/channel");
const { quoteIdOf } = await import("../worker/quotes");
const { writeSaved } = await import("../worker/saved");

/** Cents `.5N` stops the session at kill point N. */
const amountStoppingAt = (point: number) =>
  25n * ONE_CASH + BigInt(50 + point) * (ONE_CASH / 100n);

function assignment(
  direction: "In" | "Out",
  amount: bigint,
  saved?: string,
): FundingAssignment {
  return {
    intent: `${direction.toLowerCase()}-session`,
    direction,
    amount,
    expiresAt: 0n,
    ...(saved === undefined ? {} : { saved: saved as `0x${string}` }),
  };
}

/** Runs a session to its kill point, then again as a restarted worker would,
 *  handed back what the host kept for it. */
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

test("a cancel while the user is on the provider's screen ends the session as cancelled", async () => {
  world.frame = "stays-open";
  const running = new Session(assignment("In", 25n * ONE_CASH));
  const outcome = running.run();
  await Bun.sleep(20);
  running.cancel();
  expect(await outcome).toBe("ended");
  expect(tags()).toEqual(["AwaitingPayment", "Details", "Failed"]);
  expect(lastUpdate().value).toEqual({ reason: { tag: "Cancelled" } });
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
