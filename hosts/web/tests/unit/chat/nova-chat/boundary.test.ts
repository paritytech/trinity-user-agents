import { describe, expect, test } from "bun:test";
import * as sr25519 from "@scure/sr25519";
import {
  Reader,
  concat,
  equalBytes,
  text as scaleText,
  u64,
} from "../../../../src/platform/codec.js";
import {
  keyedBlake2b256,
  x25519PublicKey,
} from "../../../../src/platform/crypto.js";
import type { SignedStatement } from "../../../../src/platform/statement.js";
import {
  aeadKey,
  agree,
  open,
  seal,
} from "../../../../src/chat/nova-chat/crypto.js";
import { deriveDeviceSecret } from "../../../../src/chat/nova-chat/boundary.js";
import {
  claimPeriod,
  protocolDay,
  requestDayTopic,
  requestFullTopic,
  requestSessionTopic,
} from "../../../../src/chat/nova-chat/topics.js";
import {
  decodeRequestEnvelope,
  encodeMessage,
  encodeRequestDecrypted,
  encodeRequestEnvelope,
  encodeRequestMessage,
  identityProofPayload,
  readRequestMessage,
  requestProofPayload,
} from "../../../../src/chat/nova-chat/wire.js";
import {
  A,
  A_CHAT,
  A_ID,
  B_CHAT,
  B_CHAT_SECRET,
  B_DEVICE,
  B_ID,
  B_IDENTITY_SECRET,
  B_PEER,
  B_PER_CHAT_SECRET,
  NOTIFICATION_SECRET,
  bAcceptance,
  bDeviceStatement,
  bIdentityStatement,
  boundaryA,
  chatKeys,
  establishedA,
  hex,
  seed,
  signAs,
  text,
} from "./fixtures.js";

/**
 * Nova Chat's host key boundary against statements built here from the
 * official apps' layouts (Android `fc186861db`, iOS `22407b1058`): what it
 * must accept, and every check that must refuse. These are synthetic keys and
 * bytes; they show the checks work, not that a real app takes part.
 */

const T0 = 1_800_000_300_000;

/** B's V2 request to A, signed by `signer` and sealed to `to`. */
function bRequest(
  options: {
    requestId?: string;
    signer?: Uint8Array;
    signedFor?: Uint8Array;
    proofSigner?: Uint8Array;
    to?: Uint8Array;
    version?: number;
    deviceKey?: Uint8Array;
  } = {},
): SignedStatement {
  const signerSecret = options.signer ?? B_IDENTITY_SECRET;
  const signer = sr25519.getPublicKey(signerSecret);
  const requestId = options.requestId ?? "b-req";
  let message = encodeRequestMessage({
    messageId: requestId,
    timestamp: 1_800_000_050_000n,
    identityAccount: B_ID,
    identityProof: keyedBlake2b256(
      agree(B_CHAT_SECRET, A_CHAT),
      identityProofPayload(B_ID, options.proofSigner ?? signer),
    ),
    deviceEncryptionKey: options.deviceKey ?? B_DEVICE,
    welcome: "hi",
  });
  if (options.version !== undefined) {
    message = message.slice();
    message[1 + requestId.length + 8] = options.version;
  }
  const signature = sr25519.sign(
    signerSecret,
    requestProofPayload(message, options.signedFor ?? A_ID),
  );
  const ephemeral = seed(0x56);
  return signAs(NOTIFICATION_SECRET, {
    expiry: 1n,
    channel: null,
    topics: [requestDayTopic(A_ID, 0n), requestFullTopic(A_ID)],
    data: encodeRequestEnvelope(
      x25519PublicKey(ephemeral),
      seal(
        aeadKey(agree(ephemeral, options.to ?? A_CHAT)),
        encodeRequestDecrypted(message, signature, signer),
      ),
    ),
  });
}

describe("device route: bound by topic, not signer, with every other check", () => {
  test("a peer statement signed by its per-chat account opens", async () => {
    const host = await establishedA();
    expect(
      host.openFrame(
        B_PEER,
        signAs(B_PER_CHAT_SECRET, bDeviceStatement([text("m1", "hi")], "f1")),
      ),
    ).toMatchObject({
      kind: "Request",
      requestId: "f1",
      messages: [{ id: "m1", content: { kind: "Text", text: "hi" } }],
    });
  });

  test("the same statement signed by the peer's device account opens too", async () => {
    const host = await establishedA();
    expect(
      host.openFrame(
        B_PEER,
        signAs(B_IDENTITY_SECRET, bDeviceStatement([text("m1", "hi")], "f1")),
      ),
    ).toMatchObject({ kind: "Request" });
  });

  test("a bad statement signature is refused", async () => {
    const host = await establishedA();
    const statement = signAs(
      B_PER_CHAT_SECRET,
      bDeviceStatement([text("m1", "hi")], "f1"),
    );
    statement.signature[0]! ^= 1;
    expect(host.openFrame(B_PEER, statement)).toEqual({
      refused: "statement proof",
    });
  });

  test("a valid signature over changed ciphertext is refused", async () => {
    const host = await establishedA();
    const statement = bDeviceStatement([text("m1", "hi")], "f1");
    statement.data = statement.data.slice();
    statement.data[statement.data.length - 1]! ^= 1;
    expect(
      host.openFrame(B_PEER, signAs(B_PER_CHAT_SECRET, statement)),
    ).toEqual({ refused: "decrypt or layout" });
  });

  test("a valid statement on another topic is refused, the identity route included", async () => {
    const host = await establishedA();
    const statement = bDeviceStatement([text("m1", "hi")], "f1");
    expect(
      host.openFrame(
        B_PEER,
        signAs(B_PER_CHAT_SECRET, { ...statement, topics: [seed(0x99)] }),
      ),
    ).toEqual({ refused: "topic" });
    expect(
      host.openFrame(
        B_PEER,
        signAs(B_PER_CHAT_SECRET, {
          ...statement,
          topics: bIdentityStatement([], "x").topics,
        }),
      ),
    ).toEqual({ refused: "topic" });
  });

  test("an envelope with no entry for this device is refused", async () => {
    const host = await establishedA();
    expect(
      host.openFrame(
        B_PEER,
        signAs(
          B_PER_CHAT_SECRET,
          bDeviceStatement([text("m1", "hi")], "f1", seed(0xee)),
        ),
      ),
    ).toEqual({ refused: "no entry for this device" });
  });

  test("kind 20 on the device route is no acceptance and reaches the worker as a stub", async () => {
    const host = await establishedA();
    const control = encodeMessage({
      id: "c1",
      timestamp: 1n,
      content: {
        kind: "DeviceChatAccepted",
        requestId: "x",
        device: { statementAccount: seed(1), encryptionKey: B_DEVICE },
      },
    });
    expect(
      host.openFrame(
        B_PEER,
        signAs(B_PER_CHAT_SECRET, bDeviceStatement([control], "f2")),
      ),
    ).toMatchObject({
      messages: [{ id: "c1", content: { kind: "Unsupported", index: 20 } }],
    });
    expect(host.routes(B_PEER)).toMatchObject({ incoming: { length: 1 } });
  });

  test("an unknown kind (Seen, 22) is a stub only after its proof checks", async () => {
    const host = await establishedA();
    const seen = concat(
      scaleText("s1"),
      u64(1n),
      Uint8Array.of(0, 22),
      scaleText("m0"),
      u64(1n),
    );
    const statement = signAs(B_PER_CHAT_SECRET, bDeviceStatement([seen], "f3"));
    expect(host.openFrame(B_PEER, statement)).toMatchObject({
      messages: [{ id: "s1", content: { kind: "Unsupported", index: 22 } }],
    });
    statement.signature[5]! ^= 1;
    expect(host.openFrame(B_PEER, statement)).toEqual({
      refused: "statement proof",
    });
  });

  test("the worker cannot seal control content", async () => {
    const host = await establishedA();
    const control = encodeMessage({
      id: "c",
      timestamp: 1n,
      content: {
        kind: "DeviceChatAccepted",
        requestId: "x",
        device: { statementAccount: seed(1), encryptionKey: B_DEVICE },
      },
    });
    expect(
      host.sealFrame(
        B_PEER,
        { kind: "Request", requestId: "r", messages: [control] },
        1n,
      ),
    ).toEqual({ refused: "content DeviceChatAccepted" });
  });

  test("nothing opens or seals for someone without an established chat", () => {
    const host = boundaryA();
    expect(host.routes(B_PEER)).toEqual({ refused: "not an accepted contact" });
    expect(
      host.sealFrame(
        B_PEER,
        { kind: "Request", requestId: "r", messages: [text("m", "x")] },
        1n,
      ),
    ).toEqual({ refused: "not an accepted contact" });
  });

  test("a restart keeps the chat: the device route opens from the saved peers", async () => {
    const host = await establishedA();
    const again = boundaryA();
    again.restore(host.snapshot());
    expect(again.snapshot()).toBe(host.snapshot());
    expect(
      again.openFrame(
        B_PEER,
        signAs(B_PER_CHAT_SECRET, bDeviceStatement([text("m1", "hi")], "f1")),
      ),
    ).toMatchObject({ kind: "Request" });
  });
});

describe("small-order keys", () => {
  const smallOrder = new Uint8Array(32);
  test("an agreement with an all-zero output throws", () => {
    expect(() => agree(A.chatSecret, smallOrder)).toThrow();
  });
  test("an acceptance naming a small-order device key is refused", async () => {
    const host = boundaryA();
    await host.request(B_ID, {
      requestId: "a-req",
      nowMs: T0,
      expiry: 1n,
      welcome: null,
    });
    expect(
      host.openAcceptance(
        B_PEER,
        signAs(B_PER_CHAT_SECRET, bAcceptance("a-req", smallOrder)),
      ),
    ).toEqual({ refused: "device key" });
  });
  test("a request naming a small-order device key is refused", async () => {
    expect(
      await boundaryA().openRequest(bRequest({ deviceKey: smallOrder })),
    ).toEqual({ refused: "device key" });
  });
});

describe("acceptance on the identity route", () => {
  test("an acceptance of another request id is refused", async () => {
    const host = boundaryA();
    await host.request(B_ID, {
      requestId: "a-req",
      nowMs: T0,
      expiry: 1n,
      welcome: null,
    });
    expect(
      host.openAcceptance(
        B_PEER,
        signAs(B_PER_CHAT_SECRET, bAcceptance("other")),
      ),
    ).toEqual({ refused: "another request" });
  });
  test("kind 20 with another message beside it is refused", async () => {
    const host = boundaryA();
    await host.request(B_ID, {
      requestId: "a-req",
      nowMs: T0,
      expiry: 1n,
      welcome: null,
    });
    const statement = bIdentityStatement(
      [
        {
          id: "b-acc",
          timestamp: 1n,
          content: {
            kind: "DeviceChatAccepted",
            requestId: "a-req",
            device: { statementAccount: B_ID, encryptionKey: B_DEVICE },
          },
        },
        { id: "t", timestamp: 1n, content: { kind: "Text", text: "x" } },
      ],
      "b-acc",
    );
    expect(
      host.openAcceptance(B_PEER, signAs(B_PER_CHAT_SECRET, statement)),
    ).toEqual({ refused: "not an acceptance" });
  });
});

describe("requests in", () => {
  test("a valid request opens once; the same request again is a duplicate", async () => {
    const host = boundaryA();
    expect(await host.openRequest(bRequest())).toMatchObject({
      peer: B_PEER,
      requestId: "b-req",
      welcome: "hi",
      duplicate: false,
    });
    expect(await host.openRequest(bRequest())).toMatchObject({
      duplicate: true,
    });
  });
  test("an inner signature by a key the identity proof does not name is refused", async () => {
    expect(
      await boundaryA().openRequest(
        bRequest({
          signer: sr25519.secretFromSeed(seed(0x31)),
          proofSigner: B_ID,
        }),
      ),
    ).toEqual({ refused: "identity proof" });
  });
  test("an inner signature over another acceptor is refused", async () => {
    expect(
      await boundaryA().openRequest(bRequest({ signedFor: seed(0x43) })),
    ).toEqual({ refused: "request signature" });
  });
  test("an identity proof not made with the sender's on-chain chat key is refused", async () => {
    const host = boundaryA(chatKeys([[B_PEER, x25519PublicKey(seed(0x41))]]));
    expect(await host.openRequest(bRequest())).toEqual({
      refused: "identity proof",
    });
  });
  test("a sender with no chat key on chain is refused", async () => {
    const host = boundaryA(async () => null);
    expect(await host.openRequest(bRequest())).toEqual({
      refused: "the sender has no chat key on chain",
    });
  });
  test("a request sealed to another chat key is not for us", async () => {
    expect(
      await boundaryA().openRequest(
        bRequest({ to: x25519PublicKey(seed(0x42)) }),
      ),
    ).toEqual({ refused: "not for us" });
  });
  test("a version 1 request is refused, as iOS refuses it", async () => {
    expect(
      await boundaryA().openRequest(bRequest({ version: 0 })),
    ).toMatchObject({
      refused: expect.stringContaining("version 0"),
    });
  });
  test("a request id with '/' is refused", async () => {
    expect(
      await boundaryA().openRequest(bRequest({ requestId: "b/req" })),
    ).toEqual({
      refused: "request id",
    });
  });
  test("a request crossing ours is refused, not merged", async () => {
    const host = boundaryA();
    await host.request(B_ID, {
      requestId: "a-req",
      nowMs: T0,
      expiry: 1n,
      welcome: null,
    });
    expect(await host.openRequest(bRequest())).toMatchObject({
      refused: expect.stringContaining("crossed"),
    });
  });
});

describe("our request", () => {
  test("B opens it, its signature covers (message, B), and its proof fits A's on-chain key", async () => {
    const host = boundaryA();
    const sent = await host.request(B_ID, {
      requestId: "a-req",
      nowMs: T0,
      expiry: 1n,
      welcome: "w",
    });
    if ("refused" in sent) throw new Error(sent.refused);
    const envelope = decodeRequestEnvelope(sent.statement.data);
    const plain = open(
      aeadKey(agree(B_CHAT_SECRET, envelope.ephemeral)),
      envelope.sealed,
    );
    const reader = new Reader(plain);
    const message = readRequestMessage(reader);
    const messageBytes = plain.subarray(0, reader.offset);
    expect(reader.u8()).toBe(0);
    const signature = reader.take(64);
    const signer = reader.take(32);
    expect(equalBytes(signer, A_ID)).toBe(true);
    expect(
      sr25519.verify(
        requestProofPayload(messageBytes, B_ID),
        signature,
        signer,
      ),
    ).toBe(true);
    expect(
      equalBytes(
        keyedBlake2b256(
          agree(B_CHAT_SECRET, A_CHAT),
          identityProofPayload(A_ID, signer),
        ),
        message.identityProof,
      ),
    ).toBe(true);
    expect(
      equalBytes(message.deviceEncryptionKey, x25519PublicKey(A.deviceSecret)),
    ).toBe(true);
    expect(sent.statement.channel).toBeNull();
    expect(sent.statement.topics.map(hex)).toEqual(
      [
        requestDayTopic(B_ID, protocolDay(T0)),
        requestFullTopic(B_ID),
        requestSessionTopic(agree(B_CHAT_SECRET, A_CHAT), A_ID, B_ID),
      ].map(hex),
    );
  });
  test("a request to someone with no chat key on chain is refused", async () => {
    expect(
      await boundaryA(async () => null).request(B_ID, {
        requestId: "a",
        nowMs: T0,
        expiry: 1n,
        welcome: null,
      }),
    ).toEqual({ refused: "they have no chat key on chain" });
  });
});

describe("keys and day counters", () => {
  test("the device key is the `chat-device` domain's, apart from the chat key", () => {
    const entropy = seed(0x61);
    expect(hex(x25519PublicKey(deriveDeviceSecret(entropy)))).not.toBe(
      hex(B_CHAT),
    );
    expect(hex(deriveDeviceSecret(entropy))).toBe(
      hex(deriveDeviceSecret(entropy)),
    );
  });
  test("request topics count days from the protocol epoch; claims from the Unix epoch", () => {
    const t = Date.UTC(2026, 10, 6, 23, 59, 59);
    expect(protocolDay(t)).toBe(356n);
    expect(claimPeriod(t)).toBe(20763);
    expect(protocolDay(t + 1000)).toBe(357n);
    expect(claimPeriod(t + 1000)).toBe(20764);
  });
});
