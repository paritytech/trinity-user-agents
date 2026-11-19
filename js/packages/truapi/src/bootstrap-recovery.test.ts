import { afterEach, describe, expect, it } from "bun:test";
import type { ConnectionStatus } from "./sandbox.js";
import { SYSTEM_HANDSHAKE } from "./generated/wire-table.js";
import {
    ConnectionResetError,
    MESSAGE_TYPE_REQUEST,
    MESSAGE_TYPE_RESPONSE,
    PROTOCOL_ERROR_METHOD_ID,
    PROTOCOL_ERROR_TRAIT_ID,
    decodeWireMessage,
    encodeWireMessage,
    type MethodIds,
} from "./transport.js";

let importCounter = 0;
async function importSandbox(): Promise<typeof import("./sandbox.js")> {
    return import(`./sandbox.ts?injected-transport=${++importCounter}`);
}

function installHost(initialStatus: ConnectionStatus = "connecting") {
    const priorWindow = globalThis.window;
    const win = new EventTarget() as unknown as Window & typeof globalThis;
    Object.assign(win, { top: win, parent: win });
    const sent: Uint8Array[] = [];
    const prepared: MethodIds[] = [];
    const listeners = new Set<(status: ConnectionStatus) => void>();
    let ready: Promise<void> = Promise.resolve();
    let refused = false;
    let reset: ((error: Error) => void) | undefined;
    let receive: ((frame: Uint8Array) => void) | undefined;
    let status = initialStatus;
    let frameSubscriptions = 0;
    let statusSubscriptions = 0;
    Object.defineProperty(win, "__HOST_API_TRANSPORT__", {
        value: Object.freeze({
            postMessage(frame: Uint8Array) {
                sent.push(frame);
            },
            subscribe(callback: (frame: Uint8Array) => void) {
                frameSubscriptions++;
                receive = callback;
                return () => {};
            },
            subscribeReset(callback: (error: Error) => void) {
                reset = callback;
                return () => {
                    reset = undefined;
                };
            },
            prepare(ids: MethodIds) {
                if (refused) throw new Error("host unreachable");
                prepared.push(ids);
                return ready;
            },
            subscribeConnectionStatus(callback: (status: ConnectionStatus) => void) {
                statusSubscriptions++;
                listeners.add(callback);
                callback(status);
                return () => {
                    listeners.delete(callback);
                };
            },
        }),
    });
    for (const name of ["__HOST_API_CLIENT__", "__HOST_API_PORT__", "__truapi_localhost"]) {
        Object.defineProperty(win, name, {
            get() {
                throw new Error(`an injected transport must not read ${name}`);
            },
        });
    }
    globalThis.window = win;
    return {
        win,
        sent,
        prepared,
        reads: () => ({ frameSubscriptions, statusSubscriptions }),
        holdReadiness() {
            let release!: () => void;
            ready = new Promise((resolve) => (release = resolve));
            return release;
        },
        refuseReadiness() {
            refused = true;
        },
        reset(error: Error) {
            reset?.(error);
        },
        deliver(frame: Uint8Array) {
            receive?.(frame);
        },
        status(next: ConnectionStatus) {
            status = next;
            for (const listener of [...listeners]) listener(next);
        },
        restore() {
            if (priorWindow === undefined) delete (globalThis as { window?: unknown }).window;
            else globalThis.window = priorWindow;
        },
    };
}

const settle = () => new Promise((resolve) => setTimeout(resolve, 0));

let host: ReturnType<typeof installHost> | null = null;
afterEach(() => {
    host?.restore();
    host = null;
});

describe("host-injected transport adoption", () => {
    // The product's own build encodes every frame, so it speaks the protocol
    // version it was compiled against whatever version the host was built with.
    it("encodes calls with the product's own client and sends them through the host", async () => {
        host = installHost("connected");
        const sandbox = await importSandbox();
        expect(sandbox.isCorrectEnvironment()).toBe(true);
        void sandbox.getClientSync()!.system.handshake();
        await settle();
        const frames = host.sent.map((frame) => {
            const { requestId, payload } = decodeWireMessage(frame)._unsafeUnwrap();
            return {
                requestId,
                ids: { trait: payload.traitId, method: payload.methodId },
                type: payload.messageType,
            };
        });
        expect(frames).toEqual([
            {
                requestId: "p:1",
                ids: { trait: SYSTEM_HANDSHAKE.trait, method: SYSTEM_HANDSHAKE.method },
                type: MESSAGE_TYPE_REQUEST,
            },
        ]);
    });

    it("waits for the host's readiness before sending", async () => {
        host = installHost();
        const release = host.holdReadiness();
        const sandbox = await importSandbox();
        void sandbox.getClientSync()!.system.handshake();
        await settle();
        const before = host.sent.length;
        release();
        await settle();
        expect({ before, after: host.sent.length, prepared: host.prepared }).toEqual({
            before: 0,
            after: 1,
            prepared: [SYSTEM_HANDSHAKE],
        });
    });

    // Products match the error class their own build exports, not the host's.
    it("fails a call the host lost or cannot carry with the product's ConnectionResetError and keeps the client", async () => {
        host = installHost("connected");
        const sandbox = await importSandbox();
        const client = sandbox.getClientSync()!;
        const lost = client.system.handshake();
        await settle();
        host.reset(new Error("host link lost"));
        host.refuseReadiness();
        const refused = client.system.handshake();
        const failures = await Promise.all(
            [lost, refused].map((call) => Promise.resolve(call).catch((error: unknown) => error)),
        );
        expect({
            reset: failures.map((failure) => failure instanceof ConnectionResetError),
            client: sandbox.getClientSync(),
        }).toEqual({ reset: [true, true], client });
    });

    // The host keeps its connection, and this client is never rebuilt, so a
    // frame it cannot read must not close it for the rest of the page.
    it("fails the calls in flight on an unreadable protocol error and keeps the client", async () => {
        host = installHost("connected");
        const sandbox = await importSandbox();
        const client = sandbox.getClientSync()!;
        const inFlight = Promise.resolve(client.system.handshake()).catch(
            (error: unknown) => error,
        );
        await settle();
        host.deliver(
            encodeWireMessage({
                requestId: "p:1",
                payload: {
                    traitId: PROTOCOL_ERROR_TRAIT_ID,
                    methodId: PROTOCOL_ERROR_METHOD_ID,
                    messageType: MESSAGE_TYPE_RESPONSE,
                    value: new Uint8Array(),
                },
            })._unsafeUnwrap(),
        );
        const failure = await inFlight;
        void client.system.handshake();
        await settle();
        expect({
            reset: failure instanceof ConnectionResetError,
            sent: host.sent.length,
        }).toEqual({ reset: true, sent: 2 });
    });

    it("adopts the injected transport before an iframe can negotiate another connection", async () => {
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
        expect(sandbox.getClientSync()).not.toBeNull();
    });

    it("follows the host's status and keeps the client across repeated resets", async () => {
        host = installHost();
        const sandbox = await importSandbox();
        const statuses: ConnectionStatus[] = [];
        sandbox.subscribeConnectionStatus((status) => statuses.push(status));
        const client = sandbox.getClientSync()!;
        const expected: ConnectionStatus[] = ["connecting"];
        for (let cycle = 0; cycle < 3; cycle++) {
            host.status("connected");
            host.reset(new Error("host link lost"));
            host.status("disconnected");
            expected.push("connected", "disconnected");
        }
        expect({ statuses, client: sandbox.getClientSync(), ...host.reads() }).toEqual({
            statuses: expected,
            client,
            frameSubscriptions: 1,
            statusSubscriptions: 1,
        });
    });

    it("caches the client before a synchronous status listener calls getClientSync", async () => {
        host = installHost("connected");
        const sandbox = await importSandbox();
        const clients: ReturnType<typeof sandbox.getClientSync>[] = [];
        sandbox.subscribeConnectionStatus(() => clients.push(sandbox.getClientSync()));
        expect({
            cached: clients.length === 1 && clients[0] === sandbox.getClientSync(),
            ...host.reads(),
        }).toEqual({
            cached: true,
            frameSubscriptions: 1,
            statusSubscriptions: 1,
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
        sandbox.subscribeConnectionStatus((status) => {
            statuses.push(status);
            if (status === "disconnected") {
                sandbox.getClientSync();
                sandbox.subscribeConnectionStatus((next) => nested.push(next));
            }
        });
        host.status("disconnected");
        host.status("connecting");
        host.status("connected");
        expect({ statuses, nested, ...host.reads() }).toEqual({
            statuses: ["connected", "disconnected", "connecting", "connected"],
            nested: ["disconnected", "connecting", "connected"],
            frameSubscriptions: 1,
            statusSubscriptions: 1,
        });
    });

    it("stops notifying unsubscribed observers", async () => {
        host = installHost();
        const sandbox = await importSandbox();
        const statuses: ConnectionStatus[] = [];
        const unsubscribe = sandbox.subscribeConnectionStatus((status) => statuses.push(status));
        sandbox.getClientSync();
        unsubscribe();
        host.status("connected");
        expect(statuses).toEqual(["connecting"]);
    });

    it("does not redirect an adopted host transport to an explicit WebSocket endpoint", async () => {
        host = installHost("connected");
        const sandbox = await importSandbox();
        const client = sandbox.getClientSync();
        expect(() => sandbox.connectWebSocketHost("ws://127.0.0.1:9955")).toThrow(
            "connectWebSocketHost must be called before the TrUAPI client is created",
        );
        expect(sandbox.getClientSync()).toBe(client);
    });
});
