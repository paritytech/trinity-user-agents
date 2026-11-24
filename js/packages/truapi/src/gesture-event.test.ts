import { describe, expect, it } from "bun:test";
import { RendererNode, type GestureEvent } from "./generated/types.js";
import { decodeGestureEvent, encodeGestureEvent, setOffset } from "./gesture-event.js";

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
});
