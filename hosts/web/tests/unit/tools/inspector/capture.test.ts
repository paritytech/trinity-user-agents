import { describe, expect, test } from "bun:test";
import { bytesToHex } from "@parity/truapi/scale";
import { mnemonicToEntropy } from "@scure/bip39";
import { wordlist } from "@scure/bip39/wordlists/english.js";
import {
  CAPTURE_ITEM_BYTES,
  CAPTURE_ITEM_LIMIT,
  PayloadCapture,
} from "../../../../src/chat/capture.js";
import { bytes, concat, text, u64 } from "../../../../src/chat/codec.js";
import {
  aeadKey,
  cryptoRandom,
  type RandomBytes,
} from "../../../../src/chat/crypto.js";
import {
  ChatDiagnostics,
  type ChatEvent,
} from "../../../../src/chat/diagnostics.js";
import type { ChainDirectory } from "../../../../src/chat/directory.js";
import { ChatEngine } from "../../../../src/chat/engine.js";
import { localNovaIdentity } from "../../../../src/chat/identity.js";
import {
  decodePlaintextLayer,
  decodeStatementLayer,
} from "../../../../src/chat/inspect.js";
import {
  ChatKeys,
  deriveChatSecret,
  deriveIdentitySecret,
  miniSecret,
} from "../../../../src/chat/keys.js";
import { MemoryStatementRpc } from "../../../../fixtures/nova/memory-rpc.js";
import {
  encodeSessionData,
  sealSessionPlaintext,
  sessionTopic,
  unsealSession,
} from "../../../../src/chat/protocol.js";
import {
  decodeStatement,
  encodeSigned,
  expiryAt,
  statementHash,
} from "../../../../src/chat/statement.js";
import { StatementStore } from "../../../../src/chat/statement-store.js";
import { MemoryStorage } from "../../../support/test-support.js";

/** Synthetic phrases from the BIP-39 test vectors. They hold nothing. */
const ALICE =
  "legal winner thank year wave sausage worth useful legal winner thank yellow";
const BOB =
  "letter advice cage absurd amount doctor acoustic avoid letter advice cage above";
const entropy = (phrase: string) => mnemonicToEntropy(phrase, wordlist);
const keys = (phrase: string) => ChatKeys.fromEntropy(entropy(phrase), "paseo");
const settle = () => new Promise((resolve) => setTimeout(resolve, 30));
const hex = (value: Uint8Array) => bytesToHex(value).slice(2);

function directory(
  people: { label: string; keys: ChatKeys }[],
): ChainDirectory {
  const find = (account: Uint8Array) =>
    people.find((person) => hex(person.keys.identity) === hex(account));
  return {
    async names(account) {
      const person = find(account);
      return person === undefined
        ? null
        : {
            lite: { label: person.label, chatKey: person.keys.chatPublic },
            full: null,
          };
    },
    async consumer() {
      return null;
    },
    async owners(label) {
      const person = people.find((candidate) => candidate.label === label);
      return { assetHub: person?.keys.identity ?? null, people: null };
    },
    async hasAllowance() {
      return true;
    },
  };
}

/**
 * Alice and Bob over one in-memory JSON-RPC statement store, each through a
 * real {@link StatementStore}. Only Bob's side is captured. Every 32-byte
 * random value either engine draws is kept, so a test can look for the
 * ephemeral secrets of sealed requests.
 */
function exchange() {
  const rpc = new MemoryStatementRpc();
  const alice = keys(ALICE);
  const bob = keys(BOB);
  const chain = directory([
    { label: "alice.01", keys: alice },
    { label: "bob.02", keys: bob },
  ]);
  const drawn: Uint8Array[] = [];
  const random: RandomBytes = (length) => {
    const value = cryptoRandom(length);
    if (length === 32) drawn.push(value.slice());
    return value;
  };
  const diagnostics = new ChatDiagnostics();
  const engine = (chatKeys: ChatKeys, captured: boolean) =>
    new ChatEngine({
      identity: localNovaIdentity(chatKeys, random),
      openStore: async () =>
        new StatementStore(
          rpc.connect(),
          captured ? diagnostics.payloads.connectionTap() : undefined,
        ),
      directory: chain,
      storage: new MemoryStorage(),
      recordKey: "chat",
      writes: () => true,
      ...(captured
        ? { diagnostics: diagnostics.record, payloads: diagnostics.payloads }
        : {}),
    });
  return {
    rpc,
    alice,
    bob,
    drawn,
    diagnostics,
    aliceEngine: engine(alice, false),
    bobEngine: engine(bob, true),
  };
}

/** Submit a session statement from Alice to Bob with `plaintext` as its data, sealed or not. */
async function aliceSends(
  rpc: MemoryStatementRpc,
  alice: ChatKeys,
  bob: ChatKeys,
  data: Uint8Array,
): Promise<Uint8Array> {
  const topic = sessionTopic(
    alice.agree(bob.chatPublic),
    alice.identity,
    bob.identity,
  );
  // No channel, so the store keeps each of these instead of replacing one with the next.
  const encoded = encodeSigned(
    { expiry: expiryAt(Date.now()), channel: null, topics: [topic], data },
    alice.identity,
    (payload) => alice.sign(payload),
  );
  const store = new StatementStore(rpc.connect());
  await store.submit(encoded);
  store.close();
  await settle();
  return encoded;
}

const sessionEvents = (events: readonly ChatEvent[]) =>
  events.filter(
    (event): event is Extract<ChatEvent, { type: "session" }> =>
      event.type === "session",
  );

describe("payload capture", () => {
  test("keeps what travelled at each boundary, decrypts it, and holds no key", async () => {
    const { rpc, alice, bob, drawn, diagnostics, aliceEngine, bobEngine } =
      exchange();
    diagnostics.payloads.setLevel("decrypted");
    await Promise.all([aliceEngine.start(), bobEngine.start()]);
    await aliceEngine.startChat("bob.02", "secret welcome words");
    await settle();
    await bobEngine.accept(bobEngine.snapshot().requests[0].id);
    await settle();
    await aliceEngine.send(
      aliceEngine.snapshot().contacts[0].account,
      "secret body text",
    );
    await settle();

    // A push token and a device change, as the app sends them, then two
    // statements that do not read: one that does not decrypt, one whose
    // decrypted bytes are not session data.
    const shared = alice.agree(bob.chatPublic);
    const token = Uint8Array.of(0xde, 0xad, 0xbe, 0xef, 0x01, 0x02, 0x03, 0x04);
    const deviceAccount = new Uint8Array(32).fill(3);
    const deviceKey = new Uint8Array(32).fill(4);
    const head = (id: string, kind: number) =>
      concat(text(id), u64(9n), Uint8Array.of(0, kind));
    const controlPlaintext = encodeSessionData({
      kind: "request",
      requestId: "control-request",
      messages: [
        concat(head("c1", 1), bytes(token), Uint8Array.of(1)),
        concat(head("c2", 17), bytes(deviceAccount), bytes(deviceKey)),
        concat(head("c3", 99), Uint8Array.of(7, 7)),
      ],
    });
    const control = await aliceSends(
      rpc,
      alice,
      bob,
      sealSessionPlaintext(shared, controlPlaintext, cryptoRandom),
    );
    const garbled = await aliceSends(
      rpc,
      alice,
      bob,
      new Uint8Array(40).fill(9),
    );
    const notSession = await aliceSends(
      rpc,
      alice,
      bob,
      sealSessionPlaintext(shared, Uint8Array.of(9, 1, 2), cryptoRandom),
    );

    // A reconnect: the store delivers what it holds again.
    bobEngine.stop();
    await bobEngine.start();
    await settle();
    bobEngine.stop();
    aliceEngine.stop();

    const payloads = diagnostics.payloads;
    const events = diagnostics.list();
    const controlHash = bytesToHex(statementHash(control));

    // The statement is kept as the exact bytes Alice submitted, and the frame
    // that delivered it holds them as hex.
    const captured = payloads.statementFor(controlHash)!;
    expect(captured).toMatchObject({ direction: "in", stream: "sessions" });
    expect(hex(captured.raw)).toBe(hex(control));
    const frames = payloads.framesFor(controlHash);
    expect(frames.length).toBeGreaterThanOrEqual(2);
    for (const frame of frames) {
      expect(frame.direction).toBe("in");
      expect(frame.text).toContain(hex(control));
    }
    // The plaintext is what decryption returns, and it reads to its values.
    expect(hex(captured.plaintext!)).toBe(
      hex(unsealSession(shared, decodeStatement(control).data)),
    );
    expect(hex(captured.plaintext!)).toBe(hex(controlPlaintext));
    const controlRead = decodePlaintextLayer(captured.plaintext!, "sessions");
    expect(controlRead.protocol).toMatchObject({
      index: 0,
      requestId: { utf8: "control-request" },
      messages: {
        count: 3,
        items: [
          {
            message: {
              versioned: {
                content: {
                  index: 1,
                  token: { length: token.length, bytes: bytesToHex(token) },
                  platform: 1,
                },
              },
            },
          },
          {
            message: {
              versioned: {
                content: {
                  index: 17,
                  statementAccountId: { bytes: bytesToHex(deviceAccount) },
                  encryptionPublicKey: { bytes: bytesToHex(deviceKey) },
                },
              },
            },
          },
          { message: { versioned: { content: { index: 99 } } } },
        ],
      },
    });
    expect(controlRead.annotations).toMatchObject({
      "$.messages.items[0].message.versioned.content.platform": { name: "IOS" },
      "$.messages.items[2].message": {
        complete: false,
        stopped: { kind: "not read", unread: "0x0707" },
      },
    });

    // The first delivery and the replay after the reconnect are two arrivals.
    const arrivals = sessionEvents(events)
      .filter((event) => event.statement === controlHash)
      .map((event) => event.arrival);
    expect(arrivals).toEqual([1, 2]);

    // What does not decrypt is kept raw, with the reason and no plaintext.
    const garbledCapture = payloads.statementFor(
      bytesToHex(statementHash(garbled)),
    )!;
    expect(garbledCapture.plaintext).toBeNull();
    expect(garbledCapture.plaintextGap).toContain("could not decrypt");
    expect(events).toContainEqual(
      expect.objectContaining({
        type: "set-aside",
        statement: bytesToHex(statementHash(garbled)),
      }),
    );
    // What decrypts and does not read keeps its plaintext and the reader's error.
    const notSessionCapture = payloads.statementFor(
      bytesToHex(statementHash(notSession)),
    )!;
    expect(hex(notSessionCapture.plaintext!)).toBe("090102");
    expect(
      decodePlaintextLayer(notSessionCapture.plaintext!, "sessions"),
    ).toMatchObject({
      protocol: { index: 9 },
      annotations: {
        $: {
          complete: false,
          stopped: {
            kind: "error",
            reason: "index 9 is not in StructuredStatementData",
            unread: "0x0102",
          },
        },
      },
    });

    // Bob's acknowledgement: the bytes he submitted, the frame that sent them
    // and the store's answer, and the response he sealed.
    const ack = events.find(
      (event): event is Extract<ChatEvent, { type: "submit" }> =>
        event.type === "submit" && event.purpose === "ack",
    )!;
    const ackCapture = payloads.statementFor(ack.statement!)!;
    expect(ackCapture.direction).toBe("out");
    const ackFrames = payloads.framesFor(ack.statement!);
    expect(ackFrames.map((frame) => [frame.direction, frame.method])).toEqual([
      ["out", "statement_submit"],
      ["in", null],
    ]);
    expect(ackFrames[0].text).toContain(hex(ackCapture.raw));
    expect(ackFrames[1].rpcId).toBe(ackFrames[0].rpcId);
    expect(
      decodePlaintextLayer(ackCapture.plaintext!, "sessions"),
    ).toMatchObject({
      protocol: { index: 1, responseCode: 0 },
      annotations: { "$.responseCode": { name: "acknowledged" } },
    });
    const ackStatement = decodeStatementLayer(ackCapture.raw, "sessions");
    expect(ackStatement.annotations.$).toMatchObject({
      hash: ack.statement,
      complete: true,
    });
    expect(
      (ackStatement.protocol.fields as { items: unknown[] }).items[0],
    ).toEqual({
      index: 0,
      proof: {
        index: 0,
        signature: expect.any(String),
        signer: bytesToHex(bob.identity),
      },
    });

    // The request Alice sent: decrypted with Bob's chat key, welcome and all.
    const request = events.find((event) => event.type === "request")!;
    const requestCapture = payloads.statementFor(
      (request as { statement: string }).statement,
    )!;
    expect(
      decodePlaintextLayer(requestCapture.plaintext!, "requests"),
    ).toMatchObject({
      protocol: {
        message: {
          content: {
            index: 0,
            pushToken: { option: 0 },
            welcomeMessage: {
              option: 1,
              value: { text: { value: { utf8: "secret welcome words" } } },
            },
          },
        },
        proof: { signer: bytesToHex(alice.identity) },
      },
      annotations: { "$.message.content.index": { name: "V1" } },
    });

    // No key, secret or key derived from one is held anywhere.
    const { frames: allFrames, statements: allStatements } =
      payloads.snapshot();
    const held = [
      ...allFrames.map((frame) => frame.text),
      ...allStatements.flatMap((statement) => [
        hex(statement.raw),
        statement.plaintext === null ? "" : hex(statement.plaintext),
      ]),
    ]
      .join("\n")
      .toLowerCase();
    expect(held).toContain(hex(token));
    for (const [phrase] of [[ALICE], [BOB]]) {
      expect(held).not.toContain(hex(entropy(phrase)));
      expect(held).not.toContain(hex(miniSecret(entropy(phrase))));
      expect(held).not.toContain(hex(deriveChatSecret(entropy(phrase))));
      expect(held).not.toContain(
        hex(deriveIdentitySecret(entropy(phrase), "paseo")),
      );
    }
    expect(held).not.toContain(hex(shared));
    expect(held).not.toContain(hex(aeadKey(shared)));
    expect(drawn.length).toBeGreaterThan(0);
    for (const secret of drawn) expect(held).not.toContain(hex(secret));
  });

  test("the wire level keeps no plaintext, and off keeps nothing", async () => {
    const { diagnostics, aliceEngine, bobEngine } = exchange();
    diagnostics.payloads.setLevel("wire");
    await Promise.all([aliceEngine.start(), bobEngine.start()]);
    await aliceEngine.startChat("bob.02", "secret welcome words");
    await settle();
    const usage = diagnostics.payloads.usage();
    expect(usage.statements).toBeGreaterThan(0);
    expect(usage.frames).toBeGreaterThan(0);
    const request = diagnostics
      .list()
      .find((event) => event.type === "request")!;
    const captured = diagnostics.payloads.statementFor(
      (request as { statement: string }).statement,
    )!;
    expect(captured.plaintext).toBeNull();
    expect(captured.plaintextGap).toContain("decrypted capture is off");
    const { frames, statements } = diagnostics.payloads.snapshot();
    expect(statements.every((statement) => statement.plaintext === null)).toBe(
      true,
    );
    const welcome = hex(new TextEncoder().encode("secret welcome words"));
    for (const held of [
      ...frames.map((frame) => frame.text),
      ...statements.map((statement) => hex(statement.raw)),
    ])
      expect(held).not.toContain(welcome);

    diagnostics.payloads.setLevel("off");
    expect(diagnostics.payloads.usage()).toMatchObject({
      frames: 0,
      statements: 0,
      bytes: 0,
    });
    await bobEngine.accept(bobEngine.snapshot().requests[0].id);
    await settle();
    expect(diagnostics.payloads.usage()).toMatchObject({
      frames: 0,
      statements: 0,
    });
    // The events still name the statements, so they can be followed.
    expect(
      diagnostics
        .list()
        .some(
          (event) => event.type === "submit" && event.statement !== undefined,
        ),
    ).toBe(true);
    aliceEngine.stop();
    bobEngine.stop();
  });

  test("a session change drops payloads and turns capture off; Clear keeps the level", () => {
    const diagnostics = new ChatDiagnostics();
    const encoded = encodeSigned(
      { expiry: 1n, channel: null, topics: [], data: Uint8Array.of(1) },
      keys(ALICE).identity,
      (payload) => keys(ALICE).sign(payload),
    );
    diagnostics.record({ type: "lifecycle", state: "starting" });
    diagnostics.payloads.setLevel("decrypted");
    expect(diagnostics.captureStart()).toBe(2);
    diagnostics.payloads.statement("in", "sessions", encoded);
    diagnostics.forget();
    expect(diagnostics.payloads.level).toBe("decrypted");
    expect(diagnostics.payloads.usage().statements).toBe(0);
    diagnostics.payloads.statement("in", "sessions", encoded);
    diagnostics.clear();
    expect(diagnostics.payloads.level).toBe("off");
    expect(diagnostics.payloads.usage().statements).toBe(0);
    expect(diagnostics.captureStart()).toBeNull();
  });

  test("lowering the level to wire drops plaintext already held", () => {
    const payloads = new PayloadCapture();
    payloads.setLevel("decrypted");
    const encoded = Uint8Array.of(4, 0, 0);
    payloads.statement("in", "sessions", encoded);
    const hash = bytesToHex(statementHash(encoded));
    payloads.plaintext(hash, Uint8Array.of(1, 2, 3));
    expect(payloads.statementFor(hash)!.plaintext).toEqual(
      Uint8Array.of(1, 2, 3),
    );
    payloads.setLevel("wire");
    expect(payloads.statementFor(hash)).toMatchObject({
      plaintext: null,
      plaintextGap: "decrypted capture was turned off",
    });
    expect(payloads.usage().bytes).toBe(3);
  });

  test("plaintext attaches only to a statement it holds", () => {
    const payloads = new PayloadCapture();
    payloads.setLevel("decrypted");
    payloads.plaintext(`0x${"ab".repeat(32)}`, Uint8Array.of(1));
    expect(payloads.usage()).toMatchObject({ statements: 0, bytes: 0 });
  });

  test("stays within its byte budget and item limit, and counts what it drops", () => {
    const payloads = new PayloadCapture(1000);
    payloads.setLevel("wire");
    for (let index = 0; index < 5; index++)
      payloads.statement("in", "sessions", new Uint8Array(300).fill(index));
    expect(payloads.usage()).toMatchObject({ statements: 3, bytes: 900 });
    expect(payloads.losses()).toEqual({ evicted: 2, truncated: 0 });

    const wide = new PayloadCapture(Number.MAX_SAFE_INTEGER);
    wide.setLevel("wire");
    const tap = wide.connectionTap();
    tap("in", "x".repeat(CAPTURE_ITEM_BYTES + 5));
    expect(wide.losses().truncated).toBe(1);
    for (let index = 0; index < CAPTURE_ITEM_LIMIT + 4; index++)
      tap("out", `{"id":${index}}`);
    expect(wide.usage().frames).toBe(CAPTURE_ITEM_LIMIT);
  });

  test("links a submission's frame to its answer by JSON-RPC id, and a notification to each statement it carries", () => {
    const payloads = new PayloadCapture();
    payloads.setLevel("wire");
    const first = Uint8Array.of(1, 1);
    const second = Uint8Array.of(2, 2);
    const firstHash = bytesToHex(statementHash(first));
    const secondHash = bytesToHex(statementHash(second));
    const tap = payloads.connectionTap();
    const other = payloads.connectionTap();
    tap(
      "out",
      JSON.stringify({
        jsonrpc: "2.0",
        id: 7,
        method: "statement_submit",
        params: [bytesToHex(first)],
      }),
    );
    // The same id on another connection answers something else.
    other("in", JSON.stringify({ jsonrpc: "2.0", id: 7, result: {} }));
    tap(
      "in",
      JSON.stringify({ jsonrpc: "2.0", id: 7, result: { status: "new" } }),
    );
    tap(
      "in",
      JSON.stringify({
        jsonrpc: "2.0",
        method: "statement_statement",
        params: {
          subscription: "s1",
          result: {
            event: "newStatements",
            data: { statements: [bytesToHex(first), bytesToHex(second)] },
          },
        },
      }),
    );
    expect(payloads.framesFor(firstHash).map((frame) => frame.seq)).toEqual([
      1, 3, 4,
    ]);
    expect(
      payloads.framesFor(secondHash).map((frame) => frame.statements),
    ).toEqual([[firstHash, secondHash]]);
  });
});
