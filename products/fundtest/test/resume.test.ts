// A session that stops anywhere must finish when a restarted worker is handed
// it again with what it saved, and must not move funds twice on the way. Each
// case stops a session at one of its kill points, then runs it again from the
// host's copy of its saved state, against a fake host and chain that answer a
// repeated action the way the real ones do.

import { beforeEach, expect, mock, test } from "bun:test";
import type { FundingAssignment, FundingUpdate } from "@parity/truapi";

process.env.FUNDTEST_CARD_ARRIVAL_MS = "0";
process.env.FUNDTEST_PAYOUT_MS = "0";

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
const stream = (items: unknown[]) => ({
  subscribe(observer: { next: (item: unknown) => void }) {
    for (const item of items) observer.next(item);
    return { unsubscribe() {} };
  },
});

/** What the fake host, chain and faucet saw. */
let world: {
  reports: string[];
  saved: string | undefined;
  storage: Map<string, string>;
  topUps: string[];
  requests: string[];
  drips: number;
  dripLands: "at-once" | "on-repeat";
  dripped: boolean;
  pas: bigint;
  ticksBeforeDone: number;
};

beforeEach(() => {
  world = {
    reports: [],
    saved: undefined,
    storage: new Map(),
    topUps: [],
    requests: [],
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

const client = {
  fundingProvider: {
    report: ({ update }: { update: FundingUpdate }) => {
      const key = reportKey(update);
      if (world.reports.includes(key)) return refused("OutOfOrder");
      world.reports.push(key);
      return ok();
    },
    save: ({ state }: { state: string }) => {
      world.saved = state;
      return ok();
    },
    presentFrame: () => ok({ outcome: { tag: "Closed" } }),
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
  // The fake host refuses a report repeated word for word, as the real one
  // does not, so the second retry shows only as the first.
  expect(tags()).toEqual([
    "Details",
    "Collecting",
    "Converting",
    "PayoutRetrying",
    "Payout",
  ]);
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
