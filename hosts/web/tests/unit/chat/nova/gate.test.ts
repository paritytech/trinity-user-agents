import { describe, expect, test } from "bun:test";
import { mnemonicToEntropy } from "@scure/bip39";
import { wordlist } from "@scure/bip39/wordlists/english.js";
import {
  PUBLIC_WALLET_REFUSED,
  chatRefusal,
} from "../../../../src/chat/nova/gate.js";

const entropy = (phrase: string) => mnemonicToEntropy(phrase, wordlist);

describe("chat refusal", () => {
  test("refuses the public development phrase and nothing else", () => {
    expect(
      chatRefusal(
        entropy(
          "bottom drive obey lake curtain smoke basket hold race lonely fit walk",
        ),
      ),
    ).toBe(PUBLIC_WALLET_REFUSED);
    // A BIP-39 test vector phrase. It holds nothing.
    expect(
      chatRefusal(
        entropy(
          "letter advice cage absurd amount doctor acoustic avoid letter advice cage above",
        ),
      ),
    ).toBeNull();
  });
});
