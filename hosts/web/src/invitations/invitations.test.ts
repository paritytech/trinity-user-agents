import { describe, expect, test } from "bun:test";
import { NotificationStore } from "../notifications.js";
import { MemoryStorage } from "../test-support.js";
import {
  hostKeys,
  peerKeysOf,
  sealEnvelope,
  type Envelope,
  type HostKeys,
} from "./binding.js";
import {
  PAYLOAD_MAX_BYTES,
  readProductCall,
  readSendRequest,
  type CallResult,
  type HostMessage,
  type OutgoingView,
} from "./contract.js";
import {
  INTENT_TIMEOUT_MS,
  InvitationService,
  receivedId,
  type ProductPort,
} from "./invitation-service.js";
import type { ContactService, InvitationDelivery, Person } from "./ports.js";
import { cryptoRandom, digest, toHex, x25519Pair } from "./sealing.js";

/**
 * The invitation service against fakes of its ports: made-up keys and
 * people, and a delivery the test plays. The two-user flow over the dev
 * fixtures is in `src/dev-tools/invitation-flow.test.ts`.
 */

const START = Date.UTC(2026, 9, 21, 12, 0, 0);
const MINUTE = 60_000;
const HOUR = 60 * MINUTE;
const PRODUCT = { id: "chat.example", name: "Example Chat" };
const SETUP = `0x${"ab".repeat(64)}`;
const RESPONSE = `0x${"cd".repeat(64)}`;

const keysOf = (name: string): HostKeys =>
  hostKeys(
    digest("test-signing", name),
    x25519Pair(digest("test-inbox", name)),
  );
const ADA_KEYS = keysOf("ada");
const LIN_KEYS = keysOf("lin");
const ADA: Person = {
  id: "ada",
  name: "Ada",
  handle: "ada.example",
  keys: peerKeysOf(ADA_KEYS),
};
const LIN: Person = {
  id: "lin",
  name: "Lin",
  handle: "lin.example",
  keys: peerKeysOf(LIN_KEYS),
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

class FakeDelivery implements InvitationDelivery {
  receive: ((sealed: string) => void) | null = null;
  readonly posts = new Map<string, string>();
  readonly posted: string[] = [];
  closed = false;
  listen(receive: (sealed: string) => void): void {
    this.receive = receive;
  }
  post(_to: unknown, id: string, sealed: string): void {
    this.posts.set(id, sealed);
    this.posted.push(id);
  }
  withdraw(id: string): void {
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

/** Ada's and Lin's directory: each knows the other, and refers to them as `ref:<id>`. */
function contacts(self: Person, picks: Person[]): ContactService {
  const people = [ADA, LIN].filter((person) => person.id !== self.id);
  return {
    pick: async () => {
      const person = picks.shift();
      return person === undefined ? null : `ref:${person.id}`;
    },
    resolve: (_product, contact) =>
      people.find((person) => `ref:${person.id}` === contact) ?? null,
    bySigningKey: (key) =>
      people.find((person) => person.keys.signingPublic === key),
    refFor: (_product, person) => `ref:${person.id}`,
  };
}

function host(
  self: Person,
  keys: HostKeys,
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
    self: keys,
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

/** An invitation from Ada to Lin, as Ada's host seals it. */
function fromAda(requestByte: string, overrides: Partial<Envelope> = {}) {
  const envelope: Envelope = {
    product: PRODUCT,
    purpose: "Chat?",
    createdAt: START,
    invitation: {
      requestId: `0x${requestByte.repeat(16)}`,
      challenge: `0x${"11".repeat(32)}`,
      expiresAt: START + HOUR,
      setup: SETUP,
    },
    ...overrides,
  };
  return {
    sealed: sealEnvelope(ADA_KEYS, LIN.keys, envelope, cryptoRandom),
    id: receivedId(ADA.keys.signingPublic, envelope.invitation.requestId),
  };
}

describe("the invitation service, receiving", () => {
  test("Accept is the user's intent: only the product's accept, with its response, accepts", async () => {
    const lin = host(LIN, LIN_KEYS);
    const { sealed, id } = fromAda("aa");
    lin.delivery.receive!(sealed);
    expect(lin.notifications.get(id)!.state.tag).toBe("pending");
    expect(lin.product.received).toEqual([]);

    const accepting = lin.service.requestAccept(id);
    expect(lin.product.received).toEqual([
      {
        kind: "acceptIntent",
        invitation: {
          invitationId: id,
          sender: "ref:ada",
          purpose: "Chat?",
          invitation: {
            requestId: `0x${"aa".repeat(16)}`,
            challenge: `0x${"11".repeat(32)}`,
            expiresAt: START + HOUR,
            setup: SETUP,
          },
        },
      },
    ]);
    expect(lin.notifications.get(id)!.state.tag).toBe("pending");
    const result = await lin.product.call({
      call: "accept",
      invitationId: id,
      response: RESPONSE,
      open: true,
    });
    await accepting;
    expect(result).toEqual({ ok: true, proof: expect.stringMatching(/^0x/) });
    expect(lin.notifications.get(id)!.state.tag).toBe("accepted");
    expect(lin.shown).toHaveLength(1);
    expect(lin.delivery.posted).toEqual([]);
  });

  test("the user's Accept fails, with a reason and nothing accepted, when the product refuses, is silent, or is closed", async () => {
    const lin = host(LIN, LIN_KEYS);
    const ids = ["a1", "b2", "c3", "d4"].map((byte) => {
      const { sealed, id } = fromAda(byte);
      lin.delivery.receive!(sealed);
      return id;
    });

    const refused = lin.service.requestAccept(ids[0]!);
    await lin.product.call({
      call: "refuse",
      invitationId: ids[0],
      reason: "Example Chat cannot use this invitation.",
    });
    await expect(refused).rejects.toThrow(
      "Example Chat cannot use this invitation.",
    );

    const silent = lin.service.requestAccept(ids[1]!);
    lin.clock.advance(INTENT_TIMEOUT_MS);
    await expect(silent).rejects.toThrow("Example Chat did not answer.");
    expect(
      await lin.product.call({
        call: "accept",
        invitationId: ids[1],
        response: RESPONSE,
        open: true,
      }),
    ).toEqual({
      ok: false,
      reason: "Refused",
      detail: "The user did not choose Accept for it.",
    });

    const closing = lin.service.requestAccept(ids[2]!);
    lin.service.detachProduct();
    await expect(closing).rejects.toThrow(
      "Example Chat closed before it answered.",
    );
    await expect(lin.service.requestAccept(ids[3]!)).rejects.toThrow(
      "Example Chat is not open.",
    );

    for (const id of ids)
      expect(lin.notifications.get(id)!.state.tag).toBe("pending");
    expect(lin.shown).toEqual([]);

    // After a failure the user can try again.
    lin.product.closed = false;
    lin.service.attachProduct(lin.product);
    const retry = lin.service.requestAccept(ids[0]!);
    await lin.product.call({
      call: "accept",
      invitationId: ids[0],
      response: RESPONSE,
      open: false,
    });
    await retry;
    expect(lin.notifications.get(ids[0]!)!.state.tag).toBe("accepted");
  });

  test("the product's accept after the expiry is refused, to it and to the user", async () => {
    const lin = host(LIN, LIN_KEYS);
    const { sealed, id } = fromAda("aa");
    lin.delivery.receive!(sealed);
    const accepting = lin.service.requestAccept(id);
    lin.clock.at = START + HOUR;
    expect(
      await lin.product.call({
        call: "accept",
        invitationId: id,
        response: RESPONSE,
        open: true,
      }),
    ).toEqual({
      ok: false,
      reason: "Refused",
      detail: "This request expired.",
    });
    await expect(accepting).rejects.toThrow("This request expired.");
    expect(lin.shown).toEqual([]);
    await expect(lin.service.requestAccept("sample")).rejects.toThrow(
      "This is not an invitation to you.",
    );
  });

  test("opening an accepted invitation asks the product, which shows itself only when asked", async () => {
    const lin = host(LIN, LIN_KEYS);
    const { sealed, id } = fromAda("aa");
    lin.delivery.receive!(sealed);
    const accepting = lin.service.requestAccept(id);
    await lin.product.call({
      call: "accept",
      invitationId: id,
      response: RESPONSE,
      open: false,
    });
    await accepting;
    expect(lin.shown).toEqual([]);
    await lin.product.call({ call: "open", invitationId: id });
    expect(lin.shown).toEqual([]);
    lin.service.requestOpen(id);
    expect(lin.product.received.at(-1)).toEqual({
      kind: "openIntent",
      invitationId: id,
    });
    await lin.product.call({ call: "open", invitationId: id });
    await lin.product.call({ call: "open", invitationId: id });
    expect(lin.shown).toHaveLength(1);
  });

  test("leaves other requests in the list alone", () => {
    const lin = host(LIN, LIN_KEYS);
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
  });

  test("ignores an invitation for another product or with a lifetime over the limit", () => {
    const lin = host(LIN, LIN_KEYS);
    lin.delivery.receive!(
      fromAda("aa", { product: { id: "other.example", name: "Other" } }).sealed,
    );
    lin.delivery.receive!(
      fromAda("bb", { createdAt: START - 8 * 24 * HOUR }).sealed,
    );
    expect(lin.notifications.list()).toEqual([]);
  });

  test("keeps what it received, and what it accepted, across a restart", async () => {
    const storage = new MemoryStorage();
    const clock = new Clock();
    const lin = host(LIN, LIN_KEYS, { storage, clock });
    const { sealed, id } = fromAda("aa");
    lin.delivery.receive!(sealed);
    const accepting = lin.service.requestAccept(id);
    await lin.product.call({
      call: "accept",
      invitationId: id,
      response: RESPONSE,
      open: false,
    });
    await accepting;
    lin.service.stop();
    const again = host(LIN, LIN_KEYS, { storage, clock });
    expect(again.product.received).toEqual([
      {
        kind: "accepted",
        invitation: expect.objectContaining({
          invitationId: id,
          sender: "ref:ada",
          acceptedAt: START,
        }),
      },
    ]);
    again.delivery.receive!(sealed);
    expect(again.notifications.list()).toHaveLength(1);
  });
});

describe("the invitation service, sending", () => {
  test("sends to a picked contact, posts again after a restart, and tells a new product everything", async () => {
    const storage = new MemoryStorage();
    const clock = new Clock();
    const ada = host(ADA, ADA_KEYS, { storage, clock, picks: [LIN] });
    const picked = await ada.product.call({ call: "pickContact" });
    expect(picked).toEqual({ ok: true, contact: "ref:lin" });
    expect(await ada.product.call({ call: "pickContact" })).toEqual({
      ok: false,
      reason: "Dismissed",
      detail: "No contact was chosen.",
    });
    const request = {
      contact: "ref:lin",
      purpose: "Chat?",
      ttlMs: HOUR,
      invitation: {
        requestId: `0x${"aa".repeat(16)}`,
        challenge: `0x${"11".repeat(32)}`,
        setup: SETUP,
      },
    };
    const sent = await ada.product.call({ call: "send", request });
    expect(sent).toEqual({
      ok: true,
      invitation: {
        requestId: `0x${"aa".repeat(16)}`,
        contact: "ref:lin",
        purpose: "Chat?",
        createdAt: START,
        expiresAt: START + HOUR,
        status: "waiting",
      },
    });
    expect(await ada.product.call({ call: "send", request })).toEqual({
      ok: false,
      reason: "Invalid",
      detail: "The request id is already in use.",
    });
    expect([...ada.delivery.posts.keys()]).toEqual([`0x${"aa".repeat(16)}`]);

    ada.service.stop();
    expect(ada.delivery.closed).toBe(true);
    const again = host(ADA, ADA_KEYS, { storage, clock });
    expect([...again.delivery.posts.keys()]).toEqual([`0x${"aa".repeat(16)}`]);
    expect(again.product.received).toEqual([
      {
        kind: "outgoing",
        invitation: (sent as { invitation: OutgoingView }).invitation,
      },
    ]);
  });

  test("an expired invitation is withdrawn and reported once, and not posted again", async () => {
    const ada = host(ADA, ADA_KEYS);
    await ada.product.call({
      call: "send",
      request: {
        contact: "ref:lin",
        purpose: "Chat?",
        ttlMs: 2 * MINUTE,
        invitation: {
          requestId: `0x${"aa".repeat(16)}`,
          challenge: `0x${"11".repeat(32)}`,
          setup: SETUP,
        },
      },
    });
    ada.clock.advance(2 * MINUTE);
    expect(ada.delivery.posts.size).toBe(0);
    const reports = () =>
      ada.product.received.filter(
        (message) =>
          message.kind === "outgoing" &&
          message.invitation.status === "expired",
      );
    expect(reports()).toHaveLength(1);
    ada.clock.advance(HOUR);
    expect(reports()).toHaveLength(1);
  });
});

describe("a product's calls", () => {
  const request = (overrides: Record<string, unknown> = {}) => ({
    contact: "ref:lin",
    purpose: "Chat?",
    ttlMs: HOUR,
    invitation: {
      requestId: `0x${"aa".repeat(16)}`,
      challenge: `0x${"11".repeat(32)}`,
      setup: SETUP,
    },
    ...overrides,
  });

  test("a send request is bounded", () => {
    expect(readSendRequest(request()).ok).toBe(true);
    expect(readSendRequest(request({ purpose: "x".repeat(281) }))).toEqual({
      ok: false,
      reason: "The purpose is longer than 280 characters.",
    });
    expect(readSendRequest(request({ ttlMs: 8 * 24 * HOUR }))).toEqual({
      ok: false,
      reason: "The expiry is outside one minute to seven days.",
    });
    expect(
      readSendRequest(
        request({
          invitation: { ...request().invitation, setup: "0x" },
        }),
      ),
    ).toEqual({ ok: false, reason: "The setup data is empty." });
    expect(
      readSendRequest(
        request({
          invitation: {
            ...request().invitation,
            setup: `0x${"00".repeat(PAYLOAD_MAX_BYTES + 1)}`,
          },
        }),
      ),
    ).toEqual({
      ok: false,
      reason: `The setup data is larger than ${PAYLOAD_MAX_BYTES} bytes.`,
    });
    expect(
      readSendRequest(
        request({ invitation: { ...request().invitation, challenge: "0x11" } }),
      ),
    ).toEqual({
      ok: false,
      reason: "The challenge is not 32 bytes of 0x hex.",
    });
    expect(readSendRequest(request({ contact: "" }))).toEqual({
      ok: false,
      reason: "No contact.",
    });
  });

  test("a call without an id, or of an unknown kind, is not read", () => {
    expect(readProductCall({ call: "pickContact" })).toBeNull();
    expect(
      readProductCall({ id: 1, call: "decline", invitationId: "x" }),
    ).toBeNull();
    expect(
      readProductCall({
        id: 1,
        call: "refuse",
        invitationId: "x",
        reason: " ",
      }),
    ).toBeNull();
    expect(
      readProductCall({ id: 2, call: "cancel", requestId: "0x00" }),
    ).toEqual({
      id: 2,
      call: "cancel",
      requestId: "0x00",
    });
  });
});

describe("the host's keys", () => {
  test("are separate per person, and a sealed envelope opens only for its recipient", () => {
    expect(toHex(ADA_KEYS.signingPublic)).not.toBe(
      toHex(LIN_KEYS.signingPublic),
    );
    const lin = host(LIN, LIN_KEYS);
    const ada = host(ADA, ADA_KEYS);
    const { sealed } = fromAda("aa");
    ada.delivery.receive!(sealed);
    expect(ada.notifications.list()).toEqual([]);
    lin.delivery.receive!(sealed);
    expect(lin.notifications.list()).toHaveLength(1);
  });
});
