import { describe, expect, test } from "bun:test";
import {
  FixtureIdentityAuth,
  fixtureCredential,
  publicIdentity,
} from "../../../fixtures/invitations/fixture-identity.js";
import { NotificationStore } from "../../../src/notifications/notifications.js";
import { MemoryStorage } from "../../support/test-support.js";
import {
  REPLY_SEAL,
  REQUEST_SEAL,
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
} from "../../../src/invitations/contract.js";
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
  digest,
  fromHex,
  sealTo,
  toHex,
  utf8,
} from "../../../src/invitations/sealing.js";

/**
 * The request service against fakes of its contacts and delivery, with the
 * reference fixture identity profile. The two-user flow over the dev fixtures
 * is in `tests/unit/invitations/invitation-flow.test.ts`.
 */

const START = Date.UTC(2026, 9, 21, 12, 0, 0);
const MINUTE = 60_000;
const HOUR = 60 * MINUTE;
const PRODUCT = { id: "chat.example", name: "Example Chat" };
const PAYLOAD = `0x${"ab".repeat(64)}`;
const REPLY = `0x${"cd".repeat(64)}`;

const credentialOf = (name: string) =>
  fixtureCredential(
    digest("test-signing", name),
    digest("test-agreement", name),
  );
const person = (id: string, name: string): Person => ({
  id,
  name,
  handle: `${id}.example`,
  identity: publicIdentity(credentialOf(id)),
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

/** Ada's and Lin's directory: each knows the other. */
function contacts(self: Person, picks: Person[]): ContactService {
  const people = [ADA, LIN].filter((other) => other.id !== self.id);
  return {
    pick: async () => {
      const chosen = picks.shift();
      return chosen === undefined
        ? { tag: "Dismissed" }
        : { tag: "Picked", value: { handle: handleOf(chosen) } };
    },
    resolve: (contact) =>
      people.find((other) => handleOf(other).bytes === contact.bytes) ?? null,
    byIdentity: (key) =>
      people.find((other) => other.identity.signingPublic === key),
    handleFor: handleOf,
  };
}

function host(
  self: Person,
  options: { storage?: MemoryStorage; clock?: Clock; picks?: Person[] } = {},
) {
  const storage = options.storage ?? new MemoryStorage();
  const clock = options.clock ?? new Clock();
  const notifications = new NotificationStore(storage, clock.now);
  notifications.setScope(`${self.id}:notifications`);
  const delivery = new FakeDelivery();
  const shown: number[] = [];
  const service = new InvitationService({
    product: PRODUCT,
    auth: new FixtureIdentityAuth(credentialOf(self.id), cryptoRandom),
    contacts: contacts(self, options.picks ?? []),
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
    new FixtureIdentityAuth(credentialOf("ada"), cryptoRandom),
    LIN,
    {
      product: PRODUCT,
      purpose: "Chat?",
      requestId: toHex(cryptoRandom(16)),
      createdAt: START,
      deadline: START + HOUR,
      returnRoute: `route:${toHex(cryptoRandom(8))}`,
      replyKey: toHex(cryptoRandom(32)),
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

  test("the product's reply after the deadline is refused, to it and to the user", async () => {
    const lin = host(LIN);
    const { handle } = receive(lin);
    const accepting = lin.service.requestAccept(handle);
    lin.clock.at = START + HOUR;
    expect(
      await lin.product.call({
        call: "reply",
        handle,
        payload: REPLY,
        open: true,
      }),
    ).toEqual({
      ok: false,
      reason: "Refused",
      detail: "This request expired.",
    });
    await expect(accepting).rejects.toThrow("This request expired.");
    expect(lin.delivery.calls).toEqual([]);
    await expect(lin.service.requestAccept("sample")).rejects.toThrow(
      "This is not a request to you.",
    );
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

  test("ignores a request for another product, with a lifetime over the limit, or whose deadline passed", () => {
    const lin = host(LIN);
    lin.delivery.receive!.request(
      fromAda({
        createdAt: START + 8 * 24 * HOUR,
        deadline: START + 8 * 24 * HOUR + HOUR,
      }).sealed,
    );
    lin.delivery.receive!.request(
      fromAda({ product: { id: "other.example", name: "Other" } }).sealed,
    );
    lin.delivery.receive!.request(
      fromAda({ createdAt: START - 8 * 24 * HOUR }).sealed,
    );
    lin.delivery.receive!.request(fromAda({ deadline: START }).sealed);
    expect(lin.notifications.list()).toEqual([]);
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
    expect(again.delivery.calls).toEqual([
      `postReply ${context.returnRoute} ${handle}`,
    ]);
    again.delivery.receive!.request(sealed);
    expect(again.notifications.list()).toHaveLength(1);
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
            sealTo(
              fromHex(item!.context.replyKey),
              utf8(text),
              REPLY_SEAL,
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
  const sendTo = (ada: ReturnType<typeof host>, ttlMs = HOUR) =>
    ada.product.call({
      call: "request",
      request: {
        contact: handleOf(LIN),
        purpose: "Chat?",
        ttlMs,
        payload: PAYLOAD,
      },
    });

  test("the host makes the request id and reply key, takes a return route from its delivery, and watches it before it posts", async () => {
    const ada = host(ADA, { picks: [LIN] });
    expect(await ada.product.call({ call: "pickContact" })).toEqual({
      ok: true,
      outcome: { tag: "Picked", value: { handle: handleOf(LIN) } },
    });
    expect(await ada.product.call({ call: "pickContact" })).toEqual({
      ok: true,
      outcome: { tag: "Dismissed" },
    });
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
        deadline: START + HOUR,
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

  test("a request past its deadline is withdrawn and unwatched, and reported once", async () => {
    const ada = host(ADA);
    await sendTo(ada, 2 * MINUTE);
    const [item] = ada.service.outbox.list();
    ada.clock.advance(2 * MINUTE);
    expect(ada.delivery.calls.slice(2)).toEqual([
      `withdraw ${item!.handle}`,
      `unwatch ${item!.context.returnRoute}`,
    ]);
    const reports = () =>
      ada.product.received.filter(
        (message) =>
          message.kind === "outgoing" && message.request.status === "expired",
      );
    expect(reports()).toHaveLength(1);
    ada.clock.advance(HOUR);
    expect(reports()).toHaveLength(1);
  });
});

describe("a product's calls", () => {
  const request = (overrides: Record<string, unknown> = {}) => ({
    contact: handleOf(LIN),
    purpose: "Chat?",
    ttlMs: HOUR,
    payload: PAYLOAD,
    ...overrides,
  });

  test("a request is bounded, and names its contact by a contact handle", () => {
    expect(readSendRequest(request()).ok).toBe(true);
    expect(readSendRequest(request({ purpose: "x".repeat(281) }))).toEqual({
      ok: false,
      reason: "The purpose is longer than 280 characters.",
    });
    expect(readSendRequest(request({ ttlMs: 8 * 24 * HOUR }))).toEqual({
      ok: false,
      reason: "The expiry is outside one minute to seven days.",
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
    expect(readProductCall({ call: "pickContact" })).toBeNull();
    for (const call of ["verify", "accept", "send", "decline"])
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
