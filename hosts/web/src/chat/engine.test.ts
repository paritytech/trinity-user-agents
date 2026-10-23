import { describe, expect, test } from "bun:test";
import { bytesToHex } from "@parity/truapi/scale";
import { mnemonicToEntropy } from "@scure/bip39";
import { wordlist } from "@scure/bip39/wordlists/english.js";
import type {
  AccountNames,
  ChainDirectory,
  PeopleConsumer,
} from "./directory.js";
import {
  ChatEngine,
  WRITES_OFF,
  type HopService,
  type StatementStoreLike,
} from "./engine.js";
import { MemoryHopNode } from "./hop-memory.js";
import { HOP_MAX_FILE_BYTES } from "./hop.js";
import * as sr25519 from "@scure/sr25519";
import { bytes, concat, equalBytes, text, u64 } from "./codec.js";
import { cryptoRandom } from "./crypto.js";
import { ChatDiagnostics } from "./diagnostics.js";
import { ChatKeys } from "./keys.js";
import { loadChatRecord } from "./store.js";
import {
  requestChannel,
  requestTopic,
  sealSession,
  sessionTopic,
} from "./protocol.js";
import { decodeStatement, encodeSigned, expiryAt } from "./statement.js";
import type { SubmitResult, TopicFilter } from "./statement-store.js";
import { MemoryStorage } from "../test-support.js";

/** Synthetic phrases from the BIP-39 test vectors. They hold nothing. */
const ALICE =
  "legal winner thank year wave sausage worth useful legal winner thank yellow";
const BOB =
  "letter advice cage absurd amount doctor acoustic avoid letter advice cage above";
const keys = (phrase: string) =>
  ChatKeys.fromEntropy(mnemonicToEntropy(phrase, wordlist), "paseo");

/**
 * A statement store in memory with the store's own rules: a subscription gets
 * what is stored and what comes later, and a newer statement from the same
 * signer on the same channel replaces the older one.
 */
class MemoryStatementStore {
  statements: Uint8Array[] = [];
  /** Every statement submitted, including those a later one replaced. */
  submitted: Uint8Array[] = [];
  private readonly subscribers = new Set<{
    filter: TopicFilter;
    deliver: (s: Uint8Array[]) => void;
  }>();

  private matches(filter: TopicFilter, encoded: Uint8Array): boolean {
    const topics = decodeStatement(encoded).topics;
    const has = (topic: Uint8Array) =>
      topics.some((candidate) => equalBytes(candidate, topic));
    return "matchAll" in filter
      ? filter.matchAll.every(has)
      : filter.matchAny.some(has);
  }

  /** Connections opened so far, and the way to drop each from the store's side. */
  connections: { drop: () => void; isOpen: () => boolean }[] = [];

  connect(allowed: boolean): StatementStoreLike {
    const own = new Set<{
      filter: TopicFilter;
      deliver: (s: Uint8Array[]) => void;
    }>();
    let open = true;
    let ended!: () => void;
    const closed = new Promise<void>((resolve) => (ended = resolve));
    const end = () => {
      if (!open) return;
      open = false;
      for (const subscriber of own) this.subscribers.delete(subscriber);
      ended();
    };
    this.connections.push({ drop: end, isOpen: () => open });
    return {
      closed,
      subscribe: async (filter, deliver) => {
        if (!open) throw new Error("closed");
        const subscriber = { filter, deliver };
        this.subscribers.add(subscriber);
        own.add(subscriber);
        const stored = this.statements.filter((encoded) =>
          this.matches(filter, encoded),
        );
        if (stored.length > 0) queueMicrotask(() => deliver(stored));
        return () => {
          this.subscribers.delete(subscriber);
          own.delete(subscriber);
        };
      },
      submit: async (encoded): Promise<SubmitResult> => {
        if (!open) throw new Error("closed");
        if (!allowed) return { status: "rejected", reason: "noAllowance" };
        const incoming = decodeStatement(encoded);
        this.submitted.push(encoded);
        this.statements = this.statements.filter((stored) => {
          const old = decodeStatement(stored);
          return !(
            incoming.channel !== null &&
            old.channel !== null &&
            equalBytes(old.channel, incoming.channel) &&
            equalBytes(old.signer, incoming.signer)
          );
        });
        this.statements.push(encoded);
        for (const { filter, deliver } of this.subscribers)
          if (this.matches(filter, encoded))
            queueMicrotask(() => deliver([encoded]));
        return { status: "new" };
      },
      close: end,
    };
  }
}

/** Where a person's username and chat key are registered, as the two app registration paths leave them. */
type Registry = "assetHub" | "people";

interface Person {
  label: string;
  keys: ChatKeys;
  allowance: boolean;
  /** Defaults to Asset Hub. */
  registry?: Registry;
  /** A key that chain names in place of the person's own, to model a mismatch. */
  peopleChatKey?: Uint8Array;
}

function directory(people: Person[]): ChainDirectory {
  const find = (account: Uint8Array) =>
    people.find((person) => equalBytes(person.keys.identity, account));
  const on = (person: Person | undefined, registry: Registry) =>
    person !== undefined && (person.registry ?? "assetHub") === registry;
  return {
    async names(account): Promise<AccountNames | null> {
      const person = find(account);
      return person === undefined || !on(person, "assetHub")
        ? null
        : {
            lite: { label: person.label, chatKey: person.keys.chatPublic },
            full: null,
          };
    },
    async consumer(account): Promise<PeopleConsumer | null> {
      const person = find(account);
      if (person === undefined) return null;
      if (!on(person, "people") && person.peopleChatKey === undefined)
        return null;
      return {
        liteUsername: person.label,
        fullUsername: null,
        chatKey: person.peopleChatKey ?? person.keys.chatPublic,
      };
    },
    async owners(label) {
      const person = people.find((candidate) => candidate.label === label);
      return {
        assetHub: on(person, "assetHub") ? person!.keys.identity : null,
        people: on(person, "people") ? person!.keys.identity : null,
      };
    },
    async hasAllowance(account) {
      return find(account)?.allowance ?? false;
    },
  };
}

const settle = () => new Promise((resolve) => setTimeout(resolve, 20));

async function pair(bobAllowed = true, hop?: HopService) {
  const store = new MemoryStatementStore();
  const alice = keys(ALICE);
  const bob = keys(BOB);
  const chain = directory([
    { label: "alice.01", keys: alice, allowance: true },
    { label: "bob.02", keys: bob, allowance: bobAllowed },
  ]);
  let clock = Date.UTC(2026, 9, 15, 12);
  const engine = (chatKeys: ChatKeys, allowed: boolean) =>
    new ChatEngine({
      keys: chatKeys,
      openStore: async () => store.connect(allowed),
      directory: chain,
      storage: new MemoryStorage(),
      recordKey: "chat",
      now: () => (clock += 1000),
      writes: () => true,
      retryDelayMs: 10,
      hop,
    });
  const aliceEngine = engine(alice, true);
  const bobEngine = engine(bob, bobAllowed);
  await Promise.all([aliceEngine.start(), bobEngine.start()]);
  await settle();
  return { store, alice, bob, aliceEngine, bobEngine };
}

describe("chat engine", () => {
  test("a request, its acceptance and text reach the other side, and text is acknowledged", async () => {
    const { alice, aliceEngine, bobEngine } = await pair();
    expect(aliceEngine.snapshot()).toMatchObject({
      name: "alice.01",
      chainKey: "matches",
      canSend: true,
    });

    await aliceEngine.startChat("bob.02", "hello bob");
    await settle();
    const [request] = bobEngine.snapshot().requests;
    expect(request).toMatchObject({
      peer: bytesToHex(alice.identity),
      name: "alice.01",
      welcome: "hello bob",
      problem: null,
    });

    await bobEngine.accept(request.id);
    await settle();
    const [aliceSide] = aliceEngine.snapshot().contacts;
    expect(aliceSide.state).toBe("active");
    expect(aliceSide.messages.at(-1)).toMatchObject({
      direction: "in",
      kind: "accepted",
    });

    await aliceEngine.send(aliceSide.account, "first message");
    await settle();
    const [bobSide] = bobEngine.snapshot().contacts;
    expect(
      bobSide.messages.map((message) => [
        message.direction,
        message.kind,
        message.status,
      ]),
    ).toEqual([
      ["in", "request", "received"],
      ["out", "accepted", "acknowledged"],
      ["in", "text", "received"],
    ]);
    expect(bobSide.messages[2].text).toBe("first message");
    expect(aliceEngine.snapshot().contacts[0].messages.at(-1)).toMatchObject({
      text: "first message",
      status: "acknowledged",
    });
  });

  test("messages sent before an acknowledgement go out together in the newer request", async () => {
    const { store, alice, bob, aliceEngine, bobEngine } = await pair();
    await aliceEngine.startChat("bob.02", "hi");
    await settle();
    await bobEngine.accept(bobEngine.snapshot().requests[0].id);
    await settle();
    bobEngine.stop();
    const account = aliceEngine.snapshot().contacts[0].account;
    await aliceEngine.send(account, "one");
    await aliceEngine.send(account, "two");
    // The second request replaced the first on Alice's request channel.
    const channel = requestChannel(
      sessionTopic(alice.agree(bob.chatPublic), alice.identity, bob.identity),
    );
    const onChannel = store.statements.filter((encoded) => {
      const statement = decodeStatement(encoded);
      return (
        statement.channel !== null && equalBytes(statement.channel, channel)
      );
    });
    expect(onChannel.length).toBe(1);
    await bobEngine.start();
    await settle();
    expect(
      bobEngine
        .snapshot()
        .contacts[0].messages.filter((m) => m.kind === "text")
        .map((m) => m.text),
    ).toEqual(["one", "two"]);
  });

  test("an identity with no statement allowance sends nothing and says why", async () => {
    const { store, aliceEngine, bobEngine } = await pair(false);
    expect(bobEngine.snapshot().canSend).toBe(false);
    await aliceEngine.startChat("bob.02", "hi");
    await settle();
    const before = store.statements.length;
    await bobEngine.accept(bobEngine.snapshot().requests[0].id);
    const accepted = bobEngine.snapshot().contacts[0].messages[1];
    expect(accepted).toMatchObject({ kind: "accepted", status: "failed" });
    expect(accepted.problem).toContain("no statement allowance");
    expect(store.statements.length).toBe(before);
  });

  test("a request from an account with no chat key on chain cannot be accepted", async () => {
    const store = new MemoryStatementStore();
    const alice = keys(ALICE);
    const bob = keys(BOB);
    const onlyBob = directory([
      { label: "bob.02", keys: bob, allowance: true },
    ]);
    const withBoth = directory([
      { label: "alice.01", keys: alice, allowance: true },
      { label: "bob.02", keys: bob, allowance: true },
    ]);
    const make = (chatKeys: ChatKeys, chain: ChainDirectory) =>
      new ChatEngine({
        keys: chatKeys,
        openStore: async () => store.connect(true),
        directory: chain,
        storage: new MemoryStorage(),
        recordKey: "chat",
        writes: () => true,
      });
    const aliceEngine = make(alice, withBoth);
    const bobEngine = make(bob, onlyBob);
    await Promise.all([aliceEngine.start(), bobEngine.start()]);
    await aliceEngine.startChat("bob.02", "hi");
    await settle();
    const [request] = bobEngine.snapshot().requests;
    expect(request.problem).toContain(
      "no chat key on Asset Hub or the People chain",
    );
    expect(bobEngine.accept(request.id)).rejects.toThrow(
      "no chat key on Asset Hub or the People chain",
    );
  });
});

describe("chat engine permissions", () => {
  const signedBy = (store: MemoryStatementStore, chatKeys: ChatKeys) =>
    store.statements.filter((encoded) =>
      equalBytes(decodeStatement(encoded).signer, chatKeys.identity),
    ).length;

  test("constructing an engine opens no connection and publishes nothing", async () => {
    let opened = 0;
    const store = new MemoryStatementStore();
    const engine = new ChatEngine({
      keys: keys(BOB),
      openStore: async () => {
        opened += 1;
        return store.connect(true);
      },
      directory: directory([]),
      storage: new MemoryStorage(),
      recordKey: "chat",
    });
    await settle();
    expect(opened).toBe(0);
    expect(engine.snapshot().status).toEqual({ state: "stopped" });
    expect(store.statements.length).toBe(0);
  });

  test("an engine with no write permission refuses every action that publishes", async () => {
    const store = new MemoryStatementStore();
    const alice = keys(ALICE);
    const bob = keys(BOB);
    const chain = directory([
      { label: "alice.01", keys: alice, allowance: true },
      { label: "bob.02", keys: bob, allowance: true },
    ]);
    const make = (chatKeys: ChatKeys, writes?: () => boolean) =>
      new ChatEngine({
        keys: chatKeys,
        openStore: async () => store.connect(true),
        directory: chain,
        storage: new MemoryStorage(),
        recordKey: "chat",
        writes,
      });
    const aliceEngine = make(alice, () => true);
    // No `writes` at all is the default every caller gets.
    const bobEngine = make(bob);
    await Promise.all([aliceEngine.start(), bobEngine.start()]);
    await aliceEngine.startChat("bob.02", "hi");
    await settle();
    const [request] = bobEngine.snapshot().requests;
    expect(request).toBeDefined();

    await expect(bobEngine.accept(request.id)).rejects.toThrow(WRITES_OFF);
    await expect(bobEngine.startChat("alice.01", "x")).rejects.toThrow(
      WRITES_OFF,
    );
    expect(bobEngine.snapshot().contacts).toEqual([]);
    expect(bobEngine.snapshot().requests.map((r) => r.id)).toEqual([
      request.id,
    ]);
    expect(signedBy(store, bob)).toBe(0);
  });

  test("withdrawing write permission stops acknowledgements of received text", async () => {
    const store = new MemoryStatementStore();
    const alice = keys(ALICE);
    const bob = keys(BOB);
    const chain = directory([
      { label: "alice.01", keys: alice, allowance: true },
      { label: "bob.02", keys: bob, allowance: true },
    ]);
    let bobWrites = true;
    const make = (chatKeys: ChatKeys, writes: () => boolean) =>
      new ChatEngine({
        keys: chatKeys,
        openStore: async () => store.connect(true),
        directory: chain,
        storage: new MemoryStorage(),
        recordKey: "chat",
        writes,
      });
    const aliceEngine = make(alice, () => true);
    const bobEngine = make(bob, () => bobWrites);
    await Promise.all([aliceEngine.start(), bobEngine.start()]);
    await aliceEngine.startChat("bob.02", "hi");
    await settle();
    await bobEngine.accept(bobEngine.snapshot().requests[0].id);
    await settle();
    const before = signedBy(store, bob);
    const bobChannels = store.statements
      .map(decodeStatement)
      .filter((statement) => equalBytes(statement.signer, bob.identity))
      .map((statement) => statement.channel);

    bobWrites = false;
    await aliceEngine.send(aliceEngine.snapshot().contacts[0].account, "later");
    await settle();
    const bobSide = bobEngine.snapshot().contacts[0];
    expect(bobSide.messages.at(-1)).toMatchObject({
      text: "later",
      direction: "in",
    });
    expect(signedBy(store, bob)).toBe(before);
    expect(
      store.statements
        .map(decodeStatement)
        .filter((statement) => equalBytes(statement.signer, bob.identity))
        .map((statement) => statement.channel),
    ).toEqual(bobChannels);
    await expect(bobEngine.send(bobSide.account, "no")).rejects.toThrow(
      WRITES_OFF,
    );
    expect(aliceEngine.snapshot().contacts[0].messages.at(-1)?.status).toBe(
      "sent",
    );
  });
});

describe("chat engine identity lookup", () => {
  async function engines(people: Person[]) {
    const store = new MemoryStatementStore();
    const chain = directory(people);
    const make = (chatKeys: ChatKeys) =>
      new ChatEngine({
        keys: chatKeys,
        openStore: async () => store.connect(true),
        directory: chain,
        storage: new MemoryStorage(),
        recordKey: "chat",
        writes: () => true,
      });
    const [sender, receiver] = people.map((person) => make(person.keys));
    await Promise.all([sender.start(), receiver.start()]);
    return { sender, receiver };
  }

  // Polkadot app builds that register through the People chain leave no
  // Asset Hub record; their chat key is the Consumers identifier key.
  test("takes the chat key of a sender registered only on the People chain", async () => {
    const alice = keys(ALICE);
    const bob = keys(BOB);
    const { sender, receiver } = await engines([
      { label: "alice.01", keys: alice, allowance: true, registry: "people" },
      { label: "bob.02", keys: bob, allowance: true },
    ]);
    await sender.startChat("bob.02", "hello");
    await settle();
    expect(receiver.snapshot().requests).toMatchObject([
      {
        peer: bytesToHex(alice.identity),
        name: "alice.01",
        peerChatKey: bytesToHex(alice.chatPublic),
        problem: null,
      },
    ]);
    expect(sender.snapshot()).toMatchObject({
      name: "alice.01",
      chainKey: "matches",
    });
  });

  test("looks a stored request's sender up again when the store delivers it again", async () => {
    const store = new MemoryStatementStore();
    const alice = keys(ALICE);
    const bob = keys(BOB);
    const people: Person[] = [{ label: "bob.02", keys: bob, allowance: true }];
    const chain = directory(people);
    const storage = new MemoryStorage();
    const make = (chatKeys: ChatKeys) =>
      new ChatEngine({
        keys: chatKeys,
        openStore: async () => store.connect(true),
        directory: chain,
        storage,
        recordKey: chatKeys === bob ? "bob" : "alice",
        writes: () => true,
      });
    // Alice reaches Bob directly, as a sender Bob's directory cannot see yet.
    const aliceEngine = new ChatEngine({
      keys: alice,
      openStore: async () => store.connect(true),
      directory: directory([
        { label: "alice.01", keys: alice, allowance: true },
        ...people,
      ]),
      storage: new MemoryStorage(),
      recordKey: "alice",
      writes: () => true,
    });
    let bobEngine = make(bob);
    await Promise.all([aliceEngine.start(), bobEngine.start()]);
    await aliceEngine.startChat("bob.02", "hello");
    await settle();
    const [before] = bobEngine.snapshot().requests;
    expect(before.peerChatKey).toBeNull();

    // The sender's People chain record is now readable; nothing else changed.
    people.push({
      label: "alice.01",
      keys: alice,
      allowance: true,
      registry: "people",
    });
    bobEngine.stop();
    bobEngine = make(bob);
    await bobEngine.start();
    await settle();
    expect(bobEngine.snapshot().requests).toEqual([
      {
        ...before,
        name: "alice.01",
        peerChatKey: bytesToHex(alice.chatPublic),
        problem: null,
      },
    ]);
  });

  test("uses neither key when Asset Hub and the People chain disagree", async () => {
    const alice = keys(ALICE);
    const bob = keys(BOB);
    const { sender, receiver } = await engines([
      {
        label: "alice.01",
        keys: alice,
        allowance: true,
        peopleChatKey: bob.chatPublic,
      },
      { label: "bob.02", keys: bob, allowance: true },
    ]);
    await sender.startChat("bob.02", "hello");
    await settle();
    const [request] = receiver.snapshot().requests;
    expect(request.peerChatKey).toBeNull();
    expect(request.problem).toContain("differ");
    await expect(receiver.accept(request.id)).rejects.toThrow("differ");
    expect(sender.snapshot().chainKey).toBe("conflict");
  });
});

describe("chat engine control messages", () => {
  test("keeps a peer's push token and device changes out of the conversation", async () => {
    const { store, alice, bob, aliceEngine, bobEngine } = await pair();
    await aliceEngine.startChat("bob.02", "hi");
    await settle();
    await bobEngine.accept(bobEngine.snapshot().requests[0].id);
    await settle();

    // What the iPhone sent after acceptance: a token, device announcements,
    // and here a text and a reaction, in one request from Alice.
    const head = (id: string, tag: number) =>
      concat(text(id), u64(9n), Uint8Array.of(0, tag));
    const messages = [
      concat(
        head("c1", 17),
        bytes(new Uint8Array(32).fill(1)),
        bytes(new Uint8Array(32).fill(2)),
      ),
      concat(head("c2", 1), bytes(Uint8Array.of(5, 5)), Uint8Array.of(1)),
      concat(head("c3", 0), text("visible")),
      concat(head("c4", 4), text("c3"), text("+1")),
    ];
    const topic = sessionTopic(
      alice.agree(bob.chatPublic),
      alice.identity,
      bob.identity,
    );
    await store.connect(true).submit(
      encodeSigned(
        {
          expiry: expiryAt(Date.UTC(2026, 9, 15, 13)),
          channel: requestChannel(topic),
          topics: [topic],
          data: sealSession(
            alice.agree(bob.chatPublic),
            { kind: "request", requestId: "q-control", messages },
            cryptoRandom,
          ),
        },
        alice.identity,
        (payload) => alice.sign(payload),
      ),
    );
    await settle();
    const bobSide = bobEngine.snapshot().contacts[0];
    expect(
      bobSide.messages.map((message) => [message.id, message.kind]),
    ).toEqual([
      [bobSide.messages[0].id, "request"],
      [bobSide.messages[1].id, "accepted"],
      ["c3", "text"],
    ]);
    // The reaction lands on the text it names, not as a bubble of its own.
    expect(bobSide.messages[2].reactions).toEqual([
      { emoji: "+1", from: "peer" },
    ]);
    // The whole request is acknowledged, controls included.
    expect(bobSide.acknowledged).toBe("q-control");
  });

  test("drops stored placeholders of control kinds when a record loads", () => {
    const storage = new MemoryStorage();
    const placeholder = (id: string, tag: number) => ({
      id,
      direction: "in" as const,
      kind: "unsupported" as const,
      text: `A message this host does not show yet (kind ${tag})`,
      at: 1,
      status: "received" as const,
    });
    const contact = {
      account: "0x01" as const,
      chatKey: "0x02" as const,
      name: null,
      state: "active" as const,
      messages: [
        placeholder("a", 17),
        placeholder("b", 1),
        placeholder("c", 18),
        placeholder("d", 4),
      ],
      outbound: null,
      expiry: { request: null, response: null },
      acknowledged: null,
    };
    storage.setItem(
      "chat",
      JSON.stringify({ contacts: [contact], requests: [], declined: [] }),
    );
    expect(
      loadChatRecord(storage, "chat").contacts[0].messages.map((m) => m.id),
    ).toEqual(["d"]);
  });
});

describe("chat engine lifecycle", () => {
  function single() {
    const store = new MemoryStatementStore();
    let opened = 0;
    const engine = new ChatEngine({
      keys: keys(BOB),
      openStore: async () => {
        opened += 1;
        return store.connect(true);
      },
      directory: directory([
        { label: "bob.02", keys: keys(BOB), allowance: true },
      ]),
      storage: new MemoryStorage(),
      recordKey: "chat",
      writes: () => true,
      retryDelayMs: 10,
    });
    return { store, engine, opened: () => opened };
  }

  test("starting twice opens one connection", async () => {
    const { store, engine, opened } = single();
    await Promise.all([engine.start(), engine.start()]);
    await engine.start();
    expect(opened()).toBe(1);
    expect(store.connections.filter((c) => c.isOpen()).length).toBe(1);
    expect(engine.snapshot().status).toEqual({ state: "listening" });
  });

  test("reconnects after the store drops the connection, and stop ends it", async () => {
    const { store, engine, opened } = single();
    await engine.start();
    store.connections[0].drop();
    await settle();
    await settle();
    expect(opened()).toBe(2);
    expect(engine.snapshot().status).toEqual({ state: "listening" });
    expect(store.connections.filter((c) => c.isOpen()).length).toBe(1);

    engine.stop();
    await settle();
    expect(store.connections.every((c) => !c.isOpen())).toBe(true);
    await new Promise((resolve) => setTimeout(resolve, 50));
    expect(opened()).toBe(2);
    expect(engine.snapshot().status).toEqual({ state: "stopped" });
  });

  test("retries a failed start until stopped", async () => {
    const store = new MemoryStatementStore();
    let attempts = 0;
    const engine = new ChatEngine({
      keys: keys(BOB),
      openStore: async () => {
        attempts += 1;
        if (attempts < 3) throw new Error("no connection");
        return store.connect(true);
      },
      directory: directory([]),
      storage: new MemoryStorage(),
      recordKey: "chat",
      retryDelayMs: 5,
    });
    await engine.start();
    expect(engine.snapshot().status).toMatchObject({
      state: "failed",
      reason: "no connection",
    });
    await new Promise((resolve) => setTimeout(resolve, 80));
    expect(attempts).toBe(3);
    expect(engine.snapshot().status).toEqual({ state: "listening" });
    engine.stop();
  });

  test("a message delivered again is neither shown twice nor acknowledged twice", async () => {
    const { store, bob, aliceEngine, bobEngine } = await pair();
    await aliceEngine.startChat("bob.02", "hi");
    await settle();
    await bobEngine.accept(bobEngine.snapshot().requests[0].id);
    await settle();
    await aliceEngine.send(aliceEngine.snapshot().contacts[0].account, "once");
    await settle();
    const bobSubmissions = () =>
      store.submitted.filter((encoded) =>
        equalBytes(decodeStatement(encoded).signer, bob.identity),
      ).length;
    const before = bobSubmissions();
    // Reconnecting replays every stored statement to the new subscriptions.
    for (const connection of store.connections.filter((c) => c.isOpen()))
      connection.drop();
    await new Promise((resolve) => setTimeout(resolve, 80));
    expect(bobSubmissions()).toBe(before);
    const bobSide = bobEngine.snapshot().contacts[0];
    expect(bobSide.messages.filter((m) => m.text === "once").length).toBe(1);
    expect(aliceEngine.snapshot().contacts[0].messages.at(-1)?.status).toBe(
      "acknowledged",
    );
    aliceEngine.stop();
    bobEngine.stop();
  });

  test("two wallets in one storage keep separate records", async () => {
    const storage = new MemoryStorage();
    const store = new MemoryStatementStore();
    const chain = directory([
      { label: "alice.01", keys: keys(ALICE), allowance: true },
      { label: "bob.02", keys: keys(BOB), allowance: true },
    ]);
    const make = (chatKeys: ChatKeys, recordKey: string) =>
      new ChatEngine({
        keys: chatKeys,
        openStore: async () => store.connect(true),
        directory: chain,
        storage,
        recordKey,
        writes: () => true,
      });
    const alice = make(keys(ALICE), "chat:alice");
    const bob = make(keys(BOB), "chat:bob");
    await Promise.all([alice.start(), bob.start()]);
    await alice.startChat("bob.02", "hi");
    await settle();
    expect(alice.snapshot().contacts.length).toBe(1);
    expect(bob.snapshot().contacts).toEqual([]);
    expect(JSON.parse(storage.getItem("chat:bob")!).contacts).toEqual([]);
    alice.stop();
    bob.stop();
  });
});

describe("chat engine diagnostics", () => {
  test("reports an exchange as metadata, with no key, text or token", async () => {
    const store = new MemoryStatementStore();
    const alice = keys(ALICE);
    const bob = keys(BOB);
    const chain = directory([
      { label: "alice.01", keys: alice, allowance: true, registry: "people" },
      { label: "bob.02", keys: bob, allowance: true },
    ]);
    const diagnostics = new ChatDiagnostics();
    const make = (chatKeys: ChatKeys, sink?: ChatDiagnostics) =>
      new ChatEngine({
        keys: chatKeys,
        openStore: async () => store.connect(true),
        directory: chain,
        storage: new MemoryStorage(),
        recordKey: "chat",
        writes: () => true,
        diagnostics: sink?.record,
      });
    const aliceEngine = make(alice);
    const bobEngine = make(bob, diagnostics);
    await Promise.all([aliceEngine.start(), bobEngine.start()]);
    await aliceEngine.startChat("bob.02", "secret welcome words");
    await settle();
    await bobEngine.accept(bobEngine.snapshot().requests[0].id);
    await settle();
    await aliceEngine.send(
      aliceEngine.snapshot().contacts[0].account,
      "secret body text",
    );
    // A push token and a device change from Alice, as the app sends them.
    const token = Uint8Array.of(0xde, 0xad, 0xbe, 0xef, 0xde, 0xad, 0xbe, 0xef);
    const head = (id: string, tag: number) =>
      concat(text(id), u64(9n), Uint8Array.of(0, tag));
    const topic = sessionTopic(
      alice.agree(bob.chatPublic),
      alice.identity,
      bob.identity,
    );
    await store.connect(true).submit(
      encodeSigned(
        {
          expiry: expiryAt(Date.UTC(2026, 9, 16)),
          channel: requestChannel(topic),
          topics: [topic],
          data: sealSession(
            alice.agree(bob.chatPublic),
            {
              kind: "request",
              requestId: "control-request",
              messages: [
                concat(head("c1", 1), bytes(token), Uint8Array.of(1)),
                concat(
                  head("c2", 17),
                  bytes(new Uint8Array(32).fill(3)),
                  bytes(new Uint8Array(32).fill(4)),
                ),
              ],
            },
            cryptoRandom,
          ),
        },
        alice.identity,
        (payload) => alice.sign(payload),
      ),
    );
    await settle();
    bobEngine.stop();
    aliceEngine.stop();

    const events = diagnostics.list();
    const types = events.map((event) => event.type);
    expect(types).toContain("lifecycle");
    expect(types).toContain("subscription");
    expect(events).toContainEqual(
      expect.objectContaining({ type: "request", outcome: "new", version: 1 }),
    );
    expect(events).toContainEqual(
      expect.objectContaining({
        type: "lookup",
        outcome: "found",
        from: ["people"],
      }),
    );
    expect(events).toContainEqual(
      expect.objectContaining({
        type: "submit",
        purpose: "accept",
        result: "stored",
      }),
    );
    expect(events).toContainEqual(
      expect.objectContaining({ type: "session", data: "request", kinds: [0] }),
    );
    expect(events).toContainEqual(
      expect.objectContaining({
        type: "session",
        data: "request",
        kinds: [1, 17],
      }),
    );
    expect(events).toContainEqual(
      expect.objectContaining({
        type: "submit",
        purpose: "ack",
        result: "stored",
      }),
    );
    expect(events.at(-1)).toMatchObject({
      type: "lifecycle",
      state: "stopped",
    });
    // A subscription names the topics it filters on in full: Bob's request
    // inbox, then the topic Alice writes to him on once they are contacts.
    const subscriptions = events.filter(
      (event) => event.type === "subscription",
    );
    expect(subscriptions).toContainEqual(
      expect.objectContaining({
        stream: "requests",
        action: "subscribe",
        topics: 1,
        topicIds: [bytesToHex(requestTopic(bob.identity))],
      }),
    );
    expect(subscriptions).toContainEqual(
      expect.objectContaining({
        stream: "sessions",
        action: "subscribe",
        topics: 1,
        topicIds: [bytesToHex(topic)],
      }),
    );

    const dump = JSON.stringify(events).toLowerCase();
    const hex = (bytes: Uint8Array) => bytesToHex(bytes).slice(2);
    for (const secret of [
      "secret welcome words",
      "secret body text",
      hex(token),
      hex(alice.chatPublic),
      hex(bob.chatPublic),
      hex(alice.agree(bob.chatPublic)),
      hex(alice.identity),
      hex(bob.identity),
      hex(new Uint8Array(32).fill(3)),
    ])
      expect(dump).not.toContain(secret.toLowerCase());
  });

  test("reports a failed user action with its reason", async () => {
    const diagnostics = new ChatDiagnostics();
    const engine = new ChatEngine({
      keys: keys(BOB),
      openStore: async () => new MemoryStatementStore().connect(true),
      directory: directory([
        { label: "bob.02", keys: keys(BOB), allowance: true },
      ]),
      storage: new MemoryStorage(),
      recordKey: "chat",
      writes: () => true,
      diagnostics: diagnostics.record,
    });
    await engine.start();
    await expect(engine.startChat("nobody.99", "hi")).rejects.toThrow();
    expect(diagnostics.list().at(-1)).toEqual(
      expect.objectContaining({
        type: "action-failed",
        action: "start chat",
        reason: "No account holds the username nobody.99.",
      }),
    );
    engine.stop();
  });
});

/** Alice and Bob with an accepted chat, and each side's view of the other. */
async function connected(hop?: HopService) {
  const pairing = await pair(true, hop);
  await pairing.aliceEngine.startChat("bob.02", "hi");
  await settle();
  await pairing.bobEngine.accept(pairing.bobEngine.snapshot().requests[0].id);
  await settle();
  const toBob = pairing.aliceEngine.snapshot().contacts[0].account;
  const toAlice = pairing.bobEngine.snapshot().contacts[0].account;
  return { ...pairing, toBob, toAlice };
}

describe("chat engine reactions", () => {
  test("a reaction reaches the peer on the message it names, and is acknowledged", async () => {
    const { aliceEngine, bobEngine, toBob, toAlice } = await connected();
    await aliceEngine.send(toBob, "lunch?");
    await settle();
    const target = bobEngine
      .snapshot()
      .contacts[0].messages.find((message) => message.text === "lunch?")!;
    await bobEngine.react(toAlice, target.id, "👍");
    await settle();
    const onAlice = aliceEngine
      .snapshot()
      .contacts[0].messages.find((message) => message.id === target.id)!;
    expect(onAlice.reactions).toEqual([{ emoji: "👍", from: "peer" }]);
    expect(bobEngine.snapshot().contacts[0].pending).toEqual([]);
    // No bubble was added for the reaction on either side.
    expect(aliceEngine.snapshot().contacts[0].messages.length).toBe(3);
  });

  test("a new emoji replaces the user's old one, and the same one again removes it", async () => {
    const { aliceEngine, bobEngine, toBob, toAlice } = await connected();
    await aliceEngine.send(toBob, "lunch?");
    await settle();
    const id = bobEngine.snapshot().contacts[0].messages.at(-1)!.id;
    await bobEngine.react(toAlice, id, "👍");
    await bobEngine.react(toAlice, id, "❤️");
    await settle();
    const reactions = () =>
      aliceEngine
        .snapshot()
        .contacts[0].messages.find((message) => message.id === id)!.reactions;
    expect(reactions()).toEqual([{ emoji: "❤️", from: "peer" }]);
    expect(
      bobEngine
        .snapshot()
        .contacts[0].messages.find((message) => message.id === id)!.reactions,
    ).toEqual([{ emoji: "❤️", from: "self" }]);
    await bobEngine.react(toAlice, id, "❤️");
    await settle();
    expect(reactions()).toEqual([]);
  });

  test("a reaction delivered again changes nothing after its removal", async () => {
    const { aliceEngine, bobEngine, toBob, toAlice } = await connected();
    await aliceEngine.send(toBob, "lunch?");
    await settle();
    const id = bobEngine.snapshot().contacts[0].messages.at(-1)!.id;
    bobEngine.stop();
    await bobEngine.react(toAlice, id, "👍").catch(() => {});
    await bobEngine.start();
    await bobEngine.react(toAlice, id, "👍");
    await settle();
    // Alice restarts and the store delivers Bob's latest request again.
    aliceEngine.stop();
    await aliceEngine.start();
    await settle();
    expect(
      aliceEngine
        .snapshot()
        .contacts[0].messages.find((message) => message.id === id)!.reactions ??
        [],
    ).toEqual([]);
  });
});

describe("chat engine block and leave", () => {
  test("a blocked person's messages are not read or acknowledged until unblocked", async () => {
    const { aliceEngine, bobEngine, toBob, toAlice } = await connected();
    await bobEngine.block(toAlice);
    await aliceEngine.send(toBob, "are you there?");
    await settle();
    const bobSide = () => bobEngine.snapshot().contacts[0];
    expect(bobSide().blocked).toBe(true);
    expect(bobSide().messages.some((m) => m.text === "are you there?")).toBe(
      false,
    );
    expect(aliceEngine.snapshot().contacts[0].messages.at(-1)!.status).toBe(
      "sent",
    );
    await expect(bobEngine.send(toAlice, "hi")).rejects.toThrow("You blocked");
    await bobEngine.unblock(toAlice);
    await settle();
    expect(bobSide().blocked).toBeUndefined();
    expect(bobSide().messages.some((m) => m.text === "are you there?")).toBe(
      true,
    );
    expect(aliceEngine.snapshot().contacts[0].messages.at(-1)!.status).toBe(
      "acknowledged",
    );
  });

  test("a blocked person's new request is set aside", async () => {
    const { aliceEngine, bobEngine, toBob, toAlice } = await connected();
    await bobEngine.block(toAlice);
    await aliceEngine.leave(toBob);
    // Alice left, then asks again: Bob blocked her, so no request shows.
    await aliceEngine.startChat("bob.02", "again");
    await settle();
    expect(bobEngine.snapshot().requests).toEqual([]);
  });

  test("leaving tells the peer, keeps the conversation out of the list, and a new request reopens it", async () => {
    const { aliceEngine, bobEngine, toBob, toAlice } = await connected();
    await aliceEngine.send(toBob, "bye");
    await settle();
    await aliceEngine.leave(toBob);
    await settle();
    expect(aliceEngine.snapshot().contacts).toEqual([]);
    const bobSide = bobEngine.snapshot().contacts[0];
    expect(bobSide.peerLeft).toBe(true);
    expect(bobSide.messages.at(-1)).toMatchObject({
      direction: "in",
      kind: "left",
    });
    await expect(bobEngine.send(toAlice, "wait")).rejects.toThrow(
      "left this chat",
    );

    await aliceEngine.startChat("bob.02", "back again");
    await settle();
    const [request] = bobEngine.snapshot().requests;
    expect(request.welcome).toBe("back again");
    await bobEngine.accept(request.id);
    await settle();
    const reopened = bobEngine.snapshot().contacts;
    expect(reopened.length).toBe(1);
    expect(reopened[0].peerLeft).toBe(false);
    expect(reopened[0].messages.map((m) => m.text)).toContain("bye");
  });

  test("leaving changes nothing when the store does not keep the message", async () => {
    const { store, aliceEngine, toBob } = await connected();
    for (const connection of store.connections) connection.drop();
    await expect(aliceEngine.leave(toBob)).rejects.toThrow(
      "The message that you left was not sent",
    );
    expect(aliceEngine.snapshot().contacts.length).toBe(1);
  });
});

const NODE = "wss://hop.invalid";

/** One HOP node in memory both sides use, and a synthetic sender account. */
function memoryHop(): { node: MemoryHopNode; hop: HopService } {
  const node = new MemoryHopNode();
  const secret = sr25519.secretFromSeed(new Uint8Array(32).fill(3));
  return {
    node,
    hop: {
      nodes: () => [NODE],
      connect: async () => node.connect(),
      sender: {
        publicKey: sr25519.getPublicKey(secret),
        sign: (payload) => sr25519.sign(secret, payload),
      },
    },
  };
}

describe("chat engine files", () => {
  test("a file is uploaded, named in a rich text message, and fetched by the peer", async () => {
    const { node, hop } = memoryHop();
    const { aliceEngine, bobEngine, toBob, toAlice } = await connected(hop);
    const bytes = new TextEncoder().encode("synthetic file body");
    await aliceEngine.sendFile(
      toBob,
      { bytes, name: "note.txt", mimeType: "text/plain" },
      "here it is",
    );
    await settle();
    const sent = aliceEngine.snapshot().contacts[0].messages.at(-1)!;
    expect(sent.status).toBe("acknowledged");
    const received = bobEngine.snapshot().contacts[0].messages.at(-1)!;
    expect(received.text).toBe("here it is");
    // The wire carries no file name; the rest arrives as the sender gave it.
    expect(received.attachments).toEqual([
      {
        identifier: sent.attachments![0].identifier,
        ticket: sent.attachments![0].ticket,
        node: NODE,
        kind: "file",
        mimeType: "text/plain",
        size: bytes.length,
      },
    ]);
    expect(await bobEngine.fetchFile(toAlice, received.id, 0)).toEqual(bytes);
    // Fetching acknowledges nothing: the entry stays for other devices.
    expect(node.calls.filter((call) => call === "hop_ack")).toEqual([]);
  });

  test("a file on a node this host was not given is not fetched", async () => {
    const { hop } = memoryHop();
    const { aliceEngine, bobEngine, toBob, toAlice } = await connected(hop);
    await aliceEngine.sendFile(
      toBob,
      { bytes: Uint8Array.of(1, 2, 3), name: "a.bin", mimeType: "" },
      "",
    );
    await settle();
    const received = bobEngine.snapshot().contacts[0].messages.at(-1)!;
    received.attachments![0].node = "wss://elsewhere.invalid";
    await expect(bobEngine.fetchFile(toAlice, received.id, 0)).rejects.toThrow(
      "not configured with",
    );
  });

  test("a failed upload keeps the message, marked failed, and sends nothing", async () => {
    const { node, hop } = memoryHop();
    node.authorized.add(`0x${"00".repeat(32)}`);
    const { store, aliceEngine, toBob } = await connected(hop);
    const before = store.submitted.length;
    await expect(
      aliceEngine.sendFile(
        toBob,
        { bytes: Uint8Array.of(1), name: "a.bin", mimeType: "" },
        "",
      ),
    ).rejects.toThrow("no storage authorization");
    const message = aliceEngine.snapshot().contacts[0].messages.at(-1)!;
    expect(message.status).toBe("failed");
    expect(message.problem).toContain("The file was not uploaded");
    expect(store.submitted.length).toBe(before);
  });

  test("files are refused without a node, and above 128 MiB", async () => {
    const { aliceEngine, toBob } = await connected();
    expect(aliceEngine.fileProblem()).toContain("No HOP node");
    await expect(
      aliceEngine.sendFile(
        toBob,
        { bytes: Uint8Array.of(1), name: "a", mimeType: "" },
        "",
      ),
    ).rejects.toThrow("No HOP node");
    const { hop } = memoryHop();
    const withHop = await connected(hop);
    await expect(
      withHop.aliceEngine.sendFile(
        withHop.toBob,
        {
          bytes: { length: HOP_MAX_FILE_BYTES + 1 } as Uint8Array,
          name: "big",
          mimeType: "",
        },
        "",
      ),
    ).rejects.toThrow("128 MiB");
  });

  test("diagnostics keep the plaintext with every ticket replaced by zeros", async () => {
    const { hop } = memoryHop();
    const pairing = await pair(true, hop);
    const diagnostics = new ChatDiagnostics();
    diagnostics.payloads.setLevel("decrypted");
    const watched = new ChatEngine({
      keys: pairing.alice,
      openStore: async () => pairing.store.connect(true),
      directory: directory([
        { label: "alice.01", keys: pairing.alice, allowance: true },
        { label: "bob.02", keys: pairing.bob, allowance: true },
      ]),
      storage: new MemoryStorage(),
      recordKey: "watched",
      writes: () => true,
      hop,
      payloads: diagnostics.payloads,
    });
    await watched.start();
    await watched.startChat("bob.02", "hi");
    await settle();
    await pairing.bobEngine.accept(pairing.bobEngine.snapshot().requests[0].id);
    await settle();
    const toBob = watched.snapshot().contacts[0].account;
    await watched.sendFile(
      toBob,
      { bytes: Uint8Array.of(5, 6), name: "x", mimeType: "" },
      "",
    );
    await settle();
    const ticket = watched.snapshot().contacts[0].messages.at(-1)!
      .attachments![0].ticket!;
    const kept = JSON.stringify(diagnostics.payloads.snapshot());
    expect(kept.length).toBeGreaterThan(0);
    expect(kept.includes(ticket.slice(2))).toBe(false);
    const plaintexts = diagnostics.payloads
      .snapshot()
      .statements.flatMap((entry) =>
        entry.plaintext === null
          ? []
          : [Buffer.from(entry.plaintext).toString("hex")],
      );
    expect(plaintexts.some((text) => text.includes(ticket.slice(2)))).toBe(
      false,
    );
    expect(plaintexts.some((text) => text.includes("00".repeat(32)))).toBe(
      true,
    );
    watched.stop();
  });
});

describe("chat engine call signals", () => {
  test("an offer becomes a call line on both sides, and the answer and end reach the peer", async () => {
    const { aliceEngine, bobEngine, toBob, toAlice } = await connected();
    const heard: string[] = [];
    bobEngine.onCall((signal) =>
      heard.push(`${signal.content.kind} ${signal.id}`),
    );
    const offerId = await aliceEngine.sendCall(toBob, {
      kind: "call-offer",
      sdp: Uint8Array.of(1, 2),
      video: true,
    });
    await settle();
    expect(heard).toEqual([`call-offer ${offerId}`]);
    const line = () =>
      bobEngine.snapshot().contacts[0].messages.find((m) => m.id === offerId)!;
    expect(line()).toMatchObject({
      kind: "call",
      direction: "in",
      call: { video: true },
    });
    // Acknowledged, the offer's setup is no longer kept.
    const own = aliceEngine
      .snapshot()
      .contacts[0].messages.find((m) => m.id === offerId)!;
    expect(own.status).toBe("acknowledged");
    expect(own.call?.setup).toBeUndefined();

    const heardByAlice: string[] = [];
    aliceEngine.onCall((signal) => heardByAlice.push(signal.content.kind));
    await bobEngine.sendCall(toAlice, {
      kind: "call-answer",
      offerId,
      sdp: Uint8Array.of(3),
    });
    await bobEngine.sendCall(toAlice, {
      kind: "call-candidates",
      offerId,
      sdp: Uint8Array.of(4),
    });
    await settle();
    await aliceEngine.sendCall(toBob, { kind: "call-closed", offerId });
    await settle();
    expect(heardByAlice).toEqual(["call-answer", "call-candidates"]);
    expect(line().call).toMatchObject({
      answeredAt: expect.any(Number),
      closedBy: "peer",
    });
    // Signals are not bubbles: the conversation holds one line for the call.
    expect(
      bobEngine.snapshot().contacts[0].messages.filter((m) => m.kind === "call")
        .length,
    ).toBe(1);
    expect(bobEngine.snapshot().contacts[0].pending).toEqual([]);
  });
});
