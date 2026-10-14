import { afterEach, describe, expect, it } from "bun:test";
import { createClient } from "./generated/index.js";
import { createTransport } from "./client.js";
import type { ConnectionStatus } from "./sandbox.js";

let importCounter = 0;
async function importSandbox(): Promise<typeof import("./sandbox.js")> {
    return import(`./sandbox.ts?injected-client=${++importCounter}`);
}

function installHost(initialStatus: ConnectionStatus = "connecting") {
    const priorWindow = globalThis.window;
    const win = new EventTarget() as unknown as Window & typeof globalThis;
    Object.assign(win, { top: win, parent: win });
    const transport = createTransport({
        postMessage() {
            throw new Error("adopting a client must not send another request");
        },
        subscribe() {
            return () => {};
        },
        dispose() {},
    });
    const client = createClient(transport);
    const listeners = new Set<(status: ConnectionStatus) => void>();
    let status = initialStatus;
    let clientReads = 0;
    let subscriptions = 0;
    Object.defineProperty(win, "__HOST_API_CLIENT__", {
        value: Object.freeze({
            get client() {
                clientReads++;
                return client;
            },
            subscribeConnectionStatus(callback: (status: ConnectionStatus) => void) {
                subscriptions++;
                listeners.add(callback);
                callback(status);
                return () => {
                    listeners.delete(callback);
                };
            },
        }),
    });
    for (const name of ["__HOST_API_PORT__", "__truapi_localhost"]) {
        Object.defineProperty(win, name, {
            get() {
                throw new Error(`injected clients must not read ${name}`);
            },
        });
    }
    globalThis.window = win;
    return {
        win,
        client,
        reads: () => ({ clientReads, subscriptions }),
        status(next: ConnectionStatus) {
            status = next;
            for (const listener of [...listeners]) listener(next);
        },
        restore() {
            transport.dispose();
            if (priorWindow === undefined) delete (globalThis as { window?: unknown }).window;
            else globalThis.window = priorWindow;
        },
    };
}

let host: ReturnType<typeof installHost> | null = null;
afterEach(() => {
    host?.restore();
    host = null;
});

describe("host-injected client adoption", () => {
    it("recognizes the injected host without reading its port or private endpoint", async () => {
        host = installHost();
        const sandbox = await importSandbox();
        expect(sandbox.isCorrectEnvironment()).toBe(true);
        expect(sandbox.getClientSync()).toBe(host.client);
        expect(host.reads()).toEqual({ clientReads: 1, subscriptions: 1 });
    });

    it("adopts the injected client before an iframe can negotiate another connection", async () => {
        host = installHost();
        Object.assign(host.win, {
            top: {},
            parent: {
                postMessage() {
                    throw new Error("unexpected iframe negotiation");
                },
            },
        });
        const sandbox = await importSandbox();
        expect(sandbox.getClientSync()).toBe(host.client);
    });

    it("reads the client getter once so a renderer-only product starts its connection", async () => {
        host = installHost();
        const sandbox = await importSandbox();
        const client = sandbox.getClientSync()!;
        client.renderer.onRender(() => {});
        expect({ client: sandbox.getClientSync(), ...host.reads() }).toEqual({
            client: host.client,
            clientReads: 1,
            subscriptions: 1,
        });
    });

    it("uses host readiness and retains client and namespace identity across repeated resets", async () => {
        host = installHost();
        const sandbox = await importSandbox();
        const statuses: ConnectionStatus[] = [];
        sandbox.subscribeConnectionStatus((status) => statuses.push(status));
        const client = sandbox.getClientSync()!;
        const namespace = client.system;
        const expected: ConnectionStatus[] = ["connecting"];
        for (let cycle = 0; cycle < 3; cycle++) {
            host.status("connected");
            host.status("disconnected");
            expected.push("connected", "disconnected");
            expect({ client: sandbox.getClientSync(), namespace: client.system }).toEqual({
                client,
                namespace,
            });
        }
        expect({ statuses, ...host.reads() }).toEqual({
            statuses: expected,
            clientReads: 1,
            subscriptions: 1,
        });
    });

    it("caches the client before a synchronous status listener calls getClientSync", async () => {
        host = installHost("connected");
        const sandbox = await importSandbox();
        const clients: ReturnType<typeof sandbox.getClientSync>[] = [];
        sandbox.subscribeConnectionStatus(() => clients.push(sandbox.getClientSync()));
        expect({ clients, ...host.reads() }).toEqual({
            clients: [host.client],
            clientReads: 1,
            subscriptions: 1,
        });
    });

    it("keeps the host's disconnected status when the first observer subscribes", async () => {
        host = installHost("disconnected");
        const sandbox = await importSandbox();
        const statuses: ConnectionStatus[] = [];
        sandbox.subscribeConnectionStatus((status) => statuses.push(status));
        expect(statuses).toEqual(["disconnected"]);
    });

    it("does not restart a disconnected host when a status callback subscribes again", async () => {
        host = installHost("connected");
        const sandbox = await importSandbox();
        const statuses: ConnectionStatus[] = [];
        const nested: ConnectionStatus[] = [];
        const clients: ReturnType<typeof sandbox.getClientSync>[] = [];
        sandbox.subscribeConnectionStatus((status) => {
            statuses.push(status);
            if (status === "disconnected") {
                clients.push(sandbox.getClientSync());
                sandbox.subscribeConnectionStatus((next) => nested.push(next));
            }
        });
        host.status("disconnected");
        host.status("connecting");
        host.status("connected");
        expect({ statuses, nested, clients, ...host.reads() }).toEqual({
            statuses: ["connected", "disconnected", "connecting", "connected"],
            nested: ["disconnected", "connecting", "connected"],
            clients: [host.client],
            clientReads: 1,
            subscriptions: 1,
        });
    });

    it("keeps public methods replaceable and stops notifying unsubscribed observers", async () => {
        host = installHost();
        const sandbox = await importSandbox();
        const statuses: ConnectionStatus[] = [];
        const unsubscribe = sandbox.subscribeConnectionStatus((status) => statuses.push(status));
        const client = sandbox.getClientSync()!;
        const replacement = client.system.handshake.bind(client.system);
        client.system.handshake = replacement;
        unsubscribe();
        host.status("connected");
        expect({ method: client.system.handshake, statuses }).toEqual({
            method: replacement,
            statuses: ["connecting"],
        });
    });

    it("does not redirect an adopted host client to an explicit WebSocket endpoint", async () => {
        host = installHost("connected");
        const sandbox = await importSandbox();
        sandbox.getClientSync();
        expect(() => sandbox.connectWebSocketHost("ws://127.0.0.1:9955")).toThrow(
            "connectWebSocketHost must be called before the TrUAPI client is created",
        );
        expect(sandbox.getClientSync()).toBe(host.client);
    });
});
