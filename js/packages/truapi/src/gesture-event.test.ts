import { describe, expect, it } from "bun:test";
import { RendererNode, type GestureEvent } from "./generated/types.js";
import { decodeGestureEvent, encodeGestureEvent, extendMask, maskCircle, maskStroke, setOffset } from "./gesture-event.js";

describe("gesture wire contract", () => {
    it("matches the Rust payload vector", () => {
        const event: GestureEvent = { phase: "Completed", x: 240, y: 0, fractionPermille: 1000 };
        const bytes = Uint8Array.from([5, 240, 0, 0, 0, 0, 0, 0, 0, 232, 3]);
        expect(encodeGestureEvent(event)).toEqual(bytes);
        expect(decodeGestureEvent(bytes)).toEqual(event);
    });

    it("keeps Gesture at 12 and preserves its children", () => {
        const node: RendererNode = {
            tag: "Gesture",
            value: { modifiers: [], props: {}, children: [{ tag: "Nil" }] },
        };
        expect(Array.from(RendererNode.enc(node))).toEqual([12, 0, 0, 0, 0, 0, 0, 0, 0, 4, 0]);
        expect(RendererNode.dec(RendererNode.enc(node))).toEqual(node);
    });

    it("round trips the worker and host movement modes", () => {
        for (const hostDriven of [false, true]) {
            const node: RendererNode = {
                tag: "Gesture",
                value: {
                    modifiers: [],
                    props: {
                        enabled: true,
                        hapticOnComplete: true,
                        accessibilityLabel: "Register",
                        key: "piece",
                        pan: {
                            axis: "Horizontal",
                            startAction: "start",
                            endAction: "end",
                            moveIntervalMs: 0,
                            ...(hostDriven
                                ? {
                                      drag: {
                                          travel: { tag: "ParentWidth" as const },
                                          release: {
                                              tag: "Complete" as const,
                                              value: { thresholdPercent: 90 },
                                          },
                                          holdAtEnd: true,
                                      },
                                  }
                                : { moveAction: "move" }),
                        },
                    },
                    children: [{ tag: "Nil" }],
                },
            };
            expect(RendererNode.dec(RendererNode.enc(node))).toEqual(node);
        }
    });

    it("sends an offset update with the Rust bytes", () => {
        const node = setOffset("piece", 1.2, -0.02);
        expect(node).toEqual({ tag: "SetOffset", value: { key: "piece", x: 120, y: -2 } });
        const bytes = [13, 20, 112, 105, 101, 99, 101, 120, 0, 0, 0, 254, 255, 255, 255];
        expect(Array.from(RendererNode.enc(node))).toEqual(bytes);
        expect(RendererNode.dec(Uint8Array.from(bytes))).toEqual(node);
    });

    it("carries an offset in hundredths of a logical pixel", () => {
        expect(setOffset("piece", 10.606)).toEqual({ tag: "SetOffset", value: { key: "piece", x: 1061, y: 0 } });
    });

    it("extends a mask with the Rust bytes", () => {
        const node = extendMask("mask", [maskCircle(1, -0.01, 26), maskStroke(52, [{ x: 0.01, y: 0.02 }])]);
        expect(node).toEqual({
            tag: "ExtendMask",
            value: {
                key: "mask",
                shapes: [
                    { tag: "Circle", value: { center: { x: 100, y: -1 }, radius: 2600 } },
                    { tag: "Stroke", value: { width: 5200, points: [{ x: 1, y: 2 }] } },
                ],
            },
        });
        const bytes = [
            14, 16, 109, 97, 115, 107, 8,
            1, 100, 0, 0, 0, 255, 255, 255, 255, 40, 10, 0, 0,
            2, 80, 20, 0, 0, 4, 1, 0, 0, 0, 2, 0, 0, 0,
        ];
        expect(Array.from(RendererNode.enc(node))).toEqual(bytes);
        expect(RendererNode.dec(Uint8Array.from(bytes))).toEqual(node);
    });

    it("answers a move with an empty extend of a few bytes", () => {
        expect(Array.from(RendererNode.enc(extendMask("mask.top")))).toHaveLength(11);
    });

    it("carries a mask modifier after ExitFade as the Rust bytes", () => {
        const mask = {
            key: "k",
            mode: "Show" as const,
            shapes: [{ tag: "All" as const }],
            follow: { pan: "p", width: 100 },
            fadeMs: 350,
            clearedShare: { atPermille: 550, action: "a" },
        };
        const node: RendererNode = { tag: "Box", value: { modifiers: [{ tag: "Mask", value: mask }], props: {}, children: [] } };
        const bytes = RendererNode.enc(node);
        // As `a_masked_box_has_the_bytes_the_client_pins` in the core: the follow's absent brush after its width.
        expect(Array.from(bytes)).toEqual([
            2, 4, 20, 1, 4, 107, 1, 4, 0, 1, 4, 112, 100, 0, 0, 0, 0, 1, 94, 1, 1, 38, 2, 4, 97, 0, 0,
        ]);
        expect(RendererNode.dec(bytes)).toEqual(node);
    });

    it("carries a location pan as its last flag", () => {
        const pan = { axis: "Free" as const, location: true };
        const node: RendererNode = { tag: "Gesture", value: { modifiers: [], props: { pan }, children: [] } };
        const decoded = RendererNode.dec(RendererNode.enc(node));
        expect(decoded).toEqual({ tag: "Gesture", value: { modifiers: [], props: { pan: { axis: "Free", location: true } }, children: [] } });
    });
});
