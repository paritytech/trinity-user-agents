import { describe, expect, test } from "bun:test";
import { validateMnemonic } from "@scure/bip39";
import { wordlist } from "@scure/bip39/wordlists/english.js";
import * as sr25519 from "@scure/sr25519";
import {
  deriveChatSecret,
  deriveIdentitySecret,
} from "../../../../src/invitations/chat-keys.js";
import { x25519PublicKey } from "../../../../bridges/primitives/crypto.js";
import {
  TEST_USERS,
  chatContactsOf,
  testUserAccount,
  testUserDirectory,
  testUserEntropy,
  testUserKey,
  testUserScope,
} from "../../../../fixtures/invitations/test-users.js";

const hex = (bytes: Uint8Array): string =>
  `0x${Buffer.from(bytes).toString("hex")}`;

describe("test users", () => {
  test("each signs in with its own valid public test phrase, apart from every wallet", () => {
    for (const user of TEST_USERS)
      expect(validateMnemonic(user.phrase, wordlist)).toBe(true);
    const entropies = TEST_USERS.map((user) =>
      Buffer.from(testUserEntropy(user)).toString("hex"),
    );
    expect(new Set(entropies).size).toBe(TEST_USERS.length);
    expect(TEST_USERS.map(testUserScope)).toEqual([
      "test-user:alice",
      "test-user:bob",
      "test-user:carol",
    ]);
  });

  // Contacts name each user by their identity account, as chat names people,
  // so the core mints for them the handles it gives a product that picked
  // them, and the host checks their host proof with that account.
  test("each is named by the identity account its phrase derives on the network", () => {
    for (const user of TEST_USERS)
      expect<string>(testUserAccount(user, "paseo")).toBe(
        hex(
          sr25519.getPublicKey(
            deriveIdentitySecret(testUserEntropy(user), "paseo"),
          ),
        ),
      );
    const [alice] = TEST_USERS;
    expect(testUserAccount(alice!, "paseo")).not.toBe(
      testUserAccount(alice!, "other"),
    );
  });

  test("Alice has two contacts to choose from; each list names others only", () => {
    const [alice, bob, carol] = TEST_USERS;
    expect(chatContactsOf(alice!, "paseo")).toEqual([
      { account: testUserAccount(bob!, "paseo"), name: "Bob" },
      { account: testUserAccount(carol!, "paseo"), name: "Carol" },
    ]);
    expect(
      chatContactsOf(bob!, "paseo").map((contact) => contact.name),
    ).toEqual(["Alice"]);
    for (const user of TEST_USERS)
      expect(
        chatContactsOf(user, "paseo").some(
          (contact) => contact.account === testUserAccount(user, "paseo"),
        ),
      ).toBe(false);
  });

  test("the fixture directory publishes each user's own chat key, and nothing for anyone else", async () => {
    const directory = testUserDirectory("paseo");
    for (const user of TEST_USERS) {
      const record = await directory.consumer(
        Buffer.from(testUserAccount(user, "paseo").slice(2), "hex"),
      );
      expect(record?.chatKey && hex(record.chatKey)).toBe(
        hex(x25519PublicKey(deriveChatSecret(testUserEntropy(user)))),
      );
    }
    expect(await directory.consumer(new Uint8Array(32))).toBeNull();
    expect(await directory.names(new Uint8Array(32))).toBeNull();
  });

  // Requests kept before the wallet keys named fixture identities, so those
  // lists are left in place and not read.
  test("the lists moved to versions that hold wallet-key requests only", () => {
    const [alice] = TEST_USERS;
    expect(
      (
        [
          "outbox",
          "received",
          "notifications",
          "contact-chat-outbox",
          "contact-chat-received",
        ] as const
      ).map((part) => testUserKey("paseo", alice!.id, part).split(":").at(-1)),
    ).toEqual(["v3", "v3", "v2", "v2", "v2"]);
  });
});
