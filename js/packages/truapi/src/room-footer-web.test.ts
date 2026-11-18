import { describe, expect, it } from "bun:test";
import * as T from "./generated/types.js";

// Matches `a_web_footer_is_variant_three_and_round_trips` in
// rust/crates/truapi/src/versioned/chat/tests.rs, so a product built on this
// client and a host built on the core read the same bytes.
describe("ChatRoomFooter Web", () => {
    const web: T.ChatRoomFooter = { tag: "Web", value: { path: "f.html?a=1", height: 104 } };

    it("is variant 3, after Card, with the path then a u32 height", () => {
        expect(Array.from(T.ChatRoomFooter.enc(web))).toEqual([
            3,
            // "f.html?a=1", compact length 10.
            40,
            ...Array.from(new TextEncoder().encode("f.html?a=1")),
            // 104 as a little-endian u32.
            104, 0, 0, 0,
        ]);
    });

    it("keeps the indices the other footers already use", () => {
        expect(Array.from(T.ChatRoomFooter.enc("TextInput"))).toEqual([0]);
        expect(Array.from(T.ChatRoomFooter.enc("Empty"))).toEqual([1]);
        expect(T.ChatRoomFooter.enc({ tag: "Card", value: { messageType: "c", payload: "0x01" } })[0]).toBe(2);
    });

    it("round-trips a page with its query string", () => {
        const page: T.ChatRoomFooter = {
            tag: "Web",
            value: {
                path: "slide-footer.html?label=Slide%20to%20register&secondary=Nov%2014%20at%2010%3A00&action=jollity.register",
                height: 104,
            },
        };

        expect(T.ChatRoomFooter.dec(T.ChatRoomFooter.enc(page))).toEqual(page);
    });
});
