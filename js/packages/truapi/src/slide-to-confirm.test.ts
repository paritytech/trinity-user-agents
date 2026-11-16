import { describe, expect, it } from "bun:test";
import * as T from "./generated/types.js";

// Matches `slide_to_confirm_is_variant_twelve` in rust/crates/truapi/src/v01/renderer/tests.rs,
// so a product built on this client and a host built on the core read the same bytes.
describe("RendererNode SlideToConfirm", () => {
    const minimal: T.RendererNode = {
        tag: "SlideToConfirm",
        value: {
            modifiers: [],
            props: { text: "Go", variant: "Reveal", confirmed: true, confirmAction: "r" },
        },
    };

    it("is variant 12, after Materials at 11", () => {
        expect(Array.from(T.RendererNode.enc(minimal))).toEqual([
            // Variant 12, then an empty modifier list.
            12, 0,
            // "Go", compact length 2.
            8, 0x47, 0x6f,
            // No second line, no confirmed text, Some(Reveal), no accent.
            0, 0, 1, 1, 0,
            // OptionBool is one byte: enabled and loading absent, confirmed true.
            0, 0, 1,
            // Some("r").
            1, 4, 0x72,
        ]);
    });

    it("round-trips every field the product can set", () => {
        const full: T.RendererNode = {
            tag: "Column",
            value: {
                modifiers: [],
                props: {},
                children: [
                    {
                        tag: "SlideToConfirm",
                        value: {
                            modifiers: [{ tag: "FillWidth", value: true }],
                            props: {
                                text: "Slide to register",
                                secondaryText: "Fri 14 Nov",
                                confirmedText: "Registered",
                                variant: "Thumb",
                                accent: "FgWarning",
                                enabled: false,
                                loading: true,
                                confirmed: false,
                                confirmAction: "jollity.register",
                            },
                        },
                    },
                ],
            },
        };

        expect(T.RendererNode.dec(T.RendererNode.enc(full))).toEqual(full);
    });

    it("leaves what the product does not set absent", () => {
        const bare: T.RendererNode = {
            tag: "SlideToConfirm",
            value: { modifiers: [], props: { text: "Slide to register" } },
        };

        expect(T.RendererNode.dec(T.RendererNode.enc(bare))).toEqual(bare);
    });
});
