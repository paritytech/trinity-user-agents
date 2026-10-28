import { describe, expect, test } from "bun:test";
import { validateMnemonic } from "@scure/bip39";
import { wordlist } from "@scure/bip39/wordlists/english.js";
import {
  TEST_USERS,
  chatContactsOf,
  testAccountWallet,
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

  // A test account is the phrase's account under a scope of its own, so it
  // shares no stored state with the invitation demo of the same name.
  test("each is also a wallet, under a storage scope apart from the demo's", () => {
    const [alice] = TEST_USERS;
    const wallet = testAccountWallet(alice!);
    expect({ ...wallet, entropy: [...wallet.entropy] }).toEqual({
      id: "test-account:alice",
      name: "Alice (test account)",
      entropy: [...testUserEntropy(alice!)],
    });
    expect(wallet.id).not.toBe(testUserScope(alice!));
  });

  test("Alice has two contacts to choose from; each list names others only, by made-up accounts", () => {
    const [alice, bob] = TEST_USERS;
    expect(chatContactsOf(alice!)).toEqual([
      { account: `0x${"b0".repeat(32)}`, name: "Bob" },
      { account: `0x${"c4".repeat(32)}`, name: "Carol" },
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
