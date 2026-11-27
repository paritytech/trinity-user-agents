import { afterEach, describe, expect, it, jest } from "bun:test";
import { createHostConnection, type HostConnection } from "./host-connection.js";
import {
    ConnectionResetError,
    createClient,
    createMessagePortProvider,
    createTransport,
    decodeWireMessage,
    encodeWireMessage,
    MESSAGE_TYPE_RECEIVE,
    scale as S,
    type ProtocolMessage,
    type TrUApiClient,
    type WebSocketWireProvider,
} from "@parity/truapi";
import * as T from "@parity/truapi";
import { createWebSocketProviderFactory } from "@parity/truapi/internal";
import * as W from "@parity/truapi/wire-table";

const cleanup: (() => void)[] = [];
afterEach(() => {
    for (const close of cleanup.splice(0)) close();
});

// Builds the client the way the product's SDK does over the injected transport.
function productClient({ transport }: HostConnection): TrUApiClient {
    return createClient(
        createTransport({ ...transport, dispose() {} }, { prepare: transport.prepare }),
    );
}

function last<Item>(items: readonly Item[]): Item | undefined {
    return items[items.length - 1];
}

async function until(condition: () => boolean) {
    for (let attempt = 0; attempt < 1000 && !condition(); attempt++) await Bun.sleep(1);
    expect(condition()).toBe(true);
}

function host(createProvider?: (url: string) => WebSocketWireProvider) {
    const frames: ProtocolMessage[] = [];
    const sockets: Bun.ServerWebSocket<undefined>[] = [];
    let answer = true;
    const server = Bun.serve<undefined>({
        hostname: "127.0.0.1",
        port: 0,
        fetch(request, server) {
            if (server.upgrade(request)) return;
            return new Response("WebSocket required", { status: 400 });
        },
        websocket: {
            open(socket) {
                sockets.push(socket);
            },
            message(socket, bytes) {
                const message = decodeWireMessage(
                    new Uint8Array(bytes as Uint8Array),
                )._unsafeUnwrap();
                frames.push(message);
                if (!answer || message.payload.messageType !== 0) return;
                const handshake =
                    message.payload.traitId === W.SYSTEM_HANDSHAKE.trait &&
                    message.payload.methodId === W.SYSTEM_HANDSHAKE.method;
                if (
                    !handshake &&
                    message.payload.traitId !== W.PERMISSIONS_REQUEST_REMOTE_PERMISSION.trait
                )
                    return;
                const value = handshake
                    ? S.Result(
                          T.VersionedHostHandshakeResponse,
                          S.CallError(T.VersionedHostHandshakeError),
                      ).enc({ success: true, value: { tag: "V1", value: undefined } })
                    : S.Result(
                          T.VersionedRemotePermissionResponse,
                          S.CallError(T.VersionedRemotePermissionError),
                      ).enc({ success: true, value: { tag: "V1", value: { granted: false } } });
                socket.send(
                    encodeWireMessage({
                        ...message,
                        payload: { ...message.payload, messageType: 1, value },
                    })._unsafeUnwrap(),
                );
            },
        },
    });
    const connection = createHostConnection(`ws://127.0.0.1:${server.port}`, createProvider);
    cleanup.push(() => {
        connection.dispose();
        server.stop(true);
    });
    return {
        connection,
        frames,
        sockets,
        answer(value: boolean) {
            answer = value;
        },
    };
}

const permission: T.RemotePermissionRequest = {
    permission: { tag: "Remote", value: { domains: ["denied.example"] } },
};

function controlledHost(url = "ws://127.0.0.1:9955") {
    const originalSocket = globalThis.WebSocket;
    const sockets: ControlledSocket[] = [];
    const statuses: string[] = [];
    let answerHandshake = true;
    jest.useFakeTimers({ now: 0 });
    class ControlledSocket extends EventTarget {
        static readonly OPEN = 1;
        static readonly CLOSED = 3;
        readyState = 0;
        private binary = "blob";
        readonly sent: ProtocolMessage[] = [];
        get binaryType() {
            return this.binary;
        }
        set binaryType(value: string) {
            this.binary = value;
        }
        constructor(readonly url: string) {
            super();
            sockets.push(this);
        }
        open() {
            this.readyState = 1;
            this.dispatchEvent(new Event("open"));
        }
        send(frame: Uint8Array) {
            if (this.readyState !== 1) throw new Error("socket is not open");
            const message = decodeWireMessage(frame)._unsafeUnwrap();
            this.sent.push(message);
            if (
                answerHandshake &&
                message.payload.messageType === 0 &&
                message.payload.traitId === W.SYSTEM_HANDSHAKE.trait &&
                message.payload.methodId === W.SYSTEM_HANDSHAKE.method
            )
                queueMicrotask(() => this.reply(message, Uint8Array.of(0, 0)));
        }
        receive(message: ProtocolMessage) {
            const frame = encodeWireMessage(message)._unsafeUnwrap();
            this.dispatchEvent(new MessageEvent("message", { data: frame.buffer }));
        }
        reply(message: ProtocolMessage, value: Uint8Array) {
            this.receive({ ...message, payload: { ...message.payload, messageType: 1, value } });
        }
        close() {
            if (this.readyState === 3) return;
            this.readyState = 3;
            this.dispatchEvent(new Event("close"));
        }
    }
    globalThis.WebSocket = ControlledSocket as unknown as typeof WebSocket;
    const connection = createHostConnection(url);
    connection.subscribeConnectionStatus((status) => statuses.push(status));
    cleanup.push(() => {
        try {
            connection.dispose();
        } finally {
            globalThis.WebSocket = originalSocket;
            jest.useRealTimers();
        }
    });
    return {
        connection,
        sockets,
        statuses,
        respond(value: boolean) {
            answerHandshake = value;
        },
        advance(milliseconds: number) {
            jest.advanceTimersByTime(milliseconds);
        },
    };
}

describe("shared SDK host connection", () => {
    it("uses one transport for public calls and internal authorization", async () => {
        const { connection, sockets, frames } = host();
        const client = productClient(connection);
        const grant = client.permissions.requestRemotePermission(permission);
        const authorization = connection.internal.permissions.authorizeRemotePermission(permission);
        expect([(await grant)._unsafeUnwrap(), (await authorization)._unsafeUnwrap()]).toEqual([
            { granted: false },
            { granted: false },
        ]);
        expect({
            connections: sockets.length,
            ids: new Set(frames.map((frame) => frame.requestId)).size,
            methods: frames.map((frame) => [frame.payload.traitId, frame.payload.methodId]),
        }).toEqual({
            connections: 1,
            ids: 3,
            methods: [
                [W.SYSTEM_HANDSHAKE.trait, W.SYSTEM_HANDSHAKE.method],
                [
                    W.PERMISSIONS_REQUEST_REMOTE_PERMISSION.trait,
                    W.PERMISSIONS_REQUEST_REMOTE_PERMISSION.method,
                ],
                [
                    W.PERMISSIONS_AUTHORIZE_REMOTE_PERMISSION.trait,
                    W.PERMISSIONS_AUTHORIZE_REMOTE_PERMISSION.method,
                ],
            ],
        });
    });

    it("starts a connection for a passive client and retains it after socket loss without lifecycle hooks", async () => {
        const createProvider = jest.fn(createWebSocketProviderFactory());
        const { connection, sockets } = host(createProvider);
        const statuses: string[] = [];
        connection.subscribeConnectionStatus((status) => statuses.push(status));
        const client = productClient(connection);
        await until(() => last(statuses) === "connected");
        sockets[0]!.close();
        await until(() => sockets.length === 2 && last(statuses) === "connected");
        expect(createProvider).toHaveBeenCalledTimes(2);
        expect(
            (await client.permissions.requestRemotePermission(permission))._unsafeUnwrap(),
        ).toEqual({ granted: false });
        expect(statuses).toEqual([
            "disconnected",
            "connecting",
            "connected",
            "disconnected",
            "connecting",
            "connected",
        ]);
    });

    it("fails interrupted calls and subscriptions instead of replaying them", async () => {
        const fixture = host();
        const client = productClient(fixture.connection);
        await client.system.handshake();
        fixture.answer(false);
        const interrupted = Promise.resolve(
            client.permissions.requestRemotePermission(permission),
        ).catch((error) => error);
        const errors: unknown[] = [];
        client.theme.subscribe().subscribe({ error: (error) => errors.push(error.cause) });
        await until(() =>
            fixture.frames.some((frame) => frame.payload.traitId === W.THEME_SUBSCRIBE.trait),
        );
        fixture.answer(true);
        fixture.sockets[0]!.close();
        expect((await interrupted).name).toBe("ConnectionResetError");
        await until(() => fixture.sockets.length === 2);
        expect(errors).toHaveLength(1);
        expect((errors[0] as Error).name).toBe("ConnectionResetError");
        expect((await client.permissions.requestRemotePermission(permission)).isOk()).toBe(true);
        expect(
            fixture.frames.filter(
                (frame) =>
                    frame.payload.methodId === W.PERMISSIONS_REQUEST_REMOTE_PERMISSION.method &&
                    frame.payload.traitId === W.PERMISSIONS_REQUEST_REMOTE_PERMISSION.trait,
            ),
        ).toHaveLength(2);
    });

    it("keeps legacy replies separate and does not replace the legacy port after a disconnect", async () => {
        const { connection, sockets, frames } = host();
        const port = connection.legacyPort;
        const replies: ProtocolMessage[] = [];
        port.addEventListener("message", (event) =>
            replies.push(decodeWireMessage(event.data)._unsafeUnwrap()),
        );
        const transport = createTransport(createMessagePortProvider(port));
        cleanup.push(() => transport.dispose());
        const legacy = createClient(transport);
        expect((await legacy.permissions.requestRemotePermission(permission)).isOk()).toBe(true);
        expect(
            (await connection.internal.permissions.authorizeRemotePermission(permission)).isOk(),
        ).toBe(true);
        port.postMessage(
            encodeWireMessage({ ...frames[0]!, requestId: "host:forged" })._unsafeUnwrap(),
        );
        await Bun.sleep(5);
        expect({
            connections: sockets.length,
            replies: replies.map((frame) => frame.requestId),
            forged: frames.some((frame) => frame.requestId === "host:forged"),
        }).toEqual({ connections: 1, replies: ["p:1"], forged: false });
        sockets[0]!.close();
        await until(() => sockets.length === 2);
        expect(connection.legacyPort).toBe(port);
    });

    it("closes the physical socket and permanently rejects work on explicit disposal", async () => {
        const fixture = host();
        const statuses: string[] = [];
        fixture.connection.subscribeConnectionStatus((status) => statuses.push(status));
        const client = productClient(fixture.connection);
        await until(() => last(statuses) === "connected");
        fixture.connection.dispose();
        expect(last(statuses)).toBe("disconnected");
        await until(() => fixture.sockets[0]!.readyState === WebSocket.CLOSED);
        await expect(
            Promise.resolve(client.permissions.requestRemotePermission(permission)),
        ).rejects.toBeInstanceOf(ConnectionResetError);
        expect(fixture.sockets).toHaveLength(1);
    });

    it("retains renderer handlers and receives a new host start without another public call", async () => {
        const fixture = host();
        const client = productClient(fixture.connection);
        const handled: T.ProductRendererRenderRequest[] = [];
        let teardowns = 0;
        client.renderer.onRender((request) => {
            handled.push(request);
            return () => {
                teardowns++;
            };
        });
        const request: T.V01ProductRendererRenderRequest = {
            context: {
                tag: "ChatMessage",
                value: { roomId: "room", messageId: "message", messageType: "vote" },
            },
            payload: "0x",
        };
        const start = encodeWireMessage({
            requestId: "h:1",
            payload: {
                traitId: W.RENDERER_RENDER.trait,
                methodId: W.RENDERER_RENDER.method,
                messageType: 0,
                value: T.VersionedProductRendererRenderRequest.enc({ tag: "V1", value: request }),
            },
        })._unsafeUnwrap();
        await until(() => fixture.sockets.length === 1);
        fixture.sockets[0]!.send(start);
        await until(() => handled.length === 1);
        fixture.sockets[0]!.close();
        await until(() => fixture.sockets.length === 2);
        fixture.sockets[1]!.send(start);
        await until(() => handled.length === 2);
        expect({ handled, teardowns }).toEqual({ handled: [request, request], teardowns: 1 });
    });
});

// Products built with an SDK that predates the injected transport reach the
// host only through this port, and have no other signal that a connection was
// replaced.
describe("legacy port across connection loss", () => {
    const lost = {
        tag: "HostFailure" as const,
        value: { reason: "TrUAPI host connection interrupted" },
    };

    function portClient(connection: HostConnection): TrUApiClient {
        const transport = createTransport(createMessagePortProvider(connection.legacyPort));
        cleanup.push(() => transport.dispose());
        return createClient(transport);
    }

    it("keeps a port client working on the replacement connection", async () => {
        const { connection, sockets } = host();
        const client = portClient(connection);
        expect((await client.permissions.requestRemotePermission(permission)).isOk()).toBe(true);
        sockets[0]!.close();
        await until(() => sockets.length === 2);
        expect((await client.permissions.requestRemotePermission(permission)).isOk()).toBe(true);
    });

    it("ends the calls and subscriptions a lost connection carried with a host failure", async () => {
        const fixture = host();
        const client = portClient(fixture.connection);
        await client.system.handshake();
        fixture.answer(false);
        const call = client.permissions.requestRemotePermission(permission);
        const interrupted: unknown[] = [];
        client.theme.subscribe().subscribe({ error: (error) => interrupted.push(error.reason) });
        await until(
            () =>
                fixture.frames.filter(
                    (frame) =>
                        frame.payload.traitId === W.THEME_SUBSCRIBE.trait ||
                        frame.payload.traitId === W.PERMISSIONS_REQUEST_REMOTE_PERMISSION.trait,
                ).length === 2,
        );
        fixture.sockets[0]!.close();
        const result = await call;
        await until(() => interrupted.length === 1);
        expect({ call: result.isErr() && result.error, interrupted }).toEqual({
            call: lost,
            interrupted: [lost],
        });
    });

    it("fails a call it could not send when the host cannot be dialed", async () => {
        const dial = createWebSocketProviderFactory();
        let dials = 0;
        const { connection, sockets } = host((url) => {
            if (dials++ > 0) throw new Error("refused");
            return dial(url);
        });
        const statuses: string[] = [];
        connection.subscribeConnectionStatus((status) => statuses.push(status));
        const client = portClient(connection);
        await client.system.handshake();
        sockets[0]!.close();
        await until(() => dials === 2 && last(statuses) === "disconnected");
        const result = await client.permissions.requestRemotePermission(permission);
        expect(result.isErr() && result.error).toEqual(lost);
    });

    it("stops the streams the host started on a lost connection", async () => {
        const fixture = host();
        const client = portClient(fixture.connection);
        let renders = 0;
        let teardowns = 0;
        client.renderer.onRender(() => {
            renders++;
            return () => {
                teardowns++;
            };
        });
        const request: T.V01ProductRendererRenderRequest = {
            context: {
                tag: "ChatMessage",
                value: { roomId: "room", messageId: "message", messageType: "vote" },
            },
            payload: "0x",
        };
        const start = encodeWireMessage({
            requestId: "h:1",
            payload: {
                traitId: W.RENDERER_RENDER.trait,
                methodId: W.RENDERER_RENDER.method,
                messageType: 0,
                value: T.VersionedProductRendererRenderRequest.enc({ tag: "V1", value: request }),
            },
        })._unsafeUnwrap();
        await client.system.handshake();
        fixture.sockets[0]!.send(start);
        await until(() => renders === 1);
        fixture.sockets[0]!.close();
        await until(() => teardowns === 1);
    });
});

describe("shared SDK connection failure timing", () => {
    async function untilPrepared(condition: () => boolean) {
        for (let turn = 0; turn < 200 && !condition(); turn++) await Promise.resolve();
        expect(condition()).toBe(true);
    }

    // A browser opens a socket in a later task, once every pending microtask has run.
    async function nextTask() {
        for (let turn = 0; turn < 200; turn++) await Promise.resolve();
    }

    it.each([0, -60_000])(
        "checks a stale OPEN socket and sends the side effect on its replacement after a %i ms clock adjustment",
        async (clockAdjustment) => {
            const fixture = controlledHost();
            const client = productClient(fixture.connection);
            const stale = fixture.sockets[0]!;
            const initial = client.permissions.requestRemotePermission(permission);
            stale.open();
            await untilPrepared(() => stale.sent.length === 2);
            stale.reply(stale.sent[1]!, Uint8Array.of(0, 0, 0));
            expect((await initial)._unsafeUnwrap()).toEqual({ granted: false });
            jest.setSystemTime(Date.now() + clockAdjustment);
            fixture.advance(10_001);
            fixture.respond(false);
            const navigation = client.system.navigateTo({ url: "https://example.com" });
            await untilPrepared(() => stale.sent.length === 3);
            expect([stale.sent[2]!.payload.traitId, stale.sent[2]!.payload.methodId]).toEqual([
                W.SYSTEM_HANDSHAKE.trait,
                W.SYSTEM_HANDSHAKE.method,
            ]);
            expect(stale.readyState).toBe(WebSocket.OPEN);
            fixture.advance(10_000);
            await untilPrepared(() => last(fixture.statuses) === "disconnected");
            fixture.respond(true);
            fixture.advance(0);
            await nextTask();
            const replacement = fixture.sockets[1]!;
            replacement.open();
            await untilPrepared(() => replacement.sent.length === 2);
            replacement.reply(
                replacement.sent.find(
                    (message) =>
                        message.payload.traitId === W.SYSTEM_NAVIGATE_TO.trait &&
                        message.payload.methodId === W.SYSTEM_NAVIGATE_TO.method,
                )!,
                Uint8Array.of(0, 0),
            );
            expect({
                outcome: (await navigation)._unsafeUnwrap(),
                navigations: fixture.sockets.map(
                    (socket) =>
                        socket.sent.filter(
                            (message) =>
                                message.payload.traitId === W.SYSTEM_NAVIGATE_TO.trait &&
                                message.payload.methodId === W.SYSTEM_NAVIGATE_TO.method,
                        ).length,
                ),
                replacementFrames: replacement.sent.map((message) => [
                    message.payload.traitId,
                    message.payload.methodId,
                ]),
            }).toEqual({
                outcome: undefined,
                navigations: [0, 1],
                replacementFrames: [
                    [W.SYSTEM_HANDSHAKE.trait, W.SYSTEM_HANDSHAKE.method],
                    [W.SYSTEM_NAVIGATE_TO.trait, W.SYSTEM_NAVIGATE_TO.method],
                ],
            });
        },
    );

    it.each([
        ["background", 10_000],
        ["visible", 45_250],
    ] as const)("keeps one pending WebSocket for the %s recovery window", async (page, budget) => {
        if (page === "visible") visibleDocument();
        const fixture = controlledHost();
        const client = productClient(fixture.connection);
        fixture.sockets[0]!.open();
        await untilPrepared(() => last(fixture.statuses) === "connected");
        fixture.sockets[0]!.close();
        fixture.advance(0);
        const socket = fixture.sockets[1]!;
        let failure: Error | undefined;
        let openingFailed = false;
        void fixture.connection.transport.prepare(W.SYSTEM_HANDSHAKE).catch(() => {
            openingFailed = true;
        });
        void Promise.resolve(client.permissions.requestRemotePermission(permission)).catch(
            (error) => {
                failure = error;
            },
        );
        fixture.advance(budget - 1);
        await nextTask();
        expect(failure).toBeUndefined();
        fixture.advance(1);
        await nextTask();
        expect({
            error: failure?.name,
            openingFailed,
            attempts: fixture.sockets.length,
            sent: socket.sent,
            status: last(fixture.statuses),
        }).toEqual({
            error: "ConnectionResetError",
            openingFailed: true,
            attempts: 2,
            sent: [],
            status: "connecting",
        });

        const opening = fixture.connection.transport.prepare(W.SYSTEM_HANDSHAKE);
        fixture.respond(false);
        fixture.advance(budget - 1_000);
        socket.open();
        await opening;
        await untilPrepared(() => socket.sent.length === 1);
        const fresh = client.permissions.requestRemotePermission(permission);
        fixture.advance(2_000);
        socket.reply(socket.sent[0]!, Uint8Array.of(0, 0));
        await untilPrepared(() => socket.sent.length === 2);
        socket.reply(socket.sent[1]!, Uint8Array.of(0, 0, 0));
        expect((await fresh)._unsafeUnwrap()).toEqual({ granted: false });
    });

    it("reconnects a visible client whose WebSocket upgrade stalls", async () => {
        visibleDocument();
        const fixture = controlledHost();
        void productClient(fixture.connection);

        fixture.advance(120_000);
        expect(fixture.sockets[0]!.readyState).toBe(WebSocket.CLOSED);
        fixture.advance(250);
        fixture.sockets[1]!.open();
        await untilPrepared(() => last(fixture.statuses) === "connected");

        fixture.advance(120_000);
        expect(last(fixture.statuses)).toBe("connected");
    });

    it("stops automatic replacement after one failed reconnect instead of retrying indefinitely", async () => {
        const fixture = controlledHost();
        void productClient(fixture.connection);
        fixture.sockets[0]!.open();
        await untilPrepared(() => last(fixture.statuses) === "connected");
        fixture.sockets[0]!.close();
        fixture.advance(0);
        fixture.sockets[1]!.close();
        await untilPrepared(() => last(fixture.statuses) === "disconnected");
        fixture.advance(120_000);
        expect(fixture.sockets).toHaveLength(2);
    });

    function visibleDocument() {
        const previousDocument = globalThis.document;
        const document = Object.assign(new EventTarget(), { visibilityState: "visible" });
        globalThis.document = document as unknown as Document;
        cleanup.push(() => {
            if (previousDocument) globalThis.document = previousDocument;
            else delete (globalThis as { document?: Document }).document;
        });
    }

    // A reconnect can be refused for a moment while the host rebinds its
    // listener; a visible page must not stay offline until its next call.
    it("retries a reconnect that fails while the page is visible", async () => {
        visibleDocument();
        const fixture = controlledHost();
        void productClient(fixture.connection);
        fixture.sockets[0]!.open();
        await untilPrepared(() => last(fixture.statuses) === "connected");
        fixture.sockets[0]!.close();
        fixture.advance(0);
        fixture.sockets[1]!.close();
        await untilPrepared(() => last(fixture.statuses) === "disconnected");
        fixture.advance(250);
        fixture.sockets[2]!.open();
        await untilPrepared(() => last(fixture.statuses) === "connected");
        expect(fixture.sockets).toHaveLength(3);
    });

    // A host that stays unreachable must not keep a visible page dialing.
    it("stops retrying a refused reconnect after a bounded number of attempts", async () => {
        visibleDocument();
        const fixture = controlledHost();
        void productClient(fixture.connection);
        fixture.sockets[0]!.open();
        await untilPrepared(() => last(fixture.statuses) === "connected");
        fixture.sockets[0]!.close();
        for (const delay of [0, 250, 1_000, 4_000]) {
            fixture.advance(delay);
            last(fixture.sockets)!.close();
        }
        fixture.advance(120_000);
        expect(fixture.sockets).toHaveLength(5);
    });

    it("sends calls and subscription starts made during a refused redial on the connection that completes its handshake", async () => {
        visibleDocument();
        const fixture = controlledHost();
        const client = productClient(fixture.connection);
        fixture.sockets[0]!.open();
        await untilPrepared(() => last(fixture.statuses) === "connected");
        fixture.sockets[0]!.close();
        const response = client.permissions.requestRemotePermission(permission);
        client.theme.subscribe().subscribe();
        fixture.advance(0);
        fixture.sockets[1]!.close();
        fixture.advance(250);
        await nextTask();
        const replacement = fixture.sockets[2]!;
        replacement.open();
        await untilPrepared(() => replacement.sent.length === 3);
        replacement.reply(
            replacement.sent.find(
                (message) =>
                    message.payload.traitId === W.PERMISSIONS_REQUEST_REMOTE_PERMISSION.trait &&
                    message.payload.methodId === W.PERMISSIONS_REQUEST_REMOTE_PERMISSION.method,
            )!,
            Uint8Array.of(0, 0, 0),
        );
        expect({
            decision: (await response)._unsafeUnwrap(),
            frames: fixture.sockets.map((socket) =>
                socket.sent.map((message) => [message.payload.traitId, message.payload.methodId]),
            ),
        }).toEqual({
            decision: { granted: false },
            frames: [
                [[W.SYSTEM_HANDSHAKE.trait, W.SYSTEM_HANDSHAKE.method]],
                [],
                [
                    [W.SYSTEM_HANDSHAKE.trait, W.SYSTEM_HANDSHAKE.method],
                    [
                        W.PERMISSIONS_REQUEST_REMOTE_PERMISSION.trait,
                        W.PERMISSIONS_REQUEST_REMOTE_PERMISSION.method,
                    ],
                    [W.THEME_SUBSCRIBE.trait, W.THEME_SUBSCRIBE.method],
                ],
            ],
        });
    });

    it("rejects a call waiting for a redial once no further dial is scheduled", async () => {
        const fixture = controlledHost();
        const client = productClient(fixture.connection);
        fixture.sockets[0]!.open();
        await untilPrepared(() => last(fixture.statuses) === "connected");
        fixture.sockets[0]!.close();
        const pending = Promise.resolve(
            client.permissions.requestRemotePermission(permission),
        ).catch((error) => error);
        fixture.advance(0);
        fixture.sockets[1]!.close();
        const error = await pending;
        fixture.advance(120_000);
        expect({ error: error.name, sockets: fixture.sockets.length }).toEqual({
            error: "ConnectionResetError",
            sockets: 2,
        });
    });

    it("rejects a waiting call when a visible page spends its retries", async () => {
        visibleDocument();
        const fixture = controlledHost();
        const client = productClient(fixture.connection);
        fixture.sockets[0]!.open();
        await untilPrepared(() => last(fixture.statuses) === "connected");
        fixture.sockets[0]!.close();
        const pending = Promise.resolve(
            client.permissions.requestRemotePermission(permission),
        ).catch((error) => error);
        for (const delay of [0, 250, 1_000, 4_000]) {
            fixture.advance(delay);
            last(fixture.sockets)!.close();
        }
        const error = await pending;
        fixture.advance(120_000);
        expect({ error: error.name, sockets: fixture.sockets.length }).toEqual({
            error: "ConnectionResetError",
            sockets: 5,
        });
    });

    it("fails a call whose connection retired before its frame could be written", async () => {
        const fixture = controlledHost();
        const client = productClient(fixture.connection);
        fixture.sockets[0]!.open();
        await untilPrepared(() => last(fixture.statuses) === "connected");
        let failure: Error | undefined;
        void Promise.resolve(client.permissions.requestRemotePermission(permission)).catch(
            (error) => {
                failure = error;
            },
        );
        await Promise.resolve();
        fixture.sockets[0]!.close();
        await untilPrepared(() => failure !== undefined);
        expect(failure!.name).toBe("ConnectionResetError");
    });

    it("warns once with the close reason when a connection retires", async () => {
        const fixture = controlledHost();
        const client = productClient(fixture.connection);
        fixture.sockets[0]!.open();
        await untilPrepared(() => last(fixture.statuses) === "connected");
        const warnings: unknown[][] = [];
        const originalWarn = console.warn;
        console.warn = (...args: unknown[]) => {
            warnings.push(args);
        };
        try {
            const pending = Promise.resolve(
                client.permissions.requestRemotePermission(permission),
            ).catch((error) => error);
            await untilPrepared(() => fixture.sockets[0]!.sent.length === 2);
            fixture.sockets[0]!.close();
            await pending;
        } finally {
            console.warn = originalWarn;
        }
        expect(warnings).toEqual([
            ["[truapi] host link lost: websocket closed (ws://127.0.0.1:9955)"],
        ]);
    });

    it.each([
        ["close", "websocket closed"],
        ["error", "websocket error"],
    ])(
        "keeps the bridge token in the URL query out of the warning a socket %s causes",
        async (event, reason) => {
            const fixture = controlledHost("ws://127.0.0.1:9955/?t=0123456789abcdef");
            void productClient(fixture.connection);
            fixture.sockets[0]!.open();
            await untilPrepared(() => last(fixture.statuses) === "connected");
            const warnings: unknown[][] = [];
            const originalWarn = console.warn;
            console.warn = (...args: unknown[]) => {
                warnings.push(args);
            };
            try {
                fixture.sockets[0]!.dispatchEvent(new Event(event));
            } finally {
                console.warn = originalWarn;
            }
            expect(warnings).toEqual([
                [`[truapi] host link lost: ${reason} (ws://127.0.0.1:9955)`],
            ]);
        },
    );

    it("wakes existing subscription listeners when a failed background replacement becomes visible", async () => {
        const previousDocument = globalThis.document;
        const document = Object.assign(new EventTarget(), { visibilityState: "visible" });
        globalThis.document = document as unknown as Document;
        cleanup.push(() => {
            if (previousDocument) globalThis.document = previousDocument;
            else delete (globalThis as { document?: Document }).document;
        });
        const fixture = controlledHost();
        const client = productClient(fixture.connection);
        const items: T.HostAccountConnectionStatusSubscribeItem[] = [];
        let waiting = false;
        function watch() {
            waiting = false;
            client.account.connectionStatusSubscribe().subscribe({
                next: (item) => items.push(item),
                error: (error) => {
                    waiting = error.cause instanceof ConnectionResetError;
                },
            });
        }
        fixture.connection.subscribeConnectionStatus((status) => {
            if (status === "connected" && waiting) watch();
        });
        watch();
        fixture.sockets[0]!.open();
        await untilPrepared(() => fixture.sockets[0]!.sent.length === 2);
        document.visibilityState = "hidden";
        document.dispatchEvent(new Event("visibilitychange"));
        fixture.sockets[0]!.close();
        fixture.advance(0);
        fixture.sockets[1]!.close();
        await untilPrepared(() => last(fixture.statuses) === "disconnected");
        fixture.advance(120_000);
        expect(fixture.sockets).toHaveLength(2);

        document.visibilityState = "visible";
        document.dispatchEvent(new Event("visibilitychange"));
        expect(fixture.sockets).toHaveLength(3);
        const replacement = fixture.sockets[2]!;
        replacement.open();
        await untilPrepared(() => replacement.sent.length === 2);
        const subscription = replacement.sent[1]!;
        replacement.receive({
            ...subscription,
            payload: {
                ...subscription.payload,
                messageType: MESSAGE_TYPE_RECEIVE,
                value: T.VersionedHostAccountConnectionStatusSubscribeItem.enc({
                    tag: "V1",
                    value: "Connected",
                }),
            },
        });
        expect({ items, waiting }).toEqual({ items: ["Connected"], waiting: false });

        fixture.connection.dispose();
        document.dispatchEvent(new Event("visibilitychange"));
        fixture.advance(120_000);
        expect({ sockets: fixture.sockets.length, status: last(fixture.statuses) }).toEqual({
            sockets: 3,
            status: "disconnected",
        });
    });

    it("preserves a rejected handshake as the cause of interrupted calls", async () => {
        const fixture = controlledHost();
        fixture.respond(false);
        const client = productClient(fixture.connection);
        const pending = Promise.resolve(
            client.permissions.requestRemotePermission(permission),
        ).catch((error) => error);
        fixture.sockets[0]!.open();
        await untilPrepared(() => fixture.sockets[0]!.sent.length === 1);
        const cause = {
            tag: "Domain",
            value: { tag: "V1", value: { tag: "UnsupportedProtocolVersion" } },
        } as const;
        fixture.sockets[0]!.reply(
            fixture.sockets[0]!.sent[0]!,
            S.Result(
                T.VersionedHostHandshakeResponse,
                S.CallError(T.VersionedHostHandshakeError),
            ).enc({ success: false, value: cause }),
        );
        const error = await pending;
        expect(error).toBeInstanceOf(ConnectionResetError);
        expect(error.cause).toEqual(cause);
    });

    const malformedFrames = [
        ["wire envelope", new Uint8Array()],
        [
            "protocol-error payload",
            encodeWireMessage({
                requestId: "host:malformed",
                payload: { traitId: 255, methodId: 255, messageType: 1, value: new Uint8Array() },
            })._unsafeUnwrap(),
        ],
    ] as const;
    it.each(
        malformedFrames.flatMap(([name, frame]) =>
            [false, true].map((legacyPort) => [name, legacyPort, frame] as const),
        ),
    )(
        "recovers from a malformed %s without approving permissions (legacy port: %s)",
        async (_name, legacyPort, frame) => {
            const fixture = controlledHost();
            void productClient(fixture.connection);
            if (legacyPort) {
                const port = fixture.connection.legacyPort;
                cleanup.push(() => port.close());
            }
            fixture.sockets[0]!.open();
            await untilPrepared(() => last(fixture.statuses) === "connected");
            const pending = Promise.resolve(
                fixture.connection.internal.permissions.authorizeRemotePermission(permission),
            ).catch((error) => error);
            await untilPrepared(() => fixture.sockets[0]!.sent.length === 2);
            fixture.sockets[0]!.dispatchEvent(new MessageEvent("message", { data: frame.buffer }));
            const error = await pending;
            expect(error).toBeInstanceOf(ConnectionResetError);
            expect(error.cause).toBeInstanceOf(Error);
            fixture.advance(0);
            const replacement = fixture.sockets[1]!;
            replacement.open();
            await untilPrepared(() => last(fixture.statuses) === "connected");
            const fresh =
                fixture.connection.internal.permissions.authorizeRemotePermission(permission);
            await untilPrepared(() => replacement.sent.length === 2);
            replacement.reply(replacement.sent[1]!, Uint8Array.of(0, 0, 0));
            expect((await fresh)._unsafeUnwrap()).toEqual({ granted: false });
        },
    );

    it("ignores replies and errors from retired sockets through repeated resets", async () => {
        const fixture = controlledHost();
        const client = productClient(fixture.connection);
        fixture.sockets[0]!.open();
        await untilPrepared(() => last(fixture.statuses) === "connected");
        for (let cycle = 0; cycle < 3; cycle++) {
            const old = fixture.sockets[cycle]!;
            const pending = Promise.resolve(
                client.permissions.requestRemotePermission(permission),
            ).catch((error) => error);
            await untilPrepared(
                () =>
                    last(old.sent)?.payload.methodId ===
                        W.PERMISSIONS_REQUEST_REMOTE_PERMISSION.method &&
                    last(old.sent)?.payload.traitId ===
                        W.PERMISSIONS_REQUEST_REMOTE_PERMISSION.trait,
            );
            old.close();
            expect((await pending).name).toBe("ConnectionResetError");
            fixture.advance(0);
            const replacement = fixture.sockets[cycle + 1]!;
            replacement.open();
            await untilPrepared(() => last(fixture.statuses) === "connected");
            const response = client.permissions.requestRemotePermission(permission);
            await untilPrepared(() => replacement.sent.length === 2);
            const request = replacement.sent[1]!;
            old.reply(request, Uint8Array.of(0, 0, 1));
            old.dispatchEvent(new Event("error"));
            replacement.reply(request, Uint8Array.of(0, 0, 0));
            expect((await response)._unsafeUnwrap()).toEqual({ granted: false });
        }
        expect(fixture.sockets).toHaveLength(4);
    });

    it("keeps recovering when one status observer throws and another reenters the client", async () => {
        const fixture = controlledHost();
        const client = productClient(fixture.connection);
        fixture.connection.subscribeConnectionStatus((status) => {
            if (status === "connected") throw new Error("broken observer");
            if (status === "disconnected")
                void Promise.resolve(client.permissions.requestRemotePermission(permission)).catch(
                    () => {},
                );
        });
        const observed: string[] = [];
        fixture.connection.subscribeConnectionStatus((status) => observed.push(status));
        fixture.sockets[0]!.open();
        await untilPrepared(() => last(observed) === "connected");
        fixture.sockets[0]!.close();
        fixture.sockets[1]!.open();
        await untilPrepared(() => last(observed) === "connected");
        fixture.advance(0);
        expect({ observed, sockets: fixture.sockets.length }).toEqual({
            observed: ["connecting", "connected", "connecting", "connected"],
            sockets: 2,
        });
    });

    it("discards a scheduled reconnect when the owner disposes the connection", async () => {
        const fixture = controlledHost();
        const client = productClient(fixture.connection);
        fixture.sockets[0]!.open();
        await untilPrepared(() => last(fixture.statuses) === "connected");
        fixture.sockets[0]!.close();
        fixture.connection.dispose();
        fixture.advance(120_000);
        await expect(
            Promise.resolve(client.permissions.requestRemotePermission(permission)),
        ).rejects.toBeInstanceOf(ConnectionResetError);
        expect({ sockets: fixture.sockets.length, status: last(fixture.statuses) }).toEqual({
            sockets: 1,
            status: "disconnected",
        });
    });

    // An SDK that reopens a reset stream waits for a "connected" that follows a
    // status that is not. That only finds the new connection if the status
    // leaves "connected" between the reset and the reconnect; reopening on the
    // old status would dial a host that is not listening yet.
    it("reports a status other than connected between a stream's reset and the next connection", async () => {
        const fixture = controlledHost();
        const client = productClient(fixture.connection);
        let reopenedOn: { sockets: number; verified: boolean } | undefined;
        client.theme.subscribe().subscribe({
            error: (error) => {
                if (!(error.cause instanceof ConnectionResetError)) return;
                let lost = false;
                fixture.connection.subscribeConnectionStatus((status) => {
                    if (status !== "connected") lost = true;
                    else if (lost && !reopenedOn)
                        reopenedOn = {
                            sockets: fixture.sockets.length,
                            verified: last(fixture.statuses) === "connected",
                        };
                });
            },
        });
        fixture.sockets[0]!.open();
        await untilPrepared(() => fixture.sockets[0]!.sent.length === 2);

        fixture.sockets[0]!.close();
        expect(reopenedOn).toBeUndefined();
        fixture.advance(0);
        fixture.sockets[1]!.open();
        await untilPrepared(() => last(fixture.statuses) === "connected");

        expect(reopenedOn).toEqual({ sockets: 2, verified: true });
    });
});
