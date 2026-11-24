import { describe, expect, it } from "bun:test";
import { RendererNode, type GestureEvent } from "./generated/types.js";
import { clearCells, coverBits, decodeGestureEvent, encodeGestureEvent, setOffset } from "./gesture-event.js";

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
        expect(Array.from(RendererNode.enc(node))).toEqual([13, 0, 0, 0, 0, 0, 0, 0, 0, 4, 0]);
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
                        key: "thumb",
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
        const node = setOffset("thumb", 1.2, -0.02);
        expect(node).toEqual({ tag: "SetOffset", value: { key: "thumb", x: 120, y: -2 } });
        const bytes = [14, 20, 116, 104, 117, 109, 98, 120, 0, 0, 0, 254, 255, 255, 255];
        expect(Array.from(RendererNode.enc(node))).toEqual(bytes);
        expect(RendererNode.dec(Uint8Array.from(bytes))).toEqual(node);
    });

    it("carries an offset in hundredths of a logical pixel", () => {
        expect(setOffset("thumb", 10.606)).toEqual({ tag: "SetOffset", value: { key: "thumb", x: 1061, y: 0 } });
    });

    it("clears cover cells with the Rust bytes", () => {
        const node = clearCells("foil", [3, 300]);
        expect(node).toEqual({ tag: "ClearCells", value: { key: "foil", cells: [3, 300] } });
        const bytes = [16, 16, 102, 111, 105, 108, 8, 3, 0, 44, 1];
        expect(Array.from(RendererNode.enc(node))).toEqual(bytes);
        expect(RendererNode.dec(Uint8Array.from(bytes))).toEqual(node);
    });

    it("draws a cover grid with the Rust bytes", () => {
        const node: RendererNode = {
            tag: "CoverGrid",
            value: { modifiers: [], props: { key: "f", columns: 2, rows: 1, cleared: coverBits([1], 2, 1) }, children: [{ tag: "Nil" }] },
        };
        expect(Array.from(RendererNode.enc(node))).toEqual([15, 0, 4, 102, 2, 0, 1, 0, 4, 2, 0, 0, 4, 0]);
    });

    it("asks for a cover's reveal with the Rust bytes", () => {
        const reveal = { pan: "p", atPermille: 550, action: "r" };
        const node: RendererNode = {
            tag: "CoverGrid",
            value: { modifiers: [], props: { key: "f", columns: 2, rows: 1, cleared: coverBits([1], 2, 1), reveal }, children: [{ tag: "Nil" }] },
        };
        const bytes = [15, 0, 4, 102, 2, 0, 1, 0, 4, 2, 0, 1, 4, 112, 38, 2, 4, 114, 4, 0];
        expect(Array.from(RendererNode.enc(node))).toEqual(bytes);
        expect(RendererNode.dec(Uint8Array.from(bytes))).toEqual(node);
    });

    it("packs cleared cells lowest bit first and ignores cells outside the grid", () => {
        expect(coverBits([0, 2, 11, 12, -1], 4, 3)).toBe("0x0508");
    });

    it("carries a location pan as its last flag", () => {
        const pan = { axis: "Free" as const, location: true };
        const node: RendererNode = { tag: "Gesture", value: { modifiers: [], props: { pan }, children: [] } };
        const decoded = RendererNode.dec(RendererNode.enc(node));
        expect(decoded).toEqual({ tag: "Gesture", value: { modifiers: [], props: { pan: { axis: "Free", location: true } }, children: [] } });
    });
});
