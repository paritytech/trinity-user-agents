import { describe, expect, test } from "bun:test";
import { validateMnemonic } from "@scure/bip39";
import { wordlist } from "@scure/bip39/wordlists/english.js";
import {
  DEV_ACCOUNTS,
  devAccountEntropy,
  devAccountWallet,
} from "../../../src/platform/dev-accounts.js";
import {
  TEST_USERS,
  testUserEntropy,
  testUserScope,
} from "../../../fixtures/invitations/test-users.js";

const hex = (bytes: Uint8Array) => Buffer.from(bytes).toString("hex");

describe("the accounts built into the host", () => {
  test("are three distinct, valid public test vectors", () => {
    expect(DEV_ACCOUNTS.map((account) => account.id)).toEqual([
      "alice",
      "bob",
      "carol",
    ]);
    for (const account of DEV_ACCOUNTS)
      expect(validateMnemonic(account.phrase, wordlist)).toBe(true);
    expect(
      new Set(DEV_ACCOUNTS.map((account) => hex(devAccountEntropy(account))))
        .size,
    ).toBe(DEV_ACCOUNTS.length);
  });

  // The id keys the wallet's stored state, the product storage scope and the
  // tab's remembered sign-in, so renaming it would orphan what is stored.
  test("are wallets whose ids and names stored state depends on", () => {
    expect(
      DEV_ACCOUNTS.map(devAccountWallet).map(({ id, name }) => ({ id, name })),
    ).toEqual([
      { id: "test-account:alice", name: "Alice (test account)" },
      { id: "test-account:bob", name: "Bob (test account)" },
      { id: "test-account:carol", name: "Carol (test account)" },
    ]);
  });

  test("sign in with the entropy of their phrase, as an imported wallet does", () => {
    for (const account of DEV_ACCOUNTS)
      expect(hex(devAccountWallet(account).entropy)).toBe(
        hex(devAccountEntropy(account)),
      );
  });

  // The invitation demo builds on these accounts; they never build on it.
  test("are the accounts the invitation demo signs in with, under a scope of its own", () => {
    expect(TEST_USERS.map((user) => user.phrase)).toEqual(
      DEV_ACCOUNTS.map((account) => account.phrase),
    );
    for (const [index, user] of TEST_USERS.entries()) {
      expect(hex(testUserEntropy(user))).toBe(
        hex(devAccountEntropy(DEV_ACCOUNTS[index]!)),
      );
      expect(testUserScope(user)).not.toBe(
        devAccountWallet(DEV_ACCOUNTS[index]!).id,
      );
    }
  });
});
