import { describe, expect, it } from "bun:test";
import { TaggedUnion, _void, bool, str, withPlainVariants } from "./scale.js";

describe("bool", () => {
    it("rejects invalid SCALE booleans instead of treating them as approval", () => {
        for (const byte of [2, 255]) {
            expect(() => bool.dec(Uint8Array.of(byte))).toThrow("Invalid SCALE boolean");
        }
    });
});

describe("withPlainVariants", () => {
    const footer = withPlainVariants(
        TaggedUnion({ TextInput: _void, Empty: _void, Card: str }),
        ["TextInput", "Empty"],
    );

    it("encodes a plain string as the tagged value of the same name", () => {
        expect(footer.enc("Empty")).toEqual(footer.enc({ tag: "Empty" }));
        expect(footer.enc("TextInput")).toEqual(footer.enc({ tag: "TextInput" }));
    });

    it("keeps encoding tagged values, with or without a payload", () => {
        expect(footer.enc({ tag: "Empty" })).toEqual(Uint8Array.of(1));
        expect(footer.dec(footer.enc({ tag: "Card", value: "x" }))).toEqual({
            tag: "Card",
            value: "x",
        });
    });

    it("decodes to the tagged value, so a host never reads a plain string", () => {
        expect(footer.dec(footer.enc("Empty"))).toEqual({ tag: "Empty", value: undefined });
    });

    it("refuses a string outside the plain variants with the name it was given", () => {
        expect(() => footer.enc("Card" as "Empty")).toThrow("Unknown enum variant: Card");
    });
});
