import { afterAll, beforeAll, describe, expect, test } from "bun:test";
import * as sr25519 from "@scure/sr25519";
import { bytesToHex, hexToBytes, type HexString } from "@parity/truapi/scale";
import {
  blake2b256,
  keyedBlake2b256,
} from "../../../../src/platform/crypto.js";
import {
  decodeStatement,
  encodeSigned,
} from "../../../../src/platform/statement.js";
import { utf8 } from "../../../../src/platform/codec.js";
import { NovaBoundary } from "../../../../src/chat/nova-chat/boundary.js";
import {
  NovaSession,
  novaHostOver,
  type NovaStatements,
} from "../../../../src/chat/nova-chat/session.js";
import { encodeMessage } from "../../../../src/chat/nova-chat/wire.js";
import type { HandleNames } from "../../../../src/contacts/contacts.js";
import { startNovaChatHost } from "../../../../src/invitations/nova-chat-host.js";
import type { ContactChatHost } from "../../../../src/invitations/contact-chat-host.js";
import { asProductId } from "@parity/truapi";
import { NotificationStore } from "../../../../src/notifications/notifications.js";
import { MemoryStatementStore } from "../../../support/statement-memory.js";
import { MemoryStorage } from "../../../support/test-support.js";
import { A, B, chatKeys, hex } from "./fixtures.js";

/**
 * Nova Chat's host side (`src/invitations/nova-chat-host.ts`) and worker
 * session (`src/chat/nova-chat/session.ts`) for two users over one in-memory
 * Statement Store, joined by real `MessagePort`s: a request, the Inbox and
 * Accept, text both ways with acknowledgements, a restart, and a blocked
 * person. Both ends are this code, so this shows the parts work together,
 * not that a Nova app reads them.
 */

const saved = {
  localStorage: (globalThis as { localStorage?: unknown }).localStorage,
};
beforeAll(() =>
  Object.assign(globalThis, { localStorage: new MemoryStorage() }),
);
afterAll(() => Object.assign(globalThis, saved));

const product = { id: asProductId("nova-chat.test"), name: "Nova Chat" };
const ids = {
  A: hex(sr25519.getPublicKey(A.identitySecret)),
  B: hex(sr25519.getPublicKey(B.identitySecret)),
};
/** Each user's handle of the other: what the core would mint. */
const handleOf = (viewer: string, account: string) =>
  bytesToHex(
    keyedBlake2b256(blake2b256(utf8(viewer)), hexToBytes(account as HexString)),
  ).toLowerCase();

function handlesFor(viewer: string): HandleNames {
  const people = [ids.A, ids.B];
  return {
    accountFor: (handle) =>
      (people.find(
        (account) => handleOf(viewer, account) === handle.toLowerCase(),
      ) as HexString | undefined) ?? null,
    handleFor: (account) =>
      handleOf(viewer, account.toLowerCase()) as HexString,
    onChange: () => () => {},
  };
}

const accountAt = (index: string) =>
  sr25519.secretFromSeed(blake2b256(hexToBytes(index as HexString)));

function statementsOver(store: MemoryStatementStore): NovaStatements {
  return {
    follow(topics, deliver) {
      let stop = () => {};
      void store
        .connect()
        .subscribe(
          { matchAny: topics.map((topic) => hexToBytes(topic as HexString)) },
          (encoded) =>
            deliver(
              encoded.map((bytes) => {
                const statement = decodeStatement(bytes);
                return {
                  expiry: statement.expiry.toString(),
                  channel:
                    statement.channel === null
                      ? null
                      : bytesToHex(statement.channel),
                  topics: statement.topics.map((topic) => bytesToHex(topic)),
                  data: bytesToHex(statement.data),
                  signer: bytesToHex(statement.signer),
                  signature: bytesToHex(statement.signature),
                };
              }),
            ),
        )
        .then((unsubscribe) => (stop = unsubscribe));
      return () => stop();
    },
    async post(index, wire) {
      const secret = accountAt(index);
      store.inject(
        encodeSigned(
          {
            expiry: BigInt(wire.expiry),
            channel:
              wire.channel === null
                ? null
                : hexToBytes(wire.channel as HexString),
            topics: wire.topics.map((topic) => hexToBytes(topic as HexString)),
            data: hexToBytes(wire.data as HexString),
          },
          sr25519.getPublicKey(secret),
          (payload) => sr25519.sign(secret, payload),
        ),
      );
    },
  };
}

interface User {
  name: "A" | "B";
  host: ContactChatHost;
  session: NovaSession;
  notifications: NotificationStore;
  blocked: string[];
  productStorage: MemoryStorage;
}

let counter = 0;
function startUser(
  name: "A" | "B",
  store: MemoryStatementStore,
  kept?: {
    notifications: NotificationStore;
    productStorage: MemoryStorage;
    blocked: string[];
  },
): User {
  const own = ids[name];
  const notifications =
    kept?.notifications ?? new NotificationStore(new MemoryStorage());
  notifications.setScope(`notifications:${name}`);
  const blocked = kept?.blocked ?? [];
  const secrets = name === "A" ? A : B;
  const host = startNovaChatHost({
    product,
    storageKey: `nova-chat:${name}`,
    boundary: new NovaBoundary(secrets, chatKeys()),
    handles: handlesFor(own),
    nameOf: (account) => ({
      name: account === ids.A ? "Alice" : "Bob",
      handle: account,
    }),
    lookupName: async () => null,
    blocked: () => blocked,
    notifications,
    verify: async () => null,
    openStore: async () => store.connect(),
    notificationSigner: (kind, requestId, period) => {
      const secret = sr25519.secretFromSeed(
        blake2b256(utf8(`${own}/${kind}/${requestId}/${period}`)),
      );
      return {
        public: sr25519.getPublicKey(secret),
        sign: (payload) => sr25519.sign(secret, payload),
      };
    },
    claimNotification: async () => ({ line: "claimed in the test", seq: 0 }),
    notificationSettled: async () => {},
    accounts: {
      allow: async (_product, index) => ({
        account: bytesToHex(sr25519.getPublicKey(accountAt(bytesToHex(index)))),
        until: Date.now() + 86_400_000,
      }),
    },
    confirmSend: async () => true,
    confirmAccept: async () => true,
    showChat: () => {},
    log: () => {},
  });
  const channel = new MessageChannel();
  host.attachChat(channel.port2);
  const productStorage = kept?.productStorage ?? new MemoryStorage();
  const session = new NovaSession({
    host: novaHostOver(channel.port1),
    statements: statementsOver(store),
    storage: productStorage,
    recordKey: "nova-chat:record",
    now: Date.now,
    newId: () => `${name}-${++counter}`,
    indexFor: (handle) => bytesToHex(blake2b256(utf8(`${name}:${handle}`))),
    schedule: (task, delayMs) => {
      const timer = setTimeout(task, delayMs);
      return () => clearTimeout(timer);
    },
    encodeText: (id, timestamp, text) =>
      bytesToHex(
        encodeMessage({ id, timestamp, content: { kind: "Text", text } }),
      ),
    report: () => {},
  });
  return { name, host, session, notifications, blocked, productStorage };
}

async function until(
  what: string,
  check: () => boolean,
  timeoutMs = 5_000,
): Promise<void> {
  const end = Date.now() + timeoutMs;
  while (!check()) {
    if (Date.now() > end) throw new Error(`timed out waiting for ${what}`);
    await new Promise((resolve) => setTimeout(resolve, 10));
  }
}

const texts = (user: User, peer: string) =>
  user.session
    .messages(handleOf(ids[user.name], peer))
    .map(
      (message) => `${message.author.kind}:${message.text}:${message.state}`,
    );

describe("Nova Chat, host side and worker session together", () => {
  test("request, Accept in the Inbox, text both ways, a restart, and a block", async () => {
    const store = new MemoryStatementStore();
    let a = startUser("A", store);
    const b = startUser("B", store);
    await a.host.start();
    await b.host.start();
    await a.session.run();
    await b.session.run();

    const bForA = handleOf(ids.A, ids.B);
    await a.session.start({ handle: { bytes: bForA as HexString } }, "Hi Bob");
    expect(a.session.conversations()).toMatchObject([
      {
        id: bForA,
        contact: { state: "Outgoing" },
        composer: { allowed: false },
      },
    ]);
    expect(a.host.sent()).toMatchObject([{ purpose: "Hi Bob", paused: null }]);

    await until("B's Inbox", () => b.notifications.list().length === 1);
    const item = b.notifications.list()[0]!;
    expect(item).toMatchObject({
      sender: { name: "Alice" },
      purpose: "Hi Bob",
      state: { tag: "pending" },
    });
    expect(b.host.owns(item.id)).toBe(true);
    await b.host.accept(item);
    expect(b.notifications.get(item.id)?.state.tag).toBe("accepted");

    const aForB = handleOf(ids.B, ids.A);
    await until(
      "both established",
      () =>
        a.session.conversations()[0]?.contact?.state === "Established" &&
        b.session.conversations()[0]?.contact?.state === "Established",
    );
    expect(a.host.sent()).toEqual([]);

    await a.session.send(bForA, "hello Bob");
    await until("B reads A", () =>
      texts(b, ids.A).includes("participant:hello Bob:received"),
    );
    await until("A's message acked", () =>
      texts(a, ids.B).includes("self:hello Bob:delivered"),
    );
    await b.session.send(aForB, "hello Alice");
    await until("A reads B", () =>
      texts(a, ids.B).includes("participant:hello Alice:received"),
    );

    // A restarts: a new host side and session from what each kept.
    a.session.stop();
    a.host.stop();
    a = startUser("A", store, a);
    await a.host.start();
    await a.session.run();
    expect(
      texts(a, ids.B).filter((line) => line.endsWith("hello Alice:received")),
    ).toHaveLength(1);
    await b.session.send(aForB, "after restart");
    await until("A reads B after restart", () =>
      texts(a, ids.B).includes("participant:after restart:received"),
    );
    expect(
      texts(a, ids.B).filter((line) => line.includes("hello Alice")),
    ).toHaveLength(1);
    await a.session.send(bForA, "back again");
    await until("B reads A after restart", () =>
      texts(b, ids.A).includes("participant:back again:received"),
    );

    // B blocks A: B's host neither seals for A nor opens A's frames.
    b.blocked.push(ids.A);
    await b.session.send(aForB, "not sent");
    await until("B's send failed", () =>
      texts(b, ids.A).includes("self:not sent:failed"),
    );
    await a.session.send(bForA, "not read");
    await new Promise((resolve) => setTimeout(resolve, 200));
    expect(texts(b, ids.A).some((line) => line.includes("not read"))).toBe(
      false,
    );
    // A's message left A: B's host refused to open it.
    expect(texts(a, ids.B)).toContain("self:not read:sent");

    // One account per chat: the host refuses a second index for Bob.
    const port = new MessageChannel();
    a.host.attachChat(port.port2);
    const probe = novaHostOver(port.port1);
    expect(
      await probe.call({
        call: "conversationAccount",
        handle: bForA,
        index: bytesToHex(blake2b256(utf8("another index"))),
      }),
    ).toEqual({
      ok: false,
      reason: "This chat already has another account.",
      final: true,
    });

    a.session.stop();
    b.session.stop();
    a.host.stop();
    b.host.stop();
  }, 20_000);

  test("a request from a blocked person never reaches the Inbox", async () => {
    Object.assign(globalThis, { localStorage: new MemoryStorage() });
    const store = new MemoryStatementStore();
    const a = startUser("A", store);
    const b = startUser("B", store, {
      notifications: new NotificationStore(new MemoryStorage()),
      productStorage: new MemoryStorage(),
      blocked: [ids.A],
    });
    await a.host.start();
    await b.host.start();
    await a.session.run();
    await a.session.start(
      { handle: { bytes: handleOf(ids.A, ids.B) as HexString } },
      "Hi",
    );
    await new Promise((resolve) => setTimeout(resolve, 200));
    expect(b.notifications.list()).toEqual([]);
    a.session.stop();
    a.host.stop();
    b.host.stop();
  });
});
