import { describe, expect, it } from "bun:test";
import { bool } from "./scale.js";

describe("bool", () => {
    it("rejects invalid SCALE booleans instead of treating them as approval", () => {
        for (const byte of [2, 255]) {
            expect(() => bool.dec(Uint8Array.of(byte))).toThrow("Invalid SCALE boolean");
        }
    });
});
