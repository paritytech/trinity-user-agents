import type { HostNotification, NotificationStore } from "../notifications.js";
import {
  openReply,
  openRequest,
  requestDigest,
  sealReply,
  sealRequest,
} from "./binding.js";
import {
  readPayload,
  readProductCall,
  readSendRequest,
  TTL_MAX_MS,
  type CallResult,
  type FailureReason,
  type HostMessage,
  type OutgoingView,
  type ProductCall,
  type RequestHandle,
} from "./contract.js";
import { Outbox, type OutgoingRequest } from "./outbox.js";
import type {
  ContactService,
  HostDelivery,
  IdentityAuth,
  Person,
  Product,
} from "./ports.js";
import { ReceivedRequests, type ReceivedRequest } from "./received.js";
import {
  digest,
  fromHex,
  toHex,
  x25519Pair,
  type RandomBytes,
} from "./sealing.js";

/**
 * One user's host side of the proposed request service (`contract.ts`) for
 * one product. It serves the product, asks its user, keeps what was sent and
 * received, and reaches other hosts only through a {@link HostDelivery}.
 *
 * Sending: the product names a contact by handle and gives its payload. The
 * host resolves the contact, asks its user, makes the request id, the return
 * topic `T_SETUP` and a reply key pair, watches `T_SETUP`, then authenticates,
 * seals and posts the request until its deadline. A reply on `T_SETUP` reaches
 * the product only once the host opened and authenticated it. The product's
 * `complete` closes the request and stops the reposts; nothing is sent.
 *
 * Receiving: an authenticated request from a contact, for this product, before
 * its deadline and not seen before, goes to Notifications. Nothing goes back:
 * not on arrival, not when seen, not on Dismiss. Accept is the user's intent,
 * handed with the payload to the product. Only the product's `reply` accepts
 * it; the host then authenticates, seals and posts the reply on the request's
 * return topic.
 *
 * Demo choice, not the design's: the recipient's host posts its sealed reply
 * again on the delivery's cadence until the request's deadline, and again
 * when it starts. The design leaves when the recipient resends and stops open.
 */

/** What the host asks its user before a request leaves. */
export interface SendPrompt {
  product: Product;
  to: { name: string; handle: string };
  purpose: string;
  ttlMs: number;
}

/** The product's end of its port, as the host holds it. */
export interface ProductPort {
  post(message: HostMessage): void;
  listen(handler: (message: unknown) => void): void;
  close(): void;
}

export interface InvitationServiceDeps {
  /** The product this service serves, from the caller's context. */
  product: Product;
  auth: IdentityAuth;
  contacts: ContactService;
  /** This user's Notifications list. */
  notifications: NotificationStore;
  storage: Storage;
  /** Where this user's sent and received requests are kept. */
  keys: { sent: string; received: string };
  delivery: HostDelivery;
  now(): number;
  /** Run `task` after `delayMs`. Returns a function that cancels it. */
  schedule(task: () => void, delayMs: number): () => void;
  random: RandomBytes;
  /** Ask the user whether to send. */
  confirmSend(prompt: SendPrompt): Promise<boolean>;
  /** Bring this user's product to the front, at its own request. */
  showProduct(): void;
  log(line: string): void;
}

const short = (id: string): string => id.replace(/^0x/, "").slice(0, 8);

/** How long the product has to answer an intent before the user is told it did not. */
export const INTENT_TIMEOUT_MS = 30_000;

/** An Accept the user chose, waiting for the product. */
interface PendingAccept {
  done: Promise<void>;
  resolve(): void;
  reject(error: Error): void;
  cancelTimeout(): void;
}

/** The host's end of the port a product frame handed over in its hello. */
export function productPort(port: MessagePort): ProductPort {
  return {
    post: (message) => port.postMessage(message),
    listen: (handler) => {
      port.onmessage = (message: MessageEvent<unknown>) =>
        handler(message.data);
      port.start();
    },
    close: () => port.close(),
  };
}

const failure = (reason: FailureReason, detail: string): CallResult => ({
  ok: false,
  reason,
  detail,
});

export class InvitationService {
  readonly outbox: Outbox;
  readonly received: ReceivedRequests;
  private product: ProductPort | null = null;
  private cancelExpiry: (() => void) | null = null;
  /** Deadlines up to this time were handled. */
  private deadlinesHandledTo: number;
  private stopped = false;
  /** Accepts the user chose that the product has not answered yet, by handle. */
  private readonly accepting = new Map<string, PendingAccept>();
  /** Replied requests the user opened, which the product may answer by coming to the front. */
  private readonly opening = new Set<string>();

  constructor(private readonly deps: InvitationServiceDeps) {
    this.outbox = new Outbox(deps.storage, deps.keys.sent, deps.now);
    this.received = new ReceivedRequests(deps.storage, deps.keys.received);
    this.deadlinesHandledTo = deps.now();
  }

  start(): void {
    this.deps.delivery.listen({
      request: (sealed) => this.receiveRequest(sealed),
      reply: (topic, sealed) => this.receiveReply(topic, sealed),
    });
    for (const item of this.outbox.waiting()) this.post(item);
    const now = this.deps.now();
    for (const item of this.received.list())
      if (item.replied !== null && now < item.context.deadline)
        this.postReply(item, item.replied.sealed);
    this.scheduleExpiry();
  }

  /** Close the delivery and the product's port, and stop every timer. */
  stop(): void {
    if (this.stopped) return;
    this.stopped = true;
    this.deps.delivery.close();
    this.detachProduct();
    this.cancelExpiry?.();
    this.cancelExpiry = null;
    this.failAccepts("This user's host stopped.");
  }

  /** Serve `port`'s product in place of any before it, and tell it everything it is owed. */
  attachProduct(port: ProductPort): void {
    this.detachProduct();
    this.product = port;
    port.listen((value) => {
      if (this.product === port) void this.call(port, value);
    });
    this.replay(port);
  }

  /** Tell `port`'s product everything it is owed, as the lists say now. */
  private replay(port: ProductPort): void {
    for (const item of this.outbox.list()) {
      port.post({ kind: "outgoing", request: this.outgoingView(item) });
      if (this.outbox.status(item) === "waiting")
        for (const reply of item.replies)
          port.post({
            kind: "response",
            handle: item.handle,
            payload: reply.payload,
          });
    }
    for (const item of this.received.list()) {
      const sender = this.deps.contacts.byIdentity(item.context.from);
      if (item.replied !== null && sender !== undefined)
        port.post({
          kind: "replied",
          request: {
            handle: item.handle,
            sender: this.deps.contacts.handleFor(sender),
            repliedAt: item.replied.at,
          },
        });
    }
  }

  detachProduct(): void {
    this.product?.close();
    this.product = null;
    this.opening.clear();
    this.failAccepts(`${this.deps.product.name} closed before it answered.`);
  }

  private failAccepts(reason: string): void {
    for (const [handle, pending] of [...this.accepting]) {
      this.accepting.delete(handle);
      pending.cancelTimeout();
      pending.reject(new Error(reason));
    }
  }

  /** Whether `handle` is a request this host received, so this service answers it. */
  owns(handle: string): boolean {
    return this.received.get(handle) !== undefined;
  }

  /**
   * The user chose Accept for a received request. The product gets the intent
   * with the payload, and this resolves once it replied. Rejects, with a
   * message for the user, when the product refuses, does not answer in time,
   * or is not open; then nothing is accepted and nothing is sent.
   */
  requestAccept(handle: string): Promise<void> {
    const waiting = this.accepting.get(handle);
    if (waiting !== undefined) return waiting.done;
    const received = this.received.get(handle);
    const item = this.deps.notifications.get(handle);
    const refuse = (reason: string) => Promise.reject(new Error(reason));
    if (this.stopped) return refuse("This user's host stopped.");
    if (received === undefined || item === undefined)
      return refuse("This is not a request to you.");
    if (this.deps.now() >= item.expiresAt && item.state.tag === "pending")
      return refuse("This request expired.");
    if (item.state.tag !== "pending")
      return refuse(`This request was already ${item.state.tag}.`);
    const sender = this.deps.contacts.byIdentity(received.context.from);
    if (sender === undefined)
      return refuse("The sender is no longer one of your contacts.");
    const product = this.product;
    if (product === null)
      return refuse(`${this.deps.product.name} is not open.`);

    let resolve = () => {};
    let reject: (error: Error) => void = () => {};
    const done = new Promise<void>((yes, no) => {
      resolve = yes;
      reject = no;
    });
    const pending: PendingAccept = {
      done,
      resolve,
      reject,
      cancelTimeout: () => {},
    };
    pending.cancelTimeout = this.deps.schedule(() => {
      if (this.accepting.get(handle) !== pending) return;
      this.accepting.delete(handle);
      pending.reject(
        new Error(`${this.deps.product.name} did not answer. Try again.`),
      );
    }, INTENT_TIMEOUT_MS);
    this.accepting.set(handle, pending);
    this.deps.log(
      `Request ${short(handle)}: the user chose Accept; ${this.deps.product.name} decides.`,
    );
    product.post({
      kind: "acceptIntent",
      request: {
        handle,
        sender: this.deps.contacts.handleFor(sender),
        payload: received.payload,
      },
    });
    return done;
  }

  /** The user opened a replied request: the product may come to the front for it. */
  requestOpen(handle: string): void {
    const item = this.deps.notifications.get(handle);
    if (
      this.product === null ||
      item?.state.tag !== "accepted" ||
      !this.owns(handle)
    )
      return;
    this.opening.add(handle);
    this.product.post({ kind: "openIntent", handle });
  }

  /**
   * Read every list again after another tab wrote them, and stop posting what
   * another tab closed.
   */
  reload(): void {
    this.deps.notifications.reload();
    this.outbox.reload();
    this.received.reload();
    for (const item of this.outbox.list())
      if (item.state.tag !== "waiting") this.stopPosting(item);
    if (this.product !== null) this.replay(this.product);
    this.scheduleExpiry();
  }

  private post(item: OutgoingRequest): void {
    this.deps.delivery.watch(item.context.setupTopic);
    const person = this.deps.contacts.byIdentity(item.context.to);
    if (person === undefined) return;
    this.deps.delivery.postRequest(
      person.identity,
      item.handle,
      item.sealed,
      item.context.deadline,
    );
  }

  private postReply(item: ReceivedRequest, sealed: string): void {
    this.deps.delivery.postReply(
      item.context.setupTopic,
      item.handle,
      sealed,
      item.context.deadline,
    );
  }

  private stopPosting(item: OutgoingRequest): void {
    this.deps.delivery.withdraw(item.handle);
    this.deps.delivery.unwatch(item.context.setupTopic);
  }

  private outgoingView(item: OutgoingRequest): OutgoingView {
    return {
      handle: item.handle,
      contact: item.contact,
      purpose: item.context.purpose,
      createdAt: item.context.createdAt,
      deadline: item.context.deadline,
      status: this.outbox.status(item),
      ...(item.state.tag === "waiting" ? {} : { closedAt: item.state.at }),
    };
  }

  private pushOutgoing(item: OutgoingRequest): void {
    this.product?.post({ kind: "outgoing", request: this.outgoingView(item) });
  }

  private async call(port: ProductPort, value: unknown): Promise<void> {
    const call = readProductCall(value);
    if (call === null) {
      this.deps.log("Ignored a product message that is not a call.");
      return;
    }
    const result = await this.answer(call);
    if (this.product === port && !this.stopped)
      port.post({ kind: "result", id: call.id, result });
  }

  private async answer(call: ProductCall): Promise<CallResult> {
    switch (call.call) {
      case "pickContact":
        return {
          ok: true,
          outcome: await this.deps.contacts.pick(this.deps.product),
        };
      case "request":
        return this.send(call.request);
      case "complete":
        return this.complete(call.handle);
      case "cancel":
        return this.cancel(call.handle);
      case "reply":
        return this.reply(call.handle, call.payload, call.open);
      case "refuse":
        return this.refuse(call.handle, call.reason);
      case "open":
        return this.open(call.handle);
    }
  }

  private async send(value: unknown): Promise<CallResult> {
    const request = readSendRequest(value);
    if (!request.ok) return failure("Invalid", request.reason);
    const { contact, purpose, ttlMs, payload } = request.value;
    const product = this.deps.product;
    const person = this.deps.contacts.resolve(contact);
    if (person === null)
      return failure("Unknown", "This contact names nobody here.");
    const confirmed = await this.deps.confirmSend({
      product,
      to: { name: person.name, handle: person.handle },
      purpose,
      ttlMs,
    });
    if (!confirmed) return failure("Dismissed", "The user did not send it.");
    if (this.stopped) return failure("Refused", "This user's host stopped.");
    const { random } = this.deps;
    const replyKey = x25519Pair(random(32));
    const now = this.deps.now();
    const { context, sealed } = sealRequest(
      this.deps.auth,
      person,
      {
        product,
        purpose,
        requestId: toHex(random(16)),
        createdAt: now,
        deadline: now + ttlMs,
        setupTopic: toHex(random(32)),
        replyKey: toHex(replyKey.public),
      },
      payload,
      random,
    );
    const item: OutgoingRequest = {
      handle: toHex(random(16)),
      to: person.id,
      contact,
      context,
      payload,
      digest: requestDigest(context, payload),
      sealed,
      replySecret: toHex(replyKey.secret),
      replies: [],
      state: { tag: "waiting" },
    };
    this.outbox.add(item);
    this.deps.log(
      `Request ${short(item.handle)}: watching its return topic, then posting it, sealed for ${person.name}.`,
    );
    this.post(item);
    this.scheduleExpiry();
    return { ok: true, request: this.outgoingView(item) };
  }

  /** A waiting request of the product's, or why the call is refused. */
  private waitingItem(
    handle: RequestHandle,
  ): OutgoingRequest | { failure: CallResult } {
    const item = this.outbox.get(handle);
    if (item === undefined)
      return {
        failure: failure("Unknown", "No sent request has this handle."),
      };
    const status = this.outbox.status(item);
    return status === "waiting"
      ? item
      : { failure: failure("Refused", `This request is ${status}.`) };
  }

  private complete(handle: RequestHandle): CallResult {
    const item = this.waitingItem(handle);
    if ("failure" in item) return item.failure;
    if (item.replies.length === 0)
      return failure("Refused", "No authenticated reply has arrived.");
    this.outbox.close(handle, { tag: "completed", at: this.deps.now() });
    this.stopPosting(item);
    this.deps.log(
      `Request ${short(handle)} completed by ${this.deps.product.name}; reposting stopped. Nothing is sent.`,
    );
    this.scheduleExpiry();
    return { ok: true, request: this.outgoingView(item) };
  }

  private cancel(handle: RequestHandle): CallResult {
    const item = this.waitingItem(handle);
    if ("failure" in item) return item.failure;
    this.outbox.close(handle, { tag: "cancelled", at: this.deps.now() });
    this.stopPosting(item);
    this.deps.log(
      `Request ${short(handle)} cancelled; reposting stopped. Nothing is sent.`,
    );
    this.scheduleExpiry();
    return { ok: true, request: this.outgoingView(item) };
  }

  /** The product replied to a request the user chose to accept. */
  private reply(
    handle: RequestHandle,
    payload: string,
    open: boolean,
  ): CallResult {
    const pending = this.accepting.get(handle);
    if (pending === undefined) {
      this.deps.log(
        `Ignored ${this.deps.product.name}'s reply to ${short(handle)}: the user did not choose Accept for it.`,
      );
      return failure("Refused", "The user did not choose Accept for it.");
    }
    const settle = (result: CallResult): CallResult => {
      this.accepting.delete(handle);
      pending.cancelTimeout();
      if (result.ok) pending.resolve();
      else pending.reject(new Error(result.detail));
      return result;
    };
    const read = readPayload(payload);
    if (!read.ok) return settle(failure("Invalid", read.reason));
    // Another tab of this user may have replied: read the lists as they are now.
    this.received.reload();
    this.deps.notifications.reload();
    const received = this.received.get(handle);
    const sender =
      received === undefined
        ? undefined
        : this.deps.contacts.byIdentity(received.context.from);
    if (received === undefined || sender === undefined)
      return settle(failure("Unknown", "The sender is no longer a contact."));
    if (received.replied !== null)
      return settle(failure("Refused", "This request was already answered."));
    const answered = this.deps.notifications.accept(handle);
    if (!answered.ok) return settle(failure("Refused", answered.reason));
    if (!answered.changed)
      return settle(failure("Refused", "This request was already answered."));
    const sealed = sealReply(
      this.deps.auth,
      sender,
      { context: received.context, digest: received.digest },
      payload,
      this.deps.random,
    );
    this.received.markReplied(handle, sealed, this.deps.now());
    this.postReply(received, sealed);
    this.deps.log(
      `${this.deps.product.name} replied to request ${short(handle)}; the host sealed it for ${sender.name} and posted it on the request's return topic.`,
    );
    if (open) this.deps.showProduct();
    return settle({ ok: true });
  }

  private refuse(handle: RequestHandle, reason: string): CallResult {
    const pending = this.accepting.get(handle);
    if (pending === undefined)
      return failure("Refused", "The user did not choose Accept for it.");
    this.accepting.delete(handle);
    pending.cancelTimeout();
    this.deps.log(
      `${this.deps.product.name} refused request ${short(handle)}: ${reason} Nothing is sent.`,
    );
    pending.reject(new Error(reason));
    return { ok: true };
  }

  private open(handle: RequestHandle): CallResult {
    if (!this.opening.delete(handle)) {
      this.deps.log(
        `Ignored ${this.deps.product.name}'s request to open: the user did not ask for it.`,
      );
      return failure("Refused", "The user did not open it.");
    }
    this.deps.showProduct();
    return { ok: true };
  }

  /** A sealed request arrived. Kept only when it is new, authentic and in time. */
  private receiveRequest(sealed: string): void {
    if (this.stopped) return;
    const opened = openRequest(this.deps.auth, sealed, (key) =>
      this.deps.contacts.byIdentity(key),
    );
    if (!opened.ok) {
      this.deps.log(`Ignored a request: ${opened.reason}.`);
      return;
    }
    const { peer, context, payload } = opened;
    const exact = requestDigest(context, payload);
    // Another tab of this user may have kept it already.
    this.received.reload();
    const known = this.received.find(peer.id, context.requestId);
    if (known !== undefined) {
      if (known.digest !== exact)
        this.deps.log(
          `Ignored a request from ${peer.name}: another request of theirs has its id.`,
        );
      return;
    }
    const label = short(context.requestId);
    if (context.product.id !== this.deps.product.id) {
      this.deps.log(
        `Ignored request ${label}: ${context.product.id} is not installed.`,
      );
      return;
    }
    const now = this.deps.now();
    if (
      now >= context.deadline ||
      context.deadline <= context.createdAt ||
      context.deadline - context.createdAt > TTL_MAX_MS ||
      context.deadline - now > TTL_MAX_MS
    ) {
      this.deps.log(`Ignored request ${label}: its deadline passed.`);
      return;
    }
    if (!this.deps.notifications.scoped()) return;
    // The same for every tab of this user, so their lists agree; a product
    // cannot work it out, as it never sees the identities or the request id.
    const handle = toHex(
      digest(
        "truapi-web-host/received-handle/1",
        this.deps.auth.identityFor(peer).signingPublic,
        context.from,
        context.requestId,
      ).subarray(0, 16),
    );
    this.deps.notifications.reload();
    if (this.deps.notifications.get(handle) !== undefined) return;
    this.received.add({
      handle,
      from: peer.id,
      context,
      payload,
      digest: exact,
      replied: null,
    });
    this.deps.notifications.add({
      id: handle,
      kind: "invitation",
      sender: { name: peer.name, handle: peer.handle },
      product: context.product,
      purpose: context.purpose,
      receivedAt: now,
      expiresAt: context.deadline,
      seenAt: null,
      state: { tag: "pending" },
    } satisfies HostNotification);
    this.deps.log(
      `Request ${label} from ${peer.name}: authenticated, kept. Nothing is sent back.`,
    );
  }

  /** A sealed reply arrived on a return topic. Handed to the product only once authenticated. */
  private receiveReply(topic: string, sealed: string): void {
    if (this.stopped) return;
    const item = this.outbox.byTopic(topic);
    if (item === undefined) return;
    const label = short(item.handle);
    const status = this.outbox.status(item);
    if (status !== "waiting") {
      this.deps.log(`Ignored a reply to request ${label}: it is ${status}.`);
      return;
    }
    const peer: Person | undefined = this.deps.contacts.byIdentity(
      item.context.to,
    );
    if (peer === undefined) {
      this.deps.log(
        `Ignored a reply to request ${label}: the contact is gone.`,
      );
      return;
    }
    const opened = openReply(
      this.deps.auth,
      {
        peer,
        productId: this.deps.product.id,
        requestId: item.context.requestId,
        requestDigest: item.digest,
        replyKey: x25519Pair(fromHex(item.replySecret)),
      },
      sealed,
    );
    if (!opened.ok) {
      this.deps.log(`Ignored a reply to request ${label}: ${opened.reason}.`);
      return;
    }
    if (!this.outbox.addReply(item.handle, opened.payload)) return;
    this.deps.log(
      `Request ${label}: authenticated a reply from ${peer.name}; ${this.deps.product.name} checks it.`,
    );
    this.product?.post({
      kind: "response",
      handle: item.handle,
      payload: opened.payload,
    });
  }

  private scheduleExpiry(): void {
    this.cancelExpiry?.();
    this.cancelExpiry = null;
    if (this.stopped) return;
    const next = this.outbox.nextDeadline();
    if (next === null) return;
    this.cancelExpiry = this.deps.schedule(
      () => this.handleDeadlines(),
      Math.max(0, next - this.deps.now()),
    );
  }

  /** Stop posting what passed its deadline since the last check, and tell the product. */
  private handleDeadlines(): void {
    const now = this.deps.now();
    for (const item of this.outbox.list()) {
      const { deadline } = item.context;
      if (
        item.state.tag === "waiting" &&
        deadline > this.deadlinesHandledTo &&
        deadline <= now
      ) {
        this.stopPosting(item);
        this.deps.log(
          `Request ${short(item.handle)} expired; reposting stopped.`,
        );
        this.pushOutgoing(item);
      }
    }
    this.deadlinesHandledTo = now;
    this.scheduleExpiry();
  }
}
