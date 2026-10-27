import { describe, expect, it } from "bun:test";
import { err, ok, ResultAsync, type Result } from "neverthrow";

import { createTransport } from "./client.js";
import * as S from "./scale.js";
import {
    decodeWireMessage,
    encodeWireMessage,
    MESSAGE_TYPE_CANCEL,
    MESSAGE_TYPE_REQUEST,
    MESSAGE_TYPE_RESPONSE,
    MESSAGE_TYPE_STOP,
    PROTOCOL_ERROR_METHOD_ID,
    PROTOCOL_ERROR_TRAIT_ID,
    type MethodIds,
    type ProtocolMessage,
} from "./transport.js";

const IDS: MethodIds = { trait: 250, method: 1, kind: "request" };
const OTHER_IDS: MethodIds = { trait: 250, method: 2, kind: "request" };
const RESPONSE = S.Result(S.str, S.CallError(S.str));
type Failure = S.CallErrorValue<string>;
type Answer = Result<string, Failure>;

function deferred<Value>() {
    let resolve!: (value: Value) => void;
    const promise = new Promise<Value>((complete) => {
        resolve = complete;
    });
    return { promise, resolve };
}

function requestPayload(value: string, version = 1) {
    return new Uint8Array([version - 1, ...S.str.enc(value)]);
}

function fixture(bufferCapacity = 64, requestVersion = 1) {
    const sent: ProtocolMessage[] = [];
    const decodedVersions: number[] = [];
    let receive: (message: Uint8Array) => void = () => {};
    let reset: (reason: Error) => void = () => {};
    const transport = createTransport({
        postMessage(bytes) {
            const decoded = decodeWireMessage(bytes);
            if (decoded.isErr()) throw decoded.error;
            sent.push(decoded.value);
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
        requestVersion,
        decodeRequest(payload: Uint8Array) {
            decodedVersions.push(payload[0]! + 1);
            return S.Tuple(S.u8, S.str).dec(payload)[1];
        },
        encodeResponse(result: Answer) {
            return RESPONSE.enc(
                result.match(
                    (value) => ({ success: true as const, value }),
                    (value) => ({ success: false as const, value }),
                ),
            );
        },
        bufferCapacity,
    };
    const registration = transport.registerHostInitiatedRequest(params);
    return {
        transport,
        registration,
        params,
        sent,
        decodedVersions,
        reset() {
            reset(new Error("connection replaced"));
        },
        receive(
            requestId: string,
            type = MESSAGE_TYPE_REQUEST,
            value = requestPayload(requestId, requestVersion),
            ids = IDS,
        ) {
            const encoded = encodeWireMessage({
                requestId,
                payload: { traitId: ids.trait, methodId: ids.method, messageType: type, value },
            });
            if (encoded.isErr()) throw encoded.error;
            receive(encoded.value);
        },
        responses() {
            return sent.map((message) => ({
                requestId: message.requestId,
                ids: [message.payload.traitId, message.payload.methodId],
                type: message.payload.messageType,
                result: RESPONSE.dec(message.payload.value),
            }));
        },
    };
}

function response(requestId: string, result: S.ResultPayload<string, Failure>, ids = IDS) {
    return { requestId, ids: [ids.trait, ids.method], type: MESSAGE_TYPE_RESPONSE, result };
}

async function settle() {
    await Promise.resolve();
    await Promise.resolve();
    await Promise.resolve();
}

describe("host-initiated requests", () => {
    it("accepts only the selected request version before decoding or invoking its handler", async () => {
        const test = fixture(64, 2);
        const calls: string[] = [];
        test.registration.setHandler((request) => {
            calls.push(request);
            return ok(request);
        });
        test.receive("older", MESSAGE_TYPE_REQUEST, requestPayload("wrong shape", 1));
        test.receive("future", MESSAGE_TYPE_REQUEST, requestPayload("unknown shape", 3));
        test.receive("empty", MESSAGE_TYPE_REQUEST, new Uint8Array());
        test.receive("selected");
        await settle();
        expect({
            calls,
            decodedVersions: test.decodedVersions,
            messages: test.sent,
        }).toEqual({
            calls: ["selected"],
            decodedVersions: [2],
            messages: [
                {
                    requestId: "older",
                    payload: {
                        traitId: IDS.trait,
                        methodId: IDS.method,
                        messageType: MESSAGE_TYPE_RESPONSE,
                        value: RESPONSE.enc({ success: false, value: { tag: "Unsupported" } }),
                    },
                },
                {
                    requestId: "future",
                    payload: {
                        traitId: PROTOCOL_ERROR_TRAIT_ID,
                        methodId: PROTOCOL_ERROR_METHOD_ID,
                        messageType: MESSAGE_TYPE_RESPONSE,
                        value: new Uint8Array([0, 1, IDS.trait, IDS.method, 2]),
                    },
                },
                {
                    requestId: "empty",
                    payload: {
                        traitId: IDS.trait,
                        methodId: IDS.method,
                        messageType: MESSAGE_TYPE_RESPONSE,
                        value: RESPONSE.enc({
                            success: false,
                            value: {
                                tag: "MalformedFrame",
                                value: { reason: "host request payload did not decode" },
                            },
                        }),
                    },
                },
                {
                    requestId: "selected",
                    payload: {
                        traitId: IDS.trait,
                        methodId: IDS.method,
                        messageType: MESSAGE_TYPE_RESPONSE,
                        value: RESPONSE.enc({ success: true, value: "selected" }),
                    },
                },
            ],
        });
        test.transport.dispose();
    });

    it("refuses unsupported versions before startup buffering can evict valid work", async () => {
        const test = fixture(1, 2);
        test.receive("selected");
        test.receive("older", MESSAGE_TYPE_REQUEST, requestPayload("older", 1));
        const beforeRegistration = test.responses();
        test.registration.setHandler((request) => ok(request));
        await settle();
        expect({
            beforeRegistration,
            responses: test.responses(),
            decodedVersions: test.decodedVersions,
        }).toEqual({
            beforeRegistration: [
                response("older", { success: false, value: { tag: "Unsupported" } }),
            ],
            responses: [
                response("older", { success: false, value: { tag: "Unsupported" } }),
                response("selected", { success: true, value: "selected" }),
            ],
            decodedVersions: [2],
        });
        test.transport.dispose();
    });

    it("honors an early cancellation without decoding an unsupported request version", async () => {
        const test = fixture(64, 2);
        test.receive("cancelled", MESSAGE_TYPE_CANCEL, new Uint8Array());
        test.receive("cancelled", MESSAGE_TYPE_REQUEST, requestPayload("future", 3));
        test.registration.setHandler((request) => ok(request));
        await settle();
        expect({ responses: test.responses(), decodedVersions: test.decodedVersions }).toEqual({
            responses: [response("cancelled", { success: false, value: { tag: "Cancelled" } })],
            decodedVersions: [],
        });
        test.transport.dispose();
    });

    it("answers sync, async and ResultAsync handlers with their declared outcome", async () => {
        const test = fixture();
        test.registration.setHandler((request) => ok(request));
        test.receive("sync");
        await settle();
        test.registration.setHandler(async () => err({ tag: "Domain", value: "refused" }));
        test.receive("async");
        await settle();
        test.registration.setHandler(() =>
            ResultAsync.fromSafePromise(Promise.resolve("thenable")),
        );
        test.receive("result-async");
        await settle();
        expect(test.responses()).toEqual([
            response("sync", { success: true, value: "sync" }),
            response("async", { success: false, value: { tag: "Domain", value: "refused" } }),
            response("result-async", { success: true, value: "thenable" }),
        ]);
        test.transport.dispose();
    });

    it("keeps framework failures distinct from domain failures", async () => {
        const test = fixture();
        test.registration.setHandler(() => err({ tag: "Denied" }));
        test.receive("denied");
        await settle();
        expect(test.responses()).toEqual([
            response("denied", { success: false, value: { tag: "Denied" } }),
        ]);
        test.transport.dispose();
    });

    it("turns thrown and rejected exceptions into responses without stranding siblings", async () => {
        const test = fixture();
        test.registration.setHandler((request) => {
            if (request === "throw") throw new Error("handler threw");
            if (request === "reject") return Promise.reject(new Error("handler rejected"));
            return ok(request);
        });
        test.receive("throw");
        test.receive("reject");
        test.receive("sibling");
        await settle();
        expect(test.responses()).toEqual([
            response("throw", {
                success: false,
                value: { tag: "HostFailure", value: { reason: "handler threw" } },
            }),
            response("reject", {
                success: false,
                value: { tag: "HostFailure", value: { reason: "handler rejected" } },
            }),
            response("sibling", { success: true, value: "sibling" }),
        ]);
        test.transport.dispose();
    });

    it("answers encoding failures using the framework codec", async () => {
        const test = fixture();
        const registration = test.transport.registerHostInitiatedRequest({
            ...test.params,
            ids: OTHER_IDS,
            encodeResponse() {
                throw new Error("response encoding failed");
            },
        });
        registration.setHandler(() => ok("value"));
        test.receive("invalid", MESSAGE_TYPE_REQUEST, requestPayload("value"), OTHER_IDS);
        await settle();
        expect(test.responses()).toEqual([
            response(
                "invalid",
                {
                    success: false,
                    value: { tag: "HostFailure", value: { reason: "response encoding failed" } },
                },
                OTHER_IDS,
            ),
        ]);
        test.transport.dispose();
    });

    it("rejects malformed requests without invoking their handlers", async () => {
        const test = fixture();
        let calls = 0;
        test.registration.setHandler(() => {
            calls++;
            return ok("unexpected");
        });
        test.receive("malformed", MESSAGE_TYPE_REQUEST, new Uint8Array());
        await settle();
        expect({
            calls,
            responses: test.responses().map((item) => ({
                id: item.requestId,
                result: item.result.success ? "success" : item.result.value.tag,
            })),
        }).toEqual({
            calls: 0,
            responses: [{ id: "malformed", result: "MalformedFrame" }],
        });
        test.transport.dispose();
    });

    for (const interruption of ["reset", "dispose", "cancel"] as const) {
        it(`does not invoke a handler after decoding reenters ${interruption}`, async () => {
            const test = fixture();
            let calls = 0;
            const registration = test.transport.registerHostInitiatedRequest({
                ...test.params,
                ids: OTHER_IDS,
                decodeRequest(payload) {
                    const request = test.params.decodeRequest(payload);
                    if (interruption === "reset") test.reset();
                    else if (interruption === "dispose") test.transport.dispose();
                    else test.receive("decoding", MESSAGE_TYPE_CANCEL, new Uint8Array(), OTHER_IDS);
                    return request;
                },
            });
            registration.setHandler(() => {
                calls++;
                return ok("unexpected side effect");
            });
            test.receive("decoding", MESSAGE_TYPE_REQUEST, requestPayload("payload"), OTHER_IDS);
            await settle();
            expect({ calls, responses: test.responses() }).toEqual({
                calls: 0,
                responses:
                    interruption === "cancel"
                        ? [
                              response(
                                  "decoding",
                                  { success: false, value: { tag: "Cancelled" } },
                                  OTHER_IDS,
                              ),
                          ]
                        : [],
            });
            test.transport.dispose();
        });
    }

    for (const encoding of ["returns", "throws"] as const) {
        it(`lets cancellation during encoding override a codec that ${encoding}`, async () => {
            const test = fixture();
            const registration = test.transport.registerHostInitiatedRequest({
                ...test.params,
                ids: OTHER_IDS,
                encodeResponse(result: Answer) {
                    test.receive("encoding", MESSAGE_TYPE_CANCEL, new Uint8Array(), OTHER_IDS);
                    if (encoding === "throws")
                        throw new Error("encoding failed after cancellation");
                    return test.params.encodeResponse(result);
                },
            });
            registration.setHandler(() => ok("completed"));
            test.receive("encoding", MESSAGE_TYPE_REQUEST, requestPayload("payload"), OTHER_IDS);
            await settle();
            expect(test.responses()).toEqual([
                response("encoding", { success: false, value: { tag: "Cancelled" } }, OTHER_IDS),
            ]);
            test.transport.dispose();
        });
    }

    it("bounds startup buffering and explicitly refuses the oldest unserved call", async () => {
        const test = fixture(2);
        test.receive("first");
        test.receive("second");
        test.receive("third");
        test.registration.setHandler((request) => ok(request));
        await settle();
        expect(test.responses()).toEqual([
            response("first", {
                success: false,
                value: { tag: "HostFailure", value: { reason: "unavailable" } },
            }),
            response("second", { success: true, value: "second" }),
            response("third", { success: true, value: "third" }),
        ]);
        test.transport.dispose();
    });

    it("does not execute a call cancelled before its request arrives", async () => {
        const test = fixture();
        let calls = 0;
        test.registration.setHandler(() => {
            calls++;
            return ok("unexpected");
        });
        test.receive("cancelled", MESSAGE_TYPE_CANCEL, new Uint8Array());
        test.receive("cancelled");
        await settle();
        expect({ calls, responses: test.responses() }).toEqual({
            calls: 0,
            responses: [response("cancelled", { success: false, value: { tag: "Cancelled" } })],
        });
        test.transport.dispose();
    });

    it("removes cancelled buffered work before a handler is installed", async () => {
        const test = fixture();
        let calls = 0;
        test.receive("cancelled");
        test.receive("cancelled", MESSAGE_TYPE_CANCEL, new Uint8Array());
        test.registration.setHandler(() => {
            calls++;
            return ok("unexpected");
        });
        await settle();
        expect({ calls, responses: test.responses() }).toEqual({
            calls: 0,
            responses: [response("cancelled", { success: false, value: { tag: "Cancelled" } })],
        });
        test.transport.dispose();
    });

    it("aborts active work and lets cancellation override its eventual result once", async () => {
        const test = fixture();
        const answer = deferred<Answer>();
        let signal!: AbortSignal;
        test.registration.setHandler((_request, context) => {
            signal = context.signal;
            return answer.promise;
        });
        test.receive("running");
        test.receive("running", MESSAGE_TYPE_CANCEL, new Uint8Array());
        test.receive("running", MESSAGE_TYPE_CANCEL, new Uint8Array());
        const beforeCompletion = test.responses();
        answer.resolve(ok("completed after cancellation"));
        await settle();
        expect({ aborted: signal.aborted, beforeCompletion, responses: test.responses() }).toEqual({
            aborted: true,
            beforeCompletion: [],
            responses: [response("running", { success: false, value: { tag: "Cancelled" } })],
        });
        test.transport.dispose();
    });

    it("does not run a duplicate live request twice, even on another method", async () => {
        const test = fixture();
        const answer = deferred<Answer>();
        let calls = 0;
        test.registration.setHandler(() => {
            calls++;
            return answer.promise;
        });
        const other = test.transport.registerHostInitiatedRequest({
            ...test.params,
            ids: OTHER_IDS,
        });
        other.setHandler(() => {
            calls++;
            return ok("wrong method");
        });
        test.receive("duplicate");
        test.receive("duplicate");
        test.receive("duplicate", MESSAGE_TYPE_REQUEST, requestPayload("duplicate"), OTHER_IDS);
        answer.resolve(ok("one execution"));
        await settle();
        expect({ calls, responses: test.responses() }).toEqual({
            calls: 1,
            responses: [response("duplicate", { success: true, value: "one execution" })],
        });
        test.transport.dispose();
    });

    it("does not cancel a request through another method or a subscription stop", async () => {
        const test = fixture();
        const answer = deferred<Answer>();
        let signal!: AbortSignal;
        test.registration.setHandler((_request, context) => {
            signal = context.signal;
            return answer.promise;
        });
        test.transport.registerHostInitiatedRequest({ ...test.params, ids: OTHER_IDS });
        test.receive("running");
        test.receive("running", MESSAGE_TYPE_CANCEL, new Uint8Array(), OTHER_IDS);
        test.receive("running", MESSAGE_TYPE_STOP, new Uint8Array());
        answer.resolve(ok("completed"));
        await settle();
        expect({ aborted: signal.aborted, responses: test.responses() }).toEqual({
            aborted: false,
            responses: [response("running", { success: true, value: "completed" })],
        });
        test.transport.dispose();
    });

    it("suppresses old completions after a reset even if the replacement reuses the id", async () => {
        const test = fixture();
        const oldAnswer = deferred<Answer>();
        const newAnswer = deferred<Answer>();
        let oldSignal!: AbortSignal;
        test.registration.setHandler((_request, context) => {
            oldSignal = context.signal;
            return oldAnswer.promise;
        });
        test.receive("reused");
        test.reset();
        test.registration.setHandler(() => newAnswer.promise);
        test.receive("reused");
        oldAnswer.resolve(ok("old"));
        await settle();
        const afterOld = test.responses();
        newAnswer.resolve(ok("new"));
        await settle();
        expect({ oldAborted: oldSignal.aborted, afterOld, responses: test.responses() }).toEqual({
            oldAborted: true,
            afterOld: [],
            responses: [response("reused", { success: true, value: "new" })],
        });
        test.transport.dispose();
    });

    it("aborts disposal work and sends no response when its promise later settles", async () => {
        const test = fixture();
        const answer = deferred<Answer>();
        let signal!: AbortSignal;
        test.registration.setHandler((_request, context) => {
            signal = context.signal;
            return answer.promise;
        });
        test.receive("disposed");
        test.transport.dispose();
        answer.resolve(err({ tag: "Denied" }));
        await settle();
        expect({ aborted: signal.aborted, responses: test.responses() }).toEqual({
            aborted: true,
            responses: [],
        });
    });

    it("keeps an active handler and a newer registration when an older registration unsubscribes", async () => {
        const test = fixture();
        const answer = deferred<Answer>();
        const previous = test.registration.setHandler(() => answer.promise);
        test.receive("active");
        test.registration.setHandler(() => ok("new handler"));
        previous.unsubscribe();
        test.receive("new");
        answer.resolve(ok("old handler"));
        await settle();
        expect(test.responses()).toEqual([
            response("new", { success: true, value: "new handler" }),
            response("active", { success: true, value: "old handler" }),
        ]);
        test.transport.dispose();
    });

    it("clears buffered calls and early cancellation memory on reset", async () => {
        const test = fixture();
        test.receive("buffered");
        test.receive("early", MESSAGE_TYPE_CANCEL, new Uint8Array());
        test.reset();
        test.registration.setHandler((request) => ok(request));
        test.receive("early");
        await settle();
        expect(test.responses()).toEqual([response("early", { success: true, value: "early" })]);
        test.transport.dispose();
    });

    it("keeps the newest registration when the same handler is registered twice", async () => {
        const test = fixture();
        const handler = () => ok<string, Failure>("registered twice");
        const previous = test.registration.setHandler(handler);
        test.registration.setHandler(handler);
        previous.unsubscribe();
        test.receive("same-handler");
        await settle();
        expect(test.responses()).toEqual([
            response("same-handler", { success: true, value: "registered twice" }),
        ]);
        test.transport.dispose();
    });

    it("deduplicates buffered calls before startup", async () => {
        const test = fixture();
        let calls = 0;
        test.receive("buffered");
        test.receive("buffered");
        test.registration.setHandler(() => {
            calls++;
            return ok("started once");
        });
        await settle();
        expect({ calls, responses: test.responses() }).toEqual({
            calls: 1,
            responses: [response("buffered", { success: true, value: "started once" })],
        });
        test.transport.dispose();
    });

    it("bounds early cancellations while preserving the newest withdrawals", async () => {
        const test = fixture();
        for (let index = 0; index < 65; index++) {
            test.receive(`cancel-${index}`, MESSAGE_TYPE_CANCEL, new Uint8Array());
        }
        test.registration.setHandler((request) => ok(request));
        test.receive("cancel-0");
        test.receive("cancel-64");
        await settle();
        expect(test.responses()).toEqual([
            response("cancel-64", { success: false, value: { tag: "Cancelled" } }),
            response("cancel-0", { success: true, value: "cancel-0" }),
        ]);
        test.transport.dispose();
    });

    it("rejects ambiguous route registration and invalid buffer bounds", () => {
        const test = fixture();
        expect(() => test.transport.registerHostInitiatedRequest(test.params)).toThrow(
            "already registered",
        );
        for (const bufferCapacity of [0, -1, 1.5, Infinity]) {
            expect(() =>
                test.transport.registerHostInitiatedRequest({
                    ...test.params,
                    ids: OTHER_IDS,
                    bufferCapacity,
                }),
            ).toThrow("positive integer");
        }
        for (const requestVersion of [0, -1, 1.5, 256, Infinity]) {
            expect(() =>
                test.transport.registerHostInitiatedRequest({
                    ...test.params,
                    ids: OTHER_IDS,
                    requestVersion,
                }),
            ).toThrow("requestVersion must be an integer from 1 to 255");
        }
        expect(() =>
            test.transport.registerHostInitiatedRequest({
                ...test.params,
                ids: { ...OTHER_IDS, kind: "subscription" },
            }),
        ).toThrow("request method");
        test.transport.dispose();
    });
});
