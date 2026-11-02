import { afterAll, beforeAll, describe, expect, test } from "bun:test";
import { bytesToHex } from "@parity/truapi/scale";
import type { HostDelivery } from "../../../src/invitations/ports.js";
import {
  WalletChat,
  chatCosts,
  type WalletChatDeps,
  type WalletDelivery,
  type WalletRequestsSession,
} from "../../../src/invitations/wallet-chat.js";
import { NotificationStore } from "../../../src/notifications/notifications.js";
import {
  TEST_USERS,
  testUserEntropy,
  testUserKeys,
} from "../../../fixtures/invitations/test-users.js";
import { memoryDirectory } from "../../support/statement-memory.js";
import { MemoryStorage } from "../../support/test-support.js";

/**
 * Contact Chat for the signed-in wallet: on the network with nothing turned
 * on, in another delivery or off only when the dev server's tools say so, and
 * with prompts that say what it costs and that its messages are readable.
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

function walletChat(mode?: WalletDelivery | "off") {
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
    ...(mode === undefined
      ? {}
      : { devDelivery: { mode: () => mode, local: () => quietDelivery } }),
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
  test("uses the network unless the dev server's tools chose otherwise, and then keeps saved contacts out", () => {
    const of = (mode?: WalletDelivery | "off") => {
      const { chat } = walletChat(mode);
      return [chat.mode(), chat.hidesWalletContacts()];
    };
    expect([of(), of("validate"), of("local"), of("off")]).toEqual([
      ["network", true],
      ["validate", true],
      ["local", false],
      ["off", false],
    ]);
  });

  test("runs for a signed-in wallet with nothing turned on, and not where the dev server's tools turned it off", () => {
    const running = walletChat();
    running.chat.setWallet(session());
    const off = walletChat("off");
    off.chat.setWallet(session());
    expect({
      running: running.chat.owner.workerNames(),
      logged: running.log.includes(
        "Contact Chat's requests go on the network's day inbox: each post claims a notification slot.",
      ),
      off: off.chat.owner.workerNames(),
    }).toEqual({ running: ["contact-chat.paseo"], logged: true, off: [] });
    running.chat.setWallet(null);
    expect(running.chat.owner.workerNames()).toEqual([]);
  });

  // The prompts are the only place a user learns what a request spends and who reads a chat.
  test("on the network, says what sending and replying spend, that a waiting request is posted again, and that messages are readable", () => {
    const costs = chatCosts("network")!;
    expect(costs.request).toContain("posts it again on each later day");
    expect(costs.reply).toContain("posted again");
    expect(costs.messages).toBe(
      "Not encrypted. Anyone who reads the network's Statement Store can read them.",
    );
    expect([
      chatCosts("validate")?.messages,
      chatCosts("local-validate")?.messages,
      chatCosts("local"),
    ]).toEqual([null, null, null]);
  });
});
