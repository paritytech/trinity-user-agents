import type { Result } from "neverthrow";
import { describe, expect, it, jest } from "bun:test";

import { createTransport, RequestTimeoutError } from "./client.js";
import * as S from "./scale.js";
import { str, type CallErrorValue } from "./scale.js";
import {
  createClient,
  SubscriptionError,
  TRUAPI_CODEC_VERSION,
} from "./generated/client.js";
import * as T from "./generated/types.js";
import * as W from "./generated/wire-table.js";
import {
  encodeWireMessage,
  MESSAGE_TYPE_CANCEL,
  MESSAGE_TYPE_INTERRUPT,
  MESSAGE_TYPE_RECEIVE,
  MESSAGE_TYPE_REQUEST,
  MESSAGE_TYPE_RESPONSE,
  MESSAGE_TYPE_START,
  MESSAGE_TYPE_STOP,
  PROTOCOL_ERROR_METHOD_ID,
  PROTOCOL_ERROR_TRAIT_ID,
  UnsupportedMessageError,
} from "./transport.js";

function toHex(u: Uint8Array): string {
    return Array.from(u)
        .map((b) => b.toString(16).padStart(2, "0"))
        .join("");
}

/** Return the successful result value or fail the test with context. */
function unwrap<T>(result: Result<T, { message: string }>, message: string): T {
    return result.match(
        (value) => value,
        (error): never => {
            throw new Error(`${message}: ${error.message}`);
        },
    );
}

/** Create an in-memory provider plus helpers for injecting frames and closes. */
function providerFixture() {
    const sent: Uint8Array[] = [];
    let listener: (message: Uint8Array) => void = () => {};
    let closeListener: (error: Error) => void = () => {};
    let resetListener: (error: Error) => void = () => {};
    return {
        sent,
        provider: {
            postMessage(message: Uint8Array) {
                sent.push(message);
            },
            subscribe(callback: (message: Uint8Array) => void) {
                listener = callback;
                return () => {};
            },
            subscribeClose(callback: (error: Error) => void) {
                closeListener = callback;
                return () => {};
            },
            subscribeReset(callback: (error: Error) => void) {
                resetListener = callback;
                return () => {
                    resetListener = () => {};
                };
            },
            dispose() {},
        },
        receive(message: Uint8Array) {
            listener(message);
        },
        close(error: Error) {
            closeListener(error);
        },
        reset(error: Error) {
            resetListener(error);
        },
    };
}

/**
 * Encode one wire frame: `[requestId][trait][method][messageType][payload]`.
 * `ids` takes a generated `W.*` constant directly, or a literal pair for an
 * address this build does not know.
 */
function wireFrame(
    requestId: string,
    ids: { trait: number; method: number },
    messageType: number,
    value: Uint8Array = new Uint8Array(),
): Uint8Array {
    return unwrap(
        encodeWireMessage({
            requestId,
            payload: { traitId: ids.trait, methodId: ids.method, messageType, value },
        }),
        `encode (${ids.trait}, ${ids.method}) messageType ${messageType}`,
    );
}

/** Codec for `system_handshake`'s `Response`-leg payload. */
const HANDSHAKE_RESPONSE_CODEC = S.Result(
    T.VersionedHostHandshakeResponse,
    S.CallError(T.VersionedHostHandshakeError),
);

/** Encode a successful V1 host-handshake `Response`-leg payload. */
function handshakeResponsePayload(_value: { success: true; value: undefined }): Uint8Array {
    return HANDSHAKE_RESPONSE_CODEC.enc({ success: true, value: { tag: "V1" } });
}

/**
 * Encode a V1 `account_get_account` `Response`-leg payload. Both legs already
 * carry their own version tag (their own wrapper), so the wrapped shapes
 * passed in are exactly what the wire holds — no further translation.
 */
function accountGetResponsePayload(
    value:
        | {
              success: true;
              value: T.HostAccountGetResponse;
          }
        | {
              success: false;
              value: { tag: "Domain"; value: T.VersionedHostAccountGetError };
          },
): Uint8Array {
    return S.Result(
        T.VersionedHostAccountGetResponse,
        S.CallError(T.VersionedHostAccountGetError),
    ).enc(
        value.success
            ? { success: true, value: { tag: "V1", value: value.value } }
            : value,
    );
}

function rendererStart(
    requestId: string,
    request: T.ProductRendererRenderRequest,
): Uint8Array {
    return wireFrame(
        requestId,
        W.RENDERER_RENDER,
        MESSAGE_TYPE_START,
        T.VersionedProductRendererRenderRequest.enc({
            tag: "V1",
            value: request,
        }),
    );
}

function rendererReceive(requestId: string, node: T.RendererNode): Uint8Array {
    return wireFrame(
        requestId,
        W.RENDERER_RENDER,
        MESSAGE_TYPE_RECEIVE,
        T.VersionedProductRendererRenderItem.enc({
            tag: "V1",
            value: node,
        }),
    );
}

/**
 * The fixed frame `transport.ts` sends to decline a host-initiated render:
 * `Err(CallError::HostFailure{reason: "unavailable"})`, with `messageType =
 * Interrupt`. `HostFailure`'s payload doesn't depend on the method's own
 * domain error type, so one constant frame covers every method.
 */
function rendererInterrupt(requestId: string): Uint8Array {
    return wireFrame(
        requestId,
        W.RENDERER_RENDER,
        MESSAGE_TYPE_INTERRUPT,
        new Uint8Array([
            1, 4, 44, 117, 110, 97, 118, 97, 105, 108, 97, 98, 108, 101,
        ]),
    );
}

/** The interrupt frame a handler produces by ending its stream with `reason`. */
function rendererTypedInterrupt(
    requestId: string,
    reason: CallErrorValue<T.GenericError>,
): Uint8Array {
    return wireFrame(
        requestId,
        W.RENDERER_RENDER,
        MESSAGE_TYPE_INTERRUPT,
        S.Result(S._void, S.CallError(T.GenericError)).enc({
            success: false,
            value: reason,
        }),
    );
}

/** The interrupt frame a handler produces by ending its stream cleanly. */
function rendererCleanInterrupt(requestId: string): Uint8Array {
    return wireFrame(
        requestId,
        W.RENDERER_RENDER,
        MESSAGE_TYPE_INTERRUPT,
        S.Result(S._void, S.CallError(T.GenericError)).enc({
            success: true,
            value: undefined,
        }),
    );
}

function rendererStop(requestId: string): Uint8Array {
    return wireFrame(requestId, W.RENDERER_RENDER, MESSAGE_TYPE_STOP);
}

function protocolError(requestId: string, payload: Uint8Array): Uint8Array {
    return wireFrame(
        requestId,
        { trait: PROTOCOL_ERROR_TRAIT_ID, method: PROTOCOL_ERROR_METHOD_ID },
        MESSAGE_TYPE_RESPONSE,
        payload,
    );
}

function unsupportedMessage(
    requestId: string,
    traitId: number,
    methodId: number,
): Uint8Array {
    // [0] version index, [0] variant index, then the unsupported pair.
    return protocolError(requestId, new Uint8Array([0, 0, traitId, methodId]));
}

describe("generated client transport", () => {
    it("encodes unit-only enums as a single-byte SCALE discriminant", () => {
        // Unit-only enums expose a string union on the public API while
        // preserving the same single-byte SCALE discriminant encoding.
        expect(toHex(T.HostDevicePermissionRequest.enc("Camera"))).toBe("01");
        expect(T.HostDevicePermissionRequest.dec(new Uint8Array([1]))).toBe("Camera");
    });

    it("wraps generated method requests in the selected wire wrapper", () => {
        const fixture = providerFixture();
        const transport = createTransport(fixture.provider);
        const client = createClient(transport);

        const request = {
            productAccountId: {
                dotNsIdentifier: "foo",
                derivationIndex: { tag: "Index", value: 0 },
            },
        };
        void client.account.getAccount(request);

        const expectedPayload = T.VersionedHostAccountGetRequest.enc({
            tag: "V1",
            value: request,
        });
        const expectedFrame = new Uint8Array(str.enc("p:1").length + 3 + expectedPayload.length);
        expectedFrame.set(str.enc("p:1"), 0);
        expectedFrame[str.enc("p:1").length] = 2; // account trait
        expectedFrame[str.enc("p:1").length + 1] = 1; // get_account
        expectedFrame[str.enc("p:1").length + 2] = MESSAGE_TYPE_REQUEST;
        expectedFrame.set(expectedPayload, str.enc("p:1").length + 3);

        expect(toHex(fixture.sent[0])).toBe(toHex(expectedFrame));
    });

    it("uses the transport codec version for generated handshake calls", () => {
        const fixture = providerFixture();
        const transport = createTransport(fixture.provider);
        const client = createClient(transport);

        void client.system.handshake();

        const expectedPayload = T.VersionedHostHandshakeRequest.enc({
            tag: "V1",
            value: { codecVersion: TRUAPI_CODEC_VERSION },
        });
        const expectedFrame = new Uint8Array(str.enc("p:1").length + 3 + expectedPayload.length);
        expectedFrame.set(str.enc("p:1"), 0);
        expectedFrame[str.enc("p:1").length] = 1; // system trait
        expectedFrame[str.enc("p:1").length + 1] = 0; // handshake
        expectedFrame[str.enc("p:1").length + 2] = MESSAGE_TYPE_REQUEST;
        expectedFrame.set(expectedPayload, str.enc("p:1").length + 3);

        expect(toHex(fixture.sent[0])).toBe(toHex(expectedFrame));
    });

    it("resolves a request from its versioned response envelope", async () => {
        const fixture = providerFixture();
        const transport = createTransport(fixture.provider);
        const client = createClient(transport);

        const response = client.system.handshake();
        const frame = wireFrame(
            "p:1",
            W.SYSTEM_HANDSHAKE,
            MESSAGE_TYPE_RESPONSE,
            handshakeResponsePayload({ success: true, value: undefined }),
        );
        fixture.receive(frame);

        const result = await response;
        expect(result.isOk()).toBe(true);
    });

    it("returns the current product context without request arguments", async () => {
        const fixture = providerFixture();
        const client = createClient(createTransport(fixture.provider));

        const response = client.system.getProductContext();
        const frame = wireFrame(
            "p:1",
            W.SYSTEM_GET_PRODUCT_CONTEXT,
            MESSAGE_TYPE_RESPONSE,
            S.Result(
                T.VersionedHostGetProductContextResponse,
                S.CallError(T.VersionedHostGetProductContextError),
            ).enc({
                success: true,
                value: {
                    tag: "V1",
                    value: { productId: "truapi-playground.paseo" },
                },
            }),
        );
        fixture.receive(frame);

        expect((await response)._unsafeUnwrap()).toEqual({
            productId: "truapi-playground.paseo",
        });
    });

    it("decodes request domain errors from the versioned response envelope", async () => {
        const fixture = providerFixture();
        const transport = createTransport(fixture.provider);
        const client = createClient(transport);

        const response = client.account.getAccount({
            productAccountId: {
                dotNsIdentifier: "foo",
                derivationIndex: { tag: "Index", value: 0 },
            },
        });
        const reason = { tag: "V1", value: { tag: "NotConnected", value: undefined } } as const;
        const frame = wireFrame(
            "p:1",
            W.ACCOUNT_GET_ACCOUNT,
            MESSAGE_TYPE_RESPONSE,
            accountGetResponsePayload({
                success: false,
                value: { tag: "Domain", value: reason },
            }),
        );
        fixture.receive(frame);

        const result = await response;
        expect(result.isErr()).toBe(true);
        expect(result._unsafeUnwrapErr()).toEqual({ tag: "Domain", value: reason });
    });

    it("settles an unknown API request as unsupported from a correlated protocol error", async () => {
        const fixture = providerFixture();
        const transport = createTransport(fixture.provider);
        const response = transport.request<undefined, CallErrorValue<never>>({
            ids: { trait: 200, method: 194, kind: "request" },
            payload: new Uint8Array(),
            decodeResponse: () => {
                throw new Error("protocol errors must bypass the method response decoder");
            },
        });
        fixture.receive(unsupportedMessage("p:1", 200, 194));

        expect((await response)._unsafeUnwrapErr()).toEqual({ tag: "Unsupported" });

        const followup = transport.request<string, CallErrorValue<never>>({
            ids: W.LOCAL_STORAGE_READ,
            payload: new Uint8Array(),
            decodeResponse: () => ({ success: true, value: "still connected" }),
        });
        fixture.receive(wireFrame("p:2", W.LOCAL_STORAGE_READ, MESSAGE_TYPE_RESPONSE));

        expect((await followup)._unsafeUnwrap()).toBe("still connected");
        expect(fixture.sent).toHaveLength(2);
    });

    it("does not settle a request from an unmatched or mismatched protocol error", async () => {
        const fixture = providerFixture();
        const transport = createTransport(fixture.provider);
        const response = transport.request<string, CallErrorValue<never>>({
            ids: { trait: 200, method: 194, kind: "request" },
            payload: new Uint8Array(),
            decodeResponse: () => ({ success: true, value: "supported" }),
        });
        for (const [requestId, traitId, methodId] of [
            // right pair, wrong request id
            ["p:99", 200, 194],
            // right request id, wrong method
            ["p:1", 200, 196],
            // right request id and method but the WRONG TRAIT - under a
            // one-byte discriminant this was indistinguishable from a match
            ["p:1", 201, 194],
        ] as const) {
            fixture.receive(unsupportedMessage(requestId, traitId, methodId));
        }
        fixture.receive(wireFrame("p:1", { trait: 200, method: 194 }, MESSAGE_TYPE_RESPONSE));

        expect((await response)._unsafeUnwrap()).toBe("supported");
        expect(fixture.sent).toHaveLength(1);
    });

    it("reports an unsupported raw subscription only for its matching start", () => {
        const fixture = providerFixture();
        const transport = createTransport(fixture.provider);
        const errors: Error[] = [];
        const subscription = transport.subscribeRaw({
            ids: { trait: 7, method: 194, kind: "subscription" },
            payload: new Uint8Array(),
            onReceive: () => {},
            onClose: (error) => errors.push(error),
        });
        // Right trait, wrong method: an error about a different method must
        // not end this subscription.
        fixture.receive(unsupportedMessage(subscription.subscriptionId, 7, 195));
        // Right METHOD, wrong trait. Under a one-byte discriminant these two
        // were indistinguishable; the pair is the whole point, so a trait-8
        // error about method 194 must be ignored here.
        fixture.receive(unsupportedMessage(subscription.subscriptionId, 8, 194));
        // Our actual start pair: this one ends it.
        fixture.receive(unsupportedMessage(subscription.subscriptionId, 7, 194));
        subscription.unsubscribe();

        expect(errors).toHaveLength(1);
        expect(errors[0]).toBeInstanceOf(UnsupportedMessageError);
        const unsupported = errors[0] as UnsupportedMessageError;
        expect({
            name: unsupported.name,
            message: unsupported.message,
            traitId: unsupported.traitId,
            methodId: unsupported.methodId,
        }).toEqual({
            name: "UnsupportedMessageError",
            message: "Peer does not support wire message (7, 194)",
            traitId: 7,
            methodId: 194,
        });
        expect(fixture.sent).toHaveLength(1);
    });

    it("terminates a generated subscription when its API is unsupported", () => {
        const fixture = providerFixture();
        const client = createClient(createTransport(fixture.provider));
        const errors: SubscriptionError[] = [];
        const subscription = client.account
            .connectionStatusSubscribe()
            .subscribe({ error: (error) => errors.push(error) });

        fixture.receive(
            unsupportedMessage(
                subscription.subscriptionId,
                W.ACCOUNT_CONNECTION_STATUS_SUBSCRIBE.trait,
                W.ACCOUNT_CONNECTION_STATUS_SUBSCRIBE.method,
            ),
        );

        expect(errors).toHaveLength(1);
        expect(errors[0]).toBeInstanceOf(SubscriptionError);
        expect(errors[0].reason).toBeUndefined();
        expect(errors[0].cause).toBeInstanceOf(UnsupportedMessageError);
        const cause = errors[0].cause as UnsupportedMessageError;
        expect([cause.traitId, cause.methodId]).toEqual([
            W.ACCOUNT_CONNECTION_STATUS_SUBSCRIBE.trait,
            W.ACCOUNT_CONNECTION_STATUS_SUBSCRIBE.method,
        ]);
        expect(fixture.sent).toHaveLength(1);
    });

    it("closes the transport for every malformed protocol error shape", async () => {
        // Re-derived for the two-byte address: a valid payload is 4 bytes, so
        // the old trailing-byte fixture `[0, 0, 194, 0]` decodes cleanly as
        // the pair (194, 0) and would have silently stopped testing anything.
        //
        // An unknown version or variant index is deliberately absent: that is
        // a newer peer rather than corruption, and it must NOT close the
        // transport. See "settles one call without closing the transport on an
        // unknown protocol error".
        const malformedPayloads = [
            [new Uint8Array([]), "empty"],
            [new Uint8Array([0, 0]), "expected 4 bytes, received 2"],
            // trait present, method truncated
            [new Uint8Array([0, 0, 194]), "expected 4 bytes, received 3"],
            // one trailing byte past a full pair
            [new Uint8Array([0, 0, 194, 193, 0]), "expected 4 bytes, received 5"],
        ] as const;

        for (const [payload, message] of malformedPayloads) {
            const fixture = providerFixture();
            const transport = createTransport(fixture.provider);
            const response = transport.request<undefined, CallErrorValue<never>>({
                ids: { trait: 200, method: 194, kind: "request" },
                payload: new Uint8Array(),
                decodeResponse: () => ({ success: true, value: undefined }),
            });
            const outcome = Promise.resolve(response);
            fixture.receive(protocolError("p:1", payload));

            await expect(outcome).rejects.toThrow(`Malformed protocol error payload: ${message}`);
            expect(fixture.sent).toHaveLength(1);
        }
    });

    it("settles one call without closing the transport on an unknown protocol error", async () => {
        // `(255, 255)` is the one address every peer answers on, so rejecting a
        // payload this build cannot read would mean a newer peer could never
        // report anything new without killing the connection, freezing the
        // channel at whatever shape shipped first. An unknown variant settles
        // the correlated call and leaves the transport usable.
        const fixture = providerFixture();
        const transport = createTransport(fixture.provider);

        const first = transport.request<undefined, CallErrorValue<never>>({
            ids: { trait: 200, method: 194, kind: "request" },
            payload: new Uint8Array(),
            decodeResponse: () => ({ success: true, value: undefined }),
        });
        // Variant 7 with a payload whose length this build cannot know.
        fixture.receive(protocolError("p:1", new Uint8Array([7, 0xaa, 0xbb])));

        expect((await first)._unsafeUnwrapErr()).toEqual({ tag: "Unsupported" });

        // The transport is still alive: a second call goes out and completes.
        const second = transport.request<undefined, CallErrorValue<never>>({
            ids: W.LOCAL_STORAGE_READ,
            payload: new Uint8Array(),
            decodeResponse: () => ({ success: true, value: undefined }),
        });
        expect(fixture.sent).toHaveLength(2);
        fixture.receive(wireFrame("p:2", W.LOCAL_STORAGE_READ, MESSAGE_TYPE_RESPONSE));
        expect((await second)._unsafeUnwrap()).toBeUndefined();
    });

    it("rejects an unknown host-initiated message without starting an error loop", () => {
        const fixture = providerFixture();
        createTransport(fixture.provider);
        const incoming = wireFrame("h:future", { trait: 200, method: 194 }, MESSAGE_TYPE_REQUEST);

        fixture.receive(incoming);

        expect(fixture.sent.map(toHex)).toEqual([toHex(unsupportedMessage("h:future", 200, 194))]);

        fixture.receive(unsupportedMessage("h:future", 200, 194));
        expect(fixture.sent).toHaveLength(1);
    });

    it("rejects a known host-initiated start when no handler is registered", () => {
        const fixture = providerFixture();
        createTransport(fixture.provider);
        fixture.receive(
            // No handler is ever registered in this test (no client is created),
            // so this never reaches a typed decode of the rest.
            wireFrame("h:known", W.RENDERER_RENDER, MESSAGE_TYPE_START),
        );

        expect(fixture.sent.map(toHex)).toEqual([
            toHex(
                unsupportedMessage(
                    "h:known",
                    W.RENDERER_RENDER.trait,
                    W.RENDERER_RENDER.method,
                ),
            ),
        ]);
    });

    it("rejects an unknown message without disturbing its correlated request", async () => {
        const fixture = providerFixture();
        const transport = createTransport(fixture.provider);
        const response = transport.request<string, CallErrorValue<never>>({
            ids: W.LOCAL_STORAGE_READ,
            payload: new Uint8Array(),
            decodeResponse: () => ({ success: true, value: "supported" }),
        });
        fixture.receive(wireFrame("p:1", { trait: 200, method: 194 }, MESSAGE_TYPE_REQUEST));

        expect(fixture.sent.map(toHex)).toEqual([
            toHex(fixture.sent[0]),
            toHex(unsupportedMessage("p:1", 200, 194)),
        ]);

        fixture.receive(wireFrame("p:1", W.LOCAL_STORAGE_READ, MESSAGE_TYPE_RESPONSE));
        expect((await response)._unsafeUnwrap()).toBe("supported");
    });

    it("ignores stale frames whose discriminants are known locally", async () => {
        const requestFixture = providerFixture();
        const requestTransport = createTransport(requestFixture.provider);
        const response = requestTransport.request<string, CallErrorValue<never>>({
            ids: W.LOCAL_STORAGE_READ,
            payload: new Uint8Array(),
            decodeResponse: () => ({ success: true, value: "done" }),
        });
        const responseFrame = wireFrame("p:1", W.LOCAL_STORAGE_READ, MESSAGE_TYPE_RESPONSE);
        requestFixture.receive(responseFrame);
        expect((await response)._unsafeUnwrap()).toBe("done");
        requestFixture.receive(responseFrame);
        expect(requestFixture.sent).toHaveLength(1);

        const subscriptionFixture = providerFixture();
        const subscriptionTransport = createTransport(subscriptionFixture.provider);
        const received: Uint8Array[] = [];
        const subscription = subscriptionTransport.subscribeRaw({
            ids: W.ACCOUNT_CONNECTION_STATUS_SUBSCRIBE,
            payload: new Uint8Array(),
            onReceive: (payload) => received.push(payload),
        });
        subscription.unsubscribe();
        // Receive and Interrupt now share one address; both are distinguished
        // by `messageType`, not anything inside `value` — and since the
        // subscription is already gone, `value` is never decoded at all.
        for (const messageType of [MESSAGE_TYPE_RECEIVE, MESSAGE_TYPE_INTERRUPT]) {
            subscriptionFixture.receive(
                wireFrame(
                    subscription.subscriptionId,
                    W.ACCOUNT_CONNECTION_STATUS_SUBSCRIBE,
                    messageType,
                ),
            );
        }
        expect(received).toEqual([]);
        expect(subscriptionFixture.sent).toHaveLength(2);
    });

    it("logs a protocol violation for a known pair's out-of-range message type", () => {
        const fixture = providerFixture();
        createTransport(fixture.provider);

        const warnings: unknown[][] = [];
        const originalWarn = console.warn;
        console.warn = (...args: unknown[]) => {
            warnings.push(args);
        };
        try {
            fixture.receive(wireFrame("unrelated:1", W.LOCAL_STORAGE_READ, 99));
        } finally {
            console.warn = originalWarn;
        }

        expect(fixture.sent).toHaveLength(0);
        expect(
            warnings.some((args) =>
                String(args[0]).includes("unexpected messageType 99"),
            ),
        ).toBe(true);
    });

    it("auto-responds to an inbound handshake with the versioned-result shape", () => {
        const fixture = providerFixture();
        createTransport(fixture.provider);

        const requestPayload = T.VersionedHostHandshakeRequest.enc({
            tag: "V1",
            value: { codecVersion: TRUAPI_CODEC_VERSION },
        });
        const requestFrame = wireFrame(
            "h:1",
            W.SYSTEM_HANDSHAKE,
            MESSAGE_TYPE_REQUEST,
            requestPayload,
        );
        fixture.receive(requestFrame);

        const expectedFrame = wireFrame(
            "h:1",
            W.SYSTEM_HANDSHAKE,
            MESSAGE_TYPE_RESPONSE,
            handshakeResponsePayload({ success: true, value: undefined }),
        );
        expect(toHex(fixture.sent[0])).toBe(toHex(expectedFrame));
    });

    it("rejects an unanswered request at the configured deadline", async () => {
        // Ported from #665 onto the two-byte address: the error carries the
        // pair rather than a single discriminant.
        jest.useFakeTimers();
        try {
            const fixture = providerFixture();
            const transport = createTransport(fixture.provider, {
                requestTimeoutMs: 25,
            });
            const response = transport.request<undefined, CallErrorValue<never>>({
                ids: { trait: 200, method: 194, kind: "request" },
                payload: new Uint8Array(),
                decodeResponse: () => ({ success: true, value: undefined }),
            });
            const outcome = Promise.resolve(response);
            jest.advanceTimersByTime(26);

            await expect(outcome).rejects.toBeInstanceOf(RequestTimeoutError);
            await outcome.catch((error: RequestTimeoutError) => {
                expect({
                    requestId: error.requestId,
                    traitId: error.traitId,
                    methodId: error.methodId,
                    timeoutMs: error.timeoutMs,
                }).toEqual({
                    requestId: "p:1",
                    traitId: 200,
                    methodId: 194,
                    timeoutMs: 25,
                });
            });
        } finally {
            jest.useRealTimers();
        }
    });

    it("sends a cancel frame when an in-flight call is aborted", async () => {
        const fixture = providerFixture();
        const transport = createTransport(fixture.provider);
        const controller = new AbortController();

        const response = transport.request<undefined, CallErrorValue<never>>({
            ids: { trait: 200, method: 194, kind: "request" },
            payload: new Uint8Array(),
            decodeResponse: () => ({
                success: false,
                value: { tag: "Cancelled" },
            }),
            signal: controller.signal,
        });
        controller.abort();

        expect(fixture.sent).toHaveLength(2);
        expect(toHex(fixture.sent[1]!)).toBe(
            toHex(
                wireFrame(
                    "p:1",
                    { trait: 200, method: 194 },
                    MESSAGE_TYPE_CANCEL,
                ),
            ),
        );

        // The call is still pending: aborting asks, the response answers.
        fixture.receive(
            wireFrame("p:1", { trait: 200, method: 194 }, MESSAGE_TYPE_RESPONSE),
        );
        const outcome = await response;
        expect(outcome.isErr() && outcome.error).toEqual({ tag: "Cancelled" });
    });

    it("decodes a withdrawn call's response as Cancelled", async () => {
        // End to end through a real generated method, against the exact bytes
        // `encode_cancelled_response` puts on the wire: `Err` (1) then
        // `CallError::Cancelled` (5). The payload names neither of the
        // method's own types, which is what lets one host-side encoder answer
        // any method.
        const fixture = providerFixture();
        const transport = createTransport(fixture.provider);
        const client = createClient(transport);
        const controller = new AbortController();

        const pending = client.account.getAccount(
            {
                productAccountId: {
                    dotNsIdentifier: "foo",
                    derivationIndex: { tag: "Index", value: 0 },
                },
            },
            { signal: controller.signal },
        );
        controller.abort();

        expect(fixture.sent).toHaveLength(2);
        expect(toHex(fixture.sent[1]!)).toBe(
            toHex(wireFrame("p:1", W.ACCOUNT_GET_ACCOUNT, MESSAGE_TYPE_CANCEL)),
        );

        fixture.receive(
            wireFrame(
                "p:1",
                W.ACCOUNT_GET_ACCOUNT,
                MESSAGE_TYPE_RESPONSE,
                new Uint8Array([1, 5]),
            ),
        );

        // Raced against a macrotask rather than awaited outright: a client
        // that stops settling on the response should redden here, not time the
        // whole suite out two minutes later.
        const STILL_PENDING = Symbol("still pending");
        const outcome = await Promise.race([
            Promise.resolve(pending),
            new Promise((resolve) => setTimeout(() => resolve(STILL_PENDING), 0)),
        ]);
        expect(outcome).not.toBe(STILL_PENDING);
        expect(
            outcome instanceof Object && "isErr" in outcome && outcome.isErr()
                ? outcome.error
                : outcome,
        ).toEqual({ tag: "Cancelled" });
    });

    it("tells the host before it gives up on its own deadline", async () => {
        jest.useFakeTimers();
        try {
            const fixture = providerFixture();
            const transport = createTransport(fixture.provider, {
                requestTimeoutMs: 25,
            });
            const response = transport.request<undefined, CallErrorValue<never>>({
                ids: { trait: 200, method: 194, kind: "request" },
                payload: new Uint8Array(),
                decodeResponse: () => ({ success: true, value: undefined }),
            });
            const outcome = Promise.resolve(response);
            jest.advanceTimersByTime(26);
            await expect(outcome).rejects.toBeInstanceOf(RequestTimeoutError);

            expect(fixture.sent).toHaveLength(2);
            expect(fixture.sent[1]![str.enc("p:1").length + 2]).toBe(
                MESSAGE_TYPE_CANCEL,
            );
        } finally {
            jest.useRealTimers();
        }
    });

    it("never sends a request its signal has already withdrawn", async () => {
        const fixture = providerFixture();
        const transport = createTransport(fixture.provider);

        const response = transport.request<undefined, CallErrorValue<never>>({
            ids: { trait: 200, method: 194, kind: "request" },
            payload: new Uint8Array(),
            decodeResponse: () => ({ success: true, value: undefined }),
            signal: AbortSignal.abort(),
        });

        await expect(Promise.resolve(response)).rejects.toBeDefined();
        expect(fixture.sent).toHaveLength(0);
    });

    it("refuses a non-positive request deadline", () => {
        const fixture = providerFixture();
        expect(() =>
            createTransport(fixture.provider, { requestTimeoutMs: 0 }),
        ).toThrow("requestTimeoutMs must be a positive finite number");
    });

    it("rejects the handshake call when the host never answers", async () => {
        // The handshake is the one call with a deadline, because a codec
        // mismatch means no answer ever arrives and the call that exists to
        // detect the mismatch must not hang on it. Framework failures surface
        // as a rejection rather than an `Err`, the same as a closed transport
        // or a malformed control frame.
        jest.useFakeTimers();
        try {
            const fixture = providerFixture();
            const client = createClient(createTransport(fixture.provider));
            const outcome = Promise.resolve(client.system.handshake());
            jest.advanceTimersByTime(10_001);
            await expect(outcome).rejects.toThrow(
                "TrUAPI handshake timed out after 10000ms",
            );
        } finally {
            jest.useRealTimers();
        }
    });

    it("answers a codec 1 handshake ping with a protocol error and stays usable", () => {
        const fixture = providerFixture();
        const transport = createTransport(fixture.provider);
        const client = createClient(transport);

        // A codec 1 host frames its ping as [requestId][u8 id=0][V1][codec=1].
        // Read against the three-byte header (trait, method, messageType)
        // this client now expects, that's trait=0, method=0, messageType=1,
        // with an EMPTY payload -- trait 0 is unassigned either way (real
        // traits start at 1), so this is an ordinary unknown pair, answered
        // with a protocol error like any other, rather than silently dropped
        // on a guess about the sender's codec version.
        const legacyFrame = new Uint8Array([
            ...str.enc("h:1"),
            0x00, // old flat discriminant, read as the trait byte
            0x00, // old V1 tag, read as the method byte
            0x01, // old codecVersion, read as the messageType byte
        ]);
        fixture.receive(legacyFrame);

        expect(fixture.sent.length).toBe(1);
        expect(toHex(fixture.sent[0])).toBe(toHex(unsupportedMessage("h:1", 0, 0)));

        // The transport must survive: a ping it cannot parse is a peer
        // problem, not grounds for tearing down every pending call.
        void client.account.getAccount({
            productAccountId: {
                dotNsIdentifier: "foo",
                derivationIndex: { tag: "Index", value: 0 },
            },
        });
        expect(fixture.sent.length).toBe(2);
    });

    it("ignores a frame whose message type is not a response on a pending call", () => {
        // Every leg of a method shares one address, so id plus address cannot
        // establish that a frame is the answer. Without the leg check a
        // `Request`, `Interrupt`, `Stop` or an out-of-range byte settles the
        // call from the wrong bytes before the real response arrives.
        //
        // `decodeResponse` runs synchronously inside the pending entry's
        // resolve, so counting its calls is what proves the frame was refused;
        // racing the returned promise does not, because its `.then` lands a
        // microtask later either way.
        for (const messageType of [
            MESSAGE_TYPE_REQUEST,
            MESSAGE_TYPE_INTERRUPT,
            MESSAGE_TYPE_STOP,
            255,
        ]) {
            const fixture = providerFixture();
            const transport = createTransport(fixture.provider);
            let decodeCalls = 0;
            void transport.request<string, CallErrorValue<never>>({
                ids: W.LOCAL_STORAGE_READ,
                payload: new Uint8Array(),
                decodeResponse: () => {
                    decodeCalls += 1;
                    return { success: true, value: "answered" };
                },
            });

            fixture.receive(wireFrame("p:1", W.LOCAL_STORAGE_READ, messageType));
            expect(decodeCalls).toBe(0);

            // The pending entry survived, so the real response still lands.
            fixture.receive(wireFrame("p:1", W.LOCAL_STORAGE_READ, MESSAGE_TYPE_RESPONSE));
            expect(decodeCalls).toBe(1);
        }
    });

    it("ignores a response whose trait does not match the pending request", async () => {
        const fixture = providerFixture();
        const transport = createTransport(fixture.provider);
        const client = createClient(transport);

        const response = client.account.getAccount({
            productAccountId: {
                dotNsIdentifier: "foo",
                derivationIndex: { tag: "Index", value: 0 },
            },
        });

        // Right request id, right method id, neighbouring trait: what a whole
        // trait of discriminant skew looks like from the product side.
        const skewed = wireFrame(
            "p:1",
            { trait: W.ACCOUNT_GET_ACCOUNT.trait + 1, method: W.ACCOUNT_GET_ACCOUNT.method },
            MESSAGE_TYPE_RESPONSE,
            accountGetResponsePayload({
                success: false,
                value: {
                    tag: "Domain",
                    value: { tag: "V1", value: { tag: "NotConnected", value: undefined } },
                },
            }),
        );
        fixture.receive(skewed);

        // The frame is refused rather than mistaken for the real response.
        const settled = await Promise.race([
            response.then(() => "settled" as const),
            Promise.resolve().then(() => "pending" as const),
        ]);
        expect(settled).toBe("pending");
    });

    it("decodes receive frames as wire wrappers and delivers inner values", () => {
        const fixture = providerFixture();
        const transport = createTransport(fixture.provider);
        const client = createClient(transport);
        const events: unknown[] = [];

        const sub = client.account
            .connectionStatusSubscribe()
            .subscribe({ next: (value) => events.push(value) });

        const frame = wireFrame(
            sub.subscriptionId,
            W.ACCOUNT_CONNECTION_STATUS_SUBSCRIBE,
            MESSAGE_TYPE_RECEIVE,
            T.VersionedHostAccountConnectionStatusSubscribeItem.enc({
                tag: "V1",
                value: "Connected",
            }),
        );
        fixture.receive(frame);

        expect(events).toEqual(["Connected"]);
    });

    it("buffers a host render start until the product registers its handler", () => {
        const fixture = providerFixture();
        const client = createClient(createTransport(fixture.provider));
        const request: T.ProductRendererRenderRequest = {
            context: {
                tag: "ChatMessage",
                value: { roomId: "room", messageId: "message-1", messageType: "vote" },
            },
            payload: "0x0102",
        };
        // Legacy hosts use opaque ids rather than the Rust host's `h:` prefix.
        fixture.receive(rendererStart("legacy-render-1", request));

        const handled: T.ProductRendererRenderRequest[] = [];
        client.renderer.onRender((value) => {
            handled.push(value);
        });

        expect(handled).toEqual([request]);
        expect(fixture.sent).toHaveLength(0);
    });

    it("streams complete replacement trees on the host-owned request id", () => {
        const fixture = providerFixture();
        const client = createClient(createTransport(fixture.provider));
        let send: ((node: T.RendererNode) => void) | undefined;
        client.renderer.onRender((_request, sendItem) => {
            send = sendItem;
        });

        fixture.receive(
            rendererStart("h:7", {
                context: {
                    tag: "ChatMessage",
                    value: { roomId: "room", messageId: "message-7", messageType: "vote" },
                },
                payload: "0x",
            }),
        );
        const first = { tag: "String", value: { text: "Votes: 1" } } as const;
        const second = { tag: "String", value: { text: "Votes: 2" } } as const;
        send?.(first);
        send?.(second);

        expect(fixture.sent.map(toHex)).toEqual(
            [rendererReceive("h:7", first), rendererReceive("h:7", second)].map(toHex),
        );
    });

    it("declines a render when the handler throws", () => {
        const fixture = providerFixture();
        const client = createClient(createTransport(fixture.provider));
        client.renderer.onRender(() => {
            throw new Error("unsupported renderer");
        });

        fixture.receive(
            rendererStart("h:2", {
                context: {
                    tag: "ChatMessage",
                    value: { roomId: "room", messageId: "message-2", messageType: "unknown" },
                },
                payload: "0x",
            }),
        );

        expect(toHex(fixture.sent[0])).toBe(toHex(rendererInterrupt("h:2")));
    });

    it("ends a render with the interrupt value its handler supplies", () => {
        const fixture = providerFixture();
        const client = createClient(createTransport(fixture.provider));
        client.renderer.onRender((_request, _send, interrupt) => {
            interrupt({ tag: "HostFailure", value: { reason: "renderer failed" } });
        });

        fixture.receive(
            rendererStart("h:3", {
                context: {
                    tag: "ChatMessage",
                    value: { roomId: "room", messageId: "message-3", messageType: "vote" },
                },
                payload: "0x",
            }),
        );

        expect(toHex(fixture.sent[0])).toBe(
            toHex(
                rendererTypedInterrupt("h:3", {
                    tag: "HostFailure",
                    value: { reason: "renderer failed" },
                }),
            ),
        );
    });

    it("ends the host's stream when its handler interrupts with no reason", () => {
        const fixture = providerFixture();
        const client = createClient(createTransport(fixture.provider));
        let disposed = false;
        client.renderer.onRender((_request, _send, interrupt) => {
            interrupt();
            return () => (disposed = true);
        });

        fixture.receive(
            rendererStart("h:4", {
                context: {
                    tag: "ChatMessage",
                    value: { roomId: "room", messageId: "message-4", messageType: "vote" },
                },
                payload: "0x",
            }),
        );

        expect(toHex(fixture.sent[0])).toBe(toHex(rendererCleanInterrupt("h:4")));
        expect(disposed).toBe(true);
    });

    it("interrupts the oldest buffered render when capacity is exceeded", () => {
        const fixture = providerFixture();
        createClient(createTransport(fixture.provider));
        for (let index = 1; index <= 65; index += 1) {
            fixture.receive(
                rendererStart(`h:${index}`, {
                    context: {
                        tag: "ChatMessage",
                        value: { roomId: "room", messageId: `message-${index}`, messageType: "vote" },
                    },
                    payload: "0x",
                }),
            );
        }

        expect(fixture.sent).toHaveLength(1);
        expect(toHex(fixture.sent[0])).toBe(toHex(rendererInterrupt("h:1")));
    });

    it("disposes only the stopped render instance", () => {
        const fixture = providerFixture();
        const client = createClient(createTransport(fixture.provider));
        const disposed: string[] = [];
        client.renderer.onRender((request) => () => {
            if (request.context.tag === "ChatMessage") {
                disposed.push(request.context.value.messageId);
            }
        });
        fixture.receive(
            rendererStart("h:1", {
                context: {
                    tag: "ChatMessage",
                    value: { roomId: "room", messageId: "one", messageType: "vote" },
                },
                payload: "0x",
            }),
        );
        fixture.receive(
            rendererStart("h:2", {
                context: {
                    tag: "ChatMessage",
                    value: { roomId: "room", messageId: "two", messageType: "vote" },
                },
                payload: "0x",
            }),
        );

        fixture.receive(rendererStop("h:1"));
        expect(disposed).toEqual(["one"]);
    });

    it("completes the observable on a clean interrupt terminator", () => {
        const fixture = providerFixture();
        const transport = createTransport(fixture.provider);
        const client = createClient(transport);
        const completions: unknown[][] = [];

        const sub = client.account
            .connectionStatusSubscribe()
            .subscribe({ complete: (...args) => completions.push(args) });

        const frame = wireFrame(
            sub.subscriptionId,
            W.ACCOUNT_CONNECTION_STATUS_SUBSCRIBE,
            MESSAGE_TYPE_INTERRUPT,
            S.Result(S._void, S.CallError(T.GenericError)).enc({
                success: true,
                value: undefined,
            }),
        );
        fixture.receive(frame);

        expect(completions).toEqual([[]]);
    });

    it("ends a running stream on an interrupt that arrives after its items", () => {
        // A live subscription fails like any other stage: the items already
        // delivered stand, and the interrupt that follows carries the reason.
        const fixture = providerFixture();
        const transport = createTransport(fixture.provider);
        const client = createClient(transport);
        const events: unknown[] = [];
        const errors: Error[] = [];
        const completions: unknown[][] = [];

        const sub = client.account.connectionStatusSubscribe().subscribe({
            next: (value) => events.push(value),
            error: (error) => errors.push(error),
            complete: (...args) => completions.push(args),
        });

        fixture.receive(
            wireFrame(
                sub.subscriptionId,
                W.ACCOUNT_CONNECTION_STATUS_SUBSCRIBE,
                MESSAGE_TYPE_RECEIVE,
                T.VersionedHostAccountConnectionStatusSubscribeItem.enc({
                    tag: "V1",
                    value: "Connected",
                }),
            ),
        );

        const reason: CallErrorValue<T.GenericError> = {
            tag: "HostFailure",
            value: { reason: "platform stream failed" },
        };
        fixture.receive(
            wireFrame(
                sub.subscriptionId,
                W.ACCOUNT_CONNECTION_STATUS_SUBSCRIBE,
                MESSAGE_TYPE_INTERRUPT,
                S.Result(S._void, S.CallError(T.GenericError)).enc({
                    success: false,
                    value: reason,
                }),
            ),
        );

        expect(events).toEqual(["Connected"]);
        expect(completions).toEqual([]);
        expect(errors).toHaveLength(1);
        expect((errors[0] as SubscriptionError).reason).toEqual(reason);
    });

    it("surfaces a framework interrupt as an observable error on a plain subscription", () => {
        // `chat.listSubscribe` has no domain error of its own (unlike
        // `payment.balanceSubscribe` below): its Interrupt carries a bare
        // `CallErrorValue<GenericError>`. A worker-only subscription like this
        // one denied to an app connection, or a malformed start frame, both
        // arrive this way and must surface as a real error, not a silent
        // `.complete()`.
        const fixture = providerFixture();
        const transport = createTransport(fixture.provider);
        const client = createClient(transport);
        const completions: unknown[][] = [];
        const errors: Error[] = [];

        const sub = client.chat.listSubscribe().subscribe({
            complete: (...args) => completions.push(args),
            error: (error) => errors.push(error),
        });

        const callError: CallErrorValue<never> = { tag: "Denied" };
        const frame = wireFrame(
            sub.subscriptionId,
            W.CHAT_LIST_SUBSCRIBE,
            MESSAGE_TYPE_INTERRUPT,
            S.Result(S._void, S.CallError(T.GenericError)).enc({
                success: false,
                value: callError,
            }),
        );
        fixture.receive(frame);

        expect(completions).toEqual([]);
        expect(errors).toHaveLength(1);
        expect(errors[0]).toBeInstanceOf(SubscriptionError);
        expect((errors[0] as SubscriptionError).reason).toEqual(callError);
    });

    it("surfaces a typed payment interrupt as an observable error", () => {
        const fixture = providerFixture();
        const transport = createTransport(fixture.provider);
        const client = createClient(transport);
        const completions: boolean[] = [];
        const errors: Error[] = [];

        const sub = client.payment.balanceSubscribe({ request: {} }).subscribe({
            complete: () => completions.push(true),
            error: (error) => errors.push(error),
        });

        const reason = { tag: "PermissionDenied", value: undefined } as const;
        const callError = {
            tag: "Domain",
            value: { tag: "V1", value: reason },
        } as const;
        const frame = wireFrame(
            sub.subscriptionId,
            W.PAYMENT_BALANCE_SUBSCRIBE,
            MESSAGE_TYPE_INTERRUPT,
            S.Option(
                S.CallError(T.VersionedHostPaymentBalanceSubscribeError),
            ).enc(callError),
        );
        fixture.receive(frame);

        expect(completions).toEqual([]);
        expect(errors).toHaveLength(1);
        expect(errors[0]).toBeInstanceOf(SubscriptionError);
        expect((errors[0] as SubscriptionError).reason).toEqual(callError);
        expect(fixture.sent).toHaveLength(1);
    });

    it("uses the same typed-interrupt envelope for RFC0017 coin-payment streams", () => {
        const fixture = providerFixture();
        const transport = createTransport(fixture.provider);
        const client = createClient(transport);
        const errors: Error[] = [];

        const sub = client.coinPayment
            .rebalancePurse({ request: { from: 1, to: 2, amount: 1000 } })
            .subscribe({ error: (error) => errors.push(error) });

        const reason = "Denied";
        const callError = {
            tag: "Domain",
            value: { tag: "V1", value: reason },
        } as const;
        const frame = wireFrame(
            sub.subscriptionId,
            W.COIN_PAYMENT_REBALANCE_PURSE,
            MESSAGE_TYPE_INTERRUPT,
            S.Option(
                S.CallError(T.VersionedHostCoinPaymentRebalancePurseError),
            ).enc(callError),
        );
        fixture.receive(frame);

        expect(errors).toHaveLength(1);
        expect(errors[0]).toBeInstanceOf(SubscriptionError);
        expect((errors[0] as SubscriptionError).reason).toEqual(callError);
    });

    it("treats a malformed receive payload as terminal and sends _stop", () => {
        // After the error, the generated wrapper sends `_stop` and ignores later
        // receive frames for that subscription.
        const fixture = providerFixture();
        const transport = createTransport(fixture.provider);
        const client = createClient(transport);
        const events: unknown[] = [];
        const errors: Error[] = [];

        const sub = client.account.connectionStatusSubscribe().subscribe({
            next: (value) => events.push(value),
            error: (error) => errors.push(error),
        });

        // An out-of-range item wrapper discriminant, so decoding fails immediately.
        const malformedFrame = wireFrame(
            sub.subscriptionId,
            W.ACCOUNT_CONNECTION_STATUS_SUBSCRIBE,
            MESSAGE_TYPE_RECEIVE,
            new Uint8Array([0xff]),
        );
        fixture.receive(malformedFrame);

        expect(events).toEqual([]);
        expect(errors).toHaveLength(1);
        expect(errors[0]).toBeInstanceOf(SubscriptionError);
        expect((errors[0] as SubscriptionError).reason).toBeUndefined();
        expect(fixture.sent).toHaveLength(2);

        const expectedStop = wireFrame(
            sub.subscriptionId,
            W.ACCOUNT_CONNECTION_STATUS_SUBSCRIBE,
            MESSAGE_TYPE_STOP,
        );
        expect(toHex(fixture.sent[1])).toBe(toHex(expectedStop));

        const validFrame = wireFrame(
            sub.subscriptionId,
            W.ACCOUNT_CONNECTION_STATUS_SUBSCRIBE,
            MESSAGE_TYPE_RECEIVE,
            T.VersionedHostAccountConnectionStatusSubscribeItem.enc({
                tag: "V1",
                value: "Connected",
            }),
        );
        fixture.receive(validFrame);

        expect(events).toEqual([]);
    });

    it("sends _stop on unsubscribe without invoking terminal callbacks locally", () => {
        const fixture = providerFixture();
        const transport = createTransport(fixture.provider);
        const client = createClient(transport);
        const completions: boolean[] = [];
        const errors: Error[] = [];

        const sub = client.account.connectionStatusSubscribe().subscribe({
            complete: () => completions.push(true),
            error: (error) => errors.push(error),
        });
        sub.unsubscribe();

        const expectedStop = wireFrame(
            sub.subscriptionId,
            W.ACCOUNT_CONNECTION_STATUS_SUBSCRIBE,
            MESSAGE_TYPE_STOP,
        );
        expect(toHex(fixture.sent[1])).toBe(toHex(expectedStop));
        expect(completions).toEqual([]);
        expect(errors).toEqual([]);
    });

    it("propagates a provider close/error as a terminal observable error", () => {
        const fixture = providerFixture();
        const transport = createTransport(fixture.provider);
        const client = createClient(transport);
        const errors: Error[] = [];

        client.account
            .connectionStatusSubscribe()
            .subscribe({ error: (error) => errors.push(error) });

        const providerError = new Error("provider closed");
        fixture.close(providerError);

        expect(errors).toHaveLength(1);
        expect(errors[0]).toBeInstanceOf(SubscriptionError);
        expect(errors[0].message).toBe("provider closed");
        expect((errors[0] as SubscriptionError).reason).toBeUndefined();
        expect(errors[0].cause).toBe(providerError);
    });
});

describe("connection preparation", () => {
    const requestIds = { trait: 200, method: 194, kind: "request" as const };
    const request = {
        ids: requestIds,
        payload: new Uint8Array(),
        decodeResponse: () => ({ success: true as const, value: undefined }),
    };

    function readiness() {
        let resolve!: () => void;
        let reject!: (error: Error) => void;
        const promise = new Promise<void>((ready, failed) => {
            resolve = ready;
            reject = failed;
        });
        return { promise, resolve, reject };
    }

    it("keeps dispatch synchronous when preparation is absent", async () => {
        const fixture = providerFixture();
        const transport = createTransport(fixture.provider);
        const response = transport.request(request);
        createClient(transport).theme.subscribe().subscribe();
        expect(fixture.sent.map(toHex)).toEqual(
            [
                wireFrame("p:1", requestIds, MESSAGE_TYPE_REQUEST),
                wireFrame("p:2", W.THEME_SUBSCRIBE, MESSAGE_TYPE_START, new Uint8Array([0])),
            ].map(toHex),
        );
        fixture.receive(wireFrame("p:1", requestIds, MESSAGE_TYPE_RESPONSE));
        expect((await response)._unsafeUnwrap()).toBeUndefined();
        transport.dispose();
    });

    it("prepares requests and subscriptions through the same readiness promise", async () => {
        const fixture = providerFixture();
        const ready = readiness();
        const prepared: { trait: number; method: number }[] = [];
        const transport = createTransport(fixture.provider, {
            prepare(ids) {
                prepared.push({ trait: ids.trait, method: ids.method });
                return ready.promise;
            },
        });
        const response = transport.request(request);
        createClient(transport).theme.subscribe().subscribe();
        const beforeReady = [...fixture.sent];
        ready.resolve();
        await ready.promise;
        expect({ beforeReady, prepared, sent: fixture.sent.map(toHex) }).toEqual({
            beforeReady: [],
            prepared: [
                { trait: requestIds.trait, method: requestIds.method },
                { trait: W.THEME_SUBSCRIBE.trait, method: W.THEME_SUBSCRIBE.method },
            ],
            sent: [
                wireFrame("p:1", requestIds, MESSAGE_TYPE_REQUEST),
                wireFrame("p:2", W.THEME_SUBSCRIBE, MESSAGE_TYPE_START, new Uint8Array([0])),
            ].map(toHex),
        });
        fixture.receive(wireFrame("p:1", requestIds, MESSAGE_TYPE_RESPONSE));
        await response;
        transport.dispose();
    });

    it("abandons an aborted request before readiness without contacting the host", async () => {
        const fixture = providerFixture();
        const ready = readiness();
        const transport = createTransport(fixture.provider, { prepare: () => ready.promise });
        const controller = new AbortController();
        const cancelled = new Error("request withdrawn while opening");
        const response = Promise.resolve(
            transport.request({ ...request, signal: controller.signal }),
        ).catch((error) => error);
        controller.abort(cancelled);
        ready.resolve();
        await ready.promise;
        expect({ outcome: await response, sent: fixture.sent }).toEqual({
            outcome: cancelled,
            sent: [],
        });
        transport.dispose();
    });

    it("does not send a timed-out request after readiness arrives", async () => {
        jest.useFakeTimers();
        const fixture = providerFixture();
        const ready = readiness();
        const transport = createTransport(fixture.provider, {
            prepare: () => ready.promise,
            requestTimeoutMs: 25,
        });
        try {
            const response = Promise.resolve(transport.request(request)).catch((error) => error);
            jest.advanceTimersByTime(26);
            ready.resolve();
            await ready.promise;
            expect({ outcome: await response, sent: fixture.sent }).toEqual({
                outcome: new RequestTimeoutError("p:1", requestIds.trait, requestIds.method, 25),
                sent: [],
            });
        } finally {
            transport.dispose();
            jest.useRealTimers();
        }
    });

    it("keeps a sent prepared request pending for the host's cancellation response", async () => {
        const fixture = providerFixture();
        const ready = readiness();
        const transport = createTransport(fixture.provider, { prepare: () => ready.promise });
        const controller = new AbortController();
        let settled = false;
        const response = transport
            .request({
                ...request,
                signal: controller.signal,
                decodeResponse: () => ({
                    success: false as const,
                    value: { tag: "Cancelled" as const },
                }),
            })
            .then((result) => {
                settled = true;
                return result;
            });
        ready.resolve();
        await ready.promise;
        controller.abort();
        await Promise.resolve();
        expect({ settled, sent: fixture.sent.map(toHex) }).toEqual({
            settled: false,
            sent: [
                wireFrame("p:1", requestIds, MESSAGE_TYPE_REQUEST),
                wireFrame("p:1", requestIds, MESSAGE_TYPE_CANCEL),
            ].map(toHex),
        });
        fixture.receive(wireFrame("p:1", requestIds, MESSAGE_TYPE_RESPONSE));
        expect((await response)._unsafeUnwrapErr()).toEqual({ tag: "Cancelled" });
        transport.dispose();
    });

    it("does not send work interrupted while opening and allows a later operation", async () => {
        const fixture = providerFixture();
        const ready = readiness();
        const transport = createTransport(fixture.provider, { prepare: () => ready.promise });
        const response = Promise.resolve(transport.request(request)).catch((error) => error);
        const errors: unknown[] = [];
        createClient(transport)
            .theme.subscribe()
            .subscribe({ error: (error) => errors.push(error.cause) });
        const interrupted = new Error("opening connection interrupted");
        fixture.reset(interrupted);
        ready.resolve();
        await ready.promise;
        const fresh = transport.request(request);
        await Promise.resolve();
        fixture.receive(wireFrame("p:3", requestIds, MESSAGE_TYPE_RESPONSE));
        expect({
            outcome: await response,
            errors,
            sent: fixture.sent.map(toHex),
            fresh: (await fresh)._unsafeUnwrap(),
        }).toEqual({
            outcome: interrupted,
            errors: [interrupted],
            sent: [toHex(wireFrame("p:3", requestIds, MESSAGE_TYPE_REQUEST))],
            fresh: undefined,
        });
        transport.dispose();
    });

    it("does not start or stop a subscription cancelled before readiness", async () => {
        const fixture = providerFixture();
        const ready = readiness();
        const transport = createTransport(fixture.provider, { prepare: () => ready.promise });
        const subscription = createClient(transport).theme.subscribe().subscribe();
        subscription.unsubscribe();
        ready.resolve();
        await ready.promise;
        expect(fixture.sent).toEqual([]);
        transport.dispose();
    });

    it("fails only the operation whose preparation rejects", async () => {
        const fixture = providerFixture();
        const requestReady = readiness();
        const subscriptionReady = readiness();
        const transport = createTransport(fixture.provider, {
            prepare: (ids) =>
                ids.kind === "request" ? requestReady.promise : subscriptionReady.promise,
        });
        const response = Promise.resolve(transport.request(request)).catch((error) => error);
        const errors: unknown[] = [];
        const subscription = createClient(transport)
            .theme.subscribe()
            .subscribe({ error: (error) => errors.push(error.cause) });
        const failed = new Error("request preparation failed");
        requestReady.reject(failed);
        expect(await response).toBe(failed);
        subscriptionReady.resolve();
        await subscriptionReady.promise;
        expect({ errors, sent: fixture.sent.map(toHex) }).toEqual({
            errors: [],
            sent: [
                toHex(wireFrame("p:2", W.THEME_SUBSCRIBE, MESSAGE_TYPE_START, new Uint8Array([0]))),
            ],
        });
        subscription.unsubscribe();
        transport.dispose();
    });

    it("ends a subscription whose preparation rejects without sending a stop", async () => {
        const fixture = providerFixture();
        const ready = readiness();
        const transport = createTransport(fixture.provider, { prepare: () => ready.promise });
        const errors: unknown[] = [];
        const subscription = createClient(transport)
            .theme.subscribe()
            .subscribe({ error: (error) => errors.push(error.cause) });
        const failed = new Error("subscription preparation failed");
        ready.reject(failed);
        await ready.promise.catch(() => {});
        subscription.unsubscribe();
        expect({ errors, sent: fixture.sent }).toEqual({ errors: [failed], sent: [] });
        transport.dispose();
    });

    it("settles requests and subscriptions when preparation throws synchronously", async () => {
        const fixture = providerFixture();
        const failed = new Error("connection creation failed");
        const transport = createTransport(fixture.provider, {
            prepare() {
                throw failed;
            },
        });
        const response = Promise.resolve(transport.request(request)).catch((error) => error);
        const errors: unknown[] = [];
        createClient(transport)
            .theme.subscribe()
            .subscribe({ error: (error) => errors.push(error.cause) });
        expect({ outcome: await response, errors, sent: fixture.sent }).toEqual({
            outcome: failed,
            errors: [failed],
            sent: [],
        });
        transport.dispose();
    });

    it.each(["request", "subscription"] as const)(
        "settles a prepared %s when sending fails",
        async (kind) => {
            const fixture = providerFixture();
            const ready = readiness();
            const failed = new Error("socket closed before dispatch");
            fixture.provider.postMessage = () => {
                throw failed;
            };
            const transport = createTransport(fixture.provider, { prepare: () => ready.promise });
            const outcome =
                kind === "request"
                    ? Promise.resolve(transport.request(request)).catch((error) => error)
                    : new Promise((resolve) => {
                          transport.subscribeRaw({
                              ids: W.THEME_SUBSCRIBE,
                              payload: new Uint8Array([0]),
                              onReceive() {},
                              onClose: resolve,
                          });
                      });
            ready.resolve();
            await ready.promise;
            expect({ outcome: await outcome, sent: fixture.sent }).toEqual({
                outcome: failed,
                sent: [],
            });
            transport.dispose();
        },
    );
});

describe("recoverable connection resets", () => {
    it("never restarts a subscription that deletes a purse", () => {
        const fixture = providerFixture();
        const transport = createTransport(fixture.provider);
        const client = createClient(transport);
        const errors: unknown[] = [];
        const request = { target: 1, drainInto: 2 };
        client.coinPayment
            .deletePurse({ request })
            .subscribe({ error: (error) => errors.push(error.cause) });
        const interruption = new Error("connection interrupted");
        fixture.reset(interruption);
        client.theme.subscribe().subscribe();
        expect({ errors, frames: fixture.sent.map(toHex) }).toEqual({
            errors: [interruption],
            frames: [
                wireFrame(
                    "p:1",
                    W.COIN_PAYMENT_DELETE_PURSE,
                    MESSAGE_TYPE_START,
                    T.VersionedHostCoinPaymentDeletePurseRequest.enc({ tag: "V1", value: request }),
                ),
                wireFrame("p:2", W.THEME_SUBSCRIBE, MESSAGE_TYPE_START, new Uint8Array([0])),
            ].map(toHex),
        });
        transport.dispose();
    });

    it("rejects interrupted calls and subscriptions without replaying them", async () => {
        const fixture = providerFixture();
        const transport = createTransport(fixture.provider);
        const client = createClient(transport);
        const outcome = Promise.resolve(client.system.handshake()).catch((error: unknown) => error);
        const errors: unknown[] = [];
        const oldSubscription = client.theme
            .subscribe()
            .subscribe({ error: (error) => errors.push(error.cause) });
        const reset = new Error("connection interrupted");
        fixture.reset(reset);

        expect(await outcome).toBe(reset);
        expect(errors).toEqual([reset]);
        const nextSubscription = client.theme.subscribe().subscribe();
        oldSubscription.unsubscribe();
        expect(fixture.sent.map(toHex)).toEqual(
            [
                wireFrame(
                    "p:1",
                    W.SYSTEM_HANDSHAKE,
                    MESSAGE_TYPE_REQUEST,
                    T.VersionedHostHandshakeRequest.enc({
                        tag: "V1",
                        value: { codecVersion: TRUAPI_CODEC_VERSION },
                    }),
                ),
                wireFrame("p:2", W.THEME_SUBSCRIBE, MESSAGE_TYPE_START, new Uint8Array([0])),
                wireFrame("p:3", W.THEME_SUBSCRIBE, MESSAGE_TYPE_START, new Uint8Array([0])),
            ].map(toHex),
        );
        expect(nextSubscription.subscriptionId).toBe("p:3");
        transport.dispose();
    });

    it("does not erase work started by interruption callbacks", () => {
        const fixture = providerFixture();
        const transport = createTransport(fixture.provider);
        const client = createClient(transport);
        const values: unknown[] = [];
        let freshId = "";
        client.theme.subscribe().subscribe({
            error() {
                freshId = client.theme
                    .subscribe()
                    .subscribe({ next: (value) => values.push(value) }).subscriptionId;
            },
        });
        fixture.reset(new Error("connection interrupted"));
        fixture.receive(
            wireFrame(
                freshId,
                W.THEME_SUBSCRIBE,
                MESSAGE_TYPE_RECEIVE,
                T.VersionedHostThemeSubscribeItem.enc({
                    tag: "V1",
                    value: { name: { tag: "Default" }, variant: "Dark" },
                }),
            ),
        );
        expect({ freshId, values }).toEqual({
            freshId: "p:2",
            values: [{ name: { tag: "Default" }, variant: "Dark" }],
        });
        transport.dispose();
    });

    it("preserves render registrations but ends old instances and buffered starts", () => {
        const fixture = providerFixture();
        const transport = createTransport(fixture.provider);
        const client = createClient(transport);
        const request: T.ProductRendererRenderRequest = {
            context: {
                tag: "ChatMessage",
                value: { roomId: "room", messageId: "message", messageType: "vote" },
            },
            payload: "0x",
        };
        fixture.receive(rendererStart("h:buffered", request));
        fixture.reset(new Error("connection interrupted"));
        const handled: unknown[] = [];
        const emitters: ((node: T.RendererNode) => void)[] = [];
        let teardowns = 0;
        client.renderer.onRender((value, send) => {
            handled.push(value);
            emitters.push(send);
            return () => {
                teardowns += 1;
            };
        });
        fixture.receive(rendererStart("h:1", request));
        fixture.reset(new Error("connection interrupted"));
        emitters[0]!({ tag: "String", value: { text: "stale" } });
        fixture.receive(rendererStart("h:1", request));
        const node = { tag: "String", value: { text: "fresh" } } as const;
        emitters[1]!(node);
        expect({ handled, teardowns, sent: fixture.sent.map(toHex) }).toEqual({
            handled: [request, request],
            teardowns: 1,
            sent: [toHex(rendererReceive("h:1", node))],
        });
        transport.dispose();
    });

    it("does not revive an explicitly disposed transport", async () => {
        const fixture = providerFixture();
        const transport = createTransport(fixture.provider);
        const client = createClient(transport);
        transport.dispose();
        fixture.reset(new Error("connection interrupted"));
        await expect(Promise.resolve(client.system.handshake())).rejects.toThrow(
            "transport disposed",
        );
        expect(fixture.sent).toEqual([]);
    });

    it("notifies the other subscriptions when an interruption callback throws", () => {
        const fixture = providerFixture();
        const transport = createTransport(fixture.provider);
        const client = createClient(transport);
        client.theme.subscribe().subscribe({
            error() {
                throw new Error("broken observer");
            },
        });
        const errors: unknown[] = [];
        client.theme.subscribe().subscribe({ error: (error) => errors.push(error.cause) });
        const interruption = new Error("connection interrupted");
        expect(() => fixture.reset(interruption)).not.toThrow();
        expect(errors).toEqual([interruption]);
        transport.dispose();
    });
});
