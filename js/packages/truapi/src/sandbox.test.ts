import { afterEach, describe, expect, it, mock } from "bun:test";

import {
  MESSAGE_TYPE_REQUEST,
  MESSAGE_TYPE_RESPONSE,
  encodeWireMessage,
  PROTOCOL_ERROR_METHOD_ID,
  PROTOCOL_ERROR_TRAIT_ID,
} from "./transport.js";

let importCounter = 0;

async function importSandbox(): Promise<typeof import("./sandbox.js")> {
    importCounter += 1;
    return import(`./sandbox.ts?test=${importCounter}`);
}

type MessageListener = (event: MessageEvent) => void;

function installFakeIframeWindow(options: { referrer?: string; ancestorOrigins?: string[] }) {
    const listeners = new Set<MessageListener>();
    const intervals = new Map<number, () => void>();
    let intervalId = 0;
    const priorWindow = globalThis.window;
    const priorDocument = globalThis.document;
    const parentPostMessage = mock((_message: unknown, _origin: string) => {});
    const parent = {
        postMessage: parentPostMessage,
    } as unknown as Window;
    const win = {
        parent,
        top: {} as Window,
        location: {
            ancestorOrigins: options.ancestorOrigins,
        },
        addEventListener(name: string, callback: EventListener) {
            if (name === "message") listeners.add(callback as MessageListener);
        },
        removeEventListener(name: string, callback: EventListener) {
            if (name === "message") listeners.delete(callback as MessageListener);
        },
        setInterval(handler: TimerHandler) {
            intervalId += 1;
            intervals.set(intervalId, handler as () => void);
            return intervalId;
        },
        clearInterval(id: number) {
            intervals.delete(id);
        },
    } as unknown as Window & typeof globalThis;

    globalThis.window = win;
    globalThis.document = {
        referrer: options.referrer ?? "",
    } as Document;

    return {
        listeners,
        parent,
        parentPostMessage,
        win,
        dispatch(event: { source: unknown; origin: string; data: unknown; ports?: MessagePort[] }) {
            for (const listener of [...listeners]) {
                listener({ ports: [], ...event } as MessageEvent);
            }
        },
        runIntervals() {
            for (const callback of [...intervals.values()]) callback();
        },
        restore() {
            if (priorWindow === undefined) {
                delete (globalThis as { window?: unknown }).window;
            } else {
                globalThis.window = priorWindow;
            }
            if (priorDocument === undefined) {
                delete (globalThis as { document?: unknown }).document;
            } else {
                globalThis.document = priorDocument;
            }
        },
    };
}

/** The transport adopts its port off a promise, not inline. */
const settle = () => new Promise((resolve) => setTimeout(resolve, 0));

async function until(predicate: () => boolean): Promise<void> {
    for (let i = 0; i < 200 && !predicate(); i += 1) await settle();
    if (!predicate()) throw new Error("until() gave up after 200 turns");
}

let currentWindow: ReturnType<typeof installFakeIframeWindow> | null = null;
const openPorts: MessagePort[] = [];

function trackChannel(): MessageChannel {
    const channel = new MessageChannel();
    openPorts.push(channel.port1, channel.port2);
    return channel;
}

afterEach(() => {
    for (const port of openPorts.splice(0)) {
        port.close();
    }
    currentWindow?.restore();
    currentWindow = null;
});

describe("sandbox iframe MessagePort handshake", () => {
    it("posts ready to the resolved host origin and rejects non-parent or mismatched init messages", async () => {
        currentWindow = installFakeIframeWindow({
            referrer: "https://host.example/product",
        });
        const sandbox = await importSandbox();

        expect(sandbox.getClientSync()).not.toBeNull();
        expect(currentWindow.parentPostMessage.mock.calls).toEqual([
            [{ type: "truapi-ready" }, "https://host.example"],
        ]);

        const wrongSource = trackChannel();
        currentWindow.dispatch({
            source: {},
            origin: "https://host.example",
            data: { type: "truapi-init" },
            ports: [wrongSource.port1],
        });
        const wrongOrigin = trackChannel();
        currentWindow.dispatch({
            source: currentWindow.parent,
            origin: "https://attacker.example",
            data: { type: "truapi-init" },
            ports: [wrongOrigin.port1],
        });
        const opaqueOrigin = trackChannel();
        currentWindow.dispatch({
            source: currentWindow.parent,
            origin: "null",
            data: { type: "truapi-init" },
            ports: [opaqueOrigin.port1],
        });
        await Promise.resolve();
        expect(currentWindow.win.__HOST_API_PORT__).toBeUndefined();
        expect(currentWindow.listeners.size).toBe(1);

        const accepted = trackChannel();
        currentWindow.dispatch({
            source: currentWindow.parent,
            origin: "https://host.example",
            data: { type: "truapi-init" },
            ports: [accepted.port1],
        });
        await Promise.resolve();
        expect(currentWindow.win.__HOST_API_PORT__).toBe(accepted.port1);
        expect(currentWindow.listeners.size).toBe(0);
    });

    it("retries ready until the host transfers a port, then stops", async () => {
        currentWindow = installFakeIframeWindow({
            referrer: "https://host.example/product",
        });
        const sandbox = await importSandbox();

        expect(sandbox.getClientSync()).not.toBeNull();
        currentWindow.runIntervals();
        expect(currentWindow.parentPostMessage.mock.calls).toHaveLength(2);

        const accepted = trackChannel();
        currentWindow.dispatch({
            source: currentWindow.parent,
            origin: "https://host.example",
            data: { type: "truapi-init" },
            ports: [accepted.port1],
        });
        currentWindow.runIntervals();

        expect(currentWindow.parentPostMessage.mock.calls).toHaveLength(2);
        expect(currentWindow.listeners.size).toBe(0);
    });

    it('treats a masked "null" ancestor origin as hidden and pings with the wildcard', async () => {
        // Firefox implements location.ancestorOrigins but serializes cross-origin
        // ancestors as "null", which is not a valid postMessage targetOrigin.
        currentWindow = installFakeIframeWindow({ ancestorOrigins: ["null"] });
        const sandbox = await importSandbox();

        expect(sandbox.getClientSync()).not.toBeNull();
        expect(currentWindow.parentPostMessage.mock.calls).toEqual([
            [{ type: "truapi-ready" }, "*"],
        ]);

        const accepted = trackChannel();
        currentWindow.dispatch({
            source: currentWindow.parent,
            origin: "https://host.example",
            data: { type: "truapi-init" },
            ports: [accepted.port1],
        });
        await Promise.resolve();
        expect(currentWindow.win.__HOST_API_PORT__).toBe(accepted.port1);
        expect(currentWindow.listeners.size).toBe(0);
    });

    it("uses a data-free wildcard ready ping only when the host origin is hidden", async () => {
        currentWindow = installFakeIframeWindow({});
        const sandbox = await importSandbox();

        expect(sandbox.getClientSync()).not.toBeNull();
        expect(currentWindow.parentPostMessage.mock.calls).toEqual([
            [{ type: "truapi-ready" }, "*"],
        ]);

        const wrongSource = trackChannel();
        currentWindow.dispatch({
            source: {},
            origin: "https://host.example",
            data: { type: "truapi-init" },
            ports: [wrongSource.port1],
        });
        await Promise.resolve();
        expect(currentWindow.win.__HOST_API_PORT__).toBeUndefined();

        const accepted = trackChannel();
        currentWindow.dispatch({
            source: currentWindow.parent,
            origin: "https://host.example",
            data: { type: "truapi-init" },
            ports: [accepted.port1],
        });
        await Promise.resolve();
        expect(currentWindow.win.__HOST_API_PORT__).toBe(accepted.port1);
        expect(currentWindow.listeners.size).toBe(0);
    });

    it("reports connecting until the MessagePort handover completes", async () => {
        currentWindow = installFakeIframeWindow({
            referrer: "https://host.example/product",
        });
        const sandbox = await importSandbox();
        const statuses: string[] = [];
        sandbox.subscribeConnectionStatus((status) => statuses.push(status));
        expect(statuses).toEqual(["connecting"]);

        const accepted = trackChannel();
        currentWindow.dispatch({
            source: currentWindow.parent,
            origin: "https://host.example",
            data: { type: "truapi-init" },
            ports: [accepted.port1],
        });
        expect(statuses).toEqual(["connecting", "connected"]);
    });

    it("reports connecting until the first legacy frame pins the transport", async () => {
        currentWindow = installFakeIframeWindow({
            referrer: "https://legacy-host.example/product",
        });
        const sandbox = await importSandbox();
        const statuses: string[] = [];
        sandbox.subscribeConnectionStatus((status) => statuses.push(status));
        expect(statuses).toEqual(["connecting"]);

        const probe = encodeWireMessage({
            requestId: "legacy-probe",
            payload: { traitId: 254, methodId: 253, messageType: MESSAGE_TYPE_REQUEST, value: new Uint8Array() },
        });
        expect(probe.isOk()).toBe(true);
        if (probe.isErr()) throw probe.error;
        currentWindow.dispatch({
            source: currentWindow.parent,
            origin: "https://legacy-host.example",
            data: probe.value,
        });
        expect(statuses).toEqual(["connecting", "connected"]);
    });

    it("reports connected immediately when the host port is already injected", async () => {
        currentWindow = installFakeIframeWindow({
            referrer: "https://host.example/product",
        });
        const channel = trackChannel();
        currentWindow.win.__HOST_API_PORT__ = channel.port1;
        const sandbox = await importSandbox();
        const statuses: string[] = [];
        sandbox.subscribeConnectionStatus((status) => statuses.push(status));
        expect(statuses).toEqual(["connected"]);
    });

    it("falls back to legacy window frames and pins their parent origin", async () => {
        currentWindow = installFakeIframeWindow({
            referrer: "https://legacy-host.example/product",
        });
        const sandbox = await importSandbox();
        const client = sandbox.getClientSync();
        expect(client).not.toBeNull();

        const probe = encodeWireMessage({
            requestId: "legacy-probe",
            payload: { traitId: 254, methodId: 253, messageType: MESSAGE_TYPE_REQUEST, value: new Uint8Array() },
        });
        expect(probe.isOk()).toBe(true);
        if (probe.isErr()) throw probe.error;
        currentWindow.dispatch({
            source: currentWindow.parent,
            origin: "https://legacy-host.example",
            data: probe.value,
        });

        void client?.system.handshake();
        const unsupported = encodeWireMessage({
            requestId: "legacy-probe",
            payload: {
                traitId: PROTOCOL_ERROR_TRAIT_ID,
                methodId: PROTOCOL_ERROR_METHOD_ID,
                messageType: MESSAGE_TYPE_RESPONSE,
                // [0] version index, [0] variant index, then the pair that was
                // not understood, echoed in arrival order.
                value: new Uint8Array([0, 0, 254, 253]),
            },
        });
        expect(unsupported.isOk()).toBe(true);
        if (unsupported.isErr()) throw unsupported.error;
        expect(currentWindow.parentPostMessage.mock.calls).toHaveLength(3);
        expect(currentWindow.parentPostMessage.mock.calls[1][0]).toEqual(unsupported.value);
        expect(currentWindow.parentPostMessage.mock.calls[2][0]).toBeInstanceOf(Uint8Array);
        expect(currentWindow.parentPostMessage.mock.calls[2][1]).toBe(
            "https://legacy-host.example",
        );
    });
});

describe("connectWebSocketHost", () => {
    const servers: ReturnType<typeof Bun.serve>[] = [];

    afterEach(async () => {
        for (const server of servers.splice(0)) server.stop(true);
        // A finished module's close still runs; keep it out of the next test.
        await new Promise((resolve) => setTimeout(resolve, 0));
    });

    /** Loopback frame socket, standing in for `truapi-host --frame-listen`. */
    function frameServer() {
        const server = Bun.serve({
            hostname: "127.0.0.1",
            port: 0,
            fetch(request, server) {
                if (server.upgrade(request)) return;
                return new Response("websocket upgrade required", { status: 426 });
            },
            websocket: {
                message() {},
            },
        });
        servers.push(server);
        return `ws://127.0.0.1:${server.port}`;
    }

    it("makes a plain page count as hosted and caches one client", async () => {
        const sandbox = await importSandbox();
        expect(sandbox.isCorrectEnvironment()).toBe(false);

        const client = sandbox.connectWebSocketHost(frameServer());

        expect(sandbox.isCorrectEnvironment()).toBe(true);
        expect(client).not.toBeNull();
        expect(sandbox.getClientSync()).toBe(client);
    });

    it("uses an explicitly selected WebSocket without reading an injected client", async () => {
        currentWindow = installFakeWebviewWindow();
        Object.defineProperty(currentWindow.win, "__HOST_API_CLIENT__", {
            get() {
                throw new Error("explicit endpoints must not adopt the injected client");
            },
        });
        const sandbox = await importSandbox();
        const client = sandbox.connectWebSocketHost(frameServer());
        expect(client).not.toBeNull();
        expect(sandbox.getClientSync()).toBe(client);
    });

    it("reports connected once the socket is open", async () => {
        const sandbox = await importSandbox();
        const statuses: string[] = [];
        const connected = new Promise<void>((resolve) => {
            sandbox.subscribeConnectionStatus((status) => {
                statuses.push(status);
                if (status === "connected") resolve();
            });
        });

        sandbox.connectWebSocketHost(frameServer());
        await connected;

        expect(statuses).toContain("connected");
    });

    it("refuses to redirect a client that already exists", async () => {
        const sandbox = await importSandbox();
        sandbox.connectWebSocketHost(frameServer());

        expect(() => sandbox.connectWebSocketHost(frameServer())).toThrow(
            /before the TrUAPI client is created/,
        );
    });

    it("keeps a natively injected port when the socket closes", async () => {
        currentWindow = installFakeWebviewWindow();
        const channel = trackChannel();
        const sandbox = await importSandbox();
        currentWindow.win.__HOST_API_PORT__ = channel.port1;
        sandbox.connectWebSocketHost(frameServer());
        let connected = false;
        let closed = false;
        sandbox.subscribeConnectionStatus((status) => {
            if (status === "connected") connected = true;
            if (status === "disconnected" && connected) closed = true;
        });

        await until(() => connected);
        for (const server of servers.splice(0)) server.stop(true);
        await until(() => closed);

        expect(currentWindow.win.__HOST_API_PORT__).toBe(channel.port1);
    });

    it("does not end on connected when the host hangs up on open", async () => {
        const server = Bun.serve({
            hostname: "127.0.0.1",
            port: 0,
            fetch(request, server) {
                if (server.upgrade(request)) return;
                return new Response("websocket upgrade required", { status: 426 });
            },
            websocket: {
                open(socket) {
                    socket.close();
                },
                message() {},
            },
        });
        servers.push(server);
        const sandbox = await importSandbox();
        sandbox.connectWebSocketHost(`ws://127.0.0.1:${server.port}`);
        const statuses: string[] = [];
        sandbox.subscribeConnectionStatus((status) => statuses.push(status));

        await until(() => statuses.includes("disconnected"));
        await settle();

        // Whichever order the open and close land in, the close is the truth.
        expect(statuses[statuses.length - 1]).toBe("disconnected");
    });

    it("re-dials the same endpoint after the host hangs up", async () => {
        let connections = 0;
        const server = Bun.serve({
            hostname: "127.0.0.1",
            port: 0,
            fetch(request, server) {
                if (server.upgrade(request)) return;
                return new Response("websocket upgrade required", { status: 426 });
            },
            websocket: {
                open(socket) {
                    connections += 1;
                    // Drop the first connection, accept the rebuild's.
                    if (connections === 1) socket.close();
                },
                message() {},
            },
        });
        servers.push(server);
        const sandbox = await importSandbox();

        const first = sandbox.connectWebSocketHost(`ws://127.0.0.1:${server.port}`);
        await until(() => connections === 1);
        await settle();
        const rebuilt = sandbox.getClientSync();
        await until(() => connections === 2);

        expect(rebuilt).not.toBeNull();
        expect(rebuilt).not.toBe(first);
        expect(connections).toBe(2);
    });

    it("accepts a different endpoint once the pipe has closed", async () => {
        const sandbox = await importSandbox();
        let connected = false;
        let closed = false;
        sandbox.subscribeConnectionStatus((status) => {
            if (status === "connected") connected = true;
            if (status === "disconnected" && connected) closed = true;
        });

        sandbox.connectWebSocketHost(frameServer());
        await until(() => connected);
        for (const server of servers.splice(0)) server.stop(true);
        await until(() => closed);

        // The guard is on a live client, and the close cleared it.
        expect(() => sandbox.connectWebSocketHost(frameServer())).not.toThrow();
    });
});

function closePipe(port: MessagePort): void {
    const hook = port.onmessageerror;
    if (!hook) throw new Error("no messageerror hook installed on the port");
    hook.call(port, new MessageEvent("messageerror"));
}

function installFakeWebviewWindow() {
    const harness = installFakeIframeWindow({});
    // Top-level, so isIframe() reads false.
    (harness.win as unknown as { top: Window }).top = harness.win;
    harness.win.__HOST_WEBVIEW_MARK__ = true;
    return harness;
}

describe("sandbox after the pipe closes", () => {
    async function connectedSandbox(port: MessagePort) {
        const harness = installFakeIframeWindow({
            referrer: "https://host.example/product",
        });
        currentWindow = harness;
        const sandbox = await importSandbox();
        // After the import: a stale module still closing would clear this global.
        harness.win.__HOST_API_PORT__ = port;
        const client = sandbox.getClientSync();
        expect(client).not.toBeNull();
        await settle();
        return { client, harness, sandbox };
    }

    it("gives a subscriber arriving after a close a negotiation that can complete", async () => {
        const channel = trackChannel();
        const { harness, sandbox } = await connectedSandbox(channel.port1);
        closePipe(channel.port1);

        const statuses: string[] = [];
        sandbox.subscribeConnectionStatus((status) => statuses.push(status));
        expect(statuses).toEqual(["connecting"]);

        const rebuilt = trackChannel();
        harness.dispatch({
            source: harness.parent,
            origin: "https://host.example",
            data: { type: "truapi-init" },
            ports: [rebuilt.port1],
        });

        // Pre-fix this "connecting" was permanent: nothing listened for the answer.
        expect(statuses).toEqual(["connecting", "connected"]);
    });

    it("builds a new client after a close instead of handing back the closed one", async () => {
        const channel = trackChannel();
        const { client, sandbox } = await connectedSandbox(channel.port1);
        closePipe(channel.port1);

        const rebuilt = sandbox.getClientSync();

        expect(rebuilt).not.toBeNull();
        expect(rebuilt).not.toBe(client);
    });

    it("drops the injected port so a rebuild cannot re-adopt the closed one", async () => {
        const channel = trackChannel();
        const { harness } = await connectedSandbox(channel.port1);
        expect(harness.win.__HOST_API_PORT__).toBe(channel.port1);

        closePipe(channel.port1);

        expect(harness.win.__HOST_API_PORT__).toBeUndefined();
    });

    it("renegotiates with the parent on the next build", async () => {
        const channel = trackChannel();
        const { harness, sandbox } = await connectedSandbox(channel.port1);
        // An injected port skips the handshake, so nothing was posted yet.
        expect(harness.parentPostMessage.mock.calls).toEqual([]);

        closePipe(channel.port1);
        sandbox.getClientSync();

        expect(harness.parentPostMessage.mock.calls).toEqual([
            [{ type: "truapi-ready" }, "https://host.example"],
        ]);
    });

    it("does not hand the closed client to a listener notified of the close", async () => {
        const channel = trackChannel();
        const { client, sandbox } = await connectedSandbox(channel.port1);
        const seen: unknown[] = [];
        sandbox.subscribeConnectionStatus((status) => {
            if (status === "disconnected") seen.push(sandbox.getClientSync());
        });

        closePipe(channel.port1);

        expect(seen).toHaveLength(1);
        expect(seen[0]).not.toBeNull();
        expect(seen[0]).not.toBe(client);
    });

    it("stops delivering a status that a listener has replaced", async () => {
        const channel = trackChannel();
        const { sandbox } = await connectedSandbox(channel.port1);
        const seen: string[] = [];
        sandbox.subscribeConnectionStatus((status) => {
            // A product remounting its status view on a disconnect.
            if (status === "disconnected") {
                sandbox.subscribeConnectionStatus(() => {});
            }
        });
        sandbox.subscribeConnectionStatus((status) => seen.push(status));

        closePipe(channel.port1);

        expect(seen).toEqual(["connected", "connecting"]);
    });

    it("still reports the close when the port cannot be deleted", async () => {
        const harness = installFakeIframeWindow({
            referrer: "https://host.example/product",
        });
        currentWindow = harness;
        const sandbox = await importSandbox();
        const channel = trackChannel();
        // A host can prevent deletion while allowing its port to be replaced.
        Object.defineProperty(harness.win, "__HOST_API_PORT__", {
            value: channel.port1,
            writable: true,
            configurable: false,
        });
        const statuses: string[] = [];
        sandbox.subscribeConnectionStatus((status) => statuses.push(status));
        await settle();
        expect(statuses).toEqual(["connected"]);

        closePipe(channel.port1);

        expect(statuses).toEqual(["connected", "disconnected"]);
    });

    it("does not re-adopt a port it could not delete", async () => {
        const harness = installFakeIframeWindow({
            referrer: "https://host.example/product",
        });
        currentWindow = harness;
        const sandbox = await importSandbox();
        const channel = trackChannel();
        // A host can prevent deletion while allowing its port to be replaced.
        Object.defineProperty(harness.win, "__HOST_API_PORT__", {
            value: channel.port1,
            writable: true,
            configurable: false,
        });
        const statuses: string[] = [];
        sandbox.subscribeConnectionStatus((status) => statuses.push(status));
        await settle();
        closePipe(channel.port1);
        expect(statuses).toEqual(["connected", "disconnected"]);

        sandbox.getClientSync();

        // The port is still on the window, and must not count as a live one.
        expect(harness.win.__HOST_API_PORT__).toBe(channel.port1);
        expect(statuses).toEqual(["connected", "disconnected"]);

        const fresh = trackChannel();
        harness.dispatch({
            source: harness.parent,
            origin: "https://host.example",
            data: { type: "truapi-init" },
            ports: [fresh.port1],
        });

        expect(statuses).toEqual(["connected", "disconnected", "connected"]);
    });

    it("keeps a marked webview page hosted after the port is dropped", async () => {
        const channel = trackChannel();
        const harness = installFakeWebviewWindow();
        currentWindow = harness;
        const sandbox = await importSandbox();
        harness.win.__HOST_API_PORT__ = channel.port1;
        expect(sandbox.isCorrectEnvironment()).toBe(true);
        expect(sandbox.getClientSync()).not.toBeNull();
        await settle();

        closePipe(channel.port1);

        expect(harness.win.__HOST_API_PORT__).toBeUndefined();
        expect(sandbox.isCorrectEnvironment()).toBe(true);
    });

    it("treats a MessagePort from a locked getter as an ordinary terminal connection", async () => {
        const channel = trackChannel();
        currentWindow = installFakeWebviewWindow();
        Object.defineProperty(currentWindow.win, "__HOST_API_PORT__", {
            get: () => channel.port1,
        });
        const sandbox = await importSandbox();
        const client = sandbox.getClientSync()!;
        await settle();
        closePipe(channel.port1);
        await expect(Promise.resolve(client.system.handshake())).rejects.toThrow(
            "message port closed unexpectedly",
        );
    });
});
