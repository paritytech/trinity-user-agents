// Programmatic wire-equality loop.
//
// `wire-equality.test.ts` exercises a handful of hand-picked frames. This file
// iterates every generated (trait, method) frame pair and asserts the codec
// round-trips a sentinel payload and produces the expected byte layout for
// each.

import type { Result } from "neverthrow";
import { describe, expect, it } from "bun:test";

import { str } from "./scale.js";
import {
  MESSAGE_TYPE_REQUEST,
  decodeWireMessage,
  encodeWireMessage,
} from "./transport.js";
import * as W from "./generated/wire-table.js";

function toHex(u: Uint8Array): string {
    return Array.from(u)
        .map((b) => b.toString(16).padStart(2, "0"))
        .join("");
}

function expectedWire(
    reqId: string,
    traitId: number,
    methodId: number,
    messageType: number,
    valueBytes: Uint8Array,
): Uint8Array {
    const idBytes = str.enc(reqId);
    const out = new Uint8Array(idBytes.length + 3 + valueBytes.length);
    out.set(idBytes, 0);
    out[idBytes.length] = traitId;
    out[idBytes.length + 1] = methodId;
    out[idBytes.length + 2] = messageType;
    out.set(valueBytes, idBytes.length + 3);
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

interface WireTableEntry {
    trait: number;
    method: number;
    kind: "request" | "subscription";
}

const frames = Object.entries(W as Record<string, WireTableEntry>).map(
    ([name, ids]) => ({ name, traitId: ids.trait, methodId: ids.method }),
);

describe("generated wire-table round-trip", () => {
    it("exposes a non-empty set of wire frame constants", () => {
        expect(frames.length).toBeGreaterThan(0);
    });

    it("gives every constant a trait discriminant", () => {
        for (const [name, ids] of Object.entries(W as Record<string, WireTableEntry>)) {
            expect(Number.isInteger(ids.trait), `${name} is missing a trait id`).toBe(true);
        }
    });

    // Per-pair sentinel payload so any cross-talk between pairs surfaces as a
    // concrete byte mismatch rather than a silent equality.
    it.each(frames)("round-trips $name (pair ($traitId, $methodId))", ({ traitId, methodId }) => {
        const sentinel = new Uint8Array([traitId, methodId, 0xa5, ~methodId & 0xff, 0x5a]);
        const requestId = `r:${traitId}:${methodId}`;

        const encoded = unwrap(
            encodeWireMessage({
                requestId,
                payload: {
                    traitId,
                    methodId,
                    messageType: MESSAGE_TYPE_REQUEST,
                    value: sentinel,
                },
            }),
            "encode",
        );
        expect(toHex(encoded)).toBe(
            toHex(expectedWire(requestId, traitId, methodId, MESSAGE_TYPE_REQUEST, sentinel)),
        );

        const decoded = unwrap(decodeWireMessage(encoded), "decode");
        expect(decoded.requestId).toBe(requestId);
        expect(decoded.payload.traitId).toBe(traitId);
        expect(decoded.payload.methodId).toBe(methodId);
        expect(decoded.payload.messageType).toBe(MESSAGE_TYPE_REQUEST);
        expect(toHex(decoded.payload.value)).toBe(toHex(sentinel));
    });
});
