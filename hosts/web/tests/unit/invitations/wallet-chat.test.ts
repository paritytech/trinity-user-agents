import { afterAll, beforeAll, describe, expect, test } from "bun:test";
import { bytesToHex } from "@parity/truapi/scale";
import type { HostDelivery } from "../../../src/invitations/ports.js";
import type { StatementAccountClaim } from "@parity/truapi-host/web";
import { asProductId } from "@parity/truapi";
import {
  WalletChat,
  type WalletChatDeps,
  type WalletRequestsSession,
} from "../../../src/invitations/wallet-chat.js";
import {
  conversationAllowance,
  networkCosts,
  networkDelivery,
  type WalletDelivery,
} from "../../../src/invitations/wallet-delivery.js";
import type { ClaimedAccount } from "../../../src/platform/allowance-finality.js";
import {
  PROVISIONAL_MS,
  type AllowanceGuard,
} from "../../../src/platform/allowance-usable.js";
import { finalizedGuard, usable } from "../../support/allowance.js";
import { devWalletDelivery } from "../../../tools/demos/wallet-deliveries.js";
import { NotificationStore } from "../../../src/notifications/notifications.js";
import {
  TEST_USERS,
  testUserEntropy,
  testUserKeys,
} from "../../../fixtures/invitations/test-users.js";
import { memoryDirectory } from "../../support/statement-memory.js";
import { MemoryStorage } from "../../support/test-support.js";

/**
 * Contact Chat for the signed-in wallet: over the delivery the page hands it,
 * the network's in a build, off only when no chat product runs, and with
 * prompts that say what it costs and that its messages are readable.
 */

const saved = {
  window: (globalThis as { window?: unknown }).window,
  localStorage: (globalThis as { localStorage?: unknown }).localStorage,
};
beforeAll(() => {
  Object.assign(globalThis, {
    window: new EventTarget(),
    localStorage: new MemoryStorage(),
  });
});
afterAll(() => {
  Object.assign(globalThis, saved);
});

/** A delivery that carries nothing. */
const quietDelivery: HostDelivery = {
  listen: () => {},
  returnRoute: () => "route",
  isReturnRoute: () => true,
  postRequest: () => {},
  postReply: () => {},
  watch: () => {},
  unwatch: () => {},
  withdraw: () => {},
  close: () => {},
};

function walletChat(
  delivery?: WalletDelivery,
  products?: WalletChatDeps["products"],
) {
  const log: string[] = [];
  const deps: WalletChatDeps = {
    base: "/",
    notifications: new NotificationStore(new MemoryStorage()),
    handles: {
      accountFor: () => null,
      handleFor: () => null,
      onChange: () => () => {},
    },
    blocked: () => [],
    names: { name: () => null, onChange: () => () => {} },
    namedChanged: () => {},
    showChat: () => {},
    log: (line) => log.push(line),
    delivery: delivery ?? networkDelivery({ log: (line) => log.push(line) }),
    ...(products === undefined ? {} : { products }),
  };
  return { chat: new WalletChat(deps), log };
}

/** A wallet session whose keys are the ones the core reports, on a chain that publishes none. */
function session(): WalletRequestsSession {
  const user = TEST_USERS[0]!;
  const keys = testUserKeys(user, "paseo");
  const never = () => new Promise<never>(() => {});
  return {
    walletId: "wallet",
    entropy: testUserEntropy(user),
    network: "paseo",
    networkSuffix: "paseo",
    identityAccountId: bytesToHex(keys.identity),
    chatPublicKey: bytesToHex(keys.chatPublic),
    directory: memoryDirectory([]),
    openStatementStore: never,
    claimNotificationAccount: never,
    claimProductStatementAccount: never,
    usableAllowance: finalizedGuard,
  };
}

describe("a wallet's Contact Chat", () => {
  test("keeps saved contacts out of the picker while a chat product runs over a delivery that says so", () => {
    const dev = (mode: "validate" | "local" | "local-validate") =>
      devWalletDelivery(mode, { log: () => {}, local: () => quietDelivery });
    const hides = (delivery?: WalletDelivery, products?: []) =>
      walletChat(delivery, products).chat.hidesWalletContacts();
    expect([
      hides(),
      hides(dev("validate")),
      hides(dev("local")),
      hides(dev("local-validate")),
      // No chat product runs: no request reaches anyone.
      hides(undefined, []),
    ]).toEqual([true, true, false, true, false]);
  });

  test("runs for a signed-in wallet with nothing turned on, and not where the dev server's tools turned it off", () => {
    const running = walletChat();
    running.chat.setWallet(session());
    const off = walletChat(undefined, []);
    off.chat.setWallet(session());
    expect({
      running: running.chat.owner.workerNames(),
      logged: running.log.includes(
        "Host requests for Contact Chat: requests and replies go on the network's day inbox, and each post claims a notification slot.",
      ),
      off: off.chat.owner.workerNames(),
    }).toEqual({ running: ["contact-chat.paseo"], logged: true, off: [] });
    running.chat.setWallet(null);
    expect(running.chat.owner.workerNames()).toEqual([]);
  });

  // The prompts are the only place a user learns what a request spends and who reads a chat.
  test("on the network, says what sending and replying spend, that a waiting request is posted again, and that messages are readable", () => {
    const costs = networkCosts(false);
    expect(costs.summary).toBe("Uses daily slots · Messages are public");
    expect(costs.request).toContain("1 more each day it is pending");
    expect(costs.reply).toContain("their request arrives again");
    expect(costs.chat).toContain("Statement Store slot each day you send");
    expect(costs.messages).toBe("Not encrypted: anyone can read them.");
    expect(networkDelivery({ log: () => {} }).costs(false)).toEqual(costs);
    // An encrypting chat product's prompts say so, and what stays public.
    const kettle = networkCosts(true);
    expect([kettle.summary, kettle.messages]).toEqual([
      "Uses daily slots · Messages are end-to-end encrypted",
      "End-to-end encrypted. Who posts, to which topics, when and how much is public.",
    ]);
  });
});

/** A guard held until the test answers, with the claims it was asked about. */
function heldGuard() {
  const asked: ClaimedAccount[] = [];
  const waits: ("finalized" | undefined)[] = [];
  let answer!: (usableAt: "best" | "finalized") => void;
  const gate = new Promise<"best" | "finalized">(
    (resolve) => (answer = resolve),
  );
  return {
    asked,
    waits,
    answer,
    guard: (async (claim, request) => {
      asked.push(claim);
      waits.push(request?.wait);
      return usable(claim, await gate);
    }) satisfies AllowanceGuard,
  };
}

/** Whether `promise` has settled once every pending task has run. */
async function settledYet(promise: Promise<unknown>): Promise<boolean> {
  let settled = false;
  void promise.then(
    () => (settled = true),
    () => (settled = true),
  );
  await new Promise((resolve) => setTimeout(resolve, 0));
  return settled;
}

describe("a wallet's conversation accounts", () => {
  const claim: StatementAccountClaim = {
    account: "0xaccount",
    period: 1,
    outcome: "registered",
    seq: 3,
    collection: "People",
    blockHash: "0xb10c",
  };
  const DAY_END = 2 * 86_400_000;

  /** The answer of a conversation allowance whose guard answers `usableAt`, at `now`. */
  async function allowed(
    usableAt: "best" | "finalized",
    now: number,
    known = true,
  ) {
    const held = heldGuard();
    const seen: string[] = [];
    const claimed: boolean[] = [];
    const answering: WalletRequestsSession = {
      ...session(),
      claimProductStatementAccount: async (_product, _index, submit) => {
        claimed.push(submit);
        return known ? claim : { ...claim, blockHash: undefined };
      },
      usableAllowance: held.guard,
    };
    const allowing = conversationAllowance(
      answering,
      true,
      (usable) => void seen.push(usable.usableAt),
      () => now,
    ).allow(asProductId("chat.paseo"), new Uint8Array(4));
    const heldBack = !(await settledYet(allowing));
    held.answer(usableAt);
    const answer = await allowing;
    expect(held.waits).toEqual(["finalized"]);
    return { heldBack, claimed, asked: held.asked, seen, answer };
  }

  // A conversation account posts only once the network's rule lets it.
  test("waits for the session's guard before it answers, and tells the spec where the account became usable", async () => {
    expect(await allowed("finalized", 86_400_000)).toEqual({
      heldBack: true,
      claimed: [true],
      asked: [claim],
      seen: ["finalized"],
      answer: { account: "0xaccount", until: DAY_END },
    });
  });

  // Usable at a best block is not final: whoever keeps the answer asks again
  // once the claim's finality is known, rather than at the end of the day.
  test("finalized allowance without a known final inclusion hash never gets a full-day lease", async () => {
    const now = 86_400_000 + 3_600_000;
    expect((await allowed("finalized", now, false)).answer).toEqual({
      account: "0xaccount",
      until: now + PROVISIONAL_MS,
    });
    expect(
      (await allowed("finalized", DAY_END - 1_000, false)).answer.until,
    ).toBe(DAY_END);
  });
});

describe("a wallet's allowance waits", () => {
  // A wait, and the check behind a post, must not outlive the wallet that
  // claimed: the next wallet's session is not theirs to read for.
  test("stop when the wallet signs out", async () => {
    let scoped: WalletRequestsSession | null = null;
    const delivery: WalletDelivery = {
      ...networkDelivery({ log: () => {} }),
      hostDelivery: () => quietDelivery,
      allowance: (session) => {
        scoped = session;
        return undefined;
      },
    };
    const signals: (AbortSignal | undefined)[] = [];
    const { chat } = walletChat(delivery);
    chat.setWallet({
      ...session(),
      usableAllowance: async (claim, request) => {
        signals.push(request?.signal);
        return usable(claim);
      },
    });
    await scoped!.usableAllowance({ account: "0xaccount", period: 1, seq: 0 });
    const signal = signals[0]!;
    expect(signal.aborted).toBe(false);
    const deadline = new AbortController();
    await scoped!.usableAllowance(
      { account: "0xaccount", period: 1, seq: 0 },
      { signal: deadline.signal },
    );
    expect(signals[1]!.aborted).toBe(false);
    deadline.abort();
    expect(signals[1]!.aborted).toBe(true);
    expect(signal.aborted).toBe(false);
    const another = new AbortController();
    await scoped!.usableAllowance(
      { account: "0xaccount", period: 1, seq: 0 },
      { signal: another.signal },
    );
    chat.setWallet(null);
    expect(signal.aborted).toBe(true);
    expect(signals[2]!.aborted).toBe(true);
  });
});
