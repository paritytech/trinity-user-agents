import {
  hexBytes,
  isHexOf,
  readPayload,
  readPurpose,
  PAYLOAD_MAX_BYTES,
} from "../../bridges/host-requests/contract.js";
import type {
  AuthEvidence,
  EvidenceKind,
  IdentityAuth,
  Person,
  Product,
} from "./ports.js";
import {
  fromHex,
  fromUtf8,
  toHex,
  utf8,
  type RandomBytes,
} from "../../bridges/primitives/bytes.js";
import {
  digest,
  isXWingPublic,
  openSealed,
  openXWingSealed,
  sealTo,
  sealToXWing,
  x25519Pair,
  xwingPair,
  XWING_PUBLIC_BYTES,
} from "./sealing.js";

/**
 * PROTOTYPE. The outer envelopes the hosts exchange for a request and its
 * reply, and the transcripts their authentication evidence covers.
 *
 * - A request carries its context (both identities, the product, the request
 *   id, when it was made, the return route the sender's delivery chose and the
 *   public reply key),
 *   the product's payload and the sender's evidence over all of it. It is
 *   sealed to the recipient's published X25519 chat key: classical.
 * - A reply carries its context (both identities, the product, the request id
 *   and the digest of the exact request), the product's payload and the
 *   responder's evidence over all of it. It is sealed to the request's reply
 *   key, which the sender's host made for that request and alone holds the
 *   secret of. The key's exact length says how: 1,216 bytes, X-Wing, hybrid
 *   post-quantum; 32 bytes, X25519, classical. The responder needs no
 *   published post-quantum key.
 *
 * Kind, version and the order of the identities are in every transcript, so
 * evidence for one direction or kind does not check as another. Evidence is
 * made and checked by the host's {@link IdentityAuth}; this module holds no
 * identity credential, and the evidence stays classical. Encoding here is
 * length-prefixed BLAKE2b over UTF-8 fields, a prototype choice, not the
 * design's wire format.
 *
 * The reply key is in the request's transcript, so only the sender chooses
 * its suite: a reply is sealed with the suite of that key and opened only
 * with it, never another. A key of any other length is refused.
 */

/** Version of the hosts' envelopes. */
export const WIRE_VERSION = "host-requests-wire/1";
export const REQUEST_SEAL = "truapi-web-host/request-seal/1";

/**
 * How a reply is sealed to the request's reply key: `xwing`, the X-Wing KEM,
 * hybrid post-quantum; or `x25519`, classical. Either way the reply is
 * encrypted with a key derived for it, never with the key exchange alone.
 */
export type ReplySuite = "xwing" | "x25519";

interface ReplyKeySuite {
  /** The purpose label the reply is sealed under. */
  seal: string;
  pair(secret: Uint8Array): { secret: Uint8Array; public: Uint8Array };
  sealTo(
    key: Uint8Array,
    plaintext: Uint8Array,
    purpose: string,
    random: RandomBytes,
  ): Uint8Array;
  open(
    secret: Uint8Array,
    sealed: Uint8Array,
    purpose: string,
  ): Uint8Array | null;
}

const REPLY_SUITES: Record<ReplySuite, ReplyKeySuite> = {
  xwing: {
    seal: "truapi-web-host/reply-seal/xwing/1",
    pair: xwingPair,
    sealTo: sealToXWing,
    open: openXWingSealed,
  },
  x25519: {
    seal: "truapi-web-host/reply-seal/1",
    pair: x25519Pair,
    sealTo,
    open: (secret, sealed, purpose) =>
      openSealed(x25519Pair(secret), sealed, purpose),
  },
};

/** The purpose label each suite's replies are sealed under. */
export const REPLY_SEALS: Record<ReplySuite, string> = {
  xwing: REPLY_SUITES.xwing.seal,
  x25519: REPLY_SUITES.x25519.seal,
};

/**
 * The suite of a reply key from another host, `0x` hex, by its exact length,
 * or null when it is none: 32 bytes is X25519; 1,216 bytes passing the ML-KEM
 * key check is X-Wing.
 */
export function replySuiteOf(key: unknown): ReplySuite | null {
  const suite = suiteByLength(key);
  return suite === "xwing" && !isXWingPublic(fromHex(key as string))
    ? null
    : suite;
}

/**
 * The suite a reply key's exact length gives, with no other check: for a key
 * that was checked when it arrived, or that this host made.
 */
function suiteByLength(key: unknown): ReplySuite | null {
  if (isHexOf(key, 32)) return "x25519";
  return isHexOf(key, XWING_PUBLIC_BYTES) ? "xwing" : null;
}

/** The reply key pair of `suite` for a 32-byte random `secret`. */
export const replyKeyPair = (suite: ReplySuite, secret: Uint8Array) =>
  REPLY_SUITES[suite].pair(secret);

const REQUEST_ID_BYTES = 16;
/**
 * The largest sealed envelope: a largest payload as hex, twice its bytes, and
 * room for the context, the evidence and the sealing.
 */
const ENVELOPE_MAX_BYTES = 2 * PAYLOAD_MAX_BYTES + 8192;
/** The longest return route a request may name. */
const ROUTE_MAX_CHARS = 256;

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
  /** Where the reply reaches the sender, as the sender's delivery chose it. Opaque here. */
  returnRoute: string;
  /** The public key the reply is sealed to, made for this request. Its length gives its suite. */
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
    context.returnRoute,
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
    typeof item.returnRoute === "string" &&
    item.returnRoute !== "" &&
    item.returnRoute.length <= ROUTE_MAX_CHARS &&
    replySuiteOf(item.replyKey) !== null
    ? {
        version: WIRE_VERSION,
        from: item.from,
        to: item.to,
        product: { id: item.product.id, name: item.product.name },
        purpose: item.purpose as string,
        requestId: item.requestId,
        createdAt: item.createdAt,
        returnRoute: item.returnRoute,
        replyKey: item.replyKey as string,
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
  | Refused;

/**
 * Why an envelope was refused. `sealedElsewhere` when it does not open with
 * this host's key at all: on a shared inbox, someone else's post.
 */
export interface Refused {
  ok: false;
  reason: string;
  sealedElsewhere: boolean;
}

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
  const refuse = (reason: string): OpenedRequest => ({
    ok: false,
    reason,
    sealedElsewhere: false,
  });
  const bytes = hexBytes(sealed);
  if (bytes === null || bytes > ENVELOPE_MAX_BYTES)
    return refuse("not a request");
  const plain = auth.openSealed(fromHex(sealed), REQUEST_SEAL);
  if (plain === null)
    return {
      ok: false,
      reason: "not sealed to this host",
      sealedElsewhere: true,
    };
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

/**
 * Whom a request sealed to this host says it is from, before anything is
 * checked: only to know whose published key to read before
 * {@link openRequest} authenticates it. Null when it does not open or read.
 */
export function requestSender(
  auth: IdentityAuth,
  sealed: string,
): string | null {
  const bytes = hexBytes(sealed);
  if (bytes === null || bytes > ENVELOPE_MAX_BYTES) return null;
  const plain = auth.openSealed(fromHex(sealed), REQUEST_SEAL);
  return plain === null
    ? null
    : (parse(plain, readRequestContext)?.context.from ?? null);
}

/**
 * The reply from this host to `peer`'s `request`, authenticated and sealed to
 * its reply key. Throws when the key cannot be sealed to, such as an X25519
 * key of low order.
 */
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
  const suite = suiteByLength(request.context.replyKey);
  if (suite === null) throw new Error("the request's reply key has no suite");
  const { sealTo: seal, seal: purpose } = REPLY_SUITES[suite];
  return toHex(
    seal(
      fromHex(request.context.replyKey),
      encode({ context, payload, evidence }),
      purpose,
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
  /** The request's public reply key: its suite is the only one the reply is opened with. */
  replyKey: string;
  /** The 32-byte secret of the request's reply key. */
  replySecret: Uint8Array;
}

export type OpenedReply = { ok: true; payload: string } | Refused;

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
  const refuse = (reason: string): OpenedReply => ({
    ok: false,
    reason,
    sealedElsewhere: false,
  });
  const bytes = hexBytes(sealed);
  if (bytes === null || bytes > ENVELOPE_MAX_BYTES)
    return refuse("not a reply");
  // This host made the key; a full check would cost an encapsulation per post.
  const suite = suiteByLength(expected.replyKey);
  const plain =
    suite === null
      ? null
      : REPLY_SUITES[suite].open(
          expected.replySecret,
          fromHex(sealed),
          REPLY_SUITES[suite].seal,
        );
  if (plain === null)
    return {
      ok: false,
      reason: "not sealed to this request",
      sealedElsewhere: true,
    };
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
