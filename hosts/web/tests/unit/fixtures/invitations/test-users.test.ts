import { describe, expect, test } from "bun:test";
import { validateMnemonic } from "@scure/bip39";
import { wordlist } from "@scure/bip39/wordlists/english.js";
import * as sr25519 from "@scure/sr25519";
import { miniSecret } from "../../../../src/chat/nova/keys.js";
import {
  TEST_USERS,
  chatContactsOf,
  testUserEntropy,
  testUserScope,
} from "../../../../fixtures/invitations/test-users.js";

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

  // Contacts name each user by their own session's root account, so the
  // core mints for them the handles it gives a product that picked them.
  test("each is named by the root account of its own phrase, the one the core signs in as", () => {
    for (const user of TEST_USERS)
      expect(user.account).toBe(
        `0x${Buffer.from(
          sr25519.getPublicKey(
            sr25519.secretFromSeed(miniSecret(testUserEntropy(user))),
          ),
        ).toString("hex")}`,
      );
  });

  test("Alice has two contacts to choose from; each list names others only", () => {
    const [alice, bob, carol] = TEST_USERS;
    expect(chatContactsOf(alice!)).toEqual([
      { account: bob!.account, name: "Bob" },
      { account: carol!.account, name: "Carol" },
    ]);
    expect(chatContactsOf(bob!).map((contact) => contact.name)).toEqual([
      "Alice",
    ]);
    for (const user of TEST_USERS)
      expect(
        chatContactsOf(user).some(
          (contact) => contact.account === user.account,
        ),
      ).toBe(false);
  });
});
