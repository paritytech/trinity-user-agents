import { describe, expect, test } from "bun:test";
import {
  SignedStatement,
  type HexString,
  type SignedStatement as SignedStatementValue,
} from "@parity/truapi";
import { bytesToHex, hexToBytes } from "@parity/truapi/scale";
import { mnemonicToEntropy } from "@scure/bip39";
import { wordlist } from "@scure/bip39/wordlists/english.js";
import { ok } from "neverthrow";
import { equalBytes } from "../chat/codec.js";
import { cryptoRandom, x25519Agree } from "../chat/crypto.js";
import { ChatDiagnostics } from "../chat/diagnostics.js";
import type { ChainDirectory } from "../chat/directory.js";
import { ChatEngine } from "../chat/engine.js";
import { localNovaIdentity } from "../chat/identity.js";
import {
  ChatKeys,
  deriveChatSecret,
  deriveIdentitySecret,
} from "../chat/keys.js";
import { MemoryStatementRpc } from "../chat/memory-rpc.js";
import {
  requestChannel,
  requestTopic,
  responseChannel,
  sessionTopic,
} from "../chat/protocol.js";
import { decodeStatement, verifyStatement } from "../chat/statement.js";
import { StatementStore } from "../chat/statement-store.js";
import { abbreviateAccount, chatRecordKey } from "../chat/store.js";
import { ChatInbox } from "../chat-inbox.js";
import { messagePortBridge } from "../chat-v2/contract.js";
import { ChatV2Host } from "../chat-v2/host.js";
import { chatV2Over } from "../chat-v2/product.js";
import { ChatV2Provider } from "../chat-v2/provider.js";
import { NOVA_PROVIDER_ID, type ChatSession } from "../nova-chat-provider.js";
import { ProductChat } from "../product-chat.js";
import { MemoryStorage } from "../test-support.js";
import {
  serveConversations,
  type ConversationEngine,
  type ReadMarks,
} from "./conversations.js";
import {
  connectNovaIdentity,
  refuseNovaIdentity,
  serveNovaIdentity,
  type IdentityEnvelope,
  type IdentityPort,
} from "./identity-bridge.js";
import { novaConversations } from "./nova-conversations.js";
import {
  ProductStorage,
  fromSignedStatement,
  toSignedStatement,
  truapiStatementStore,
} from "./truapi-ports.js";
import {
  NOVA_WORKER_PRODUCT,
  NovaWorkerMode,
  START_REFERENCE_TTL_MS,
} from "./worker-mode.js";
import { HostNames } from "../host-names.js";

/** Synthetic phrases from the BIP-39 test vectors. They hold nothing. */
const ALICE =
  "legal winner thank year wave sausage worth useful legal winner thank yellow";
const BOB =
  "letter advice cage absurd amount doctor acoustic avoid letter advice cage above";
const entropy = (phrase: string) => mnemonicToEntropy(phrase, wordlist);
const keysOf = (phrase: string) =>
  ChatKeys.fromEntropy(entropy(phrase), "paseo");

async function waitFor(check: () => boolean, what: string, ms = 3_000) {
  const until = Date.now() + ms;
  while (!check()) {
    if (Date.now() > until) throw new Error(`timed out waiting for ${what}`);
    await Bun.sleep(5);
  }
}

function directory(
  people: { label: string; keys: ChatKeys }[],
): ChainDirectory {
  const find = (account: Uint8Array) =>
    people.find((person) => equalBytes(person.keys.identity, account));
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
      const person = people.find((entry) => entry.label === label);
      return { assetHub: person?.keys.identity ?? null, people: null };
    },
    async hasAllowance() {
      return true;
    },
  };
}

/** Both ends of a page-local channel, with every envelope on it kept. */
function pipe() {
  const channel = new MessageChannel();
  const seen: IdentityEnvelope[] = [];
  const end = (port: MessagePort): IdentityPort => ({
    post: (message) => {
      seen.push(message);
      port.postMessage(message);
    },
    listen: (handler) => {
      port.onmessage = (event: MessageEvent<IdentityEnvelope>) =>
        handler(event.data);
      port.start();
    },
    close: () => port.close(),
  });
  return { host: end(channel.port2), product: end(channel.port1), seen };
}

/**
 * The parts of a product's TrUAPI client the worker uses, over an in-memory
 * statement store. Statements cross as the wire's `SignedStatement`, encoded
 * and decoded with its SCALE codec, and reach the store as the core turns
 * them back into bytes. Storage is one map, as the host keeps a product's.
 */
function productClient(rpc: MemoryStatementRpc, kept: Map<string, HexString>) {
  const store = new StatementStore(rpc.connect());
  const wire = (statement: SignedStatementValue) =>
    SignedStatement.dec(SignedStatement.enc(statement));
  const submitted: Uint8Array[] = [];
  const client = {
    statementStore: {
      subscribe: ({
        request,
      }: {
        request: { tag: "MatchAll" | "MatchAny"; value: HexString[] };
      }) => ({
        subscribe(observer: {
          next(item: { statements: SignedStatementValue[] }): void;
        }) {
          const topics = request.value.map(hexToBytes);
          let stop: (() => void) | null = null;
          let ended = false;
          void store
            .subscribe(
              request.tag === "MatchAll"
                ? { matchAll: topics }
                : { matchAny: topics },
              (statements) =>
                observer.next({
                  statements: statements.map((bytes) =>
                    wire(toSignedStatement(bytes)),
                  ),
                }),
            )
            .then((unsubscribe) => {
              if (ended) unsubscribe();
              else stop = unsubscribe;
            });
          return {
            subscriptionId: "test",
            unsubscribe: () => {
              ended = true;
              stop?.();
            },
          };
        },
      }),
      async submit(statement: SignedStatementValue) {
        const bytes = fromSignedStatement(wire(statement))!;
        submitted.push(bytes);
        await store.submit(bytes);
        return ok(undefined);
      },
    },
    localStorage: {
      async read({ key }: { key: string }) {
        const value = kept.get(key);
        return ok(value === undefined ? {} : { value });
      },
      async write({ key, value }: { key: string; value: HexString }) {
        kept.set(key, value);
        return ok(undefined);
      },
      async clear({ key }: { key: string }) {
        kept.delete(key);
        return ok(undefined);
      },
    },
  };
  return { client: client as never, submitted };
}

const RECORD = "nova-chat:paseo:bob:v1";

/**
 * Alice on the host's own engine, and Bob's engine as the Nova chat worker
 * runs it: keys lent over the identity bridge, the Statement Store and
 * storage through the product's client.
 */
async function worlds() {
  const rpc = new MemoryStatementRpc();
  const alice = keysOf(ALICE);
  const bob = keysOf(BOB);
  const chain = directory([
    { label: "alice.01", keys: alice },
    { label: "bob.02", keys: bob },
  ]);
  const aliceEngine = new ChatEngine({
    identity: localNovaIdentity(alice),
    openStore: async () => new StatementStore(rpc.connect()),
    directory: chain,
    storage: new MemoryStorage(),
    recordKey: "alice",
    writes: () => true,
    retryDelayMs: 10,
  });
  const kept = new Map<string, HexString>();
  const lent = pipe();
  const service = serveNovaIdentity(lent.host, {
    identity: localNovaIdentity(bob),
  });
  const lease = await connectNovaIdentity(lent.product);
  const { client, submitted } = productClient(rpc, kept);
  const startBob = async () => {
    const storage = await ProductStorage.load(client, [RECORD], () => {});
    const engine = new ChatEngine({
      identity: lease.identity,
      openStore: async () => truapiStatementStore(client),
      directory: chain,
      storage,
      recordKey: RECORD,
      writes: () => true,
      retryDelayMs: 10,
    });
    await engine.start();
    return { engine, storage, conversations: novaConversations(engine) };
  };
  await aliceEngine.start();
  return {
    rpc,
    alice,
    bob,
    aliceEngine,
    startBob,
    kept,
    lent,
    service,
    submitted,
  };
}

const hex = (value: unknown): string =>
  JSON.stringify(value, (_key, entry) =>
    entry instanceof Uint8Array
      ? bytesToHex(entry)
      : typeof entry === "bigint"
        ? String(entry)
        : entry,
  );

describe("the Nova chat worker's engine", () => {
  test("talks to the host's own engine with the real wire, signing and sealing only through lent operations", async () => {
    const w = await worlds();
    const bob = await w.startBob();
    const aliceAccount = bytesToHex(w.alice.identity);

    await w.aliceEngine.startChat("bob.02", "hello from alice");
    await waitFor(
      () => bob.conversations.conversations().length === 1,
      "the request",
    );
    // The engine names the person by account only; the host names them.
    expect(bob.conversations.conversations()[0]).toMatchObject({
      id: aliceAccount,
      title: abbreviateAccount(aliceAccount),
      subtitle: "Chat request",
      composer: { allowed: true },
      contact: { account: aliceAccount, state: "Incoming" },
    });
    expect(
      bob.conversations
        .messages(aliceAccount)
        .map((m) => [m.author.kind, m.text]),
    ).toEqual([
      ["participant", "hello from alice"],
      ["system", "Wants to chat with you. Reply to accept."],
    ]);

    // Bob's first reply accepts, then goes out as text.
    const sent = await bob.conversations.send(aliceAccount, "hi alice");
    await waitFor(
      () =>
        bob.conversations.messages(aliceAccount).find((m) => m.id === sent)
          ?.state === "delivered",
      "Alice to acknowledge Bob's text",
    );
    const aliceSide = w.aliceEngine.snapshot().contacts[0];
    expect(aliceSide.state).toBe("active");
    expect(
      aliceSide.messages.map((m) => [m.direction, m.kind, m.text]),
    ).toEqual([
      ["out", "request", "hello from alice"],
      ["in", "accepted", "Accepted the chat request"],
      ["in", "text", "hi alice"],
    ]);

    // Alice answers; the worker's engine reads and acknowledges it.
    await w.aliceEngine.send(aliceSide.account, "how are you");
    await waitFor(
      () =>
        w.aliceEngine.snapshot().contacts[0].messages.at(-1)?.status ===
        "acknowledged",
      "Bob's worker to acknowledge",
    );
    expect(
      bob.conversations
        .messages(aliceAccount)
        .map((m) => [m.author.kind, m.text]),
    ).toEqual([
      ["participant", "hello from alice"],
      ["system", "You accepted the chat request"],
      ["self", "hi alice"],
      ["participant", "how are you"],
    ]);

    // Every statement Bob's worker submitted is the identity account's own
    // signature, unchanged by the trip through the product's client.
    expect(w.submitted.length).toBeGreaterThanOrEqual(2);
    for (const bytes of w.submitted) {
      const statement = decodeStatement(bytes);
      expect(bytesToHex(statement.signer)).toBe(bytesToHex(w.bob.identity));
      expect(verifyStatement(statement)).toBe(true);
    }
    // Messages and acknowledgements go on their own channels of Bob's
    // session topic, so a newer request never replaces an acknowledgement.
    const topic = sessionTopic(
      w.alice.agree(w.bob.chatPublic),
      w.bob.identity,
      w.alice.identity,
    );
    const channels = new Set(
      w.submitted
        .map(decodeStatement)
        .filter((statement) => equalBytes(statement.topics[0], topic))
        .map((statement) => bytesToHex(statement.channel!)),
    );
    expect(channels).toEqual(
      new Set([
        bytesToHex(requestChannel(topic)),
        bytesToHex(responseChannel(topic)),
      ]),
    );
    // What the store holds from Bob is what the worker submitted, and every
    // held statement survives the trip through the wire's shape unchanged.
    const fromBob = w.rpc.statements.filter((held) =>
      equalBytes(decodeStatement(held).signer, w.bob.identity),
    );
    expect(fromBob.length).toBeGreaterThan(0);
    for (const held of fromBob)
      expect(w.submitted.some((bytes) => equalBytes(bytes, held))).toBe(true);
    for (const held of w.rpc.statements)
      expect(
        bytesToHex(
          fromSignedStatement(
            SignedStatement.dec(SignedStatement.enc(toSignedStatement(held))),
          )!,
        ),
      ).toBe(bytesToHex(held));

    // No secret crossed the identity bridge: not the entropy, the identity
    // secret, the chat secret, or the secret agreed with Alice.
    const chatSecret = deriveChatSecret(entropy(BOB));
    const secrets = [
      entropy(BOB),
      deriveIdentitySecret(entropy(BOB), "paseo"),
      chatSecret,
      x25519Agree(chatSecret, w.alice.chatPublic),
    ].map((secret) => bytesToHex(secret).slice(2));
    const wire = hex(w.lent.seen);
    for (const secret of secrets) expect(wire).not.toContain(secret);
    expect(
      new Set(
        w.lent.seen.flatMap((m) => (m.kind === "call" ? [m.method] : [])),
      ),
    ).toEqual(
      new Set([
        "hello",
        "openRequest",
        "sessionTopics",
        "sessionStatement",
        "openSession",
      ]),
    );
    bob.engine.stop();
    w.aliceEngine.stop();
  });

  test("keeps its history in the product's storage, and picks up where it stopped after a restart", async () => {
    const w = await worlds();
    let bob = await w.startBob();
    const aliceAccount = bytesToHex(w.alice.identity);
    await w.aliceEngine.startChat("bob.02", "hello");
    await waitFor(
      () => bob.conversations.conversations().length === 1,
      "the request",
    );
    await bob.conversations.send(aliceAccount, "hi");
    await waitFor(
      () => w.aliceEngine.snapshot().contacts[0]?.state === "active",
      "the acceptance",
    );
    await bob.storage.flush();
    const before = bob.engine.snapshot().contacts;
    bob.engine.stop();
    expect(w.kept.has(RECORD)).toBe(true);

    // Alice writes while the worker is stopped.
    await w.aliceEngine.send(
      w.aliceEngine.snapshot().contacts[0].account,
      "while you were away",
    );
    bob = await w.startBob();
    const ids = (contacts: typeof before) =>
      contacts[0].messages.map((m) => [m.id, m.kind, m.direction]);
    expect(
      ids(bob.engine.snapshot().contacts).slice(0, before[0].messages.length),
    ).toEqual(ids(before));
    await waitFor(
      () =>
        bob.conversations
          .messages(aliceAccount)
          .some((m) => m.text === "while you were away"),
      "the message sent while stopped",
    );
    const texts = bob.conversations
      .messages(aliceAccount)
      .filter((m) => m.author.kind === "participant")
      .map((m) => m.text);
    expect(texts).toEqual(["hello", "while you were away"]);
    bob.engine.stop();
    w.aliceEngine.stop();
  });
});

describe("the identity bridge", () => {
  async function lent() {
    const ends = pipe();
    const service = serveNovaIdentity(ends.host, {
      identity: localNovaIdentity(keysOf(BOB), cryptoRandom),
      history: () => '{"contacts":[]}',
    });
    const client = await connectNovaIdentity(ends.product, 100);
    return { ends, service, client };
  }

  test("signs only statements it builds itself", async () => {
    const { client, ends } = await lent();
    const alice = keysOf(ALICE);
    const garbage = new Uint8Array([9, 9, 9]);
    await expect(
      client.identity.sessionStatement({
        peer: alice.identity,
        peerChatKey: alice.chatPublic,
        plaintext: garbage,
        expiry: 1n,
      }),
    ).rejects.toThrow();
    await expect(
      client.identity.sessionTopics(alice.identity, new Uint8Array(31)),
    ).rejects.toThrow("peerChatKey must be 32 bytes.");
    // No method signs bytes the caller chose.
    const answer = new Promise<IdentityEnvelope>((resolve) =>
      ends.product.listen(resolve),
    );
    ends.product.post({
      kind: "call",
      id: 99,
      method: "sign" as never,
      request: { payload: garbage },
    });
    expect(await answer).toEqual({
      kind: "result",
      id: 99,
      ok: false,
      reason: "There is no method sign.",
    });
  });

  test("hands the offered history once, and nothing after it is closed", async () => {
    const { client, service } = await lent();
    expect(await client.takeHistory()).toBe('{"contacts":[]}');
    expect(await client.takeHistory()).toBeNull();
    service.close();
    const alice = keysOf(ALICE);
    await expect(
      client.identity.sessionTopics(alice.identity, alice.chatPublic),
    ).rejects.toThrow("did not answer");
  });

  test("a frame with no grant is refused", async () => {
    const ends = pipe();
    refuseNovaIdentity(ends.host, "Chat runs in the host, not in this worker.");
    await expect(connectNovaIdentity(ends.product, 1_000)).rejects.toThrow(
      "Chat runs in the host, not in this worker.",
    );
  });
});

describe("the chat engine switch", () => {
  function setup(keys: ChatKeys | string = keysOf(BOB)) {
    const storage = new MemoryStorage();
    storage.setItem(
      chatRecordKey("paseo", "w1"),
      '{"contacts":[],"requests":[]}',
    );
    let changes = 0;
    const mode = new NovaWorkerMode({
      storage,
      base: "/",
      origin: "http://localhost:5180",
      changed: () => (changes += 1),
      log: () => {},
    });
    const signIn = (walletId: string) =>
      mode.setSession({
        network: "paseo",
        walletId,
        identity: walletId,
        keys: () => keys,
      });
    const ask = async (productId: string) => {
      const ends = pipe();
      mode.connect(productId, ends.host);
      return connectNovaIdentity(ends.product, 200);
    };
    return { mode, storage, signIn, ask, changes: () => changes };
  }

  test("runs the host's engine by default and lends the identity to no frame", async () => {
    const { mode, signIn, ask } = setup();
    signIn("w1");
    expect(mode.mode()).toBe("host");
    expect(mode.hostRuns()).toBe(true);
    expect(mode.workerNames()).toEqual([]);
    await expect(ask(NOVA_WORKER_PRODUCT)).rejects.toThrow(
      "Chat runs in the host",
    );
  });

  test("in worker mode lends the identity to the Nova chat worker only, once per switch with the history", async () => {
    const { mode, signIn, ask, changes } = setup();
    signIn("w1");
    mode.setMode("worker", { copyHistory: true });
    expect(changes()).toBe(1);
    expect(mode.hostRuns()).toBe(false);
    expect(mode.workerNames()).toEqual([NOVA_WORKER_PRODUCT]);
    await expect(ask("chat.paseo")).rejects.toThrow(
      "chat.paseo may not use this wallet's chat identity.",
    );
    const lease = await ask(NOVA_WORKER_PRODUCT);
    expect(bytesToHex(lease.identity.identity)).toBe(
      bytesToHex(keysOf(BOB).identity),
    );
    expect(await lease.takeHistory()).toBe('{"contacts":[],"requests":[]}');
    // A worker restarted later gets no second copy.
    const again = await ask(NOVA_WORKER_PRODUCT);
    expect(await again.takeHistory()).toBeNull();
    // The first frame's link ended when the second one connected.
    const alice = keysOf(ALICE);
    await expect(
      lease.identity.sessionTopics(alice.identity, alice.chatPublic),
    ).rejects.toThrow("did not answer");
  });

  test("switching back, signing out or another wallet ends the lent identity", async () => {
    const { mode, signIn, ask } = setup();
    const alice = keysOf(ALICE);
    signIn("w1");
    mode.setMode("worker");
    const lease = await ask(NOVA_WORKER_PRODUCT);
    await lease.identity.sessionTopics(alice.identity, alice.chatPublic);
    mode.setMode("host");
    await expect(
      lease.identity.sessionTopics(alice.identity, alice.chatPublic),
    ).rejects.toThrow("did not answer");

    mode.setMode("worker");
    const second = await ask(NOVA_WORKER_PRODUCT);
    signIn("w2");
    // The choice is the wallet's: w2 still runs the host's engine.
    expect(mode.mode()).toBe("host");
    await expect(
      second.identity.sessionTopics(alice.identity, alice.chatPublic),
    ).rejects.toThrow("did not answer");
    signIn("w1");
    expect(mode.mode()).toBe("worker");
    mode.setSession(null);
    expect(mode.mode()).toBe("host");
  });

  test("keys that fail their check are never lent", async () => {
    const { mode, signIn, ask } = setup(
      "The host derived a different chat key than the core. Chat stays off.",
    );
    signIn("w1");
    mode.setMode("worker");
    await expect(ask(NOVA_WORKER_PRODUCT)).rejects.toThrow(
      "different chat key",
    );
    expect(mode.link()).toMatchObject({ state: "refused" });
  });

  test("the inbox stops the host's engine before the worker runs, and starts it again after", async () => {
    const bob = keysOf(BOB);
    const storage = new MemoryStorage();
    let connects = 0;
    const session = {
      walletId: "w1",
      entropy: entropy(BOB),
      network: "paseo",
      networkSuffix: "paseo",
      identityAccountId: bytesToHex(bob.identity),
      chatPublicKey: bytesToHex(bob.chatPublic),
      username: undefined,
      chain: {
        connect: async () => {
          connects += 1;
          throw new Error("offline");
        },
      },
      genesis: { assetHub: "0x00", people: "0x00" },
      identityBackend: null,
    } as unknown as ChatSession;
    const mode = new NovaWorkerMode({
      storage,
      base: "/",
      origin: "http://localhost",
      changed: () => inbox.syncHostChat(),
      log: () => {},
    });
    mode.setSession({
      network: "paseo",
      walletId: "w1",
      identity: "w1",
      keys: () => bob,
    });
    const inbox = new ChatInbox({
      storage,
      diagnostics: new ChatDiagnostics(),
      rooms: {
        chat: new ProductChat(storage, () => 1),
        channel: () => null,
        workerState: () => null,
        onWorkersChange: () => () => {},
        archiveImage: async () => null,
        gateway: () => "https://gateway.invalid",
        log: () => {},
      },
      workerNames: () => mode.workerNames(),
      log: () => {},
      hostChat: () => mode.hostRuns(),
    });
    const ids = () => inbox.providers().map((provider) => provider.id);
    inbox.setSession(session);
    expect(ids()).toContain(NOVA_PROVIDER_ID);
    await waitFor(() => connects > 0, "the host's engine to connect");

    mode.setMode("worker");
    expect(ids()).not.toContain(NOVA_PROVIDER_ID);
    expect(inbox.problem()).toBeNull();
    const stoppedAt = connects;
    await Bun.sleep(60);
    expect(connects).toBe(stoppedAt);

    mode.setMode("host");
    expect(ids()).toContain(NOVA_PROVIDER_ID);
    inbox.setSession(null);
  });
});

describe("the conversation adapter", () => {
  /** A made-up engine with one conversation, to drive the adapter alone. */
  function fakeEngine() {
    const listeners = new Set<() => void>();
    const messages = [
      {
        id: "m1",
        author: { kind: "participant" as const, id: "ada" },
        text: "hello",
        at: 10,
        state: "received" as const,
      },
    ];
    let next = 1;
    const engine: ConversationEngine & { arrive(text: string): void } = {
      conversations: () => [
        {
          id: "ada",
          title: "Ada",
          participants: [{ id: "ada", name: "Ada" }],
          composer: { allowed: true },
        },
      ],
      messages: () => [...messages],
      async send(_id, text) {
        const id = `s${next++}`;
        messages.push({
          id,
          author: { kind: "self" } as never,
          text,
          at: 20,
          state: "sending" as never,
        });
        for (const listener of listeners) listener();
        return id;
      },
      async retry() {},
      onChange: (listener) => {
        listeners.add(listener);
        return () => listeners.delete(listener);
      },
      arrive(text) {
        messages.push({
          id: `in${next++}`,
          author: { kind: "participant", id: "ada" },
          text,
          at: 30,
          state: "received",
        });
        for (const listener of listeners) listener();
      },
    };
    return engine;
  }

  test("shows an engine's conversations through the v2 draft bridge, with sends, arrivals and reads", async () => {
    const host = new ChatV2Host({ media: () => null, log: () => {} });
    const channel = new MessageChannel();
    host.connect("localhost", messagePortBridge(channel.port2));
    const product = chatV2Over(messagePortBridge(channel.port1));
    const marks = new Map<string, string>();
    const reads: ReadMarks = {
      get: (id) => marks.get(id) ?? null,
      set: (id, messageId) => void marks.set(id, messageId),
    };
    const engine = fakeEngine();
    const served = serveConversations(product, engine, { reads });
    await served.ready;
    const provider = new ChatV2Provider("localhost", host);
    expect(provider.conversations()).toMatchObject([
      { id: "ada", title: "Ada", preview: "hello", unread: 1 },
    ]);
    const page = await provider.timeline("ada");
    expect(page.messages.map((m) => [m.sender.label, m.status])).toEqual([
      ["Ada", "received"],
    ]);
    await provider.send("ada", "hi ada");
    // Sending reads the conversation through the user's own message.
    await waitFor(
      () =>
        provider.conversations()[0].preview === "You: hi ada" &&
        provider.conversations()[0].unread === 0,
      "the preview",
    );
    engine.arrive("are you there?");
    await waitFor(
      () => provider.conversations()[0].unread === 1,
      "the arrival",
    );
    const newest = (await provider.timeline("ada")).messages.at(-1)!;
    expect(newest).toMatchObject({ status: "received" });
    provider.markRead("ada", newest.id);
    await waitFor(() => provider.conversations()[0].unread === 0, "the read");
    served.stop();
  });
});

describe("starting a chat in the Nova chat worker from the host's picker", () => {
  const CAROL = "zoo zoo zoo zoo zoo zoo zoo zoo zoo zoo zoo wrong";
  /** What the host's picker showed for Alice: host-only, never for the product. */
  const PICKED_NAME = "Alice from the host picker";

  /**
   * Alice and Carol on the host's own engine; Bob's chat in the Nova chat
   * worker as the page runs it: the engine switch lends his identity, the v2
   * draft bridge carries his rooms to the host's provider, and the host
   * names the people. Every envelope on both bridges is kept.
   */
  async function setup() {
    const rpc = new MemoryStatementRpc();
    const alice = keysOf(ALICE);
    const bob = keysOf(BOB);
    const carol = keysOf(CAROL);
    const chain = directory([
      { label: "alice.01", keys: alice },
      { label: "bob.02", keys: bob },
      { label: "carol.03", keys: carol },
    ]);
    const hostEngine = (keys: ChatKeys, record: string) =>
      new ChatEngine({
        identity: localNovaIdentity(keys),
        openStore: async () => new StatementStore(rpc.connect()),
        directory: chain,
        storage: new MemoryStorage(),
        recordKey: record,
        writes: () => true,
        retryDelayMs: 10,
      });
    const aliceEngine = hostEngine(alice, "alice");
    const carolEngine = hostEngine(carol, "carol");
    let clock = 1_000_000;
    const blocked = new Set<string>();
    const mode = new NovaWorkerMode({
      storage: new MemoryStorage(),
      base: "/",
      origin: "http://localhost",
      changed: () => {},
      log: () => {},
      blocked: (account) => blocked.has(account),
      now: () => clock,
    });
    mode.setSession({
      network: "paseo",
      walletId: "bob",
      identity: "bob",
      keys: () => bob,
    });
    mode.setMode("worker");
    const lent = pipe();
    mode.connect(NOVA_WORKER_PRODUCT, lent.host);
    const lease = await connectNovaIdentity(lent.product);

    const host = new ChatV2Host({
      media: () => null,
      contactRooms: (productId) => mode.isChatProduct(productId),
      log: () => {},
    });
    const v2: unknown[] = [];
    const channel = new MessageChannel();
    host.connect(NOVA_WORKER_PRODUCT, messagePortBridge(channel.port2));
    const productEnd = messagePortBridge(channel.port1);
    const product = chatV2Over({
      post: (message) => {
        v2.push(message);
        productEnd.post(message);
      },
      listen: (handler) =>
        productEnd.listen((message) => {
          v2.push(message);
          handler(message);
        }),
      close: () => productEnd.close(),
    });

    const names = new HostNames();
    names.setSession({
      scope: "paseo:bob",
      record: () => null,
      directory: () => chain,
    });
    const { client, submitted } = productClient(rpc, new Map());
    const storage = await ProductStorage.load(client, [RECORD], () => {});
    const engine = new ChatEngine({
      identity: lease.identity,
      openStore: async () => truapiStatementStore(client),
      directory: chain,
      storage,
      recordKey: RECORD,
      writes: () => true,
      retryDelayMs: 10,
    });
    const marks = new Map<string, string>();
    const served = serveConversations(
      product,
      novaConversations(engine, { resolveContact: lease.resolveContact }),
      {
        reads: {
          get: (id) => marks.get(id) ?? null,
          set: (id, messageId) => void marks.set(id, messageId),
        },
      },
    );
    const provider = new ChatV2Provider(NOVA_WORKER_PRODUCT, host, Date.now, {
      names: (account) => names.name(account),
      onNamesChange: (listener) => names.onChange(listener),
      start: {
        reference: (account) => mode.startReference(account),
        blocked: () => null,
      },
    });
    await Promise.all([
      aliceEngine.start(),
      carolEngine.start(),
      engine.start(),
      served.ready,
    ]);
    const stop = () => {
      served.stop();
      provider.dispose();
      for (const running of [aliceEngine, carolEngine, engine]) running.stop();
    };
    return {
      rpc,
      alice,
      bob,
      carol,
      aliceEngine,
      carolEngine,
      engine,
      mode,
      lent,
      lease,
      host,
      v2,
      names,
      provider,
      submitted,
      blocked,
      tick: (ms: number) => (clock += ms),
      stop,
    };
  }

  /** Bob's chat requests in the store, to `to`. */
  const requestsTo = (rpc: MemoryStatementRpc, to: ChatKeys, from: ChatKeys) =>
    rpc.statements.filter((bytes) => {
      const statement = decodeStatement(bytes);
      return (
        equalBytes(statement.signer, from.identity) &&
        statement.topics.some((topic) =>
          equalBytes(topic, requestTopic(to.identity)),
        )
      );
    });

  test("sends the request through the worker's engine to the person chosen, and the host names their room", async () => {
    const w = await setup();
    try {
      const alice = bytesToHex(w.alice.identity);
      w.names.remember(alice, PICKED_NAME);
      const start = w.provider.start!;
      expect(start.blocked()).toBeNull();
      expect(start.conversationWith!(alice)).toBeNull();

      const roomId = await start.start(alice, "hi from the worker");
      expect(start.conversationWith!(alice)).toBe(roomId);
      expect(w.provider.conversations()).toMatchObject([
        { id: roomId, title: PICKED_NAME, seed: alice.toLowerCase() },
      ]);
      expect(w.provider.composer(roomId)).toEqual({
        kind: "blocked",
        reason: "Waiting for them to accept your request",
      });

      // Alice's own engine gets Bob's request, signed by his identity.
      await waitFor(
        () => w.aliceEngine.snapshot().requests.length === 1,
        "Alice to see the request",
      );
      expect(w.aliceEngine.snapshot().requests[0]).toMatchObject({
        peer: bytesToHex(w.bob.identity),
        welcome: "hi from the worker",
        problem: null,
      });
      expect(requestsTo(w.rpc, w.alice, w.bob)).toHaveLength(1);
      expect(
        w.submitted.every((bytes) =>
          equalBytes(decodeStatement(bytes).signer, w.bob.identity),
        ),
      ).toBe(true);

      // She accepts: the room is established, and the user can write.
      await w.aliceEngine.accept(w.aliceEngine.snapshot().requests[0].id);
      await waitFor(
        () =>
          w.host.room(NOVA_WORKER_PRODUCT, roomId)?.contact?.state ===
          "Established",
        "the room to be established",
      );
      expect(w.provider.composer(roomId)).toEqual({ kind: "text" });

      // The host's names never crossed either bridge: not the picker's name,
      // nor the username the chain holds.
      const crossed = hex([w.v2, w.lent.seen]);
      expect(crossed).not.toContain(PICKED_NAME);
      expect(crossed).not.toContain("alice.01");
      // The start carried a reference, not the account; the account crossed
      // only once, from the identity bridge, as the Nova wire needs it.
      const startQuery = w.v2.find(
        (message) =>
          (message as { method?: string }).method === "startConversation",
      ) as { request: { contact: string; welcome: string } };
      expect(startQuery.request.welcome).toBe("hi from the worker");
      expect(startQuery.request.contact).not.toContain(alice.slice(2));
      expect(
        w.lent.seen.filter(
          (message) =>
            message.kind === "call" && message.method === "resolveContact",
        ),
      ).toHaveLength(1);
    } finally {
      w.stop();
    }
  });

  test("a person already in a conversation is opened, and two starts for one person send one request", async () => {
    const w = await setup();
    try {
      const alice = bytesToHex(w.alice.identity);
      const start = w.provider.start!;
      const [first, second] = await Promise.all([
        start.start(alice, "one"),
        start.start(alice, "two"),
      ]);
      expect(second).toBe(first);
      expect(await start.start(alice, "three")).toBe(first);
      await waitFor(
        () => w.aliceEngine.snapshot().requests.length === 1,
        "the request",
      );
      expect(requestsTo(w.rpc, w.alice, w.bob)).toHaveLength(1);
      expect(w.aliceEngine.snapshot().requests[0].welcome).toBe("one");

      // Someone asking Bob is opened, not asked back.
      await w.carolEngine.startChat("bob.02", "hello bob");
      await waitFor(
        () => w.provider.conversations().length === 2,
        "Carol's request",
      );
      const carol = bytesToHex(w.carol.identity);
      const room = await start.start(carol, "");
      expect(w.host.room(NOVA_WORKER_PRODUCT, room)?.contact?.state).toBe(
        "Incoming",
      );
      expect(requestsTo(w.rpc, w.carol, w.bob)).toHaveLength(0);
    } finally {
      w.stop();
    }
  });

  test("starts made at once reach each their own person", async () => {
    const w = await setup();
    try {
      const alice = bytesToHex(w.alice.identity);
      const carol = bytesToHex(w.carol.identity);
      const [toAlice, toCarol] = await Promise.all([
        w.provider.start!.start(alice, "for alice"),
        w.provider.start!.start(carol, "for carol"),
      ]);
      expect(w.provider.start!.conversationWith!(alice)).toBe(toAlice);
      expect(w.provider.start!.conversationWith!(carol)).toBe(toCarol);
      await waitFor(
        () =>
          w.aliceEngine.snapshot().requests.length === 1 &&
          w.carolEngine.snapshot().requests.length === 1,
        "both requests",
      );
      expect(w.aliceEngine.snapshot().requests[0].welcome).toBe("for alice");
      expect(w.carolEngine.snapshot().requests[0].welcome).toBe("for carol");
    } finally {
      w.stop();
    }
  });

  test("a reference resolves once, in its grant, before it expires, and not for a blocked person", async () => {
    const w = await setup();
    try {
      const alice = bytesToHex(w.alice.identity);
      const reference = w.mode.startReference(alice);
      expect(
        bytesToHex(await w.lease.resolveContact(reference)).toLowerCase(),
      ).toBe(alice.toLowerCase());
      await expect(w.lease.resolveContact(reference)).rejects.toThrow(
        "names no one",
      );

      const stale = w.mode.startReference(alice);
      w.tick(START_REFERENCE_TTL_MS + 1);
      await expect(w.lease.resolveContact(stale)).rejects.toThrow(
        "names no one",
      );

      w.blocked.add(alice.toLowerCase());
      await expect(
        w.lease.resolveContact(w.mode.startReference(alice)),
      ).rejects.toThrow("You blocked this person.");
      // The worker reports it, and nothing is sent.
      await expect(w.provider.start!.start(alice, "no")).rejects.toThrow(
        "You blocked this person.",
      );
      expect(requestsTo(w.rpc, w.alice, w.bob)).toHaveLength(0);
      w.blocked.clear();

      // A reference from before the worker's grant ended names no one after.
      const before = w.mode.startReference(alice);
      w.mode.setMode("host");
      expect(() => w.mode.startReference(alice)).toThrow("not connected");
      await expect(w.lease.resolveContact(before)).rejects.toThrow();
      expect(w.mode.isChatProduct(NOVA_WORKER_PRODUCT)).toBe(false);
    } finally {
      w.stop();
    }
  });

  test("only the chat product the user switched to may say whom its rooms are with", async () => {
    const w = await setup();
    try {
      const channel = new MessageChannel();
      w.host.connect("other.paseo", messagePortBridge(channel.port2));
      const other = chatV2Over(messagePortBridge(channel.port1));
      await other.createRoom({
        roomId: "r",
        metadata: { name: "Looks like Alice" },
        config: {
          productSource: true,
          reactionDetails: false,
          participants: false,
          messageActions: false,
          reportSeen: false,
        },
      });
      const claimed = await other.setRoomContact({
        roomId: "r",
        contact: {
          account: bytesToHex(w.alice.identity),
          state: "Established",
        },
      });
      expect(claimed.isErr()).toBe(true);
      expect(w.host.room("other.paseo", "r")?.contact).toBeUndefined();
      expect(
        (await other.capabilities())._unsafeUnwrap().features,
      ).not.toContain("contact_rooms");
      expect(w.host.capabilities(NOVA_WORKER_PRODUCT).features).toContain(
        "contact_rooms",
      );
    } finally {
      w.stop();
    }
  });
});
