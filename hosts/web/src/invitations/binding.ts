import {
  hexBytes,
  isHexOf,
  readPayload,
  readPurpose,
  PAYLOAD_MAX_BYTES,
} from "./contract.js";
import type {
  AuthEvidence,
  EvidenceKind,
  IdentityAuth,
  Person,
  Product,
} from "./ports.js";
import {
  digest,
  fromHex,
  fromUtf8,
  openSealed,
  sealTo,
  toHex,
  utf8,
  type RandomBytes,
  type X25519Pair,
} from "./sealing.js";

/**
 * PROTOTYPE. The outer envelopes the hosts exchange for a request and its
 * reply, and the transcripts their authentication evidence covers.
 *
 * - A request carries its context (both identities, the product, the request
 *   id, the deadline, the return topic `T_SETUP` and the public reply key),
 *   the product's payload and the sender's evidence over all of it. It is
 *   sealed to the recipient's identity key.
 * - A reply carries its context (both identities, the product, the request id
 *   and the digest of the exact request), the product's payload and the
 *   responder's evidence over all of it. It is sealed to the request's reply
 *   key, which only the sender's host holds.
 *
 * Kind, version and the order of the identities are in every transcript, so
 * evidence for one direction or kind does not check as another. Evidence is
 * made and checked by the host's {@link IdentityAuth}; this module holds no
 * identity credential. Encoding here is length-prefixed BLAKE2b over UTF-8
 * fields, a prototype choice, not the design's wire format.
 */

/** Version of the hosts' envelopes. */
export const WIRE_VERSION = "host-requests-wire/1";
export const REQUEST_SEAL = "truapi-web-host/request-seal/1";
export const REPLY_SEAL = "truapi-web-host/reply-seal/1";
const REQUEST_ID_BYTES = 16;
/**
 * The largest sealed envelope: a largest payload as hex, twice its bytes, and
 * room for the context, the evidence and the sealing.
 */
const ENVELOPE_MAX_BYTES = 2 * PAYLOAD_MAX_BYTES + 8192;
const TOPIC_BYTES = 32;

/** What a request says, besides its payload. Every field is authenticated. */
export interface RequestContext {
  version: typeof WIRE_VERSION;
  /** The sender's identity, by signing key. */
  from: string;
  /** The recipient's identity, by signing key. */
  to: string;
  product: Product;
  /** Shown to the recipient by their host. */
  purpose: string;
  requestId: string;
  createdAt: number;
  deadline: number;
  /** The return topic, `T_SETUP`. */
  setupTopic: string;
  /** The public reply-envelope key. */
  replyKey: string;
}

/** What a reply says, besides its payload. Every field is authenticated. */
export interface ReplyContext {
  version: typeof WIRE_VERSION;
  /** The responder's identity, by signing key. */
  from: string;
  /** The requester's identity, by signing key. */
  to: string;
  productId: string;
  requestId: string;
  /** The digest of the exact request answered. */
  requestDigest: string;
}

const payloadDigest = (payload: string): string =>
  toHex(digest("truapi-web-host/payload/1", payload));

/** The transcript a request's evidence covers. Its hex is the request digest. */
export function requestTranscript(
  context: RequestContext,
  payload: string,
): Uint8Array {
  return digest(
    "truapi-web-host/request/1",
    "Request" satisfies EvidenceKind,
    context.version,
    context.from,
    context.to,
    context.product.id,
    context.product.name,
    context.purpose,
    context.requestId,
    String(context.createdAt),
    String(context.deadline),
    context.setupTopic,
    context.replyKey,
    payloadDigest(payload),
  );
}

/** The transcript a reply's evidence covers. */
export function replyTranscript(
  context: ReplyContext,
  payload: string,
): Uint8Array {
  return digest(
    "truapi-web-host/reply/1",
    "Reply" satisfies EvidenceKind,
    context.version,
    context.from,
    context.to,
    context.productId,
    context.requestId,
    context.requestDigest,
    payloadDigest(payload),
  );
}

export const requestDigest = (context: RequestContext, payload: string) =>
  toHex(requestTranscript(context, payload));

function readEvidence(value: unknown): AuthEvidence | null {
  const item = value as Partial<AuthEvidence> | null;
  return (item?.kind === "Request" || item?.kind === "Reply") &&
    isHexOf(item.identityMac, 32) &&
    isHexOf(item.messageSignature, 64)
    ? {
        kind: item.kind,
        identityMac: item.identityMac,
        messageSignature: item.messageSignature,
      }
    : null;
}

const isKey = (value: unknown): value is string => isHexOf(value, 32);
const isTime = (value: unknown): value is number =>
  typeof value === "number" && Number.isSafeInteger(value);

function readRequestContext(value: unknown): RequestContext | null {
  const item = value as Partial<RequestContext> | null;
  return item?.version === WIRE_VERSION &&
    isKey(item.from) &&
    isKey(item.to) &&
    typeof item.product?.id === "string" &&
    typeof item.product.name === "string" &&
    readPurpose(item.purpose).ok &&
    isHexOf(item.requestId, REQUEST_ID_BYTES) &&
    isTime(item.createdAt) &&
    isTime(item.deadline) &&
    isHexOf(item.setupTopic, TOPIC_BYTES) &&
    isKey(item.replyKey)
    ? {
        version: WIRE_VERSION,
        from: item.from,
        to: item.to,
        product: { id: item.product.id, name: item.product.name },
        purpose: item.purpose as string,
        requestId: item.requestId,
        createdAt: item.createdAt,
        deadline: item.deadline,
        setupTopic: item.setupTopic,
        replyKey: item.replyKey,
      }
    : null;
}

function readReplyContext(value: unknown): ReplyContext | null {
  const item = value as Partial<ReplyContext> | null;
  return item?.version === WIRE_VERSION &&
    isKey(item.from) &&
    isKey(item.to) &&
    typeof item.productId === "string" &&
    isHexOf(item.requestId, REQUEST_ID_BYTES) &&
    isKey(item.requestDigest)
    ? {
        version: WIRE_VERSION,
        from: item.from,
        to: item.to,
        productId: item.productId,
        requestId: item.requestId,
        requestDigest: item.requestDigest,
      }
    : null;
}

interface Sealed<C> {
  context: C;
  payload: string;
  evidence: AuthEvidence;
}

/** The parts of an opened envelope, or null when it is not one. */
function parse<C>(
  plain: Uint8Array,
  readContext: (value: unknown) => C | null,
): Sealed<C> | null {
  let value: unknown;
  try {
    value = JSON.parse(fromUtf8(plain));
  } catch {
    return null;
  }
  if (typeof value !== "object" || value === null) return null;
  const {
    context: rawContext,
    payload: rawPayload,
    evidence: rawEvidence,
  } = value as Record<string, unknown>;
  const context = readContext(rawContext);
  const payload = readPayload(rawPayload);
  const evidence = readEvidence(rawEvidence);
  return context === null || !payload.ok || evidence === null
    ? null
    : { context, payload: payload.value, evidence };
}

const encode = <C>(sealed: Sealed<C>): Uint8Array =>
  utf8(JSON.stringify(sealed));

/** The request from this host to `peer`, authenticated and sealed to them. */
export function sealRequest(
  auth: IdentityAuth,
  peer: Person,
  context: Omit<RequestContext, "version" | "from" | "to">,
  payload: string,
  random: RandomBytes,
): { context: RequestContext; sealed: string } {
  const full: RequestContext = {
    version: WIRE_VERSION,
    from: auth.identityFor(peer).signingPublic,
    to: peer.identity.signingPublic,
    ...context,
  };
  const evidence = auth.evidence(
    "Request",
    requestTranscript(full, payload),
    peer,
  );
  const sealed = sealTo(
    fromHex(peer.identity.agreementPublic),
    encode({ context: full, payload, evidence }),
    REQUEST_SEAL,
    random,
  );
  return { context: full, sealed: toHex(sealed) };
}

export type OpenedRequest =
  | { ok: true; peer: Person; context: RequestContext; payload: string }
  | { ok: false; reason: string };

/**
 * Open a request sealed to this host and authenticate it: from a contact, to
 * the identity this host's user shows them, with evidence over the whole
 * request.
 */
export function openRequest(
  auth: IdentityAuth,
  sealed: string,
  contact: (signingPublic: string) => Person | undefined,
): OpenedRequest {
  const refuse = (reason: string): OpenedRequest => ({ ok: false, reason });
  const bytes = hexBytes(sealed);
  if (bytes === null || bytes > ENVELOPE_MAX_BYTES)
    return refuse("not a request");
  const plain = auth.openSealed(fromHex(sealed), REQUEST_SEAL);
  if (plain === null) return refuse("not sealed to this host");
  const opened = parse(plain, readRequestContext);
  if (opened === null) return refuse("not a request");
  const { context, payload, evidence } = opened;
  const peer = contact(context.from);
  if (peer === undefined) return refuse("the sender is not a contact");
  if (context.to !== auth.identityFor(peer).signingPublic)
    return refuse("it is for another identity");
  if (
    evidence.kind !== "Request" ||
    !auth.check("Request", requestTranscript(context, payload), evidence, peer)
  )
    return refuse("its evidence does not check");
  return { ok: true, peer, context, payload };
}

/** The reply from this host to `peer`'s `request`, authenticated and sealed to its reply key. */
export function sealReply(
  auth: IdentityAuth,
  peer: Person,
  request: { context: RequestContext; digest: string },
  payload: string,
  random: RandomBytes,
): string {
  const context: ReplyContext = {
    version: WIRE_VERSION,
    from: auth.identityFor(peer).signingPublic,
    to: request.context.from,
    productId: request.context.product.id,
    requestId: request.context.requestId,
    requestDigest: request.digest,
  };
  const evidence = auth.evidence(
    "Reply",
    replyTranscript(context, payload),
    peer,
  );
  return toHex(
    sealTo(
      fromHex(request.context.replyKey),
      encode({ context, payload, evidence }),
      REPLY_SEAL,
      random,
    ),
  );
}

/** What the sender's host expects of a reply to one of its requests. */
export interface ExpectedReply {
  /** The contact the request was sent to. */
  peer: Person;
  productId: string;
  requestId: string;
  requestDigest: string;
  replyKey: X25519Pair;
}

export type OpenedReply =
  | { ok: true; payload: string }
  | { ok: false; reason: string };

/**
 * Open a reply with the request's reply key and authenticate it: from the
 * contact the request went to, to this host's user, for this product and the
 * exact request, with evidence over the whole reply.
 */
export function openReply(
  auth: IdentityAuth,
  expected: ExpectedReply,
  sealed: string,
): OpenedReply {
  const refuse = (reason: string): OpenedReply => ({ ok: false, reason });
  const bytes = hexBytes(sealed);
  if (bytes === null || bytes > ENVELOPE_MAX_BYTES)
    return refuse("not a reply");
  const plain = openSealed(expected.replyKey, fromHex(sealed), REPLY_SEAL);
  if (plain === null) return refuse("not sealed to this request");
  const opened = parse(plain, readReplyContext);
  if (opened === null) return refuse("not a reply");
  const { context, payload, evidence } = opened;
  const { peer } = expected;
  if (context.from !== peer.identity.signingPublic)
    return refuse("it is from someone other than the contact asked");
  if (context.to !== auth.identityFor(peer).signingPublic)
    return refuse("it is for another identity");
  if (
    context.productId !== expected.productId ||
    context.requestId !== expected.requestId ||
    context.requestDigest !== expected.requestDigest
  )
    return refuse("it answers another request");
  if (
    evidence.kind !== "Reply" ||
    !auth.check("Reply", replyTranscript(context, payload), evidence, peer)
  )
    return refuse("its evidence does not check");
  return { ok: true, payload };
}
