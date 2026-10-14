// Wire byte-equality smoke test.
//
// `encodeWireMessage` must produce the same bytes as the Rust `ProtocolMessage`
// encoder for the canonical reference vectors. The Rust side pins these in
// `crates/truapi/src/frame.rs` (`mod tests`); we compute them here
// independently and compare.

import { readFileSync } from "node:fs";

import type { Result } from "neverthrow";
import { describe, expect, it } from "bun:test";

import { TRUAPI_CODEC_VERSION } from "./generated/client.js";
import { str } from "./scale.js";
import {
  MESSAGE_TYPE_REQUEST,
  decodeWireMessage,
  encodeWireMessage,
} from "./transport.js";
import * as T from "./generated/types.js";
import * as W from "./generated/wire-table.js";
import { HostSignPayloadRequest } from "./generated/types.js";

function toHex(u: Uint8Array): string {
    return Array.from(u)
        .map((b) => b.toString(16).padStart(2, "0"))
        .join("");
}

function expectedWire(
    traitId: number,
    methodId: number,
    messageType: number,
    valueBytes: Uint8Array,
): Uint8Array {
    const reqId = str.enc("p:1");
    const out = new Uint8Array(reqId.length + 3 + valueBytes.length);
    out.set(reqId, 0);
    out[reqId.length] = traitId;
    out[reqId.length + 1] = methodId;
    out[reqId.length + 2] = messageType;
    out.set(valueBytes, reqId.length + 3);
    return out;
}

/** Return the successful result value or fail the test with context. */
function unwrap<T>(result: Result<T, { message: string }>, message: string): T {
    return result.match(
        (value) => value,
        (error): never => {
            throw new Error(`${message}: ${error.message}`);
        },
    );
}

describe("encodeWireMessage / decodeWireMessage wire equality", () => {
    it("matches Rust signing bytes for absent, true, and false transaction flags", () => {
        for (const [flag, byte] of [
            [undefined, "00"],
            [true, "01"],
            [false, "02"],
        ] as const) {
            const request: HostSignPayloadRequest = {
                account: {
                    dotNsIdentifier: "myapp.dot",
                    derivationIndex: { tag: "Index", value: 7 },
                },
                payload: {
                    blockHash: "0x",
                    blockNumber: "0x",
                    era: "0x",
                    genesisHash: "0x",
                    method: "0x",
                    nonce: "0x",
                    specVersion: "0x",
                    tip: "0x",
                    transactionVersion: "0x",
                    signedExtensions: [],
                    version: 4,
                    assetId: undefined,
                    metadataHash: undefined,
                    mode: undefined,
                    withSignedTransaction: flag,
                },
            };
            const encoded = HostSignPayloadRequest.enc(request);
            expect(toHex(encoded)).toBe(
                `246d796170702e646f7400070000000000000000000000000004000000000000${byte}`,
            );
            expect(HostSignPayloadRequest.dec(encoded)).toEqual(request);
        }
    });

    it("pins the handshake frame end-to-end: requestId + 0x01 0x00 0x00 + payload", () => {
        // Trait 1 = system, method 0 = handshake request. The handshake is the
        // first frame either side sends, so its envelope must never drift.
        expect(W.SYSTEM_HANDSHAKE.trait).toBe(1);
        expect(W.SYSTEM_HANDSHAKE.method).toBe(0);

        // Built the way the generated `system.handshake` builds it: the V1
        // request wrapper followed by the codec version the client speaks. A
        // hand-rolled codec byte here would keep passing while the generated
        // client drifted away from the host it talks to.
        const inner = T.VersionedHostHandshakeRequest.enc({
            tag: "V1",
            value: { codecVersion: TRUAPI_CODEC_VERSION },
        });
        expect(Array.from(inner)).toEqual([0x00, TRUAPI_CODEC_VERSION]);

        const encoded = unwrap(
            encodeWireMessage({
                requestId: "p:1",
                payload: {
                    traitId: W.SYSTEM_HANDSHAKE.trait,
                    methodId: W.SYSTEM_HANDSHAKE.method,
                    messageType: MESSAGE_TYPE_REQUEST,
                    value: inner,
                },
            }),
            "encode handshake_request",
        );
        // [0c 70 3a 31] "p:1" + [01] system trait + [00] handshake request
        // + [00] messageType=Request + payload.
        expect(toHex(encoded)).toBe(`0c703a31010000${toHex(inner)}`);
        expect(toHex(encoded)).toBe(toHex(expectedWire(1, 0, MESSAGE_TYPE_REQUEST, inner)));

        const decoded = unwrap(decodeWireMessage(encoded), "decode handshake_request");
        expect(decoded.payload.traitId).toBe(1);
        expect(decoded.payload.methodId).toBe(0);
        expect(decoded.payload.messageType).toBe(MESSAGE_TYPE_REQUEST);
        expect(toHex(decoded.payload.value)).toBe(toHex(inner));
    });

    it("speaks the codec version the host's handshake accepts", () => {
        // `system.handshake` answers `UnsupportedProtocolVersion` unless the
        // client's codec version equals `truapi::WIRE_CODEC_VERSION` exactly,
        // so a client generated with a pinned `--codec-version` is rejected by
        // the host built from the same tree. Read the constant the host
        // compares against rather than restating the number here.
        const lib = readFileSync(
            new URL("../../../../rust/crates/truapi/src/lib.rs", import.meta.url),
            "utf8",
        );
        const declaration = /pub const WIRE_CODEC_VERSION: u8 = (\d+);/.exec(lib);
        if (declaration === null) {
            throw new Error(
                "no WIRE_CODEC_VERSION declaration in rust/crates/truapi/src/lib.rs",
            );
        }
        expect(TRUAPI_CODEC_VERSION).toBe(Number(declaration[1]));
    });

    it("encodes account_get_request (pair (2, 1)) to match the golden fixture", () => {
        // Same vector as the Rust golden fixture
        // (`truapi/tests/snapshots/golden-account-get.bin`). Encoded
        // through the generated codec rather than assembled byte by byte: a
        // hand-rolled payload keeps encoding the layout it was written against
        // long after the type has moved on, which is exactly how the Rust
        // fixture went stale across the 0.6.0 `DerivationIndex` change.
        const inner = T.VersionedHostAccountGetRequest.enc({
            tag: "V1",
            value: {
                productAccountId: {
                    dotNsIdentifier: "foo",
                    derivationIndex: { tag: "Index", value: 0 },
                },
            },
        });
        const encoded = unwrap(
            encodeWireMessage({
                requestId: "p:1",
                payload: {
                    traitId: W.ACCOUNT_GET_ACCOUNT.trait,
                    methodId: W.ACCOUNT_GET_ACCOUNT.method,
                    messageType: MESSAGE_TYPE_REQUEST,
                    value: inner,
                },
            }),
            "encode account_get_request",
        );
        expect(toHex(encoded)).toBe(
            toHex(expectedWire(2, 1, MESSAGE_TYPE_REQUEST, inner)),
        );
        // [0c 70 3a 31] "p:1" + [02 01] pair + [00] messageType=Request
        // + [00] request wrapper V1 + [0c 66 6f 6f] "foo"
        // + [00] DerivationIndex::Index + [00 00 00 00] u32 = 0.
        //
        // Byte-for-byte identical to the pre-messageType-byte fixture: the
        // wrapper's own V1 tag now sits where the old envelope's direction
        // tag used to, and both happen to be 0x00.
        expect(toHex(encoded)).toBe("0c703a31020100000c666f6f0000000000");
    });

    it("round-trips a local_storage_read frame through encode + decode", () => {
        const inner = new Uint8Array([0x00, 0x42, 0xab, 0xcd]);
        const encoded = unwrap(
            encodeWireMessage({
                requestId: "p:1",
                payload: {
                    traitId: W.LOCAL_STORAGE_READ.trait,
                    methodId: W.LOCAL_STORAGE_READ.method,
                    messageType: MESSAGE_TYPE_REQUEST,
                    value: inner,
                },
            }),
            "encode local_storage_read_request",
        );
        const decoded = unwrap(decodeWireMessage(encoded), "decode local_storage_read_request");
        expect(decoded.requestId).toBe("p:1");
        expect(decoded.payload.traitId).toBe(W.LOCAL_STORAGE_READ.trait);
        expect(decoded.payload.methodId).toBe(W.LOCAL_STORAGE_READ.method);
        expect(decoded.payload.messageType).toBe(MESSAGE_TYPE_REQUEST);
        expect(toHex(decoded.payload.value)).toBe(toHex(inner));
    });

    it("rejects an invalid outbound trait discriminant", () => {
        const result = encodeWireMessage({
            requestId: "p:1",
            payload: {
                traitId: 256,
                methodId: 0,
                messageType: MESSAGE_TYPE_REQUEST,
                value: new Uint8Array(),
            },
        });
        expect(result.isErr()).toBe(true);
        expect(result._unsafeUnwrapErr().message).toMatch(/Invalid wire trait discriminant/);
    });

    it("rejects an invalid outbound method discriminant", () => {
        const result = encodeWireMessage({
            requestId: "p:1",
            payload: {
                traitId: 0,
                methodId: 256,
                messageType: MESSAGE_TYPE_REQUEST,
                value: new Uint8Array(),
            },
        });
        expect(result.isErr()).toBe(true);
        expect(result._unsafeUnwrapErr().message).toMatch(/Invalid wire method discriminant/);
    });

    it("rejects an invalid outbound message type", () => {
        const result = encodeWireMessage({
            requestId: "p:1",
            payload: {
                traitId: 0,
                methodId: 0,
                messageType: 256,
                value: new Uint8Array(),
            },
        });
        expect(result.isErr()).toBe(true);
        expect(result._unsafeUnwrapErr().message).toMatch(/Invalid wire message type/);
    });

    it("rejects a truncated frame with no trait byte", () => {
        const truncated = str.enc("p:1"); // just the requestId, nothing after.
        const result = decodeWireMessage(truncated);
        expect(result.isErr()).toBe(true);
        expect(result._unsafeUnwrapErr().message).toMatch(/missing trait discriminant byte/);
    });

    it("rejects a truncated frame with a trait byte but no method byte", () => {
        const reqId = str.enc("p:1");
        const truncated = new Uint8Array(reqId.length + 1);
        truncated.set(reqId, 0);
        truncated[reqId.length] = 0x00;
        const result = decodeWireMessage(truncated);
        expect(result.isErr()).toBe(true);
        expect(result._unsafeUnwrapErr().message).toMatch(/missing method discriminant byte/);
    });

    it("rejects a truncated frame with a method byte but no message-type byte", () => {
        const reqId = str.enc("p:1");
        const truncated = new Uint8Array(reqId.length + 2);
        truncated.set(reqId, 0);
        truncated[reqId.length] = 0x00;
        truncated[reqId.length + 1] = 0x00;
        const result = decodeWireMessage(truncated);
        expect(result.isErr()).toBe(true);
        expect(result._unsafeUnwrapErr().message).toMatch(/missing message-type byte/);
    });

    it("round-trips a 32 KiB requestId via the mode-2 compact-len prefix", () => {
        // Mirrors Rust `max_length_request_id_mode_two_round_trips` so both sides
        // agree the high-shift branch in scanStrEnd is correct.
        const longId = "y".repeat(32 * 1024);
        const inner = new Uint8Array([0x00, 0xab, 0xcd]);
        const encoded = unwrap(
            encodeWireMessage({
                requestId: longId,
                payload: {
                    traitId: W.ACCOUNT_GET_ACCOUNT.trait,
                    methodId: W.ACCOUNT_GET_ACCOUNT.method,
                    messageType: MESSAGE_TYPE_REQUEST,
                    value: inner,
                },
            }),
            "encode long-id account_get_request",
        );
        // Confirm a mode-2 prefix was actually emitted; otherwise the test would
        // silently exercise mode-0/1 and the assertion would be hollow.
        expect(encoded[0] & 0b11).toBe(0b10);
        const decoded = unwrap(decodeWireMessage(encoded), "decode long-id account_get_request");
        expect(decoded.requestId).toBe(longId);
        expect(decoded.payload.traitId).toBe(W.ACCOUNT_GET_ACCOUNT.trait);
        expect(decoded.payload.methodId).toBe(W.ACCOUNT_GET_ACCOUNT.method);
        expect(decoded.payload.messageType).toBe(MESSAGE_TYPE_REQUEST);
        expect(toHex(decoded.payload.value)).toBe(toHex(inner));
    });
});
