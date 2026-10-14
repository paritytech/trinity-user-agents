import { afterEach, describe, expect, it, jest } from "bun:test";
import { createHostConnection } from "./host-connection.js";
import { createClient } from "./generated/client.js";
import { createTransport } from "./client.js";
import {
    ConnectionResetError,
    createMessagePortProvider,
    createWebSocketProviderFactory,
    decodeWireMessage,
    encodeWireMessage,
    MESSAGE_TYPE_RECEIVE,
    type ProtocolMessage,
    type WebSocketWireProvider,
} from "./transport.js";
import * as W from "./generated/wire-table.js";
import * as T from "./generated/types.js";
import * as S from "./scale.js";

const cleanup: (() => void)[] = [];
afterEach(() => {
    for (const close of cleanup.splice(0)) close();
});

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

function controlledHost() {
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
    const connection = createHostConnection("ws://127.0.0.1:9955");
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
        const client = connection.client;
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
        expect(connection.client).toBe(client);
    });

    it("starts a connection for a passive client and retains it after socket loss without lifecycle hooks", async () => {
        const createProvider = jest.fn(createWebSocketProviderFactory());
        const { connection, sockets } = host(createProvider);
        const statuses: string[] = [];
        connection.subscribeConnectionStatus((status) => statuses.push(status));
        const client = connection.client;
        await until(() => statuses.at(-1) === "connected");
        sockets[0]!.close();
        await until(() => sockets.length === 2 && statuses.at(-1) === "connected");
        expect(createProvider).toHaveBeenCalledTimes(2);
        expect(connection.client).toBe(client);
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
        const client = fixture.connection.client;
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
        const client = fixture.connection.client;
        await until(() => statuses.at(-1) === "connected");
        fixture.connection.dispose();
        expect(statuses.at(-1)).toBe("disconnected");
        await until(() => fixture.sockets[0]!.readyState === WebSocket.CLOSED);
        await expect(
            Promise.resolve(client.permissions.requestRemotePermission(permission)),
        ).rejects.toThrow("transport disposed");
        expect({ client: fixture.connection.client, connections: fixture.sockets.length }).toEqual({
            client,
            connections: 1,
        });
    });

    it("retains renderer handlers and receives a new host start without another public call", async () => {
        const fixture = host();
        const client = fixture.connection.client;
        const handled: T.ProductRendererRenderRequest[] = [];
        let teardowns = 0;
        client.renderer.onRender((request) => {
            handled.push(request);
            return () => {
                teardowns++;
            };
        });
        const request: T.ProductRendererRenderRequest = {
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

describe("shared SDK connection failure timing", () => {
    async function untilPrepared(condition: () => boolean) {
        for (let turn = 0; turn < 200 && !condition(); turn++) await Promise.resolve();
        expect(condition()).toBe(true);
    }

    it.each([0, -60_000])(
        "checks a stale OPEN socket before sending a side effect after a %i ms clock adjustment",
        async (clockAdjustment) => {
            const fixture = controlledHost();
            const client = fixture.connection.client;
            const stale = fixture.sockets[0]!;
            const initial = client.permissions.requestRemotePermission(permission);
            stale.open();
            await untilPrepared(() => stale.sent.length === 2);
            stale.reply(stale.sent[1]!, Uint8Array.of(0, 0, 0));
            expect((await initial)._unsafeUnwrap()).toEqual({ granted: false });
            jest.setSystemTime(Date.now() + clockAdjustment);
            fixture.advance(10_001);
            fixture.respond(false);
            const navigation = Promise.resolve(
                client.system.navigateTo({ url: "https://example.com" }),
            ).catch((error) => error);
            await untilPrepared(() => stale.sent.length === 3);
            expect([stale.sent[2]!.payload.traitId, stale.sent[2]!.payload.methodId]).toEqual([
                W.SYSTEM_HANDSHAKE.trait,
                W.SYSTEM_HANDSHAKE.method,
            ]);
            expect(stale.readyState).toBe(WebSocket.OPEN);
            fixture.advance(10_000);
            expect((await navigation).name).toBe("ConnectionResetError");
            fixture.respond(true);
            fixture.advance(0);
            fixture.sockets[1]!.open();
            await untilPrepared(() => fixture.statuses.at(-1) === "connected");
            expect(
                fixture.sockets
                    .flatMap((socket) => socket.sent)
                    .filter(
                        (message) =>
                            message.payload.traitId === W.SYSTEM_NAVIGATE_TO.trait &&
                            message.payload.methodId === W.SYSTEM_NAVIGATE_TO.method,
                    ),
            ).toEqual([]);
        },
    );

    it("bounds a failed opening without a retry loop and leaves a later call free to reconnect", async () => {
        const fixture = controlledHost();
        const client = fixture.connection.client;
        const pending = Promise.resolve(
            client.permissions.requestRemotePermission(permission),
        ).catch((error) => error);
        fixture.advance(10_000);
        expect((await pending).name).toBe("ConnectionResetError");
        fixture.advance(120_000);
        expect({
            attempts: fixture.sockets.length,
            sent: fixture.sockets[0]!.sent,
            status: fixture.statuses.at(-1),
        }).toEqual({ attempts: 1, sent: [], status: "disconnected" });
        const fresh = client.permissions.requestRemotePermission(permission);
        fixture.sockets[1]!.open();
        await untilPrepared(() => fixture.sockets[1]!.sent.length === 2);
        fixture.sockets[1]!.reply(fixture.sockets[1]!.sent[1]!, Uint8Array.of(0, 0, 0));
        expect((await fresh)._unsafeUnwrap()).toEqual({ granted: false });
    });

    it("stops automatic replacement after one failed reconnect instead of retrying indefinitely", async () => {
        const fixture = controlledHost();
        void fixture.connection.client;
        fixture.sockets[0]!.open();
        await untilPrepared(() => fixture.statuses.at(-1) === "connected");
        fixture.sockets[0]!.close();
        fixture.advance(0);
        fixture.sockets[1]!.close();
        await untilPrepared(() => fixture.statuses.at(-1) === "disconnected");
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
        void fixture.connection.client;
        fixture.sockets[0]!.open();
        await untilPrepared(() => fixture.statuses.at(-1) === "connected");
        fixture.sockets[0]!.close();
        fixture.advance(0);
        fixture.sockets[1]!.close();
        await untilPrepared(() => fixture.statuses.at(-1) === "disconnected");
        fixture.advance(250);
        fixture.sockets[2]!.open();
        await untilPrepared(() => fixture.statuses.at(-1) === "connected");
        expect(fixture.sockets).toHaveLength(3);
    });

    // A host that stays unreachable must not keep a visible page dialing.
    it("stops retrying a refused reconnect after a bounded number of attempts", async () => {
        visibleDocument();
        const fixture = controlledHost();
        void fixture.connection.client;
        fixture.sockets[0]!.open();
        await untilPrepared(() => fixture.statuses.at(-1) === "connected");
        fixture.sockets[0]!.close();
        for (const delay of [0, 250, 1_000, 4_000]) {
            fixture.advance(delay);
            fixture.sockets.at(-1)!.close();
        }
        fixture.advance(120_000);
        expect(fixture.sockets).toHaveLength(5);
    });

    it("wakes existing subscription listeners when a failed background replacement becomes visible", async () => {
        const previousDocument = globalThis.document;
        const document = Object.assign(new EventTarget(), { visibilityState: "visible" });
        globalThis.document = document as unknown as Document;
        cleanup.push(() => {
            if (previousDocument) globalThis.document = previousDocument;
            else delete (globalThis as { document?: Document }).document;
        });
        const fixture = controlledHost();
        const client = fixture.connection.client;
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
        await untilPrepared(() => fixture.statuses.at(-1) === "disconnected");
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
        expect({ items, waiting, client: fixture.connection.client }).toEqual({
            items: ["Connected"],
            waiting: false,
            client,
        });

        fixture.connection.dispose();
        document.dispatchEvent(new Event("visibilitychange"));
        fixture.advance(120_000);
        expect({ sockets: fixture.sockets.length, status: fixture.statuses.at(-1) }).toEqual({
            sockets: 3,
            status: "disconnected",
        });
    });

    it("preserves a rejected handshake as the cause of interrupted calls", async () => {
        const fixture = controlledHost();
        fixture.respond(false);
        const client = fixture.connection.client;
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
            const client = fixture.connection.client;
            if (legacyPort) {
                const port = fixture.connection.legacyPort;
                cleanup.push(() => port.close());
            }
            fixture.sockets[0]!.open();
            await untilPrepared(() => fixture.statuses.at(-1) === "connected");
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
            await untilPrepared(() => fixture.statuses.at(-1) === "connected");
            const fresh =
                fixture.connection.internal.permissions.authorizeRemotePermission(permission);
            await untilPrepared(() => replacement.sent.length === 2);
            replacement.reply(replacement.sent[1]!, Uint8Array.of(0, 0, 0));
            expect({
                decision: (await fresh)._unsafeUnwrap(),
                client: fixture.connection.client,
            }).toEqual({
                decision: { granted: false },
                client,
            });
        },
    );

    it("ignores replies and errors from retired sockets through repeated resets", async () => {
        const fixture = controlledHost();
        const client = fixture.connection.client;
        fixture.sockets[0]!.open();
        await untilPrepared(() => fixture.statuses.at(-1) === "connected");
        for (let cycle = 0; cycle < 3; cycle++) {
            const old = fixture.sockets[cycle]!;
            const pending = Promise.resolve(
                client.permissions.requestRemotePermission(permission),
            ).catch((error) => error);
            await untilPrepared(
                () =>
                    old.sent.at(-1)?.payload.methodId ===
                        W.PERMISSIONS_REQUEST_REMOTE_PERMISSION.method &&
                    old.sent.at(-1)?.payload.traitId ===
                        W.PERMISSIONS_REQUEST_REMOTE_PERMISSION.trait,
            );
            old.close();
            expect((await pending).name).toBe("ConnectionResetError");
            fixture.advance(0);
            const replacement = fixture.sockets[cycle + 1]!;
            replacement.open();
            await untilPrepared(() => fixture.statuses.at(-1) === "connected");
            const response = client.permissions.requestRemotePermission(permission);
            await untilPrepared(() => replacement.sent.length === 2);
            const request = replacement.sent[1]!;
            old.reply(request, Uint8Array.of(0, 0, 1));
            old.dispatchEvent(new Event("error"));
            replacement.reply(request, Uint8Array.of(0, 0, 0));
            expect({
                value: (await response)._unsafeUnwrap(),
                client: fixture.connection.client,
            }).toEqual({ value: { granted: false }, client });
        }
        expect(fixture.sockets).toHaveLength(4);
    });

    it("keeps recovering when one status observer throws and another reenters the client", async () => {
        const fixture = controlledHost();
        const client = fixture.connection.client;
        fixture.connection.subscribeConnectionStatus((status) => {
            if (status === "connected") throw new Error("broken observer");
            if (status === "disconnected") void fixture.connection.client;
        });
        const observed: string[] = [];
        fixture.connection.subscribeConnectionStatus((status) => observed.push(status));
        fixture.sockets[0]!.open();
        await untilPrepared(() => observed.at(-1) === "connected");
        fixture.sockets[0]!.close();
        fixture.sockets[1]!.open();
        await untilPrepared(() => observed.at(-1) === "connected");
        fixture.advance(0);
        expect({
            observed,
            sockets: fixture.sockets.length,
            client: fixture.connection.client,
        }).toEqual({
            observed: ["connecting", "connected", "connecting", "connected"],
            sockets: 2,
            client,
        });
    });

    it("discards a scheduled reconnect when the owner disposes the connection", async () => {
        const fixture = controlledHost();
        const client = fixture.connection.client;
        fixture.sockets[0]!.open();
        await untilPrepared(() => fixture.statuses.at(-1) === "connected");
        fixture.sockets[0]!.close();
        fixture.connection.dispose();
        fixture.advance(120_000);
        await expect(
            Promise.resolve(client.permissions.requestRemotePermission(permission)),
        ).rejects.toThrow("transport disposed");
        expect({ sockets: fixture.sockets.length, status: fixture.statuses.at(-1) }).toEqual({
            sockets: 1,
            status: "disconnected",
        });
    });
});
