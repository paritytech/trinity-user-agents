import type { ContactHandle, ContactPickOutcome } from "@parity/truapi";

/**
 * DRAFT, NOT TRUAPI. A proposed host request/reply service between a product
 * and its host, after the "Establish Connection" v3 design: a product sends a
 * request to a contact and the contact's product replies through its host.
 * Chat invitations are its first use. Nothing here is agreed.
 *
 * It reuses TrUAPI's Contacts types: a contact is named by a
 * {@link ContactHandle}, and `pickContact` answers a
 * {@link ContactPickOutcome}, as `contacts.pick` does. The handle it answers is
 * minted by the dev tools, not the core, because the core gives the host no
 * way to resolve its own handles outside a transaction.
 *
 * Who owns what:
 * - The product owns its payload: protocol starter material, keys and
 *   challenges inside it, and its private keys. The host carries the payload
 *   unchanged and never reads it.
 * - The sender's host owns the pending request: it resolves the contact,
 *   makes the request id, the return topic `T_SETUP` and a reply key pair,
 *   authenticates and seals the request, posts it until the deadline, and
 *   watches `T_SETUP`. It hands the product a reply's payload only after it
 *   authenticated the reply against the contact chosen, the exact request and
 *   the exact payload.
 * - The recipient's host authenticates the request and renders who sent it.
 *   On the product's `reply` it authenticates, seals and posts the reply on
 *   the request's `T_SETUP`. The product never sees proofs, routes, reply keys
 *   or anyone's identity: only payloads and opaque handles.
 * - `complete` is the sender product's local word that it checked the reply
 *   and is connected. It stops the reposts; nothing is sent to the recipient.
 * - Decline is silent: the recipient's host sends nothing back, ever.
 *
 * Messages travel over a `MessagePort` the product's frame hands its host in
 * a hello. Every call carries an `id`; the host answers each with one
 * `result`.
 */

/** `type` of the window message a product frame sends to hand its host a port. */
export const PRODUCT_HELLO = "host-requests-product";
/** Version of the messages on that port. */
export const PRODUCT_PROTOCOL = "host-requests/1";

/** The largest payload, from the notification allowance the design tested. */
export const PAYLOAD_MAX_BYTES = 10_240;
export const PURPOSE_MAX_CHARS = 280;
export const TTL_MIN_MS = 60_000;
export const TTL_MAX_MS = 7 * 24 * 60 * 60_000;

/**
 * A request as one host knows it, opaque to the product and meaningful only
 * to this host and product: 16 random bytes as `0x` hex. It is not the
 * request id the hosts exchange.
 */
export type RequestHandle = string;

export type { ContactHandle, ContactPickOutcome };

/** The hello a product frame posts to its parent, with one `MessagePort`. */
export interface ProductHello {
  type: typeof PRODUCT_HELLO;
  protocol: typeof PRODUCT_PROTOCOL;
}

/** The product asks its host to send a request to a contact. */
export interface SendRequest {
  contact: ContactHandle;
  /** What the request is for, shown to the recipient by their host. */
  purpose: string;
  /** How long the request may be answered. The host sets the deadline. */
  ttlMs: number;
  /** The product's payload as `0x` hex. The hosts carry it unchanged. */
  payload: string;
}

/**
 * Where a sent request is. `waiting` until the product completes it, cancels
 * it, or its deadline passes. There is no delivered, declined or answered.
 */
export type OutgoingStatus = "waiting" | "completed" | "cancelled" | "expired";

export interface OutgoingView {
  handle: RequestHandle;
  contact: ContactHandle;
  purpose: string;
  createdAt: number;
  deadline: number;
  status: OutgoingStatus;
  /** When it was completed or cancelled. */
  closedAt?: number;
}

/** A received request, handed to the product once its user chose Accept. */
export interface IncomingRequest {
  handle: RequestHandle;
  /** The authenticated sender. */
  sender: ContactHandle;
  payload: string;
}

/** A received request the product replied to, told again on every attach. */
export interface RepliedView {
  handle: RequestHandle;
  sender: ContactHandle;
  repliedAt: number;
}

/**
 * Product to host. Each answers with one `result` of the same `id`.
 *
 * Sender side:
 * - `pickContact`: the host's picker; answers a {@link ContactPickOutcome}.
 * - `request`: send a request to a contact, after the host asks its user.
 *   Answers the {@link OutgoingView}.
 * - `complete`: the product checked a reply its host delivered and is done;
 *   the reposts stop. Only after a reply was delivered.
 * - `cancel`: stop the reposts; a later reply is ignored.
 *
 * Recipient side, only for an `acceptIntent` the host sent:
 * - `reply`: answer with the product's payload. The host sends it.
 * - `refuse`: the product cannot accept it; the user sees `reason`.
 * - `open`: bring the product to the front, answering an `openIntent`.
 */
export type ProductCall =
  | { id: number; call: "pickContact" }
  | { id: number; call: "request"; request: unknown }
  | { id: number; call: "complete"; handle: RequestHandle }
  | { id: number; call: "cancel"; handle: RequestHandle }
  | {
      id: number;
      call: "reply";
      handle: RequestHandle;
      payload: string;
      open: boolean;
    }
  | { id: number; call: "refuse"; handle: RequestHandle; reason: string }
  | { id: number; call: "open"; handle: RequestHandle };

export type FailureReason =
  /** The user closed the host's prompt. */
  | "Dismissed"
  /** The call is malformed or out of bounds. */
  | "Invalid"
  /** The host does not know what the call names. */
  | "Unknown"
  /** The state does not allow it: closed, expired, already answered, no reply yet. */
  | "Refused";

export type Failure = { ok: false; reason: FailureReason; detail: string };

export type CallResult =
  | { ok: true; outcome?: ContactPickOutcome; request?: OutgoingView }
  | Failure;

/**
 * Host to product.
 *
 * - `result`: the answer to the call with that `id`.
 * - `outgoing`: a sent request changed, or every one on attach.
 * - `response`: an authenticated reply's payload, for a waiting request.
 * - `acceptIntent`: the user chose Accept in the host's Notifications.
 * - `replied`: a request the product replied to, on attach.
 * - `openIntent`: the user opened a request the product replied to.
 */
export type HostMessage =
  | { kind: "result"; id: number; result: CallResult }
  | { kind: "outgoing"; request: OutgoingView }
  | { kind: "response"; handle: RequestHandle; payload: string }
  | { kind: "acceptIntent"; request: IncomingRequest }
  | { kind: "replied"; request: RepliedView }
  | { kind: "openIntent"; handle: RequestHandle };

export type Read<T> = { ok: true; value: T } | { ok: false; reason: string };

const fail = <T>(reason: string): Read<T> => ({ ok: false, reason });

const HEX = /^0x(?:[0-9a-f]{2})*$/;

/** Bytes in `0x` hex, or null when `value` is not such hex. */
export function hexBytes(value: unknown): number | null {
  return typeof value === "string" && HEX.test(value)
    ? (value.length - 2) / 2
    : null;
}

export const isHexOf = (value: unknown, bytes: number): value is string =>
  hexBytes(value) === bytes;

export function readPurpose(value: unknown): Read<string> {
  if (typeof value !== "string" || value.trim() === "")
    return fail("The purpose is empty.");
  if (value.length > PURPOSE_MAX_CHARS)
    return fail(`The purpose is longer than ${PURPOSE_MAX_CHARS} characters.`);
  return { ok: true, value };
}

/** A payload: non-empty `0x` hex within the size limit. */
export function readPayload(value: unknown): Read<string> {
  const bytes = hexBytes(value);
  if (bytes === null) return fail("The payload is not 0x hex.");
  if (bytes === 0) return fail("The payload is empty.");
  if (bytes > PAYLOAD_MAX_BYTES)
    return fail(`The payload is larger than ${PAYLOAD_MAX_BYTES} bytes.`);
  return { ok: true, value: value as string };
}

/** A contact handle as `contacts.pick` gives it: 32 bytes. */
export function readContact(value: unknown): Read<ContactHandle> {
  const bytes = (value as Partial<ContactHandle> | null)?.bytes;
  return isHexOf(bytes, 32)
    ? { ok: true, value: { bytes: bytes as ContactHandle["bytes"] } }
    : fail("The contact is not a contact handle.");
}

export function readSendRequest(value: unknown): Read<SendRequest> {
  if (typeof value !== "object" || value === null) return fail("No request.");
  const request = value as Partial<Record<keyof SendRequest, unknown>>;
  const contact = readContact(request.contact);
  if (!contact.ok) return contact;
  const purpose = readPurpose(request.purpose);
  if (!purpose.ok) return purpose;
  const { ttlMs } = request;
  if (
    typeof ttlMs !== "number" ||
    !Number.isSafeInteger(ttlMs) ||
    ttlMs < TTL_MIN_MS ||
    ttlMs > TTL_MAX_MS
  )
    return fail("The expiry is outside one minute to seven days.");
  const payload = readPayload(request.payload);
  if (!payload.ok) return payload;
  return {
    ok: true,
    value: {
      contact: contact.value,
      purpose: purpose.value,
      ttlMs,
      payload: payload.value,
    },
  };
}

/** The call `value` is, with a request still unread, or null. */
export function readProductCall(value: unknown): ProductCall | null {
  if (typeof value !== "object" || value === null) return null;
  const call = value as Record<string, unknown>;
  if (!Number.isSafeInteger(call.id)) return null;
  const id = call.id as number;
  const text = (key: string): string | null =>
    typeof call[key] === "string" ? (call[key] as string) : null;
  const handle = text("handle");
  switch (call.call) {
    case "pickContact":
      return { id, call: "pickContact" };
    case "request":
      return { id, call: "request", request: call.request };
    case "complete":
    case "cancel":
    case "open":
      return handle !== null ? { id, call: call.call, handle } : null;
    case "reply": {
      const payload = text("payload");
      return handle !== null && payload !== null
        ? { id, call: "reply", handle, payload, open: call.open === true }
        : null;
    }
    case "refuse": {
      const reason = text("reason");
      return handle !== null && reason !== null && reason.trim() !== ""
        ? {
            id,
            call: "refuse",
            handle,
            reason: reason.slice(0, PURPOSE_MAX_CHARS),
          }
        : null;
    }
    default:
      return null;
  }
}

/** The hello `data` is, when it hands over exactly one port. */
export function isProductHello(data: unknown, ports: readonly unknown[]) {
  const hello = data as Partial<ProductHello> | null;
  return (
    hello?.type === PRODUCT_HELLO &&
    hello.protocol === PRODUCT_PROTOCOL &&
    ports.length === 1
  );
}
