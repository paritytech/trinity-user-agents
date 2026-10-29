import { describe, expect, test } from "bun:test";
import { bytesToHex } from "@parity/truapi/scale";
import { mnemonicToEntropy } from "@scure/bip39";
import { wordlist } from "@scure/bip39/wordlists/english.js";
import {
  concat,
  equalBytes,
  text,
  u64,
} from "../../../../src/chat/nova/codec.js";
import { cryptoRandom } from "../../../../src/chat/nova/crypto.js";
import type { ChainDirectory } from "../../../../src/chat/nova/directory.js";
import { ChatEngine } from "../../../../src/chat/nova/engine.js";
import type { ChatEventSink } from "../../../../src/chat/nova/events.js";
import {
  localNovaIdentity,
  type NovaIdentity,
} from "../../../../src/chat/nova/identity.js";
import { ChatKeys } from "../../../../src/chat/nova/keys.js";
import { openDeviceEnvelope } from "../../../../src/chat/nova/multi-device.js";
import {
  decodeMessage,
  decodeSessionData,
  encodeSessionData,
  identityProof,
  requestChannel,
  requestTopic,
  requestDayTopic,
  requestSessionTopic,
  sealRequest,
  sealSession,
  sessionTopic,
  unsealSession,
  type ChatRequest,
} from "../../../../src/chat/nova/protocol.js";
import {
  decodeStatement,
  encodeSigned,
  expiryAt,
  requestDay,
} from "../../../../src/chat/nova/statement.js";
import {
  chatRecordKey,
  loadChatRecord,
} from "../../../../src/chat/nova/store.js";
import { MemoryStorage } from "../../../support/test-support.js";
import {
  MemoryStatementStore,
  memoryDirectory,
  settle,
} from "../../../support/nova-memory.js";

/** Synthetic phrases from the BIP-39 test vectors. They hold nothing. */
const ALICE =
  "legal winner thank year wave sausage worth useful legal winner thank yellow";
const BOB =
  "letter advice cage absurd amount doctor acoustic avoid letter advice cage above";
const CAROL =
  "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";
const keys = (phrase: string) =>
  ChatKeys.fromEntropy(mnemonicToEntropy(phrase, wordlist), "paseo");
const alice = keys(ALICE);
const bob = keys(BOB);
const carol = keys(CAROL);

const directory = () =>
  memoryDirectory([
    { label: "alice.01", identity: alice.identity, chatKey: alice.chatPublic },
    { label: "bob.02", identity: bob.identity, chatKey: bob.chatPublic },
    { label: "carol.03", identity: carol.identity, chatKey: carol.chatPublic },
  ]);

function host(
  chatKeys: ChatKeys,
  store: MemoryStatementStore,
  options: {
    storage?: Storage;
    writes?: () => boolean;
    clock?: () => number;
    diagnostics?: ChatEventSink;
  } = {},
) {
  let clock = Date.UTC(2026, 9, 28, 12);
  const storage = options.storage ?? new MemoryStorage();
  const engine = new ChatEngine({
    identity: localNovaIdentity(chatKeys),
    openStore: async () => store.connect(),
    directory: directory(),
    storage,
    recordKey: "chat",
    now: options.clock ?? (() => (clock += 1000)),
    writes: options.writes ?? (() => true),
    diagnostics: options.diagnostics,
    retryDelayMs: 10,
  });
  return { engine, storage };
}

const signedBy = (store: MemoryStatementStore, who: ChatKeys) =>
  store.submitted.filter((encoded) =>
    equalBytes(decodeStatement(encoded).signer, who.identity),
  );

/** What a statement of the identity route holds, opened as `reader` would. */
function openIdentity(encoded: Uint8Array, reader: ChatKeys, writer: ChatKeys) {
  return decodeSessionData(
    unsealSession(
      reader.agree(writer.chatPublic),
      decodeStatement(encoded).data,
    ),
  );
}

/** What a statement of the device route holds, opened as `reader` would. */
async function openDevice(
  encoded: Uint8Array,
  reader: NovaIdentity,
  writer: NovaIdentity,
) {
  return decodeSessionData(
    await reader.openDeviceSession(
      writer.device,
      decodeStatement(encoded).data,
    ),
  );
}

const messagesOf = (data: ReturnType<typeof decodeSessionData>) =>
  data.kind === "request"
    ? data.messages.map((encoded) => decodeMessage(encoded).content)
    : [];

/** A request of the older kind, as a peer that does not announce a device sends it. */
function legacyRequest(
  from: ChatKeys,
  to: ChatKeys,
  id: string,
  welcome: string,
) {
  const request: ChatRequest = {
    id,
    timestampMs: BigInt(Date.UTC(2026, 9, 28, 11)),
    content: { version: 1, welcome },
  };
  const at = Date.UTC(2026, 9, 28, 11);
  return encodeSigned(
    {
      expiry: expiryAt(at),
      channel: null,
      topics: [
        requestDayTopic(to.identity, requestDay(at)),
        requestTopic(to.identity),
        requestSessionTopic(
          from.agree(to.chatPublic),
          from.identity,
          to.identity,
        ),
      ],
      data: sealRequest(
        request,
        to.identity,
        to.chatPublic,
        from.identity,
        (payload) => from.sign(payload),
        cryptoRandom,
      ),
    },
    from.identity,
    (payload) => from.sign(payload),
  );
}

/** A statement on the pair's identity route, written by `from` as an older peer does. */
function identityRoute(
  from: ChatKeys,
  to: ChatKeys,
  data: Parameters<typeof sealSession>[1],
  expiry = Date.UTC(2026, 9, 28, 13),
) {
  const secret = from.agree(to.chatPublic);
  const topic = sessionTopic(secret, from.identity, to.identity);
  return encodeSigned(
    {
      expiry: expiryAt(expiry),
      channel: requestChannel(topic),
      topics: [topic],
      data: sealSession(secret, data, cryptoRandom),
    },
    from.identity,
    (payload) => from.sign(payload),
  );
}

const textMessage = (id: string, body: string) =>
  concat(text(id), u64(9n), Uint8Array.of(0, 0), text(body));

describe("the device route between two hosts", () => {
  async function started() {
    const store = new MemoryStatementStore();
    const a = host(alice, store);
    const b = host(bob, store);
    await Promise.all([a.engine.start(), b.engine.start()]);
    await settle();
    return { store, a, b };
  }

  test("a request announces the device, its acceptance is one unanswered statement, and the chat then runs on the device route", async () => {
    const { store, a, b } = await started();
    await a.engine.startChat("bob.02", "hello");
    await settle();
    const [request] = b.engine.snapshot().requests;
    expect(request).toMatchObject({
      version: 2,
      peer: bytesToHex(alice.identity),
      problem: null,
      device: {
        statementAccount: bytesToHex(alice.identity),
        encryptionKey: bytesToHex(alice.devicePublic),
      },
    });

    await b.engine.accept(request.id);
    await settle();
    // One statement from Bob: the acceptance, on the identity route, alone.
    const [acceptance, ...rest] = signedBy(store, bob);
    expect(rest).toEqual([]);
    const accepted = openIdentity(acceptance, alice, bob);
    expect(accepted.kind).toBe("request");
    expect(messagesOf(accepted)).toEqual([
      {
        kind: "device-accepted",
        requestId: request.id,
        device: {
          statementAccount: bob.identity,
          encryptionKey: bob.devicePublic,
        },
      },
    ]);
    expect(decodeStatement(acceptance).topics).toEqual([
      sessionTopic(bob.agree(alice.chatPublic), bob.identity, alice.identity),
    ]);
    // Alice answered nothing on the identity route.
    expect(signedBy(store, alice)).toHaveLength(1);
    const [contact] = a.engine.snapshot().contacts;
    expect(contact).toMatchObject({
      state: "active",
      deviceRoute: true,
      devices: [
        {
          statementAccount: bytesToHex(bob.identity),
          encryptionKey: bytesToHex(bob.devicePublic),
        },
      ],
    });

    await a.engine.send(contact.account, "first message");
    await settle();
    const aliceIdentity = localNovaIdentity(alice);
    const bobIdentity = localNovaIdentity(bob);
    const sent = signedBy(store, alice).at(-1)!;
    const data = await openDevice(sent, bobIdentity, aliceIdentity);
    expect(messagesOf(data)).toEqual([{ kind: "text", text: "first message" }]);
    expect(decodeStatement(sent).topics).toEqual([
      sessionTopic(
        alice.agreeDevice(bob.chatPublic),
        alice.identity,
        bob.identity,
      ),
    ]);
    // Bob's acknowledgement is on the device route too, and Alice's text is acknowledged.
    const ack = signedBy(store, bob).at(-1)!;
    expect(await openDevice(ack, aliceIdentity, bobIdentity)).toMatchObject({
      kind: "response",
      code: 0,
    });
    expect(a.engine.snapshot().contacts[0].messages.at(-1)).toMatchObject({
      text: "first message",
      status: "acknowledged",
    });
    // Seeing Alice on her device route shows the acceptance arrived.
    const bobSide = b.engine.snapshot().contacts[0];
    expect(bobSide.acceptance?.delivered).toBe(true);
    expect(
      bobSide.messages.find(
        (m) => m.direction === "out" && m.kind === "accepted",
      )?.status,
    ).toBe("acknowledged");
    a.engine.stop();
    b.engine.stop();
  });

  test("the acceptance is published again on every start until the requester is seen on its device route", async () => {
    const store = new MemoryStatementStore();
    const aliceHost = host(alice, store);
    const bobStorage = new MemoryStorage();
    let bobHost = host(bob, store, { storage: bobStorage });
    await Promise.all([aliceHost.engine.start(), bobHost.engine.start()]);
    await settle();
    await aliceHost.engine.startChat("bob.02", "hello");
    await settle();
    aliceHost.engine.stop();
    await bobHost.engine.accept(bobHost.engine.snapshot().requests[0].id);
    const first = signedBy(store, bob);
    expect(first).toHaveLength(1);

    bobHost.engine.stop();
    bobHost = host(bob, store, { storage: bobStorage });
    await bobHost.engine.start();
    await settle();
    const renewed = signedBy(store, bob);
    expect(renewed).toHaveLength(2);
    const [one, two] = renewed.map((encoded) => decodeStatement(encoded));
    // The same message under the same transport request, with a higher expiry so it replaces.
    expect(two.channel).toEqual(one.channel);
    expect(two.expiry > one.expiry).toBe(true);
    const seen = (encoded: Uint8Array) => {
      const data = openIdentity(encoded, alice, bob);
      if (data.kind !== "request") throw new Error("a request");
      return [data.requestId, data.messages[0]] as const;
    };
    expect(seen(renewed[1])).toEqual(seen(renewed[0]));

    // Alice comes back, learns Bob's device from one copy, and writes to him.
    await aliceHost.engine.start();
    await settle();
    const [contact] = aliceHost.engine.snapshot().contacts;
    expect(contact.messages.filter((m) => m.kind === "accepted")).toHaveLength(
      1,
    );
    await aliceHost.engine.send(contact.account, "hello again");
    await settle();
    expect(bobHost.engine.snapshot().contacts[0].acceptance?.delivered).toBe(
      true,
    );

    // A restart now publishes no acceptance.
    bobHost.engine.stop();
    const count = signedBy(store, bob).length;
    bobHost = host(bob, store, { storage: bobStorage });
    await bobHost.engine.start();
    await settle();
    expect(bobHost.engine.snapshot().contacts[0].acceptance?.delivered).toBe(
      true,
    );
    expect(signedBy(store, bob)).toHaveLength(count);
    aliceHost.engine.stop();
    bobHost.engine.stop();
  });

  test("with writes off the acceptance is not renewed, and it goes out once they are on", async () => {
    const store = new MemoryStatementStore();
    let writes = true;
    const aliceHost = host(alice, store);
    const storage = new MemoryStorage();
    let bobHost = host(bob, store, { storage, writes: () => writes });
    await Promise.all([aliceHost.engine.start(), bobHost.engine.start()]);
    await aliceHost.engine.startChat("bob.02", "hello");
    await settle();
    aliceHost.engine.stop();
    await bobHost.engine.accept(bobHost.engine.snapshot().requests[0].id);
    bobHost.engine.stop();
    writes = false;
    bobHost = host(bob, store, { storage, writes: () => writes });
    await bobHost.engine.start();
    await settle();
    expect(signedBy(store, bob)).toHaveLength(1);
    writes = true;
    await bobHost.engine.retry(bobHost.engine.snapshot().contacts[0].account);
    expect(signedBy(store, bob)).toHaveLength(2);
    bobHost.engine.stop();
  });

  test("a peer that also asks while a request waits answers it, and learns this device from an acceptance", async () => {
    const { store, a, b } = await started();
    await a.engine.startChat("bob.02", "from alice");
    await b.engine.startChat("alice.01", "from bob");
    await settle();
    const aliceSide = a.engine.snapshot().contacts[0];
    expect(aliceSide.state).toBe("active");
    expect(aliceSide.devices).toEqual([
      {
        statementAccount: bytesToHex(bob.identity),
        encryptionKey: bytesToHex(bob.devicePublic),
      },
    ]);
    // Alice answered Bob's request with an acceptance naming her device.
    const identityTopic = sessionTopic(
      alice.agree(bob.chatPublic),
      alice.identity,
      bob.identity,
    );
    const acceptances = signedBy(store, alice)
      .filter((encoded) =>
        decodeStatement(encoded).topics.some((topic) =>
          equalBytes(topic, identityTopic),
        ),
      )
      .map((encoded) => messagesOf(openIdentity(encoded, bob, alice)))
      .flat()
      .filter((content) => content.kind === "device-accepted");
    expect(acceptances).toHaveLength(1);
    a.engine.stop();
    b.engine.stop();
  });

  test("a blocked person on the device route is not read or acknowledged until unblocked", async () => {
    const { store, a, b } = await started();
    await a.engine.startChat("bob.02", "hi");
    await settle();
    await b.engine.accept(b.engine.snapshot().requests[0].id);
    await settle();
    const bobSide = () => b.engine.snapshot().contacts[0];
    await b.engine.block(bobSide().account);
    const before = signedBy(store, bob).length;
    await a.engine.send(
      a.engine.snapshot().contacts[0].account,
      "while blocked",
    );
    await settle();
    expect(bobSide().messages.map((m) => m.text)).not.toContain(
      "while blocked",
    );
    expect(signedBy(store, bob)).toHaveLength(before);
    expect(bobSide().blocked).toBe(true);
    await b.engine.unblock(bobSide().account);
    await settle();
    expect(bobSide().messages.map((m) => m.text)).toContain("while blocked");
    expect(bobSide().blocked).toBeUndefined();
    a.engine.stop();
    b.engine.stop();
  });

  test("statements the store delivers again do not bring back a device the peer removed", async () => {
    const store = new MemoryStatementStore();
    const aliceStorage = new MemoryStorage();
    let aliceHost = host(alice, store, { storage: aliceStorage });
    const bobHost = host(bob, store);
    await Promise.all([aliceHost.engine.start(), bobHost.engine.start()]);
    await aliceHost.engine.startChat("bob.02", "hi");
    await settle();
    await bobHost.engine.accept(bobHost.engine.snapshot().requests[0].id);
    await settle();
    const bobIdentity = localNovaIdentity(bob);
    const aliceIdentity = localNovaIdentity(alice);
    const prefixed = (value: Uint8Array) =>
      concat(Uint8Array.of(value.length << 2), value);
    const announce = async (
      id: string,
      tag: number,
      minute: number,
      ...fields: Uint8Array[]
    ) =>
      store.inject(
        await bobIdentity.deviceSessionStatement({
          peer: alice.identity,
          peerChatKey: alice.chatPublic,
          devices: [aliceIdentity.device],
          plaintext: encodeSessionData({
            kind: "request",
            requestId: `announce-${id}`,
            messages: [
              concat(text(id), u64(9n), Uint8Array.of(0, tag), ...fields),
            ],
          }),
          expiry: expiryAt(Date.UTC(2026, 9, 28, 14, minute)),
        }),
      );
    await announce(
      "added",
      17,
      1,
      prefixed(carol.identity),
      prefixed(carol.devicePublic),
    );
    await settle();
    // The device that accepted is removed, from the added one's side of the story.
    await announce("removed", 18, 2, prefixed(bob.identity));
    await settle();
    const devices = () => aliceHost.engine.snapshot().contacts[0].devices;
    const only = [
      {
        statementAccount: bytesToHex(carol.identity),
        encryptionKey: bytesToHex(carol.devicePublic),
      },
    ];
    expect(devices()).toEqual(only);

    // A restart: the store delivers the acceptance, which names the removed device, again.
    aliceHost.engine.stop();
    aliceHost = host(alice, store, { storage: aliceStorage });
    await aliceHost.engine.start();
    await settle();
    expect(devices()).toEqual(only);
    await settle();
    expect(devices()).toEqual(only);
    aliceHost.engine.stop();
    bobHost.engine.stop();
  });

  test("a device added and removed again stays removed when the store delivers both again", async () => {
    const store = new MemoryStatementStore();
    const aliceStorage = new MemoryStorage();
    let aliceHost = host(alice, store, { storage: aliceStorage });
    const bobHost = host(bob, store);
    await Promise.all([aliceHost.engine.start(), bobHost.engine.start()]);
    await aliceHost.engine.startChat("bob.02", "hi");
    await settle();
    await bobHost.engine.accept(bobHost.engine.snapshot().requests[0].id);
    await settle();
    const bobIdentity = localNovaIdentity(bob);
    const aliceIdentity = localNovaIdentity(alice);
    const prefixed = (value: Uint8Array) =>
      concat(Uint8Array.of(value.length << 2), value);
    // Both on Bob's device route: the device added, then removed.
    let minute = 0;
    for (const [id, tag, fields] of [
      ["added", 17, [prefixed(carol.identity), prefixed(carol.devicePublic)]],
      ["removed", 18, [prefixed(carol.identity)]],
    ] as const) {
      minute += 1;
      store.inject(
        await bobIdentity.deviceSessionStatement({
          peer: alice.identity,
          peerChatKey: alice.chatPublic,
          devices: [aliceIdentity.device],
          plaintext: encodeSessionData({
            kind: "request",
            requestId: `announce-${id}`,
            messages: [
              concat(text(id), u64(9n), Uint8Array.of(0, tag), ...fields),
            ],
          }),
          expiry: expiryAt(Date.UTC(2026, 9, 28, 14, minute)),
        }),
      );
      await settle();
    }
    const devices = () =>
      aliceHost.engine
        .snapshot()
        .contacts[0].devices?.map((device) => device.statementAccount);
    expect(devices()).toEqual([bytesToHex(bob.identity)]);
    aliceHost.engine.stop();
    aliceHost = host(alice, store, { storage: aliceStorage });
    await aliceHost.engine.start();
    await settle();
    await settle();
    expect(devices()).toEqual([bytesToHex(bob.identity)]);
    aliceHost.engine.stop();
    bobHost.engine.stop();
  });

  test("two devices that disagree about a third are not followed back and forth when the store delivers their statements again", async () => {
    const store = new MemoryStatementStore();
    const aliceStorage = new MemoryStorage();
    const dave = keys("zoo zoo zoo zoo zoo zoo zoo zoo zoo zoo zoo wrong");
    let aliceHost = host(alice, store, { storage: aliceStorage });
    const bobHost = host(bob, store);
    await Promise.all([aliceHost.engine.start(), bobHost.engine.start()]);
    await aliceHost.engine.startChat("bob.02", "hi");
    await settle();
    await bobHost.engine.accept(bobHost.engine.snapshot().requests[0].id);
    await settle();
    const aliceIdentity = localNovaIdentity(alice);
    const prefixed = (value: Uint8Array) =>
      concat(Uint8Array.of(value.length << 2), value);
    const from = async (
      writer: ChatKeys,
      id: string,
      tag: number,
      minute: number,
      ...fields: Uint8Array[]
    ) => {
      store.inject(
        await localNovaIdentity(writer).deviceSessionStatement({
          peer: alice.identity,
          peerChatKey: alice.chatPublic,
          devices: [aliceIdentity.device],
          plaintext: encodeSessionData({
            kind: "request",
            requestId: `announce-${id}`,
            messages: [
              concat(text(id), u64(9n), Uint8Array.of(0, tag), ...fields),
            ],
          }),
          expiry: expiryAt(Date.UTC(2026, 9, 28, 14, minute)),
        }),
      );
      await settle();
    };
    // Bob announces Carol's device; Carol and Bob then disagree about Dave's.
    await from(
      bob,
      "add-carol",
      17,
      1,
      prefixed(carol.identity),
      prefixed(carol.devicePublic),
    );
    await from(
      bob,
      "add-dave",
      17,
      2,
      prefixed(dave.identity),
      prefixed(dave.devicePublic),
    );
    await from(carol, "remove-dave", 18, 3, prefixed(dave.identity));
    const devices = () =>
      aliceHost.engine
        .snapshot()
        .contacts[0].devices?.map((device) => device.statementAccount)
        .sort();
    const settled = [
      bytesToHex(bob.identity),
      bytesToHex(carol.identity),
    ].sort();
    expect(devices()).toEqual(settled);

    aliceHost.engine.stop();
    let subscriptions = 0;
    aliceHost = host(alice, store, {
      storage: aliceStorage,
      diagnostics: (event) => {
        if (
          event.type === "subscription" &&
          event.stream === "sessions" &&
          event.action === "subscribe"
        )
          subscriptions += 1;
      },
    });
    await aliceHost.engine.start();
    await settle();
    await settle();
    expect(subscriptions).toBeLessThanOrEqual(2);
    expect(devices()).toEqual(settled);
    aliceHost.engine.stop();
    bobHost.engine.stop();
  });

  test("a device with a key that cannot open a session is set aside, and the engine goes on following its contacts", async () => {
    const store = new MemoryStatementStore();
    const aliceStorage = new MemoryStorage();
    let aliceHost = host(alice, store, { storage: aliceStorage });
    await aliceHost.engine.start();
    await aliceHost.engine.startChat("bob.02", "hi");
    const requestId = aliceHost.engine.snapshot().contacts[0].messages[0].id;
    // Bob's acceptance names a zero key.
    store.inject(
      identityRoute(bob, alice, {
        kind: "request",
        requestId: "acc-zero",
        messages: [
          concat(
            text("acc-zero-1"),
            u64(9n),
            Uint8Array.of(0, 20),
            text(requestId),
            bob.identity,
            new Uint8Array(32),
          ),
        ],
      }),
    );
    await settle();
    expect(aliceHost.engine.snapshot().contacts[0].devices).toBeUndefined();
    expect(aliceHost.engine.snapshot().skipped.join()).toContain(
      "cannot open a session",
    );

    // A later, good acceptance is learned, and a zero key announced by a device change is not.
    store.inject(
      identityRoute(
        bob,
        alice,
        {
          kind: "request",
          requestId: "acc-good",
          messages: [
            concat(
              text("acc-good-1"),
              u64(9n),
              Uint8Array.of(0, 20),
              text(requestId),
              bob.identity,
              bob.devicePublic,
            ),
          ],
        },
        Date.UTC(2026, 9, 28, 14),
      ),
    );
    await settle();
    const prefixed = (value: Uint8Array) =>
      concat(Uint8Array.of(value.length << 2), value);
    store.inject(
      await localNovaIdentity(bob).deviceSessionStatement({
        peer: alice.identity,
        peerChatKey: alice.chatPublic,
        devices: [localNovaIdentity(alice).device],
        plaintext: encodeSessionData({
          kind: "request",
          requestId: "announce-zero",
          messages: [
            concat(
              text("added-zero"),
              u64(9n),
              Uint8Array.of(0, 17),
              prefixed(carol.identity),
              prefixed(new Uint8Array(32)),
            ),
          ],
        }),
        expiry: expiryAt(Date.UTC(2026, 9, 28, 15)),
      }),
    );
    await settle();
    expect(aliceHost.engine.snapshot().contacts[0].devices).toEqual([
      {
        statementAccount: bytesToHex(bob.identity),
        encryptionKey: bytesToHex(bob.devicePublic),
      },
    ]);
    aliceHost.engine.stop();
    aliceHost = host(alice, store, { storage: aliceStorage });
    await aliceHost.engine.start();
    await settle();
    expect(aliceHost.engine.snapshot().status).toEqual({ state: "listening" });
    aliceHost.engine.stop();
  });

  test("a device the peer adds is written to and listened to, and one it removes no longer is", async () => {
    const { store, a, b } = await started();
    await a.engine.startChat("bob.02", "hi");
    await settle();
    await b.engine.accept(b.engine.snapshot().requests[0].id);
    await settle();
    const bobIdentity = localNovaIdentity(bob);
    const aliceIdentity = localNovaIdentity(alice);
    const announce = async (id: string, tag: number, ...fields: Uint8Array[]) =>
      store.inject(
        await bobIdentity.deviceSessionStatement({
          peer: alice.identity,
          peerChatKey: alice.chatPublic,
          devices: [aliceIdentity.device],
          plaintext: encodeSessionData({
            kind: "request",
            requestId: `announce-${id}`,
            messages: [
              concat(text(id), u64(9n), Uint8Array.of(0, tag), ...fields),
            ],
          }),
          expiry: expiryAt(Date.UTC(2026, 9, 28, 14, Number(id.slice(-1)))),
        }),
      );
    const prefixed = (value: Uint8Array) =>
      concat(Uint8Array.of(value.length << 2), value);
    await announce(
      "added-1",
      17,
      prefixed(carol.identity),
      prefixed(carol.devicePublic),
    );
    await settle();
    expect(a.engine.snapshot().contacts[0].devices).toHaveLength(2);

    await a.engine.send(a.engine.snapshot().contacts[0].account, "to both");
    const sent = signedBy(store, alice).at(-1)!;
    const carolIdentity = localNovaIdentity(carol);
    for (const reader of [bobIdentity, carolIdentity])
      expect(
        messagesOf(
          decodeSessionData(
            openDeviceEnvelope(
              unsealSession(
                alice.agreeDevice(bob.chatPublic),
                decodeStatement(sent).data,
              ),
              reader.device.statementAccount,
              alice.devicePublic,
              (key) => (reader === bobIdentity ? bob : carol).agreeDevice(key),
            ),
          ),
        ),
      ).toEqual([{ kind: "text", text: "to both" }]);

    await announce("removed-2", 18, prefixed(carol.identity));
    await settle();
    expect(a.engine.snapshot().contacts[0].devices).toHaveLength(1);
    a.engine.stop();
    b.engine.stop();
  });
});

describe("chats from before the device route", () => {
  test("a request of the older kind is accepted as it was: kind 14 on the identity route, and answered there", async () => {
    const store = new MemoryStatementStore();
    const bobHost = host(bob, store);
    await bobHost.engine.start();
    store.inject(legacyRequest(alice, bob, "old-req-1", "from an older app"));
    await settle();
    const [request] = bobHost.engine.snapshot().requests;
    expect(request).toMatchObject({ version: 1, problem: null });
    expect(request.device).toBeUndefined();

    await bobHost.engine.accept(request.id);
    await settle();
    const [acceptance] = signedBy(store, bob);
    const data = openIdentity(acceptance, alice, bob);
    expect(data.kind).toBe("request");
    expect(messagesOf(data)).toEqual([
      { kind: "accepted", requestId: "old-req-1" },
    ]);
    const contact = bobHost.engine.snapshot().contacts[0];
    expect(contact.deviceRoute).toBeUndefined();
    expect(contact.acceptance).toBeUndefined();

    // The older peer acknowledges, then writes on the identity route: acknowledged there.
    const requestId = data.kind === "request" ? data.requestId : "";
    store.inject(
      identityRoute(alice, bob, { kind: "response", requestId, code: 0 }),
    );
    store.inject(
      identityRoute(alice, bob, {
        kind: "request",
        requestId: "old-flush-1",
        messages: [textMessage("old-msg-1", "hello bob")],
      }),
    );
    await settle();
    expect(bobHost.engine.snapshot().contacts[0].messages.at(-1)).toMatchObject(
      {
        text: "hello bob",
        status: "received",
      },
    );
    const reply = signedBy(store, bob).at(-1)!;
    expect(openIdentity(reply, alice, bob)).toEqual({
      kind: "response",
      requestId: "old-flush-1",
      code: 0,
    });
    await bobHost.engine.send(contact.account, "hello back");
    const sent = signedBy(store, bob).at(-1)!;
    expect(decodeStatement(sent).topics).toEqual([
      sessionTopic(bob.agree(alice.chatPublic), bob.identity, alice.identity),
    ]);
    bobHost.engine.stop();
  });

  test("a chat the older engine stored still shows, keeps its blocked and read states and its route, and a device acceptance for it adds no device", async () => {
    const storage = new MemoryStorage();
    const contact = (who: ChatKeys, name: string, extra = {}) => ({
      account: bytesToHex(who.identity),
      chatKey: bytesToHex(who.chatPublic),
      name,
      state: "active" as const,
      messages: [
        {
          id: `${name}-1`,
          direction: "in" as const,
          kind: "text" as const,
          text: `hello from ${name}`,
          at: 5,
          status: "received" as const,
        },
        {
          id: `${name}-2`,
          direction: "out" as const,
          kind: "text" as const,
          text: "an answer",
          at: 6,
          status: "acknowledged" as const,
        },
      ],
      outbound: null,
      expiry: { request: null, response: null },
      acknowledged: null,
      ...extra,
    });
    const record = {
      contacts: [
        contact(alice, "alice", { peerLeft: false }),
        contact(carol, "carol", { blocked: true }),
      ],
      requests: [],
      declined: ["gone"],
      left: [],
    };
    storage.setItem(chatRecordKey("paseo", "wallet"), JSON.stringify(record));
    const store = new MemoryStatementStore();
    const engine = new ChatEngine({
      identity: localNovaIdentity(bob),
      openStore: async () => store.connect(),
      directory: directory(),
      storage,
      recordKey: chatRecordKey("paseo", "wallet"),
      writes: () => true,
      retryDelayMs: 10,
    });
    expect(engine.snapshot().contacts).toEqual(record.contacts);
    await engine.start();
    await settle();

    // The chat keeps its route: the next message goes on the identity route.
    await engine.send(bytesToHex(alice.identity), "still pairwise");
    const sent = signedBy(store, bob).at(-1)!;
    expect(decodeStatement(sent).topics).toEqual([
      sessionTopic(bob.agree(alice.chatPublic), bob.identity, alice.identity),
    ]);
    // An acceptance that names a device does not move it to the device route:
    // the peer never learned this host's device in that chat.
    store.inject(
      identityRoute(alice, bob, {
        kind: "request",
        requestId: "late-acceptance",
        messages: [
          concat(
            text("acc-1"),
            u64(9n),
            Uint8Array.of(0, 20),
            text("never-sent"),
            alice.identity,
            alice.devicePublic,
          ),
        ],
      }),
    );
    await settle();
    expect(engine.snapshot().contacts[0].devices).toBeUndefined();
    expect(engine.snapshot().contacts[0].deviceRoute).toBeUndefined();
    // The blocked person is not followed.
    const blocked = engine.snapshot().contacts[1];
    expect(blocked.blocked).toBe(true);
    store.inject(
      identityRoute(carol, bob, {
        kind: "request",
        requestId: "from-blocked",
        messages: [textMessage("blocked-1", "ignored")],
      }),
    );
    await settle();
    expect(engine.snapshot().contacts[1].messages).toHaveLength(2);
    engine.stop();
    expect(
      loadChatRecord(storage, chatRecordKey("paseo", "wallet")).contacts[1]
        .blocked,
    ).toBe(true);
  });

  test("a version 2 request from a contact accepted before moves the chat to the device route once, and keeps its history", async () => {
    const store = new MemoryStatementStore();
    const bobStorage = new MemoryStorage();
    const older = {
      account: bytesToHex(alice.identity),
      chatKey: bytesToHex(alice.chatPublic),
      name: "alice.01",
      state: "active" as const,
      messages: [
        {
          id: "old-1",
          direction: "in" as const,
          kind: "text" as const,
          text: "from before",
          at: 5,
          status: "received" as const,
        },
      ],
      outbound: null,
      expiry: { request: null, response: null },
      acknowledged: null,
    };
    bobStorage.setItem(
      "chat",
      JSON.stringify({
        contacts: [older],
        requests: [],
        declined: [],
        left: [],
      }),
    );
    const aliceHost = host(alice, store);
    let bobHost = host(bob, store, { storage: bobStorage });
    await Promise.all([aliceHost.engine.start(), bobHost.engine.start()]);
    await settle();
    // Bob sends on the identity route until Alice's app asks again.
    expect(bobHost.engine.snapshot().requests).toEqual([]);
    await aliceHost.engine.startChat("bob.02", "hello again");
    await settle();

    const bobSide = () => bobHost.engine.snapshot().contacts[0];
    expect(bobHost.engine.snapshot().requests).toEqual([]);
    expect(bobSide()).toMatchObject({
      state: "active",
      deviceRoute: true,
      devices: [
        {
          statementAccount: bytesToHex(alice.identity),
          encryptionKey: bytesToHex(alice.devicePublic),
        },
      ],
    });
    expect(bobSide().messages[0]).toMatchObject({
      id: "old-1",
      text: "from before",
    });
    expect(aliceHost.engine.snapshot().contacts[0].state).toBe("active");
    const acceptances = signedBy(store, bob);
    expect(acceptances).toHaveLength(1);

    await bobHost.engine.send(bobSide().account, "on the device route");
    await settle();
    expect(aliceHost.engine.snapshot().contacts[0].messages.at(-1)?.text).toBe(
      "on the device route",
    );

    // The store delivers the request again after a restart: no second answer.
    bobHost.engine.stop();
    bobHost = host(bob, store, { storage: bobStorage });
    const before = signedBy(store, bob).length;
    await bobHost.engine.start();
    await settle();
    expect(signedBy(store, bob)).toHaveLength(before);
    aliceHost.engine.stop();
    bobHost.engine.stop();
  });

  test("a version 2 request kept before the device was stored is read again, then accepted on the device route", async () => {
    const store = new MemoryStatementStore();
    const aliceHost = host(alice, store);
    const bobStorage = new MemoryStorage();
    let bobHost = host(bob, store, { storage: bobStorage });
    await Promise.all([aliceHost.engine.start(), bobHost.engine.start()]);
    await aliceHost.engine.startChat("bob.02", "hi");
    await settle();
    bobHost.engine.stop();
    // The record an older engine wrote: the request without its device.
    const stored = JSON.parse(bobStorage.getItem("chat")!);
    delete stored.requests[0].device;
    bobStorage.setItem("chat", JSON.stringify(stored));
    bobHost = host(bob, store, { storage: bobStorage });
    await expect(bobHost.engine.accept(stored.requests[0].id)).rejects.toThrow(
      "before this host read the sender's device",
    );
    await bobHost.engine.start();
    await settle();
    expect(bobHost.engine.snapshot().requests[0].device).toEqual({
      statementAccount: bytesToHex(alice.identity),
      encryptionKey: bytesToHex(alice.devicePublic),
    });
    await bobHost.engine.accept(stored.requests[0].id);
    expect(bobHost.engine.snapshot().contacts[0].deviceRoute).toBe(true);
    aliceHost.engine.stop();
    bobHost.engine.stop();
  });

  // Request ids are not secret: another person's request with the same id
  // must not lend its device to the stored request.
  test("a stored version 2 request is not given the device of someone else's request with the same id", async () => {
    const store = new MemoryStatementStore();
    const carolHost = host(carol, store);
    const bobStorage = new MemoryStorage();
    let bobHost = host(bob, store, { storage: bobStorage });
    await Promise.all([carolHost.engine.start(), bobHost.engine.start()]);
    await carolHost.engine.startChat("bob.02", "hi");
    await settle();
    bobHost.engine.stop();
    // Alice's request, kept by an older engine, has the id of Carol's.
    const stored = JSON.parse(bobStorage.getItem("chat")!);
    stored.requests[0] = {
      ...stored.requests[0],
      peer: bytesToHex(alice.identity),
      peerChatKey: bytesToHex(alice.chatPublic),
      name: "alice.01",
    };
    delete stored.requests[0].device;
    bobStorage.setItem("chat", JSON.stringify(stored));

    bobHost = host(bob, store, { storage: bobStorage });
    await bobHost.engine.start();
    await settle();
    const [request] = bobHost.engine.snapshot().requests;
    expect(request.peer).toBe(bytesToHex(alice.identity));
    expect(request.device).toBeUndefined();
    await expect(bobHost.engine.accept(request.id)).rejects.toThrow(
      "before this host read the sender's device",
    );
    carolHost.engine.stop();
    bobHost.engine.stop();
  });

  test("a peer who left a chat on the device route and asks again with a version 1 request is answered on the identity route", async () => {
    const store = new MemoryStatementStore();
    const aliceHost = host(alice, store);
    const bobHost = host(bob, store);
    await Promise.all([aliceHost.engine.start(), bobHost.engine.start()]);
    await aliceHost.engine.startChat("bob.02", "hello");
    await settle();
    await bobHost.engine.accept(bobHost.engine.snapshot().requests[0].id);
    await settle();
    await aliceHost.engine.leave(bytesToHex(bob.identity));
    await settle();
    aliceHost.engine.stop();
    expect(bobHost.engine.snapshot().contacts[0]).toMatchObject({
      deviceRoute: true,
      peerLeft: true,
    });

    store.inject(legacyRequest(alice, bob, "old-again", "back, older app"));
    await settle();
    const before = signedBy(store, bob).length;
    await bobHost.engine.accept("old-again");
    await settle();
    const [acceptance] = signedBy(store, bob).slice(before);
    expect(decodeStatement(acceptance).topics).toEqual([
      sessionTopic(bob.agree(alice.chatPublic), bob.identity, alice.identity),
    ]);
    expect(messagesOf(openIdentity(acceptance, alice, bob))).toEqual([
      { kind: "accepted", requestId: "old-again" },
    ]);
    const contact = bobHost.engine.snapshot().contacts[0];
    expect(contact.deviceRoute).toBeUndefined();
    expect(contact.devices).toBeUndefined();
    expect(contact.acceptance).toBeUndefined();
    expect(contact.messages[0].text).toBe("hello");
    bobHost.engine.stop();
  });
});

describe("requests that cannot be trusted", () => {
  /** A version 2 request from `from`, with the proof and device the test chooses. */
  async function v2Request(
    store: MemoryStatementStore,
    options: {
      id?: string;
      proofSecret?: Uint8Array;
      /** The account the proof names as the device; the signer's by default. */
      proofFor?: ChatKeys;
      signer?: ChatKeys;
      deviceKey?: Uint8Array;
    },
  ) {
    const signer = options.signer ?? alice;
    const secret = options.proofSecret ?? alice.agree(bob.chatPublic);
    const request: ChatRequest = {
      id: options.id ?? "v2-req-1",
      timestampMs: BigInt(Date.UTC(2026, 9, 28, 11)),
      content: {
        version: 2,
        identity: alice.identity,
        identityProof: identityProof(
          secret,
          alice.identity,
          (options.proofFor ?? signer).identity,
        ),
        deviceKey: options.deviceKey ?? alice.devicePublic,
        welcome: "hi",
      },
    };
    const at = Date.UTC(2026, 9, 28, 11);
    store.inject(
      encodeSigned(
        {
          expiry: expiryAt(at),
          channel: null,
          topics: [
            requestDayTopic(bob.identity, requestDay(at)),
            requestTopic(bob.identity),
            requestSessionTopic(
              alice.agree(bob.chatPublic),
              alice.identity,
              bob.identity,
            ),
          ],
          data: sealRequest(
            request,
            bob.identity,
            bob.chatPublic,
            signer.identity,
            (payload) => signer.sign(payload),
            cryptoRandom,
          ),
        },
        signer.identity,
        (payload) => signer.sign(payload),
      ),
    );
  }

  test("a proof made with another key, or for another signer, is not accepted", async () => {
    for (const options of [
      { proofSecret: carol.agree(bob.chatPublic) },
      {
        // Signed by Carol's account, with a proof made for Alice's own account.
        signer: carol,
        proofFor: alice,
      },
    ]) {
      const store = new MemoryStatementStore();
      const bobHost = host(bob, store);
      await bobHost.engine.start();
      await v2Request(store, options);
      await settle();
      const [request] = bobHost.engine.snapshot().requests;
      expect(request.problem).toContain("identity proof does not match");
      await expect(bobHost.engine.accept(request.id)).rejects.toThrow(
        "identity proof does not match",
      );
      expect(signedBy(store, bob)).toHaveLength(0);
      bobHost.engine.stop();
    }
  });

  test("a forged request that crosses one this host sent does not answer it", async () => {
    const store = new MemoryStatementStore();
    const bobHost = host(bob, store);
    await bobHost.engine.start();
    await bobHost.engine.startChat("alice.01", "hello");
    // Signed by Carol's account and naming Alice, with a proof made for Alice's account.
    await v2Request(store, { signer: carol, proofFor: alice });
    await settle();
    expect(bobHost.engine.snapshot().contacts[0].state).toBe("requested");
    const [request] = bobHost.engine.snapshot().requests;
    expect(request.problem).toContain("identity proof does not match");
    expect(signedBy(store, bob)).toHaveLength(1);
    bobHost.engine.stop();
  });

  test("an acceptance priority only goes up, as the channel's older statements and the clock would have it", async () => {
    const store = new MemoryStatementStore();
    const bobHost = host(bob, store, {
      clock: () => Date.UTC(2026, 9, 28, 12),
    });
    await bobHost.engine.start();
    await v2Request(store, { id: "first" });
    await settle();
    await bobHost.engine.accept("first");
    // The same person asks again within the same second, as after a reinstall.
    await v2Request(store, { id: "second" });
    await settle();
    const [one, two] = signedBy(store, bob).map((encoded) =>
      decodeStatement(encoded),
    );
    expect(two.channel).toEqual(one.channel);
    expect(two.expiry > one.expiry).toBe(true);
    bobHost.engine.stop();
  });

  test("a stored request that cannot be read again now is read on a later delivery", async () => {
    const store = new MemoryStatementStore();
    const aliceHost = host(alice, store);
    const storage = new MemoryStorage();
    let failing = false;
    const flaky: ChainDirectory = {
      ...directory(),
      async names(account) {
        if (failing) throw new Error("the chain did not answer");
        return directory().names(account);
      },
    };
    const make = () => {
      let clock = Date.UTC(2026, 9, 28, 12);
      return new ChatEngine({
        identity: localNovaIdentity(bob),
        openStore: async () => store.connect(),
        directory: flaky,
        storage,
        recordKey: "chat",
        now: () => (clock += 1000),
        writes: () => true,
        retryDelayMs: 10,
      });
    };
    let engine = make();
    await Promise.all([aliceHost.engine.start(), engine.start()]);
    await aliceHost.engine.startChat("bob.02", "hi");
    await settle();
    engine.stop();
    const stored = JSON.parse(storage.getItem("chat")!);
    delete stored.requests[0].device;
    storage.setItem("chat", JSON.stringify(stored));
    failing = true;
    engine = make();
    await engine.start();
    await settle();
    expect(engine.snapshot().requests[0].device).toBeUndefined();
    engine.stop();
    failing = false;
    engine = make();
    await engine.start();
    await settle();
    expect(engine.snapshot().requests[0]).toMatchObject({
      problem: null,
      device: { statementAccount: bytesToHex(alice.identity) },
    });
    await engine.accept(stored.requests[0].id);
    aliceHost.engine.stop();
    engine.stop();
  });

  test("a device account that is not the identity names a device of the identity, and is the one written to", async () => {
    const store = new MemoryStatementStore();
    const bobHost = host(bob, store);
    await bobHost.engine.start();
    // Alice's request signed by another account of hers (Carol's keys stand in
    // for it), with the identity proof made for that account.
    const device = carol;
    const request: ChatRequest = {
      id: "v2-req-2",
      timestampMs: 1n,
      content: {
        version: 2,
        identity: alice.identity,
        identityProof: identityProof(
          alice.agree(bob.chatPublic),
          alice.identity,
          device.identity,
        ),
        deviceKey: device.devicePublic,
        welcome: null,
      },
    };
    const at = Date.UTC(2026, 9, 28, 11);
    store.inject(
      encodeSigned(
        {
          expiry: expiryAt(at),
          channel: null,
          topics: [
            requestDayTopic(bob.identity, requestDay(at)),
            requestTopic(bob.identity),
            requestSessionTopic(
              alice.agree(bob.chatPublic),
              alice.identity,
              bob.identity,
            ),
          ],
          data: sealRequest(
            request,
            bob.identity,
            bob.chatPublic,
            device.identity,
            (payload) => device.sign(payload),
            cryptoRandom,
          ),
        },
        device.identity,
        (payload) => device.sign(payload),
      ),
    );
    await settle();
    const [pending] = bobHost.engine.snapshot().requests;
    expect(pending).toMatchObject({
      peer: bytesToHex(alice.identity),
      problem: null,
      device: {
        statementAccount: bytesToHex(device.identity),
        encryptionKey: bytesToHex(device.devicePublic),
      },
    });
    await bobHost.engine.accept("v2-req-2");
    const [contact] = bobHost.engine.snapshot().contacts;
    expect(contact.account).toBe(bytesToHex(alice.identity));
    expect(contact.devices?.[0].statementAccount).toBe(
      bytesToHex(device.identity),
    );
    // Bob listens on the topic of that device, not of the identity account.
    await bobHost.engine.send(contact.account, "to the device");
    const sent = signedBy(store, bob).at(-1)!;
    const envelope = openDeviceEnvelope(
      unsealSession(
        bob.agreeDevice(alice.chatPublic),
        decodeStatement(sent).data,
      ),
      device.identity,
      bob.devicePublic,
      (key) => device.agreeDevice(key),
    );
    expect(messagesOf(decodeSessionData(envelope))).toEqual([
      { kind: "text", text: "to the device" },
    ]);
    bobHost.engine.stop();
  });

  test("a request whose device key cannot open a session cannot be accepted", async () => {
    const store = new MemoryStatementStore();
    const bobHost = host(bob, store);
    await bobHost.engine.start();
    await v2Request(store, { deviceKey: new Uint8Array(32) });
    await settle();
    const [request] = bobHost.engine.snapshot().requests;
    expect(request.problem).toContain("device key cannot be used");
    await expect(bobHost.engine.accept(request.id)).rejects.toThrow(
      "device key cannot be used",
    );
    bobHost.engine.stop();
  });

  test("an acceptance for a request this host did not send adds no device", async () => {
    const store = new MemoryStatementStore();
    const aliceHost = host(alice, store);
    await aliceHost.engine.start();
    await aliceHost.engine.startChat("bob.02", "hi");
    store.inject(
      identityRoute(bob, alice, {
        kind: "request",
        requestId: "stray-acceptance",
        messages: [
          concat(
            text("stray-1"),
            u64(9n),
            Uint8Array.of(0, 20),
            text("another-request"),
            bob.identity,
            bob.devicePublic,
          ),
        ],
      }),
    );
    await settle();
    const [contact] = aliceHost.engine.snapshot().contacts;
    expect(contact.devices).toBeUndefined();
    expect(aliceHost.engine.snapshot().skipped.join()).toContain(
      "acceptance for a request this host did not send",
    );
    aliceHost.engine.stop();
  });
});
