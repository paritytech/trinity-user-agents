import type {
  HostNotification,
  NotificationStore,
} from "../notifications/notifications.js";
import {
  openReply,
  openRequest,
  requestDigest,
  requestSender,
  sealReply,
  sealRequest,
} from "./binding.js";
import {
  readPayload,
  readProductCall,
  readSendRequest,
  type CallResult,
  type FailureReason,
  type HostMessage,
  type OutgoingView,
  type ProductCall,
  type RequestHandle,
} from "./contract.js";
import { Outbox, isPending, type OutgoingRequest } from "./outbox.js";
import type {
  ContactService,
  ConversationAllowance,
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
 * host resolves the contact, asks its user, makes the request id and a reply
 * key pair, asks its delivery for a return route and watches it, then
 * authenticates, seals and posts the request. It has no deadline: it is
 * pending, and kept posted while this host runs, until the product completes
 * it or it is cancelled. When the delivery cannot keep it posted it is
 * paused, still pending, and the host says why. A reply on that route reaches
 * the product only once the host opened and authenticated it, and only while
 * the request is pending. The product's `complete` closes the request and
 * stops the reposts; nothing is sent.
 *
 * Receiving: an authenticated request from a contact, for this product, not
 * seen before, goes to Notifications. Nothing goes back:
 * not on arrival, not when seen, not on Dismiss. Accept is the user's intent,
 * handed with the payload to the product. Only the product's `reply` accepts
 * it; the host then authenticates, seals and posts the reply on the return
 * route the authenticated request names, once its delivery confirmed it can
 * reach the sender there.
 *
 * A reply has no deadline either. The recipient's host posts its sealed
 * reply when it replies, and posts the same reply again whenever the request
 * arrives again, including after a restart: the sender's host posts a pending request again on its
 * delivery's cadence, so a reply that was missed is answered by the next
 * copy of the request. The delivery keeps trying to post each reply for
 * {@link REPLY_POSTING_MS}, to ride out a post that failed; that is how long
 * it retries, not how long the reply counts. When the design has the
 * recipient resend is open; this is a local choice.
 *
 * A request's sender is not found yet, as when their chat key could not be
 * read: the request is paused with why, kept, and tried again every
 * {@link REACH_RETRY_MS}.
 */

/** How long the delivery keeps trying to post a reply after it was handed one. A local choice. */
export const REPLY_POSTING_MS = 60 * 60_000;

/** How often a request whose recipient the host could not find yet is tried again. */
export const REACH_RETRY_MS = 60_000;

/** What the host asks its user before a request leaves. */
export interface SendPrompt {
  product: Product;
  to: { name: string; handle: string };
  purpose: string;
}

/** What the host asks its user before an Accept reaches the product. */
export interface AcceptPrompt {
  product: Product;
  from: { name: string; handle: string };
  purpose: string;
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
  /**
   * Ask the user whether to accept, with what accepting costs them. Absent:
   * the Accept they chose in Notifications is the answer.
   */
  confirmAccept?(prompt: AcceptPrompt): Promise<boolean>;
  /** Bring this user's product to the front, at its own request. */
  showProduct(): void;
  /**
   * How the host gets a conversation account its allowance. Absent: this
   * host gives none, and `chatAccount` says so.
   */
  accounts?: ConversationAllowance;
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
  /** Deadlines up to this time were handled. */
  private stopped = false;
  /** Accepts the user chose that the product has not answered yet, by handle. */
  private readonly accepting = new Map<string, PendingAccept>();
  /** Replied requests the user opened, which the product may answer by coming to the front. */
  private readonly opening = new Set<string>();
  /** Received requests the attached product was told it replied to, by handle. */
  private readonly toldReplied = new Set<string>();
  private stopContacts: () => void = () => {};
  private readonly outgoingListeners = new Set<() => void>();
  /** The allowance each request's conversation account got, by handle, until its period ends. */
  private readonly allowances = new Map<
    string,
    { account: string; until: number }
  >();
  /** Requests whose recipient was not found, by handle: the retry that is due. */
  private readonly reachRetries = new Map<string, () => void>();

  constructor(private readonly deps: InvitationServiceDeps) {
    this.outbox = new Outbox(deps.storage, deps.keys.sent, deps.now);
    this.received = new ReceivedRequests(deps.storage, deps.keys.received);
  }

  start(): void {
    // A sender the host could not name yet is told about once it can.
    this.stopContacts = this.deps.contacts.onChange(() => {
      if (this.product !== null) this.tellReplied(this.product);
    });
    this.deps.delivery.listen({
      request: (sealed) => void this.receiveRequest(sealed),
      reply: (route, sealed) => void this.receiveReply(route, sealed),
      posting: (id, problem) => this.posting(id, problem),
    });
    for (const item of this.outbox.pending()) this.post(item);
  }

  /** Close the delivery and the product's port, and stop every timer. */
  stop(): void {
    if (this.stopped) return;
    this.stopped = true;
    this.stopContacts();
    for (const cancel of this.reachRetries.values()) cancel();
    this.reachRetries.clear();
    this.deps.delivery.close();
    this.detachProduct();
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
      if (isPending(item))
        for (const reply of item.replies)
          port.post({
            kind: "response",
            handle: item.handle,
            payload: reply.payload,
          });
    }
    this.toldReplied.clear();
    this.tellReplied(port);
  }

  /**
   * Tell `port`'s product each request it replied to that it was not told
   * about yet, once the host can name the sender. Only the view: nothing is
   * sent or posted again.
   */
  private tellReplied(port: ProductPort): void {
    for (const item of this.received.list()) {
      if (item.replied === null || this.toldReplied.has(item.handle)) continue;
      // Authenticated when it arrived; naming the sender needs no current key.
      const handle = this.deps.contacts.handleForIdentity(item.context.from);
      if (handle === null) continue;
      this.toldReplied.add(item.handle);
      port.post({
        kind: "replied",
        request: {
          handle: item.handle,
          sender: handle,
          repliedAt: item.replied.at,
        },
      });
    }
  }

  detachProduct(): void {
    this.product?.close();
    this.product = null;
    this.opening.clear();
    this.toldReplied.clear();
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
    if (item.state.tag !== "pending")
      return refuse(`This request was already ${item.state.tag}.`);
    const sender = this.deps.contacts.byIdentity(received.context.from);
    if (sender === undefined)
      return refuse("The sender is no longer one of your contacts.");
    const senderHandle = this.deps.contacts.handleFor(sender);
    if (senderHandle === null)
      return refuse("Your host cannot name the sender yet. Try again.");
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
    this.accepting.set(handle, pending);
    const settled = () => this.accepting.get(handle) !== pending;
    void (async () => {
      const confirmed =
        this.deps.confirmAccept === undefined ||
        (await this.deps.confirmAccept({
          product: this.deps.product,
          from: { name: sender.name, handle: sender.handle },
          purpose: received.context.purpose,
        }));
      if (settled()) return;
      if (!confirmed) {
        this.accepting.delete(handle);
        pending.reject(new Error("You did not accept it."));
        return;
      }
      pending.cancelTimeout = this.deps.schedule(() => {
        if (settled()) return;
        this.accepting.delete(handle);
        pending.reject(
          new Error(`${this.deps.product.name} did not answer. Try again.`),
        );
      }, INTENT_TIMEOUT_MS);
      this.deps.log(
        `Request ${short(handle)}: the user chose Accept; ${this.deps.product.name} decides.`,
      );
      product.post({
        kind: "acceptIntent",
        request: {
          handle,
          sender: senderHandle,
          payload: received.payload,
        },
      });
    })();
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
      if (!isPending(item)) this.stopPosting(item);
    if (this.product !== null) this.replay(this.product);
    this.outgoingChanged();
  }

  /** The user cancelled a pending request in the host: as the product's `cancel`, and the product is told. */
  requestCancel(handle: RequestHandle): CallResult {
    const result = this.cancel(handle);
    const item = this.outbox.get(handle);
    if (result.ok && item !== undefined) this.pushOutgoing(item);
    return result;
  }

  /** The person with this identity once their published key is read, or undefined. */
  private async learnIdentity(identity: string): Promise<Person | undefined> {
    if (this.deps.contacts.learn === undefined) return undefined;
    await this.deps.contacts.learn({ identity });
    return this.deps.contacts.byIdentity(identity);
  }

  /** The delivery says whether it can keep a post up; a request it cannot is paused, and the product told. */
  private posting(id: string, problem: string | null): void {
    if (this.stopped || !this.outbox.setPaused(id, problem)) return;
    const item = this.outbox.get(id)!;
    this.deps.log(
      problem === null
        ? `Request ${short(id)} is posted again.`
        : `Request ${short(id)} is paused: ${problem}`,
    );
    this.pushOutgoing(item);
  }

  private post(item: OutgoingRequest): void {
    this.deps.delivery.watch(item.context.returnRoute);
    const person = this.deps.contacts.byIdentity(item.context.to);
    if (person !== undefined) {
      // A pause kept from before, such as across a restart, is cleared first:
      // the delivery reports again if it still cannot post.
      if (item.paused !== null) this.posting(item.handle, null);
      this.deps.delivery.postRequest(person.identity, item.handle, item.sealed);
      return;
    }
    // Read their published key first; only then can the request be posted.
    const { handle } = item;
    void this.learnIdentity(item.context.to).then((learnt) => {
      // Another tab, or the user, may have closed it meanwhile.
      const current = this.outbox.get(handle);
      if (this.stopped || current === undefined || !isPending(current)) return;
      if (learnt !== undefined) {
        this.posting(handle, null);
        this.deps.delivery.postRequest(learnt.identity, handle, current.sealed);
        return;
      }
      const why =
        this.deps.contacts.problem(current.contact) ??
        "their chat key is not read yet";
      this.posting(handle, `your host cannot find the contact now: ${why}`);
      this.reachRetries.get(handle)?.();
      this.reachRetries.set(
        handle,
        this.deps.schedule(() => {
          this.reachRetries.delete(handle);
          const again = this.outbox.get(handle);
          if (!this.stopped && again !== undefined && isPending(again))
            this.post(again);
        }, REACH_RETRY_MS),
      );
    });
  }

  /** Hand the delivery a reply to post, which it keeps trying for {@link REPLY_POSTING_MS}. */
  private postReply(item: ReceivedRequest, replied: { sealed: string }): void {
    this.deps.delivery.postReply(
      item.context.returnRoute,
      item.handle,
      replied.sealed,
      this.deps.now() + REPLY_POSTING_MS,
    );
  }

  private stopPosting(item: OutgoingRequest): void {
    this.reachRetries.get(item.handle)?.();
    this.reachRetries.delete(item.handle);
    this.deps.delivery.withdraw(item.handle);
    this.deps.delivery.unwatch(item.context.returnRoute);
  }

  private outgoingView(item: OutgoingRequest): OutgoingView {
    return {
      handle: item.handle,
      contact: item.contact,
      purpose: item.context.purpose,
      createdAt: item.context.createdAt,
      status: this.outbox.status(item),
      ...(item.state.tag === "waiting" ? {} : { closedAt: item.state.at }),
    };
  }

  private pushOutgoing(item: OutgoingRequest): void {
    this.product?.post({ kind: "outgoing", request: this.outgoingView(item) });
    this.outgoingChanged();
  }

  /** Calls back whenever a sent request was added or changed. */
  onOutgoing(listener: () => void): () => void {
    this.outgoingListeners.add(listener);
    return () => this.outgoingListeners.delete(listener);
  }

  private outgoingChanged(): void {
    for (const listener of [...this.outgoingListeners]) listener();
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
      case "chatAccount":
        return this.chatAccount(call.handle, call.index);
    }
  }

  private async send(value: unknown): Promise<CallResult> {
    const request = readSendRequest(value);
    if (!request.ok) return failure("Invalid", request.reason);
    const { contact, purpose, payload } = request.value;
    const product = this.deps.product;
    if (this.deps.contacts.resolve(contact) === null)
      await this.deps.contacts.learn?.({ contact });
    const person = this.deps.contacts.resolve(contact);
    if (person === null) {
      const problem = this.deps.contacts.problem(contact);
      if (problem === null)
        return failure("Unknown", "This contact names nobody here.");
      this.deps.log(`${product.name}'s request was not sent: ${problem}`);
      return failure("Unknown", "Your host cannot reach this contact now.");
    }
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
        returnRoute: this.deps.delivery.returnRoute(),
        replyKey: toHex(replyKey.public),
      },
      payload,
      random,
    );
    const tooLarge = this.tooLarge("request", sealed);
    if (tooLarge !== null) return failure("Invalid", tooLarge);
    const confirmed = await this.deps.confirmSend({
      product,
      to: { name: person.name, handle: person.handle },
      purpose,
    });
    if (!confirmed) return failure("Dismissed", "The user did not send it.");
    if (this.stopped) return failure("Refused", "This user's host stopped.");
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
      paused: null,
    };
    this.outbox.add(item);
    this.outgoingChanged();
    this.deps.log(
      `Request ${short(item.handle)}: watching its return route, then posting it, sealed for ${person.name}.`,
    );
    this.post(item);
    return { ok: true, request: this.outgoingView(item) };
  }

  /** A pending request of the product's, or why the call is refused. */
  private waitingItem(
    handle: RequestHandle,
  ): OutgoingRequest | { failure: CallResult } {
    const item = this.outbox.get(handle);
    if (item === undefined)
      return {
        failure: failure("Unknown", "No sent request has this handle."),
      };
    return isPending(item)
      ? item
      : {
          failure: failure(
            "Refused",
            `This request is ${this.outbox.status(item)}.`,
          ),
        };
  }

  private complete(handle: RequestHandle): CallResult {
    const item = this.waitingItem(handle);
    if ("failure" in item) return item.failure;
    if (item.replies.length === 0)
      return failure("Refused", "No authenticated reply has arrived.");
    this.outbox.close(handle, { tag: "completed", at: this.deps.now() });
    this.outgoingChanged();
    this.stopPosting(item);
    this.deps.log(
      `Request ${short(handle)} completed by ${this.deps.product.name}; reposting stopped. Nothing is sent.`,
    );
    return { ok: true, request: this.outgoingView(item) };
  }

  private cancel(handle: RequestHandle): CallResult {
    const item = this.waitingItem(handle);
    if ("failure" in item) return item.failure;
    this.outbox.close(handle, { tag: "cancelled", at: this.deps.now() });
    this.outgoingChanged();
    this.stopPosting(item);
    this.deps.log(
      `Request ${short(handle)} cancelled; reposting stopped. Nothing is sent.`,
    );
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
    const sealed = sealReply(
      this.deps.auth,
      sender,
      { context: received.context, digest: received.digest },
      payload,
      this.deps.random,
    );
    // Before the request counts as answered, so a reply too large changes nothing.
    const tooLarge = this.tooLarge("reply", sealed);
    if (tooLarge !== null) return settle(failure("Invalid", tooLarge));
    const answered = this.deps.notifications.accept(handle);
    if (!answered.ok) return settle(failure("Refused", answered.reason));
    if (!answered.changed)
      return settle(failure("Refused", "This request was already answered."));
    const replied = { sealed, at: this.deps.now() };
    this.received.markReplied(handle, replied.sealed, replied.at);
    this.toldReplied.add(handle);
    this.postReply(received, replied);
    this.deps.log(
      `${this.deps.product.name} replied to request ${short(handle)}; the host sealed it for ${sender.name} and posted it on the request's return route.`,
    );
    if (open) this.deps.showProduct();
    return settle({ ok: true });
  }

  /** Why `sealed` is too large for the delivery, for the product to act on, or null. */
  private tooLarge(kind: "request" | "reply", sealed: string): string | null {
    const max = this.deps.delivery.maxPostBytes;
    const bytes = (sealed.length - 2) / 2;
    return max === undefined || bytes <= max
      ? null
      : `The ${kind} is ${bytes} bytes once sealed, and its delivery carries at most ${max}. Send a smaller payload.`;
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

  /**
   * Allow this product's own account at `Raw(index)`, for the conversation
   * `handle` set up, to post in the current period.
   *
   * Consent: the user's Send of that request, once an authenticated reply
   * arrived and while it is not cancelled, or their Accept of it. The first
   * index the product names is bound to the request and kept; another is
   * refused, so one Send or Accept funds one account. Rollover: the account
   * gets at most one slot per allowance period; the product asks again in a
   * later period it posts in, and an answer is reused until its period ends.
   * The product id is this service's, never the caller's word.
   */
  private async chatAccount(
    handle: RequestHandle,
    index: string,
  ): Promise<CallResult> {
    const sent = this.outbox.get(handle);
    const received = this.received.get(handle);
    if (sent === undefined && received === undefined)
      return failure("Unknown", "No request has this handle.");
    if (sent !== undefined) {
      if (sent.state.tag === "cancelled")
        return failure("Refused", "This request was cancelled.");
      if (sent.replies.length === 0)
        return failure("Refused", "No authenticated reply has arrived.");
    } else if (received!.replied === null && !this.accepting.has(handle))
      return failure("Refused", "The user did not accept this request.");
    const bound = sent?.chatIndex ?? received?.chatIndex;
    if (bound !== undefined && bound !== index)
      return failure(
        "Refused",
        "This conversation already has another account.",
      );
    const kept = this.allowances.get(handle);
    if (kept !== undefined && kept.until > this.deps.now())
      return { ok: true, allowance: kept };
    if (this.deps.accounts === undefined)
      return failure(
        "Refused",
        "This host gives conversation accounts no Statement Store allowance.",
      );
    if (sent !== undefined) this.outbox.setChatIndex(handle, index);
    else this.received.setChatIndex(handle, index);
    try {
      const allowance = await this.deps.accounts.allow(
        this.deps.product.id,
        fromHex(index),
      );
      this.allowances.set(handle, allowance);
      this.deps.log(
        `${this.deps.product.name}'s account ${short(allowance.account)} for request ${short(handle)} may post until ${new Date(allowance.until).toISOString()}.`,
      );
      return { ok: true, allowance };
    } catch (error) {
      const reason = error instanceof Error ? error.message : String(error);
      this.deps.log(
        `${this.deps.product.name}'s account for request ${short(handle)} got no allowance: ${reason}`,
      );
      return failure("Refused", reason);
    }
  }

  /** A sealed request arrived. Kept only when it is new and authentic. */
  private async receiveRequest(sealed: string): Promise<void> {
    if (this.stopped) return;
    // A sender whose key is not read yet: read it, then check them with it.
    const sender =
      this.deps.contacts.learn && requestSender(this.deps.auth, sealed);
    if (sender && this.deps.contacts.byIdentity(sender) === undefined) {
      await this.deps.contacts.learn?.({ identity: sender });
      if (this.stopped) return;
    }
    const opened = openRequest(this.deps.auth, sealed, (key) =>
      this.deps.contacts.byIdentity(key),
    );
    if (!opened.ok) {
      // On an inbox other hosts share, most posts are sealed to someone else.
      if (opened.sealedElsewhere) return;
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
      else if (known.replied !== null) {
        // The sender still waits, so it did not get the reply: post it again.
        this.postReply(known, known.replied);
        this.deps.log(
          `Request ${short(context.requestId)} from ${peer.name} came again; its reply is posted again.`,
        );
      }
      return;
    }
    const label = short(context.requestId);
    if (context.product.id !== this.deps.product.id) {
      this.deps.log(
        `Ignored request ${label}: ${context.product.id} is not installed.`,
      );
      return;
    }
    if (!this.deps.delivery.isReturnRoute(context.returnRoute, peer.identity)) {
      this.deps.log(
        `Ignored request ${label}: its return route does not reach ${peer.name}.`,
      );
      return;
    }
    const now = this.deps.now();
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
      expiresAt: null,
      seenAt: null,
      state: { tag: "pending" },
    } satisfies HostNotification);
    this.deps.log(
      `Request ${label} from ${peer.name}: authenticated, kept. Nothing is sent back.`,
    );
  }

  /**
   * A sealed reply arrived on a return route. Every request on that route is
   * tried, since a route may be shared; the one whose reply key opens it
   * decides. Handed to the product only once authenticated, and only while
   * that request is pending. A request that is not pending is tried only
   * when its contact is known already, so a closed request costs no read.
   */
  private async receiveReply(route: string, sealed: string): Promise<void> {
    for (const { handle } of this.outbox.withRoute(route)) {
      if (this.stopped) return;
      let item = this.outbox.get(handle);
      if (item === undefined) continue;
      const label = short(handle);
      let peer = this.deps.contacts.byIdentity(item.context.to);
      if (peer === undefined && isPending(item)) {
        peer = await this.learnIdentity(item.context.to);
        if (this.stopped) return;
        // It may have been completed or cancelled while the key was read.
        item = this.outbox.get(handle);
        if (item === undefined) continue;
      }
      if (peer === undefined) continue;
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
      if (!opened.ok && opened.sealedElsewhere) continue;
      if (!opened.ok) {
        this.deps.log(`Ignored a reply to request ${label}: ${opened.reason}.`);
        return;
      }
      if (!isPending(item)) {
        this.deps.log(
          `Ignored a reply to request ${label}: it is ${this.outbox.status(item)}.`,
        );
        return;
      }
      if (!this.outbox.addReply(handle, opened.payload)) return;
      this.deps.log(
        `Request ${label}: authenticated a reply from ${peer.name}; ${this.deps.product.name} checks it.`,
      );
      this.product?.post({
        kind: "response",
        handle,
        payload: opened.payload,
      });
      return;
    }
  }
}
