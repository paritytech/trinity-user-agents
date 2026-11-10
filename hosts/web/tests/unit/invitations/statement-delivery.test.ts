import { asProductId } from "@parity/truapi";
import { beforeEach, describe, expect, test } from "bun:test";
import { bytesToHex } from "@parity/truapi/scale";
import type { NotificationClaim } from "@parity/truapi-host/web";
import { equalBytes } from "../../../bridges/primitives/codec.js";
import type { StatementStoreLike } from "../../../src/platform/statement-store.js";
import { decodeStatement } from "../../../bridges/primitives/statement.js";
import {
  type CallResult,
  type HostMessage,
} from "../../../bridges/host-requests/contract.js";
import {
  InvitationService,
  type ProductPort,
} from "../../../src/invitations/invitation-service.js";
import {
  DAY_INBOX_ROUTE,
  NOTIFICATION_DATA_MAX_BYTES,
  RETRY_MS,
  StatementStoreDelivery,
  inboxTopic,
  periodOf,
} from "../../../src/invitations/statement-delivery.js";
import type { InvitationEvent } from "../../../src/invitations/observer.js";
import { cryptoRandom, toHex } from "../../../bridges/primitives/bytes.js";
import { WalletIdentityAuth } from "../../../src/invitations/wallet-auth.js";
import { NotificationStore } from "../../../src/notifications/notifications.js";
import { FixtureContacts } from "../../../fixtures/invitations/fixture-contacts.js";
import { simpleText } from "../../../apps/contact-chat/simple-text.js";
import {
  TEST_USERS,
  personOf as personOn,
  testUserDirectory,
  testUserEntropy,
  testUserKeys,
  type TestUser,
} from "../../../fixtures/invitations/test-users.js";
import { MemoryStatementStore } from "../../support/statement-memory.js";
import { SUFFIX, hostContactsOf, indexed } from "../../support/host-handles.js";
import { MemoryStorage, settle } from "../../support/test-support.js";
import type { AllowanceGuard } from "../../../src/platform/allowance-usable.js";
import { usable } from "../../support/allowance.js";

/**
 * Two or three users' hosts over one in-memory Statement Store, with the
 * real request service, sealing and authentication, and the network
 * delivery. The chain is a stand-in that registers notification accounts as
 * the runtime does: once per account, only for today, and the store takes a
 * statement only from a registered account, as the node checks allowance.
 */

const NETWORK = "testnet";
const PRODUCT = { id: asProductId("example-chat.test"), name: "Example Chat" };
const DAY = 86_400_000;
const [ALICE, BOB, CAROL] = TEST_USERS as [TestUser, TestUser, TestUser];
const personOf = (user: TestUser) =>
  personOn(user, SUFFIX, testUserKeys(user, SUFFIX).chatPublic);

class Clock {
  at = Date.UTC(2026, 9, 21, 12, 0, 0);
  private tasks = new Map<number, { due: number; task: () => void }>();
  private next = 0;
  readonly now = (): number => this.at;
  readonly schedule = (task: () => void, delayMs: number): (() => void) => {
    const id = this.next++;
    this.tasks.set(id, { due: this.at + delayMs, task });
    return () => this.tasks.delete(id);
  };
  async advance(ms: number): Promise<void> {
    const end = this.at + ms;
    await settle();
    for (;;) {
      const due = [...this.tasks.entries()]
        .filter(([, entry]) => entry.due <= end)
        .sort(([, left], [, right]) => left.due - right.due)[0];
      if (due === undefined) break;
      this.tasks.delete(due[0]);
      this.at = due[1].due;
      due[1].task();
      await settle();
    }
    this.at = end;
    await settle();
  }
}

/** The runtime's notification registrations and the node's allowance check. */
class Chain {
  readonly memory = new MemoryStatementStore();
  /** Registered account, as hex, to the period it holds. */
  readonly registered = new Map<string, number>();
  readonly claims: { account: string; submit: boolean }[] = [];
  /** When set, every claim fails with it, as when no slot is free. */
  refusal: string | null = null;
  /** Where the guard lets an account post, and why its claim did not become final behind a best-block post, or null. */
  usableAt: "best" | "finalized" = "finalized";
  notFinal: string | null = null;
  checkedNode = true;
  /** When set, a guard asked for the finalized head fails with it. */
  finalRefusal: string | null = null;
  /** When set, the guard answers only once it settles. */
  gate: Promise<void> | null = null;
  /** What the guard was asked: the account, and the finalized head when asked for it. */
  readonly guarded: string[] = [];
  /** The next submits of a registered account that the node answers `noAllowance`, as before it saw the claim. */
  lagging = 0;
  /** Every statement submitted, in order, `noAllowance` answers included. */
  readonly submits: string[] = [];

  constructor(private readonly clock: Clock) {}

  readonly claim = async (
    account: Uint8Array,
    submit: boolean,
  ): Promise<NotificationClaim> => {
    const period = periodOf(this.clock.now());
    const key = toHex(account);
    this.claims.push({ account: key, submit });
    const held = this.registered.get(key);
    if (held !== undefined) {
      if (held !== period) throw new Error("account already registered");
      return { period, outcome: "alreadyRegistered", seq: 0 };
    }
    if (this.refusal !== null) throw new Error(this.refusal);
    const seq = this.registered.size;
    if (!submit)
      return { period, outcome: "validated", seq, collection: "LitePeople" };
    this.registered.set(key, period);
    return {
      period,
      outcome: "registered",
      seq,
      collection: "LitePeople",
      blockHash: "0xb10c",
    };
  };

  readonly guard: AllowanceGuard = async (claim, request = {}) => {
    this.guarded.push(
      request.wait === undefined
        ? claim.account
        : `${claim.account} at the finalized head`,
    );
    await this.gate;
    if (request.wait === "finalized" && this.finalRefusal !== null)
      throw new Error(this.finalRefusal);
    return usable(
      claim,
      request.wait ?? this.usableAt,
      Promise.resolve(this.notFinal),
    );
  };

  store(): StatementStoreLike {
    const inner = this.memory.connect();
    return {
      ...inner,
      supportsBestBlockPosting: async () => this.checkedNode,
      submit: async (encoded) => {
        this.submits.push(toHex(encoded));
        const signer = toHex(decodeStatement(encoded).signer);
        if (this.registered.get(signer) !== periodOf(this.clock.now()))
          return { status: "rejected", reason: "noAllowance" };
        if (this.lagging > 0) {
          this.lagging -= 1;
          return { status: "rejected", reason: "noAllowance" };
        }
        if (this.memory.submitted.some((held) => equalBytes(held, encoded)))
          return { status: "known" };
        return inner.submit(encoded);
      },
    };
  }

  /** Every statement the store took, decoded. */
  posted() {
    return this.memory.submitted.map(decodeStatement);
  }
}

class Actor {
  readonly storage = new MemoryStorage();
  readonly notifications: NotificationStore;
  readonly messages: HostMessage[] = [];
  readonly log: string[] = [];
  /** What the delivery told the developer tools. */
  readonly observed: InvitationEvent[] = [];
  readonly auth: WalletIdentityAuth;
  readonly contacts: FixtureContacts;
  service!: InvitationService;
  private handler: ((value: unknown) => void) | null = null;
  private readonly pending = new Map<number, (result: CallResult) => void>();
  private nextId = 1;

  constructor(
    readonly user: TestUser,
    readonly chain: Chain,
    readonly clock: Clock,
    readonly submit = true,
  ) {
    this.notifications = new NotificationStore(this.storage, clock.now);
    this.notifications.setScope(`${user.id}:notifications`);
    this.auth = new WalletIdentityAuth(testUserKeys(user, SUFFIX));
    this.contacts = new FixtureContacts(user, hostContactsOf(user), {
      networkSuffix: SUFFIX,
      directory: testUserDirectory(SUFFIX),
    });
    void this.contacts.refresh();
    this.start();
  }

  start(): void {
    this.service = new InvitationService({
      product: PRODUCT,
      auth: this.auth,
      contacts: this.contacts,
      notifications: this.notifications,
      storage: this.storage,
      keys: { sent: "sent", received: "received" },
      delivery: new StatementStoreDelivery({
        network: NETWORK,
        entropy: testUserEntropy(this.user),
        openStore: async () => this.chain.store(),
        claim: this.chain.claim,
        guard: this.chain.guard,
        submit: this.submit,
        now: this.clock.now,
        schedule: this.clock.schedule,
        log: (line) => this.log.push(line),
        observe: (event) => void this.observed.push(event),
      }),
      now: this.clock.now,
      schedule: this.clock.schedule,
      random: cryptoRandom,
      confirmSend: async () => true,
      showProduct: () => {},
      log: (line) => this.log.push(line),
    });
    this.service.start();
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

  call(fields: Record<string, unknown>): Promise<CallResult> {
    const id = this.nextId++;
    return new Promise((resolve) => {
      this.pending.set(id, resolve);
      this.handler?.({ id, ...fields });
    });
  }

  responses(handle: string): string[] {
    return this.messages.flatMap((message) =>
      message.kind === "response" && message.handle === handle
        ? [message.payload]
        : [],
    );
  }

  paused(handle: string): string | null {
    return this.service.outbox.get(handle)!.paused;
  }
}

function ok(result: CallResult): Extract<CallResult, { ok: true }> {
  if (!result.ok) throw new Error(`${result.reason}: ${result.detail}`);
  return result;
}

async function invite(
  from: Actor,
  to: TestUser,
  payload = simpleText.invite(toHex(cryptoRandom(32)), cryptoRandom).payload,
): Promise<string> {
  await indexed();
  const contact = from.contacts.handleFor(personOf(to))!;
  const sent = ok(
    await from.call({
      call: "request",
      request: { contact, purpose: "Chat?", payload },
    }),
  );
  await settle();
  return sent.request!.handle;
}

async function accept(bob: Actor): Promise<void> {
  const handle = bob.notifications.list()[0]!.id;
  const user = bob.service.requestAccept(handle);
  user.catch(() => {});
  const intent = bob.messages.findLast(
    (message) => message.kind === "acceptIntent",
  );
  if (intent?.kind !== "acceptIntent") throw new Error("no intent");
  const made = simpleText.respond(
    intent.request.payload,
    toHex(cryptoRandom(32)),
    cryptoRandom,
  )!;
  ok(
    await bob.call({
      call: "reply",
      handle,
      payload: made.response,
      open: false,
    }),
  );
  await settle();
}

let clock: Clock;
let chain: Chain;

beforeEach(() => {
  clock = new Clock();
  chain = new Chain(clock);
});

describe("the network delivery", () => {
  test("a request and its reply travel on the day inbox, each signed by a claimed notification account", async () => {
    const alice = new Actor(ALICE, chain, clock);
    const bob = new Actor(BOB, chain, clock);
    const handle = await invite(alice, BOB);
    await accept(bob);

    const identities: string[] = [ALICE, BOB].map((user) =>
      bytesToHex(testUserKeys(user, SUFFIX).identity),
    );
    const posted = chain.posted();
    expect({
      replies: alice.responses(handle).length,
      posts: posted.length,
      everyTopicIsToday: posted.every(
        (statement) =>
          statement.topics.length === 1 &&
          equalBytes(
            statement.topics[0]!,
            inboxTopic(NETWORK, periodOf(clock.now())),
          ),
      ),
      everySignerClaimed: posted.every((statement) =>
        chain.registered.has(toHex(statement.signer)),
      ),
      noIdentitySigns: posted.every(
        (statement) => !identities.includes(toHex(statement.signer)),
      ),
      fitsANotification: posted.every(
        (statement) => statement.data.length <= NOTIFICATION_DATA_MAX_BYTES,
      ),
    }).toEqual({
      replies: 1,
      posts: 2,
      everyTopicIsToday: true,
      everySignerClaimed: true,
      noIdentitySigns: true,
      fitsANotification: true,
    });
  });

  test("on the shared inbox, a reply reaches only the request whose key opens it, and only its recipient sees a request", async () => {
    const alice = new Actor(ALICE, chain, clock);
    const bob = new Actor(BOB, chain, clock);
    const carol = new Actor(CAROL, chain, clock);
    const toBob = await invite(alice, BOB);
    const toCarol = await invite(alice, CAROL);
    await accept(bob);

    expect({
      bob: bob.notifications.list().length,
      carol: carol.notifications.list().length,
      fromBob: alice.responses(toBob).length,
      toCarolStillWaiting: alice.responses(toCarol).length,
      quiet: [...alice.log, ...bob.log, ...carol.log].filter((line) =>
        line.startsWith("Ignored"),
      ),
    }).toEqual({
      bob: 1,
      carol: 1,
      fromBob: 1,
      toCarolStillWaiting: 0,
      quiet: [],
    });
  });

  test("a pending request is posted again each day with a new account; a cancelled one is not, and its reply does not open", async () => {
    const alice = new Actor(ALICE, chain, clock);
    const kept = await invite(alice, BOB);
    const cancelled = await invite(alice, CAROL);
    ok(await alice.call({ call: "cancel", handle: cancelled }));
    const firstDay = chain.posted().length;
    await clock.advance(DAY);
    const nextDay = chain.posted().slice(firstDay);

    const bob = new Actor(BOB, chain, clock);
    const carol = new Actor(CAROL, chain, clock);
    await settle();
    await accept(carol);
    expect({
      firstDay,
      nextDay: nextDay.length,
      newAccount: !chain
        .posted()
        .slice(0, firstDay)
        .some((statement) => equalBytes(statement.signer, nextDay[0]!.signer)),
      bobSees: bob.notifications.list().length,
      keptStatus: alice.service.outbox.status(alice.service.outbox.get(kept)!),
      cancelledReply: alice.responses(cancelled),
      refused: alice.log.filter((line) => line.startsWith("Ignored")),
    }).toEqual({
      firstDay: 2,
      nextDay: 1,
      newAccount: true,
      bobSees: 1,
      keptStatus: "waiting",
      cancelledReply: [],
      // Its reply key's secret went with the cancel: the reply does not open.
      refused: [],
    });
  });

  test("a host that runs throughout answers the request posted again on a later day with its reply again", async () => {
    const alice = new Actor(ALICE, chain, clock);
    const bob = new Actor(BOB, chain, clock);
    const handle = await invite(alice, BOB);
    alice.service.stop();
    await accept(bob);
    // Alice missed the reply, and the store dropped every statement since.
    chain.memory.statements = [];
    await clock.advance(2 * DAY);
    alice.start();
    await settle();

    expect({
      responses: alice.responses(handle).length,
      resent: bob.log.filter((line) => line.includes("came again")).length,
    }).toEqual({ responses: 1, resent: 1 });
  });

  test("a request with no free slot is paused, kept, and posted once a claim succeeds", async () => {
    const alice = new Actor(ALICE, chain, clock);
    chain.refusal = "no free notification slot in period 20747 (max 9)";
    const handle = await invite(alice, BOB);
    const paused = alice.paused(handle);
    chain.refusal = null;
    await clock.advance(RETRY_MS);

    expect({
      paused,
      after: alice.paused(handle),
      status: alice.service.outbox.status(alice.service.outbox.get(handle)!),
      posts: chain.posted().length,
    }).toEqual({
      paused: "no free notification slot in period 20747 (max 9)",
      after: null,
      status: "waiting",
      posts: 1,
    });
    // The developer tools heard which account each try used, and the problem and its end.
    const [posted] = chain.posted();
    expect(
      alice.observed.map((event) =>
        event.type === "delivery-account"
          ? `${event.type} ${event.account === toHex(posted!.signer) ? "posted" : "other"}`
          : `${event.type} ${event.type === "delivery-problem" ? event.problem : ""}`,
      ),
    ).toEqual([
      "delivery-account posted",
      "delivery-problem no free notification slot in period 20747 (max 9)",
      "delivery-account posted",
      "delivery-problem null",
    ]);
  });

  test("posting again on the same day claims nothing new", async () => {
    const alice = new Actor(ALICE, chain, clock);
    await invite(alice, BOB);
    alice.service.stop();
    alice.start();
    await settle();

    expect(chain.claims.map((claim) => claim.account)).toEqual([
      chain.claims[0]!.account,
      chain.claims[0]!.account,
    ]);
    expect(chain.registered.size).toBe(1);
  });

  test("validate only submits nothing and says so", async () => {
    const alice = new Actor(ALICE, chain, clock, false);
    const handle = await invite(alice, BOB);

    expect({
      claims: chain.claims.map((claim) => claim.submit),
      registered: chain.registered.size,
      posts: chain.posted().length,
      paused: alice.paused(handle),
    }).toEqual({
      claims: [false],
      registered: 0,
      posts: 0,
      paused:
        "not posted: claims are only validated. Today's claim would take slot 0 of LitePeople.",
    });
  });

  test("a request or reply that seals larger than one notification is refused before anything is asked or kept", async () => {
    const alice = new Actor(ALICE, chain, clock);
    const bob = new Actor(BOB, chain, clock);
    await indexed();
    const large = `0x${"ab".repeat(6_000)}`;
    const request = await alice.call({
      call: "request",
      request: {
        contact: alice.contacts.handleFor(personOf(BOB))!,
        purpose: "Chat?",
        payload: large,
      },
    });
    await invite(alice, BOB);
    const handle = bob.notifications.list()[0]!.id;
    bob.service.requestAccept(handle).catch(() => {});
    const reply = await bob.call({
      call: "reply",
      handle,
      payload: large,
      open: false,
    });
    const tooLarge = (kind: string) =>
      expect.stringMatching(
        new RegExp(
          `^The ${kind} is \\d+ bytes once sealed, and its delivery carries at most ${NOTIFICATION_DATA_MAX_BYTES}\\. Send a smaller payload\\.$`,
        ),
      );

    expect({
      request,
      reply,
      sent: alice.service.outbox.list().length,
      stillPending: bob.notifications.get(handle)?.state.tag,
      claims: chain.claims.length,
    }).toEqual({
      request: { ok: false, reason: "Invalid", detail: tooLarge("request") },
      reply: { ok: false, reason: "Invalid", detail: tooLarge("reply") },
      sent: 1,
      stillPending: "pending",
      claims: 1,
    });
  });

  test("every request names the day inbox as its return route", async () => {
    const alice = new Actor(ALICE, chain, clock);
    const handle = await invite(alice, BOB);

    expect(alice.service.outbox.get(handle)!.context.returnRoute).toBe(
      DAY_INBOX_ROUTE,
    );
  });
});

describe("the network delivery's allowance wait", () => {
  const signers = () =>
    new Set(chain.posted().map((statement) => toHex(statement.signer)));

  test("an unchecked submit connection forces finalized posting even when a different read connection supports best", async () => {
    chain.checkedNode = false;
    chain.usableAt = "best";
    const alice = new Actor(ALICE, chain, clock);
    await invite(alice, BOB);
    expect(
      chain.guarded.map((entry) => entry.endsWith("finalized head")),
    ).toEqual([true]);
    expect(chain.submits).toHaveLength(1);
    alice.service.stop();
  });

  test("one replacement preserves the payload and receiver dedupe, without spending a second slot", async () => {
    chain.usableAt = "best";
    chain.notFinal = "claim orphaned";
    let received = 0;
    const delivery = new StatementStoreDelivery({
      network: NETWORK,
      entropy: testUserEntropy(ALICE),
      openStore: async () => chain.store(),
      claim: chain.claim,
      guard: chain.guard,
      submit: true,
      now: clock.now,
      schedule: clock.schedule,
      log: () => {},
    });
    delivery.listen({
      request: () => {
        received += 1;
      },
      reply: () => {},
      posting: () => {},
    });
    delivery.postRequest(personOf(BOB).identity, "logical-request", "0x010203");
    await settle();
    expect([chain.submits.length, chain.claims.length, received]).toEqual([
      2, 1, 1,
    ]);
    const [first, next] = chain.posted();
    expect({
      ...next!,
      expiry: first!.expiry,
      signature: first!.signature,
    }).toEqual(first!);
    delivery.close();
  });

  test("duplicate publish calls share one attempt and an older day's final callback cannot downgrade the new post", async () => {
    const finals: ((why: string | null) => void)[] = [];
    const problems: (string | null)[] = [];
    const delivery = new StatementStoreDelivery({
      network: NETWORK,
      entropy: testUserEntropy(ALICE),
      openStore: async () => chain.store(),
      claim: chain.claim,
      guard: async (claim) =>
        usable(claim, "best", new Promise((resolve) => finals.push(resolve))),
      submit: true,
      now: clock.now,
      schedule: clock.schedule,
      log: () => {},
    });
    delivery.listen({
      request: () => {},
      reply: () => {},
      posting: (_id, problem) => problems.push(problem),
    });
    delivery.postRequest(personOf(BOB).identity, "logical-request", "0x010203");
    delivery.postRequest(personOf(BOB).identity, "logical-request", "0x010203");
    await settle();
    expect(chain.claims).toHaveLength(1);
    await clock.advance(DAY);
    expect(chain.claims).toHaveLength(2);
    finals[0]!("old claim orphaned");
    await settle();
    expect(problems).toEqual([]);
    expect(chain.submits).toHaveLength(2);
    finals[1]!(null);
    delivery.close();
  });

  // On a best-block network the account posts before its claim is final; a
  // node behind the claim's block answers noAllowance until it sees it.
  test("posts once the guard lets the account, and sends the same bytes again while the node does not see the claim", async () => {
    chain.usableAt = "best";
    chain.lagging = 2;
    const alice = new Actor(ALICE, chain, clock);
    const handle = await invite(alice, BOB);
    await clock.advance(4_000);

    expect({
      guarded: chain.guarded.length,
      submits: chain.submits.length,
      sameBytes: new Set(chain.submits).size,
      posts: chain.posted().length,
      paused: alice.paused(handle),
    }).toEqual({
      guarded: 1,
      submits: 3,
      sameBytes: 1,
      posts: 1,
      paused: null,
    });
  });

  test("an unconfirmed post can recover later against final allowance with one fresh outer statement and no new claim", async () => {
    chain.usableAt = "best";
    chain.notFinal = "its allowance was not final after 180 s";
    chain.finalRefusal = "the People chain could not be read";
    const alice = new Actor(ALICE, chain, clock);
    const handle = await invite(alice, BOB);
    const paused = alice.paused(handle);
    chain.notFinal = null;
    chain.finalRefusal = null;
    await clock.advance(RETRY_MS);

    expect({
      paused,
      after: alice.paused(handle),
      claims: chain.claims.map((claim) => claim.account),
      guarded: chain.guarded.map((entry) => entry.endsWith("finalized head")),
      submits: chain.submits.length,
      sameBytes: new Set(chain.submits).size,
      signers: signers().size,
    }).toEqual({
      paused:
        "posted at a best block, but its allowance was not final after 180 s; the People chain could not be read.",
      after: null,
      claims: [chain.claims[0]!.account],
      guarded: [false, true, true],
      submits: 2,
      sameBytes: 2,
      signers: 1,
    });
    const [first, replacement] = chain.posted();
    expect({
      ...replacement!,
      expiry: first!.expiry,
      signature: first!.signature,
    }).toEqual(first!);
    expect(replacement!.expiry).toBe(first!.expiry + 1n);
  });

  test("a request withdrawn while its account waits is never submitted", async () => {
    let open!: () => void;
    chain.gate = new Promise((resolve) => (open = resolve));
    const alice = new Actor(ALICE, chain, clock);
    const handle = await invite(alice, BOB);
    ok(await alice.call({ call: "cancel", handle }));
    open();
    await settle();

    expect({ guarded: chain.guarded.length, submits: chain.submits }).toEqual({
      guarded: 1,
      submits: [],
    });
  });
});
