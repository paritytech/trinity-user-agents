import { mnemonicToEntropy } from "@scure/bip39";
import { wordlist } from "@scure/bip39/wordlists/english.js";
import { equalBytes } from "./codec.js";

/**
 * Phrases published for anyone to use. An identity derived from one is
 * shared with every other user of it, so chat does not run as it: requests
 * to it are not this user's, and anything it publishes speaks for everyone
 * who holds the phrase.
 */
const PUBLIC_PHRASES = [
  // The Substrate development phrase (Alice, Bob, ... derive from it).
  "bottom drive obey lake curtain smoke basket hold race lonely fit walk",
];

const publicEntropies = PUBLIC_PHRASES.map((phrase) =>
  mnemonicToEntropy(phrase, wordlist),
);

export const PUBLIC_WALLET_REFUSED =
  "Not available for the public development phrase.";

/** Why chat may not run for this wallet, or null when it may. */
export const chatRefusal = (entropy: Uint8Array): string | null =>
  publicEntropies.some((known) => equalBytes(known, entropy))
    ? PUBLIC_WALLET_REFUSED
    : null;
