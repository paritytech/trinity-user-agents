import { describe, expect, test } from "bun:test";
import { readFileSync, writeFileSync } from "node:fs";
import { bytesToHex, hexToBytes } from "@parity/truapi/scale";
import { mnemonicToEntropy } from "@scure/bip39";
import { wordlist } from "@scure/bip39/wordlists/english.js";
import * as sr25519 from "@scure/sr25519";
import { bytes, concat } from "../../../../src/chat/nova/codec.js";
import {
  x25519Agree,
  type RandomBytes,
} from "../../../../src/chat/nova/crypto.js";
import { ChatEngine } from "../../../../src/chat/nova/engine.js";
import { localNovaIdentity } from "../../../../src/chat/nova/identity.js";
import { ChatKeys } from "../../../../src/chat/nova/keys.js";
import {
  identityProof,
  identityProofMatches,
  requestDayTopic,
  requestSessionTopic,
  requestTopic,
  sealRequestPlaintext,
} from "../../../../src/chat/nova/protocol.js";
import {
  decodeStatement,
  encodeSigned,
  expiryAt,
  requestDay,
} from "../../../../src/chat/nova/statement.js";
import { MemoryStorage } from "../../../support/test-support.js";
import {
  MemoryStatementStore,
  memoryDirectory,
  settle,
} from "../../../support/nova-memory.js";

/**
 * The engine against statements a second, separately written implementation
 * of the apps' wire made (`tests/unit/fixtures/nova/native-peer.py`, from the
 * apps' public Swift and Kotlin sources), and that implementation's reading
 * of the engine's statements, pinned in `native-peer.emitted.json`.
 *
 * This is not the apps' own code and no app took part. What it shows is that
 * the engine's bytes (SCALE layouts, identity proof, topics, envelopes, keys)
 * agree with a second reading of the same wire, which a round trip between
 * two copies of the engine cannot. The Python peer reads the engine's
 * statements only when it is run, not on every test run; between runs the
 * pinned bytes catch a change. The first statement (the request) holds an
 * sr25519 signature that differs on every run, so only its topics are pinned.
 * To re-check after a change:
 *
 *   NOVA_VECTORS_WRITE=1 bun test tests/unit/chat/nova/native-interop.test.ts
 *   uv run tests/unit/fixtures/nova/native-peer.py
 */

const FIXTURES = new URL("../../fixtures/nova/", import.meta.url);
const read = (name: string) =>
  JSON.parse(readFileSync(new URL(name, FIXTURES), "utf8"));
const peer = read("native-peer.json");
const WRITE = process.env.NOVA_VECTORS_WRITE === "1";

/** Synthetic phrases from the BIP-39 test vectors. They hold nothing. */
const ALICE =
  "legal winner thank year wave sausage worth useful legal winner thank yellow";
const BOB =
  "letter advice cage absurd amount doctor acoustic avoid letter advice cage above";
const keys = (phrase: string) =>
  ChatKeys.fromEntropy(mnemonicToEntropy(phrase, wordlist), "paseo");
const alice = keys(ALICE);
const bob = keys(BOB);

/** The constants the Python peer holds for the engine's side must be the engine's own. */
const A_ID =
  "0x3ac1b80910fa1a633577db96e5c77fca970a8e9b3e09b5db005e4563268a3336";
const A_CHAT =
  "0x7e65a3cffd41e39eaab65201ea848d3c50a6e7c907382a31f41ea535279d675b";
const A_DEV =
  "0xe7cc20ade0585e02c18da1030fe4c254f65d333b75ea59da05f995d27cfaa137";

/** Bytes that depend on nothing but the call order. */
function counter(): RandomBytes {
  let next = 1;
  return (length) => Uint8Array.from({ length }, () => next++ & 0xff);
}

/** A signer that is nobody's identity, as a per-request account of the apps is. */
const anonymous = sr25519.secretFromSeed(new Uint8Array(32).fill(9));
const anonymousKey = sr25519.getPublicKey(anonymous);

/** B's statement, signed by an account that is not B's identity. */
function signed(part: { topic: string; channel: string; data: string }) {
  return encodeSigned(
    {
      expiry: expiryAt(1_800_000_000_000),
      channel: hexToBytes(part.channel as `0x${string}`),
      topics: [hexToBytes(part.topic as `0x${string}`)],
      data: hexToBytes(part.data as `0x${string}`),
    },
    anonymousKey,
    (payload) => sr25519.sign(anonymous, payload),
  );
}

const capture = (encoded: Uint8Array) => {
  const statement = decodeStatement(encoded);
  return {
    topics: statement.topics.map(bytesToHex),
    channel: statement.channel === null ? null : bytesToHex(statement.channel),
    data: bytesToHex(statement.data),
  };
};

async function world(ids: string[]) {
  const store = new MemoryStatementStore();
  let clock = 1_800_000_000_000;
  const queue = [...ids];
  const engine = new ChatEngine({
    identity: localNovaIdentity(alice, counter()),
    openStore: async () => store.connect(),
    directory: memoryDirectory([
      {
        label: "alice.01",
        identity: alice.identity,
        chatKey: alice.chatPublic,
      },
      {
        label: "native.01",
        identity: bob.identity,
        chatKey: hexToBytes(peer.keys.chat),
      },
    ]),
    storage: new MemoryStorage(),
    recordKey: "chat",
    now: () => (clock += 1000),
    random: counter(),
    newId: () => queue.shift() ?? "unplanned-id",
    writes: () => true,
    retryDelayMs: 10,
  });
  await engine.start();
  await settle();
  return { store, engine };
}

const emitted: Record<string, Record<string, ReturnType<typeof capture>>> = {
  t1: {},
  t2: {},
};

describe("a native peer's wire", () => {
  test("the keys the Python peer holds for the engine are the engine's", () => {
    expect(bytesToHex(alice.identity)).toBe(A_ID);
    expect(bytesToHex(alice.chatPublic)).toBe(A_CHAT);
    expect(bytesToHex(alice.devicePublic)).toBe(A_DEV);
    expect(peer.keys.identity).toBe(bytesToHex(bob.identity));
  });

  test("the identity proof the Python peer made is the one this code makes and accepts", () => {
    const secret = x25519Agree(new Uint8Array(32).fill(3), alice.chatPublic);
    expect(bytesToHex(identityProof(secret, bob.identity, bob.identity))).toBe(
      peer.request.identityProof,
    );
    expect(
      identityProofMatches(
        alice.agree(hexToBytes(peer.keys.chat)),
        bob.identity,
        bob.identity,
        hexToBytes(peer.request.identityProof),
      ),
    ).toBe(true);
  });

  test("a request the engine sends, the peer's acceptance, and device-route messages both ways", async () => {
    const { store, engine } = await world([
      "a-req-0001",
      "a-msg-0002",
      "a-flush-0003",
    ]);
    await engine.startChat("native.01", "hello native");
    emitted.t1.request = capture(store.submitted[0]);

    // The signer of B's acceptance is not B's identity; the engine goes by topic.
    store.inject(signed(peer.acceptance));
    await settle();
    const [contact] = engine.snapshot().contacts;
    expect(contact.state).toBe("active");
    expect(contact.devices).toEqual([
      { statementAccount: peer.keys.identity, encryptionKey: peer.keys.device },
    ]);
    expect(
      contact.messages.map((message) => [message.direction, message.kind]),
    ).toEqual([
      ["out", "request"],
      ["in", "accepted"],
    ]);
    // Nobody answers an acceptance.
    expect(store.submitted).toHaveLength(1);

    await engine.send(contact.account, "hi native");
    emitted.t1.text = capture(store.submitted[1]);

    store.inject(signed(peer.response));
    await settle();
    expect(
      engine.snapshot().contacts[0].messages.find((m) => m.text === "hi native")
        ?.status,
    ).toBe("acknowledged");

    store.inject(signed(peer.text));
    await settle();
    expect(
      engine
        .snapshot()
        .contacts[0].messages.filter((m) => m.direction === "in")
        .map((m) => m.text),
    ).toEqual(["Accepted the chat request", "hello from native"]);
    emitted.t1.ack = capture(store.submitted[2]);

    // The same statement again, from another signer, changes nothing.
    const before = store.submitted.length;
    store.inject(
      encodeSigned(
        {
          ...decodeStatement(signed(peer.text)),
          expiry: expiryAt(1_800_000_001_000),
        },
        anonymousKey,
        (payload) => sr25519.sign(anonymous, payload),
      ),
    );
    await settle();
    expect(store.submitted.length).toBe(before);
    expect(
      engine
        .snapshot()
        .contacts[0].messages.filter((m) => m.text === "hello from native"),
    ).toHaveLength(1);

    // An envelope with no entry for the engine's device is set aside.
    store.inject(signed(peer.unaddressed));
    await settle();
    expect(engine.snapshot().skipped[0]).toContain("not addressed");
    expect(store.submitted.length).toBe(before);
    engine.stop();
  });

  test("a version 2 request the peer sends is verified, accepted with a device acceptance, and answered on the device route", async () => {
    const { store, engine } = await world([
      "a-acc-0001",
      "a-msg-0002",
      "a-flush-0003",
    ]);
    // B signs the request with its identity, as the app does; the statement
    // that carries it is signed by an account that is nobody's identity.
    const message = hexToBytes(peer.request.message);
    const acceptorBytes = alice.identity;
    const signature = bob.sign(concat(message, bytes(acceptorBytes)));
    const plaintext = concat(
      message,
      Uint8Array.of(0),
      signature,
      bob.identity,
    );
    const peerChat = new Uint8Array(32).fill(3);
    const at = 1_800_000_050_000;
    const request = encodeSigned(
      {
        expiry: expiryAt(at),
        channel: null,
        topics: [
          requestDayTopic(alice.identity, requestDay(at)),
          requestTopic(alice.identity),
          requestSessionTopic(
            x25519Agree(peerChat, alice.chatPublic),
            bob.identity,
            alice.identity,
          ),
        ],
        data: sealRequestPlaintext(plaintext, alice.chatPublic, counter()),
      },
      anonymousKey,
      (payload) => sr25519.sign(anonymous, payload),
    );
    store.inject(request);
    await settle();
    const [pending] = engine.snapshot().requests;
    expect(pending).toMatchObject({
      id: "b-req-0001",
      peer: bytesToHex(bob.identity),
      version: 2,
      welcome: "hi from native",
      problem: null,
      device: {
        statementAccount: peer.keys.identity,
        encryptionKey: peer.keys.device,
      },
    });

    await engine.accept("b-req-0001");
    emitted.t2.acceptance = capture(store.submitted[0]);
    expect(store.submitted).toHaveLength(1);

    store.inject(signed(peer.text));
    await settle();
    emitted.t2.ack = capture(store.submitted[1]);
    const [contact] = engine.snapshot().contacts;
    expect(contact.acceptance?.delivered).toBe(true);
    expect(
      contact.messages.find(
        (m) => m.kind === "accepted" && m.direction === "out",
      )?.status,
    ).toBe("acknowledged");

    await engine.send(contact.account, "reply");
    emitted.t2.text = capture(store.submitted[2]);
    engine.stop();
  });

  test("what the Python peer reads of the engine's statements is pinned", () => {
    if (WRITE) {
      writeFileSync(
        new URL("native-peer.emitted.json", FIXTURES),
        `${JSON.stringify(emitted, null, 1)}\n`,
      );
      return;
    }
    // The request's data holds an sr25519 signature, which differs on every
    // run, so only its topics are pinned. The statements of a session hold
    // none inside their sealed data.
    const pinned = (record: typeof emitted) => ({
      ...record,
      t1: { ...record.t1, request: { ...record.t1.request, data: null } },
    });
    expect(pinned(emitted)).toEqual(pinned(read("native-peer.emitted.json")));
  });
});
