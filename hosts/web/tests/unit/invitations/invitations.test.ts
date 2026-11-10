import { asProductId } from "@parity/truapi";
import { describe, expect, spyOn, test } from "bun:test";
import * as sealingModule from "../../../src/invitations/sealing.js";
import { ChatKeys } from "../../../src/invitations/chat-keys.js";
import { WalletIdentityAuth } from "../../../src/invitations/wallet-auth.js";
import { NotificationStore } from "../../../src/notifications/notifications.js";
import { MemoryStorage, settle } from "../../support/test-support.js";
import {
  Outbox,
  type OutgoingRequest,
} from "../../../src/invitations/outbox.js";
import {
  REPLY_SEALS,
  REQUEST_SEAL,
  requestDigest,
  sealReply,
  sealRequest,
  type RequestContext,
} from "../../../src/invitations/binding.js";
import {
  isProductHello,
  PAYLOAD_MAX_BYTES,
  PRODUCT_HELLO,
  readProductCall,
  readSendRequest,
  type CallResult,
  type ContactHandle,
  type HostMessage,
} from "../../../bridges/host-requests/contract.js";
import {
  INTENT_TIMEOUT_MS,
  InvitationService,
  type ProductPort,
} from "../../../src/invitations/invitation-service.js";
import type {
  ContactService,
  HostDelivery,
  Person,
} from "../../../src/invitations/ports.js";
import {
  cryptoRandom,
  fromHex,
  toHex,
  utf8,
} from "../../../bridges/primitives/bytes.js";
import {
  digest,
  sealTo,
  sealToXWing,
  x25519Pair,
  xwingPair,
} from "../../../src/invitations/sealing.js";

/**
 * The request service against fakes of its contacts and delivery, with
 * wallet keys of made-up entropy. The two-user flow over the dev fixtures is
 * in `tests/unit/invitations/invitation-flow.test.ts`.
 */

const START = Date.UTC(2026, 9, 21, 12, 0, 0);
const MINUTE = 60_000;
const HOUR = 60 * MINUTE;
const PRODUCT = { id: asProductId("chat.example"), name: "Example Chat" };
const PAYLOAD = `0x${"ab".repeat(64)}`;
const REPLY = `0x${"cd".repeat(64)}`;

const keys = new Map<string, ChatKeys>();
/** Wallet keys of made-up entropy, derived once each. */
const keysOf = (id: string): ChatKeys => {
  let derived = keys.get(id);
  if (derived === undefined) {
    derived = ChatKeys.fromEntropy(digest("test-entropy", id), "test");
    keys.set(id, derived);
  }
  return derived;
};
const person = (id: string, name: string): Person => ({
  id,
  account: toHex(keysOf(id).identity),
  name,
  handle: `${id}.example`,
  identity: {
    signingPublic: toHex(keysOf(id).identity),
    agreementPublic: toHex(keysOf(id).chatPublic),
  },
});
const ADA = person("ada", "Ada");
const LIN = person("lin", "Lin");
const handleOf = (who: Person): ContactHandle =>
  ({ bytes: toHex(digest("test-handle", who.id)) }) as ContactHandle;

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

/**
 * Records what the service asks of its delivery, in order. Its return routes
 * are `route:<n>`, with nothing about the transport in them; it can reach
 * anyone on one, except on the routes in `unreachable`.
 */
class FakeDelivery implements HostDelivery {
  receive: Parameters<HostDelivery["listen"]>[0] | null = null;
  readonly calls: string[] = [];
  readonly unreachable = new Set<string>();
  private routes = 0;
  readonly posts = new Map<string, string>();
  readonly watched = new Set<string>();
  closed = false;
  listen(receiver: Parameters<HostDelivery["listen"]>[0]): void {
    this.receive = receiver;
  }
  postRequest(_to: unknown, id: string, sealed: string): void {
    this.calls.push(`postRequest ${id}`);
    this.posts.set(id, sealed);
  }
  returnRoute(): string {
    this.routes += 1;
    return `route:${this.routes}`;
  }
  isReturnRoute(route: string): boolean {
    return route.startsWith("route:") && !this.unreachable.has(route);
  }
  postReply(route: string, id: string, sealed: string): void {
    this.calls.push(`postReply ${route} ${id}`);
    this.posts.set(id, sealed);
  }
  watch(route: string): void {
    this.calls.push(`watch ${route}`);
    this.watched.add(route);
  }
  unwatch(route: string): void {
    this.calls.push(`unwatch ${route}`);
    this.watched.delete(route);
  }
  withdraw(id: string): void {
    this.calls.push(`withdraw ${id}`);
    this.posts.delete(id);
  }
  close(): void {
    this.closed = true;
  }
}

class FakeProduct implements ProductPort {
  readonly received: HostMessage[] = [];
  closed = false;
  private handler: ((value: unknown) => void) | null = null;
  private next = 1;
  private readonly waiting = new Map<number, (result: CallResult) => void>();
  post(message: HostMessage): void {
    if (this.closed) return;
    this.received.push(structuredClone(message));
    if (message.kind === "result")
      this.waiting.get(message.id)?.(message.result);
  }
  listen(handler: (value: unknown) => void): void {
    this.handler = handler;
  }
  close(): void {
    this.closed = true;
  }
  call(fields: Record<string, unknown>): Promise<CallResult> {
    const id = this.next++;
    return new Promise((resolve) => {
      this.waiting.set(id, resolve);
      this.handler?.({ id, ...fields });
    });
  }
}

/** Whether the host can name people by handle yet, as its handle index answers, and who listens. */
class Naming {
  ready = true;
  readonly listeners = new Set<() => void>();
  set(ready: boolean): void {
    this.ready = ready;
    for (const listener of [...this.listeners]) listener();
  }
}

/** Ada's and Lin's directory: each knows the other. */
function contacts(self: Person, naming: Naming): ContactService {
  const people = [ADA, LIN].filter((other) => other.id !== self.id);
  return {
    resolve: (contact) =>
      people.find((other) => handleOf(other).bytes === contact.bytes) ?? null,
    byIdentity: (key) =>
      people.find((other) => other.identity.signingPublic === key),
    problem: () => null,
    handleFor: (who) => (naming.ready ? handleOf(who) : null),
    handleForIdentity: (key) => {
      const who = people.find((other) => other.identity.signingPublic === key);
      return who === undefined || !naming.ready ? null : handleOf(who);
    },
    onChange: (listener) => {
      naming.listeners.add(listener);
      return () => naming.listeners.delete(listener);
    },
  };
}

function host(
  self: Person,
  options: { storage?: MemoryStorage; clock?: Clock; naming?: Naming } = {},
) {
  const storage = options.storage ?? new MemoryStorage();
  const clock = options.clock ?? new Clock();
  const notifications = new NotificationStore(storage, clock.now);
  notifications.setScope(`${self.id}:notifications`);
  const delivery = new FakeDelivery();
  const shown: number[] = [];
  const service = new InvitationService({
    product: PRODUCT,
    auth: new WalletIdentityAuth(keysOf(self.id)),
    contacts: contacts(self, options.naming ?? new Naming()),
    notifications,
    storage,
    keys: { sent: `${self.id}:sent`, received: `${self.id}:received` },
    delivery,
    now: clock.now,
    schedule: clock.schedule,
    random: cryptoRandom,
    confirmSend: async () => true,
    showProduct: () => shown.push(clock.now()),
    log: () => {},
  });
  const product = new FakeProduct();
  service.attachProduct(product);
  service.start();
  return { service, notifications, delivery, product, storage, clock, shown };
}

/** A request from Ada to Lin, as Ada's host seals it. */
function fromAda(overrides: Partial<RequestContext> = {}) {
  return sealRequest(
    new WalletIdentityAuth(keysOf("ada")),
    LIN,
    {
      product: PRODUCT,
      purpose: "Chat?",
      requestId: toHex(cryptoRandom(16)),
      createdAt: START,
      returnRoute: `route:${toHex(cryptoRandom(8))}`,
      replyKey: toHex(xwingPair(cryptoRandom(32)).public),
      ...overrides,
    },
    PAYLOAD,
    cryptoRandom,
  );
}

/** Lin's host receives a request from Ada; the handle of its row. */
function receive(lin: ReturnType<typeof host>, overrides = {}) {
  const { sealed, context } = fromAda(overrides);
  lin.delivery.receive!.request(sealed);
  const row = lin.notifications.list()[0]!;
  return { handle: row.id, context, sealed };
}

describe("the request service, receiving", () => {
  test("Accept is the user's intent: only the product's reply accepts, and the host posts it on the request's return topic", async () => {
    const lin = host(LIN);
    const { handle, context } = receive(lin);
    expect(lin.notifications.get(handle)!.state.tag).toBe("pending");
    expect(lin.product.received).toEqual([]);

    const accepting = lin.service.requestAccept(handle);
    expect(lin.product.received).toEqual([
      {
        kind: "acceptIntent",
        request: { handle, sender: handleOf(ADA), payload: PAYLOAD },
      },
    ]);
    expect(lin.notifications.get(handle)!.state.tag).toBe("pending");
    const result = await lin.product.call({
      call: "reply",
      handle,
      payload: REPLY,
      open: true,
    });
    await accepting;
    expect(result).toEqual({ ok: true });
    expect(lin.notifications.get(handle)!.state.tag).toBe("accepted");
    expect(lin.shown).toHaveLength(1);
    expect(lin.delivery.calls).toEqual([
      `postReply ${context.returnRoute} ${handle}`,
    ]);
  });

  // A request's key can be the right length and still not sealable to; the
  // product's call must still get its answer.
  test("a reply to a reply key that cannot be sealed to fails as Invalid, and nothing is accepted or sent", async () => {
    const lin = host(LIN);
    const { handle } = receive(lin, { replyKey: `0x${"00".repeat(32)}` });
    const accepting = lin.service.requestAccept(handle);
    accepting.catch(() => {});
    const result = await lin.product.call({
      call: "reply",
      handle,
      payload: REPLY,
      open: false,
    });
    await expect(accepting).rejects.toThrow(
      "The request's reply key cannot be sealed to",
    );
    expect({
      result,
      state: lin.notifications.get(handle)!.state.tag,
      calls: lin.delivery.calls,
    }).toEqual({
      result: {
        ok: false,
        reason: "Invalid",
        detail: expect.stringMatching(
          /^The request's reply key cannot be sealed to: /,
        ),
      },
      state: "pending",
      calls: [],
    });
  });

  test("the user's Accept fails, with a reason and nothing sent, when the product refuses, is silent, or is closed", async () => {
    const lin = host(LIN);
    const handles = [1, 2, 3, 4].map(() => receive(lin).handle);

    const refused = lin.service.requestAccept(handles[0]!);
    await lin.product.call({
      call: "refuse",
      handle: handles[0],
      reason: "Example Chat cannot use this invitation.",
    });
    await expect(refused).rejects.toThrow(
      "Example Chat cannot use this invitation.",
    );

    const silent = lin.service.requestAccept(handles[1]!);
    lin.clock.advance(INTENT_TIMEOUT_MS);
    await expect(silent).rejects.toThrow("Example Chat did not answer.");
    expect(
      await lin.product.call({
        call: "reply",
        handle: handles[1],
        payload: REPLY,
        open: true,
      }),
    ).toEqual({
      ok: false,
      reason: "Refused",
      detail: "The user did not choose Accept for it.",
    });

    const closing = lin.service.requestAccept(handles[2]!);
    lin.service.detachProduct();
    await expect(closing).rejects.toThrow(
      "Example Chat closed before it answered.",
    );
    await expect(lin.service.requestAccept(handles[3]!)).rejects.toThrow(
      "Example Chat is not open.",
    );

    for (const handle of handles)
      expect(lin.notifications.get(handle)!.state.tag).toBe("pending");
    expect(lin.delivery.calls).toEqual([]);
    expect(lin.shown).toEqual([]);

    // After a failure the user can try again.
    lin.product.closed = false;
    lin.service.attachProduct(lin.product);
    const retry = lin.service.requestAccept(handles[0]!);
    await lin.product.call({
      call: "reply",
      handle: handles[0],
      payload: REPLY,
      open: false,
    });
    await retry;
    expect(lin.notifications.get(handles[0]!)!.state.tag).toBe("accepted");
  });

  test("reading the lists again after another tab wrote them keeps the product's port and an Accept under way", async () => {
    const lin = host(LIN);
    const { handle } = receive(lin);
    const accepting = lin.service.requestAccept(handle);
    lin.service.reload();
    expect(lin.product.closed).toBe(false);
    expect(
      await lin.product.call({
        call: "reply",
        handle,
        payload: REPLY,
        open: true,
      }),
    ).toEqual({ ok: true });
    await accepting;
  });

  test("opening a replied request asks the product, which shows itself only when asked", async () => {
    const lin = host(LIN);
    const { handle } = receive(lin);
    const accepting = lin.service.requestAccept(handle);
    await lin.product.call({
      call: "reply",
      handle,
      payload: REPLY,
      open: false,
    });
    await accepting;
    expect(lin.shown).toEqual([]);
    await lin.product.call({ call: "open", handle });
    expect(lin.shown).toEqual([]);
    lin.service.requestOpen(handle);
    expect(lin.product.received.at(-1)).toEqual({ kind: "openIntent", handle });
    await lin.product.call({ call: "open", handle });
    await lin.product.call({ call: "open", handle });
    expect(lin.shown).toHaveLength(1);
  });

  test("leaves other requests in the list alone", () => {
    const lin = host(LIN);
    lin.notifications.add({
      id: "sample",
      kind: "invitation",
      sender: { name: "Kim", handle: "kim.example" },
      product: PRODUCT,
      purpose: "Hello",
      receivedAt: START,
      expiresAt: START + HOUR,
      seenAt: null,
      state: { tag: "pending" },
    });
    lin.notifications.accept("sample");
    expect(lin.service.owns("sample")).toBe(false);
    expect(lin.product.received).toEqual([]);
    expect(lin.delivery.calls).toEqual([]);
  });

  test("ignores a request whose return route its delivery cannot reach the sender on", () => {
    const lin = host(LIN);
    lin.delivery.unreachable.add("route:elsewhere");
    lin.delivery.receive!.request(
      fromAda({ returnRoute: "route:elsewhere" }).sealed,
    );
    lin.delivery.receive!.request(fromAda({ returnRoute: "other:1" }).sealed);
    expect(lin.notifications.list()).toEqual([]);
    expect(lin.delivery.calls).toEqual([]);
  });

  // A request has no deadline, so its time is only shown: an old one, or one
  // from a sender whose clock runs ahead, is still a request.
  test("ignores a request for another product, and takes one made at any time", () => {
    const lin = host(LIN);
    lin.delivery.receive!.request(
      fromAda({ product: { id: asProductId("other.example"), name: "Other" } })
        .sealed,
    );
    expect(lin.notifications.list()).toEqual([]);
    lin.delivery.receive!.request(
      fromAda({ createdAt: START + 2 * HOUR }).sealed,
    );
    lin.delivery.receive!.request(
      fromAda({ createdAt: START - 30 * 24 * HOUR }).sealed,
    );
    expect(lin.notifications.list()).toHaveLength(2);
  });

  test("keeps what it received and replied across a restart, and recognises a repeat", async () => {
    const storage = new MemoryStorage();
    const clock = new Clock();
    const lin = host(LIN, { storage, clock });
    const { handle, sealed, context } = receive(lin);
    const accepting = lin.service.requestAccept(handle);
    await lin.product.call({
      call: "reply",
      handle,
      payload: REPLY,
      open: false,
    });
    await accepting;
    lin.service.stop();
    const again = host(LIN, { storage, clock });
    expect(again.product.received).toEqual([
      {
        kind: "replied",
        request: { handle, sender: handleOf(ADA), repliedAt: START },
      },
    ]);
    // Nothing is posted until the sender, still waiting, posts the request again.
    expect(again.delivery.calls).toEqual([]);
    again.delivery.receive!.request(sealed);
    expect(again.notifications.list()).toHaveLength(1);
    expect(again.delivery.calls).toEqual([
      `postReply ${context.returnRoute} ${handle}`,
    ]);
  });

  test("a sender the host cannot name yet is told about once it can, once, with nothing posted again", async () => {
    const storage = new MemoryStorage();
    const clock = new Clock();
    const lin = host(LIN, { storage, clock });
    const { handle } = receive(lin);
    const accepting = lin.service.requestAccept(handle);
    await lin.product.call({
      call: "reply",
      handle,
      payload: REPLY,
      open: false,
    });
    await accepting;
    lin.service.stop();

    const naming = new Naming();
    naming.ready = false;
    const again = host(LIN, { storage, clock, naming });
    expect(again.product.received).toEqual([]);
    const replied: HostMessage = {
      kind: "replied",
      request: { handle, sender: handleOf(ADA), repliedAt: START },
    };
    naming.set(true);
    naming.set(true);
    expect(again.product.received).toEqual([replied]);
    // Reading the lists again tells everything again, as it always has.
    again.service.reload();
    expect(again.product.received).toEqual([replied, replied]);
    expect(again.delivery.calls).toEqual([]);

    // A stopped service no longer listens.
    again.service.stop();
    expect(naming.listeners.size).toBe(0);
  });

  test("a reply the product made this attach is not told back to it when names change", async () => {
    const naming = new Naming();
    const lin = host(LIN, { naming });
    const { handle } = receive(lin);
    const accepting = lin.service.requestAccept(handle);
    await lin.product.call({
      call: "reply",
      handle,
      payload: REPLY,
      open: false,
    });
    await accepting;
    const before = lin.product.received.length;
    naming.set(true);
    expect(lin.product.received.slice(before)).toEqual([]);
  });

  test("Accept waits for nothing it cannot name: it says so, sends nothing, and works once the host can", async () => {
    const naming = new Naming();
    naming.ready = false;
    const lin = host(LIN, { naming });
    const { handle } = receive(lin);
    await expect(lin.service.requestAccept(handle)).rejects.toThrow(
      "Your host cannot name the sender yet. Try again.",
    );
    expect(lin.product.received).toEqual([]);
    expect(lin.notifications.get(handle)!.state.tag).toBe("pending");
    naming.set(true);
    const accepting = lin.service.requestAccept(handle);
    await lin.product.call({
      call: "reply",
      handle,
      payload: REPLY,
      open: false,
    });
    await accepting;
    expect(lin.notifications.get(handle)!.state.tag).toBe("accepted");
  });
});

describe("the request service, across tabs and malformed input", () => {
  test("sealed bytes that open but are not an envelope are ignored, in both directions", async () => {
    const lin = host(LIN);
    for (const text of ["null", "1", "[]", '"x"'])
      expect(() =>
        lin.delivery.receive!.request(
          toHex(
            sealTo(
              fromHex(LIN.identity.agreementPublic),
              utf8(text),
              REQUEST_SEAL,
              cryptoRandom,
            ),
          ),
        ),
      ).not.toThrow();
    expect(lin.notifications.list()).toEqual([]);

    const ada = host(ADA);
    await ada.product.call({
      call: "request",
      request: {
        contact: handleOf(LIN),
        purpose: "Chat?",
        ttlMs: HOUR,
        payload: PAYLOAD,
      },
    });
    const [item] = ada.service.outbox.list();
    for (const text of ["null", "1", "[]"])
      expect(() =>
        ada.delivery.receive!.reply(
          item!.context.returnRoute,
          toHex(
            sealToXWing(
              fromHex(item!.context.replyKey),
              utf8(text),
              REPLY_SEALS.xwing,
              cryptoRandom,
            ),
          ),
        ),
      ).not.toThrow();
    expect(item!.replies).toEqual([]);
  });

  test("two tabs of one user give a request the same handle, and only one of them can reply", async () => {
    const storage = new MemoryStorage();
    const clock = new Clock();
    const first = host(LIN, { storage, clock });
    const second = host(LIN, { storage, clock });
    const { sealed, context } = fromAda();
    first.delivery.receive!.request(sealed);
    second.delivery.receive!.request(sealed);
    // As each tab does on the other's storage event.
    first.service.reload();
    second.service.reload();
    expect(first.notifications.list().map((row) => row.id)).toEqual(
      second.notifications.list().map((row) => row.id),
    );
    expect(first.notifications.list()).toHaveLength(1);
    const handle = first.notifications.list()[0]!.id;

    const accepts = [first, second].map((tab) => {
      const accepting = tab.service.requestAccept(handle);
      accepting.catch(() => {});
      return accepting;
    });
    expect(
      await first.product.call({
        call: "reply",
        handle,
        payload: REPLY,
        open: false,
      }),
    ).toEqual({ ok: true });
    expect(
      await second.product.call({
        call: "reply",
        handle,
        payload: `0x${"ef".repeat(64)}`,
        open: false,
      }),
    ).toEqual({
      ok: false,
      reason: "Refused",
      detail: "This request was already answered.",
    });
    await accepts[0];
    await expect(accepts[1]!).rejects.toThrow(
      "This request was already answered.",
    );
    expect(second.delivery.calls).toEqual([]);
    expect(first.delivery.calls).toEqual([
      `postReply ${context.returnRoute} ${handle}`,
    ]);
  });

  test("a request one tab cancels or completes stops posting in the user's other tabs once they read the lists", async () => {
    const storage = new MemoryStorage();
    const clock = new Clock();
    const first = host(ADA, { storage, clock });
    await first.product.call({
      call: "request",
      request: {
        contact: handleOf(LIN),
        purpose: "Chat?",
        ttlMs: HOUR,
        payload: PAYLOAD,
      },
    });
    const second = host(ADA, { storage, clock });
    const [item] = second.service.outbox.list();
    expect([...second.delivery.posts.keys()]).toEqual([item!.handle]);
    expect([...second.delivery.watched]).toEqual([item!.context.returnRoute]);
    await first.product.call({ call: "cancel", handle: item!.handle });
    second.service.reload();
    expect([...second.delivery.posts.keys()]).toEqual([]);
    expect([...second.delivery.watched]).toEqual([]);
  });
});

describe("the request service, sending", () => {
  const sendTo = (ada: ReturnType<typeof host>) =>
    ada.product.call({
      call: "request",
      request: {
        contact: handleOf(LIN),
        purpose: "Chat?",
        payload: PAYLOAD,
      },
    });

  test("the host makes the request id and reply key, takes a return route from its delivery, and watches it before it posts", async () => {
    const ada = host(ADA);
    const first = await sendTo(ada);
    const second = await sendTo(ada);
    const [one, two] = ada.service.outbox.list();
    expect(first).toEqual({
      ok: true,
      request: {
        handle: one!.handle,
        contact: handleOf(LIN),
        purpose: "Chat?",
        createdAt: START,
        status: "waiting",
      },
    });
    expect(second.ok).toBe(true);
    expect(one!.context.requestId).not.toBe(two!.context.requestId);
    expect([one!.context.returnRoute, two!.context.returnRoute]).toEqual([
      "route:1",
      "route:2",
    ]);
    expect(one!.context.replyKey).not.toBe(two!.context.replyKey);
    expect(one!.handle).not.toBe(one!.context.requestId);
    expect(ada.delivery.calls).toEqual([
      `watch ${one!.context.returnRoute}`,
      `postRequest ${one!.handle}`,
      `watch ${two!.context.returnRoute}`,
      `postRequest ${two!.handle}`,
    ]);
  });

  /**
   * A pending request from Ada to Lin on `route`, as Ada's host keeps it,
   * with `replySecret` kept for its reply key; and Lin's reply to it.
   */
  function kept(
    route: string,
    replyKey: { public: Uint8Array; secret: Uint8Array },
    replySecret = toHex(replyKey.secret),
  ) {
    const { context, sealed } = sealRequest(
      new WalletIdentityAuth(keysOf("ada")),
      LIN,
      {
        product: PRODUCT,
        purpose: "Chat?",
        requestId: toHex(cryptoRandom(16)),
        createdAt: START,
        returnRoute: route,
        replyKey: toHex(replyKey.public),
      },
      PAYLOAD,
      cryptoRandom,
    );
    const digest = requestDigest(context, PAYLOAD);
    const item = {
      handle: toHex(cryptoRandom(16)),
      to: LIN.id,
      contact: handleOf(LIN),
      context,
      payload: PAYLOAD,
      digest,
      sealed,
      replySecret,
      replies: [],
      state: { tag: "waiting" },
      paused: null,
    };
    const reply = sealReply(
      new WalletIdentityAuth(keysOf("lin")),
      ADA,
      { context, digest },
      REPLY,
      cryptoRandom,
    );
    return { item, reply };
  }

  const responses = (ada: ReturnType<typeof host>) =>
    ada.product.received.filter((message) => message.kind === "response");

  // Requests the host kept before X-Wing replies have an X25519 reply key
  // and nothing else new: the update must not strand them.
  test("a request kept before the update, with an X25519 reply key, is posted again and its reply opens", async () => {
    const storage = new MemoryStorage();
    const { item, reply } = kept("route:9", x25519Pair(cryptoRandom(32)));
    storage.setItem("ada:sent", JSON.stringify([item]));
    const ada = host(ADA, { storage });
    ada.delivery.receive!.reply("route:9", reply);
    await settle();
    expect({ calls: ada.delivery.calls, responses: responses(ada) }).toEqual({
      calls: ["watch route:9", `postRequest ${item.handle}`],
      responses: [{ kind: "response", handle: item.handle, payload: REPLY }],
    });
  });

  // One bad record must not stop the others on its route from opening.
  test("a kept reply secret that is not 32 bytes of hex opens nothing, and the next request on the route still opens", async () => {
    const storage = new MemoryStorage();
    const good = kept("route:9", xwingPair(cryptoRandom(32)));
    const notHex = kept("route:9", xwingPair(cryptoRandom(32)), "0xzz");
    const short = kept("route:9", x25519Pair(cryptoRandom(32)), "0x04");
    storage.setItem(
      "ada:sent",
      JSON.stringify([good.item, notHex.item, short.item]),
    );
    const ada = host(ADA, { storage });
    for (const { reply } of [short, notHex, good])
      ada.delivery.receive!.reply("route:9", reply);
    await settle();
    expect(responses(ada)).toEqual([
      { kind: "response", handle: good.item.handle, payload: REPLY },
    ]);
  });

  // Every post on a shared route is tried against every pending request:
  // a post that is not an X-Wing reply must not cost an encapsulation.
  test("a post that is not an X-Wing reply runs no ML-KEM key check against a pending X-Wing request", async () => {
    const ada = host(ADA);
    await sendTo(ada);
    const [item] = ada.service.outbox.list();
    const checks = spyOn(sealingModule, "isXWingPublic");
    try {
      for (const post of [
        toHex(cryptoRandom(2000)),
        toHex(
          sealTo(
            x25519Pair(cryptoRandom(32)).public,
            utf8("{}"),
            REPLY_SEALS.x25519,
            cryptoRandom,
          ),
        ),
      ])
        ada.delivery.receive!.reply(item!.context.returnRoute, post);
      await settle();
      expect(checks).not.toHaveBeenCalled();
    } finally {
      checks.mockRestore();
    }
    expect(responses(ada)).toEqual([]);
  });

  test("after a restart it watches and posts what still waits, and tells a new product everything", async () => {
    const storage = new MemoryStorage();
    const clock = new Clock();
    const ada = host(ADA, { storage, clock });
    const sent = await sendTo(ada);
    ada.service.stop();
    expect(ada.delivery.closed).toBe(true);
    const again = host(ADA, { storage, clock });
    const [item] = again.service.outbox.list();
    expect(again.delivery.calls).toEqual([
      `watch ${item!.context.returnRoute}`,
      `postRequest ${item!.handle}`,
    ]);
    expect(again.product.received).toEqual([
      {
        kind: "outgoing",
        request: (sent as { request: unknown }).request,
      } as HostMessage,
    ]);
  });
});

describe("a product's calls", () => {
  const request = (overrides: Record<string, unknown> = {}) => ({
    contact: handleOf(LIN),
    purpose: "Chat?",
    payload: PAYLOAD,
    ...overrides,
  });

  test("a request is bounded, and names its contact by a contact handle", () => {
    expect(readSendRequest(request()).ok).toBe(true);
    expect(readSendRequest(request({ purpose: "x".repeat(281) }))).toEqual({
      ok: false,
      reason: "The purpose is longer than 280 characters.",
    });
    expect(readSendRequest(request({ payload: "0x" }))).toEqual({
      ok: false,
      reason: "The payload is empty.",
    });
    expect(
      readSendRequest(
        request({ payload: `0x${"00".repeat(PAYLOAD_MAX_BYTES + 1)}` }),
      ),
    ).toEqual({
      ok: false,
      reason: `The payload is larger than ${PAYLOAD_MAX_BYTES} bytes.`,
    });
    for (const contact of [
      "fixture-contact:0x00",
      { bytes: "0x00" },
      handleOf(LIN).bytes,
    ])
      expect(readSendRequest(request({ contact }))).toEqual({
        ok: false,
        reason: "The contact is not a contact handle.",
      });
  });

  test("a call without an id, of a removed or unknown kind, or an earlier version's hello, is not read", () => {
    expect(readProductCall({ call: "request" })).toBeNull();
    for (const call of ["pickContact", "verify", "accept", "send", "decline"])
      expect(readProductCall({ id: 1, call, handle: "x" })).toBeNull();
    expect(
      readProductCall({ id: 1, call: "refuse", handle: "x", reason: " " }),
    ).toBeNull();
    expect(
      readProductCall({ id: 2, call: "complete", handle: "0x00" }),
    ).toEqual({ id: 2, call: "complete", handle: "0x00" });
    expect(
      isProductHello(
        { type: "invitations-product", protocol: "invitations/1" },
        [{}],
      ),
    ).toBe(false);
    expect(
      isProductHello({ type: PRODUCT_HELLO, protocol: "host-requests/1" }, [
        {},
      ]),
    ).toBe(true);
  });
});

describe("the outbox", () => {
  /** A request only as the outbox reads it back; nothing here is sealed. */
  const sent = (handle: string): OutgoingRequest =>
    ({
      handle,
      to: "bob",
      contact: { bytes: `0x${"aa".repeat(32)}` },
      context: { createdAt: 1, returnRoute: "route" },
      payload: "0x01",
      digest: "0x02",
      sealed: "0x03",
      replySecret: "0x04",
      replies: [],
      state: { tag: "waiting" },
      paused: null,
    }) as unknown as OutgoingRequest;

  // A reply checked before a close and kept after it would reach a product
  // that closed the request: the outbox itself refuses it.
  test("keeps a reply only for a pending request", () => {
    const outbox = new Outbox(new MemoryStorage(), "sent", () => 5);
    outbox.add(sent("0x0a"));
    outbox.add(sent("0x0b"));
    outbox.close("0x0b", { tag: "cancelled", at: 4 });

    expect([
      outbox.addReply("0x0a", "0x10"),
      outbox.addReply("0x0b", "0x10"),
    ]).toEqual([true, false]);
  });

  // The design deletes the reply key after setup: once closed, no later reply opens.
  test("drops the reply key's secret when a request is completed or cancelled, and keeps it while pending", () => {
    const storage = new MemoryStorage();
    const outbox = new Outbox(storage, "sent", () => 5);
    for (const handle of ["0x0a", "0x0b", "0x0c"]) outbox.add(sent(handle));
    outbox.addReply("0x0a", "0x10");
    outbox.close("0x0a", { tag: "completed", at: 5 });
    outbox.close("0x0b", { tag: "cancelled", at: 5 });

    const secrets = (list: readonly OutgoingRequest[]) =>
      list.map((item) => [item.handle, item.replySecret]);
    const stored = JSON.parse(storage.getItem("sent")!) as OutgoingRequest[];
    const expected = [
      ["0x0a", null],
      ["0x0b", null],
      ["0x0c", "0x04"],
    ];
    expect([secrets(outbox.list()), secrets(stored)]).toEqual([
      expected,
      expected,
    ]);
  });

  // A tab whose list is older than another tab's close must not write the
  // request back as pending, with its secret, nor drop the other tab's requests.
  test("a tab that has not read another tab's writes keeps its closes and its new requests", () => {
    const storage = new MemoryStorage();
    const first = new Outbox(storage, "sent", () => 5);
    first.add(sent("0x0a"));
    const second = new Outbox(storage, "sent", () => 5);
    first.close("0x0a", { tag: "cancelled", at: 5 });
    first.add(sent("0x0b"));

    expect([
      second.setPaused("0x0a", "offline"),
      second.addReply("0x0a", "0x10"),
    ]).toEqual([false, false]);
    second.add(sent("0x0c"));
    const stored = JSON.parse(storage.getItem("sent")!) as OutgoingRequest[];
    expect(
      stored.map((item) => [item.handle, item.state.tag, item.replySecret]),
    ).toEqual([
      ["0x0a", "cancelled", null],
      ["0x0b", "waiting", "0x04"],
      ["0x0c", "waiting", "0x04"],
    ]);
    expect(second.get("0x0a")!.state.tag).toBe("cancelled");
  });

  test("drops the secret of a closed request it reads back with one, in storage too", () => {
    const storage = new MemoryStorage();
    storage.setItem(
      "sent",
      JSON.stringify([
        { ...sent("0x0a"), state: { tag: "completed", at: 4 } },
        sent("0x0b"),
      ]),
    );
    new Outbox(storage, "sent", () => 5);
    const kept = JSON.parse(storage.getItem("sent")!) as OutgoingRequest[];
    expect(kept.map((item) => [item.handle, item.replySecret])).toEqual([
      ["0x0a", null],
      ["0x0b", "0x04"],
    ]);
  });
});
