// Chat Extension v2 frames, as the generated client encodes them.
//
// A host that knows only v2 must accept each v2 call of a product that uses
// this client. The Rust test `truapi/tests/chat_v2_frames.rs` keeps one frame
// for each leg of the v2 methods in `truapi/tests/snapshots/chat-v2-frames.txt`.
// This test encodes request legs through the generated codecs and compares
// them with that snapshot. It also pins the request frames of methods 23 to 26.
//
// `createRoom`, `postMessage` and `setRoomFooter` keep the shapes of the first
// SDK: a host injects this client into products built against that SDK, so
// those products call the client with the first shapes.

import { readFileSync } from "node:fs";

import type { Result } from "neverthrow";
import { describe, expect, it } from "bun:test";

import { createTransport } from "./client.js";
import { createClient } from "./generated/client.js";
import {
    MESSAGE_TYPE_REQUEST,
    MESSAGE_TYPE_START,
    encodeWireMessage,
    type WireProvider,
} from "./transport.js";
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

function fromHex(hex: string): Uint8Array {
    return Uint8Array.from(hex.match(/../g) ?? [], (byte) => Number.parseInt(byte, 16));
}

/** A client over a provider that keeps what the client sends. */
function recordingClient() {
    const sent: string[] = [];
    let deliver: (message: Uint8Array) => void = () => {};
    const provider: WireProvider = {
        postMessage(message: Uint8Array) {
            sent.push(toHex(message));
        },
        subscribe(callback: (message: Uint8Array) => void) {
            deliver = callback;
            return () => {};
        },
        subscribeClose() {
            return () => {};
        },
        subscribeReset() {
            return () => {};
        },
        dispose() {},
    };
    return {
        sent,
        client: createClient(createTransport(provider)),
        receive: (hex: string) => deliver(fromHex(hex)),
    };
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
    reportSeen: true,
};

const message: T.ChatMessage = {
    messageId: "m1",
    origin: { participantId: "contact" },
    timestamp: 1_700_000_000_000n,
    body: {
        tag: "RichText",
        value: {
            text: "hi",
            media: [
                {
                    tag: "Image",
                    value: {
                        resource: {
                            tag: "Hop",
                            value: {
                                identifier: `0x${"07".repeat(32)}`,
                                ticket: `0x${"08".repeat(32)}`,
                                node: "wss://hop.example",
                                size: 3n,
                            },
                        },
                        meta: {
                            mimeType: "image/png",
                            sizeBytes: 3n,
                            width: 4,
                            height: 5,
                            placeholder: undefined,
                            alt: "a dot",
                            durationMs: 6n,
                        },
                    },
                },
            ],
        },
    },
    status: "Sent",
    replyTo: "m0",
    replyPreview: {
        origin: { participantId: "user" },
        body: { tag: "Text", value: { text: "yo" } },
    },
    editedAt: undefined,
    reactions: [{ emoji: "👍", count: 2, reactedByUser: true, unseen: true }],
    request: true,
};

const call: T.ChatMessage = {
    messageId: "c1",
    origin: { participantId: "user" },
    timestamp: 1_700_000_000_000n,
    body: {
        tag: "Call",
        value: {
            video: true,
            answeredAt: 1_700_000_001_000n,
            closed: { at: 1_700_000_060_000n, byUser: false },
        },
    },
    status: "Delivered",
    replyTo: undefined,
    editedAt: undefined,
    reactions: [],
    request: false,
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
                        tag: "V1",
                        value: {
                            roomId: "room",
                            name: "Room",
                            icon: "https://icons.example/room.png",
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
                "6.set_room_footer.request",
                frame(
                    W.CHAT_SET_ROOM_FOOTER,
                    MESSAGE_TYPE_REQUEST,
                    T.VersionedHostChatSetRoomFooterRequest.enc({
                        tag: "V1",
                        value: { roomId: "room", footer: "TextInput" },
                    }),
                ),
            ],
            [
                "4.action_subscribe.start",
                frame(
                    W.CHAT_ACTION_SUBSCRIBE,
                    MESSAGE_TYPE_START,
                    T.VersionedHostChatActionSubscribeRequest.enc({
                        tag: "V1",
                        value: undefined,
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
                "11.update_feed.replaced",
                frame(
                    W.CHAT_UPDATE_FEED,
                    MESSAGE_TYPE_REQUEST,
                    T.VersionedHostChatUpdateFeedRequest.enc({
                        tag: "V1",
                        value: { roomId: "room", update: { tag: "Replaced", value: call } },
                    }),
                ),
            ],
            [
                "9.set_room_preview.request",
                frame(
                    W.CHAT_SET_ROOM_PREVIEW,
                    MESSAGE_TYPE_REQUEST,
                    T.VersionedHostChatSetRoomPreviewRequest.enc({
                        tag: "V1",
                        value: {
                            roomId: "room",
                            preview: {
                                timestamp: 5n,
                                badge: { tag: "UnreadCount", value: { count: 3 } },
                                lastMessage: message,
                                lastReaction: {
                                    emoji: "👍",
                                    byUser: false,
                                    message: undefined,
                                    removed: true,
                                },
                                unseenReactions: 2,
                                lastCall: call,
                            },
                        },
                    }),
                ),
            ],
            [
                "17.user_action.start",
                frame(
                    W.CHAT_USER_ACTION,
                    MESSAGE_TYPE_START,
                    T.VersionedProductChatUserActionRequest.enc({
                        tag: "V1",
                        value: {
                            roomId: "room",
                            action: {
                                tag: "NewMessage",
                                value: {
                                    body: { tag: "Text", value: { text: "hi" } },
                                    replyTo: undefined,
                                    messageId: "m1",
                                    timestamp: 1_700_000_000_000n,
                                },
                            },
                        },
                    }),
                ),
            ],
            [
                "17.user_action.accept",
                frame(
                    W.CHAT_USER_ACTION,
                    MESSAGE_TYPE_START,
                    T.VersionedProductChatUserActionRequest.enc({
                        tag: "V1",
                        value: { roomId: "room", action: { tag: "AcceptRequest", value: undefined } },
                    }),
                ),
            ],
            [
                "17.user_action.pay",
                frame(
                    W.CHAT_USER_ACTION,
                    MESSAGE_TYPE_START,
                    T.VersionedProductChatUserActionRequest.enc({
                        tag: "V1",
                        value: {
                            roomId: "room",
                            action: {
                                tag: "SendPayment",
                                value: {
                                    messageId: "m2",
                                    timestamp: 1_700_000_000_000n,
                                    totalValue: 5_000_000n,
                                    coinKeys: [`0x${"11".repeat(64)}`],
                                },
                            },
                        },
                    }),
                ),
            ],
            [
                "27.search_query.start",
                frame(
                    W.CHAT_SEARCH_QUERY,
                    MESSAGE_TYPE_START,
                    T.VersionedProductChatSearchQueryRequest.enc({
                        tag: "V1",
                        value: { roomId: "room", text: "hi", cursor: "c1", pageSize: 20 },
                    }),
                ),
            ],
            [
                "29.set_room_contact.request",
                frame(
                    W.CHAT_SET_ROOM_CONTACT,
                    MESSAGE_TYPE_REQUEST,
                    T.VersionedHostChatSetRoomContactRequest.enc({
                        tag: "V1",
                        value: {
                            roomId: "room",
                            contact: {
                                account: `0x${"09".repeat(32)}`,
                                username: "alice",
                                state: "Incoming",
                            },
                        },
                    }),
                ),
            ],
            [
                "30.edit_history.start",
                frame(
                    W.CHAT_EDIT_HISTORY,
                    MESSAGE_TYPE_START,
                    T.VersionedProductChatEditHistoryQueryRequest.enc({
                        tag: "V1",
                        value: { roomId: "room", messageId: "m1" },
                    }),
                ),
            ],
            [
                "31.set_room_config.request",
                frame(
                    W.CHAT_SET_ROOM_CONFIG,
                    MESSAGE_TYPE_REQUEST,
                    T.VersionedHostChatSetRoomConfigRequest.enc({
                        tag: "V1",
                        value: { roomId: "room", config },
                    }),
                ),
            ],
        ];
        for (const [name, encoded] of legs) {
            expect(`${name} ${encoded}`).toBe(`${name} ${snapshot.get(name)}`);
        }
    });

    it("gives the methods after 22 the next ids of the chat trait", () => {
        expect(
            [
                W.CHAT_SET_ROOM_FEATURES,
                W.CHAT_SET_ROOM_CALL,
                W.CHAT_ROOM_EVENTS_SUBSCRIBE,
                W.CHAT_SEARCH_QUERY,
                W.CHAT_OPEN_LINK,
                W.CHAT_SET_ROOM_CONTACT,
                W.CHAT_EDIT_HISTORY,
                W.CHAT_SET_ROOM_CONFIG,
            ].map(({ trait, method }) => [trait, method]),
        ).toEqual([
            [4, 23],
            [4, 25],
            [4, 26],
            [4, 27],
            [4, 28],
            [4, 29],
            [4, 30],
            [4, 31],
        ]);
    });

    it("encodes the requests of methods 23 to 26 at their first version", () => {
        // V1, "room", calls Some { audio true, video false }, hop true.
        expect(
            frame(
                W.CHAT_SET_ROOM_FEATURES,
                MESSAGE_TYPE_REQUEST,
                T.VersionedHostChatSetRoomFeaturesRequest.enc({
                    tag: "V1",
                    value: {
                        roomId: "room",
                        features: {
                            calls: { audio: true, video: false },
                            hopAttachments: true,
                        },
                    },
                }),
            ),
        ).toBe("0c703a3104170000" + "10726f6f6d" + "010100" + "01");
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
    });

    it("sends the first shapes of createRoom, setRoomFooter and postMessage as the first SDK did", async () => {
        const created = recordingClient();
        const room = created.client.chat.createRoom({
            roomId: "room",
            name: "Room",
            icon: "https://icons.example/room.png",
        });
        expect(created.sent).toEqual([snapshot.get("0.create_room.request")]);
        created.receive(snapshot.get("0.create_room.response")!);
        expect((await room)._unsafeUnwrap()).toEqual({ status: "Exists" });

        const footed = recordingClient();
        const footer = footed.client.chat.setRoomFooter({ roomId: "room", footer: "TextInput" });
        expect(footed.sent).toEqual([snapshot.get("6.set_room_footer.request")]);
        footed.receive(snapshot.get("6.set_room_footer.response")!);
        expect((await footer)._unsafeUnwrap()).toBeUndefined();

        const posted = recordingClient();
        const post = posted.client.chat.postMessage({
            roomId: "room",
            payload: { tag: "Text", value: { text: "hi" } },
        });
        expect(posted.sent).toEqual([snapshot.get("3.post_message.request_without_alt")]);
        posted.receive(snapshot.get("3.post_message.response")!);
        expect((await post)._unsafeUnwrap()).toEqual({ messageId: "m1" });
    });

    it("decodes the first requests of createRoom, setRoomFooter and postMessage", () => {
        const payload = (name: string): Uint8Array => {
            // The request id "p:1", then the trait, method and message type.
            return fromHex(snapshot.get(name)!).slice(4 + 3);
        };
        expect(T.VersionedHostChatCreateRoomRequest.dec(payload("0.create_room.request"))).toEqual({
            tag: "V1",
            value: { roomId: "room", name: "Room", icon: "https://icons.example/room.png" },
        });
        expect(
            T.VersionedHostChatSetRoomFooterRequest.dec(payload("6.set_room_footer.request")),
        ).toEqual({ tag: "V1", value: { roomId: "room", footer: "TextInput" } });
        // The first SDK posted at V2 as well, with `alt` optional.
        expect(
            T.VersionedHostChatPostMessageRequest.dec(
                payload("3.post_message.request_without_alt"),
            ),
        ).toEqual({
            tag: "V2",
            value: {
                roomId: "room",
                payload: { tag: "Text", value: { text: "hi" } },
                alt: undefined,
            },
        });
    });

    it("decodes each room event it encodes", () => {
        const events: T.ChatRoomEvent[] = [
            { tag: "CallRequested", value: { video: true, callId: "c1" } },
            { tag: "CallRequested", value: { video: false, callId: undefined } },
            { tag: "CallAnswered", value: { callId: "c1", accept: false } },
            { tag: "CallEnded", value: { callId: "c1" } },
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
