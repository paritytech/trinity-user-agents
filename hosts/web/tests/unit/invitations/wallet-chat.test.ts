import { afterAll, beforeAll, describe, expect, test } from "bun:test";
import { bytesToHex } from "@parity/truapi/scale";
import type { HostDelivery } from "../../../src/invitations/ports.js";
import type { StatementAccountClaim } from "@parity/truapi-host/web";
import { asProductId } from "@parity/truapi";
import {
  NOVA_NEEDS_SETTLEMENT,
  WalletChat,
  novaNotificationSettled,
  type WalletChatDeps,
  type WalletRequestsSession,
} from "../../../src/invitations/wallet-chat.js";
import { novaChat } from "../../../tools/chat/nova.js";
import {
  networkCosts,
  networkDelivery,
  type WalletDelivery,
} from "../../../src/invitations/wallet-delivery.js";
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

/** A finality guard held until the test releases it, with the claims it was asked about. */
function heldGuard() {
  const asked: StatementAccountClaim[] = [];
  let release!: () => void;
  const gate = new Promise<void>((resolve) => (release = resolve));
  return {
    asked,
    release,
    guard: (claim: StatementAccountClaim) => {
      asked.push(claim);
      return gate;
    },
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

describe("a wallet's chat products' finality guard", () => {
  // A node evicts statements whose allowance is only in a best block, so an
  // account that posts before its claim is final loses what it posted.
  test("holds a conversation account until its guard resolves, and Contact Chat's claim answer alone is enough", async () => {
    const claim: StatementAccountClaim = {
      account: "0xaccount",
      period: 1,
      outcome: "registered",
      seq: 3,
      collection: "People",
    };
    const claimed: boolean[] = [];
    const answering: WalletRequestsSession = {
      ...session(),
      claimProductStatementAccount: async (_product, _index, submit) => {
        claimed.push(submit);
        return claim;
      },
    };
    const network = networkDelivery({ log: () => {} });
    const held = heldGuard();
    const allowed = network
      .allowance(answering, held.guard)!
      .allow(asProductId("chat.paseo"), new Uint8Array(4));
    expect(await settledYet(allowed)).toBe(false);
    expect({ claimed, asked: held.asked }).toEqual({
      claimed: [true],
      asked: [claim],
    });
    held.release();
    expect((await allowed).account).toBe("0xaccount");

    const unguarded = network
      .allowance(answering, null)!
      .allow(asProductId("chat.paseo"), new Uint8Array(4));
    expect(await settledYet(unguarded)).toBe(true);
  });

  test("holds a Nova notification account until the spec's guard resolves, and refuses without one", async () => {
    const held = heldGuard();
    const account = new Uint8Array(32).fill(7);
    const waiting = novaNotificationSettled(held.guard)(account, {
      period: 2,
      seq: 5,
    });
    expect(await settledYet(waiting)).toBe(false);
    expect(held.asked).toEqual([
      {
        account: bytesToHex(account),
        period: 2,
        outcome: "registered",
        seq: 5,
        collection: "People",
      },
    ]);
    held.release();
    await waiting;

    await expect(
      novaNotificationSettled(null)(account, { period: 2, seq: 5 }),
    ).rejects.toThrow(NOVA_NEEDS_SETTLEMENT);
  });

  test("keeps Nova Chat off, claiming nothing, for a spec with no guard", async () => {
    const claims: string[] = [];
    const { chat, log } = walletChat(undefined, [
      { ...novaChat(async () => {}), allowanceSettled: null },
    ]);
    chat.setWallet({
      ...session(),
      claimNotificationAccount: (target) => {
        claims.push(bytesToHex(target));
        return new Promise(() => {});
      },
      claimProductStatementAccount: (product) => {
        claims.push(product);
        return new Promise(() => {});
      },
    });
    const off = `Nova Chat stays off: ${NOVA_NEEDS_SETTLEMENT}`;
    for (let wait = 0; wait < 50 && !log.includes(off); wait += 1)
      await new Promise((resolve) => setTimeout(resolve, 10));
    expect({ off: log.includes(off), claims }).toEqual({
      off: true,
      claims: [],
    });
    chat.setWallet(null);
  });
});
