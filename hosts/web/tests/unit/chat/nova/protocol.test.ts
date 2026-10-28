import { describe, expect, test } from "bun:test";
import { mnemonicToEntropy } from "@scure/bip39";
import { wordlist } from "@scure/bip39/wordlists/english.js";
import {
  bytes,
  concat,
  equalBytes,
  text,
  u64,
} from "../../../../src/chat/nova/codec.js";
import {
  aeadKey,
  keyedBlake2b256,
  seal,
  x25519Agree,
  x25519PublicKey,
  type RandomBytes,
} from "../../../../src/chat/nova/crypto.js";
import { ChatKeys } from "../../../../src/chat/nova/keys.js";
import {
  checkIdentityProof,
  decodeMessage,
  decodeSessionData,
  encodeMessage,
  encodeSessionData,
  openRequest,
  openSession,
  requestChannel,
  requestTopic,
  sealRequest,
  sealSession,
  sessionTopic,
  withoutTickets,
} from "../../../../src/chat/nova/protocol.js";
import {
  decodeStatement,
  encodeSigned,
  expiryAt,
  nextExpiry,
  requestDay,
  verifyStatement,
} from "../../../../src/chat/nova/statement.js";

/** Synthetic phrases from the BIP-39 test vectors. They hold nothing. */
const ALICE =
  "legal winner thank year wave sausage worth useful legal winner thank yellow";
const BOB =
  "letter advice cage absurd amount doctor acoustic avoid letter advice cage above";
const keys = (phrase: string) =>
  ChatKeys.fromEntropy(mnemonicToEntropy(phrase, wordlist), "paseo");

/** A repeatable byte source, so sealed output does not depend on the clock or the platform. */
function counter(): RandomBytes {
  let next = 1;
  return (length) => Uint8Array.from({ length }, () => next++ & 0xff);
}

describe("statements", () => {
  test("encode, decode and verify, and a changed field breaks the proof", () => {
    const alice = keys(ALICE);
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

describe("chat requests", () => {
  test("a sealed request opens only for its acceptor and names its signer", () => {
    const alice = keys(ALICE);
    const bob = keys(BOB);
    const request = {
      id: "request-1",
      timestampMs: 1_700_000_000_000n,
      content: { version: 1 as const, welcome: "hi bob" },
    };
    const data = sealRequest(
      request,
      bob.identity,
      bob.chatPublic,
      alice.identity,
      (payload) => alice.sign(payload),
      counter(),
    );
    const opened = openRequest(data, bob.identity, (ephemeral) =>
      bob.agree(ephemeral),
    );
    expect(opened.content).toEqual({ version: 1, welcome: "hi bob" });
    expect(opened.peer).toEqual(alice.identity);
    expect(() =>
      openRequest(data, alice.identity, (ephemeral) => alice.agree(ephemeral)),
    ).toThrow();
  });

  test("a request addressed to someone else does not verify", () => {
    const alice = keys(ALICE);
    const bob = keys(BOB);
    const request = {
      id: "r",
      timestampMs: 1n,
      content: { version: 1 as const, welcome: null },
    };
    // Signed for Alice as acceptor but sealed to Bob's chat key.
    const data = sealRequest(
      request,
      alice.identity,
      bob.chatPublic,
      alice.identity,
      (payload) => alice.sign(payload),
      counter(),
    );
    expect(() =>
      openRequest(data, bob.identity, (ephemeral) => bob.agree(ephemeral)),
    ).toThrow("the request signature does not verify");
  });

  test("a version 2 request is read, and its identity proof binds identity and device", () => {
    const alice = keys(ALICE);
    const bob = keys(BOB);
    // Version 2 is what the app sends: signed by a device key, naming the identity.
    const device = keys(
      "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about",
    );
    const secret = alice.agree(bob.chatPublic);
    const proof = keyedBlake2b256(
      secret,
      concat(alice.identity, device.identity, text("mds-chat-request")),
    );
    const message = concat(
      text("request-2"),
      u64(5n),
      Uint8Array.of(1),
      alice.identity,
      proof,
      x25519PublicKey(new Uint8Array(32).fill(7)),
      Uint8Array.of(0),
      Uint8Array.of(1, 1),
      text("hello"),
      Uint8Array.of(0),
    );
    const signature = device.sign(
      concat(message, Uint8Array.of(0x80), bob.identity),
    );
    const random = counter();
    const ephemeral = random(32);
    const sealed = seal(
      aeadKey(x25519Agree(ephemeral, bob.chatPublic)),
      concat(message, Uint8Array.of(0), signature, device.identity),
      random,
    );
    const data = concat(bytes(x25519PublicKey(ephemeral)), bytes(sealed));
    const opened = openRequest(data, bob.identity, (key) => bob.agree(key));
    expect(opened.peer).toEqual(alice.identity);
    expect(opened.signer).toEqual(device.identity);
    expect(opened.content.welcome).toBe("hello");
    expect(checkIdentityProof(opened, bob.agree(alice.chatPublic))).toBe(true);
    expect(checkIdentityProof(opened, bob.agree(device.chatPublic))).toBe(
      false,
    );
  });
});

describe("sessions", () => {
  test("both sides agree on the topic each one writes to", () => {
    const alice = keys(ALICE);
    const bob = keys(BOB);
    const aliceToBob = sessionTopic(
      alice.agree(bob.chatPublic),
      alice.identity,
      bob.identity,
    );
    expect(
      equalBytes(
        aliceToBob,
        sessionTopic(bob.agree(alice.chatPublic), alice.identity, bob.identity),
      ),
    ).toBe(true);
    expect(
      equalBytes(
        aliceToBob,
        sessionTopic(bob.agree(alice.chatPublic), bob.identity, alice.identity),
      ),
    ).toBe(false);
    expect(equalBytes(requestChannel(aliceToBob), aliceToBob)).toBe(false);
    expect(
      equalBytes(requestTopic(alice.identity), requestTopic(bob.identity)),
    ).toBe(false);
  });

  test("session data and messages round-trip, and unknown kinds keep their tag", () => {
    const alice = keys(ALICE);
    const bob = keys(BOB);
    const messages = [
      encodeMessage({
        id: "m1",
        timestampMs: 10n,
        content: { kind: "text", text: "hello" },
      }),
      encodeMessage({
        id: "m2",
        timestampMs: 11n,
        content: { kind: "accepted", requestId: "r1" },
      }),
    ];
    const sealed = sealSession(
      alice.agree(bob.chatPublic),
      { kind: "request", requestId: "q1", messages },
      counter(),
    );
    const opened = openSession(bob.agree(alice.chatPublic), sealed);
    expect(opened).toEqual({ kind: "request", requestId: "q1", messages });
    expect(decodeMessage(messages[1])).toEqual({
      id: "m2",
      timestampMs: 11n,
      content: { kind: "accepted", requestId: "r1" },
    });
    const reaction = concat(
      text("m3"),
      u64(12n),
      Uint8Array.of(0, 4),
      text("m1"),
      text("+1"),
    );
    expect(decodeMessage(reaction).content).toEqual({
      kind: "reaction",
      messageId: "m1",
      emoji: "+1",
      removed: false,
    });
    expect(
      decodeSessionData(
        encodeSessionData({ kind: "response", requestId: "q1", code: 0 }),
      ),
    ).toEqual({
      kind: "response",
      requestId: "q1",
      code: 0,
    });
    expect(decodeSessionData(Uint8Array.of(2, 0, 0))).toEqual({
      kind: "multi-device",
    });
  });
});

describe("control messages", () => {
  // Layouts from the apps' codecs: iOS RemoteChatMessage.swift and
  // ChatTokenRemoteContent.swift, Android ChatMessageStatementContent.kt.
  const head = (id: string) => concat(text(id), u64(7n), Uint8Array.of(0));
  const account = new Uint8Array(32).fill(0x11);
  const encryption = new Uint8Array(32).fill(0x22);

  test("decodes a push token as a control, for each platform", () => {
    for (const platform of [0, 1, 2])
      expect(
        decodeMessage(
          concat(
            head("t"),
            Uint8Array.of(1),
            bytes(Uint8Array.of(9, 9, 9)),
            Uint8Array.of(platform),
          ),
        ).content,
      ).toEqual({
        kind: "control",
        control: "token",
        token: Uint8Array.of(9, 9, 9),
        platform,
      });
    expect(() =>
      decodeMessage(
        concat(
          head("t"),
          Uint8Array.of(1),
          bytes(Uint8Array.of(9)),
          Uint8Array.of(3),
        ),
      ),
    ).toThrow();
  });

  test("decodes a device change as a control, with its keys", () => {
    expect(
      decodeMessage(
        concat(head("d"), Uint8Array.of(17), bytes(account), bytes(encryption)),
      ).content,
    ).toEqual({
      kind: "control",
      control: "device-added",
      statementAccount: account,
      encryptionKey: encryption,
    });
    expect(
      decodeMessage(concat(head("r"), Uint8Array.of(18), bytes(account)))
        .content,
    ).toEqual({
      kind: "control",
      control: "device-removed",
      statementAccount: account,
    });
  });

  test("refuses a device change that is short or has bytes left over", () => {
    expect(() =>
      decodeMessage(
        concat(
          head("d"),
          Uint8Array.of(17),
          bytes(account.subarray(1)),
          bytes(encryption),
        ),
      ),
    ).toThrow();
    expect(() =>
      decodeMessage(
        concat(
          head("d"),
          Uint8Array.of(17),
          bytes(account),
          bytes(encryption),
          Uint8Array.of(0),
        ),
      ),
    ).toThrow();
  });
});

describe("reaction and leave messages, in the apps' byte layout", () => {
  // id "r1", timestamp 0x0102030405060708, version 0, then the content.
  const HEAD = "0872310807060504030201" + "00";

  test("a reaction is tag 4, the target id, then the emoji, each as SCALE text", () => {
    const encoded = encodeMessage({
      id: "r1",
      timestampMs: 0x0102030405060708n,
      content: {
        kind: "reaction",
        messageId: "m1",
        emoji: "👍",
        removed: false,
      },
    });
    // "m1" = 08 6d 31; "👍" = 10 f0 9f 91 8d.
    expect(Buffer.from(encoded).toString("hex")).toBe(
      `${HEAD}04086d3110f09f918d`,
    );
  });

  test("a removal is tag 5 with the same fields", () => {
    const encoded = Buffer.from(`${HEAD}05086d3110f09f918d`, "hex");
    expect(decodeMessage(encoded).content).toEqual({
      kind: "reaction",
      messageId: "m1",
      emoji: "👍",
      removed: true,
    });
  });

  test("leaving is tag 13 with nothing after it, and contact added is tag 3", () => {
    const encoded = encodeMessage({
      id: "r1",
      timestampMs: 0x0102030405060708n,
      content: { kind: "left" },
    });
    expect(Buffer.from(encoded).toString("hex")).toBe(`${HEAD}0d`);
    expect(decodeMessage(Buffer.from(`${HEAD}03`, "hex")).content).toEqual({
      kind: "contact-added",
    });
    expect(() => decodeMessage(Buffer.from(`${HEAD}0d00`, "hex"))).toThrow();
  });
});

describe("attachments, in the apps' byte layout", () => {
  const HEAD = "0872310807060504030201" + "00";
  const IMAGE =
    "0f" + // RichText
    "01" +
    "086869" + // Some("hi")
    "01" +
    "04" + // Some(one attachment)
    "00" + // FileVariant::P2PMixnet
    "80" +
    "11".repeat(32) + // identifier
    "80" +
    "22".repeat(32) + // claim ticket
    "00" +
    "1c7773733a2f2f6e" + // NodeEndpoint::WssUrl("wss://n")
    "01" +
    "24696d6167652f706e67" +
    "0a000000" + // Image: "image/png", 10 bytes
    "03000000" +
    "04000000" +
    "00"; // 3 × 4, no thumbnail

  test("an image is rich text whose attachment names the HOP entry, ticket, node and size", () => {
    const content = {
      kind: "rich" as const,
      text: "hi",
      attachments: [
        {
          identifier: new Uint8Array(32).fill(0x11),
          ticket: new Uint8Array(32).fill(0x22),
          node: "wss://n",
          meta: {
            kind: "image" as const,
            mimeType: "image/png",
            size: 10,
            width: 3,
            height: 4,
            thumbnail: null,
          },
        },
      ],
    };
    const encoded = encodeMessage({
      id: "r1",
      timestampMs: 0x0102030405060708n,
      content,
    });
    expect(Buffer.from(encoded).toString("hex")).toBe(`${HEAD}${IMAGE}`);
    expect(decodeMessage(encoded).content).toEqual(content);
  });

  test("diagnostics get the session data with the ticket zeroed, and nothing else changed", () => {
    const message = Buffer.from(`${HEAD}${IMAGE}`, "hex");
    const plaintext = encodeSessionData({
      kind: "request",
      requestId: "q",
      messages: [message],
    });
    const redacted = withoutTickets(plaintext);
    const expected = Buffer.from(plaintext)
      .toString("hex")
      .replace("22".repeat(32), "00".repeat(32));
    expect(Buffer.from(redacted).toString("hex")).toBe(expected);
    const plain = encodeSessionData({
      kind: "request",
      requestId: "q",
      messages: [Buffer.from(`${HEAD}0d`, "hex")],
    });
    expect(withoutTickets(plain)).toBe(plain);
  });
});
