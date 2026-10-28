import { mnemonicToEntropy } from "@scure/bip39";
import { wordlist } from "@scure/bip39/wordlists/english.js";
import type { Wallet } from "./wallets.js";

/** The ids of the accounts built into this host. */
export type DevAccountId = "alice" | "bob" | "carol";

/** An account built into this host from a published BIP-39 test vector. */
export interface DevAccount {
  id: DevAccountId;
  name: string;
  /** A public test vector: anyone can sign as the account it derives. */
  phrase: string;
}

/**
 * The accounts the Account picker offers beside imported wallets. The
 * phrases are public on purpose, so the accounts are ready to use and to be
 * registered elsewhere; they are shared by everyone who has them, so nothing
 * of value belongs on them. Their ids and names key stored state and must
 * not change.
 */
export const DEV_ACCOUNTS: readonly DevAccount[] = [
  {
    id: "alice",
    name: "Alice",
    phrase:
      "jelly better achieve collect unaware mountain thought cargo oxygen act hood bridge",
  },
  {
    id: "bob",
    name: "Bob",
    phrase:
      "cat swing flag economy stadium alone churn speed unique patch report train",
  },
  {
    id: "carol",
    name: "Carol",
    phrase:
      "vessel ladder alter error federal sibling chat ability sun glass valve picture",
  },
];

/** The entropy a session signs in with for `account`. */
export function devAccountEntropy(account: DevAccount): Uint8Array {
  return mnemonicToEntropy(account.phrase, wordlist);
}

/**
 * `account` as a wallet the host signs in like any imported one. Its id is
 * its storage scope, apart from every imported wallet's.
 */
export function devAccountWallet(account: DevAccount): Wallet {
  return {
    id: `test-account:${account.id}`,
    name: `${account.name} (test account)`,
    entropy: devAccountEntropy(account),
  };
}
