import { describe, expect, it } from "bun:test";
import { okAsync } from "neverthrow";
import { createTransport } from "./client.js";
import { createClient } from "./generated/client.js";
import { createInternalClient } from "./generated/internal-client.js";
import * as T from "./generated/types.js";
import * as W from "./generated/wire-table.js";
import * as S from "./scale.js";
import {
    decodeWireMessage,
    encodeWireMessage,
    MESSAGE_TYPE_CANCEL,
    MESSAGE_TYPE_REQUEST,
    MESSAGE_TYPE_RESPONSE,
    type WireProvider,
} from "./transport.js";

const permission: T.RemotePermissionRequest = {
    permission: { tag: "Remote", value: { domains: ["api.example.com"] } },
};

function connection() {
    const sent: Uint8Array[] = [];
    let receive = (_frame: Uint8Array) => {};
    const provider: WireProvider = {
        postMessage(frame) {
            sent.push(frame);
        },
        subscribe(callback) {
            receive = callback;
            return () => {};
        },
        dispose() {},
    };
    const transport = createTransport(provider, { requestIdPrefix: "host:permission:" });
    return {
        transport,
        sent,
        respond(index: number, granted: boolean) {
            const request = decodeWireMessage(sent[index]!)._unsafeUnwrap();
            receive(
                encodeWireMessage({
                    requestId: request.requestId,
                    payload: {
                        ...request.payload,
                        messageType: MESSAGE_TYPE_RESPONSE,
                        value: S.Result(
                            T.VersionedRemotePermissionResponse,
                            S.CallError(T.VersionedRemotePermissionError),
                        ).enc({ success: true, value: { tag: "V1", value: { granted } } }),
                    },
                })._unsafeUnwrap(),
            );
        },
    };
}

describe("generated internal authorization", () => {
    it("shares normal SDK request handling with public calls without exposing internal methods", async () => {
        const host = connection();
        const product = createClient(host.transport);
        const internal = createInternalClient(host.transport);
        try {
            const grant = product.permissions.requestRemotePermission(permission);
            const authorize = internal.permissions.authorizeRemotePermission(permission);
            const frames = host.sent.map((frame) => decodeWireMessage(frame)._unsafeUnwrap());
            expect(
                frames.map((frame) => ({
                    id: frame.requestId,
                    method: frame.payload.methodId,
                    type: frame.payload.messageType,
                    request: T.VersionedRemotePermissionRequest.dec(frame.payload.value),
                })),
            ).toEqual([
                {
                    id: "host:permission:1",
                    method: W.PERMISSIONS_REQUEST_REMOTE_PERMISSION.method,
                    type: MESSAGE_TYPE_REQUEST,
                    request: { tag: "V1", value: permission },
                },
                {
                    id: "host:permission:2",
                    method: W.PERMISSIONS_AUTHORIZE_REMOTE_PERMISSION.method,
                    type: MESSAGE_TYPE_REQUEST,
                    request: { tag: "V1", value: permission },
                },
            ]);
            expect("authorizeRemotePermission" in product.permissions).toBe(false);
            host.respond(0, true);
            host.respond(1, false);
            expect([(await grant)._unsafeUnwrap(), (await authorize)._unsafeUnwrap()]).toEqual([
                { granted: true },
                { granted: false },
            ]);
        } finally {
            host.transport.dispose();
        }
    });

    it("keeps authorization independent of replaceable public methods", async () => {
        const host = connection();
        const product = createClient(host.transport);
        const internal = createInternalClient(host.transport);
        try {
            product.permissions.requestRemotePermission = () => okAsync({ granted: true });
            expect(
                (await product.permissions.requestRemotePermission(permission))._unsafeUnwrap(),
            ).toEqual({ granted: true });
            const result = internal.permissions.authorizeRemotePermission(permission);
            host.respond(0, false);
            expect((await result)._unsafeUnwrap()).toEqual({ granted: false });
        } finally {
            host.transport.dispose();
        }
    });

    it("protects generated internal entrypoints and hides their shared transport", () => {
        const host = connection();
        const product = createClient(host.transport);
        const internal = createInternalClient(host.transport);
        try {
            const replacement = () => okAsync({ granted: true });
            expect([
                Reflect.set(internal, "permissions", {}),
                Reflect.set(internal.permissions, "authorizeRemotePermission", replacement),
                Reflect.set(
                    Object.getPrototypeOf(internal.permissions),
                    "authorizeRemotePermission",
                    replacement,
                ),
                Reflect.set(internal.permissions, "transport", {}),
                Reflect.has(internal.permissions, "transport"),
                Reflect.has(product.permissions, "transport"),
            ]).toEqual([false, false, false, false, false, false]);
        } finally {
            host.transport.dispose();
        }
    });

    it("uses SDK cancellation for an abandoned authorization", async () => {
        const host = connection();
        const internal = createInternalClient(host.transport);
        const abort = new AbortController();
        try {
            const result = internal.permissions.authorizeRemotePermission(permission, {
                signal: abort.signal,
            });
            abort.abort();
            expect(
                host.sent.map((frame) => {
                    const message = decodeWireMessage(frame)._unsafeUnwrap();
                    return { id: message.requestId, type: message.payload.messageType };
                }),
            ).toEqual([
                { id: "host:permission:1", type: MESSAGE_TYPE_REQUEST },
                { id: "host:permission:1", type: MESSAGE_TYPE_CANCEL },
            ]);
            host.respond(0, false);
            expect((await result)._unsafeUnwrap()).toEqual({ granted: false });
        } finally {
            host.transport.dispose();
        }
    });
});
