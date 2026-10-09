import { sha256 } from "@noble/hashes/sha2.js";
import { bytesToHex, concatBytes } from "@noble/hashes/utils.js";

// Base Gordian Envelope grammar, independently implemented from
// https://datatracker.ietf.org/doc/html/draft-mcnally-envelope-12#section-3
// This notification profile accepts integer-only dCBOR leaves (no floats).
export const MAX_CONTAINER_BYTES = 256 * 1024;
export const NOTIFICATION_PREDICATE = "truapiNotification";
export const MAX_NOTIFICATION_CANDIDATES = 32;
const encoder = new TextEncoder();
const decoder = new TextDecoder("utf-8", { fatal: true, ignoreBOM: true });
const MAX_DEPTH = 16;
const MAX_ITEMS = 4096;
const MAX_ASSERTIONS = 128;
interface Item {
  major: number;
  argument: number;
  start: number;
  end: number;
  children: Item[];
  bytes?: Uint8Array;
  text?: string;
}
function compareBytes(left: Uint8Array, right: Uint8Array): number {
  for (let index = 0; index < Math.min(left.length, right.length); index++) {
    if (left[index] !== right[index]) return left[index]! - right[index]!;
  }
  return left.length - right.length;
}

/** Canonical definite-length, bounded integer-only dCBOR reader. */
function readContainer(bytes: Uint8Array): Item {
  if (bytes.length > MAX_CONTAINER_BYTES) throw new Error("notification container too large");
  let offset = 0;
  let remainingItems = MAX_ITEMS;
  function read(depth: number): Item {
    if (depth > MAX_DEPTH || --remainingItems < 0 || offset >= bytes.length) throw new Error("invalid CBOR bounds");
    const start = offset;
    const initial = bytes[offset++]!;
    const major = initial >>> 5;
    const additional = initial & 31;
    if (additional >= 28 || (major === 7 && additional > 23)) throw new Error("unsupported CBOR value");
    let argument = additional;
    if (additional >= 24) {
      const size = 2 ** (additional - 24);
      if (offset + size > bytes.length) throw new Error("truncated CBOR argument");
      argument = 0;
      for (let index = 0; index < size; index++) argument = argument * 256 + bytes[offset++]!;
      if (!Number.isSafeInteger(argument) || argument < [24, 256, 65536, 4294967296][additional - 24]!) throw new Error("noncanonical CBOR integer");
    }
    const item: Item = { major, argument, start, end: 0, children: [] };
    if (major === 2 || major === 3) {
      if (argument > bytes.length - offset) throw new Error("truncated CBOR bytes");
      item.bytes = bytes.subarray(offset, offset + argument);
      offset += argument;
      if (major === 3) {
        item.text = decoder.decode(item.bytes);
        if (item.text.normalize("NFC") !== item.text) throw new Error("noncanonical CBOR text");
      }
    } else if (major === 4 || major === 5 || major === 6) {
      const count = major === 6 ? 1 : argument * (major === 5 ? 2 : 1);
      if (count > remainingItems) throw new Error("CBOR item limit");
      for (let index = 0; index < count; index++) item.children.push(read(depth + 1));
      if (major === 5) {
        for (let index = 2; index < item.children.length; index += 2) {
          const prior = item.children[index - 2]!;
          const current = item.children[index]!;
          if (compareBytes(bytes.subarray(prior.start, prior.end), bytes.subarray(current.start, current.end)) >= 0) throw new Error("noncanonical CBOR map");
        }
      }
    } else if (major === 7 && ![20, 21, 22].includes(argument)) {
      throw new Error("unsupported CBOR simple value");
    }
    item.end = offset;
    return item;
  }
  const root = read(0);
  if (offset !== bytes.length) throw new Error("trailing CBOR data");
  return root;
}
interface Envelope {
  kind: "leaf" | "node" | "assertion" | "elided" | "wrapped";
  digest: Uint8Array;
  bytes?: Uint8Array;
  text?: string;
  predicate?: Envelope;
  object?: Envelope;
  assertions?: Envelope[];
}

/** Inspect bounded standard structure without interpreting application assertions. */
export function decodeNotificationContainer(bytes: Uint8Array): { headers: string[]; subjects: Map<string, Uint8Array> } {
  const root = readContainer(bytes);
  if (root.major !== 6 || root.argument !== 200) throw new Error("not a Gordian Envelope");
  const headers: string[] = [];
  const subjects = new Map<string, Uint8Array>();
  function envelope(item: Item): Envelope {
    if (item.major === 6 && item.argument === 201) {
      const value = item.children[0]!;
      if (value.major === 2) {
        const digest = bytesToHex(sha256(value.bytes!));
        const prior = subjects.get(digest);
        if (prior && compareBytes(prior, value.bytes!) !== 0) throw new Error("contradictory byte witness");
        subjects.set(digest, value.bytes!);
      }
      return { kind: "leaf", digest: sha256(bytes.subarray(value.start, value.end)), bytes: value.major === 2 ? value.bytes : undefined, text: value.text };
    }
    if (item.major === 2 && item.argument === 32) return { kind: "elided", digest: item.bytes! };
    if (item.major === 5 && item.argument === 1) {
      const predicate = envelope(item.children[0]!);
      const object = envelope(item.children[1]!);
      if (predicate.text === NOTIFICATION_PREDICATE && (predicate.kind !== "leaf" || object.kind !== "leaf" || object.text === undefined)) throw new Error("ambiguous notification assertion");
      if (predicate.text === NOTIFICATION_PREDICATE) {
        if (headers.length >= MAX_NOTIFICATION_CANDIDATES) throw new Error("notification candidate limit");
        headers.push(object.text!);
      }
      return { kind: "assertion", predicate, object, digest: sha256(concatBytes(predicate.digest, object.digest)) };
    }
    if (item.major === 6 && item.argument === 200) {
      const child = envelope(item.children[0]!);
      return { kind: "wrapped", digest: sha256(child.digest) };
    }
    if (item.major === 4 && item.argument >= 2 && item.argument <= MAX_ASSERTIONS + 1) {
      const subject = envelope(item.children[0]!);
      const assertions = item.children.slice(1).map(envelope);
      let reservedCount = 0;
      for (let index = 0; index < assertions.length; index++) {
        const assertion = assertions[index]!;
        if (assertion.kind !== "assertion" && assertion.kind !== "elided") throw new Error("invalid Envelope assertion");
        if (index > 0 && compareBytes(assertions[index - 1]!.digest, assertion.digest) >= 0) throw new Error("noncanonical Envelope assertion order");
        if (assertion.predicate?.text === NOTIFICATION_PREDICATE && ++reservedCount > 1) throw new Error("duplicate notification assertion");
      }
      return { kind: "node", bytes: subject.bytes, text: subject.text, assertions, digest: sha256(concatBytes(subject.digest, ...assertions.map((assertion) => assertion.digest))) };
    }
    throw new Error("unsupported Envelope structure");
  }
  envelope(root.children[0]!);
  return { headers, subjects };
}

function cborHead(major: number, argument: number): Uint8Array {
  if (argument < 24) return Uint8Array.of((major << 5) | argument);
  const size = argument <= 255 ? 1 : argument <= 65535 ? 2 : 4;
  const result = new Uint8Array(size + 1);
  result[0] = (major << 5) | (size === 1 ? 24 : size === 2 ? 25 : 26);
  for (let index = size; index > 0; index--) { result[index] = argument & 255; argument = Math.floor(argument / 256); }
  return result;
}

/** Encode the minimal standard carrier, leaving application assertions to its owner. */
export function encodeNotificationContainer(headerJson: string, carrier: Uint8Array): Uint8Array {
  const predicate = encoder.encode(NOTIFICATION_PREDICATE);
  const header = encoder.encode(headerJson);
  return concatBytes(
    Uint8Array.of(0xd8, 200, 0x82, 0xd8, 201), cborHead(2, carrier.length), carrier,
    Uint8Array.of(0xa1, 0xd8, 201), cborHead(3, predicate.length), predicate,
    Uint8Array.of(0xd8, 201), cborHead(3, header.length), header,
  );
}
