import { ed25519 } from "@noble/curves/ed25519.js";
import { bytesToNumberLE } from "@noble/curves/utils.js";
import { sha256, sha512 } from "@noble/hashes/sha2.js";
import { bytesToHex, hexToBytes } from "@noble/hashes/utils.js";
import { decodeNotificationContainer, encodeNotificationContainer, MAX_CONTAINER_BYTES } from "./notification-container.js";
export { NOTIFICATION_PREDICATE, MAX_NOTIFICATION_CANDIDATES } from "./notification-container.js";

export const MAX_HEADER_BYTES = 16 * 1024;
export const MAX_CARRIER_BYTES = 240 * 1024;
export const MAX_FULL_FRAME_BYTES = MAX_CONTAINER_BYTES;
export const MAX_TTL_MS = 86_400_000;
export const FUTURE_SKEW_MS = 60_000;
const encoder = new TextEncoder();

/** Public authenticated metadata. Hex is lowercase, without a 0x prefix. */
export interface NotificationHeader {
  v: 1;
  product: string;
  genesis: string;
  channel: string;
  topics: string[];
  eventId: string;
  createdAt: number;
  expiresAt: number;
  ciphertextDigest: string;
  senderKey: string;
  signature: string;
}
export type UnsignedNotificationHeader = Omit<NotificationHeader, "signature">;
export type NotificationMetadata = Omit<UnsignedNotificationHeader, "v" | "ciphertextDigest" | "senderKey">;
/** Decoding alone does not authenticate the result. */
export interface NotificationEnvelope {
  header: NotificationHeader;
  carrier: Uint8Array;
}
const fields = ["v", "product", "genesis", "channel", "topics", "eventId", "createdAt", "expiresAt", "ciphertextDigest", "senderKey", "signature"];
const hex32 = /^[0-9a-f]{64}$/;
function isHex(value: unknown, pattern = hex32): value is string {
  return typeof value === "string" && pattern.test(value);
}
function validateHeader(value: unknown): asserts value is NotificationHeader {
  if (value === null || typeof value !== "object" || Array.isArray(value)) throw new Error("invalid notification header");
  const header = value as NotificationHeader;
  if (Object.keys(header).length !== fields.length || fields.some((field) => !Object.hasOwn(header, field))
    || header.v !== 1 || typeof header.product !== "string" || !/^[a-z0-9._-]{1,128}$/.test(header.product)
    || !isHex(header.genesis) || !isHex(header.channel) || !isHex(header.eventId) || !isHex(header.ciphertextDigest) || !isHex(header.senderKey)
    || !isHex(header.signature, /^[0-9a-f]{128}$/)
    || !Array.isArray(header.topics) || header.topics.length < 1 || header.topics.length > 4
    || header.topics.some((topic) => !isHex(topic)) || new Set(header.topics).size !== header.topics.length
    || !Number.isSafeInteger(header.createdAt) || header.createdAt < 0
    || !Number.isSafeInteger(header.expiresAt) || header.expiresAt <= header.createdAt
    || header.expiresAt - header.createdAt > MAX_TTL_MS) throw new Error("invalid notification header");
}

/** Exact domain-separated UTF-8 tuple shared with the Rust verifier. */
export function notificationSigningBytes(header: UnsignedNotificationHeader): Uint8Array {
  validateHeader({ ...header, signature: "00".repeat(64) });
  return encoder.encode(JSON.stringify([
    "truapi:notification:v1", header.v, header.product, header.genesis, header.channel,
    header.topics, header.eventId, header.createdAt, header.expiresAt, header.ciphertextDigest, header.senderKey,
  ]));
}

function parseHeader(text: string): NotificationHeader {
  if (encoder.encode(text).length > MAX_HEADER_BYTES) throw new Error("notification header too large");
  const value: unknown = JSON.parse(text);
  // JSON.parse discards duplicate keys. Tokenize valid JSON first to retain them,
  // including escaped key spellings, and reject non-integer number spellings.
  const tokens = text.match(/"(?:[^"\\]|\\.)*"|-?\d+(?:\.\d+)?(?:[eE][+-]?\d+)?|true|false|null|[{}\[\],:]/g) ?? [];
  const keys = new Set<string>();
  for (let index = 0; index < tokens.length; index++) {
    const token = tokens[index]!;
    if (tokens[index + 1] === ":") {
      const key: string = JSON.parse(token);
      if (keys.has(key)) throw new Error("duplicate notification field");
      keys.add(key);
    } else if (/^-?\d/.test(token) && !/^(0|[1-9]\d*)$/.test(token)) {
      throw new Error("non-integer notification number");
    }
  }
  validateHeader(value);
  return value;
}

/** Inspect bounded standard Envelope nodes for reserved assertions.
 * This parses structure but not header JSON or its proof. Malformed containers throw:
 * callers must drop them, never fall back to another authentication path on errors.
 * Non-Envelope bytes and valid unmarked base Envelopes return false.
 */
export function isNotificationEnvelope(frame: Uint8Array): boolean {
  if (frame.length === 0 || (frame[0]! >>> 5) !== 6) return false;
  return decodeNotificationContainer(frame).headers.length > 0;
}

/** Decode exactly one candidate; does not verify proof, source or freshness. */
export function decodeNotificationEnvelope(frame: Uint8Array): NotificationEnvelope {
  const { headers, subjects } = decodeNotificationContainer(frame);
  if (headers.length !== 1) throw new Error("expected exactly one notification candidate");
  return decodeCandidate(headers[0]!, subjects);
}

function decodeCandidate(json: string, subjects: ReadonlyMap<string, Uint8Array>): NotificationEnvelope {
  const header = parseHeader(json);
  const carrier = subjects.get(header.ciphertextDigest);
  if (carrier === undefined || carrier.length > MAX_CARRIER_BYTES) throw new Error("missing or oversized notification byte witness");
  return { header, carrier: carrier.slice() };
}

/** Encode a minimal standard carrier; supplied proof is not verified. */
export function encodeNotificationEnvelope(header: NotificationHeader, carrier: Uint8Array): Uint8Array {
  validateHeader(header);
  if (carrier.length > MAX_CARRIER_BYTES) throw new Error("notification carrier too large");
  const json = JSON.stringify(header);
  if (encoder.encode(json).length > MAX_HEADER_BYTES) throw new Error("notification header too large");
  const frame = encodeNotificationContainer(json, carrier);
  if (frame.length > MAX_FULL_FRAME_BYTES) throw new Error("notification container too large");
  return frame;
}

/** Sign public metadata over an opaque byte witness using a raw 32-byte seed.
 * Existing standard carriers can add JSON.stringify(result) as a
 * NOTIFICATION_PREDICATE assertion on any node containing the byte witness.
 */
export function signNotificationHeader(metadata: NotificationMetadata, carrier: Uint8Array, seed: Uint8Array): NotificationHeader {
  if (carrier.length > MAX_CARRIER_BYTES) throw new Error("notification carrier too large");
  const unsigned: UnsignedNotificationHeader = {
    ...metadata, v: 1, ciphertextDigest: bytesToHex(sha256(carrier)), senderKey: bytesToHex(ed25519.getPublicKey(seed)),
  };
  const signature = bytesToHex(ed25519.sign(notificationSigningBytes(unsigned), seed));
  return { ...unsigned, signature };
}

/** Sign and emit a minimal standard carrier, in Node, Bun or a browser. */
export function signNotificationEnvelope(metadata: NotificationMetadata, carrier: Uint8Array, seed: Uint8Array): Uint8Array {
  return encodeNotificationEnvelope(signNotificationHeader(metadata, carrier, seed), carrier);
}

/** Authenticate optional stored metadata against supplied opaque bytes.
 * Checks strict JSON, static bounds, digest and Ed25519 proof only.
 * Does NOT establish current-time eligibility, actual source, product authority,
 * enrollment or sender approval. Hosts/relays must use whole-carrier verification.
 */
export function authenticateNotificationHeader(json: string, carrier: Uint8Array): NotificationHeader {
  if (carrier.length > MAX_CARRIER_BYTES) throw new Error("notification carrier too large");
  const header = parseHeader(json);
  if (bytesToHex(sha256(carrier)) !== header.ciphertextDigest) throw new Error("notification byte witness digest mismatch");
  verifyHeaderProof(header);
  return header;
}

/** Authenticate stored bytes without applying current-time notification eligibility.
 * Enrollment, product, sender approval and replay policy remain caller responsibilities.
 * Subject bytes are copied so input mutation cannot alter authenticated output.
 */
export function authenticateNotificationEnvelope(frame: Uint8Array, actualGenesis: string, actualChannel: string, actualTopics: readonly string[]): NotificationEnvelope {
  return authenticateCandidate(decodeNotificationEnvelope(frame), actualGenesis, actualChannel, actualTopics);
}

function authenticateCandidate(envelope: NotificationEnvelope, actualGenesis: string, actualChannel: string, actualTopics: readonly string[]): NotificationEnvelope {
  const { header } = envelope;
  if (header.genesis !== actualGenesis || header.channel !== actualChannel
    || actualTopics.length < 1 || actualTopics.length > 4 || actualTopics.some((topic) => !isHex(topic))
    || new Set(actualTopics).size !== actualTopics.length
    || header.topics.some((topic) => !actualTopics.includes(topic))) throw new Error("notification source mismatch");
  verifyHeaderProof(header);
  return envelope;
}

function verifyHeaderProof(header: NotificationHeader): void {
  const key = hexToBytes(header.senderKey);
  const signature = hexToBytes(header.signature);
  const publicPoint = ed25519.Point.fromBytes(key, false);
  const noncePoint = ed25519.Point.fromBytes(signature.subarray(0, 32), false);
  const order = ed25519.Point.CURVE().n;
  const scalar = bytesToNumberLE(signature.subarray(32));
  if (publicPoint.isSmallOrder() || noncePoint.isSmallOrder() || scalar >= order) throw new Error("invalid notification signature");
  const challenge = bytesToNumberLE(sha512.create()
    .update(signature.subarray(0, 32)).update(key).update(notificationSigningBytes(header)).digest()) % order;
  // Noble's verify clears the cofactor even with zip215:false. Dalek verify_strict
  // requires this exact, uncofactored equation; mixed-order points are not banned.
  const expectedNonce = ed25519.Point.BASE.multiplyUnsafe(scalar).subtract(publicPoint.multiplyUnsafe(challenge));
  if (!expectedNonce.equals(noncePoint)) throw new Error("invalid notification signature");
}

/** Authenticate a carrier and enforce current-time notification eligibility, or throw. */
export function verifyNotificationEnvelope(frame: Uint8Array, actualGenesis: string, actualChannel: string, actualTopics: readonly string[], nowMs: number): NotificationEnvelope {
  const envelope = authenticateNotificationEnvelope(frame, actualGenesis, actualChannel, actualTopics);
  const { header } = envelope;
  if (!Number.isSafeInteger(nowMs) || nowMs < 0 || header.createdAt > nowMs + FUTURE_SKEW_MS || header.expiresAt <= nowMs) throw new Error("notification outside validity window");
  return envelope;
}

/** Authenticate all candidates, omitting invalid proofs/headers, without current-time checks.
 * Malformed containers or structural ambiguities throw before any result is returned.
 */
export function authenticateNotificationEnvelopes(frame: Uint8Array, actualGenesis: string, actualChannel: string, actualTopics: readonly string[]): NotificationEnvelope[] {
  const { headers, subjects } = decodeNotificationContainer(frame);
  const authenticated: NotificationEnvelope[] = [];
  for (const json of headers) {
    try {
      authenticated.push(authenticateCandidate(decodeCandidate(json, subjects), actualGenesis, actualChannel, actualTopics));
    } catch {
      // A candidate's invalid proof cannot suppress independently valid siblings.
    }
  }
  return authenticated;
}

/** Authenticate up to 32 generic candidates and keep only notification-eligible ones.
 * A watch must match the SIGNED header topics, not unsigned extra actual topics.
 */
export function verifyNotificationEnvelopes(frame: Uint8Array, actualGenesis: string, actualChannel: string, actualTopics: readonly string[], nowMs: number): NotificationEnvelope[] {
  if (!Number.isSafeInteger(nowMs) || nowMs < 0) throw new Error("invalid notification clock");
  return authenticateNotificationEnvelopes(frame, actualGenesis, actualChannel, actualTopics).filter(({ header }) =>
    header.createdAt <= nowMs + FUTURE_SKEW_MS && header.expiresAt > nowMs);
}
