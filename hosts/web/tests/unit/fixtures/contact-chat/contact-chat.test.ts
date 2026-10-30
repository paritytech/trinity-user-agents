import { describe, expect, test } from "bun:test";
import type { ContactHandle } from "@parity/truapi";
import { messagePortBridge } from "../../../../src/chat/draft/contract.js";
import { serveConversations } from "../../../../src/chat/draft/conversations.js";
import { ChatV2Host } from "../../../../src/chat/draft/host.js";
import { chatV2Over } from "../../../../src/chat/draft/product.js";
import {
  ChatV2Provider,
  UNKNOWN_CONTACT,
  type HandleContacts,
} from "../../../../src/chat/draft/provider.js";
import { topicRelayOver } from "../../../../src/chat/draft/topic-relay.js";
import type { ChatMessage } from "../../../../src/chat/provider.js";
import type {
  CallResult,
  HostMessage,
} from "../../../../src/invitations/contract.js";
import {
  InvitationService,
  type ProductPort,
  type SendPrompt,
} from "../../../../src/invitations/invitation-service.js";
import {
  cryptoRandom,
  fromHex,
  toHex,
  utf8,
} from "../../../../src/invitations/sealing.js";
import { NotificationStore } from "../../../../src/notifications/notifications.js";
import { MemoryStorage } from "../../../support/test-support.js";
import { hostContactsOf } from "../../../support/host-handles.js";
import { FixtureContacts } from "../../../../fixtures/invitations/fixture-contacts.js";
import {
  FixtureIdentityAuth,
  publicIdentity,
} from "../../../../fixtures/invitations/fixture-identity.js";
import {
  LocalDelivery,
  inboxOf,
} from "../../../../fixtures/invitations/local-delivery.js";
import { LocalTopics } from "../../../../fixtures/invitations/local-topics.js";
import { MemoryWorld } from "../../../../fixtures/invitations/shared-store.js";
import {
  TEST_USERS,
  fixtureIdentity,
  personOf,
  type TestUser,
} from "../../../../fixtures/invitations/test-users.js";
import {
  ContactChat,
  INVITATION_TTL_MS,
} from "../../../../fixtures/products/contact-chat/engine.js";
import {
  openFrame,
  sealFrame,
  sessionKeys,
} from "../../../../fixtures/products/contact-chat/messages.js";
import type { HostRequests } from "../../../../fixtures/products/contact-chat/requests.js";

/**
 * Contact Chat between two test users, each with their own host: the real
 * request service with the fixture identity, contacts and local delivery,
 * the local topics, the v2 draft bridge and the host's chat provider, and
 * Contact Chat's engine as its worker runs it. Everything a product hears
 * from its host is kept, to show what it never hears.
 */

const START = Date.UTC(2026, 9, 29, 12, 0, 0);
const NETWORK = "testnet";
const PRODUCT = { id: "contact-chat.test", name: "Contact Chat" };
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
  readonly productStorage = new MemoryStorage();
  readonly notifications: NotificationStore;
  readonly contacts: FixtureContacts;
  readonly prompts: SendPrompt[] = [];
  /** Everything the product heard from its host, as JSON. */
  readonly heard: string[] = [];
  confirm = true;
  service!: InvitationService;
  engine!: ContactChat;
  chat!: ChatV2Host;
  provider!: ChatV2Provider;
  private stopServed = () => {};

  constructor(
    readonly user: TestUser,
    readonly world: MemoryWorld,
    readonly clock: Clock,
  ) {
    this.notifications = new NotificationStore(this.hostStorage, clock.now);
    this.notifications.setScope(`${user.id}:notifications`);
    this.contacts = new FixtureContacts(user, hostContactsOf(user));
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
    const credential = fixtureIdentity(this.user);
    this.service = new InvitationService({
      product: PRODUCT,
      auth: new FixtureIdentityAuth(credential, cryptoRandom),
      contacts: this.contacts,
      notifications: this.notifications,
      storage: this.hostStorage,
      keys: { sent: "sent", received: "received" },
      delivery: new LocalDelivery({
        self: publicIdentity(credential).agreementPublic,
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
      log: () => {},
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
    const requests: HostRequests = {
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
    };

    const relay = new MessageChannel();
    new LocalTopics({
      network: NETWORK,
      store: this.world.store(),
      now: this.clock.now,
    }).serve(relay.port2);
    const topics = topicRelayOver(relay.port1);
    this.engine = new ContactChat({
      requests,
      topics: {
        post: (topic, data) => topics.post(topic, data),
        watch: (topic, onData) =>
          topics.watch(topic, (frames) => {
            this.heard.push(JSON.stringify({ topic, frames }));
            onData(frames);
          }),
      },
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
    await this.engine.run();
    await served.ready;
    await settle();
  }

  stop(): void {
    this.stopServed();
    this.provider.dispose();
    this.service.stop();
  }

  /** Start a chat with `to` as the host's New chat does. */
  async startWith(to: TestUser, welcome: string): Promise<string> {
    const roomId = await this.provider.start!.start(to.account, welcome);
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

function world(): { clock: Clock; alice: User; bob: User; carol: User } {
  const shared = new MemoryWorld();
  const clock = new Clock();
  return {
    clock,
    alice: new User(ALICE, shared, clock),
    bob: new User(BOB, shared, clock),
    carol: new User(CAROL, shared, clock),
  };
}

const texts = (messages: ChatMessage[]) =>
  messages.map((message) => ({
    from: message.sender.label,
    text:
      message.content.tag === "Text" || message.content.tag === "System"
        ? message.content.text
        : null,
  }));

/** What names a person outside a product: never in what a product hears or keeps. */
function identifying(user: TestUser): string[] {
  const identity = publicIdentity(fixtureIdentity(user));
  return [
    user.name,
    user.handle,
    user.account,
    user.account.slice(2),
    identity.signingPublic.slice(2),
    identity.agreementPublic.slice(2),
    inboxOf(identity.agreementPublic),
  ];
}

describe("Contact Chat between two hosts, by contact handle", () => {
  test("connects through the hosts, names each person in the host, and carries text both ways", async () => {
    const { alice, bob } = world();
    await alice.start();
    await bob.start();

    const roomId = await alice.startWith(BOB, "Shall we talk?");
    // The host asked Alice before the request left, and named Bob there.
    expect(alice.prompts.map((prompt) => prompt.to.name)).toEqual(["Bob"]);
    expect(alice.room(roomId).title).toBe("Bob");
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

    await alice.provider.send(roomId, "Hello there");
    await settle();
    await bob.provider.send(bobRoom.id, "Hi, good to see you");
    await settle();
    expect(
      texts(await bob.timeline(bobRoom.id)).filter((m) => m.text !== null),
    ).toEqual([
      { from: PRODUCT.id, text: "You accepted the chat request" },
      { from: "Alice", text: "Hello there" },
      { from: "You", text: "Hi, good to see you" },
    ]);
    expect(
      texts(await alice.timeline(roomId)).filter((m) => m.text !== null),
    ).toEqual([
      { from: PRODUCT.id, text: "You sent a chat request: Shall we talk?" },
      { from: PRODUCT.id, text: "They accepted your chat request" },
      { from: "You", text: "Hello there" },
      { from: "Bob", text: "Hi, good to see you" },
    ]);
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
      sent.replySecret.slice(2),
      received.context.requestId.slice(2),
    ];
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

  test("decline is silent: nothing goes back, the request expires, and a new one can be sent", async () => {
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

    clock.advance(INVITATION_TTL_MS + 1);
    await settle();
    expect(alice.engine.stored()[0]!.status).toBe("expired");
    expect(alice.provider.composer(roomId)).toEqual({
      kind: "blocked",
      reason: "Start a new chat with them to send a request",
    });
    // The host offers a new request for the person, in the same room.
    expect(alice.provider.start!.conversationWith!(BOB.account)).toBeNull();
    expect(await alice.startWith(BOB, "")).toBe(roomId);
    expect(alice.prompts).toHaveLength(2);
    expect(alice.engine.stored()[0]!.status).toBe("waiting");
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

  test("a frame posted again, changed, or meant for the other direction is dropped", async () => {
    const { alice, bob } = world();
    await alice.start();
    await bob.start();
    const roomId = await alice.startWith(BOB, "");
    await bob.acceptNewest();
    await settle();
    await alice.provider.send(roomId, "Once only");
    await settle();
    const session = alice.engine.stored()[0]!.session!;
    const frame = alice.engine.stored()[0]!.messages.at(-1)!.frame!;
    const relay = new MessageChannel();
    new LocalTopics({
      network: NETWORK,
      store: alice.world.store(),
      now: alice.clock.now,
    }).serve(relay.port2);
    const outside = topicRelayOver(relay.port1);
    const changed = `${frame.slice(0, -2)}${frame.endsWith("00") ? "01" : "00"}`;
    const backwards = sealFrame(
      session.send,
      session.in,
      99,
      utf8(JSON.stringify({ id: "x", at: 1, text: "Backwards" })),
    );
    for (const data of [frame, changed]) await outside.post(session.out, data);
    await outside.post(session.in, backwards);
    await settle();
    const bobRoom = bob.only();
    expect(
      texts(await bob.timeline(bobRoom.id)).filter(
        (message) => message.from !== PRODUCT.id,
      ),
    ).toEqual([{ from: "Alice", text: "Once only" }]);
  });

  test("a restart keeps the conversation, its names and its keys, and reads what came meanwhile", async () => {
    const { alice, bob } = world();
    await alice.start();
    await bob.start();
    const roomId = await alice.startWith(BOB, "");
    await bob.acceptNewest();
    await settle();
    bob.stop();
    await alice.provider.send(roomId, "While you were away");
    await settle();
    await bob.start();
    const room = bob.only();
    expect(room.title).toBe("Alice");
    expect(
      texts(await bob.timeline(room.id)).filter(
        (message) => message.from === "Alice",
      ),
    ).toEqual([{ from: "Alice", text: "While you were away" }]);
    await bob.provider.send(room.id, "Back now");
    await settle();
    expect(
      texts(await alice.timeline(roomId)).filter(
        (message) => message.from === "Bob",
      ),
    ).toEqual([{ from: "Bob", text: "Back now" }]);
  });
});

describe("Contact Chat's fixture text layer", () => {
  const topics = {
    tAB: toHex(cryptoRandom(32)),
    tBA: toHex(cryptoRandom(32)),
  };
  const shared = cryptoRandom(32);
  const sender = sessionKeys("sender", shared, "0x01", "0x02", topics);
  const recipient = sessionKeys("recipient", shared, "0x01", "0x02", topics);

  test("each side writes where the other reads, under the key the other opens with", () => {
    expect(sender.out).toBe(recipient.in);
    expect(sender.send).toBe(recipient.receive);
    expect(recipient.out).toBe(sender.in);
    expect(recipient.send).toBe(sender.receive);
    expect(sender.send).not.toBe(sender.receive);
  });

  test("a frame opens only with its key, on its topic", () => {
    const frame = sealFrame(sender.send, sender.out, 7, utf8("text"));
    expect(openFrame(recipient.receive, recipient.in, frame)).toEqual({
      counter: 7,
      plaintext: utf8("text"),
    });
    expect(openFrame(recipient.send, recipient.in, frame)).toBeNull();
    expect(openFrame(recipient.receive, recipient.out, frame)).toBeNull();
    expect(openFrame(recipient.receive, recipient.in, "0x01")).toBeNull();
    expect(() => sealFrame(sender.send, sender.out, 0, utf8("text"))).toThrow();
    // Another setup's keys open nothing.
    const other = sessionKeys(
      "recipient",
      cryptoRandom(32),
      "0x01",
      "0x02",
      topics,
    );
    expect(openFrame(other.receive, other.in, frame)).toBeNull();
    expect(fromHex(frame)[0]).toBe(1);
  });
});
