import type { HostNotification, NotificationStore } from "../notifications.js";
import {
  checkProof,
  envelopeDigest,
  issueProof,
  openEnvelope,
  responseDigest,
  sealEnvelope,
  type Envelope,
  type HostKeys,
} from "./binding.js";
import {
  readPayload,
  readProductCall,
  readSendRequest,
  TTL_MAX_MS,
  type AcceptedView,
  type CallResult,
  type FailureReason,
  type HostMessage,
  type OutgoingView,
  type ProductCall,
} from "./contract.js";
import { Outbox, type OutgoingInvitation } from "./outbox.js";
import type {
  ContactService,
  InvitationDelivery,
  Person,
  Product,
} from "./ports.js";
import { ReceivedInvitations, type ReceivedInvitation } from "./received.js";
import { digest, toHex, type RandomBytes } from "./sealing.js";

/**
 * One user's host side of JIT invitations (`contract.ts`). It serves its
 * product, asks its user, keeps what was sent and received, and reaches other
 * hosts only through an {@link InvitationDelivery}.
 *
 * Sending: the product picks a contact and gets an opaque reference; it sends
 * its invitation to that reference. The host resolves it, asks its user,
 * signs and seals the invitation for that person, and posts it again until
 * the product completes or cancels it, or it expires. A response reaches the
 * product, not the host; the product brings it with its proof to `verify`,
 * checks its own protocol, then calls `complete`.
 *
 * Receiving: an invitation from a contact, for this product, not expired and
 * not seen before, goes to Notifications. Nothing goes back: not on arrival,
 * not when seen, not on Dismiss. Accept is the user's intent, handed with the
 * invitation to the product. Only the product's `accept`, with its exact
 * response, accepts it; the host then answers the proof, which only the
 * sender's host can read.
 */

/** What the host asks its user before an invitation leaves. */
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
  product: Product;
  /** This host's own keys. */
  self: HostKeys;
  contacts: ContactService;
  /** This user's Notifications list. */
  notifications: NotificationStore;
  storage: Storage;
  /** Where this user's sent and received invitations are kept. */
  keys: { sent: string; received: string };
  delivery: InvitationDelivery;
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

/** The host's id for an invitation from `from` with `requestId`. */
export const receivedId = (from: string, requestId: string): string =>
  toHex(digest("truapi-web-host/received-invitation/0", from, requestId)).slice(
    2,
    34,
  );

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
  readonly received: ReceivedInvitations;
  private product: ProductPort | null = null;
  private cancelExpiry: (() => void) | null = null;
  /** Expiries up to this time were handled. */
  private expiryHandledTo: number;
  private stopped = false;
  /** Accepts the user chose that the product has not answered yet, by invitation id. */
  private readonly accepting = new Map<string, PendingAccept>();
  /** Accepted invitations the user opened, which the product may answer by coming to the front. */
  private readonly opening = new Set<string>();

  constructor(private readonly deps: InvitationServiceDeps) {
    this.outbox = new Outbox(deps.storage, deps.keys.sent, deps.now);
    this.received = new ReceivedInvitations(deps.storage, deps.keys.received);
    this.expiryHandledTo = deps.now();
  }

  start(): void {
    this.deps.delivery.listen((sealed) => this.receive(sealed));
    for (const item of this.outbox.waiting()) this.post(item);
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
    for (const item of this.outbox.list())
      port.post({ kind: "outgoing", invitation: this.outgoingView(item) });
    for (const item of this.received.list()) {
      const view = this.acceptedView(item);
      if (view !== null) port.post({ kind: "accepted", invitation: view });
    }
  }

  detachProduct(): void {
    this.product?.close();
    this.product = null;
    this.opening.clear();
    this.failAccepts(`${this.deps.product.name} closed before it answered.`);
  }

  private failAccepts(reason: string): void {
    for (const [id, pending] of [...this.accepting]) {
      this.accepting.delete(id);
      pending.cancelTimeout();
      pending.reject(new Error(reason));
    }
  }

  /** Whether `id` is an invitation this host received, so this service answers it. */
  owns(id: string): boolean {
    return this.received.get(id) !== undefined;
  }

  /**
   * The user chose Accept for a received invitation. The product gets the
   * intent with the invitation, and this resolves once it accepted. Rejects,
   * with a message for the user, when the product refuses, does not answer in
   * time, or is not open; then nothing is accepted and nothing is sent.
   */
  requestAccept(id: string): Promise<void> {
    const waiting = this.accepting.get(id);
    if (waiting !== undefined) return waiting.done;
    const received = this.received.get(id);
    const item = this.deps.notifications.get(id);
    const refuse = (reason: string) => Promise.reject(new Error(reason));
    if (this.stopped) return refuse("This user's host stopped.");
    if (received === undefined || item === undefined)
      return refuse("This is not an invitation to you.");
    if (this.deps.now() >= item.expiresAt && item.state.tag === "pending")
      return refuse("This request expired.");
    if (item.state.tag !== "pending")
      return refuse(`This request was already ${item.state.tag}.`);
    const sender = this.deps.contacts.bySigningKey(received.from.signingPublic);
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
      if (this.accepting.get(id) !== pending) return;
      this.accepting.delete(id);
      pending.reject(
        new Error(`${this.deps.product.name} did not answer. Try again.`),
      );
    }, INTENT_TIMEOUT_MS);
    this.accepting.set(id, pending);
    this.deps.log(
      `Invitation ${short(id)}: the user chose Accept; ${this.deps.product.name} decides.`,
    );
    product.post({
      kind: "acceptIntent",
      invitation: {
        invitationId: id,
        sender: this.deps.contacts.refFor(this.deps.product, sender),
        purpose: item.purpose,
        invitation: received.envelope.invitation,
      },
    });
    return done;
  }

  /** The user opened an accepted invitation: the product may come to the front for it. */
  requestOpen(id: string): void {
    const item = this.deps.notifications.get(id);
    if (
      this.product === null ||
      item?.state.tag !== "accepted" ||
      !this.owns(id)
    )
      return;
    this.opening.add(id);
    this.product.post({ kind: "openIntent", invitationId: id });
  }

  /** Read every list again after another tab wrote them. */
  reload(): void {
    this.deps.notifications.reload();
    this.outbox.reload();
    this.received.reload();
    if (this.product !== null) this.replay(this.product);
    this.scheduleExpiry();
  }

  private post(item: OutgoingInvitation): void {
    this.deps.delivery.post(
      item.to.keys,
      item.requestId,
      item.sealed,
      item.envelope.invitation.expiresAt,
    );
  }

  private outgoingView(item: OutgoingInvitation): OutgoingView {
    return {
      requestId: item.requestId,
      contact: item.contact,
      purpose: item.envelope.purpose,
      createdAt: item.envelope.createdAt,
      expiresAt: item.envelope.invitation.expiresAt,
      status: this.outbox.status(item),
      ...(item.state.tag === "waiting" ? {} : { closedAt: item.state.at }),
    };
  }

  /** What the product is told again of an invitation it accepted, or null. */
  private acceptedView(item: ReceivedInvitation): AcceptedView | null {
    const sender = this.deps.contacts.bySigningKey(item.from.signingPublic);
    if (item.accepted === null || sender === undefined) return null;
    return {
      invitationId: item.invitationId,
      sender: this.deps.contacts.refFor(this.deps.product, sender),
      purpose: item.envelope.purpose,
      invitation: item.envelope.invitation,
      acceptedAt: item.accepted.at,
    };
  }

  private pushOutgoing(item: OutgoingInvitation): void {
    this.product?.post({
      kind: "outgoing",
      invitation: this.outgoingView(item),
    });
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
      case "pickContact": {
        const contact = await this.deps.contacts.pick(this.deps.product);
        return contact === null
          ? failure("Dismissed", "No contact was chosen.")
          : { ok: true, contact };
      }
      case "send":
        return this.send(call.request);
      case "verify":
        return this.verify(call.requestId, call.response, call.proof);
      case "complete":
        return this.complete(call.requestId, call.response);
      case "cancel":
        return this.cancel(call.requestId);
      case "accept":
        return this.productAccepted(
          call.invitationId,
          call.response,
          call.open,
        );
      case "refuse":
        return this.productRefused(call.invitationId, call.reason);
      case "open":
        return this.productOpens(call.invitationId);
    }
  }

  private async send(value: unknown): Promise<CallResult> {
    const request = readSendRequest(value);
    if (!request.ok) return failure("Invalid", request.reason);
    const { contact, purpose, ttlMs, invitation } = request.value;
    const product = this.deps.product;
    const person = this.deps.contacts.resolve(product, contact);
    if (person === null)
      return failure("Unknown", "This contact names nobody here.");
    const inUse = () => this.outbox.get(invitation.requestId) !== undefined;
    if (inUse()) return failure("Invalid", "The request id is already in use.");
    const confirmed = await this.deps.confirmSend({
      product,
      to: { name: person.name, handle: person.handle },
      purpose,
      ttlMs,
    });
    if (!confirmed) return failure("Dismissed", "The user did not send it.");
    if (this.stopped) return failure("Refused", "This user's host stopped.");
    if (inUse()) return failure("Invalid", "The request id is already in use.");
    const now = this.deps.now();
    const envelope: Envelope = {
      product,
      purpose,
      createdAt: now,
      invitation: { ...invitation, expiresAt: now + ttlMs },
    };
    const item: OutgoingInvitation = {
      requestId: invitation.requestId,
      to: { id: person.id, keys: person.keys },
      contact,
      envelope,
      sealed: sealEnvelope(
        this.deps.self,
        person.keys,
        envelope,
        this.deps.random,
      ),
      verified: [],
      state: { tag: "waiting" },
    };
    this.outbox.add(item);
    this.deps.log(
      `Invitation ${short(item.requestId)} sealed for ${person.name} and posted.`,
    );
    this.post(item);
    this.scheduleExpiry();
    return { ok: true, invitation: this.outgoingView(item) };
  }

  /** A waiting invitation of the product's, or why the call is refused. */
  private waitingItem(
    requestId: string,
  ): OutgoingInvitation | { failure: CallResult } {
    const item = this.outbox.get(requestId);
    if (item === undefined)
      return { failure: failure("Unknown", "No invitation has this id.") };
    const status = this.outbox.status(item);
    return status === "waiting"
      ? item
      : { failure: failure("Refused", `This invitation is ${status}.`) };
  }

  private verify(
    requestId: string,
    response: string,
    proof: string,
  ): CallResult {
    const item = this.waitingItem(requestId);
    if ("failure" in item) return item.failure;
    const read = readPayload(response, "response");
    if (!read.ok) return failure("Invalid", read.reason);
    const problem = checkProof(
      this.deps.self,
      item.to.keys,
      item.envelope,
      response,
      proof,
    );
    if (problem !== null) {
      this.deps.log(
        `A response to invitation ${short(requestId)} did not verify: ${problem}.`,
      );
      return failure("Rejected", problem);
    }
    this.outbox.verified(requestId, responseDigest(response));
    this.deps.log(
      `Verified a response to invitation ${short(requestId)} from the contact invited.`,
    );
    return { ok: true };
  }

  private complete(requestId: string, response: string): CallResult {
    const item = this.waitingItem(requestId);
    if ("failure" in item) return item.failure;
    const digested = responseDigest(response);
    if (!item.verified.includes(digested))
      return failure("Refused", "This response was not verified.");
    this.outbox.close(requestId, {
      tag: "completed",
      at: this.deps.now(),
      response: digested,
    });
    this.deps.delivery.withdraw(requestId);
    this.deps.log(
      `Invitation ${short(requestId)} completed by ${this.deps.product.name}; reposting stopped.`,
    );
    this.scheduleExpiry();
    return { ok: true, invitation: this.outgoingView(item) };
  }

  private cancel(requestId: string): CallResult {
    const item = this.waitingItem(requestId);
    if ("failure" in item) return item.failure;
    this.outbox.close(requestId, { tag: "cancelled", at: this.deps.now() });
    this.deps.delivery.withdraw(requestId);
    this.deps.log(
      `Invitation ${short(requestId)} cancelled; reposting stopped. Nothing is sent to the recipient.`,
    );
    this.scheduleExpiry();
    return { ok: true, invitation: this.outgoingView(item) };
  }

  /** The product accepted, with its response, an invitation the user chose to accept. */
  private productAccepted(
    id: string,
    response: string,
    open: boolean,
  ): CallResult {
    const pending = this.accepting.get(id);
    if (pending === undefined) {
      this.deps.log(
        `Ignored ${this.deps.product.name}'s accept of ${short(id)}: the user did not choose it.`,
      );
      return failure("Refused", "The user did not choose Accept for it.");
    }
    const settle = (result: CallResult): CallResult => {
      this.accepting.delete(id);
      pending.cancelTimeout();
      if (result.ok) pending.resolve();
      else pending.reject(new Error(result.detail));
      return result;
    };
    const read = readPayload(response, "response");
    if (!read.ok) return settle(failure("Invalid", read.reason));
    const received = this.received.get(id);
    const sender =
      received === undefined
        ? undefined
        : this.deps.contacts.bySigningKey(received.from.signingPublic);
    if (received === undefined || sender === undefined)
      return settle(failure("Unknown", "The sender is no longer a contact."));
    const answered = this.deps.notifications.accept(id);
    if (!answered.ok) return settle(failure("Refused", answered.reason));
    const proof = issueProof(
      this.deps.self,
      sender.keys,
      received.envelope,
      response,
      this.deps.random,
    );
    this.received.markAccepted(id, responseDigest(response), this.deps.now());
    this.deps.log(
      `${this.deps.product.name} accepted invitation ${short(id)}; the host bound its proof to that response.`,
    );
    if (open) this.deps.showProduct();
    return settle({ ok: true, proof });
  }

  private productRefused(id: string, reason: string): CallResult {
    const pending = this.accepting.get(id);
    if (pending === undefined)
      return failure("Refused", "The user did not choose Accept for it.");
    this.accepting.delete(id);
    pending.cancelTimeout();
    this.deps.log(
      `${this.deps.product.name} refused invitation ${short(id)}: ${reason} Nothing is sent.`,
    );
    pending.reject(new Error(reason));
    return { ok: true };
  }

  private productOpens(id: string): CallResult {
    if (!this.opening.delete(id)) {
      this.deps.log(
        `Ignored ${this.deps.product.name}'s request to open: the user did not ask for it.`,
      );
      return failure("Refused", "The user did not open it.");
    }
    this.deps.showProduct();
    return { ok: true };
  }

  /** Something arrived for this host. Kept only when it is a new, valid invitation. */
  private receive(sealed: string): void {
    if (this.stopped) return;
    const opened = openEnvelope(
      this.deps.self,
      sealed,
      (key) => this.deps.contacts.bySigningKey(key)?.keys,
    );
    if (!opened.ok) {
      this.deps.log(`Ignored an invitation: ${opened.reason}.`);
      return;
    }
    const sender = this.deps.contacts.bySigningKey(opened.from) as Person;
    const { envelope } = opened;
    const id = receivedId(opened.from, envelope.invitation.requestId);
    const label = short(id);
    const known = this.received.get(id);
    if (known !== undefined) {
      if (envelopeDigest(known.envelope) !== envelopeDigest(envelope))
        this.deps.log(
          `Ignored invitation ${label}: another invitation from ${sender.name} has its request id.`,
        );
      return;
    }
    if (envelope.product.id !== this.deps.product.id) {
      this.deps.log(
        `Ignored invitation ${label}: ${envelope.product.id} is not installed.`,
      );
      return;
    }
    const now = this.deps.now();
    const { expiresAt } = envelope.invitation;
    if (now >= expiresAt || expiresAt - envelope.createdAt > TTL_MAX_MS) {
      this.deps.log(`Ignored invitation ${label}: it expired.`);
      return;
    }
    if (!this.deps.notifications.scoped()) return;
    if (this.deps.notifications.get(id) !== undefined) return;
    this.received.add({
      invitationId: id,
      from: { id: sender.id, signingPublic: opened.from },
      envelope,
      accepted: null,
    });
    this.deps.notifications.add({
      id,
      kind: "invitation",
      sender: { name: sender.name, handle: sender.handle },
      product: envelope.product,
      purpose: envelope.purpose,
      receivedAt: now,
      expiresAt,
      seenAt: null,
      state: { tag: "pending" },
    } satisfies HostNotification);
    this.deps.log(
      `Invitation ${label} from ${sender.name}: signature checked, kept. Nothing is sent back.`,
    );
  }

  private scheduleExpiry(): void {
    this.cancelExpiry?.();
    this.cancelExpiry = null;
    if (this.stopped) return;
    const next = this.outbox.nextExpiry();
    if (next === null) return;
    this.cancelExpiry = this.deps.schedule(
      () => this.handleExpiries(),
      Math.max(0, next - this.deps.now()),
    );
  }

  /** Stop reposting what expired since the last check, and tell the product. */
  private handleExpiries(): void {
    const now = this.deps.now();
    for (const item of this.outbox.list()) {
      const { expiresAt } = item.envelope.invitation;
      if (
        item.state.tag === "waiting" &&
        expiresAt > this.expiryHandledTo &&
        expiresAt <= now
      ) {
        this.deps.delivery.withdraw(item.requestId);
        this.deps.log(
          `Invitation ${short(item.requestId)} expired; reposting stopped.`,
        );
        this.pushOutgoing(item);
      }
    }
    this.expiryHandledTo = now;
    this.scheduleExpiry();
  }
}
