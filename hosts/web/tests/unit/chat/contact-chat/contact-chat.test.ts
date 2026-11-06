import { describe, expect, test } from "bun:test";
import { asProductId, type ContactHandle } from "@parity/truapi";
import { messagePortBridge } from "../../../../src/chat/draft/contract.js";
import { serveConversations } from "../../../../src/chat/draft/conversations.js";
import { ChatV2Host } from "../../../../src/chat/draft/host.js";
import { chatV2Over } from "../../../../src/chat/draft/product.js";
import {
  ChatV2Provider,
  UNKNOWN_CONTACT,
  type HandleContacts,
} from "../../../../src/chat/draft/provider.js";
import type { ChatMessage } from "../../../../src/chat/provider.js";
import type {
  CallResult,
  HostMessage,
} from "../../../../src/invitations/contract.js";
import {
  InvitationService,
  type AcceptPrompt,
  type ProductPort,
  type SendPrompt,
} from "../../../../src/invitations/invitation-service.js";
import {
  cryptoRandom,
  fromHex,
  fromUtf8,
  toHex,
} from "../../../../src/invitations/sealing.js";
import { NotificationStore } from "../../../../src/notifications/notifications.js";
import type { InvitationEvent } from "../../../../src/invitations/observer.js";
import { MemoryStorage } from "../../../support/test-support.js";
import { KeptStorage } from "../../../support/product-storage.js";
import type { ProductStorage } from "../../../../src/chat/worker/product-storage.js";
import { SUFFIX, hostContactsOf } from "../../../support/host-handles.js";
import { WalletIdentityAuth } from "../../../../src/invitations/wallet-auth.js";
import { FixtureContacts } from "../../../../fixtures/invitations/fixture-contacts.js";
import {
  LocalDelivery,
  inboxOf,
} from "../../../../fixtures/invitations/local-delivery.js";
import { MemoryWorld } from "../../../../fixtures/invitations/shared-store.js";
import {
  TEST_USERS,
  personOf as personOn,
  testUserAccount,
  testUserDirectory,
  testUserKeys,
  type TestUser,
} from "../../../../fixtures/invitations/test-users.js";
import {
  CONNECTED_SUBTITLE,
  ContactChat,
  START_OVER,
  START_OVER_PURPOSE,
} from "../../../../src/chat/contact-chat/engine.js";
import type { HostRequests } from "../../../../src/chat/contact-chat/requests.js";
import {
  WINDOW_MESSAGES,
  simpleText,
  type Session,
} from "../../../../src/chat/contact-chat/simple-text.js";
import { MemoryChatStatements } from "../../../support/chat-statements.js";

/**
 * Contact Chat between two test users, each with their own host: the real
 * request service with the fixture identity, contacts and local delivery,
 * the v2 draft bridge and the host's chat provider, and Contact Chat's
 * engine as its worker runs it, with `simple-text/1`. Messages go through a
 * Statement Store in memory that checks each statement's signature and
 * allowance. Everything a product hears from its host is kept, to show what
 * it never hears.
 */

const START = Date.UTC(2026, 9, 29, 12, 0, 0);
const NETWORK = "testnet";
const PRODUCT = { id: asProductId("contact-chat.test"), name: "Contact Chat" };
const [ALICE, BOB, CAROL] = TEST_USERS as [TestUser, TestUser, TestUser];

const settle = async () => {
  for (let round = 0; round < 30; round += 1)
    await new Promise((resolve) => setTimeout(resolve, 0));
};

class Clock {
  at = START;
  private tasks = new Map<number, { due: number; task: () => void }>();
  private next = 0;
  readonly now = (): number => this.at;
  readonly schedule = (task: () => void, delayMs: number): (() => void) => {
    const id = this.next++;
    this.tasks.set(id, { due: this.at + delayMs, task });
    return () => this.tasks.delete(id);
  };
  advance(ms: number): void {
    const end = this.at + ms;
    for (;;) {
      const due = [...this.tasks.entries()]
        .filter(([, entry]) => entry.due <= end)
        .sort(([, left], [, right]) => left.due - right.due)[0];
      if (due === undefined) break;
      this.tasks.delete(due[0]);
      this.at = due[1].due;
      due[1].task();
    }
    this.at = end;
  }
}

/** One user's host, and Contact Chat as its worker. */
class User {
  readonly hostStorage = new MemoryStorage();
  /** What the host keeps of Contact Chat's storage, which outlives its worker. */
  readonly kept = new KeptStorage();
  productStorage!: ProductStorage;
  readonly notifications: NotificationStore;
  readonly contacts: FixtureContacts;
  readonly prompts: SendPrompt[] = [];
  /** Everything the product heard from its host, as JSON. */
  readonly heard: string[] = [];
  confirm = true;
  /** The user's answer to the host's Accept prompt, or null when the host asks nothing. */
  acceptAnswer: boolean | null = null;
  readonly acceptPrompts: AcceptPrompt[] = [];
  /** Why this user's host gives no allowance now, or null when it does. */
  refuseAllowance: string | null = null;
  /** What the request service told the developer tools. */
  readonly observed: InvitationEvent[] = [];
  service!: InvitationService;
  requests!: HostRequests;
  engine!: ContactChat;
  chat!: ChatV2Host;
  provider!: ChatV2Provider;
  private stopServed = () => {};

  constructor(
    readonly user: TestUser,
    readonly world: MemoryWorld,
    readonly clock: Clock,
    readonly store: MemoryChatStatements,
  ) {
    this.notifications = new NotificationStore(this.hostStorage, clock.now);
    this.notifications.setScope(`${user.id}:notifications`);
    this.contacts = new FixtureContacts(user, hostContactsOf(user), {
      networkSuffix: SUFFIX,
      directory: testUserDirectory(SUFFIX),
    });
  }

  /** The host's own contacts, as Contact Chat's provider is lent them. */
  handleContacts(): HandleContacts {
    return {
      resolve: (handle) => {
        const person = this.contacts.resolve(handle);
        return person === null
          ? null
          : { account: person.account, name: person.name, seed: person.handle };
      },
      handleFor: (account) => {
        const person = this.contacts.byAccount(account);
        return person === undefined ? null : this.contacts.handleFor(person);
      },
    };
  }

  /** Start the host and the worker (again, over the same storage). */
  async start(): Promise<void> {
    await this.contacts.refresh();
    this.productStorage = await this.kept.open(["record"]);
    const auth = new WalletIdentityAuth(testUserKeys(this.user, SUFFIX));
    this.service = new InvitationService({
      product: PRODUCT,
      auth,
      contacts: this.contacts,
      notifications: this.notifications,
      storage: this.hostStorage,
      keys: { sent: "sent", received: "received" },
      delivery: new LocalDelivery({
        self: auth.identityFor().agreementPublic,
        network: NETWORK,
        store: this.world.store(),
        now: this.clock.now,
        schedule: this.clock.schedule,
        random: cryptoRandom,
        log: () => {},
        // Reposts are the delivery's own; hourly keeps a day-long expiry quick.
        repostIntervalMs: 60 * 60_000,
      }),
      now: this.clock.now,
      schedule: this.clock.schedule,
      random: cryptoRandom,
      confirmSend: async (prompt) => {
        this.prompts.push(prompt);
        return this.confirm;
      },
      showProduct: () => {},
      ...(this.acceptAnswer === null
        ? {}
        : {
            confirmAccept: async (prompt: AcceptPrompt) => {
              this.acceptPrompts.push(prompt);
              return this.acceptAnswer!;
            },
          }),
      accounts: this.store.allowance(
        () => this.clock.now() + 24 * 60 * 60_000,
        () => this.refuseAllowance,
      ),
      log: () => {},
      observe: (event) => void this.observed.push(event),
    });
    this.service.start();

    // The request service's port, as the worker's end of it.
    let handler: ((value: unknown) => void) | null = null;
    let listener: ((message: HostMessage) => void) | null = null;
    const early: HostMessage[] = [];
    const pending = new Map<number, (result: CallResult) => void>();
    let nextId = 1;
    const port: ProductPort = {
      post: (message) => {
        this.heard.push(JSON.stringify(message));
        if (message.kind === "result") {
          pending.get(message.id)?.(message.result);
          pending.delete(message.id);
        } else if (listener === null) early.push(message);
        else listener(message);
      },
      listen: (next) => (handler = next),
      close: () => {},
    };
    this.service.attachProduct(port);
    const requests: HostRequests = (this.requests = {
      call: (call) =>
        new Promise((resolve) => {
          const id = nextId++;
          pending.set(id, resolve);
          handler?.({ ...call, id });
        }),
      onMessage: (next) => {
        listener = next;
        for (const message of early.splice(0)) next(message);
      },
    });

    this.engine = new ContactChat({
      requests,
      protocol: simpleText,
      statements: this.store.statements(PRODUCT.id),
      storage: this.productStorage,
      recordKey: "record",
      random: cryptoRandom,
      now: this.clock.now,
    });

    this.chat = new ChatV2Host({
      media: () => null,
      contactRooms: (productId) => productId === PRODUCT.id,
      log: () => {},
    });
    const bridge = new MessageChannel();
    this.chat.connect(PRODUCT.id, messagePortBridge(bridge.port2));
    const productEnd = messagePortBridge(bridge.port1);
    const product = chatV2Over({
      post: (message) => productEnd.post(message),
      listen: (next) =>
        productEnd.listen((message) => {
          this.heard.push(
            JSON.stringify(message, (_key, value) =>
              typeof value === "bigint" ? String(value) : value,
            ),
          );
          next(message);
        }),
      close: () => productEnd.close(),
    });
    const marks = new Map<string, string>();
    const served = serveConversations(product, this.engine, {
      reads: {
        get: (id) => marks.get(id) ?? null,
        set: (id, messageId) => void marks.set(id, messageId),
      },
    });
    this.stopServed = () => served.stop();
    const contacts = this.handleContacts();
    this.provider = new ChatV2Provider(PRODUCT.id, this.chat, this.clock.now, {
      contacts,
      start: {
        target: (account) => ({ handle: contacts.handleFor(account)! }),
        blocked: () => null,
      },
    });
    this.engine.run();
    await served.ready;
    await settle();
  }

  stop(): void {
    this.engine.stop();
    this.stopServed();
    this.provider.dispose();
    this.service.stop();
  }

  /** Start a chat with `to` as the host's New chat does. */
  async startWith(to: TestUser, welcome: string): Promise<string> {
    const roomId = await this.provider.start!.start(
      testUserAccount(to, SUFFIX),
      welcome,
    );
    await settle();
    return roomId;
  }

  room(roomId: string) {
    return this.provider.conversations().find((room) => room.id === roomId)!;
  }

  only() {
    const [room] = this.provider.conversations();
    return room!;
  }

  async timeline(roomId: string): Promise<ChatMessage[]> {
    return (await this.provider.timeline(roomId)).messages;
  }

  /** The user chooses Accept on the newest request in the host's Inbox. */
  async acceptNewest(): Promise<void> {
    const item = this.notifications.list()[0]!;
    await this.service.requestAccept(item.id);
    await settle();
  }

  /** The entries the room's options menu shows. */
  menu(roomId: string): string[] {
    return (this.room(roomId).actions ?? []).map((action) => action.label);
  }

  /** The user chooses Start over in the room's options menu. */
  async startOver(roomId: string): Promise<void> {
    await this.provider.triggerRoomAction(roomId, START_OVER);
    await settle();
  }

  /** Every product-visible thing: what it heard and what it stored. */
  visible(): string {
    const kept = Array.from(
      { length: this.productStorage.length },
      (_, index) =>
        this.productStorage.getItem(this.productStorage.key(index)!),
    );
    return [...this.heard, ...kept].join("\n");
  }
}

function world(): {
  clock: Clock;
  store: MemoryChatStatements;
  alice: User;
  bob: User;
  carol: User;
} {
  const shared = new MemoryWorld();
  const clock = new Clock();
  const store = new MemoryChatStatements();
  store.now = () => Math.floor(clock.now() / 1000);
  return {
    clock,
    store,
    alice: new User(ALICE, shared, clock, store),
    bob: new User(BOB, shared, clock, store),
    carol: new User(CAROL, shared, clock, store),
  };
}

/** The texts written by people in a room, oldest first. */
const said = async (side: User, roomId: string) =>
  texts(await side.timeline(roomId)).filter(
    (message) => message.text !== null && message.from !== PRODUCT.id,
  );

const texts = (messages: ChatMessage[]) =>
  messages.map((message) => ({
    from: message.sender.label,
    text:
      message.content.tag === "Text" || message.content.tag === "System"
        ? message.content.text
        : null,
  }));

/** What names a person outside a product: never in what a product hears or keeps. */
/** `user` as another test user's host knows them, with the chat key their phrase derives. */
const personOf = (user: TestUser) =>
  personOn(user, SUFFIX, testUserKeys(user, SUFFIX).chatPublic);

function identifying(user: TestUser): string[] {
  const { identity } = personOf(user);
  return [
    user.name,
    user.handle,
    identity.signingPublic.slice(2),
    identity.agreementPublic.slice(2),
    inboxOf(identity.agreementPublic),
  ];
}

describe("Contact Chat between two hosts, by contact handle", () => {
  test("connects through the hosts, names each person in the host, and both sides write and read text", async () => {
    const { alice, bob, store } = world();
    await alice.start();
    await bob.start();

    const roomId = await alice.startWith(BOB, "Shall we talk?");
    // The host asked Alice before the request left, and named Bob there.
    expect(alice.prompts.map((prompt) => prompt.to.name)).toEqual(["Bob"]);
    expect(alice.room(roomId).title).toBe("Bob");
    expect(alice.menu(roomId)).toEqual([]);
    expect(alice.provider.composer(roomId)).toEqual({
      kind: "blocked",
      reason: "Waiting for them to accept your request",
    });

    // Bob's host authenticated it and keeps it for him; his product has heard nothing.
    expect(bob.notifications.list()).toHaveLength(1);
    expect(bob.notifications.list()[0]!.sender.name).toBe("Alice");
    expect(bob.provider.conversations()).toEqual([]);

    await bob.acceptNewest();
    await settle();
    const bobRoom = bob.only();
    expect(bobRoom.title).toBe("Alice");
    expect(bob.provider.composer(bobRoom.id)).toEqual({ kind: "text" });

    // Alice's host authenticated the reply; her product checked it and completed.
    expect(alice.provider.composer(roomId)).toEqual({ kind: "text" });
    expect(alice.service.outbox.list().map((item) => item.state.tag)).toEqual([
      "completed",
    ]);

    // Bob's product made both topics: each side reads the topic the other posts on.
    const [aliceSide] = alice.engine.stored();
    const [bobSide] = bob.engine.stored();
    expect([aliceSide!.status, bobSide!.status]).toEqual([
      "connected",
      "connected",
    ]);
    const aliceSession = aliceSide!.session as Session;
    const bobSession = bobSide!.session as Session;
    expect([aliceSession.listen, aliceSession.send]).toEqual([
      bobSession.send,
      bobSession.listen,
    ]);
    expect(aliceSession.listen).not.toBe(aliceSession.send);
    expect([
      alice.room(roomId).subtitle,
      bob.room(bobRoom.id).subtitle,
    ]).toEqual([CONNECTED_SUBTITLE, CONNECTED_SUBTITLE]);
    // A connected room offers Start over in its options menu, not in its feed.
    expect([alice.menu(roomId), bob.menu(bobRoom.id)]).toEqual([
      ["Start over"],
      ["Start over"],
    ]);

    await alice.provider.send(roomId, "Hello Bob");
    await settle();
    await bob.provider.send(bobRoom.id, "Hello Alice");
    await settle();

    expect(await said(bob, bobRoom.id)).toEqual([
      { from: "Alice", text: "Hello Bob" },
      { from: "You", text: "Hello Alice" },
    ]);
    expect(await said(alice, roomId)).toEqual([
      { from: "You", text: "Hello Bob" },
      { from: "Bob", text: "Hello Alice" },
    ]);
    expect(
      (await alice.timeline(roomId))
        .filter((message) => message.sender.label === "You")
        .map((message) => message.status),
    ).toEqual(["sent"]);
    // The room's own lines say what happened, and no more.
    expect(
      texts(await alice.timeline(roomId)).filter((m) => m.from === PRODUCT.id),
    ).toEqual([
      { from: PRODUCT.id, text: "You sent a chat request: Shall we talk?" },
      { from: PRODUCT.id, text: "They accepted your chat request" },
    ]);
    expect(
      texts(await bob.timeline(bobRoom.id)).filter(
        (m) => m.from === PRODUCT.id,
      ),
    ).toEqual([{ from: PRODUCT.id, text: "You accepted the chat request" }]);

    // Each side posts as Contact Chat's own account at the index it made
    // for this conversation, on its own topic and channel; its host allowed
    // that index through the request, for this product only.
    const aliceAccount = MemoryChatStatements.account(
      PRODUCT.id,
      aliceSide!.index!,
    );
    const bobAccount = MemoryChatStatements.account(
      PRODUCT.id,
      bobSide!.index!,
    );
    expect([bobSession.peer, aliceSession.peer]).toEqual([
      aliceAccount,
      bobAccount,
    ]);
    expect(
      store.taken.map((post) => [post.signer, post.topics, post.channel]),
    ).toEqual([
      [aliceAccount, [aliceSession.send], aliceSession.channel],
      [bobAccount, [bobSession.send], bobSession.channel],
    ]);
    expect(store.asked).toEqual([
      { product: PRODUCT.id, index: aliceSide!.index! },
      { product: PRODUCT.id, index: bobSide!.index! },
    ]);
    expect([
      alice.service.outbox.list()[0]!.chatIndex,
      bob.service.received.list()[0]!.chatIndex,
    ]).toEqual([aliceSide!.index, bobSide!.index]);
    // No conversation account is an identity or chat key of either person.
    for (const user of [ALICE, BOB])
      for (const value of identifying(user))
        expect([aliceAccount, bobAccount].join(" ")).not.toContain(
          value.toLowerCase(),
        );

    // A second message replaces Alice's post: the store holds one per side,
    // and it carries both of hers.
    await alice.provider.send(roomId, "Still there?");
    await settle();
    expect(store.held.map((post) => post.signer).sort()).toEqual(
      [aliceAccount, bobAccount].sort(),
    );
    expect(await said(bob, bobRoom.id)).toEqual([
      { from: "Alice", text: "Hello Bob" },
      { from: "You", text: "Hello Alice" },
      { from: "Alice", text: "Still there?" },
    ]);
  });

  test("each side reports its protocol, request, account, routes and counts, and the host observes the request's facts", async () => {
    const { alice, bob, store } = world();
    await alice.start();
    await bob.start();
    const roomId = await alice.startWith(BOB, "");
    await bob.acceptNewest();
    await settle();
    await alice.provider.send(roomId, "One");
    await alice.provider.send(roomId, "Two");
    await settle();
    await bob.provider.send(bob.only().id, "Hi");
    await settle();

    const [aliceSide] = alice.engine.stored();
    const [bobSide] = bob.engine.stored();
    const aliceSession = aliceSide!.session as Session;
    const [report] = alice.engine.diagnostics();
    expect(report).toEqual({
      id: aliceSide!.id,
      protocol: "contact-chat/simple-text/1",
      state: "connected",
      request: aliceSide!.request!,
      account: aliceSide!.account!,
      peers: [bobSide!.account!],
      listen: [aliceSession.listen],
      post: { topics: [aliceSession.send], channel: aliceSession.channel },
      logical: {
        sent: 2,
        received: 1,
        duplicates: report!.logical!.duplicates,
        failed: 0,
        posts: report!.logical!.posts,
      },
      // What one post carries is simple-text's own: a protocol detail, as text.
      facts: [
        {
          label: "Published message batch",
          value: `2 of ${WINDOW_MESSAGES} messages · ${(store.taken.filter((post) => post.signer === aliceSide!.account).at(-1)!.data.length - 2) / 2} of 16384 B`,
        },
        {
          label: "Requested expiry",
          value: "48 h per post; peers may evict it sooner",
        },
      ],
    });
    // Each post the store took is one the product counts.
    expect(report!.logical!.posts).toBe(
      store.taken.filter((post) => post.signer === aliceSide!.account).length,
    );
    // The reported routes are the ones the store saw.
    expect(store.taken[0]).toMatchObject({
      signer: aliceSide!.account,
      topics: [aliceSession.send],
      channel: aliceSession.channel,
    });
    // Nothing anyone wrote is in a report.
    expect(JSON.stringify(alice.engine.diagnostics())).not.toContain("One");

    expect(alice.observed.map((event) => event.type)).toEqual([
      "request-sent",
      "reply-authenticated",
      "request-closed",
      "chat-account",
    ]);
    expect(alice.observed.at(-1)).toMatchObject({
      type: "chat-account",
      handle: aliceSide!.request,
      index: aliceSide!.index,
      outcome: "allowed",
    });
    expect(bob.observed.map((event) => event.type)).toEqual([
      "request-received",
      "replied",
      "chat-account",
    ]);
  });

  test("what Bob writes before Alice's host has his reply is read once she connects", async () => {
    const { alice, bob, store } = world();
    await alice.start();
    await bob.start();
    const roomId = await alice.startWith(BOB, "");
    // Alice's host is off while Bob accepts and writes four times at once,
    // more than the two statements his account's allowance keeps.
    alice.stop();
    await bob.acceptNewest();
    await settle();
    const bobRoom = bob.only();
    const early = ["Are you there?", "I wrote first", "Third", "Fourth"];
    for (const text of early) await bob.provider.send(bobRoom.id, text);
    await settle();
    expect(alice.engine.stored()[0]!.status).toBe("waiting");
    // Nobody reads Bob's topic yet. Each post replaced the last: the store
    // took four and holds one, which carries all four messages.
    const bobTopic = (bob.engine.stored()[0]!.session as Session).send;
    expect(store.followersOf(bobTopic)).toBe(0);
    const onBobsTopic = (list: { topics: string[] }[]) =>
      list.filter((post) => post.topics.includes(bobTopic)).length;
    expect([onBobsTopic(store.taken), onBobsTopic(store.held)]).toEqual([4, 1]);

    // Alice's host starts, gets the reply, and her worker subscribes after
    // Bob's posts: it reads them from what the store still holds.
    await alice.start();
    await settle();
    expect(alice.engine.stored()[0]!.status).toBe("connected");
    expect(await said(alice, roomId)).toEqual(
      early.map((text) => ({ from: "Bob", text })),
    );
  });

  test("a restart reads the store again and keeps each message once, and a failed message posts on retry", async () => {
    const { clock, alice, bob, store } = world();
    await alice.start();
    await bob.start();
    const roomId = await alice.startWith(BOB, "");
    await bob.acceptNewest();
    await settle();
    const bobRoom = bob.only();
    await alice.provider.send(roomId, "One");
    await alice.provider.send(roomId, "Two");
    await settle();
    expect((await said(bob, bobRoom.id)).map((m) => m.text)).toEqual([
      "One",
      "Two",
    ]);

    bob.stop();
    await alice.provider.send(roomId, "Three, while Bob is away");
    await settle();
    await bob.start();
    await settle();
    expect((await said(bob, bobRoom.id)).map((m) => m.text)).toEqual([
      "One",
      "Two",
      "Three, while Bob is away",
    ]);
    // Three posts, each replacing the last: the store holds one of Alice's.
    expect([store.taken.length, store.held.length]).toEqual([3, 1]);

    // A send that failed is posted by a retry once the host allows it. A
    // new period makes the host ask its chain again.
    clock.advance(25 * 60 * 60_000);
    alice.refuseAllowance = "the chain is unreachable";
    const id = await alice.engine.send(roomId, "Four");
    await settle();
    expect(alice.engine.messages(roomId).find((m) => m.id === id)!.state).toBe(
      "failed",
    );
    alice.refuseAllowance = null;
    await alice.engine.retry(roomId, id);
    await settle();
    expect(alice.engine.messages(roomId).find((m) => m.id === id)!.state).toBe(
      "sent",
    );
    expect((await said(bob, bobRoom.id)).map((m) => m.text)).toEqual([
      "One",
      "Two",
      "Three, while Bob is away",
      "Four",
    ]);
  });

  test("a message the host gives no allowance for fails with why, and posts nothing", async () => {
    const { alice, bob, store } = world();
    await alice.start();
    await bob.start();
    const roomId = await alice.startWith(BOB, "");
    await bob.acceptNewest();
    await settle();
    alice.refuseAllowance = "No free Statement Store slot this period.";
    await alice.provider.send(roomId, "Hello");
    await settle();
    expect(
      (await alice.timeline(roomId))
        .filter((message) => message.sender.label === "You")
        .map((message) => message.status),
    ).toEqual(["failed"]);
    expect(texts(await alice.timeline(roomId)).at(-1)!.text).toContain(
      "No free Statement Store slot this period.",
    );
    expect(store.taken).toEqual([]);
  });

  test("a message the Statement Store refuses fails, and the room says why", async () => {
    const { alice, bob, store } = world();
    await alice.start();
    await bob.start();
    const roomId = await alice.startWith(BOB, "");
    await bob.acceptNewest();
    await settle();
    await alice.provider.send(roomId, "One");
    await settle();
    // The host allowed the account; the store no longer takes its posts.
    store.allowed.clear();
    await alice.provider.send(roomId, "Two");
    await settle();
    const timeline = await alice.timeline(roomId);
    expect(
      timeline
        .filter((message) => message.sender.label === "You")
        .map((message) => message.status),
    ).toEqual(["sent", "failed"]);
    expect(texts(timeline).at(-1)!.text).toBe(
      "Your message was not sent: statement-store submit failed: no allowance",
    );
    expect(store.taken).toHaveLength(1);
  });

  test("the network keeps a bounded window: messages older than it are not read by a peer that was away", async () => {
    const { alice, bob } = world();
    await alice.start();
    await bob.start();
    const roomId = await alice.startWith(BOB, "");
    await bob.acceptNewest();
    await settle();
    const bobRoom = bob.only();
    bob.stop();
    const count = WINDOW_MESSAGES + 5;
    for (let index = 1; index <= count; index += 1)
      await alice.engine.send(roomId, `Message ${index}`);
    await settle();
    await bob.start();
    await settle();
    // Alice keeps all of hers; Bob reads only the window the store kept.
    expect((await said(alice, roomId)).length).toBe(count);
    expect((await said(bob, bobRoom.id)).map((m) => m.text)).toEqual(
      Array.from(
        { length: WINDOW_MESSAGES },
        (_, index) => `Message ${index + 6}`,
      ),
    );
  });

  test("the host allows one index per request, only after Send and a reply or Accept, and asks the chain again only in a new period", async () => {
    const { clock, alice, bob, store } = world();
    await alice.start();
    await bob.start();
    await alice.startWith(BOB, "");
    const sent = alice.engine.stored()[0]!.request!;
    const index = (byte: string) => `0x${byte.repeat(32)}`;
    expect(
      await alice.requests.call({
        call: "chatAccount",
        handle: sent,
        index: index("11"),
      }),
    ).toEqual({
      ok: false,
      reason: "Refused",
      detail: "No authenticated reply has arrived.",
    });
    expect(
      await alice.requests.call({
        call: "chatAccount",
        handle: `0x${"00".repeat(16)}`,
        index: index("11"),
      }),
    ).toMatchObject({ ok: false, reason: "Unknown" });
    await bob.acceptNewest();
    await settle();
    const roomId = alice.only().id;
    await alice.provider.send(roomId, "Hi");
    await settle();
    expect(
      await alice.requests.call({
        call: "chatAccount",
        handle: sent,
        index: index("22"),
      }),
    ).toEqual({
      ok: false,
      reason: "Refused",
      detail: "This conversation already has another account.",
    });
    // Within the period the host answers from what it got; after it, it asks again for the same index.
    await alice.provider.send(roomId, "Again");
    await settle();
    const bound = alice.engine.stored()[0]!.index!;
    expect(
      await alice.requests.call({
        call: "chatAccount",
        handle: sent,
        index: bound,
      }),
    ).toMatchObject({ ok: true });
    expect(store.asked).toHaveLength(1);
    clock.advance(25 * 60 * 60_000);
    await alice.provider.send(roomId, "Next day");
    await settle();
    const own = alice.engine.stored()[0]!.index;
    expect(store.asked.filter((asked) => asked.index === own)).toHaveLength(2);
  });

  test("a host that asks before Accept hands the product nothing until the user accepts, and nothing at all when they cancel", async () => {
    const { alice, bob } = world();
    bob.acceptAnswer = false;
    await alice.start();
    await bob.start();
    await alice.startWith(BOB, "Shall we talk?");
    const item = bob.notifications.list()[0]!;
    await expect(bob.service.requestAccept(item.id)).rejects.toThrow(
      "You did not accept it.",
    );
    await settle();
    expect(
      bob.acceptPrompts.map((prompt) => [prompt.from.name, prompt.purpose]),
    ).toEqual([["Alice", "Shall we talk?"]]);
    expect([
      bob.heard.some((line) => line.includes("acceptIntent")),
      bob.engine.stored(),
      bob.notifications.get(item.id)!.state.tag,
    ]).toEqual([false, [], "pending"]);

    bob.acceptAnswer = true;
    await bob.service.requestAccept(item.id);
    await settle();
    expect(bob.engine.stored()[0]!.status).toBe("connected");
  });

  test("a product holds only the handles its host gave it, and hears no name, account, identity or route", async () => {
    const { alice, bob } = world();
    await alice.start();
    await bob.start();
    await alice.startWith(BOB, "Shall we talk?");
    await bob.acceptNewest();
    await settle();

    // The handle each product holds is the one its own host knows the other by.
    const [aliceSide] = alice.engine.stored();
    const [bobSide] = bob.engine.stored();
    expect(aliceSide!.contact).toBe(
      alice.contacts.handleFor(personOf(BOB))!.bytes,
    );
    expect(bobSide!.contact).toBe(
      bob.contacts.handleFor(personOf(ALICE))!.bytes,
    );
    // The two hosts' handles for one conversation differ, and neither is an account.
    expect(aliceSide!.contact).not.toBe(bobSide!.contact);

    const sent = alice.service.outbox.list()[0]!;
    const received = bob.service.received.list()[0]!;
    const hostOnly = [
      ...identifying(ALICE),
      ...identifying(BOB),
      sent.context.requestId.slice(2),
      sent.context.returnRoute,
      sent.context.replyKey.slice(2),
      received.context.requestId.slice(2),
    ];
    // Setup completed, so Alice's host dropped the reply key's secret.
    expect(sent.replySecret).toBeNull();
    for (const side of [alice, bob]) {
      const visible = side.visible().toLowerCase();
      for (const value of hostOnly)
        expect({ value, found: visible.includes(value.toLowerCase()) }).toEqual(
          { value, found: false },
        );
    }
  });

  test("the host names a contact only from its own contacts, never from what the product declared", async () => {
    const { alice, bob, carol } = world();
    await alice.start();
    await bob.start();
    await carol.start();
    await alice.startWith(BOB, "");

    // A handle Carol's host gave for Bob names no one in Alice's host.
    const carolsHandle = carol.contacts.handleFor(personOf(BOB))!;
    const forged: ContactHandle = {
      bytes: toHex(cryptoRandom(32)) as ContactHandle["bytes"],
    };
    for (const handle of [carolsHandle, forged])
      expect(alice.handleContacts().resolve(handle)).toBeNull();

    // Rooms the product declares with such handles and its own title show as a contact, unnamed.
    const relay = new MessageChannel();
    alice.chat.connect("other.test", messagePortBridge(relay.port2));
    const other = chatV2Over(messagePortBridge(relay.port1));
    const host = new ChatV2Host({
      media: () => null,
      contactRooms: () => true,
      log: () => {},
    });
    const channel = new MessageChannel();
    host.connect(PRODUCT.id, messagePortBridge(channel.port2));
    const declaring = chatV2Over(messagePortBridge(channel.port1));
    for (const [roomId, handle] of [
      ["carols", carolsHandle],
      ["forged", forged],
      ["bobs", alice.contacts.handleFor(personOf(BOB))!],
    ] as const) {
      await declaring.createRoom({
        roomId,
        metadata: { name: "Mallory the trusted" },
        config: {
          productSource: true,
          reactionDetails: false,
          participants: true,
          messageActions: false,
          reportSeen: false,
        },
      });
      await declaring.setRoomContact({
        roomId,
        contact: { handle, state: "Established" },
      });
    }
    const provider = new ChatV2Provider(PRODUCT.id, host, Date.now, {
      contacts: alice.handleContacts(),
    });
    await settle();
    expect(
      provider
        .conversations()
        .map((room) => [room.id, room.title])
        .sort(),
    ).toEqual([
      ["bobs", "Bob"],
      ["carols", UNKNOWN_CONTACT],
      ["forged", UNKNOWN_CONTACT],
    ]);
    // Only a product the host lets declare contacts can.
    await other.createRoom({
      roomId: "elsewhere",
      metadata: { name: "Elsewhere" },
      config: {
        productSource: true,
        reactionDetails: false,
        participants: true,
        messageActions: false,
        reportSeen: false,
      },
    });
    expect(
      (
        await other.setRoomContact({
          roomId: "elsewhere",
          contact: { handle: forged, state: "Established" },
        })
      ).isErr(),
    ).toBe(true);
    provider.dispose();
  });

  test("decline is silent: nothing goes back, the request waits until the user cancels it in the host, and a new one can be sent", async () => {
    const { clock, alice, bob } = world();
    await alice.start();
    await bob.start();
    const roomId = await alice.startWith(BOB, "");
    const item = bob.notifications.list()[0]!;
    expect(bob.notifications.decline(item.id).ok).toBe(true);
    await settle();
    expect(bob.service.received.list()[0]!.replied).toBeNull();
    expect(bob.provider.conversations()).toEqual([]);
    expect(alice.provider.composer(roomId).kind).toBe("blocked");

    clock.advance(3 * 24 * 60 * 60_000);
    await settle();
    expect(alice.engine.stored()[0]!.status).toBe("waiting");
    const sent = alice.engine.stored()[0]!.request!;
    expect(alice.service.requestCancel(sent)).toEqual({
      ok: true,
      request: expect.objectContaining({ status: "cancelled" }),
    });
    await settle();
    expect(alice.engine.stored()[0]!.status).toBe("cancelled");
    expect(alice.provider.composer(roomId)).toEqual({
      kind: "blocked",
      reason: "Start a new chat with them to send a request",
    });
    // The host offers a new request for the person, in the same room.
    expect(
      alice.provider.start!.conversationWith!(testUserAccount(BOB, SUFFIX)),
    ).toBeNull();
    expect(await alice.startWith(BOB, "")).toBe(roomId);
    expect(alice.prompts).toHaveLength(2);
    expect(alice.engine.stored()[0]!.status).toBe("waiting");
  });

  test("the product cancels its own request through its host; the other person is not told, and a reply after it is ignored", async () => {
    const { alice, bob } = world();
    await alice.start();
    await bob.start();
    const roomId = await alice.startWith(BOB, "");
    const sent = alice.engine.stored()[0]!.request!;
    await alice.engine.cancel(roomId);
    await settle();
    expect([
      alice.engine.stored()[0]!.status,
      alice.service.outbox.get(sent)?.state.tag,
      alice.provider.composer(roomId),
    ]).toEqual([
      "cancelled",
      "cancelled",
      {
        kind: "blocked",
        reason: "Start a new chat with them to send a request",
      },
    ]);
    // Bob's request still waits for him; accepting it reaches no one.
    expect(bob.notifications.list()[0]!.state.tag).toBe("pending");
    await bob.acceptNewest();
    await settle();
    expect(alice.engine.stored()[0]!.status).toBe("cancelled");
    expect(alice.engine.stored()[0]!.session).toBeUndefined();
    await expect(alice.engine.cancel(roomId)).rejects.toThrow(
      "There is no request to cancel.",
    );
  });

  test("a request the user does not confirm is not sent, and says so", async () => {
    const { alice, bob } = world();
    await alice.start();
    await bob.start();
    alice.confirm = false;
    const roomId = await alice.startWith(BOB, "");
    expect(alice.engine.stored()[0]!.status).toBe("not-sent");
    expect(bob.notifications.list()).toEqual([]);
    expect(
      texts(await alice.timeline(roomId)).map((message) => message.text),
    ).toEqual(["Your chat request was not sent"]);
  });

  test("a restart keeps the conversation, its names, its session and its account", async () => {
    const { alice, bob } = world();
    await alice.start();
    await bob.start();
    await alice.startWith(BOB, "");
    await bob.acceptNewest();
    await settle();
    const before = bob.engine.stored()[0]!;
    bob.stop();
    await bob.start();
    const room = bob.only();
    expect(room.title).toBe("Alice");
    const after = bob.engine.stored()[0]!;
    expect([after.status, after.session, after.account]).toEqual([
      "connected",
      before.session,
      before.account,
    ]);
  });
});

describe("Start over in a connected Contact Chat conversation", () => {
  /** Alice and Bob connected, with one message each way. */
  async function connected() {
    const parts = world();
    const { alice, bob } = parts;
    await alice.start();
    await bob.start();
    const roomId = await alice.startWith(BOB, "");
    await bob.acceptNewest();
    await settle();
    const bobRoomId = bob.only().id;
    await alice.provider.send(roomId, "Hello Bob");
    await settle();
    await bob.provider.send(bobRoomId, "Hello Alice");
    await settle();
    return { ...parts, roomId, bobRoomId };
  }

  /** The texts one post of `signer` carries. */
  const carried = (store: MemoryChatStatements, signer: string) =>
    (
      JSON.parse(
        fromUtf8(
          fromHex(
            store.taken.filter((post) => post.signer === signer).at(-1)!.data,
          ),
        ),
      ) as { messages: { text: string }[] }
    ).messages.map((message) => message.text);

  test("sends a new request on a new account and stops reading the old one; their Accept connects both on new accounts and topics, and each reads what the other wrote while away", async () => {
    const { alice, bob, carol, clock, store, roomId, bobRoomId } =
      await connected();
    // Another conversation of Alice's, which Start over must not touch.
    await carol.start();
    await alice.startWith(CAROL, "");
    await carol.acceptNewest();
    await settle();
    const withCarol = structuredClone(alice.engine.stored()[1]!);
    expect(withCarol.status).toBe("connected");
    const before = structuredClone(alice.engine.stored()[0]!);
    const bobBefore = structuredClone(bob.engine.stored()[0]!);
    const oldListen = (before.session as Session).listen;
    expect(store.followersOf(oldListen)).toBe(1);

    clock.advance(60_000);
    await alice.startOver(roomId);

    // The host asked Alice before the new request left, with what Bob sees.
    expect(
      alice.prompts.map((prompt) => [prompt.to.name, prompt.purpose]),
    ).toEqual([
      ["Bob", "Wants to chat with you in Contact Chat."],
      ["Carol", "Wants to chat with you in Contact Chat."],
      ["Bob", START_OVER_PURPOSE],
    ]);
    const after = alice.engine.stored()[0]!;
    expect({
      status: after.status,
      session: after.session,
      newIndex: after.index !== before.index,
      newRequest: after.request !== before.request,
    }).toEqual({
      status: "waiting",
      session: undefined,
      newIndex: true,
      newRequest: true,
    });
    expect(store.followersOf(oldListen)).toBe(0);
    // The developer tools no longer report the earlier connection's last post.
    expect(alice.engine.diagnostics()[0]!.facts).toBeUndefined();
    expect(alice.provider.composer(roomId)).toEqual({
      kind: "blocked",
      reason: "Waiting for them to accept your request",
    });
    // Start over leaves the menu until they accept; a second choice is refused.
    expect(alice.menu(roomId)).toEqual([]);
    await expect(
      alice.provider.triggerRoomAction(roomId, START_OVER),
    ).rejects.toThrow("This option is not offered any more.");
    // The history stays and the break is marked.
    expect(texts(await alice.timeline(roomId))).toEqual([
      { from: PRODUCT.id, text: "You sent a chat request" },
      { from: PRODUCT.id, text: "They accepted your chat request" },
      { from: "You", text: "Hello Bob" },
      { from: "Bob", text: "Hello Alice" },
      {
        from: PRODUCT.id,
        text: "You started over. Your earlier connection ended here; they must accept your new chat request.",
      },
    ]);

    // Bob's host holds a second request with its own id; his chat is not told anything.
    expect(bob.notifications.list().map((item) => item.purpose)).toEqual([
      START_OVER_PURPOSE,
      "Wants to chat with you in Contact Chat.",
    ]);
    expect(
      new Set(bob.service.received.list().map((item) => item.context.requestId))
        .size,
    ).toBe(2);
    expect(bob.engine.stored()[0]!.session).toEqual(bobBefore.session);
    expect(alice.engine.stored()[1]).toEqual(withCarol);

    await bob.acceptNewest();
    await settle();
    const aliceSide = alice.engine.stored()[0]!;
    const bobSide = bob.engine.stored()[0]!;
    const aliceSession = aliceSide.session as Session;
    const bobSession = bobSide.session as Session;
    expect([aliceSide.status, bobSide.status]).toEqual([
      "connected",
      "connected",
    ]);
    expect([aliceSession.listen, aliceSession.send]).toEqual([
      bobSession.send,
      bobSession.listen,
    ]);
    expect([aliceSession.peer, bobSession.peer]).toEqual([
      MemoryChatStatements.account(PRODUCT.id, bobSide.index!),
      MemoryChatStatements.account(PRODUCT.id, aliceSide.index!),
    ]);
    expect([
      bobSide.index === bobBefore.index,
      aliceSession.listen === oldListen,
      bobSession.listen === (bobBefore.session as Session).listen,
    ]).toEqual([false, false, false]);

    // Each side reads what the other wrote while it was away.
    bob.stop();
    await alice.provider.send(roomId, "Again, Bob");
    await settle();
    await bob.start();
    alice.stop();
    await bob.provider.send(bobRoomId, "Again, Alice");
    await settle();
    await alice.start();
    expect((await said(bob, bobRoomId)).map((m) => m.text)).toEqual([
      "Hello Bob",
      "Hello Alice",
      "Again, Bob",
      "Again, Alice",
    ]);
    expect((await said(alice, roomId)).map((m) => m.text)).toEqual([
      "Hello Bob",
      "Hello Alice",
      "Again, Bob",
      "Again, Alice",
    ]);
    // Both rooms offer Start over again, in the menu; neither feed holds a button.
    expect([alice.menu(roomId), bob.menu(bobRoomId)]).toEqual([
      ["Start over"],
      ["Start over"],
    ]);
    for (const [side, id] of [
      [alice, roomId],
      [bob, bobRoomId],
    ] as const)
      expect(
        (await side.timeline(id)).filter(
          (message) => message.content.tag === "Actions",
        ),
      ).toEqual([]);
    // The new connection posts only what was written in it.
    expect([
      carried(store, aliceSide.account!),
      carried(store, bobSide.account!),
    ]).toEqual([["Again, Bob"], ["Again, Alice"]]);
  });

  test("a Start over the user does not send changes nothing, and the old connection still carries messages", async () => {
    const { alice, bob, roomId, bobRoomId } = await connected();
    const before = structuredClone(alice.engine.stored()[0]!);
    alice.confirm = false;
    await alice.startOver(roomId);
    expect(alice.prompts.map((prompt) => prompt.purpose).at(-1)).toBe(
      START_OVER_PURPOSE,
    );
    expect(alice.engine.stored()[0]).toEqual(before);
    expect(bob.notifications.list()).toHaveLength(1);
    expect(alice.menu(roomId)).toEqual(["Start over"]);

    await alice.provider.send(roomId, "Still here");
    await settle();
    expect((await said(bob, bobRoomId)).map((m) => m.text)).toEqual([
      "Hello Bob",
      "Hello Alice",
      "Still here",
    ]);
  });

  test("after Start over, what the peer posts on the earlier connection is not read", async () => {
    const { alice, bob, roomId, bobRoomId } = await connected();
    await alice.startOver(roomId);
    // Bob has not accepted the new request: he still writes on the old connection.
    await bob.provider.send(bobRoomId, "Anyone there?");
    await settle();
    expect((await said(alice, roomId)).map((m) => m.text)).toEqual([
      "Hello Bob",
      "Hello Alice",
    ]);
    // The first request is completed: nothing Bob's host posts for it reaches Alice's chat.
    expect(alice.service.outbox.list().map((item) => item.state.tag)).toEqual([
      "completed",
      "waiting",
    ]);
    expect(alice.engine.stored()[0]!.status).toBe("waiting");
  });
});
