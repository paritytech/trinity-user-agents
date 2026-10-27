/**
 * DRAFT, NOT TRUAPI. A provisional request/reply API between a product and
 * its host for a just-in-time (JIT) invitation, after the "Establish
 * Connection" design. It exists to try the design; nothing here is agreed.
 *
 * Who owns what:
 * - The product makes the invitation: a request id, a challenge and its own
 *   setup data (for Example Chat: the reply topic `T_BA`, the reply key `e_A`
 *   and its protocol key). Its private setup keys never leave it.
 * - The host owns people: it picks the contact, resolves the reference the
 *   product holds, authenticates and seals the invitation, delivers and
 *   reposts it, and renders who sent it. The product only ever holds an
 *   opaque {@link ContactRef}, never a name, account or key of a person.
 * - The recipient's product answers through its host (`accept` with its
 *   response). The host binds a proof to the expected contact, the whole
 *   invitation and the exact response, sealed so only the sender's host can
 *   read it. The product posts the response and proof on `T_BA` itself.
 * - The sender's product asks its host to `verify` a response it received,
 *   checks the setup protocol itself, and only then calls `complete`, which
 *   stops the reposts. `cancel` and the expiry stop them too.
 * - Decline is silent: the recipient's host sends nothing back, ever. There is
 *   no delivery receipt.
 *
 * Messages travel over a `MessagePort` the product's frame hands its host in
 * a hello. Every call carries an `id`; the host answers each with one
 * `result`.
 */

/** `type` of the window message a product frame sends to hand its host a port. */
export const PRODUCT_HELLO = "invitations-product";
/** Version of the messages on that port. */
export const PRODUCT_PROTOCOL = "invitations/1";

/** The largest setup data or response, from the notification allowance the design tested. */
export const PAYLOAD_MAX_BYTES = 10_240;
export const PURPOSE_MAX_CHARS = 280;
export const TTL_MIN_MS = 60_000;
export const TTL_MAX_MS = 7 * 24 * 60 * 60_000;
export const REQUEST_ID_BYTES = 16;
export const CHALLENGE_BYTES = 32;

/**
 * What a product holds for a person its user chose: opaque, and meaningful
 * only to this host for this product. It stands in for the handle
 * `contacts.pick` returns. That handle cannot be used here today, because the
 * core gives the host no way to resolve it outside a transaction; the dev
 * tools mint this one, and its `fixture-contact:` prefix says so.
 */
export type ContactRef = string;

/** The hello a product frame posts to its parent, with one `MessagePort`. */
export interface ProductHello {
  type: typeof PRODUCT_HELLO;
  protocol: typeof PRODUCT_PROTOCOL;
}

/** An invitation, as both products see it. The sender's host sets `expiresAt`. */
export interface Invitation {
  /** 16 bytes as `0x` hex, chosen by the sender's product. */
  requestId: string;
  /** 32 fresh random bytes as `0x` hex. */
  challenge: string;
  expiresAt: number;
  /** The product's own setup data as `0x` hex. The hosts carry it unchanged. */
  setup: string;
}

/** The product asks its host to send an invitation to a contact it picked. */
export interface SendRequest {
  contact: ContactRef;
  /** What the invitation is for, shown to the recipient by their host. */
  purpose: string;
  ttlMs: number;
  invitation: Omit<Invitation, "expiresAt">;
}

/**
 * Where a sent invitation is. `waiting` until the product completes it,
 * cancels it, or it expires. There is no delivered, declined or accepted.
 */
export type OutgoingStatus = "waiting" | "completed" | "cancelled" | "expired";

export interface OutgoingView {
  requestId: string;
  contact: ContactRef;
  purpose: string;
  createdAt: number;
  expiresAt: number;
  status: OutgoingStatus;
  /** When it was completed or cancelled. */
  closedAt?: number;
}

/** A received invitation, handed to the product when its user chose Accept. */
export interface IncomingInvitation {
  /** The host's id for this received invitation. */
  invitationId: string;
  /** The authenticated sender, as an opaque reference. */
  sender: ContactRef;
  purpose: string;
  invitation: Invitation;
}

/** A received invitation the product accepted, told again on every attach. */
export interface AcceptedView extends IncomingInvitation {
  acceptedAt: number;
}

/**
 * Product to host. Each answers with one `result` of the same `id`.
 *
 * Sender side:
 * - `pickContact`: the host's picker; answers a {@link ContactRef}.
 * - `send`: send an invitation to a picked contact, after the host asks its
 *   user. Answers the {@link OutgoingView}.
 * - `verify`: check a response and the host proof that came with it. A pass
 *   does not stop the reposts.
 * - `complete`: the product checked a verified response and is connected;
 *   the reposts stop.
 * - `cancel`: stop the reposts; a later response is refused.
 *
 * Recipient side, only for an `acceptIntent` the host sent:
 * - `accept`: accept with the product's exact response. Answers the sealed
 *   host proof, which the product passes on with the response.
 * - `refuse`: the product cannot accept it; the user sees `reason`.
 * - `open`: bring the product to the front, answering an `openIntent`.
 */
export type ProductCall =
  | { id: number; call: "pickContact" }
  | { id: number; call: "send"; request: unknown }
  | {
      id: number;
      call: "verify";
      requestId: string;
      response: string;
      proof: string;
    }
  | { id: number; call: "complete"; requestId: string; response: string }
  | { id: number; call: "cancel"; requestId: string }
  | {
      id: number;
      call: "accept";
      invitationId: string;
      response: string;
      open: boolean;
    }
  | { id: number; call: "refuse"; invitationId: string; reason: string }
  | { id: number; call: "open"; invitationId: string };

export type FailureReason =
  /** The user closed the host's prompt. */
  | "Dismissed"
  /** The call is malformed or out of bounds. */
  | "Invalid"
  /** The host does not know what the call names. */
  | "Unknown"
  /** The state does not allow it: closed, expired, already answered, not verified. */
  | "Refused"
  /** The response or its proof does not check. */
  | "Rejected";

export type Failure = { ok: false; reason: FailureReason; detail: string };

export type CallResult =
  | {
      ok: true;
      contact?: ContactRef;
      invitation?: OutgoingView;
      /** The sealed host proof, for `accept`. Opaque to the product. */
      proof?: string;
    }
  | Failure;

/**
 * Host to product.
 *
 * - `result`: the answer to the call with that `id`.
 * - `outgoing`: a sent invitation changed, or every one on attach.
 * - `acceptIntent`: the user chose Accept in the host's Notifications.
 * - `accepted`: an invitation the product accepted, on attach.
 * - `openIntent`: the user opened an accepted invitation in the host's list.
 */
export type HostMessage =
  | { kind: "result"; id: number; result: CallResult }
  | { kind: "outgoing"; invitation: OutgoingView }
  | { kind: "acceptIntent"; invitation: IncomingInvitation }
  | { kind: "accepted"; invitation: AcceptedView }
  | { kind: "openIntent"; invitationId: string };

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

/** Setup data or a response: non-empty `0x` hex within the size limit. */
export function readPayload(value: unknown, what: string): Read<string> {
  const bytes = hexBytes(value);
  if (bytes === null) return fail(`The ${what} is not 0x hex.`);
  if (bytes === 0) return fail(`The ${what} is empty.`);
  if (bytes > PAYLOAD_MAX_BYTES)
    return fail(`The ${what} is larger than ${PAYLOAD_MAX_BYTES} bytes.`);
  return { ok: true, value: value as string };
}

export function readSendRequest(value: unknown): Read<SendRequest> {
  if (typeof value !== "object" || value === null) return fail("No request.");
  const request = value as Partial<Record<keyof SendRequest, unknown>>;
  if (typeof request.contact !== "string" || request.contact === "")
    return fail("No contact.");
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
  const invitation = request.invitation as
    | Partial<Record<keyof Invitation, unknown>>
    | undefined;
  if (!isHexOf(invitation?.requestId, REQUEST_ID_BYTES))
    return fail(`The request id is not ${REQUEST_ID_BYTES} bytes of 0x hex.`);
  if (!isHexOf(invitation.challenge, CHALLENGE_BYTES))
    return fail(`The challenge is not ${CHALLENGE_BYTES} bytes of 0x hex.`);
  const setup = readPayload(invitation.setup, "setup data");
  if (!setup.ok) return setup;
  return {
    ok: true,
    value: {
      contact: request.contact,
      purpose: purpose.value,
      ttlMs,
      invitation: {
        requestId: invitation.requestId,
        challenge: invitation.challenge,
        setup: setup.value,
      },
    },
  };
}

/** The call `value` is, with a send request still unread, or null. */
export function readProductCall(value: unknown): ProductCall | null {
  if (typeof value !== "object" || value === null) return null;
  const call = value as Record<string, unknown>;
  if (!Number.isSafeInteger(call.id)) return null;
  const id = call.id as number;
  const text = (key: string): string | null =>
    typeof call[key] === "string" ? (call[key] as string) : null;
  const requestId = text("requestId");
  const invitationId = text("invitationId");
  const response = text("response");
  switch (call.call) {
    case "pickContact":
      return { id, call: "pickContact" };
    case "send":
      return { id, call: "send", request: call.request };
    case "verify": {
      const proof = text("proof");
      return requestId !== null && response !== null && proof !== null
        ? { id, call: "verify", requestId, response, proof }
        : null;
    }
    case "complete":
      return requestId !== null && response !== null
        ? { id, call: "complete", requestId, response }
        : null;
    case "cancel":
      return requestId !== null ? { id, call: "cancel", requestId } : null;
    case "accept":
      return invitationId !== null && response !== null
        ? {
            id,
            call: "accept",
            invitationId,
            response,
            open: call.open === true,
          }
        : null;
    case "refuse": {
      const reason = text("reason");
      return invitationId !== null && reason !== null && reason.trim() !== ""
        ? {
            id,
            call: "refuse",
            invitationId,
            reason: reason.slice(0, PURPOSE_MAX_CHARS),
          }
        : null;
    }
    case "open":
      return invitationId !== null ? { id, call: "open", invitationId } : null;
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
