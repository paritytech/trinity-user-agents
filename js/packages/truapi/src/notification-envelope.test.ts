import { describe, expect, it } from "bun:test";
import { sha256 } from "@noble/hashes/sha2.js";
import { concatBytes, hexToBytes } from "@noble/hashes/utils.js";
import {
  authenticateNotificationEnvelope, authenticateNotificationHeader, decodeNotificationEnvelope, encodeNotificationEnvelope,
  isNotificationEnvelope, notificationSigningBytes, signNotificationEnvelope, signNotificationHeader,
  verifyNotificationEnvelope, verifyNotificationEnvelopes, MAX_CARRIER_BYTES, MAX_FULL_FRAME_BYTES,
  MAX_HEADER_BYTES, type NotificationHeader,
} from "./notification-envelope.js";

// Independent OpenSSL Ed25519 vector: raw seed 00..1f, subject 'abc'.
const header: NotificationHeader = {
  v: 1, product: "example.paseo", genesis: "11".repeat(32), channel: "55".repeat(32),
  topics: ["22".repeat(32), "33".repeat(32)], eventId: "44".repeat(32),
  createdAt: 1700000000000, expiresAt: 1700000060000,
  ciphertextDigest: "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
  senderKey: "03a107bff3ce10be1d70dd18e74bc09967e4d6309ba50d5f1ddc8664125531b8",
  signature: "6bc192199982a1b8e850b66b1f0cd85035354e0b788edecddfad7505da94c7347447b5fa4c9a64647045eea6d52e4aecec14dcb16ea64de317b1b3e76b0f830b",
};
const seed = hexToBytes("000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f");
const encoder = new TextEncoder();
const carrier = encoder.encode("abc");
const { product, genesis, channel, topics, eventId, createdAt, expiresAt } = header;
const metadata = { product, genesis, channel, topics, eventId, createdAt, expiresAt };

// Independent test-only implementation of the standard base Envelope grammar.
interface Element { bytes: Uint8Array; digest: Uint8Array }
function head(major: number, length: number): Uint8Array {
  if (length < 24) return Uint8Array.of(major * 32 + length);
  if (length < 256) return Uint8Array.of(major * 32 + 24, length);
  if (length < 65536) return Uint8Array.of(major * 32 + 25, length >>> 8, length & 255);
  return Uint8Array.of(major * 32 + 26, length >>> 24, length >>> 16 & 255, length >>> 8 & 255, length & 255);
}
function leaf(value: string | Uint8Array): Element {
  const bytes = typeof value === "string" ? encoder.encode(value) : value;
  const encoded = concatBytes(head(typeof value === "string" ? 3 : 2, bytes.length), bytes);
  return { bytes: concatBytes(Uint8Array.of(0xd8, 201), encoded), digest: sha256(encoded) };
}
function assertion(predicate: string, object: Element): Element {
  const key = leaf(predicate);
  return { bytes: concatBytes(Uint8Array.of(0xa1), key.bytes, object.bytes), digest: sha256(concatBytes(key.digest, object.digest)) };
}
function node(subject: Element, assertions: Element[]): Element {
  const sorted = [...assertions].sort((left, right) => {
    for (let index = 0; index < 32; index++) if (left.digest[index] !== right.digest[index]) return left.digest[index]! - right.digest[index]!;
    return 0;
  });
  return { bytes: concatBytes(head(4, sorted.length + 1), subject.bytes, ...sorted.map((entry) => entry.bytes)),
    digest: sha256(concatBytes(subject.digest, ...sorted.map((entry) => entry.digest))) };
}
function frame(json = JSON.stringify(header), body = carrier): Uint8Array {
  return concatBytes(Uint8Array.of(0xd8, 200), node(leaf(body), [assertion("truapiNotification", leaf(json))]).bytes);
}
function verify(bytes: Uint8Array) {
  return verifyNotificationEnvelope(bytes, genesis, channel, topics, createdAt);
}

describe("standard generic notification carriers", () => {
  it("matches the independent fixed signing bytes, signature and standard encoding used by Rust", () => {
    expect(new TextDecoder().decode(notificationSigningBytes(header))).toBe('["truapi:notification:v1",1,"example.paseo","' + genesis + '","' + channel + '",["' + topics[0] + '","' + topics[1] + '"],"' + eventId + '",1700000000000,1700000060000,"ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad","03a107bff3ce10be1d70dd18e74bc09967e4d6309ba50d5f1ddc8664125531b8"]');
    expect(signNotificationHeader(metadata, carrier, seed)).toEqual(header);
    const signed = signNotificationEnvelope(metadata, carrier, seed);
    expect(decodeNotificationEnvelope(signed).header).toEqual(header);
    expect(verify(frame()).carrier).toEqual(carrier);
    expect(encodeNotificationEnvelope(header, carrier)).toEqual(frame());
  });

  it("matches Dalek's exact equation without blanket mixed-torsion rejection", () => {
    // Independently constructed with scalar a=r=1 and order-two T=(0,-1).
    // R=B+T, A=B satisfies only the cofactored equation and MUST fail.
    const cofactored = { ...header,
      senderKey: "5866666666666666666666666666666666666666666666666666666666666666",
      signature: "95999999999999999999999999999999999999999999999999999999999999994d0d311b42ae0fbd5231e2b7e106d734ca5e5045ba0882ac54c3f232e5718300",
    };
    expect(() => authenticateNotificationHeader(JSON.stringify(cofactored), carrier)).toThrow();
    expect(() => verify(frame(JSON.stringify(cofactored)))).toThrow();
    // A=B+T, R=B and even reduced challenge satisfies the exact equation.
    const mixedKey = { ...header, eventId: "00".repeat(31) + "04",
      senderKey: "9599999999999999999999999999999999999999999999999999999999999999",
      signature: "58666666666666666666666666666666666666666666666666666666666666663114ba2ce5c96de9e15fbc99e889f60672480566a4420d0d7806399239ed5a06",
    };
    expect(authenticateNotificationHeader(JSON.stringify(mixedKey), carrier)).toEqual(mixedKey);
    expect(verify(frame(JSON.stringify(mixedKey))).header).toEqual(mixedKey);
  });

  it("authenticates optional stored metadata without an actual-source or clock claim", () => {
    expect(authenticateNotificationHeader(JSON.stringify(header), carrier)).toEqual(header);
    expect(() => authenticateNotificationHeader(JSON.stringify(header), encoder.encode("different"))).toThrow();
    expect(() => authenticateNotificationHeader(JSON.stringify({ ...header, eventId: "66".repeat(32) }), carrier)).toThrow();
    expect(() => authenticateNotificationHeader(JSON.stringify(header).replace('"v":1', '"v":1,"v":1'), carrier)).toThrow();
    expect(() => authenticateNotificationHeader(JSON.stringify(header), new Uint8Array(MAX_CARRIER_BYTES + 1))).toThrow();
    expect(() => verifyNotificationEnvelope(frame(), genesis, "00".repeat(32), topics, expiresAt)).toThrow();
  });

  it("authenticates history without making expired candidates notification-eligible", () => {
    const bytes = frame();
    const authenticated = authenticateNotificationEnvelope(bytes, genesis, channel, topics);
    expect(() => verifyNotificationEnvelope(bytes, genesis, channel, topics, expiresAt)).toThrow();
    bytes[7] ^= 1;
    expect(authenticated.carrier).toEqual(carrier);
  });

  it("resolves a byte witness elsewhere without interpreting application predicates", () => {
    const prior = node(leaf("unrelated subject"), [
      assertion("opaque-slot", leaf(carrier)), assertion("truapiNotification", leaf(JSON.stringify(header))),
    ]);
    const outer = node(leaf("control"), [assertion("application-defined", prior)]);
    const bytes = concatBytes(Uint8Array.of(0xd8, 200), outer.bytes);
    expect(isNotificationEnvelope(bytes)).toBe(true);
    expect(verify(bytes).carrier).toEqual(carrier);
    expect(verifyNotificationEnvelopes(bytes, genesis, channel, ["66".repeat(32), ...topics], createdAt)).toHaveLength(1);
    expect(verifyNotificationEnvelopes(bytes, genesis, channel, [topics[0]!], createdAt)).toEqual([]);
  });

  it("does not let an invalid sibling suppress a valid authenticated candidate", () => {
    const valid = node(leaf(carrier), [assertion("truapiNotification", leaf(JSON.stringify(header)))]);
    const forged = node(leaf("another subject"), [assertion("truapiNotification", leaf(JSON.stringify({ ...header, signature: "00".repeat(64) })))]);
    const bytes = concatBytes(Uint8Array.of(0xd8, 200), node(leaf("container"), [assertion("first", valid), assertion("second", forged)]).bytes);
    expect(verifyNotificationEnvelopes(bytes, genesis, channel, topics, createdAt)).toHaveLength(1);
    expect(() => verify(bytes)).toThrow("exactly one");
    expect(verifyNotificationEnvelopes(frame(JSON.stringify(header), encoder.encode("wrong")), genesis, channel, topics, createdAt)).toEqual([]);
  });

  it("discriminates unmarked carriers and fails closed on structural ambiguities", () => {
    const unmarked = concatBytes(Uint8Array.of(0xd8, 200), node(leaf(carrier), [assertion("scheme", leaf("opaque"))]).bytes);
    expect(isNotificationEnvelope(unmarked)).toBe(false);
    expect(isNotificationEnvelope(encoder.encode("ordinary bytes"))).toBe(false);
    expect(isNotificationEnvelope(frame("not valid JSON"))).toBe(true);
    const duplicate = concatBytes(Uint8Array.of(0xd8, 200), node(leaf(carrier), [
      assertion("truapiNotification", leaf(JSON.stringify(header))), assertion("truapiNotification", leaf("different")),
    ]).bytes);
    expect(() => isNotificationEnvelope(duplicate)).toThrow();
    expect(() => verify(duplicate)).toThrow();
    expect(() => verify(concatBytes(frame(), Uint8Array.of(0)))).toThrow();
    expect(() => verify(concatBytes(Uint8Array.of(0xd9, 0, 200), frame().subarray(2)))).toThrow();
    const malformedUtf8 = frame(); malformedUtf8[malformedUtf8.indexOf(0x7b) + 1] = 0xff;
    expect(() => verify(malformedUtf8)).toThrow();
  });

  it("rejects duplicate/escaped duplicate/unknown/missing fields and noninteger JSON", () => {
    const json = JSON.stringify(header);
    for (const invalid of [
      json.replace('"v":1', '"v":1,"v":1'), json.replace('"v":1', '"v":1,"\\u0076":1'),
      json.replace('"v":1', '"v":1,"extra":false'), json.replace('"v":1,', ""),
      json.replace('"v":1', '"v":1.0'), json.replace('"v":1', '"v":1e0'), json.replace('"v":1', '"v":-0'),
    ]) expect(() => verify(frame(invalid))).toThrow();
  });

  it("rejects invalid metadata, source, proof and lifetime", () => {
    for (const patch of [
      { product: "😀" }, { product: "A" }, { product: "x".repeat(129) }, { product: "" },
      { genesis: "AA".repeat(32) }, { channel: "0x" + channel }, { eventId: "66".repeat(32) },
      { topics: [] }, { topics: [topics[0], topics[0]] }, { topics: Array(5).fill(topics[0]) },
      { createdAt: Number.MAX_SAFE_INTEGER + 1 }, { createdAt: -1 }, { expiresAt: createdAt },
      { expiresAt: createdAt + 86_400_001 }, { signature: "00".repeat(64) }, { senderKey: "00".repeat(32) },
      { ciphertextDigest: "00".repeat(32) },
    ]) expect(() => verify(frame(JSON.stringify({ ...header, ...patch })))).toThrow();
    expect(() => verifyNotificationEnvelope(frame(), "00".repeat(32), channel, topics, createdAt)).toThrow();
    expect(() => verifyNotificationEnvelope(frame(), genesis, "00".repeat(32), topics, createdAt)).toThrow();
    expect(() => verifyNotificationEnvelope(frame(), genesis, channel, topics, createdAt - 60_001)).toThrow();
  });

  it("bounds container bytes, headers, candidate count and nesting", () => {
    expect(() => verify(new Uint8Array(MAX_FULL_FRAME_BYTES + 1))).toThrow();
    expect(() => signNotificationEnvelope(metadata, new Uint8Array(MAX_CARRIER_BYTES + 1), seed)).toThrow();
    expect(() => verify(frame(" ".repeat(MAX_HEADER_BYTES + 1)))).toThrow();
    const candidates = Array.from({ length: 33 }, (_, index) => assertion(`entry-${index}`, node(leaf(String(index)), [assertion("truapiNotification", leaf(JSON.stringify(header)))])));
    expect(() => verify(concatBytes(Uint8Array.of(0xd8, 200), node(leaf(carrier), candidates).bytes))).toThrow();
    let nested = leaf(carrier);
    for (let index = 0; index < 20; index++) nested = node(leaf("subject"), [assertion("nested", nested)]);
    expect(() => verify(concatBytes(Uint8Array.of(0xd8, 200), nested.bytes))).toThrow();
  });
});
