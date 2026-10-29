import { describe, expect, test } from "bun:test";
import { bytesToHex } from "@parity/truapi/scale";
import { mnemonicToEntropy } from "@scure/bip39";
import { wordlist } from "@scure/bip39/wordlists/english.js";
import {
  bytes,
  compact,
  concat,
  text,
  u64,
} from "../../../../src/chat/nova/codec.js";
import type { RandomBytes } from "../../../../src/chat/nova/crypto.js";
import { localNovaIdentity } from "../../../../src/chat/nova/identity.js";
import { ChatKeys } from "../../../../src/chat/nova/keys.js";
import {
  isDeviceEnvelope,
  openDeviceEnvelope,
  wrapDeviceEnvelope,
} from "../../../../src/chat/nova/multi-device.js";
import {
  decodeMessage,
  decodeSessionData,
  encodeMessage,
  encodeRequestMessage,
  encodeSessionData,
  identityProofMatches,
  openRequestPlaintext,
  readRequestPlaintext,
  sealSessionPlaintext,
  unsealSession,
  type DeviceInfo,
} from "../../../../src/chat/nova/protocol.js";
import { decodeStatement } from "../../../../src/chat/nova/statement.js";

/** Synthetic phrases from the BIP-39 test vectors. They hold nothing. */
const ALICE =
  "legal winner thank year wave sausage worth useful legal winner thank yellow";
const BOB =
  "letter advice cage absurd amount doctor acoustic avoid letter advice cage above";
const keys = (phrase: string) =>
  ChatKeys.fromEntropy(mnemonicToEntropy(phrase, wordlist), "paseo");

function counter(): RandomBytes {
  let next = 1;
  return (length) => Uint8Array.from({ length }, () => next++ & 0xff);
}

const deviceOf = (chatKeys: ChatKeys): DeviceInfo => ({
  statementAccount: chatKeys.identity,
  encryptionKey: chatKeys.devicePublic,
});

describe("device key", () => {
  test("is derived from the entropy, and is neither the chat key nor the identity", () => {
    const entropy = mnemonicToEntropy(ALICE, wordlist);
    const first = ChatKeys.fromEntropy(entropy, "paseo");
    const again = ChatKeys.fromEntropy(entropy, "testnet");
    expect(bytesToHex(first.devicePublic)).toBe(bytesToHex(again.devicePublic));
    expect(bytesToHex(first.devicePublic)).not.toBe(
      bytesToHex(first.chatPublic),
    );
    expect(bytesToHex(first.devicePublic)).not.toBe(bytesToHex(first.identity));
    expect(bytesToHex(keys(BOB).devicePublic)).not.toBe(
      bytesToHex(first.devicePublic),
    );
  });

  test("agrees with a peer key the way X25519 does, in both directions", () => {
    const alice = keys(ALICE);
    const bob = keys(BOB);
    expect(alice.agreeDevice(bob.devicePublic)).toEqual(
      bob.agreeDevice(alice.devicePublic),
    );
    expect(alice.agreeDevice(bob.chatPublic)).toEqual(
      bob.agree(alice.devicePublic),
    );
  });
});

describe("device envelope", () => {
  const request = encodeSessionData({
    kind: "request",
    requestId: "r-1",
    messages: [
      encodeMessage({
        id: "m-1",
        timestampMs: 5n,
        content: { kind: "text", text: "hi" },
      }),
    ],
  });
  const response = encodeSessionData({
    kind: "response",
    requestId: "r-1",
    code: 0,
  });

  test("a request and a response open for the device they name, to the plain session data", () => {
    const alice = keys(ALICE);
    const bob = keys(BOB);
    for (const data of [request, response]) {
      const wrapped = wrapDeviceEnvelope(
        data,
        [deviceOf(bob)],
        (key) => alice.agreeDevice(key),
        counter(),
      );
      expect(isDeviceEnvelope(wrapped)).toBe(true);
      expect(wrapped[0]).toBe(data[0] + 2);
      expect(
        openDeviceEnvelope(wrapped, bob.identity, alice.devicePublic, (key) =>
          bob.agreeDevice(key),
        ),
      ).toEqual(data);
    }
  });

  test("each of several recipients opens it, and only the entry for its account", () => {
    const alice = keys(ALICE);
    const bob = keys(BOB);
    const other = keys(
      "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about",
    );
    const wrapped = wrapDeviceEnvelope(
      request,
      [deviceOf(bob), deviceOf(other)],
      (key) => alice.agreeDevice(key),
      counter(),
    );
    for (const recipient of [bob, other])
      expect(
        openDeviceEnvelope(
          wrapped,
          recipient.identity,
          alice.devicePublic,
          (key) => recipient.agreeDevice(key),
        ),
      ).toEqual(request);
  });

  test("is refused by a device it does not name, with the wrong sender key, or when changed", () => {
    const alice = keys(ALICE);
    const bob = keys(BOB);
    const wrapped = wrapDeviceEnvelope(
      request,
      [deviceOf(bob)],
      (key) => alice.agreeDevice(key),
      counter(),
    );
    const open = (
      data: Uint8Array,
      account = bob.identity,
      sender = alice.devicePublic,
    ) =>
      openDeviceEnvelope(data, account, sender, (key) => bob.agreeDevice(key));
    expect(() => open(wrapped, alice.identity)).toThrow("not addressed");
    expect(() => open(wrapped, bob.identity, bob.devicePublic)).toThrow();
    const changed = wrapped.slice();
    changed[changed.length - 1] ^= 1;
    expect(() => open(changed)).toThrow();
    const payload = wrapped.slice();
    payload[4] ^= 1;
    expect(() => open(payload)).toThrow();
    expect(() => open(concat(wrapped, Uint8Array.of(0)))).toThrow();
  });

  test("session data that is not an envelope passes through, and an empty recipient list or a multi-device tag cannot be wrapped", () => {
    const alice = keys(ALICE);
    expect(
      openDeviceEnvelope(request, alice.identity, alice.devicePublic, () => {
        throw new Error("no key is needed");
      }),
    ).toBe(request);
    expect(() =>
      wrapDeviceEnvelope(
        request,
        [],
        (key) => alice.agreeDevice(key),
        counter(),
      ),
    ).toThrow("at least one recipient");
    expect(() =>
      wrapDeviceEnvelope(
        Uint8Array.of(2, 0),
        [deviceOf(alice)],
        (key) => alice.agreeDevice(key),
        counter(),
      ),
    ).toThrow("only a request or a response");
  });
});

describe("identity operations of the device route", () => {
  test("a statement one host writes is read by the other, on topics both derive", async () => {
    const alice = keys(ALICE);
    const bob = keys(BOB);
    const aliceIdentity = localNovaIdentity(alice, counter());
    const bobIdentity = localNovaIdentity(bob, counter());
    const plaintext = encodeSessionData({
      kind: "request",
      requestId: "r-1",
      messages: [
        encodeMessage({
          id: "m-1",
          timestampMs: 5n,
          content: { kind: "text", text: "hi" },
        }),
      ],
    });
    const statement = decodeStatement(
      await aliceIdentity.deviceSessionStatement({
        peer: bob.identity,
        peerChatKey: bob.chatPublic,
        devices: [bobIdentity.device],
        plaintext,
        expiry: 7n,
      }),
    );
    const toBob = await aliceIdentity.deviceTopics(
      bob.identity,
      bob.chatPublic,
      bobIdentity.device,
    );
    const fromAlice = await bobIdentity.deviceTopics(
      alice.identity,
      alice.chatPublic,
      aliceIdentity.device,
    );
    expect(statement.topics).toEqual([toBob.outgoing]);
    expect(toBob.outgoing).toEqual(fromAlice.incoming);
    expect(toBob.incoming).toEqual(fromAlice.outgoing);
    expect(
      await bobIdentity.openDeviceSession(aliceIdentity.device, statement.data),
    ).toEqual(plaintext);
    // The device route is not the identity route: its topics differ.
    expect(
      (await aliceIdentity.sessionTopics(bob.identity, bob.chatPublic))
        .outgoing,
    ).not.toEqual(toBob.outgoing);
    // A session sealed under the identity key is not a device-route statement.
    await expect(
      bobIdentity.openDeviceSession(
        aliceIdentity.device,
        sealSessionPlaintext(bob.agree(alice.chatPublic), plaintext, counter()),
      ),
    ).rejects.toThrow();
  });

  test("a host will not wrap a multi-device tag or a request nobody can open", async () => {
    const alice = localNovaIdentity(keys(ALICE), counter());
    const bob = keys(BOB);
    await expect(
      alice.deviceSessionStatement({
        peer: bob.identity,
        peerChatKey: bob.chatPublic,
        devices: [],
        plaintext: encodeSessionData({
          kind: "response",
          requestId: "r",
          code: 0,
        }),
        expiry: 1n,
      }),
    ).rejects.toThrow("at least one recipient");
    await expect(
      alice.deviceSessionStatement({
        peer: bob.identity,
        peerChatKey: bob.chatPublic,
        devices: [
          { statementAccount: bob.identity, encryptionKey: bob.devicePublic },
        ],
        plaintext: Uint8Array.of(2, 0),
        expiry: 1n,
      }),
    ).rejects.toThrow();
  });
});

describe("version 2 request and device acceptance", () => {
  test("a request the identity builds carries its identity, proof and device, and verifies for the acceptor", async () => {
    const alice = keys(ALICE);
    const bob = keys(BOB);
    const identity = localNovaIdentity(alice, counter());
    const { statement, plaintext } = await identity.requestStatement({
      peer: bob.identity,
      peerChatKey: bob.chatPublic,
      id: "req-1",
      timestampMs: 9n,
      welcome: "hello",
      expiry: 1n,
      day: 3n,
    });
    expect(decodeStatement(statement).topics).toHaveLength(3);
    const opened = openRequestPlaintext(plaintext, bob.identity);
    expect(opened.content).toMatchObject({
      version: 2,
      welcome: "hello",
    });
    if (opened.content.version !== 2) throw new Error("version 2");
    expect(opened.content.identity).toEqual(alice.identity);
    expect(opened.content.deviceKey).toEqual(alice.devicePublic);
    expect(opened.signer).toEqual(alice.identity);
    expect(opened.peer).toEqual(alice.identity);
    expect(
      identityProofMatches(
        bob.agree(alice.chatPublic),
        alice.identity,
        alice.identity,
        opened.content.identityProof,
      ),
    ).toBe(true);
    // Another chat key, another signer or another identity does not match.
    for (const [secret, who, signer] of [
      [bob.agree(bob.chatPublic), alice.identity, alice.identity],
      [bob.agree(alice.chatPublic), alice.identity, bob.identity],
      [bob.agree(alice.chatPublic), bob.identity, alice.identity],
    ] as const)
      expect(
        identityProofMatches(secret, who, signer, opened.content.identityProof),
      ).toBe(false);
    // The signed message is the encoding alone; the proof follows it.
    expect(readRequestPlaintext(plaintext).message).toEqual(
      encodeRequestMessage(opened),
    );
  });

  test("a request for another acceptor does not open", async () => {
    const alice = localNovaIdentity(keys(ALICE), counter());
    const bob = keys(BOB);
    const { plaintext } = await alice.requestStatement({
      peer: bob.identity,
      peerChatKey: bob.chatPublic,
      id: "req-1",
      timestampMs: 9n,
      welcome: null,
      expiry: 1n,
      day: 3n,
    });
    expect(() => openRequestPlaintext(plaintext, alice.identity)).toThrow(
      "signature does not verify",
    );
  });

  test("a device acceptance is kind 20: the request id, the account, then the key", () => {
    const device = {
      statementAccount: new Uint8Array(32).fill(0xaa),
      encryptionKey: new Uint8Array(32).fill(0xbb),
    };
    const encoded = encodeMessage({
      id: "acc-1",
      timestampMs: 5n,
      content: { kind: "device-accepted", requestId: "req-1", device },
    });
    expect(encoded).toEqual(
      concat(
        text("acc-1"),
        u64(5n),
        Uint8Array.of(0, 20),
        text("req-1"),
        device.statementAccount,
        device.encryptionKey,
      ),
    );
    expect(decodeMessage(encoded).content).toEqual({
      kind: "device-accepted",
      requestId: "req-1",
      device,
    });
    expect(() =>
      decodeMessage(encoded.subarray(0, encoded.length - 1)),
    ).toThrow();
    expect(() => decodeMessage(concat(encoded, Uint8Array.of(0)))).toThrow();
    // The older acceptance (kind 14) stays what it was.
    expect(
      decodeMessage(
        encodeMessage({
          id: "a",
          timestampMs: 1n,
          content: { kind: "accepted", requestId: "req-1" },
        }),
      ).content,
    ).toEqual({ kind: "accepted", requestId: "req-1" });
  });

  test("a multi-device tag on the identity route is recognised and not opened", () => {
    expect(
      decodeSessionData(
        concat(Uint8Array.of(2), bytes(new Uint8Array(3)), compact(0)),
      ),
    ).toEqual({ kind: "multi-device" });
    const sealed = sealSessionPlaintext(
      new Uint8Array(32).fill(1),
      Uint8Array.of(2, 0),
      counter(),
    );
    expect(
      decodeSessionData(unsealSession(new Uint8Array(32).fill(1), sealed)),
    ).toEqual({
      kind: "multi-device",
    });
  });
});
