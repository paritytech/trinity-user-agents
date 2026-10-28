import { describe, expect, test } from "bun:test";
import { bytesToHex } from "@parity/truapi/scale";
import { mnemonicToEntropy } from "@scure/bip39";
import { wordlist } from "@scure/bip39/wordlists/english.js";
import {
  bytes,
  concat,
  option,
  text,
  u64,
} from "../../../../src/chat/codec.js";
import {
  decodeMessageLayer,
  decodePlaintextLayer,
  decodeStatementLayer,
} from "../../../../src/chat/inspect.js";
import { ChatKeys } from "../../../../src/chat/keys.js";
import {
  decodeMessage,
  encodeSessionData,
} from "../../../../src/chat/protocol.js";
import {
  decodeStatement,
  encodeSigned,
  statementHash,
} from "../../../../src/chat/statement.js";

const keys = ChatKeys.fromEntropy(
  mnemonicToEntropy(
    "legal winner thank year wave sausage worth useful legal winner thank yellow",
    wordlist,
  ),
  "paseo",
);
const TIME = 1_760_000_000_000n;
const head = (id: string, kind: number) =>
  concat(text(id), u64(TIME), Uint8Array.of(0, kind));
/** What every message's head reads to: the protocol fields and their annotations. */
const headFields = (id: string) => ({
  id: { length: id.length, utf8: id },
  timestamp: TIME.toString(),
});
const headNotes = {
  "$.timestamp": { iso: "2025-10-09T08:53:20.000Z" },
  "$.versioned.index": { name: "V1" },
};

describe("decoded layers keep protocol fields apart from annotations", () => {
  test("a push token: the platform index is a protocol field, its name an annotation", () => {
    const token = Uint8Array.of(0xfe, 0xed, 0xfa, 0xce);
    const encoded = concat(head("t1", 1), bytes(token), Uint8Array.of(1));
    expect(decodeMessageLayer(encoded)).toEqual({
      protocol: {
        ...headFields("t1"),
        versioned: {
          index: 0,
          content: {
            index: 1,
            token: { length: 4, bytes: "0xfeedface" },
            platform: 1,
          },
        },
      },
      annotations: {
        ...headNotes,
        "$.versioned.content.index": { name: "Token" },
        "$.versioned.content": {
          sensitive: "a push token: it addresses this device's notifications",
        },
        "$.versioned.content.platform": { name: "IOS" },
        $: {
          schema: "ChatMessageStatement",
          size: encoded.length,
          complete: true,
        },
      },
    });
  });

  test("a device's keys are Vec<u8> on the wire, so their length prefixes are protocol fields", () => {
    const account = new Uint8Array(32).fill(0x31);
    const key = new Uint8Array(32).fill(0x42);
    const encoded = concat(head("d1", 17), bytes(account), bytes(key));
    const decoded = decodeMessageLayer(encoded);
    expect(decoded.protocol.versioned).toEqual({
      index: 0,
      content: {
        index: 17,
        statementAccountId: { length: 32, bytes: bytesToHex(account) },
        encryptionPublicKey: { length: 32, bytes: bytesToHex(key) },
      },
    });
    expect(decoded.annotations["$.versioned.content"]).toEqual({
      public: "public keys the peer announces for a device",
    });
    // The engine's own reader gives the same keys.
    expect(decodeMessage(encoded).content).toMatchObject({
      statementAccount: account,
      encryptionKey: key,
    });
  });

  test("a device chat acceptance holds the device as two fixed 32-byte keys", () => {
    const account = new Uint8Array(32).fill(5);
    const key = new Uint8Array(32).fill(6);
    expect(
      decodeMessageLayer(concat(head("a", 20), text("r9"), account, key))
        .protocol.versioned,
    ).toEqual({
      index: 0,
      content: {
        index: 20,
        requestId: { length: 2, utf8: "r9" },
        device: {
          statementAccountId: bytesToHex(account),
          encryptionPublicKey: bytesToHex(key),
        },
      },
    });
  });

  test("rich text is read as it is laid out, not as plain text", () => {
    const encoded = concat(
      head("r", 15),
      Uint8Array.of(1),
      text("hi"),
      Uint8Array.of(0),
    );
    expect(decodeMessageLayer(encoded).protocol.versioned).toEqual({
      index: 0,
      content: {
        index: 15,
        text: { option: 1, value: { length: 2, utf8: "hi" } },
        attachments: { option: 0 },
      },
    });
    // A reply keeps the message it answers and its own rich text.
    expect(
      decodeMessageLayer(concat(head("p", 7), text("m0"), Uint8Array.of(0, 0)))
        .protocol.versioned,
    ).toEqual({
      index: 0,
      content: {
        index: 7,
        messageId: { length: 2, utf8: "m0" },
        ownContent: { text: { option: 0 }, attachments: { option: 0 } },
      },
    });
  });

  test("an attachment is read field by field, and its ticket is marked as replaced", () => {
    const zeros = new Uint8Array(32);
    const encoded = concat(
      head("f", 15),
      Uint8Array.of(0, 1, 4, 0),
      bytes(new Uint8Array(32).fill(0x11)),
      bytes(zeros),
      Uint8Array.of(0),
      text("wss://n"),
      Uint8Array.of(0),
      text("text/plain"),
      Uint8Array.of(5, 0, 0, 0),
    );
    const decoded = decodeMessageLayer(encoded);
    expect(decoded.annotations.$.complete).not.toBe(false);
    const content = decoded.protocol.versioned as {
      content: { attachments: { value: { items: Record<string, unknown>[] } } };
    };
    expect(content.content.attachments.value.items[0]).toMatchObject({
      index: 0,
      claimTicket: { length: 32, bytes: bytesToHex(zeros) },
      nodeEndpoint: { index: 0, url: { utf8: "wss://n" } },
      meta: { index: 0, mimeType: { utf8: "text/plain" }, fileSize: 5 },
    });
    expect(
      decoded.annotations[
        "$.versioned.content.attachments.value.items[0].claimTicket"
      ].sensitive,
    ).toContain("replaces it with zeros");
  });

  test("an unknown kind stops reading, names where, and keeps the bytes it did not read", () => {
    const encoded = concat(head("u", 99), Uint8Array.of(7, 7));
    const decoded = decodeMessageLayer(encoded);
    expect(decoded.protocol.versioned).toEqual({
      index: 0,
      content: { index: 99 },
    });
    expect(decoded.annotations["$.versioned.content.index"]).toBeUndefined();
    expect(decoded.annotations.$).toEqual({
      schema: "ChatMessageStatement",
      size: encoded.length,
      complete: false,
      stopped: {
        path: "$.versioned.content.index",
        at: encoded.length - 2,
        within: "$",
        kind: "not read",
        reason: "content index 99 is not read by this host",
        unread: "0x0707",
      },
    });
  });

  test("a malformed message keeps what was read, and the error", () => {
    const short = concat(
      head("m1", 17),
      bytes(new Uint8Array(31)),
      Uint8Array.of(0x80),
    );
    const decoded = decodeMessageLayer(short);
    expect(decoded.protocol.versioned).toEqual({
      index: 0,
      content: {
        index: 17,
        statementAccountId: {
          length: 31,
          bytes: bytesToHex(new Uint8Array(31)),
        },
        encryptionPublicKey: { length: 32 },
      },
    });
    expect(decoded.annotations.$).toMatchObject({
      complete: false,
      stopped: {
        path: "$.versioned.content.encryptionPublicKey.bytes",
        kind: "error",
        reason: "the record ends early",
        unread: "0x",
      },
    });
    expect(decodeMessageLayer(Uint8Array.of(8, 1)).annotations.$).toMatchObject(
      {
        complete: false,
        stopped: { path: "$.id.utf8", at: 1, unread: "0x01" },
      },
    );
    expect(
      decodeMessageLayer(concat(head("x", 0), text("a"), Uint8Array.of(5)))
        .annotations.$,
    ).toMatchObject({
      complete: false,
      stopped: {
        path: "$",
        reason: "1 byte after the last field",
        unread: "0x05",
      },
    });
  });

  test("a platform outside TokenPlatform keeps its index and is an error", () => {
    const decoded = decodeMessageLayer(
      concat(head("t", 1), bytes(Uint8Array.of(9)), Uint8Array.of(3)),
    );
    expect(decoded.protocol.versioned).toMatchObject({
      content: { index: 1, platform: 3 },
    });
    expect(decoded.annotations["$.versioned.content.platform"]).toBeUndefined();
    expect(decoded.annotations.$).toMatchObject({
      stopped: {
        kind: "error",
        reason: "platform index 3 is not in TokenPlatform",
      },
    });
  });

  test("a session request's messages are each a length prefix and a message, with offsets as annotations", () => {
    const first = concat(head("a", 0), text("hi"));
    const second = concat(head("b", 14), text("q0"));
    const plaintext = encodeSessionData({
      kind: "request",
      requestId: "q",
      messages: [first, second],
    });
    const decoded = decodePlaintextLayer(plaintext, "sessions");
    expect(decoded.protocol).toEqual({
      index: 0,
      requestId: { length: 1, utf8: "q" },
      messages: {
        count: 2,
        items: [
          {
            length: first.length,
            message: {
              ...headFields("a"),
              versioned: {
                index: 0,
                content: { index: 0, text: { length: 2, utf8: "hi" } },
              },
            },
          },
          {
            length: second.length,
            message: {
              ...headFields("b"),
              versioned: {
                index: 0,
                content: { index: 14, requestId: { length: 2, utf8: "q0" } },
              },
            },
          },
        ],
      },
    });
    expect(decoded.annotations["$.index"]).toEqual({ name: "Request" });
    expect(decoded.annotations["$.messages.items[1]"]).toEqual({
      // index, requestId (prefix and byte), count, then the first item's prefix and bytes.
      offset: 1 + 2 + 1 + 1 + first.length,
      bytes: bytesToHex(second),
    });
    expect(
      decoded.annotations[
        "$.messages.items[1].message.versioned.content.index"
      ],
    ).toEqual({ name: "ChatAccepted" });
    expect(decoded.annotations.$).toEqual({
      schema: "StructuredStatementData",
      size: plaintext.length,
      complete: true,
    });
  });

  test("an unreadable message inside a request marks the layer incomplete and leaves the others read", () => {
    const plaintext = encodeSessionData({
      kind: "request",
      requestId: "q",
      messages: [concat(head("u", 99)), concat(head("a", 0), text("ok"))],
    });
    const decoded = decodePlaintextLayer(plaintext, "sessions");
    expect(decoded.annotations.$).toMatchObject({
      complete: false,
      incompleteAt: ["$.messages.items[0].message"],
    });
    expect(decoded.annotations["$.messages.items[0].message"]).toMatchObject({
      stopped: {
        path: "$.messages.items[0].message.versioned.content.index",
        within: "$.messages.items[0].message",
      },
    });
    expect(
      (decoded.protocol.messages as { items: { message: unknown }[] }).items[1]
        .message,
    ).toMatchObject({ versioned: { content: { text: { utf8: "ok" } } } });
  });

  test("a response's code is a protocol field; that it acknowledges is an annotation", () => {
    const decoded = decodePlaintextLayer(
      encodeSessionData({ kind: "response", requestId: "r", code: 0 }),
      "sessions",
    );
    expect(decoded.protocol).toEqual({
      index: 1,
      requestId: { length: 1, utf8: "r" },
      responseCode: 0,
    });
    expect(decoded.annotations["$.responseCode"]).toEqual({
      name: "acknowledged",
    });
    expect(decoded.annotations["$.index"]).toEqual({ name: "Response" });
  });

  test("a multi-device envelope keeps its index and its unread bytes", () => {
    expect(decodePlaintextLayer(Uint8Array.of(2, 5, 6), "sessions")).toEqual({
      protocol: { index: 2 },
      annotations: {
        "$.index": { name: "MultiRequest" },
        $: {
          schema: "StructuredStatementData",
          size: 3,
          complete: false,
          stopped: {
            path: "$.index",
            at: 1,
            within: "$",
            kind: "not read",
            reason: "multi-device envelopes are not opened by this host",
            unread: "0x0506",
          },
        },
      },
    });
  });

  test("a request's version index, option tags and proof are read as they are", () => {
    const message = concat(
      text("r1"),
      u64(5n),
      Uint8Array.of(0),
      option(Uint8Array.of(9, 9), (token) =>
        concat(bytes(token), Uint8Array.of(0)),
      ),
      Uint8Array.of(1, 1),
      text("hello"),
      Uint8Array.of(0),
    );
    const signature = new Uint8Array(64).fill(5);
    const plaintext = concat(
      message,
      Uint8Array.of(0),
      signature,
      keys.identity,
    );
    const decoded = decodePlaintextLayer(plaintext, "requests");
    expect(decoded.protocol).toEqual({
      message: {
        messageId: { length: 2, utf8: "r1" },
        timestamp: "5",
        content: {
          index: 0,
          pushToken: {
            option: 1,
            value: { token: { length: 2, bytes: "0x0909" }, platform: 0 },
          },
          welcomeMessage: {
            option: 1,
            value: {
              text: { option: 1, value: { length: 5, utf8: "hello" } },
              attachments: { option: 0 },
            },
          },
        },
      },
      proof: {
        index: 0,
        signature: bytesToHex(signature),
        signer: bytesToHex(keys.identity),
      },
    });
    expect(decoded.annotations).toMatchObject({
      "$.message.content.index": { name: "V1" },
      "$.message.content.pushToken.value.platform": { name: "ANDROID" },
      "$.message.timestamp": { iso: "1970-01-01T00:00:00.005Z" },
      "$.message": { signedBytes: message.length },
      "$.proof.index": { name: "Sr25519" },
      $: { schema: "ChatRequestDecrypted", complete: true },
    });
  });

  test("a statement's fields keep their indexes, and agree with the engine's reader", () => {
    const topic = new Uint8Array(32).fill(1);
    const data = new Uint8Array(40).fill(2);
    const expiry = (0xffff_ffffn << 32n) | 42n;
    const encoded = encodeSigned(
      { expiry, channel: null, topics: [topic], data },
      keys.identity,
      (payload) => keys.sign(payload),
    );
    const engine = decodeStatement(encoded);
    const decoded = decodeStatementLayer(encoded, "sessions");
    expect(decoded.protocol).toEqual({
      fields: {
        count: 4,
        items: [
          {
            index: 0,
            proof: {
              index: 0,
              signature: bytesToHex(engine.signature),
              signer: bytesToHex(keys.identity),
            },
          },
          { index: 2, expiry: expiry.toString() },
          { index: 4, topic: bytesToHex(topic) },
          {
            index: 8,
            data: {
              length: 40,
              nonce: bytesToHex(data.subarray(0, 12)),
              ciphertext: bytesToHex(data.subarray(12, 24)),
              tag: bytesToHex(data.subarray(24)),
            },
          },
        ],
      },
    });
    expect(decoded.annotations).toMatchObject({
      "$.fields.items[0].index": { name: "AuthenticityProof" },
      "$.fields.items[0].proof.index": { name: "Sr25519" },
      "$.fields.items[1].expiry": { upper32: 0xffff_ffff, lower32: 42 },
      "$.fields.items[2].index": { name: "Topic1" },
      "$.fields.items[3].data": {
        layout: "ChaCha20-Poly1305: nonce 12 ‖ ciphertext ‖ tag 16",
        complete: true,
      },
      $: {
        hash: bytesToHex(statementHash(encoded)),
        size: encoded.length,
        complete: true,
      },
    });
    expect(
      decodeStatementLayer(Uint8Array.of(4, 9), "sessions").annotations.$,
    ).toMatchObject({
      complete: false,
      stopped: {
        path: "$.fields.items[0].index",
        reason: "field index 9 is not in the statement schema",
      },
    });
  });
});
