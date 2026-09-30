import { describe, expect, test } from "bun:test";
import { createMessagePortBridge } from "./transport.js";

const frame = (...bytes: number[]) => Uint8Array.from(bytes);

/** A host end and a container end, as the host page and the container would hold. */
function channel() {
    const { port1, port2 } = new MessageChannel();
    const received: Uint8Array[] = [];
    port1.onmessage = (event) => received.push(event.data);
    return { host: port1, container: port2, received };
}
const settle = () => new Promise((resolve) => setTimeout(resolve, 10));

describe("message port bridge", () => {
    // The container asks for permissions from the moment product code runs, and
    // the host hands its port over a little later. Those requests must wait and
    // must not be answered from anywhere else.
    test("holds frames until the host's port arrives, then sends them in order", async () => {
        const { host, container, received } = channel();
        const bridge = createMessagePortBridge();
        const provider = bridge.createProvider();
        provider.postMessage(frame(1));
        provider.postMessage(frame(2));
        await settle();
        expect(received).toEqual([]);

        expect(bridge.attach(container)).toBe(true);
        await provider.opened;
        provider.postMessage(frame(3));
        await settle();
        expect(received).toEqual([frame(1), frame(2), frame(3)]);
        host.close();
    });

    test("delivers frames from the host to subscribers", async () => {
        const { host, container } = channel();
        const bridge = createMessagePortBridge();
        const provider = bridge.createProvider();
        const seen: Uint8Array[] = [];
        provider.subscribe((message) => seen.push(message));
        bridge.attach(container);
        host.postMessage(frame(9, 9));
        await settle();
        expect(seen).toEqual([frame(9, 9)]);
        host.close();
    });

    // Anything but a byte array is not a wire frame. A page that could post to the
    // port must not be able to feed the decoder other shapes.
    test("ignores anything that is not a byte array", async () => {
        const { host, container } = channel();
        const bridge = createMessagePortBridge();
        const provider = bridge.createProvider();
        const seen: unknown[] = [];
        provider.subscribe((message) => seen.push(message));
        bridge.attach(container);
        host.postMessage("text");
        host.postMessage({ length: 1 });
        host.postMessage(new ArrayBuffer(4));
        host.postMessage(null);
        await settle();
        expect(seen).toEqual([]);
        host.close();
    });

    // Fail closed: a second port could otherwise replace the host's channel.
    test("takes only the first port", async () => {
        const first = channel();
        const second = channel();
        const bridge = createMessagePortBridge();
        const provider = bridge.createProvider();
        expect(bridge.attach(first.container)).toBe(true);
        expect(bridge.attach(second.container)).toBe(false);
        provider.postMessage(frame(7));
        await settle();
        expect(first.received).toEqual([frame(7)]);
        expect(second.received).toEqual([]);
        first.host.close();
        second.host.close();
    });

    test("refuses frames past the wait bound instead of queueing without limit", () => {
        const bridge = createMessagePortBridge();
        const provider = bridge.createProvider();
        for (let i = 0; i < 256; i += 1) provider.postMessage(frame(i % 256));
        expect(() => provider.postMessage(frame(0))).toThrow("not connected");
    });

    test("a reconnect provider reuses the attached port", async () => {
        const { host, container, received } = channel();
        const bridge = createMessagePortBridge();
        bridge.attach(container);
        const first = bridge.createProvider();
        first.dispose();
        const second = bridge.createProvider();
        second.postMessage(frame(5));
        await settle();
        expect(received).toEqual([frame(5)]);
        host.close();
    });

    test("a disposed provider stops sending and stops receiving", async () => {
        const { host, container, received } = channel();
        const bridge = createMessagePortBridge();
        bridge.attach(container);
        const provider = bridge.createProvider();
        const seen: Uint8Array[] = [];
        provider.subscribe((message) => seen.push(message));
        provider.dispose();
        expect(() => provider.postMessage(frame(1))).toThrow();
        host.postMessage(frame(2));
        await settle();
        expect(received).toEqual([]);
        expect(seen).toEqual([]);
        host.close();
    });

    // Product code can rewrite MessagePort.prototype. The bridge must have taken
    // its own copies at creation, before that code ran.
    test("keeps working after product code replaces port methods", async () => {
        const { host, container, received } = channel();
        const bridge = createMessagePortBridge();
        const provider = bridge.createProvider();
        const original = MessagePort.prototype.postMessage;
        MessagePort.prototype.postMessage = function () {
            throw new Error("hijacked");
        };
        try {
            bridge.attach(container);
            provider.postMessage(frame(4));
            await settle();
        } finally {
            MessagePort.prototype.postMessage = original;
        }
        expect(received).toEqual([frame(4)]);
        host.close();
    });
});
