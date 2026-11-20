import { describe, expect, it } from "bun:test";
import { RendererNode, type GestureEvent } from "./generated/types.js";
import { decodeGestureEvent, encodeGestureEvent } from "./gesture-event.js";

describe("gesture wire contract", () => {
    it("matches the Rust payload vector", () => {
        const event: GestureEvent = { phase: "Completed", x: 240, y: 0, fractionPermille: 1000 };
        const bytes = Uint8Array.from([5, 240, 0, 0, 0, 0, 0, 0, 0, 232, 3]);
        expect(encodeGestureEvent(event)).toEqual(bytes);
        expect(decodeGestureEvent(bytes)).toEqual(event);
    });

    it("keeps Gesture at 13 and preserves its children", () => {
        const node: RendererNode = {
            tag: "Gesture",
            value: { modifiers: [], props: {}, children: [{ tag: "Nil" }] },
        };
        expect(Array.from(RendererNode.enc(node))).toEqual([13, 0, 0, 0, 0, 0, 0, 4, 0]);
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
});
