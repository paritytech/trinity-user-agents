import { asProductId } from "@parity/truapi";
import { describe, expect, test } from "bun:test";
import { ml_kem768_x25519 } from "@noble/post-quantum/hybrid.js";
import {
  REQUEST_SEAL,
  WIRE_VERSION,
  openReply,
  openRequest,
  replyKeyPair,
  replySuiteOf,
  requestDigest,
  requestTranscript,
  sealReply,
  sealRequest,
  type ReplySuite,
  type RequestContext,
} from "../../../src/invitations/binding.js";
import { ChatKeys } from "../../../src/invitations/chat-keys.js";
import type { Person } from "../../../src/invitations/ports.js";
import {
  XWING_SEAL_OVERHEAD,
  cryptoRandom,
  digest,
  fromHex,
  isXWingPublic,
  openXWingSealed,
  sealTo,
  sealToXWing,
  toHex,
  utf8,
  x25519Pair,
  xwingPair,
} from "../../../src/invitations/sealing.js";
import {
  DAY_INBOX_ROUTE,
  NOTIFICATION_DATA_MAX_BYTES,
} from "../../../src/invitations/statement-delivery.js";
import { WalletIdentityAuth } from "../../../src/invitations/wallet-auth.js";
import vectors from "./xwing-draft-11-vectors.json";

/**
 * The reply leg's seal: X-Wing against the draft's own vectors and failing
 * closed, the reply-key suite each request names and keeps, and the sizes a
 * notification statement can carry under each suite. The vectors are Appendix C of
 * https://www.ietf.org/archive/id/draft-connolly-cfrg-xwing-kem-11.txt,
 * public test data.
 */

const PURPOSE = "test/xwing-seal";
const fixed = (bytes: Uint8Array) => () => bytes;

describe("X-Wing, as the reply seal uses it", () => {
  test("matches the draft-11 test vectors", () => {
    for (const vector of vectors) {
      const pair = xwingPair(fromHex(`0x${vector.seed}`));
      const sealed = sealToXWing(
        pair.public,
        utf8("reply"),
        PURPOSE,
        fixed(fromHex(`0x${vector.eseed}`)),
      );
      expect({
        publicKey: toHex(pair.public).slice(2),
        cipherText: toHex(sealed.subarray(1, 1121)).slice(2),
        sharedSecret: toHex(
          ml_kem768_x25519.decapsulate(fromHex(`0x${vector.ct}`), pair.secret),
        ).slice(2),
        opened: openXWingSealed(pair.secret, sealed, PURPOSE),
      }).toEqual({
        publicKey: vector.pk,
        cipherText: vector.ct,
        sharedSecret: vector.ss,
        opened: utf8("reply"),
      });
    }
  });

  test("opens only with the recipient's secret and purpose, and fails closed on any changed byte", () => {
    const recipient = xwingPair(cryptoRandom(32));
    const sealed = sealToXWing(
      recipient.public,
      utf8("reply"),
      PURPOSE,
      cryptoRandom,
    );
    const flipped = (at: number) => {
      const copy = sealed.slice();
      copy[at]! ^= 1;
      return copy;
    };
    const tries = {
      sealed: [recipient.secret, sealed, PURPOSE],
      otherSecret: [xwingPair(cryptoRandom(32)).secret, sealed, PURPOSE],
      otherPurpose: [recipient.secret, sealed, "test/other"],
      suite: [recipient.secret, flipped(0), PURPOSE],
      mlKemCiphertext: [recipient.secret, flipped(1), PURPOSE],
      x25519Ciphertext: [recipient.secret, flipped(1 + 1088), PURPOSE],
      body: [recipient.secret, flipped(1121), PURPOSE],
      tag: [recipient.secret, flipped(sealed.length - 1), PURPOSE],
      truncated: [recipient.secret, sealed.subarray(0, 1136), PURPOSE],
      classical: [
        recipient.secret,
        sealTo(
          x25519Pair(recipient.secret).public,
          utf8("reply"),
          PURPOSE,
          cryptoRandom,
        ),
        PURPOSE,
      ],
    } satisfies Record<string, Parameters<typeof openXWingSealed>>;
    expect(
      Object.fromEntries(
        Object.entries(tries).map(([name, [secret, bytes, purpose]]) => [
          name,
          openXWingSealed(secret, bytes, purpose),
        ]),
      ),
    ).toEqual({
      sealed: utf8("reply"),
      otherSecret: null,
      otherPurpose: null,
      suite: null,
      mlKemCiphertext: null,
      x25519Ciphertext: null,
      body: null,
      tag: null,
      truncated: null,
      classical: null,
    });
    expect(sealed.length).toBe(XWING_SEAL_OVERHEAD + 5);
  });

  test("accepts only a key encapsulation takes: 1,216 bytes passing the ML-KEM key check", () => {
    const valid = xwingPair(cryptoRandom(32)).public;
    // Coefficients of 4095 are out of range for ML-KEM's modulus.
    const outOfRange = valid.slice();
    outOfRange.fill(0xff, 0, 1152);
    expect([
      isXWingPublic(valid),
      isXWingPublic(valid.subarray(0, 32)),
      isXWingPublic(outOfRange),
    ]).toEqual([true, false, false]);
  });
});

const keysOf = (id: string) =>
  ChatKeys.fromEntropy(digest("xwing-test-entropy", id), "test");
const personOf = (id: string): Person => {
  const keys = keysOf(id);
  return {
    id,
    account: toHex(keys.identity),
    name: id,
    handle: `${id}.example`,
    identity: {
      signingPublic: toHex(keys.identity),
      agreementPublic: toHex(keys.chatPublic),
    },
  };
};
const ADA = personOf("ada");
const LIN = personOf("lin");
const authOf = (id: string) => new WalletIdentityAuth(keysOf(id));
const contacts = (signingPublic: string) =>
  [ADA, LIN].find((who) => who.identity.signingPublic === signingPublic);
const PRODUCT = { id: asProductId("chat.example"), name: "Example Chat" };
const CREATED_AT = Date.UTC(2026, 9, 21, 12, 0, 0);

function request(
  payload: string,
  replyKey: Uint8Array,
  purpose = "Chat?",
): { context: RequestContext; sealed: string } {
  return sealRequest(
    authOf("ada"),
    LIN,
    {
      product: PRODUCT,
      purpose,
      requestId: toHex(cryptoRandom(16)),
      createdAt: CREATED_AT,
      returnRoute: DAY_INBOX_ROUTE,
      replyKey: toHex(replyKey),
    },
    payload,
    cryptoRandom,
  );
}

/**
 * A request Ada's host authenticates and seals with `context` as it is, as an
 * earlier or other host would. `sent` is what the envelope says, when it is
 * not `context`.
 */
function signedAsIs(
  context: RequestContext,
  payload: string,
  sent: unknown = context,
): string {
  const auth = authOf("ada");
  const evidence = auth.evidence(
    "Request",
    requestTranscript(context, payload),
    LIN,
  );
  return toHex(
    sealTo(
      fromHex(LIN.identity.agreementPublic),
      utf8(JSON.stringify({ context: sent, payload, evidence })),
      REQUEST_SEAL,
      cryptoRandom,
    ),
  );
}

/** Lin's host's reply to `sent`, sealed as the request's suite says. */
function replyTo(sent: { context: RequestContext }, payload: string) {
  return sealReply(
    authOf("lin"),
    ADA,
    {
      context: sent.context,
      digest: requestDigest(sent.context, "0x0102"),
    },
    payload,
    cryptoRandom,
  );
}

describe("the reply leg, by reply-key suite", () => {
  for (const suite of ["xwing", "x25519"] as const)
    test(`a request carries Ada's ${suite} reply key; Lin needs no published key of it, and only Ada's secret opens the reply`, () => {
      const replyKey = replyKeyPair(suite, cryptoRandom(32));
      const sent = request("0x0102", replyKey.public);
      const opened = openRequest(authOf("lin"), sent.sealed, contacts);
      if (!opened.ok) throw new Error(opened.reason);
      expect([
        opened.context.version,
        replySuiteOf(opened.context.replyKey),
      ]).toEqual([WIRE_VERSION, suite]);
      const reply = replyTo(opened, "0x03");
      const expected = (replySecret: Uint8Array) => ({
        peer: LIN,
        productId: PRODUCT.id,
        requestId: sent.context.requestId,
        requestDigest: requestDigest(sent.context, "0x0102"),
        replyKey: sent.context.replyKey,
        replySecret,
      });
      expect([
        openReply(authOf("ada"), expected(replyKey.secret), reply),
        openReply(authOf("ada"), expected(cryptoRandom(32)), reply),
      ]).toEqual([
        { ok: true, payload: "0x03" },
        {
          ok: false,
          reason: "not sealed to this request",
          sealedElsewhere: true,
        },
      ]);
    });

  // A reply opens only with the suite of the request's key: no fallback.
  test("a reply does not open with the other suite, even with the matching secret", () => {
    const secret = cryptoRandom(32);
    const opensAs = (sent: ReplySuite, tried: ReplySuite) => {
      const made = request("0x0102", replyKeyPair(sent, secret).public);
      return openReply(
        authOf("ada"),
        {
          peer: LIN,
          productId: PRODUCT.id,
          requestId: made.context.requestId,
          requestDigest: requestDigest(made.context, "0x0102"),
          replyKey: toHex(replyKeyPair(tried, secret).public),
          replySecret: secret,
        },
        replyTo(made, "0x03"),
      ).ok;
    };
    expect([
      opensAs("xwing", "xwing"),
      opensAs("xwing", "x25519"),
      opensAs("x25519", "x25519"),
      opensAs("x25519", "xwing"),
    ]).toEqual([true, false, true, false]);
  });

  // The 65-byte value is the chain's chat key container, never a reply key.
  test("the reply key's exact length decides its suite; any other key is refused", () => {
    const xwingKey = xwingPair(cryptoRandom(32)).public;
    const failsKeyCheck = xwingKey.slice();
    failsKeyCheck.fill(0xff, 0, 1152);
    expect(
      [
        x25519Pair(cryptoRandom(32)).public,
        xwingKey,
        cryptoRandom(33),
        new Uint8Array(65),
        xwingKey.subarray(0, 1215),
        failsKeyCheck,
      ].map((key) => replySuiteOf(toHex(key))),
    ).toEqual(["x25519", "xwing", null, null, null, null]);

    const { context } = request("0x0102", xwingKey);
    const signedWith = (replyKey: Uint8Array) =>
      openRequest(
        authOf("lin"),
        signedAsIs({ ...context, replyKey: toHex(replyKey) }, "0x0102"),
        contacts,
      );
    const refused = (reason: string) => ({
      ok: false as const,
      reason,
      sealedElsewhere: false,
    });
    expect([
      signedWith(new Uint8Array(65)),
      signedWith(failsKeyCheck),
      // Signed with the X-Wing key, sent with a classical one.
      openRequest(
        authOf("lin"),
        signedAsIs(context, "0x0102", {
          ...context,
          replyKey: toHex(x25519Pair(cryptoRandom(32)).public),
        }),
        contacts,
      ),
    ]).toEqual([
      refused("not a request"),
      refused("not a request"),
      refused("its evidence does not check"),
    ]);
  });
});

describe("sizes against one notification statement", () => {
  /** Bytes sealed, of the request and its reply, for a product payload of `bytes` both ways. */
  function sizes(suite: ReplySuite, bytes: number, purpose?: string) {
    const payload = toHex(new Uint8Array(bytes).fill(7));
    const sent = request(
      payload,
      replyKeyPair(suite, cryptoRandom(32)).public,
      purpose,
    );
    const reply = sealReply(
      authOf("lin"),
      ADA,
      { context: sent.context, digest: requestDigest(sent.context, payload) },
      payload,
      cryptoRandom,
    );
    return {
      request: fromHex(sent.sealed).length,
      reply: fromHex(reply).length,
    };
  }

  // Payloads travel as hex in the envelope, two bytes for each.
  test("X-Wing: a request takes 3,153 bytes and a reply 1,772, plus twice the payload", () => {
    expect([sizes("xwing", 1), sizes("xwing", 1000)]).toEqual([
      { request: 3155, reply: 1774 },
      { request: 5153, reply: 3772 },
    ]);
    expect(sizes("xwing", 1, "x".repeat(280)).request).toBe(3430);
  });

  test("X25519: a request takes 785 bytes and a reply 683, plus twice the payload", () => {
    expect([sizes("x25519", 1), sizes("x25519", 1000)]).toEqual([
      { request: 787, reply: 685 },
      { request: 2785, reply: 2683 },
    ]);
  });

  test("the largest payload that fits one notification, by suite", () => {
    const limit = NOTIFICATION_DATA_MAX_BYTES;
    expect(limit).toBe(10_240);
    const fits = (
      suite: ReplySuite,
      part: "request" | "reply",
      bytes: number,
    ) => [
      sizes(suite, bytes)[part] <= limit,
      sizes(suite, bytes + 1)[part] <= limit,
    ];
    expect({
      xwingRequest: fits("xwing", "request", 3543),
      xwingReply: fits("xwing", "reply", 4234),
      x25519Request: fits("x25519", "request", 4727),
      x25519Reply: fits("x25519", "reply", 4778),
    }).toEqual({
      xwingRequest: [true, false],
      xwingReply: [true, false],
      x25519Request: [true, false],
      x25519Reply: [true, false],
    });
  });
});
