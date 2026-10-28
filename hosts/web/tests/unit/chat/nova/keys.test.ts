import { describe, expect, test } from "bun:test";
import { bytesToHex } from "@parity/truapi/scale";
import { mnemonicToEntropy } from "@scure/bip39";
import { wordlist } from "@scure/bip39/wordlists/english.js";
import * as sr25519 from "@scure/sr25519";
import {
  ChatKeys,
  deriveChatSecret,
  identityIndex,
  junctionCode,
  miniSecret,
} from "../../../../src/chat/nova/keys.js";

/** The public Substrate development phrase. It holds nothing. */
const DEV_PHRASE =
  "bottom drive obey lake curtain smoke basket hold race lonely fit walk";
const devEntropy = () => mnemonicToEntropy(DEV_PHRASE, wordlist);

describe("chat key", () => {
  test("matches the core's pinned chat secret", () => {
    // rust/crates/truapi/src/host_logic/sso/pairing.rs pins this pair.
    const entropy = Uint8Array.from({ length: 32 }, (_, index) => index + 1);
    expect(bytesToHex(deriveChatSecret(entropy))).toBe(
      "0x88705746a9788fb8bc232cc463b243a2828e470b932ce60a16426cc79b4618b1",
    );
  });
});

describe("identity account", () => {
  test("the mini-secret and a hard junction give the well-known dev accounts", () => {
    const root = sr25519.secretFromSeed(miniSecret(devEntropy()));
    expect(bytesToHex(sr25519.getPublicKey(root))).toBe(
      "0x46ebddef8cd9bb167dc30878d7113b7e168e6f0646beffd77d69d39bad76b47a",
    );
    const alice = sr25519.HDKD.secretHard(root, junctionCode("Alice"));
    expect(bytesToHex(sr25519.getPublicKey(alice))).toBe(
      "0xd43593c715fdd31c61141abd04a99fd6822c8558854ccde39a5684e7a56da27d",
    );
  });

  test("the identity index is the core's index_bytes(0)", () => {
    // rust/crates/truapi/src/host_logic/product_account.rs pins this value.
    expect(bytesToHex(identityIndex())).toBe(
      "0x0000000012e86013736c5498f050b03cdc16957dff0e422fb92ca77ec3ab168f",
    );
  });

  test("differs per network suffix and signs verifiably", () => {
    const paseo = ChatKeys.fromEntropy(devEntropy(), "paseo");
    const testnet = ChatKeys.fromEntropy(devEntropy(), "testnet");
    expect(bytesToHex(paseo.identity)).not.toBe(bytesToHex(testnet.identity));
    expect(bytesToHex(paseo.chatPublic)).toBe(bytesToHex(testnet.chatPublic));
    const message = new TextEncoder().encode("hello");
    expect(sr25519.verify(message, paseo.sign(message), paseo.identity)).toBe(
      true,
    );
  });
});
