import { describe, expect, it } from "bun:test";
import { err, ok, ResultAsync, type Result } from "neverthrow";

import { createTransport } from "./client.js";
import * as S from "./scale.js";
import {
    decodeWireMessage,
    encodeWireMessage,
    hostRequestHandler,
    MESSAGE_TYPE_INTERRUPT,
    MESSAGE_TYPE_RECEIVE,
    MESSAGE_TYPE_START,
    MESSAGE_TYPE_STOP,
    PROTOCOL_ERROR_METHOD_ID,
    PROTOCOL_ERROR_TRAIT_ID,
    type HostInitiatedRequestHandler,
    type MethodIds,
    type ProtocolMessage,
} from "./transport.js";

const IDS: MethodIds = { trait: 250, method: 1, kind: "subscription" };
const OTHER_IDS: MethodIds = { trait: 250, method: 2, kind: "subscription" };
const INTERRUPT = S.Result(S._void, S.CallError(S.str));
type Failure = S.CallErrorValue<string>;
type Answer = Result<string, Failure>;

function deferred<Value>() {
    let resolve!: (value: Value) => void;
    const promise = new Promise<Value>((complete) => {
        resolve = complete;
    });
    return { promise, resolve };
}

function payload(value: string, version = 1) {
    return new Uint8Array([version - 1, ...S.str.enc(value)]);
}

function fixture(bufferCapacity = 64, requestVersion = 1) {
    const sent: ProtocolMessage[] = [];
    const decodedVersions: number[] = [];
    let receive: (message: Uint8Array) => void = () => {};
    let reset: (reason: Error) => void = () => {};
    let onSend: (() => void) | undefined;
    const transport = createTransport({
        postMessage(bytes) {
            const decoded = decodeWireMessage(bytes);
            if (decoded.isErr()) throw decoded.error;
            sent.push(decoded.value);
            onSend?.();
        },
        subscribe(listener) {
            receive = listener;
            return () => {};
        },
        subscribeReset(listener) {
            reset = listener;
            return () => {};
        },
        dispose() {},
    });
    const params = {
        ids: IDS,
        singleResponse: { requestVersion },
        decodeRequest(bytes: Uint8Array) {
            decodedVersions.push(bytes[0]! + 1);
            return S.Tuple(S.u8, S.str).dec(bytes)[1];
        },
        encodeItem: S.str.enc,
        encodeInterrupt(reason?: Failure) {
            return INTERRUPT.enc(
                reason === undefined
                    ? { success: true, value: undefined }
                    : { success: false, value: reason },
            );
        },
        declinePayload: INTERRUPT.enc({
            success: false,
            value: { tag: "HostFailure", value: { reason: "unavailable" } },
        }),
        bufferCapacity,
    };
    const registration = transport.registerHostInitiatedSubscription(params);
    return {
        transport,
        registration,
        params,
        sent,
        decodedVersions,
        onSend(callback: () => void) {
            onSend = callback;
        },
        handler(handler: HostInitiatedRequestHandler<string, string, Failure>) {
            return registration.setHandler(hostRequestHandler(handler));
        },
        reset() {
            reset(new Error("connection replaced"));
        },
        receive(
            requestId: string,
            type = MESSAGE_TYPE_START,
            value = payload(requestId, requestVersion),
            ids = IDS,
        ) {
            const encoded = encodeWireMessage({
                requestId,
                payload: { traitId: ids.trait, methodId: ids.method, messageType: type, value },
            });
            if (encoded.isErr()) throw encoded.error;
            receive(encoded.value);
        },
        events() {
            return sent.map((message) => ({
                requestId: message.requestId,
                ids: [message.payload.traitId, message.payload.methodId],
                type: message.payload.messageType,
                value:
                    message.payload.messageType === MESSAGE_TYPE_RECEIVE
                        ? S.str.dec(message.payload.value)
                        : INTERRUPT.dec(message.payload.value),
            }));
        },
    };
}

function ended(requestId: string, reason?: Failure, ids = IDS) {
    return {
        requestId,
        ids: [ids.trait, ids.method],
        type: MESSAGE_TYPE_INTERRUPT,
        value:
            reason === undefined
                ? { success: true as const, value: undefined }
                : { success: false as const, value: reason },
    };
}

function answered(requestId: string, value: string, ids = IDS) {
    return [{ requestId, ids: [ids.trait, ids.method], type: MESSAGE_TYPE_RECEIVE, value }];
}

async function settle() {
    for (let turn = 0; turn < 8; turn++) await Promise.resolve();
}

describe("host requests over one-item subscriptions", () => {
    it("adapts sync, async and ResultAsync results to items or terminal errors", async () => {
        const test = fixture();
        test.handler((request) => ok(request));
        test.receive("sync");
        await settle();
        test.handler(async () => err({ tag: "Domain", value: "refused" }));
        test.receive("async");
        await settle();
        test.handler(() => ResultAsync.fromSafePromise(Promise.resolve("thenable")));
        test.receive("result-async");
        await settle();
        test.handler(() => err({ tag: "Denied" }));
        test.receive("denied");
        await settle();
        expect(test.events()).toEqual([
            ...answered("sync", "sync"),
            ended("async", { tag: "Domain", value: "refused" }),
            ...answered("result-async", "thenable"),
            ended("denied", { tag: "Denied" }),
        ]);
        test.transport.dispose();
    });

    it("does not abort completed handler signals during terminal cleanup", async () => {
        const test = fixture();
        const signals: AbortSignal[] = [];
        test.handler((request, { signal }) => {
            signals.push(signal);
            return request === "success" ? ok(request) : err({ tag: "Denied" });
        });
        test.receive("success");
        test.receive("failure");
        await settle();
        test.transport.dispose();
        expect(signals.map((signal) => signal.aborted)).toEqual([false, false]);
    });

    it("turns thrown and rejected exceptions into failures without stranding siblings", async () => {
        const test = fixture();
        test.handler((request) => {
            if (request === "throw") throw new Error("handler threw");
            if (request === "reject") return Promise.reject(new Error("handler rejected"));
            return ok(request);
        });
        test.receive("throw");
        test.receive("reject");
        test.receive("sibling");
        await settle();
        expect(
            test.events().sort((left, right) => left.requestId.localeCompare(right.requestId)),
        ).toEqual([
            ended("reject", { tag: "HostFailure", value: { reason: "handler rejected" } }),
            ...answered("sibling", "sibling"),
            ended("throw", { tag: "HostFailure", value: { reason: "handler threw" } }),
        ]);
        test.transport.dispose();
    });

    it("answers invalid JavaScript handler returns with a failure", async () => {
        const test = fixture();
        test.handler(
            (() => undefined) as unknown as HostInitiatedRequestHandler<string, string, Failure>,
        );
        test.receive("invalid");
        await settle();
        expect(test.events()).toEqual([
            ended("invalid", { tag: "HostFailure", value: { reason: expect.any(String) } }),
        ]);
        test.transport.dispose();
    });

    it("checks the selected version before decoding or evicting buffered work", async () => {
        const test = fixture(1, 2);
        test.receive("selected");
        test.receive("older", MESSAGE_TYPE_START, payload("older", 1));
        test.receive("empty", MESSAGE_TYPE_START, new Uint8Array());
        test.receive("future", MESSAGE_TYPE_START, payload("future", 3));
        const future = test.sent.pop()!;
        test.handler((request) => ok(request));
        await settle();
        expect({ events: test.events(), versions: test.decodedVersions, future }).toEqual({
            events: [
                ended("older", { tag: "Unsupported" }),
                ended("empty", {
                    tag: "MalformedFrame",
                    value: { reason: "host request payload did not decode" },
                }),
                ...answered("selected", "selected"),
            ],
            versions: [2],
            future: {
                requestId: "future",
                payload: {
                    traitId: PROTOCOL_ERROR_TRAIT_ID,
                    methodId: PROTOCOL_ERROR_METHOD_ID,
                    messageType: MESSAGE_TYPE_RECEIVE,
                    value: new Uint8Array([0, 1, IDS.trait, IDS.method, 2]),
                },
            },
        });
        test.transport.dispose();
    });

    for (const interruption of ["reset", "dispose", "send failure"] as const) {
        it(`does not retain incoming work when buffer eviction triggers ${interruption}`, async () => {
            const test = fixture(1);
            let calls = 0;
            test.receive("queued");
            test.onSend(() => {
                if (interruption === "reset") test.reset();
                else if (interruption === "dispose") test.transport.dispose();
                else throw new Error("connection lost while evicting buffered call");
            });
            test.receive("incoming");
            test.handler(() => {
                calls++;
                return ok("stale work");
            });
            await settle();
            expect({ calls, events: test.events() }).toEqual({
                calls: 0,
                events: [ended("queued", { tag: "HostFailure", value: { reason: "unavailable" } })],
            });
            test.transport.dispose();
        });
    }

    it("preserves buffer capacity when declining an old call reenters with a newer start", async () => {
        const test = fixture(1);
        const calls: string[] = [];
        let injected = false;
        test.receive("queued");
        test.onSend(() => {
            if (injected) return;
            injected = true;
            test.receive("newest");
        });
        test.receive("incoming");
        test.handler((request) => {
            calls.push(request);
            return ok(request);
        });
        await settle();
        expect({ calls, events: test.events() }).toEqual({
            calls: ["newest"],
            events: [
                ended("queued", { tag: "HostFailure", value: { reason: "unavailable" } }),
                ended("incoming", { tag: "HostFailure", value: { reason: "unavailable" } }),
                ...answered("newest", "newest"),
            ],
        });
        test.transport.dispose();
    });

    it("keeps an early stop tied to its method when another method uses the id", async () => {
        const test = fixture();
        test.transport
            .registerHostInitiatedSubscription({ ...test.params, ids: OTHER_IDS })
            .setHandler(hostRequestHandler((request) => ok(request)));
        test.handler((request) => ok(request));
        test.receive("shared", MESSAGE_TYPE_STOP);
        test.receive("shared", MESSAGE_TYPE_STOP, new Uint8Array(), OTHER_IDS);
        test.receive("shared", MESSAGE_TYPE_START, payload("other"), OTHER_IDS);
        await settle();
        test.receive("shared", MESSAGE_TYPE_STOP, new Uint8Array(), OTHER_IDS);
        test.receive("shared");
        await settle();
        expect(test.events()).toEqual(answered("shared", "other", OTHER_IDS));
        test.transport.dispose();
    });

    it("bounds startup buffering and refuses the oldest unserved call", async () => {
        const test = fixture(2);
        test.receive("first");
        test.receive("second");
        test.receive("third");
        test.handler((request) => ok(request));
        await settle();
        expect(test.events()).toEqual([
            ended("first", { tag: "HostFailure", value: { reason: "unavailable" } }),
            ...answered("second", "second"),
            ...answered("third", "third"),
        ]);
        test.transport.dispose();
    });

    it("deduplicates buffered and live calls across method registrations", async () => {
        const test = fixture();
        const answer = deferred<Answer>();
        let calls = 0;
        test.receive("duplicate");
        test.receive("duplicate");
        const handler = hostRequestHandler(() => {
            calls++;
            return answer.promise;
        });
        test.registration.setHandler(handler);
        test.transport
            .registerHostInitiatedSubscription({ ...test.params, ids: OTHER_IDS })
            .setHandler(handler);
        test.receive("duplicate");
        test.receive("duplicate", MESSAGE_TYPE_START, payload("duplicate"), OTHER_IDS);
        await settle();
        answer.resolve(ok("one execution"));
        await settle();
        expect({ calls, events: test.events() }).toEqual({
            calls: 1,
            events: answered("duplicate", "one execution"),
        });
        test.transport.dispose();
    });

    it("forgets stopped buffered calls and remembers stops that overtake starts", async () => {
        const test = fixture();
        let calls = 0;
        test.receive("early", MESSAGE_TYPE_STOP);
        test.receive("early", MESSAGE_TYPE_START, payload("future", 2));
        test.receive("buffered");
        test.receive("buffered", MESSAGE_TYPE_STOP);
        test.handler(() => {
            calls++;
            return ok("unexpected");
        });
        await settle();
        expect({ calls, events: test.events(), versions: test.decodedVersions }).toEqual({
            calls: 0,
            events: [],
            versions: [],
        });
        test.transport.dispose();
    });

    it("aborts a stopped handler and suppresses its eventual result", async () => {
        const test = fixture();
        const answer = deferred<Answer>();
        let signal: AbortSignal | undefined;
        test.handler((_request, context) => {
            signal = context.signal;
            return answer.promise;
        });
        test.transport.registerHostInitiatedSubscription({ ...test.params, ids: OTHER_IDS });
        test.receive("active");
        await settle();
        test.receive("active", MESSAGE_TYPE_STOP, new Uint8Array(), OTHER_IDS);
        expect(signal?.aborted).toBe(false);
        test.receive("active", MESSAGE_TYPE_STOP);
        answer.resolve(ok("too late"));
        await settle();
        expect({ aborted: signal?.aborted, events: test.events() }).toEqual({
            aborted: true,
            events: [],
        });
        test.transport.dispose();
    });

    for (const interruption of ["reset", "dispose", "stop"] as const) {
        it(`does not start a handler after decoding reenters ${interruption}`, async () => {
            const test = fixture();
            let calls = 0;
            test.transport
                .registerHostInitiatedSubscription({
                    ...test.params,
                    ids: OTHER_IDS,
                    decodeRequest(bytes) {
                        if (interruption === "reset") test.reset();
                        else if (interruption === "dispose") test.transport.dispose();
                        else
                            test.receive(
                                "decoding",
                                MESSAGE_TYPE_STOP,
                                new Uint8Array(),
                                OTHER_IDS,
                            );
                        return test.params.decodeRequest(bytes);
                    },
                })
                .setHandler(
                    hostRequestHandler(() => {
                        calls++;
                        return ok("unexpected side effect");
                    }),
                );
            test.receive("decoding", MESSAGE_TYPE_START, payload("value"), OTHER_IDS);
            await settle();
            expect({ calls, events: test.events() }).toEqual({ calls: 0, events: [] });
            test.transport.dispose();
        });
    }

    for (const encoding of ["returns", "throws"] as const) {
        it(`suppresses output when an item encoder stops the call and ${encoding}`, async () => {
            const test = fixture();
            test.transport
                .registerHostInitiatedSubscription({
                    ...test.params,
                    ids: OTHER_IDS,
                    encodeItem(value: string) {
                        test.receive("encoding", MESSAGE_TYPE_STOP, new Uint8Array(), OTHER_IDS);
                        if (encoding === "throws") throw new Error("encoding failed after stop");
                        return S.str.enc(value);
                    },
                })
                .setHandler(hostRequestHandler(() => ok("completed")));
            test.receive("encoding", MESSAGE_TYPE_START, payload("value"), OTHER_IDS);
            await settle();
            expect(test.events()).toEqual([]);
            test.transport.dispose();
        });
    }

    it("ends encoding failures with one interrupt", async () => {
        const test = fixture();
        test.transport
            .registerHostInitiatedSubscription({
                ...test.params,
                ids: OTHER_IDS,
                encodeItem() {
                    throw new Error("invalid result");
                },
            })
            .setHandler(hostRequestHandler(() => ok("value")));
        test.receive("invalid", MESSAGE_TYPE_START, payload("value"), OTHER_IDS);
        await settle();
        expect(test.events()).toEqual([
            ended("invalid", { tag: "HostFailure", value: { reason: "unavailable" } }, OTHER_IDS),
        ]);
        test.transport.dispose();
    });

    it("suppresses old completions after reset even when a new call reuses the id", async () => {
        const test = fixture();
        const old = deferred<Answer>();
        let oldSignal: AbortSignal | undefined;
        test.handler((_request, { signal }) => {
            oldSignal = signal;
            return old.promise;
        });
        test.receive("reused");
        await settle();
        test.reset();
        test.handler(() => ok("new connection"));
        test.receive("reused");
        old.resolve(ok("stale connection"));
        await settle();
        expect({ aborted: oldSignal?.aborted, events: test.events() }).toEqual({
            aborted: true,
            events: answered("reused", "new connection"),
        });
        test.transport.dispose();
    });

    it("keeps queued ids reserved while a decoder reenters registration draining", async () => {
        const test = fixture();
        const registration = test.transport.registerHostInitiatedSubscription({
            ...test.params,
            ids: OTHER_IDS,
            decodeRequest(bytes) {
                const value = test.params.decodeRequest(bytes);
                if (value === "first")
                    test.receive("second", MESSAGE_TYPE_START, payload("injected"), OTHER_IDS);
                return value;
            },
        });
        test.receive("first", MESSAGE_TYPE_START, payload("first"), OTHER_IDS);
        test.receive("second", MESSAGE_TYPE_START, payload("original"), OTHER_IDS);
        registration.setHandler(hostRequestHandler((request) => ok(request)));
        await settle();
        expect(test.events()).toEqual([
            ...answered("first", "first", OTHER_IDS),
            ...answered("second", "original", OTHER_IDS),
        ]);
        test.transport.dispose();
    });

    it("suppresses terminal frames when failure cleanup replaces the connection", async () => {
        const test = fixture();
        const registration = test.transport.registerHostInitiatedSubscription({
            ...test.params,
            ids: OTHER_IDS,
            encodeItem(value: string) {
                if (value === "old") throw new Error("old result cannot encode");
                return S.str.enc(value);
            },
        });
        registration.setHandler(
            hostRequestHandler((_request, { signal }) => {
                signal.addEventListener("abort", () => {
                    test.reset();
                    registration.setHandler(hostRequestHandler(() => ok("new")));
                    test.receive("reused", MESSAGE_TYPE_START, payload("new"), OTHER_IDS);
                });
                return ok("old");
            }),
        );
        test.receive("reused", MESSAGE_TYPE_START, payload("old"), OTHER_IDS);
        await settle();
        expect(test.events()).toEqual(answered("reused", "new", OTHER_IDS));
        test.transport.dispose();
    });

    it("stops draining queued calls when decoding replaces the connection", async () => {
        const test = fixture();
        let calls = 0;
        const registration = test.transport.registerHostInitiatedSubscription({
            ...test.params,
            ids: OTHER_IDS,
            decodeRequest(bytes) {
                test.reset();
                return test.params.decodeRequest(bytes);
            },
        });
        test.receive("first", MESSAGE_TYPE_START, payload("first"), OTHER_IDS);
        test.receive("second", MESSAGE_TYPE_START, payload("second"), OTHER_IDS);
        registration.setHandler(
            hostRequestHandler(() => {
                calls++;
                return ok("stale");
            }),
        );
        await settle();
        expect({ calls, versions: test.decodedVersions, events: test.events() }).toEqual({
            calls: 0,
            versions: [1],
            events: [],
        });
        test.transport.dispose();
    });

    it("bounds early stops and clears them with buffered calls on reset", async () => {
        const test = fixture();
        for (let index = 0; index < 65; index++) test.receive(`early:${index}`, MESSAGE_TYPE_STOP);
        test.handler((request) => ok(request));
        test.receive("early:64", MESSAGE_TYPE_STOP);
        test.receive("early:0");
        test.receive("early:1");
        test.receive("early:64");
        await settle();
        expect(test.events()).toEqual(answered("early:0", "early:0"));
        test.receive("stale", MESSAGE_TYPE_STOP);
        test.reset();
        test.receive("stale");
        await settle();
        expect(test.events()).toEqual([
            ...answered("early:0", "early:0"),
            ...answered("stale", "stale"),
        ]);
        test.transport.dispose();
    });

    it("retains active work and replacement handlers when an older registration unsubscribes", async () => {
        const test = fixture();
        const answer = deferred<Answer>();
        const handler = hostRequestHandler(() => answer.promise);
        const old = test.registration.setHandler(handler);
        test.receive("active");
        test.registration.setHandler(handler);
        old.unsubscribe();
        test.receive("replacement");
        answer.resolve(ok("served"));
        await settle();
        expect(test.events()).toEqual([
            ...answered("active", "served"),
            ...answered("replacement", "served"),
        ]);
        test.transport.dispose();
    });

    it("keeps ordinary subscription restart behavior", () => {
        const test = fixture();
        const events: string[] = [];
        test.transport
            .registerHostInitiatedSubscription({
                ...test.params,
                ids: OTHER_IDS,
                singleResponse: undefined,
            })
            .setHandler(() => {
                events.push("start");
                return () => events.push("stop");
            });
        test.receive("render", MESSAGE_TYPE_START, payload("value"), OTHER_IDS);
        test.receive("render", MESSAGE_TYPE_START, payload("value"), OTHER_IDS);
        expect(events).toEqual(["start", "stop", "start"]);
        test.transport.dispose();
    });

    it("rejects invalid one-response configuration and duplicate routes", () => {
        const test = fixture();
        expect(() => test.transport.registerHostInitiatedSubscription(test.params)).toThrow();
        for (const requestVersion of [0, 256, 1.5, NaN]) {
            expect(() =>
                test.transport.registerHostInitiatedSubscription({
                    ...test.params,
                    ids: OTHER_IDS,
                    singleResponse: { requestVersion },
                }),
            ).toThrow();
        }
        for (const bufferCapacity of [0, -1, 1.5, Infinity]) {
            expect(() =>
                test.transport.registerHostInitiatedSubscription({
                    ...test.params,
                    ids: OTHER_IDS,
                    bufferCapacity,
                }),
            ).toThrow();
        }
        test.transport.dispose();
    });
});
