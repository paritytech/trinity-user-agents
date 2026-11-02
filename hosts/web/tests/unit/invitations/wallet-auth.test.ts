import { describe, expect, test } from "bun:test";
import * as sr25519 from "@scure/sr25519";
import type { ChainDirectory } from "../../../src/platform/chain-directory.js";
import { concat, text } from "../../../src/platform/codec.js";
import { keyedBlake2b256 } from "../../../src/platform/crypto.js";
import {
  openReply,
  openRequest,
  requestDigest,
  sealReply,
  sealRequest,
  type RequestContext,
} from "../../../src/invitations/binding.js";
import type { Person } from "../../../src/invitations/ports.js";
import {
  ownIdentityProblem,
  publishedChatKey,
} from "../../../src/invitations/published-keys.js";
import {
  cryptoRandom,
  digest,
  fromHex,
  sealTo,
  toHex,
  x25519Agree,
  x25519Pair,
  xwingPair,
} from "../../../src/invitations/sealing.js";
import {
  WalletIdentityAuth,
  type WalletKeys,
} from "../../../src/invitations/wallet-auth.js";
import {
  TEST_USERS,
  personOf,
  testUserDirectory,
  testUserKeys,
  type TestUser,
} from "../../../fixtures/invitations/test-users.js";
import { SUFFIX } from "../../support/host-handles.js";

const [ALICE, BOB, CAROL] = TEST_USERS as [TestUser, TestUser, TestUser];

const keysOf = (user: TestUser, suffix = SUFFIX) => testUserKeys(user, suffix);
const authOf = (user: TestUser, suffix = SUFFIX) =>
  new WalletIdentityAuth(keysOf(user, suffix));
const peer = (user: TestUser, suffix = SUFFIX): Person =>
  personOf(user, suffix, keysOf(user, suffix).chatPublic);

/** Wallet keys over seeds, as the reference profile's vector was made. */
function seedKeys(signingSeed: Uint8Array, agreementSecret: Uint8Array) {
  const signing = sr25519.secretFromSeed(signingSeed);
  const agreement = x25519Pair(agreementSecret);
  return {
    identity: sr25519.getPublicKey(signing),
    chatPublic: agreement.public,
    sign: (message: Uint8Array) => sr25519.sign(signing, message),
    agree: (other: Uint8Array) => x25519Agree(agreement.secret, other),
  } satisfies WalletKeys;
}

describe("the wallet host proof", () => {
  // Made by the reference profile before it moved here: the move keeps its
  // bytes, so evidence from either verifies with the other.
  test("is the reference profile, byte for byte", () => {
    const a = seedKeys(
      digest("vector/signing", "a"),
      digest("vector/agreement", "a"),
    );
    const b = seedKeys(
      digest("vector/signing", "b"),
      digest("vector/agreement", "b"),
    );
    const transcript = digest("vector/transcript", "one");
    const personB: Person = {
      id: "b",
      account: toHex(b.identity),
      name: "b",
      handle: "b",
      identity: {
        signingPublic: toHex(b.identity),
        agreementPublic: toHex(b.chatPublic),
      },
    };
    const personA: Person = {
      ...personB,
      id: "a",
      identity: {
        signingPublic: toHex(a.identity),
        agreementPublic: toHex(a.chatPublic),
      },
    };
    expect([toHex(a.identity), toHex(a.chatPublic), toHex(transcript)]).toEqual(
      [
        "0x3a5a296c6b8d21994f299120010a14fa03b0b02b9815a3ca18954696f5e66f27",
        "0xa5658fe13eed793d9b414615e02ffbdbb1a6900013612ef225f97b87c6634874",
        "0x051944659937b6a85b6dc41fceaeaebb6385bbc259395b35d9313a6f678f3464",
      ],
    );
    const pinned = {
      kind: "Request" as const,
      identityMac:
        "0x643cb06019a2a1fd1b976bfbcfd56771248fef14d98c07c367d4a0c9f02d3c90",
      messageSignature:
        "0x2eda10934bcd61459faf7b5f322ad15e17541de2d4250c30c7a837f5831f556b2c018496be5ec688add4d277d101f263b4cccb96b117f4a5dc7b7e535a6ede87",
    };
    expect(
      new WalletIdentityAuth(a).evidence("Request", transcript, personB)
        .identityMac,
    ).toBe(pinned.identityMac);
    expect(
      new WalletIdentityAuth(b).check("Request", transcript, pinned, personA),
    ).toBe(true);
  });

  test("checks only for its peer, kind, transcript and direction, with the wallet's keys", () => {
    const transcript = digest("transcript", "one");
    const evidence = authOf(ALICE).evidence("Request", transcript, peer(BOB));
    expect(evidence).toEqual({
      kind: "Request",
      identityMac: expect.stringMatching(/^0x[0-9a-f]{64}$/),
      messageSignature: expect.stringMatching(/^0x[0-9a-f]{128}$/),
    });
    const bob = authOf(BOB);
    expect([
      bob.check("Request", transcript, evidence, peer(ALICE)),
      bob.check("Reply", transcript, evidence, peer(ALICE)),
      bob.check("Request", digest("transcript", "two"), evidence, peer(ALICE)),
      authOf(CAROL).check("Request", transcript, evidence, peer(ALICE)),
      // Alice's own evidence is not Bob's: the MAC is shared, the signature is not.
      authOf(ALICE).check("Request", transcript, evidence, peer(BOB)),
    ]).toEqual([true, false, false, false, false]);
  });

  test("the MAC and the signature each matter: a rotated chat key or another network's account fails", () => {
    const transcript = digest("transcript", "one");
    const evidence = authOf(ALICE).evidence("Reply", transcript, peer(BOB));
    const bob = authOf(BOB);
    const rotated: Person = {
      ...peer(ALICE),
      identity: {
        ...peer(ALICE).identity,
        agreementPublic: toHex(x25519Pair(digest("rotated", "alice")).public),
      },
    };
    // Same chat key, the identity account of another network: the MAC holds, the signature does not.
    const elsewhere = {
      ...peer(ALICE),
      identity: {
        ...peer(ALICE).identity,
        signingPublic: toHex(keysOf(ALICE, "other").identity),
      },
    };
    expect([
      bob.check("Reply", transcript, evidence, rotated),
      bob.check("Reply", transcript, evidence, elsewhere),
    ]).toEqual([false, false]);
  });

  // The host proof uses the identity account and chat key native chat uses,
  // so it must not double as a native chat identity proof.
  test("is not a native chat identity proof", () => {
    const keys = keysOf(ALICE);
    const evidence = authOf(ALICE).evidence(
      "Request",
      digest("transcript", "one"),
      peer(BOB),
    );
    const native = nativeIdentityProof(
      keys.agree(keysOf(BOB).chatPublic),
      keys.identity,
      keys.identity,
    );
    expect(evidence.identityMac).not.toBe(toHex(native));
  });

  test("opens only what was sealed to the wallet's chat key", () => {
    const sealed = sealTo(
      keysOf(BOB).chatPublic,
      new Uint8Array([1, 2, 3]),
      "purpose",
      cryptoRandom,
    );
    expect([
      authOf(BOB).openSealed(sealed, "purpose"),
      authOf(BOB).openSealed(sealed, "other"),
      authOf(CAROL).openSealed(sealed, "purpose"),
    ]).toEqual([new Uint8Array([1, 2, 3]), null, null]);
  });
});

/**
 * A version 2 native chat request's identity proof, as the Polkadot apps
 * make it over the same identity and chat keys: the identity and the signing
 * account, keyed by the secret the two chat keys agree.
 */
const nativeIdentityProof = (
  secret: Uint8Array,
  identity: Uint8Array,
  signer: Uint8Array,
): Uint8Array =>
  keyedBlake2b256(secret, concat(identity, signer, text("mds-chat-request")));

describe("a request and its reply under wallet keys", () => {
  const PRODUCT = { id: "chat.paseo", name: "Chat" };
  const PAYLOAD = "0x0102";
  const replyKey = xwingPair(digest("reply", "one"));
  const request = (overrides: Partial<RequestContext> = {}) =>
    sealRequest(
      authOf(ALICE),
      peer(BOB),
      {
        product: PRODUCT,
        purpose: "Chat?",
        requestId: toHex(digest("id", "one").slice(0, 16)),
        createdAt: 1,
        returnRoute: "route:1",
        replyKey: toHex(replyKey.public),
        ...overrides,
      },
      PAYLOAD,
      cryptoRandom,
    );
  const contacts = (people: Person[]) => (signingPublic: string) =>
    people.find((person) => person.identity.signingPublic === signingPublic);

  test("goes from Alice's wallet to Bob's and back, authenticated both ways", () => {
    const sent = request();
    const opened = openRequest(
      authOf(BOB),
      sent.sealed,
      contacts([peer(ALICE)]),
    );
    if (!opened.ok) throw new Error(opened.reason);
    expect([opened.peer.id, opened.payload]).toEqual(["alice", PAYLOAD]);
    const digestOf = requestDigest(opened.context, opened.payload);
    const reply = sealReply(
      authOf(BOB),
      peer(ALICE),
      { context: opened.context, digest: digestOf },
      "0x03",
      cryptoRandom,
    );
    expect(
      openReply(
        authOf(ALICE),
        {
          peer: peer(BOB),
          productId: PRODUCT.id,
          requestId: sent.context.requestId,
          requestDigest: requestDigest(sent.context, PAYLOAD),
          replyKey: toHex(replyKey.public),
          replySecret: replyKey.secret,
        },
        reply,
      ),
    ).toEqual({ ok: true, payload: "0x03" });
  });

  test("a request from someone who is not a contact, or checked under another key, is refused", () => {
    const sent = request();
    expect([
      openRequest(authOf(BOB), sent.sealed, contacts([])),
      openRequest(authOf(CAROL), sent.sealed, contacts([peer(ALICE)])),
      openRequest(
        authOf(BOB),
        sent.sealed,
        contacts([
          {
            ...peer(ALICE),
            identity: {
              ...peer(ALICE).identity,
              agreementPublic: toHex(keysOf(CAROL).chatPublic),
            },
          },
        ]),
      ),
    ]).toEqual([
      {
        ok: false,
        reason: "the sender is not a contact",
        sealedElsewhere: false,
      },
      { ok: false, reason: "not sealed to this host", sealedElsewhere: true },
      {
        ok: false,
        reason: "its evidence does not check",
        sealedElsewhere: false,
      },
    ]);
  });

  test("every field of the request is bound by Alice's evidence", () => {
    const sent = request();
    const original = JSON.parse(
      new TextDecoder().decode(
        authOf(BOB).openSealed(
          fromHex(sent.sealed),
          "truapi-web-host/request-seal/1",
        )!,
      ),
    );
    const changes: Record<string, unknown>[] = [
      { product: { id: "other.paseo", name: "Chat" } },
      { purpose: "Pay?" },
      { requestId: toHex(digest("id", "two").slice(0, 16)) },
      { createdAt: 3 },
      { returnRoute: "route:2" },
      { replyKey: toHex(xwingPair(digest("reply", "two")).public) },
    ];
    for (const change of [...changes, { payload: "0x09" }]) {
      const changed =
        "payload" in change
          ? { ...original, payload: change.payload }
          : { ...original, context: { ...original.context, ...change } };
      const resealed = toHex(
        sealTo(
          keysOf(BOB).chatPublic,
          new TextEncoder().encode(JSON.stringify(changed)),
          "truapi-web-host/request-seal/1",
          cryptoRandom,
        ),
      );
      expect({
        change,
        opened: openRequest(authOf(BOB), resealed, contacts([peer(ALICE)])),
      }).toEqual({
        change,
        opened: {
          ok: false,
          reason: "its evidence does not check",
          sealedElsewhere: false,
        },
      });
    }
  });
});

describe("published chat keys", () => {
  const directory = testUserDirectory(SUFFIX);

  test("a wallet proves its user only when the chain publishes its own chat key", async () => {
    const elsewhere: ChainDirectory = {
      ...directory,
      consumer: async () => ({
        liteUsername: "alice",
        fullUsername: null,
        chatKey: keysOf(BOB).chatPublic,
      }),
    };
    const absent: ChainDirectory = { ...directory, consumer: async () => null };
    expect([
      await ownIdentityProblem(directory, keysOf(ALICE)),
      await ownIdentityProblem(elsewhere, keysOf(ALICE)),
      await ownIdentityProblem(absent, keysOf(ALICE)),
    ]).toEqual([
      null,
      {
        reason:
          "The chat key published for your identity account is not this wallet's.",
        again: false,
      },
      {
        reason:
          "Your identity account: it has no chat identity on chain. To chat, register a username for this wallet in the Polkadot app, which publishes its chat key, then sign in again.",
        again: false,
      },
    ]);
  });

  test("a read that fails, or a record with another key type, reaches nobody", async () => {
    const failing: ChainDirectory = {
      ...directory,
      consumer: async () => {
        throw new Error("offline");
      },
    };
    const otherType: ChainDirectory = {
      ...directory,
      consumer: async () => ({
        liteUsername: "bob",
        fullUsername: null,
        chatKey: null,
      }),
    };
    expect([
      await publishedChatKey(failing, keysOf(BOB).identity),
      await publishedChatKey(otherType, keysOf(BOB).identity),
    ]).toEqual([
      {
        ok: false,
        reason: "its chat identity could not be read: Error: offline",
        unread: true,
      },
      { ok: false, reason: "it publishes no X25519 chat key", unread: false },
    ]);
  });
});
