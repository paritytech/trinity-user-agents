// Chat Extension v2 frames, as the generated client encodes them.
//
// A host that knows only v2 must accept each v2 call of a product that uses
// this client. The Rust test `truapi/tests/chat_v2_frames.rs` keeps one frame
// for each leg of methods 0 to 21 in `truapi/tests/snapshots/chat-v2-frames.txt`.
// This test encodes request legs through the generated codecs and compares
// them with that snapshot. It also pins the request frames of methods 22 to 27.

import { readFileSync } from "node:fs";

import type { Result } from "neverthrow";
import { describe, expect, it } from "bun:test";

import { MESSAGE_TYPE_REQUEST, MESSAGE_TYPE_START, encodeWireMessage } from "./transport.js";
import * as T from "./generated/types.js";
import * as W from "./generated/wire-table.js";

function toHex(bytes: Uint8Array): string {
    return Array.from(bytes)
        .map((byte) => byte.toString(16).padStart(2, "0"))
        .join("");
}

function unwrap<V>(result: Result<V, { message: string }>): V {
    return result.match(
        (value) => value,
        (error): never => {
            throw new Error(error.message);
        },
    );
}

function frame(
    method: { trait: number; method: number },
    messageType: number,
    value: Uint8Array,
): string {
    return toHex(
        unwrap(
            encodeWireMessage({
                requestId: "p:1",
                payload: {
                    traitId: method.trait,
                    methodId: method.method,
                    messageType,
                    value,
                },
            }),
        ),
    );
}

const snapshot = new Map(
    readFileSync(
        new URL(
            "../../../../rust/crates/truapi/tests/snapshots/chat-v2-frames.txt",
            import.meta.url,
        ),
        "utf8",
    )
        .trim()
        .split("\n")
        .map((line) => line.split(" ") as [string, string]),
);

const config: T.ChatRoomConfig = {
    productSource: true,
    reactionDetails: true,
    participants: false,
    messageActions: true,
    participantActions: false,
    allowedReactions: ["👍"],
    placeholder: "Nothing yet",
    reportSeen: true,
};

const message: T.ChatMessage = {
    messageId: "m1",
    origin: { participantId: "product" },
    timestamp: 1_700_000_000_000n,
    body: {
        tag: "RichText",
        value: {
            text: "**hi**",
            media: [
                {
                    tag: "Image",
                    value: {
                        resource: {
                            tag: "Preimage",
                            value: { key: `0x${"07".repeat(32)}` },
                        },
                        meta: {
                            mimeType: "image/png",
                            sizeBytes: 3n,
                            width: 4,
                            height: 5,
                            placeholder: undefined,
                            alt: "a dot",
                        },
                    },
                },
            ],
        },
    },
    status: "Sent",
    replyTo: "m0",
    editedAt: undefined,
    reactions: [{ emoji: "👍", count: 2, reactedByUser: true }],
};

describe("chat extension v2 frames", () => {
    it("encodes the v2 request legs as the Rust snapshot pins them", () => {
        const legs: [string, string][] = [
            [
                "0.create_room.request",
                frame(
                    W.CHAT_CREATE_ROOM,
                    MESSAGE_TYPE_REQUEST,
                    T.VersionedHostChatCreateRoomRequest.enc({
                        tag: "V2",
                        value: {
                            roomId: "room",
                            metadata: {
                                name: "Room",
                                avatar: {
                                    tag: "Inline",
                                    value: { mimeType: "image/png", bytes: "0x01" },
                                },
                                description: "About",
                            },
                            config,
                            preview: {
                                order: "PinToTop",
                                timestamp: 5n,
                                badge: { tag: "UnreadCount", value: { count: 3 } },
                                text: { text: "typing", ongoing: true },
                            },
                        },
                    }),
                ),
            ],
            [
                "3.post_message.request",
                frame(
                    W.CHAT_POST_MESSAGE,
                    MESSAGE_TYPE_REQUEST,
                    T.VersionedHostChatPostMessageRequest.enc({
                        tag: "V2",
                        value: {
                            roomId: "room",
                            payload: { tag: "Text", value: { text: "hi" } },
                            alt: "alt",
                        },
                    }),
                ),
            ],
            [
                "4.action_subscribe.start",
                frame(
                    W.CHAT_ACTION_SUBSCRIBE,
                    MESSAGE_TYPE_START,
                    T.VersionedHostChatActionSubscribeRequest.enc({
                        tag: "V2",
                        value: undefined,
                    }),
                ),
            ],
            [
                "7.set_room_config.request",
                frame(
                    W.CHAT_SET_ROOM_CONFIG,
                    MESSAGE_TYPE_REQUEST,
                    T.VersionedHostChatSetRoomConfigRequest.enc({
                        tag: "V1",
                        value: { roomId: "room", config },
                    }),
                ),
            ],
            [
                "11.update_feed.request",
                frame(
                    W.CHAT_UPDATE_FEED,
                    MESSAGE_TYPE_REQUEST,
                    T.VersionedHostChatUpdateFeedRequest.enc({
                        tag: "V1",
                        value: { roomId: "room", update: { tag: "New", value: message } },
                    }),
                ),
            ],
            [
                "15.set_room_activity.request",
                frame(
                    W.CHAT_SET_ROOM_ACTIVITY,
                    MESSAGE_TYPE_REQUEST,
                    T.VersionedHostChatSetRoomActivityRequest.enc({
                        tag: "V1",
                        value: {
                            roomId: "room",
                            activity: { text: "typing", participantId: "product" },
                        },
                    }),
                ),
            ],
        ];
        for (const [name, encoded] of legs) {
            expect(`${name} ${encoded}`).toBe(`${name} ${snapshot.get(name)}`);
        }
    });

    it("gives the methods after 21 the next ids of the chat trait", () => {
        expect(
            [
                W.CHAT_CAPABILITIES,
                W.CHAT_SET_ROOM_FEATURES,
                W.CHAT_SET_MESSAGE_META,
                W.CHAT_SET_ROOM_CALL,
                W.CHAT_ROOM_EVENTS_SUBSCRIBE,
                W.CHAT_SEARCH_QUERY,
            ].map(({ trait, method }) => [trait, method]),
        ).toEqual([
            [4, 22],
            [4, 23],
            [4, 24],
            [4, 25],
            [4, 26],
            [4, 27],
        ]);
    });

    it("encodes the requests of methods 22 to 27 at their first version", () => {
        expect(
            frame(
                W.CHAT_CAPABILITIES,
                MESSAGE_TYPE_REQUEST,
                T.VersionedHostChatCapabilitiesRequest.enc({
                    tag: "V1",
                    value: undefined,
                }),
            ),
        ).toBe("0c703a3104160000");
        // V1, "room", preimage false, search true, calls Some { audio true,
        // video false }, threadOf Some { "parent", "m1" }, readAloud true.
        expect(
            frame(
                W.CHAT_SET_ROOM_FEATURES,
                MESSAGE_TYPE_REQUEST,
                T.VersionedHostChatSetRoomFeaturesRequest.enc({
                    tag: "V1",
                    value: {
                        roomId: "room",
                        features: {
                            preimageAttachments: false,
                            search: true,
                            calls: { audio: true, video: false },
                            threadOf: { roomId: "parent", messageId: "m1" },
                            readAloud: true,
                        },
                    },
                }),
            ),
        ).toBe(
            "0c703a3104170000" +
                "10726f6f6d" +
                "0001010100" +
                "01" +
                "18706172656e74" +
                "086d31" +
                "01",
        );
        // V1, "room", "m1", thread None, branch Some { 2, 3 }, reasoning
        // Some { None }.
        expect(
            frame(
                W.CHAT_SET_MESSAGE_META,
                MESSAGE_TYPE_REQUEST,
                T.VersionedHostChatSetMessageMetaRequest.enc({
                    tag: "V1",
                    value: {
                        roomId: "room",
                        messageId: "m1",
                        meta: {
                            thread: undefined,
                            branch: { index: 2, count: 3 },
                            reasoning: { durationMs: undefined },
                        },
                    },
                }),
            ),
        ).toBe("0c703a3104180000" + "10726f6f6d" + "086d31" + "00" + "0102000300" + "0100");
        // V1, "room", Some(Ringing { "c1", video true, caller None }).
        expect(
            frame(
                W.CHAT_SET_ROOM_CALL,
                MESSAGE_TYPE_REQUEST,
                T.VersionedHostChatSetRoomCallRequest.enc({
                    tag: "V1",
                    value: {
                        roomId: "room",
                        call: {
                            tag: "Ringing",
                            value: { callId: "c1", video: true, caller: undefined },
                        },
                    },
                }),
            ),
        ).toBe("0c703a3104190000" + "10726f6f6d" + "0100" + "086331" + "01" + "00");
        expect(
            frame(
                W.CHAT_ROOM_EVENTS_SUBSCRIBE,
                MESSAGE_TYPE_START,
                T.VersionedHostChatRoomEventsSubscribeRequest.enc({
                    tag: "V1",
                    value: undefined,
                }),
            ),
        ).toBe("0c703a31041a0000");
        // V1, "room", "hi", cursor None, page size 30.
        expect(
            frame(
                W.CHAT_SEARCH_QUERY,
                MESSAGE_TYPE_START,
                T.VersionedProductChatSearchQueryRequest.enc({
                    tag: "V1",
                    value: {
                        roomId: "room",
                        text: "hi",
                        cursor: undefined,
                        pageSize: 30,
                    },
                }),
            ),
        ).toBe("0c703a31041b0000" + "10726f6f6d" + "086869" + "00" + "1e000000");
    });

    it("decodes each room event it encodes", () => {
        const events: T.ChatRoomEvent[] = [
            {
                tag: "UserStateChanged",
                value: { mutedUntil: 18_446_744_073_709_551_615n, archived: true },
            },
            { tag: "Woken", value: { watchId: "dev" } },
            { tag: "CallRequested", value: { video: true } },
            { tag: "CallAnswered", value: { callId: "c1", accept: false } },
            { tag: "CallOpened", value: { callId: "c1" } },
            { tag: "CallEnded", value: { callId: "c1" } },
            { tag: "BranchSelected", value: { messageId: "m1", index: 2 } },
        ];
        for (const event of events) {
            const item: T.VersionedHostChatRoomEventsSubscribeItem = {
                tag: "V1",
                value: { roomId: "room", event },
            };
            const encoded = T.VersionedHostChatRoomEventsSubscribeItem.enc(item);
            expect(T.VersionedHostChatRoomEventsSubscribeItem.dec(encoded)).toEqual(item);
        }
    });
});
