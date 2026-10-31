import { describe, expect, test } from "bun:test";
import { mnemonicToEntropy } from "@scure/bip39";
import { wordlist } from "@scure/bip39/wordlists/english.js";
import { ChatKeys } from "../../../src/invitations/chat-keys.js";
import {
  decodeStatement,
  encodeSigned,
  expiryAt,
  nextExpiry,
  requestDay,
  verifyStatement,
} from "../../../src/platform/statement.js";

/** A synthetic phrase from the BIP-39 test vectors. It holds nothing. */
const ALICE =
  "legal winner thank year wave sausage worth useful legal winner thank yellow";

describe("statements", () => {
  test("encode, decode and verify, and a changed field breaks the proof", () => {
    const alice = ChatKeys.fromEntropy(
      mnemonicToEntropy(ALICE, wordlist),
      "paseo",
    );
    const statement = {
      expiry: expiryAt(Date.UTC(2026, 9, 15)),
      channel: new Uint8Array(32).fill(3),
      topics: [new Uint8Array(32).fill(1), new Uint8Array(32).fill(2)],
      data: Uint8Array.of(9, 8, 7),
    };
    const encoded = encodeSigned(statement, alice.identity, (payload) =>
      alice.sign(payload),
    );
    const decoded = decodeStatement(encoded);
    expect(decoded.signer).toEqual(alice.identity);
    expect(verifyStatement(decoded)).toBe(true);
    expect(verifyStatement({ ...decoded, data: Uint8Array.of(9, 8, 6) })).toBe(
      false,
    );
    // Field count 6: proof, expiry, channel, two topics, data, in tag order.
    expect(encoded[0]).toBe(6 << 2);
    expect(encoded[1]).toBe(0);
  });

  test("expiry never lapses and counts seconds from the protocol epoch", () => {
    const at = (1_763_164_800 + 90) * 1000;
    expect(expiryAt(at)).toBe((0xffffffffn << 32n) | 90n);
    expect(nextExpiry(expiryAt(at), at)).toBe(expiryAt(at) + 1n);
    expect(requestDay(at + 2 * 86_400_000)).toBe(2n);
  });
});
