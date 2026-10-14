import { afterEach, beforeEach, describe, expect, it } from "bun:test";
import { createTransport } from "./client.js";
import { createClient } from "./generated/client.js";

import {
    createIframeProvider,
    createMessagePortProvider,
    createWebSocketProvider,
    createWebSocketProviderFactory,
} from "./transport.js";

/**
 * Install a minimal stub for the global `window` used by `createIframeProvider`.
 * Returns a dispatch helper and a snapshot of the registered message listeners
 * so individual tests can inspect cleanup.
 */
function installFakeWindow() {
    const listeners = new Set<(event: unknown) => void>();
    const prior = globalThis.window;
    globalThis.window = {
        addEventListener(name: string, cb: (event: unknown) => void) {
            if (name === "message") listeners.add(cb);
        },
        removeEventListener(name: string, cb: (event: unknown) => void) {
            if (name === "message") listeners.delete(cb);
        },
    } as unknown as Window & typeof globalThis;
    return {
        listeners,
        dispatch(event: unknown) {
            for (const cb of [...listeners]) cb(event);
        },
        restore() {
            if (prior === undefined) {
                delete (globalThis as { window?: unknown }).window;
            } else {
                globalThis.window = prior;
            }
        },
    };
}

describe("createIframeProvider", () => {
    let win: ReturnType<typeof installFakeWindow>;

    beforeEach(() => {
        win = installFakeWindow();
    });

    afterEach(() => {
        win.restore();
    });

    it("filters by source/origin/type and pins the outbound origin", () => {
        const sent: { msg: Uint8Array; origin: string }[] = [];
        const target = {
            postMessage(msg: Uint8Array, origin: string) {
                sent.push({ msg, origin });
            },
        } as unknown as Window;
        const provider = createIframeProvider({
            target,
            hostOrigin: "https://host.example",
        });

        const received: Uint8Array[] = [];
        provider.subscribe((m) => received.push(m));

        win.dispatch({
            source: target,
            origin: "https://host.example",
            data: new Uint8Array([1, 2, 3]),
        });
        expect([...received[0]]).toEqual([1, 2, 3]);

        // Bad source, bad origin, and non-bytes payloads are all dropped.
        win.dispatch({ source: {}, origin: "https://host.example", data: new Uint8Array([9]) });
        win.dispatch({
            source: target,
            origin: "https://attacker.example",
            data: new Uint8Array([9]),
        });
        win.dispatch({ source: target, origin: "https://host.example", data: "not bytes" });
        expect(received).toHaveLength(1);

        provider.postMessage(new Uint8Array([7]));
        expect(sent).toHaveLength(1);
        expect(sent[0].origin).toBe("https://host.example");
        expect([...sent[0].msg]).toEqual([7]);

        provider.dispose();
    });

    it("disposes idempotently and reports the close error to late subscribers", () => {
        const provider = createIframeProvider({
            target: { postMessage() {} } as unknown as Window,
            hostOrigin: "https://host.example",
        });

        let closeError: unknown = null;
        provider.subscribeClose?.((e) => (closeError = e));

        expect(win.listeners.size).toBeGreaterThan(0);
        provider.dispose();
        expect(closeError).toBeInstanceOf(Error);
        expect(win.listeners.size).toBe(0);

        // Idempotent: a second dispose is a no-op, and sends after close throw.
        provider.dispose();
        expect(() => provider.postMessage(new Uint8Array([1]))).toThrow();

        // Post-close subscribeClose fires immediately with the stored error.
        let late: unknown = null;
        provider.subscribeClose?.((e) => (late = e));
        expect(late).toBeInstanceOf(Error);
    });
});

describe("createMessagePortProvider", () => {
    it("notifies later close listeners when a subscription observer throws", () => {
        const { port1, port2 } = new MessageChannel();
        const provider = createMessagePortProvider(port1);
        const client = createClient(createTransport(provider));
        client.theme.subscribe().subscribe({
            error() {
                throw new Error("broken observer");
            },
        });
        const errors: Error[] = [];
        provider.subscribeClose?.((error) => errors.push(error));
        try {
            expect(() => provider.dispose()).not.toThrow();
            expect(errors.map((error) => error.message)).toEqual([
                "message port provider disposed",
            ]);
        } finally {
            port1.close();
            port2.close();
        }
    });

    it("queues sends, round-trips inbound frames, and errors after dispose", async () => {
        const { port1, port2 } = new MessageChannel();
        const provider = createMessagePortProvider(port1);

        provider.postMessage(new Uint8Array([42]));

        const drained = await new Promise<Uint8Array>((resolve) => {
            port2.onmessage = (e) => resolve(e.data);
            port2.start();
        });
        expect([...drained]).toEqual([42]);

        const inboundOnce = new Promise<Uint8Array>((resolve) => {
            const unsubscribe = provider.subscribe((m) => {
                unsubscribe();
                resolve(m);
            });
        });
        port2.postMessage(new Uint8Array([55]));
        expect([...(await inboundOnce)]).toEqual([55]);

        provider.dispose();
        let lateClose: unknown = null;
        provider.subscribeClose?.((e) => (lateClose = e));
        expect(lateClose).toBeInstanceOf(Error);
        expect(() => provider.postMessage(new Uint8Array([1]))).toThrow();

        // Free the receiver port so the runtime exits cleanly.
        port2.close();
    });
});

describe("createWebSocketProvider", () => {
    const servers: ReturnType<typeof Bun.serve>[] = [];

    afterEach(() => {
        for (const server of servers.splice(0)) server.stop(true);
    });

    /** Echo server on a loopback port, standing in for a host's frame socket. */
    function echoServer(onClose?: () => void) {
        const server = Bun.serve({
            hostname: "127.0.0.1",
            port: 0,
            fetch(request, server) {
                if (server.upgrade(request)) return;
                return new Response("websocket upgrade required", { status: 426 });
            },
            websocket: {
                message(socket, message) {
                    socket.send(message);
                },
                close() {
                    onClose?.();
                },
            },
        });
        servers.push(server);
        return `ws://127.0.0.1:${server.port}`;
    }

    it("opens replacement sockets with the captured browser APIs", async () => {
        let socketClosed!: () => void;
        const closed = new Promise<void>((resolve) => {
            socketClosed = resolve;
        });
        const url = echoServer(socketClosed);
        const createProvider = createWebSocketProviderFactory();
        const originals = [
            [globalThis, "WebSocket"],
            [WebSocket.prototype, "send"],
            [WebSocket.prototype, "close"],
            [WebSocket.prototype, "binaryType"],
            [EventTarget.prototype, "addEventListener"],
            [MessageEvent.prototype, "data"],
        ].map(([owner, key]) => ({
            owner: owner as object,
            key: key as string,
            descriptor: Object.getOwnPropertyDescriptor(owner, key as string)!,
        }));
        const replaced = () => {
            throw new Error("product replaced the browser API");
        };
        let provider: ReturnType<typeof createProvider> | undefined;
        try {
            for (const { owner, key, descriptor } of originals) {
                Object.defineProperty(
                    owner,
                    key,
                    "value" in descriptor
                        ? { ...descriptor, value: replaced }
                        : { ...descriptor, get: replaced, set: replaced },
                );
            }
            provider = createProvider(url);
            const response = new Promise<Uint8Array>((resolve) => provider!.subscribe(resolve));
            provider.postMessage(new Uint8Array([7, 8, 9]));
            await provider.opened;
            expect(await response).toEqual(new Uint8Array([7, 8, 9]));
            provider.dispose();
            await closed;
        } finally {
            for (const { owner, key, descriptor } of originals)
                Object.defineProperty(owner, key, descriptor);
            provider?.dispose();
        }
    });

    it("queues frames posted before the socket opens", async () => {
        const provider = createWebSocketProvider(echoServer());
        const received: Uint8Array[] = [];
        const first = new Promise<void>((resolve) => {
            provider.subscribe((message) => {
                received.push(message);
                if (received.length === 2) resolve();
            });
        });

        // Both posted while still connecting, so both have to be buffered.
        provider.postMessage(Uint8Array.from([1, 2, 3]));
        provider.postMessage(Uint8Array.from([4, 5]));
        await provider.opened;
        await first;

        expect(received.map((frame) => Array.from(frame))).toEqual([
            [1, 2, 3],
            [4, 5],
        ]);
        provider.dispose();
    });

    it("reports a server close to close subscribers", async () => {
        const url = echoServer();
        const provider = createWebSocketProvider(url);
        await provider.opened;

        const closed = new Promise<Error>((resolve) => {
            provider.subscribeClose?.(resolve);
        });
        for (const server of servers.splice(0)) server.stop(true);

        expect((await closed).message).toContain(url);
    });

    it("rejects `opened` when the endpoint is not there", async () => {
        // Port 1 on loopback is never a frame server, so the connect fails.
        const provider = createWebSocketProvider("ws://127.0.0.1:1");
        await expect(provider.opened).rejects.toThrow();
        expect(() => provider.postMessage(Uint8Array.from([1]))).toThrow();
    });
});
