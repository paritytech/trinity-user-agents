import { afterAll, beforeAll, describe, expect, test } from "bun:test";
import { bytesToHex } from "@parity/truapi/scale";
import { DirectoryContacts } from "../../../../src/invitations/directory-contacts.js";
import type { HostDelivery } from "../../../../src/invitations/ports.js";
import type { IdentityProblem } from "../../../../src/invitations/published-keys.js";
import { WalletIdentityAuth } from "../../../../src/invitations/wallet-auth.js";
import { NotificationStore } from "../../../../src/notifications/notifications.js";
import {
  VERIFY_RETRY_MS,
  startContactChatHost,
} from "../../../../tools/demos/contact-chat-host.js";
import {
  TEST_USERS,
  testUserKeys,
} from "../../../../fixtures/invitations/test-users.js";
import { memoryDirectory } from "../../../support/statement-memory.js";
import { MemoryStorage } from "../../../support/test-support.js";

/**
 * Contact Chat's host side for a wallet, from its identity check to a chat
 * the user may start. Only the page globals it touches are stood in for.
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

/** A delivery that carries nothing: the identity check is all that is tested. */
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

/** A host whose identity checks answer `answers`, in order, and its timers, run by hand. */
function host(
  answers: (IdentityProblem | null)[],
  delivery: HostDelivery = quietDelivery,
) {
  const keys = testUserKeys(TEST_USERS[0]!, "paseo");
  const timers: { delayMs: number; task: () => void }[] = [];
  const log: string[] = [];
  let checks = 0;
  const chat = startContactChatHost({
    product: { id: "contact-chat.paseo", name: "Contact Chat" },
    keys: { sent: "sent", received: "received" },
    network: "paseo",
    self: bytesToHex(keys.chatPublic),
    auth: new WalletIdentityAuth(keys),
    delivery,
    contacts: new DirectoryContacts({
      handles: {
        accountFor: () => null,
        handleFor: () => null,
        onChange: () => () => {},
      },
      directory: memoryDirectory([]),
      person: () => undefined,
    }),
    handles: {
      accountFor: () => null,
      handleFor: () => null,
      onChange: () => () => {},
    },
    notifications: new NotificationStore(new MemoryStorage(), Date.now),
    verify: async () => answers[checks++] ?? null,
    confirmSend: async () => false,
    showChat: () => {},
    schedule: (task, delayMs) => {
      const timer = { delayMs, task };
      timers.push(timer);
      return () => timers.splice(timers.indexOf(timer), 1);
    },
    log: (line) => log.push(line),
  });
  const start = chat.chatOptions.start!;
  let redraws = 0;
  start.onChange?.(() => (redraws += 1));
  return {
    chat,
    blocked: () => start.blocked(),
    redraws: () => redraws,
    timers,
    log,
    checks: () => checks,
  };
}

const unread: IdentityProblem = {
  reason:
    "Your identity account: its chat identity could not be read: Error: no answer from the People chain after 45s.",
  again: true,
};

describe("Contact Chat's identity check for a wallet", () => {
  // New chat is drawn while the check runs; it has to open once the check
  // passes, without the user leaving the screen.
  test("tells the Chat screen to draw New chat again once the check passes", async () => {
    const tab = host([null]);
    const before = tab.blocked();
    await tab.chat.start();

    expect([before, tab.blocked(), tab.redraws(), tab.log]).toEqual([
      "Contact Chat is checking your identity on chain.",
      null,
      1,
      ["Contact Chat's requests are on."],
    ]);
    tab.chat.stop();
  });

  // A chain that could not be read must not leave requests off until a
  // reload: the host says why, waits and checks again.
  test("waits and checks again when the chain could not be read, then opens", async () => {
    const tab = host([unread, null]);
    await tab.chat.start();
    const waiting = [tab.blocked(), tab.timers.map((timer) => timer.delayMs)];
    tab.timers.shift()!.task();
    await Promise.resolve();
    await Promise.resolve();

    expect({
      waiting,
      after: tab.blocked(),
      checks: tab.checks(),
      redraws: tab.redraws(),
      timers: tab.timers.length,
    }).toEqual({
      waiting: [
        `Contact Chat could not check your identity yet, and tries again shortly: ${unread.reason}`,
        [VERIFY_RETRY_MS],
      ],
      after: null,
      checks: 2,
      redraws: 2,
      timers: 0,
    });
    tab.chat.stop();
  });

  test("stays off, and says why, when the chain answers with another key", async () => {
    const tab = host([
      {
        reason:
          "The chat key published for your identity account is not this wallet's.",
        again: false,
      },
    ]);
    await tab.chat.start();

    expect([tab.blocked(), tab.timers.length, tab.redraws()]).toEqual([
      "Contact Chat's requests are off: The chat key published for your identity account is not this wallet's.",
      0,
      1,
    ]);
    tab.chat.stop();
  });

  // Sign-out and a change of wallet stop it: nothing it started may still
  // listen or post for the wallet that left.
  test("stopping it closes its delivery, once", async () => {
    let closed = 0;
    const tab = host([null], {
      ...quietDelivery,
      close: () => (closed += 1),
    });
    await tab.chat.start();
    tab.chat.stop();
    tab.chat.stop();

    expect(closed).toBe(1);
  });

  test("checks no more once it is stopped", async () => {
    const tab = host([unread, null]);
    await tab.chat.start();
    tab.chat.stop();

    expect([tab.timers.length, tab.checks()]).toEqual([0, 1]);
  });
});
