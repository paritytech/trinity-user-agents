import { describe, expect, test } from "bun:test";
import {
  issueProof,
  sealEnvelope,
  type Envelope,
} from "../invitations/binding.js";
import type {
  CallResult,
  HostMessage,
  IncomingInvitation,
} from "../invitations/contract.js";
import {
  InvitationService,
  receivedId,
  type ProductPort,
} from "../invitations/invitation-service.js";
import {
  cryptoRandom,
  fromHex,
  openSealed,
  sealTo,
  toHex,
  utf8,
} from "../invitations/sealing.js";
import { NotificationStore } from "../notifications.js";
import { MemoryStorage } from "../test-support.js";
import * as setup from "./example-chat/setup.js";
import { FIXTURE_REF_PREFIX, FixtureContacts } from "./fixture-contacts.js";
import {
  LocalDelivery,
  POST_LIFETIME_MS,
  REPOST_INTERVAL_MS,
  mailboxPrefix,
  routeOf,
} from "./local-delivery.js";
import { MemoryWorld, type SharedStore } from "./shared-store.js";
import {
  StatementFixture,
  type StatementMessage,
} from "./statement-fixture.js";
import {
  TEST_USERS,
  fixtureHostKeys,
  personOf,
  type TestUser,
} from "./test-users.js";

/**
 * Two (or three) users' hosts and products, each with its own storage, over
 * one shared in-memory store: the real invitation service, real signatures
 * and sealing, the local delivery, the statement store stand-in and Example
 * Chat's setup protocol. Only the people and their keys are a fixture.
 */

const START = Date.UTC(2026, 9, 21, 12, 0, 0);
const MINUTE = 60_000;
const HOUR = 60 * MINUTE;
const NETWORK = "testnet";
const PRODUCT = { id: "example-chat.test", name: "Example Chat" };
/** The sealing purpose of an envelope, as `binding.ts` uses it, to forge one. */
const ENVELOPE_SEAL = "truapi-web-host/invitation-seal/0";
const [ALICE, BOB, CAROL] = TEST_USERS as [TestUser, TestUser, TestUser];

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

/** What a sender's product keeps of an invitation. */
interface Sent {
  secrets: setup.SenderSecrets;
  request: ReturnType<typeof setup.createInvitation>["request"];
  expiresAt: number;
}

/** One user: a host with its invitation service, and the product's side of the port. */
class Actor {
  readonly storage = new MemoryStorage();
  readonly notifications: NotificationStore;
  /** Everything the host posted to the product. */
  readonly messages: HostMessage[] = [];
  /** Statements the product heard on its topics. */
  readonly heard: StatementMessage[] = [];
  /** Keys this user's documents wrote to the shared store. */
  readonly writes: string[] = [];
  readonly log: string[] = [];
  readonly intents: IncomingInvitation[] = [];
  readonly sent = new Map<string, Sent>();
  pickAnswer: TestUser | null = null;
  confirm = true;
  service!: InvitationService;
  statements!: StatementFixture;
  private handler: ((value: unknown) => void) | null = null;
  private readonly pending = new Map<number, (result: CallResult) => void>();
  private nextId = 1;
  readonly store: SharedStore;

  constructor(
    readonly user: TestUser,
    world: MemoryWorld,
    readonly clock: Clock,
  ) {
    this.notifications = new NotificationStore(this.storage, clock.now);
    this.notifications.setScope(`${user.id}:notifications`);
    const inner = world.store();
    this.store = {
      ...inner,
      set: (key, value) => {
        this.writes.push(key);
        inner.set(key, value);
      },
    };
    this.start();
  }

  /** Start the host (again, after `stop`), over the same storage. */
  start(): void {
    const keys = fixtureHostKeys(this.user);
    this.service = new InvitationService({
      product: PRODUCT,
      self: keys,
      contacts: new FixtureContacts(this.user, async () =>
        this.pickAnswer === null ? null : { account: this.pickAnswer.account },
      ),
      notifications: this.notifications,
      storage: this.storage,
      keys: { sent: "sent", received: "received" },
      delivery: new LocalDelivery({
        self: toHex(keys.inbox.public),
        network: NETWORK,
        store: this.store,
        now: this.clock.now,
        schedule: this.clock.schedule,
        log: (line) => this.log.push(line),
      }),
      now: this.clock.now,
      schedule: this.clock.schedule,
      random: cryptoRandom,
      confirmSend: async () => this.confirm,
      showProduct: () => {},
      log: (line) => this.log.push(line),
    });
    this.statements = new StatementFixture({
      network: NETWORK,
      store: this.store,
      now: this.clock.now,
      send: (message) => this.heard.push(structuredClone(message)),
    });
    this.service.start();
    const port: ProductPort = {
      post: (message) => {
        this.messages.push(structuredClone(message));
        if (message.kind === "result") {
          this.pending.get(message.id)?.(message.result);
          this.pending.delete(message.id);
        }
        if (message.kind === "acceptIntent")
          this.intents.push(message.invitation);
      },
      listen: (handler) => (this.handler = handler),
      close: () => {},
    };
    this.service.attachProduct(port);
  }

  stop(): void {
    this.service.stop();
    this.statements.close();
  }

  call(fields: Record<string, unknown>): Promise<CallResult> {
    const id = this.nextId++;
    return new Promise((resolve) => {
      this.pending.set(id, resolve);
      this.handler?.({ id, ...fields });
    });
  }

  watch(topic: string): void {
    this.statements.call({ statements: "subscribe", topic });
  }

  post(topic: string, data: string): void {
    this.statements.call({ statements: "post", topic, data });
  }
}

function world(): { clock: Clock; alice: Actor; bob: Actor; carol: Actor } {
  const shared = new MemoryWorld();
  const clock = new Clock();
  return {
    clock,
    alice: new Actor(ALICE, shared, clock),
    bob: new Actor(BOB, shared, clock),
    carol: new Actor(CAROL, shared, clock),
  };
}

function ok(result: CallResult): Extract<CallResult, { ok: true }> {
  if (!result.ok) throw new Error(`${result.reason}: ${result.detail}`);
  return result;
}

/** Alice's product: pick `to`, make an invitation, watch `T_BA`, then send. */
async function invite(
  alice: Actor,
  to: TestUser,
  options: { ttlMs?: number; setupData?: string } = {},
): Promise<Sent> {
  alice.pickAnswer = to;
  const picked = ok(await alice.call({ call: "pickContact" }));
  expect(picked.contact!.startsWith(FIXTURE_REF_PREFIX)).toBe(true);
  const { request, secrets } = setup.createInvitation(cryptoRandom);
  if (options.setupData !== undefined) request.setup = options.setupData;
  alice.watch(secrets.replyTopic);
  const sent = ok(
    await alice.call({
      call: "send",
      request: {
        contact: picked.contact,
        purpose: "Chat?",
        ttlMs: options.ttlMs ?? HOUR,
        invitation: request,
      },
    }),
  );
  const entry = { secrets, request, expiresAt: sent.invitation!.expiresAt };
  alice.sent.set(secrets.requestId, entry);
  return entry;
}

/** Bob's user chooses Accept; his product checks the setup, responds, accepts, and posts the reply. */
async function accept(
  bob: Actor,
  id: string,
  respond: (invitation: IncomingInvitation) => {
    response: string;
    replyKey: string;
    replyTopic: string;
  } | null = (intent) => {
    const made = setup.respond(intent.invitation, cryptoRandom);
    return made === null
      ? null
      : {
          response: made.response,
          replyKey: made.setup.replyKey,
          replyTopic: made.setup.replyTopic,
        };
  },
): Promise<{ user: Promise<void>; result: CallResult; reply?: string }> {
  const user = bob.service.requestAccept(id);
  user.catch(() => {});
  const intent = bob.intents.at(-1)!;
  const made = respond(intent);
  if (made === null) {
    const result = await bob.call({
      call: "refuse",
      invitationId: id,
      reason: "Example Chat cannot use this invitation's setup data.",
    });
    return { user, result };
  }
  const result = await bob.call({
    call: "accept",
    invitationId: id,
    response: made.response,
    open: true,
  });
  if (!result.ok) return { user, result };
  const reply = setup.sealReply(
    made.replyKey,
    made.response,
    result.proof!,
    cryptoRandom,
  );
  bob.post(made.replyTopic, reply);
  return { user, result, reply };
}

/** Alice's product: the replies heard on `entry`'s topic, opened. */
const replies = (alice: Actor, entry: Sent) =>
  alice.heard
    .filter((message) => message.topic === entry.secrets.replyTopic)
    .map((message) => setup.openReply(entry.secrets, message.data));

const outgoing = (actor: Actor, requestId: string) =>
  actor.service.outbox.status(actor.service.outbox.get(requestId)!);

const mailbox = (actor: Actor) =>
  actor.writes.filter((key) => key.startsWith(mailboxPrefix(NETWORK)));

const idFor = (from: TestUser, entry: Sent) =>
  receivedId(personOf(from).keys.signingPublic, entry.secrets.requestId);

describe("a JIT invitation between two users' hosts and products", () => {
  test("connects only after the host verifies the proof and the product checks its setup", async () => {
    const { clock, alice, bob } = world();
    const entry = await invite(alice, BOB);
    const id = idFor(ALICE, entry);

    // Bob's host checked Alice's signature and keeps a request. His product hears nothing yet.
    const row = bob.notifications.get(id)!;
    expect(row.sender).toEqual({ name: "Alice", handle: "alice.example" });
    expect(row.state).toEqual({ tag: "pending" });
    expect(bob.messages.filter((m) => m.kind !== "result")).toEqual([]);

    // Accept is Bob's intent; the row is accepted only once his product accepts.
    const user = bob.service.requestAccept(id);
    expect(bob.notifications.get(id)!.state.tag).toBe("pending");
    expect(bob.intents[0]!.invitation).toEqual({
      ...entry.request,
      expiresAt: entry.expiresAt,
    });
    const made = setup.respond(bob.intents[0]!.invitation, cryptoRandom)!;
    const accepted = ok(
      await bob.call({
        call: "accept",
        invitationId: id,
        response: made.response,
        open: true,
      }),
    );
    await user;
    expect(bob.notifications.get(id)!.state.tag).toBe("accepted");
    bob.post(
      made.setup.replyTopic,
      setup.sealReply(
        made.setup.replyKey,
        made.response,
        accepted.proof!,
        cryptoRandom,
      ),
    );

    // Alice's product opens the reply; her host verifies. Still waiting.
    const [reply] = replies(alice, entry);
    expect(reply!.response).toBe(made.response);
    ok(
      await alice.call({
        call: "verify",
        requestId: entry.secrets.requestId,
        ...reply,
      }),
    );
    expect(outgoing(alice, entry.secrets.requestId)).toBe("waiting");
    const posted = mailbox(alice).length;
    clock.advance(REPOST_INTERVAL_MS);
    expect(mailbox(alice).length).toBe(posted + 1);

    // The product checks its own setup, then completes: the reposts stop.
    const checked = setup.checkResponse(
      entry.secrets,
      { ...entry.request, expiresAt: entry.expiresAt },
      reply!.response,
    );
    expect(checked).toEqual({
      ok: true,
      topic: made.topic,
      peerKey: expect.any(String),
    });
    const done = ok(
      await alice.call({
        call: "complete",
        requestId: entry.secrets.requestId,
        response: reply!.response,
      }),
    );
    expect(done.invitation!.status).toBe("completed");
    const after = mailbox(alice).length;
    clock.advance(3 * REPOST_INTERVAL_MS);
    expect(mailbox(alice).length).toBe(after);

    // Bob's host never wrote to the mailbox: no receipt, answer or decline.
    expect(mailbox(bob)).toEqual([]);
    // Everything in it is routed to Bob.
    const bobRoute = routeOf(personOf(BOB).keys.inboxPublic);
    for (const key of mailbox(alice)) expect(key).toContain(bobRoute);
  });

  test("no product sees a person's name, account or keys", async () => {
    const { alice, bob } = world();
    const entry = await invite(alice, BOB);
    await accept(bob, idFor(ALICE, entry));
    const [reply] = replies(alice, entry);
    ok(
      await alice.call({
        call: "verify",
        requestId: entry.secrets.requestId,
        ...reply,
      }),
    );
    const identifiers = (user: TestUser) => {
      const person = personOf(user);
      return [
        user.name,
        user.handle,
        user.account.slice(2),
        person.keys.signingPublic.slice(2),
        person.keys.inboxPublic.slice(2),
      ];
    };
    const seenBy = (actor: Actor) =>
      JSON.stringify([actor.messages, actor.heard]).toLowerCase();
    for (const value of identifiers(BOB))
      expect(seenBy(alice)).not.toContain(value.toLowerCase());
    for (const value of identifiers(ALICE))
      expect(seenBy(bob)).not.toContain(value.toLowerCase());
    // The sender reaches Bob's product as an opaque reference.
    expect(bob.intents[0]!.sender.startsWith(FIXTURE_REF_PREFIX)).toBe(true);
  });

  test("the host rejects a proof bound to anything else", async () => {
    const { alice, bob } = world();
    const first = await invite(alice, BOB);
    const second = await invite(alice, BOB);
    const { reply } = await accept(bob, idFor(ALICE, first));
    const opened = setup.openReply(first.secrets, reply!)!;
    const verify = (requestId: string, response: string, proof: string) =>
      alice.call({ call: "verify", requestId, response, proof });
    const rejected = async (result: Promise<CallResult>, detail: string) =>
      expect(await result).toEqual({ ok: false, reason: "Rejected", detail });

    const stored = alice.service.outbox.get(first.secrets.requestId)!.envelope;
    const aliceKeys = personOf(ALICE).keys;
    const proofFor = (who: TestUser, envelope: Envelope, response: string) =>
      issueProof(
        fixtureHostKeys(who),
        aliceKeys,
        envelope,
        response,
        cryptoRandom,
      );
    const flipped = (hex: string) =>
      `${hex.slice(0, -2)}${hex.endsWith("00") ? "01" : "00"}`;

    // The response was changed.
    await rejected(
      verify(first.secrets.requestId, flipped(opened.response), opened.proof),
      "the proof is for another response",
    );
    // The proof was changed.
    await rejected(
      verify(first.secrets.requestId, opened.response, flipped(opened.proof)),
      "the proof is not sealed to this host",
    );
    // A real proof, for another invitation.
    await rejected(
      verify(second.secrets.requestId, opened.response, opened.proof),
      "the proof is for another invitation",
    );
    // Someone other than the contact invited.
    await rejected(
      verify(
        first.secrets.requestId,
        opened.response,
        proofFor(CAROL, stored, opened.response),
      ),
      "the proof is from someone other than the contact invited",
    );
    // Bob's key over a changed invitation: challenge, setup or product.
    for (const envelope of [
      {
        ...stored,
        invitation: { ...stored.invitation, challenge: `0x${"00".repeat(32)}` },
      },
      {
        ...stored,
        invitation: {
          ...stored.invitation,
          setup: flipped(stored.invitation.setup),
        },
      },
      { ...stored, product: { id: "other.test", name: "Other" } },
    ])
      await rejected(
        verify(
          first.secrets.requestId,
          opened.response,
          proofFor(BOB, envelope, opened.response),
        ),
        "the proof is for another invitation",
      );
    // A proof not sealed to Alice's host.
    const sealedToCarol = issueProof(
      fixtureHostKeys(BOB),
      personOf(CAROL).keys,
      stored,
      opened.response,
      cryptoRandom,
    );
    await rejected(
      verify(first.secrets.requestId, opened.response, sealedToCarol),
      "the proof is not sealed to this host",
    );
    // Nothing completes without a verified response.
    expect(
      await alice.call({
        call: "complete",
        requestId: first.secrets.requestId,
        response: opened.response,
      }),
    ).toEqual({
      ok: false,
      reason: "Refused",
      detail: "This response was not verified.",
    });
    expect(outgoing(alice, first.secrets.requestId)).toBe("waiting");
  });

  test("a verified response the product's own check refuses does not connect", async () => {
    const { alice, bob } = world();
    const entry = await invite(alice, BOB);
    await accept(bob, idFor(ALICE, entry), (intent) => {
      const made = setup.respond(intent.invitation, cryptoRandom)!;
      // A response whose confirmation does not match the keys it names.
      const other = setup.respond(intent.invitation, cryptoRandom)!;
      const forged = JSON.parse(
        new TextDecoder().decode(Buffer.from(made.response.slice(2), "hex")),
      );
      forged.confirm = JSON.parse(
        new TextDecoder().decode(Buffer.from(other.response.slice(2), "hex")),
      ).confirm;
      return {
        response: `0x${Buffer.from(JSON.stringify(forged)).toString("hex")}`,
        replyKey: made.setup.replyKey,
        replyTopic: made.setup.replyTopic,
      };
    });
    const [reply] = replies(alice, entry);
    ok(
      await alice.call({
        call: "verify",
        requestId: entry.secrets.requestId,
        ...reply,
      }),
    );
    expect(
      setup.checkResponse(
        entry.secrets,
        { ...entry.request, expiresAt: entry.expiresAt },
        reply!.response,
      ),
    ).toEqual({ ok: false, reason: "the confirmation does not check" });
    expect(outgoing(alice, entry.secrets.requestId)).toBe("waiting");
  });

  test("the recipient's host ignores what another person forged, changed or re-addressed", async () => {
    const { alice, bob, carol } = world();
    const entry = await invite(alice, BOB);
    const toCarol = await invite(alice, CAROL);
    const stored = alice.service.outbox.get(entry.secrets.requestId)!;
    const bobKeys = personOf(BOB).keys;
    const deliver = (sealed: string) =>
      new LocalDelivery({
        self: personOf(CAROL).keys.inboxPublic,
        network: NETWORK,
        store: carol.store,
        now: carol.clock.now,
        schedule: carol.clock.schedule,
        log: () => {},
      }).post(bobKeys, "forged", sealed, entry.expiresAt);
    /** Open `sealed` as `holder`, change it, and seal it for Bob. */
    const reseal = (
      sealed: string,
      holder: TestUser,
      edit: (plain: Record<string, unknown>) => Record<string, unknown>,
    ) => {
      const plain = openSealed(
        fixtureHostKeys(holder).inbox,
        fromHex(sealed),
        ENVELOPE_SEAL,
      )!;
      const value = JSON.parse(new TextDecoder().decode(plain)) as Record<
        string,
        unknown
      >;
      return toHex(
        sealTo(
          fromHex(bobKeys.inboxPublic),
          utf8(JSON.stringify(edit(value))),
          ENVELOPE_SEAL,
          cryptoRandom,
        ),
      );
    };

    // Carol signs a changed invitation and claims it is Alice's.
    const changed: Envelope = { ...stored.envelope, purpose: "Send me money." };
    deliver(
      reseal(
        sealEnvelope(fixtureHostKeys(CAROL), bobKeys, changed, cryptoRandom),
        BOB,
        (plain) => ({ ...plain, from: personOf(ALICE).keys.signingPublic }),
      ),
    );
    // Alice's invitation to Carol, passed on to Bob with Alice's signature.
    const carolsCopy = alice.service.outbox.get(toCarol.secrets.requestId)!;
    deliver(reseal(carolsCopy.sealed, CAROL, (plain) => plain));
    // Alice's invitation to Bob with a byte changed.
    deliver(
      `${stored.sealed.slice(0, -2)}${stored.sealed.endsWith("00") ? "01" : "00"}`,
    );

    expect(bob.notifications.list().map((item) => item.purpose)).toEqual([
      "Chat?",
    ]);
    expect(bob.log.filter((line) => line.startsWith("Ignored"))).toEqual([
      "Ignored an invitation: the signature does not check.",
      "Ignored an invitation: the signature does not check.",
      "Ignored an invitation: not sealed to this host.",
    ]);
  });

  test("decline is silent, and a dismissed invitation stays dismissed across reposts and restarts", async () => {
    const { clock, alice, bob } = world();
    const entry = await invite(alice, BOB);
    const id = idFor(ALICE, entry);
    bob.notifications.dismiss(id);
    clock.advance(2 * REPOST_INTERVAL_MS);
    bob.stop();
    bob.start();
    clock.advance(REPOST_INTERVAL_MS);
    expect(bob.notifications.listed()).toEqual([]);
    expect(bob.notifications.list()).toHaveLength(1);
    expect(bob.intents).toEqual([]);
    expect(bob.writes).toEqual([]);
    expect(outgoing(alice, entry.secrets.requestId)).toBe("waiting");
    expect(
      alice.messages.filter(
        (message) =>
          message.kind === "outgoing" &&
          message.invitation.status !== "waiting",
      ),
    ).toEqual([]);
  });

  test("the recipient's product checks the setup before anything is sent", async () => {
    const { alice, bob } = world();
    const entry = await invite(alice, BOB, {
      setupData: `0x${"ab".repeat(64)}`,
    });
    const id = idFor(ALICE, entry);
    const { user, result } = await accept(bob, id, (intent) => {
      expect(setup.readSetup(intent.invitation.setup)).toBeNull();
      return null;
    });
    expect(result.ok).toBe(true);
    await expect(user).rejects.toThrow(
      "Example Chat cannot use this invitation's setup data.",
    );
    expect(bob.notifications.get(id)!.state.tag).toBe("pending");
    expect(bob.writes).toEqual([]);
    expect(alice.heard).toEqual([]);
  });

  test("cancel stops the reposts, and a late response is refused", async () => {
    const { clock, alice, bob } = world();
    const entry = await invite(alice, BOB);
    const { requestId } = entry.secrets;
    const cancelled = ok(await alice.call({ call: "cancel", requestId }));
    expect(cancelled.invitation!.status).toBe("cancelled");
    const posted = mailbox(alice).length;
    clock.advance(3 * REPOST_INTERVAL_MS);
    expect(mailbox(alice).length).toBe(posted);

    // Bob never learns of the cancel; a reply he sends now finds nothing.
    await accept(bob, idFor(ALICE, entry));
    const [reply] = replies(alice, entry);
    expect(await alice.call({ call: "verify", requestId, ...reply })).toEqual({
      ok: false,
      reason: "Refused",
      detail: "This invitation is cancelled.",
    });
    expect(
      await alice.call({
        call: "complete",
        requestId,
        response: reply!.response,
      }),
    ).toEqual({
      ok: false,
      reason: "Refused",
      detail: "This invitation is cancelled.",
    });

    // What was posted lapses on its own.
    clock.advance(POST_LIFETIME_MS + REPOST_INTERVAL_MS);
    expect(
      [...new Set(mailbox(alice))].filter(
        (key) => alice.store.get(key) !== null,
      ),
    ).toEqual([]);
  });

  test("completed and cancelled exclude each other", async () => {
    const { alice, bob } = world();
    const entry = await invite(alice, BOB);
    const { requestId } = entry.secrets;
    await accept(bob, idFor(ALICE, entry));
    const [reply] = replies(alice, entry);
    ok(await alice.call({ call: "verify", requestId, ...reply }));
    ok(
      await alice.call({
        call: "complete",
        requestId,
        response: reply!.response,
      }),
    );
    expect(await alice.call({ call: "cancel", requestId })).toEqual({
      ok: false,
      reason: "Refused",
      detail: "This invitation is completed.",
    });
  });

  test("expiry stops the reposts; neither side can complete after it", async () => {
    const { clock, alice, bob } = world();
    const entry = await invite(alice, BOB, { ttlMs: 2 * MINUTE });
    const id = idFor(ALICE, entry);
    clock.advance(2 * MINUTE);
    expect(outgoing(alice, entry.secrets.requestId)).toBe("expired");
    expect(alice.messages.at(-1)).toEqual({
      kind: "outgoing",
      invitation: expect.objectContaining({ status: "expired" }),
    });
    const posted = mailbox(alice).length;
    clock.advance(3 * REPOST_INTERVAL_MS);
    expect(mailbox(alice).length).toBe(posted);
    await expect(bob.service.requestAccept(id)).rejects.toThrow(
      "This request expired.",
    );
    expect(bob.intents).toEqual([]);
  });

  test("an invitation that arrives after its expiry is not kept", async () => {
    const { clock, alice, bob } = world();
    bob.stop();
    const entry = await invite(alice, BOB, { ttlMs: 2 * MINUTE });
    clock.advance(2 * MINUTE + 1);
    bob.start();
    expect(bob.notifications.get(idFor(ALICE, entry))).toBeUndefined();
  });

  test("repeats have one effect: reposts, two copies of a reply, a second accept", async () => {
    const { clock, alice, bob } = world();
    const entry = await invite(alice, BOB);
    const id = idFor(ALICE, entry);
    clock.advance(3 * REPOST_INTERVAL_MS);
    expect(bob.notifications.list()).toHaveLength(1);

    // The user's second Accept while the product decides is the same one.
    const first = bob.service.requestAccept(id);
    expect(bob.service.requestAccept(id)).toBe(first);
    expect(bob.intents).toHaveLength(1);
    const made = setup.respond(bob.intents[0]!.invitation, cryptoRandom)!;
    const accepted = ok(
      await bob.call({
        call: "accept",
        invitationId: id,
        response: made.response,
        open: false,
      }),
    );
    expect(
      await bob.call({
        call: "accept",
        invitationId: id,
        response: made.response,
        open: false,
      }),
    ).toEqual({
      ok: false,
      reason: "Refused",
      detail: "The user did not choose Accept for it.",
    });
    await expect(bob.service.requestAccept(id)).rejects.toThrow(
      "This request was already accepted.",
    );

    const reply = setup.sealReply(
      made.setup.replyKey,
      made.response,
      accepted.proof!,
      cryptoRandom,
    );
    bob.post(made.setup.replyTopic, reply);
    bob.post(made.setup.replyTopic, reply);
    const { requestId } = entry.secrets;
    const opened = replies(alice, entry)[0]!;
    ok(await alice.call({ call: "verify", requestId, ...opened }));
    ok(
      await alice.call({
        call: "complete",
        requestId,
        response: opened.response,
      }),
    );
    expect(await alice.call({ call: "verify", requestId, ...opened })).toEqual({
      ok: false,
      reason: "Refused",
      detail: "This invitation is completed.",
    });
    expect(
      await alice.call({
        call: "complete",
        requestId,
        response: opened.response,
      }),
    ).toEqual({
      ok: false,
      reason: "Refused",
      detail: "This invitation is completed.",
    });
  });

  test("a stopped host stops posting; started again, it posts what still waits", async () => {
    const { clock, alice, bob } = world();
    bob.stop();
    const entry = await invite(alice, BOB);
    alice.stop();
    const posted = mailbox(alice).length;
    clock.advance(3 * REPOST_INTERVAL_MS);
    expect(mailbox(alice).length).toBe(posted);
    alice.start();
    expect(mailbox(alice).length).toBe(posted + 1);
    bob.start();
    expect(bob.notifications.get(idFor(ALICE, entry))?.state.tag).toBe(
      "pending",
    );
  });

  test("the sender asks its user, and a product cannot send to a reference it made up", async () => {
    const { alice, bob } = world();
    alice.confirm = false;
    alice.pickAnswer = BOB;
    const picked = ok(await alice.call({ call: "pickContact" }));
    const { request } = setup.createInvitation(cryptoRandom);
    const send = (contact: string) =>
      alice.call({
        call: "send",
        request: {
          contact,
          purpose: "Chat?",
          ttlMs: HOUR,
          invitation: request,
        },
      });
    expect(await send(picked.contact!)).toEqual({
      ok: false,
      reason: "Dismissed",
      detail: "The user did not send it.",
    });
    expect(await send(`${FIXTURE_REF_PREFIX}0x${"00".repeat(16)}`)).toEqual({
      ok: false,
      reason: "Unknown",
      detail: "This contact names nobody here.",
    });
    expect(alice.writes).toEqual([]);
    expect(bob.notifications.list()).toEqual([]);
  });
});
