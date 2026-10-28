import { beforeEach, describe, expect, test } from "bun:test";
import {
  REPLY_SEAL,
  REQUEST_SEAL,
  replyTranscript,
  sealReply,
  sealRequest,
  type ReplyContext,
  type RequestContext,
} from "../invitations/binding.js";
import {
  PAYLOAD_MAX_BYTES,
  PURPOSE_MAX_CHARS,
  type CallResult,
  type ContactHandle,
  type HostMessage,
  type IncomingRequest,
} from "../invitations/contract.js";
import {
  InvitationService,
  type ProductPort,
} from "../invitations/invitation-service.js";
import type { OutgoingRequest } from "../invitations/outbox.js";
import type { AuthEvidence } from "../invitations/ports.js";
import {
  cryptoRandom,
  fromHex,
  fromUtf8,
  sealTo,
  toHex,
  utf8,
} from "../invitations/sealing.js";
import { NotificationStore } from "../notifications.js";
import { MemoryStorage } from "../test-support.js";
import * as setup from "./example-chat/setup.js";
import { FixtureContacts } from "./fixture-contacts.js";
import { FixtureIdentityAuth, publicIdentity } from "./fixture-identity.js";
import {
  LocalDelivery,
  POST_LIFETIME_MS,
  REPOST_INTERVAL_MS,
  deliveryPrefix,
  inboxOf,
  type ReturnRoutes,
} from "./local-delivery.js";
import { MemoryWorld, type SharedStore } from "./shared-store.js";
import {
  TEST_USERS,
  fixtureIdentity,
  personOf,
  type TestUser,
} from "./test-users.js";

/**
 * Two or three users' hosts and products, each with its own storage, over one
 * shared in-memory store: the real request service, real authentication and
 * sealing with the reference fixture profile, the local delivery, and
 * Example Chat's setup protocol. Only the people and their keys are made up.
 */

const START = Date.UTC(2026, 9, 21, 12, 0, 0);
const MINUTE = 60_000;
const HOUR = 60 * MINUTE;
const NETWORK = "testnet";
const PRODUCT = { id: "example-chat.test", name: "Example Chat" };
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
  handle: string;
  secrets: setup.SenderSecrets;
}

/** One user: a host with its request service, and the product's side of the port. */
class Actor {
  readonly storage = new MemoryStorage();
  readonly notifications: NotificationStore;
  /** Everything the host posted to the product. */
  readonly messages: HostMessage[] = [];
  /** Keys this user's host wrote to the shared store. */
  readonly writes: string[] = [];
  readonly log: string[] = [];
  readonly auth: FixtureIdentityAuth;
  readonly store: SharedStore;
  pickAnswer: TestUser | null = null;
  confirm = true;
  service!: InvitationService;
  private handler: ((value: unknown) => void) | null = null;
  private readonly pending = new Map<number, (result: CallResult) => void>();
  private nextId = 1;

  constructor(
    readonly user: TestUser,
    world: MemoryWorld,
    readonly clock: Clock,
    readonly routes: ReturnRoutes,
  ) {
    this.notifications = new NotificationStore(this.storage, clock.now);
    this.notifications.setScope(`${user.id}:notifications`);
    this.auth = new FixtureIdentityAuth(fixtureIdentity(user), cryptoRandom);
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
    this.service = new InvitationService({
      product: PRODUCT,
      auth: this.auth,
      contacts: new FixtureContacts(this.user, async () =>
        this.pickAnswer === null ? null : { account: this.pickAnswer.account },
      ),
      notifications: this.notifications,
      storage: this.storage,
      keys: { sent: "sent", received: "received" },
      delivery: new LocalDelivery({
        self: publicIdentity(fixtureIdentity(this.user)).agreementPublic,
        network: NETWORK,
        store: this.store,
        now: this.clock.now,
        schedule: this.clock.schedule,
        random: cryptoRandom,
        returnRoutes: this.routes,
        log: (line) => this.log.push(line),
      }),
      now: this.clock.now,
      schedule: this.clock.schedule,
      random: cryptoRandom,
      confirmSend: async () => this.confirm,
      showProduct: () => {},
      log: (line) => this.log.push(line),
    });
    this.service.start();
    this.attach();
  }

  /** Attach a fresh product port, as a product that restarts does. */
  attach(): void {
    const port: ProductPort = {
      post: (message) => {
        this.messages.push(structuredClone(message));
        if (message.kind === "result") {
          this.pending.get(message.id)?.(message.result);
          this.pending.delete(message.id);
        }
      },
      listen: (handler) => (this.handler = handler),
      close: () => {},
    };
    this.service.attachProduct(port);
  }

  stop(): void {
    this.service.stop();
  }

  call(fields: Record<string, unknown>): Promise<CallResult> {
    const id = this.nextId++;
    return new Promise((resolve) => {
      this.pending.set(id, resolve);
      this.handler?.({ id, ...fields });
    });
  }

  intents(): IncomingRequest[] {
    return this.messages.flatMap((message) =>
      message.kind === "acceptIntent" ? [message.request] : [],
    );
  }

  /** Reply payloads the host handed the product for `handle`. */
  responses(handle: string): string[] {
    return this.messages.flatMap((message) =>
      message.kind === "response" && message.handle === handle
        ? [message.payload]
        : [],
    );
  }

  outgoing(handle: string): OutgoingRequest {
    return this.service.outbox.get(handle)!;
  }

  status(handle: string) {
    return this.service.outbox.status(this.outgoing(handle));
  }
}

/** The kind of return route the deliveries make in the tests now running. */
let routes: ReturnRoutes = "inbox";

function world() {
  const shared = new MemoryWorld();
  const clock = new Clock();
  return {
    shared,
    clock,
    alice: new Actor(ALICE, shared, clock, routes),
    bob: new Actor(BOB, shared, clock, routes),
    carol: new Actor(CAROL, shared, clock, routes),
  };
}

function ok(result: CallResult): Extract<CallResult, { ok: true }> {
  if (!result.ok) throw new Error(`${result.reason}: ${result.detail}`);
  return result;
}

/** Alice's product: pick `to`, make an invitation, and ask the host to send it. */
async function invite(
  alice: Actor,
  to: TestUser,
  options: { ttlMs?: number; payload?: string } = {},
): Promise<Sent> {
  alice.pickAnswer = to;
  const picked = ok(await alice.call({ call: "pickContact" }));
  if (picked.outcome?.tag !== "Picked") throw new Error("nothing picked");
  const { payload, secrets } = setup.createInvitation(cryptoRandom);
  const sent = ok(
    await alice.call({
      call: "request",
      request: {
        contact: picked.outcome.value.handle,
        purpose: "Chat?",
        ttlMs: options.ttlMs ?? HOUR,
        payload: options.payload ?? payload,
      },
    }),
  );
  return { handle: sent.request!.handle, secrets };
}

/** The handle Bob's host gave the newest request it keeps. */
const newest = (bob: Actor) => bob.notifications.list()[0]!.id;

/** Bob's user chooses Accept; his product checks the payload, makes its response and replies. */
async function accept(bob: Actor, handle = newest(bob)) {
  const user = bob.service.requestAccept(handle);
  user.catch(() => {});
  const intent = bob.intents().at(-1)!;
  const made = setup.respond(intent.payload, cryptoRandom);
  if (made === null) {
    await bob.call({ call: "refuse", handle, reason: "Unusable." });
    return { user, made, result: null };
  }
  const result = await bob.call({
    call: "reply",
    handle,
    payload: made.response,
    open: false,
  });
  return { user, made, result };
}

const deliveryWrites = (actor: Actor) =>
  actor.writes.filter((key) => key.startsWith(deliveryPrefix(NETWORK)));

/** The return routes `actor`'s host posted replies on, read back from the store. */
function replyRoutes(actor: Actor): Set<string> {
  const found = new Set<string>();
  for (const key of deliveryWrites(actor)) {
    const [kind, label] = key.slice(deliveryPrefix(NETWORK).length).split(":");
    if (kind === "topic") found.add(`topic:${label}`);
    else {
      const post = JSON.parse(actor.store.get(key) ?? "{}") as {
        route?: string;
      };
      if (post.route !== undefined) found.add(post.route);
    }
  }
  return found;
}

/** A reply to Alice's `item`, sealed to its reply key with whatever context, payload and evidence. */
function forgeReply(
  item: OutgoingRequest,
  parts: { context: ReplyContext; payload: string; evidence: AuthEvidence },
): string {
  return toHex(
    sealTo(
      fromHex(item.context.replyKey),
      utf8(JSON.stringify(parts)),
      REPLY_SEAL,
      cryptoRandom,
    ),
  );
}

/** A delivery that posts into `actor`'s store, as any host would. */
const deliveryFor = (actor: Actor) =>
  new LocalDelivery({
    self: `0x${"00".repeat(32)}`,
    network: NETWORK,
    store: actor.store,
    now: actor.clock.now,
    schedule: () => () => {},
    random: cryptoRandom,
    returnRoutes: actor.routes,
    log: () => {},
  });

/** Post `sealed` on a return route of Alice's, as any host would. */
function postOn(actor: Actor, route: string, sealed: string, until: number) {
  deliveryFor(actor).postReply(route, toHex(cryptoRandom(16)), sealed, until);
}

for (const kind of ["inbox", "topic"] as const)
  describe(`a request between two users' hosts and products, with ${kind} return routes`, () => {
    beforeEach(() => {
      routes = kind;
    });

    test("connects only after the sender's host authenticates the reply and its product checks it", async () => {
      const { clock, alice, bob } = world();
      const sent = await invite(alice, BOB);
      const item = alice.outgoing(sent.handle);

      // Bob's host authenticated Alice and keeps a request; his product hears nothing yet.
      const handle = newest(bob);
      const row = bob.notifications.get(handle)!;
      expect(row.sender).toEqual({ name: "Alice", handle: "alice.example" });
      expect(row.purpose).toBe("Chat?");
      expect(row.state).toEqual({ tag: "pending" });
      expect(bob.messages).toEqual([]);

      // Accept is Bob's intent: his product gets the payload and opaque handles only.
      const { user, made, result } = await accept(bob);
      expect(bob.intents()).toEqual([
        {
          handle,
          sender: expect.objectContaining({ bytes: expect.any(String) }),
          payload: sent.secrets.payload,
        },
      ]);
      expect(Object.keys(bob.intents()[0]!).sort()).toEqual([
        "handle",
        "payload",
        "sender",
      ]);
      expect(result).toEqual({ ok: true });
      await user;
      expect(bob.notifications.get(handle)!.state.tag).toBe("accepted");
      // Bob's host posted the reply on the return route from the authenticated request, and only there.
      expect(item.context.returnRoute.startsWith(`${kind}:`)).toBe(true);
      expect(replyRoutes(bob)).toEqual(new Set([item.context.returnRoute]));
      expect(deliveryWrites(bob)).toHaveLength(1);

      // Alice's product gets Bob's exact payload, and the request still waits.
      expect(alice.responses(sent.handle)).toEqual([made!.response]);
      expect(alice.status(sent.handle)).toBe("waiting");
      const posted = deliveryWrites(alice).length;
      clock.advance(REPOST_INTERVAL_MS);
      expect(deliveryWrites(alice).length).toBe(posted + 1);

      // Bob's product made both conversation topics; Alice's product checks, then completes.
      const checked = setup.checkResponse(sent.secrets, made!.response);
      expect(checked).toEqual({ ok: true, topics: made!.topics });
      const done = ok(
        await alice.call({ call: "complete", handle: sent.handle }),
      );
      expect(done.request!.status).toBe("completed");

      // Completion is local: Alice's host posts nothing more, to anyone.
      const after = deliveryWrites(alice).length;
      clock.advance(3 * REPOST_INTERVAL_MS);
      expect(deliveryWrites(alice).length).toBe(after);
    });

    test("no product sees a person's name, account or keys, or the request's id, route, reply key or evidence", async () => {
      const { alice, bob } = world();
      const sent = await invite(alice, BOB);
      await accept(bob);
      const context = alice.outgoing(sent.handle).context;
      const received = bob.service.received.get(newest(bob))!;
      const secret = (user: TestUser) => {
        const identity = personOf(user).identity;
        return [
          user.name,
          user.handle,
          user.account.slice(2),
          identity.signingPublic.slice(2),
          identity.agreementPublic.slice(2),
        ];
      };
      const wire = [
        context.requestId,
        context.returnRoute,
        context.replyKey,
        alice.outgoing(sent.handle).replySecret,
        received.replied!.sealed.slice(2, 66),
      ].map((value) => value.replace(/^0x/, ""));
      const seenBy = (actor: Actor) =>
        JSON.stringify(actor.messages).toLowerCase();
      for (const value of [...secret(BOB), ...wire])
        expect(seenBy(alice)).not.toContain(value.toLowerCase());
      for (const value of [...secret(ALICE), ...wire])
        expect(seenBy(bob)).not.toContain(value.toLowerCase());
      // Bob's product gets neither the purpose, which his host shows, nor the deadline.
      expect(seenBy(bob)).not.toContain("chat?");
      expect(seenBy(bob)).not.toContain(String(context.deadline));
    });

    test("the sender's host refuses a reply bound to anything else, and a refused reply does not use up the request", async () => {
      const { alice, bob, carol } = world();
      const sent = await invite(alice, BOB);
      const other = await invite(alice, BOB);
      const item = alice.outgoing(sent.handle);
      const otherItem = alice.outgoing(other.handle);
      const aliceAsPeer = personOf(ALICE);
      const refusals = () =>
        alice.log.filter((line) => line.startsWith("Ignored a reply"));
      const { context } = item;
      const deadline = context.deadline;
      const made = setup.respond(sent.secrets.payload, cryptoRandom)!;
      const replyContext: ReplyContext = {
        version: context.version,
        from: personOf(BOB).identity.signingPublic,
        to: aliceAsPeer.identity.signingPublic,
        productId: PRODUCT.id,
        requestId: context.requestId,
        requestDigest: item.digest,
      };
      const signed = (ctx: ReplyContext, payload: string, by = bob) =>
        by.auth.evidence("Reply", replyTranscript(ctx, payload), aliceAsPeer);
      const post = (sealed: string, route = context.returnRoute) =>
        postOn(carol, route, sealed, deadline);

      // From Carol, for Alice's request to Bob.
      post(
        sealReply(
          carol.auth,
          aliceAsPeer,
          { context: context as RequestContext, digest: item.digest },
          made.response,
          cryptoRandom,
        ),
      );
      // Bob's, but for another product, another request id or another request digest.
      for (const changed of [
        { ...replyContext, productId: "other.test" },
        { ...replyContext, requestId: otherItem.context.requestId },
        { ...replyContext, requestDigest: otherItem.digest },
      ])
        post(
          forgeReply(item, {
            context: changed,
            payload: made.response,
            evidence: signed(changed, made.response),
          }),
        );
      // Bob's evidence, with the payload changed after it was made.
      post(
        forgeReply(item, {
          context: replyContext,
          payload: `${made.response.slice(0, -2)}00`,
          evidence: signed(replyContext, made.response),
        }),
      );
      // Bob's request-kind evidence over the reply, and the same relabelled as a reply.
      const asRequest = bob.auth.evidence(
        "Request",
        replyTranscript(replyContext, made.response),
        aliceAsPeer,
      );
      post(
        forgeReply(item, {
          context: replyContext,
          payload: made.response,
          evidence: asRequest,
        }),
      );
      post(
        forgeReply(item, {
          context: replyContext,
          payload: made.response,
          evidence: { ...asRequest, kind: "Reply" },
        }),
      );
      // Addressed to another identity.
      const toCarol = {
        ...replyContext,
        to: personOf(CAROL).identity.signingPublic,
      };
      post(
        forgeReply(item, {
          context: toCarol,
          payload: made.response,
          evidence: signed(toCarol, made.response),
        }),
      );
      // A valid reply to the other request, posted on this request's topic.
      post(
        sealReply(
          bob.auth,
          aliceAsPeer,
          { context: otherItem.context, digest: otherItem.digest },
          made.response,
          cryptoRandom,
        ),
      );

      expect(refusals()).toEqual([
        "Ignored a reply to request " +
          sent.handle.slice(2, 10) +
          ": it is from someone other than the contact asked.",
        ...Array(3).fill(
          "Ignored a reply to request " +
            sent.handle.slice(2, 10) +
            ": it answers another request.",
        ),
        ...Array(3).fill(
          "Ignored a reply to request " +
            sent.handle.slice(2, 10) +
            ": its evidence does not check.",
        ),
        "Ignored a reply to request " +
          sent.handle.slice(2, 10) +
          ": it is for another identity.",
        "Ignored a reply to request " +
          sent.handle.slice(2, 10) +
          ": not sealed to this request.",
      ]);
      expect(alice.responses(sent.handle)).toEqual([]);
      expect(
        await alice.call({ call: "complete", handle: sent.handle }),
      ).toEqual({
        ok: false,
        reason: "Refused",
        detail: "No authenticated reply has arrived.",
      });

      // The request still waits, and Bob's own reply goes through.
      expect(alice.status(sent.handle)).toBe("waiting");
      const bobHandle = bob.notifications
        .list()
        .find(
          (row) =>
            bob.service.received.get(row.id)!.context.requestId ===
            context.requestId,
        )!.id;
      const { made: real } = await accept(bob, bobHandle);
      expect(alice.responses(sent.handle)).toEqual([real!.response]);
      ok(await alice.call({ call: "complete", handle: sent.handle }));
    });

    test("a reply the product's own check refuses does not connect", async () => {
      const { alice, bob } = world();
      const sent = await invite(alice, BOB);
      const handle = newest(bob);
      const user = bob.service.requestAccept(handle);
      const other = setup.createInvitation(cryptoRandom);
      // A response made for another invitation: authentic to the host, wrong for the product.
      const wrong = setup.respond(other.payload, cryptoRandom)!;
      ok(
        await bob.call({
          call: "reply",
          handle,
          payload: wrong.response,
          open: false,
        }),
      );
      await user;
      const [payload] = alice.responses(sent.handle);
      expect(setup.checkResponse(sent.secrets, payload!)).toEqual({
        ok: false,
        reason: "the confirmation does not check",
      });
      expect(alice.status(sent.handle)).toBe("waiting");
    });

    test("the recipient's host ignores a request forged, changed or meant for someone else", async () => {
      const { alice, bob, carol } = world();
      const sent = await invite(alice, BOB);
      await invite(alice, CAROL);
      const toBob = alice.outgoing(sent.handle);
      const toCarol = alice.service.outbox.list()[1]!;
      const bobPerson = personOf(BOB);
      const deliver = (sealed: string) =>
        deliveryFor(carol).postRequest(
          bobPerson.identity,
          toHex(cryptoRandom(16)),
          sealed,
          toBob.context.deadline,
        );
      /** Open a request sealed to `holder`, change it, and seal it for Bob. */
      const reseal = (
        sealed: string,
        holder: TestUser,
        edit: (plain: {
          context: RequestContext;
          payload: string;
          evidence: AuthEvidence;
        }) => unknown,
      ) => {
        const plain = new FixtureIdentityAuth(
          fixtureIdentity(holder),
          cryptoRandom,
        ).openSealed(fromHex(sealed), REQUEST_SEAL)!;
        return toHex(
          sealTo(
            fromHex(bobPerson.identity.agreementPublic),
            utf8(JSON.stringify(edit(JSON.parse(fromUtf8(plain))))),
            REQUEST_SEAL,
            cryptoRandom,
          ),
        );
      };

      // Changed deadline, return topic, reply key or payload of Alice's request to Bob.
      for (const edit of [
        { deadline: toBob.context.deadline + HOUR },
        { returnRoute: `topic:0x${"11".repeat(32)}` },
        {
          returnRoute: `inbox:${inboxOf(personOf(CAROL).identity.agreementPublic)}:0x${"11".repeat(16)}`,
        },
        { replyKey: `0x${"22".repeat(32)}` },
        { requestId: `0x${"33".repeat(16)}` },
      ])
        deliver(
          reseal(toBob.sealed, BOB, (plain) => ({
            ...plain,
            context: { ...plain.context, ...edit },
          })),
        );
      deliver(
        reseal(toBob.sealed, BOB, (plain) => ({
          ...plain,
          payload: `0x${"44".repeat(8)}`,
        })),
      );
      // Alice's request to Carol, passed on to Bob.
      deliver(reseal(toCarol.sealed, CAROL, (plain) => plain));
      // Carol's own request, claiming to be from Alice.
      const forged = sealRequest(
        carol.auth,
        bobPerson,
        { ...toBob.context, purpose: "Send me money." },
        toBob.payload,
        cryptoRandom,
      );
      deliver(
        reseal(forged.sealed, BOB, (plain) => ({
          ...plain,
          context: {
            ...plain.context,
            from: personOf(ALICE).identity.signingPublic,
          },
        })),
      );
      // Alice's request to Bob with a byte changed.
      deliver(
        `${toBob.sealed.slice(0, -2)}${toBob.sealed.endsWith("00") ? "01" : "00"}`,
      );
      // Authentic from Alice, but its return route does not reach her: Carol's
      // inbox, or not a route of this delivery at all.
      for (const returnRoute of [
        `inbox:${inboxOf(personOf(CAROL).identity.agreementPublic)}:0x${"55".repeat(16)}`,
        `nowhere:0x${"66".repeat(16)}`,
      ])
        deliver(
          sealRequest(
            alice.auth,
            bobPerson,
            {
              ...toBob.context,
              requestId: toHex(cryptoRandom(16)),
              returnRoute,
            },
            toBob.payload,
            cryptoRandom,
          ).sealed,
        );

      expect(bob.notifications.list().map((row) => row.purpose)).toEqual([
        "Chat?",
      ]);
      expect(bob.log.filter((line) => line.startsWith("Ignored"))).toEqual([
        ...Array(6).fill("Ignored a request: its evidence does not check."),
        "Ignored a request: it is for another identity.",
        "Ignored a request: its evidence does not check.",
        "Ignored a request: not sealed to this host.",
        ...Array(2).fill(
          expect.stringMatching(
            /^Ignored request [0-9a-f]{8}: its return route does not reach Alice\.$/,
          ),
        ),
      ]);
    });

    test("decline is silent, and a dismissed request stays dismissed across reposts and restarts", async () => {
      const { clock, alice, bob } = world();
      const sent = await invite(alice, BOB);
      bob.notifications.dismiss(newest(bob));
      clock.advance(2 * REPOST_INTERVAL_MS);
      bob.stop();
      bob.start();
      clock.advance(REPOST_INTERVAL_MS);
      expect(bob.notifications.listed()).toEqual([]);
      expect(bob.notifications.list()).toHaveLength(1);
      expect(
        bob.messages.filter((message) => message.kind !== "replied"),
      ).toEqual([]);
      expect(bob.writes).toEqual([]);
      expect(alice.status(sent.handle)).toBe("waiting");
      expect(alice.responses(sent.handle)).toEqual([]);
    });

    test("the recipient's product checks the payload before anything is sent; a refusal keeps the request", async () => {
      const { alice, bob } = world();
      await invite(alice, BOB, { payload: `0x${"ab".repeat(64)}` });
      const { user, result } = await accept(bob);
      expect(result).toBeNull();
      await expect(user).rejects.toThrow("Unusable.");
      expect(bob.notifications.get(newest(bob))!.state.tag).toBe("pending");
      expect(bob.writes).toEqual([]);
    });

    test("cancel stops the reposts; a late reply is ignored and never reaches the product", async () => {
      const { clock, alice, bob } = world();
      const sent = await invite(alice, BOB);
      const cancelled = ok(
        await alice.call({ call: "cancel", handle: sent.handle }),
      );
      expect(cancelled.request!.status).toBe("cancelled");
      const posted = deliveryWrites(alice).length;
      clock.advance(3 * REPOST_INTERVAL_MS);
      expect(deliveryWrites(alice).length).toBe(posted);

      // Bob never learns of the cancel; his reply finds nothing.
      await accept(bob);
      expect(alice.responses(sent.handle)).toEqual([]);
      expect(
        await alice.call({ call: "complete", handle: sent.handle }),
      ).toEqual({
        ok: false,
        reason: "Refused",
        detail: "This request is cancelled.",
      });
      expect(await alice.call({ call: "cancel", handle: sent.handle })).toEqual(
        {
          ok: false,
          reason: "Refused",
          detail: "This request is cancelled.",
        },
      );
    });

    test("the deadline stops both sides; posts lapse on their own, sooner than the deadline", async () => {
      const { clock, alice, bob, shared } = world();
      const sent = await invite(alice, BOB, { ttlMs: 20 * MINUTE });
      // Bob's host was offline longer than a post lives; Alice's reposts still reach it.
      bob.stop();
      clock.advance(POST_LIFETIME_MS + 2 * REPOST_INTERVAL_MS);
      bob.start();
      expect(bob.notifications.list()).toHaveLength(1);
      const { result } = await accept(bob);
      expect(result).toEqual({ ok: true });
      alice.stop();
      clock.advance(20 * MINUTE);
      // Reposts never move the deadline the request was authenticated with.
      expect(alice.outgoing(sent.handle).context.deadline).toBe(
        START + 20 * MINUTE,
      );
      alice.start();
      expect(alice.status(sent.handle)).toBe("expired");
      const writes = [deliveryWrites(alice).length, deliveryWrites(bob).length];
      clock.advance(3 * REPOST_INTERVAL_MS);
      expect([
        deliveryWrites(alice).length,
        deliveryWrites(bob).length,
      ]).toEqual(writes);
      expect(
        await alice.call({ call: "complete", handle: sent.handle }),
      ).toEqual({
        ok: false,
        reason: "Refused",
        detail: "This request is expired.",
      });
      clock.advance(POST_LIFETIME_MS);
      expect([...shared.items.keys()]).toEqual([]);
    });

    test("a request that arrives after its deadline is not kept, and Accept after it is refused", async () => {
      const { clock, alice, bob } = world();
      bob.stop();
      await invite(alice, BOB, { ttlMs: 2 * MINUTE });
      clock.advance(2 * MINUTE);
      bob.start();
      expect(bob.notifications.list()).toEqual([]);

      await invite(alice, BOB, { ttlMs: 2 * MINUTE });
      clock.advance(2 * MINUTE);
      await expect(bob.service.requestAccept(newest(bob))).rejects.toThrow(
        "This request expired.",
      );
    });

    test("repeats have one effect: reposts, a second Accept, a second reply, the same reply twice", async () => {
      const { clock, alice, bob } = world();
      const sent = await invite(alice, BOB);
      clock.advance(3 * REPOST_INTERVAL_MS);
      expect(bob.notifications.list()).toHaveLength(1);
      const handle = newest(bob);

      const first = bob.service.requestAccept(handle);
      expect(bob.service.requestAccept(handle)).toBe(first);
      expect(bob.intents()).toHaveLength(1);
      const made = setup.respond(bob.intents()[0]!.payload, cryptoRandom)!;
      ok(
        await bob.call({
          call: "reply",
          handle,
          payload: made.response,
          open: false,
        }),
      );
      expect(
        await bob.call({
          call: "reply",
          handle,
          payload: made.response,
          open: false,
        }),
      ).toEqual({
        ok: false,
        reason: "Refused",
        detail: "The user did not choose Accept for it.",
      });
      await expect(bob.service.requestAccept(handle)).rejects.toThrow(
        "This request was already accepted.",
      );

      // Bob's host posts its reply again; Alice's product hears it once.
      clock.advance(3 * REPOST_INTERVAL_MS);
      expect(alice.responses(sent.handle)).toEqual([made.response]);
      ok(await alice.call({ call: "complete", handle: sent.handle }));
      expect(
        await alice.call({ call: "complete", handle: sent.handle }),
      ).toEqual({
        ok: false,
        reason: "Refused",
        detail: "This request is completed.",
      });
    });

    test("restarts keep the state: the sender reposts and hands a waiting reply again; the recipient reposts its reply", async () => {
      const { clock, alice, bob } = world();
      const sent = await invite(alice, BOB);
      const { made } = await accept(bob);
      alice.stop();
      bob.stop();
      alice.messages.length = 0;
      const writes = [deliveryWrites(alice).length, deliveryWrites(bob).length];
      clock.advance(REPOST_INTERVAL_MS);
      expect([
        deliveryWrites(alice).length,
        deliveryWrites(bob).length,
      ]).toEqual(writes);
      alice.start();
      bob.start();
      expect(deliveryWrites(alice).length).toBe(writes[0]! + 1);
      expect(deliveryWrites(bob).length).toBe(writes[1]! + 1);
      expect(alice.responses(sent.handle)).toEqual([made!.response]);
      expect(
        bob.messages.filter((message) => message.kind === "replied"),
      ).toEqual([
        {
          kind: "replied",
          request: expect.objectContaining({
            handle: newest(bob),
            repliedAt: START,
          }),
        },
      ]);
    });

    test("the largest payload and purpose the API takes go there and back", async () => {
      const { alice, bob } = world();
      const largest = `0x${"ab".repeat(PAYLOAD_MAX_BYTES)}`;
      alice.pickAnswer = BOB;
      const picked = ok(await alice.call({ call: "pickContact" }));
      const sent = ok(
        await alice.call({
          call: "request",
          request: {
            contact: (picked.outcome as { value: { handle: ContactHandle } })
              .value.handle,
            purpose: "\u0007".repeat(PURPOSE_MAX_CHARS),
            ttlMs: HOUR,
            payload: largest,
          },
        }),
      );
      const handle = newest(bob);
      const user = bob.service.requestAccept(handle);
      expect(bob.intents()[0]!.payload).toBe(largest);
      const reply = `0x${"cd".repeat(PAYLOAD_MAX_BYTES)}`;
      ok(
        await bob.call({ call: "reply", handle, payload: reply, open: false }),
      );
      await user;
      expect(alice.responses(sent.request!.handle)).toEqual([reply]);
    });

    test("handles are the host's own: one side's handle does nothing on the other, and a reply needs the user's Accept", async () => {
      const { alice, bob, carol } = world();
      const sent = await invite(alice, BOB);
      const incoming = newest(bob);
      expect(
        await bob.call({
          call: "reply",
          handle: incoming,
          payload: "0x01",
          open: false,
        }),
      ).toEqual({
        ok: false,
        reason: "Refused",
        detail: "The user did not choose Accept for it.",
      });
      expect(
        await alice.call({
          call: "reply",
          handle: sent.handle,
          payload: "0x01",
          open: false,
        }),
      ).toEqual({
        ok: false,
        reason: "Refused",
        detail: "The user did not choose Accept for it.",
      });
      for (const [actor, handle] of [
        [bob, incoming],
        [carol, sent.handle],
        [bob, sent.handle],
      ] as const)
        expect(await actor.call({ call: "complete", handle })).toEqual({
          ok: false,
          reason: "Unknown",
          detail: "No sent request has this handle.",
        });
      await expect(alice.service.requestAccept(sent.handle)).rejects.toThrow(
        "This is not a request to you.",
      );
      expect(bob.writes).toEqual([]);
    });

    test("the sender asks its user, and a product cannot send to a handle the host does not know", async () => {
      const { alice, bob } = world();
      alice.confirm = false;
      alice.pickAnswer = BOB;
      const picked = ok(await alice.call({ call: "pickContact" }));
      const handle = (picked.outcome as { value: { handle: ContactHandle } })
        .value.handle;
      expect(handle.bytes).toMatch(/^0x[0-9a-f]{64}$/);
      alice.pickAnswer = null;
      expect(await alice.call({ call: "pickContact" })).toEqual({
        ok: true,
        outcome: { tag: "Dismissed" },
      });
      const send = (contact: ContactHandle) =>
        alice.call({
          call: "request",
          request: { contact, purpose: "Chat?", ttlMs: HOUR, payload: "0x01" },
        });
      expect(await send(handle)).toEqual({
        ok: false,
        reason: "Dismissed",
        detail: "The user did not send it.",
      });
      expect(
        await send({ bytes: `0x${"00".repeat(32)}` } as ContactHandle),
      ).toEqual({
        ok: false,
        reason: "Unknown",
        detail: "This contact names nobody here.",
      });
      expect(alice.writes).toEqual([]);
      expect(bob.notifications.list()).toEqual([]);
    });
  });
