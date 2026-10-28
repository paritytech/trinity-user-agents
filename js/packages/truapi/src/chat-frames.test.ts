// Chat Extension v2 frames, as the generated client encodes them.
//
// A host that knows only v2 must accept each v2 call of a product that uses
// this client. The Rust test `truapi/tests/chat_v2_frames.rs` keeps one frame
// for each leg of the v2 methods. The frames are in
// `truapi/tests/snapshots/chat-v2-frames.txt`.
// These tests encode the legs this client sends through the generated codecs
// and the client itself, and compare them with that snapshot. They also give
// the client the host's response frames and read the results it decodes.
//
// `createRoom`, `postMessage` and `setRoomFooter` keep the shapes of the first
// SDK. A host injects this client into products that use that SDK. Those
// products call the client with the first shapes.

import { readFileSync } from "node:fs";

import type { Result, ResultAsync } from "neverthrow";
import { describe, expect, it } from "bun:test";

import { createTransport } from "./client.js";
import { createClient } from "./generated/client.js";
import { bytesToHex, hexToBytes, type CallErrorValue } from "./scale.js";
import {
    MESSAGE_TYPE_REQUEST,
    MESSAGE_TYPE_START,
    encodeWireMessage,
    type WireProvider,
} from "./transport.js";
import * as T from "./generated/types.js";
import * as W from "./generated/wire-table.js";

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
    return bytesToHex(
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

/** A client over a provider that keeps what the client sends. */
function recordingClient() {
    const sent: string[] = [];
    let deliver: (message: Uint8Array) => void = () => {};
    const provider: WireProvider = {
        postMessage(message: Uint8Array) {
            sent.push(bytesToHex(message));
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
        receive: (hex: string) => deliver(hexToBytes(hex)),
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
        .map((line) => {
            const [name, hex] = line.split(" ");
            return [name, `0x${hex}`] as [string, string];
        }),
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

const custom: T.ChatMessage = {
    messageId: "x1",
    origin: { participantId: "contact" },
    timestamp: 1_700_000_000_000n,
    body: {
        tag: "Custom",
        value: { message: { messageType: "poll", payload: "0x0102" }, alt: "A poll" },
    },
    status: "Sent",
    replyTo: undefined,
    editedAt: undefined,
    reactions: [],
    request: false,
};

const preview: T.ChatRoomPreview = {
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
};

/** The frame the snapshot keeps under `name`. */
function pinned(name: string): string {
    const frame = snapshot.get(name);
    if (frame === undefined) {
        throw new Error(`the snapshot has no frame named ${name}`);
    }
    return frame;
}

type Client = ReturnType<typeof createClient>;

/**
 * Make one call and check its request frame. Then answer it with a response
 * frame of the snapshot, and give the result that the client reads.
 */
async function answered<E>(
    request: string,
    response: string,
    invoke: (client: Client) => ResultAsync<undefined, E>,
): Promise<Result<undefined, E>> {
    const { sent, client, receive } = recordingClient();
    const result = invoke(client);
    expect(sent).toEqual([pinned(request)]);
    receive(pinned(response));
    return await result;
}

/** Deliver a start frame of the snapshot to a product handler and give what the client sends back. */
async function answering(start: string, install: (client: Client) => void): Promise<string[]> {
    const { sent, client, receive } = recordingClient();
    install(client);
    receive(pinned(start));
    await Promise.resolve();
    return sent;
}

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
                "11.update_feed.custom",
                frame(
                    W.CHAT_UPDATE_FEED,
                    MESSAGE_TYPE_REQUEST,
                    T.VersionedHostChatUpdateFeedRequest.enc({
                        tag: "V1",
                        value: { roomId: "room", update: { tag: "New", value: custom } },
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
                        value: { roomId: "room", preview },
                    }),
                ),
            ],
            [
                "13.user_action.start",
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
                "13.user_action.accept",
                frame(
                    W.CHAT_USER_ACTION,
                    MESSAGE_TYPE_START,
                    T.VersionedProductChatUserActionRequest.enc({
                        tag: "V1",
                        value: {
                            roomId: "room",
                            action: { tag: "AcceptRequest", value: undefined },
                        },
                    }),
                ),
            ],
            [
                "13.user_action.pay",
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
                                    sr25519SecretKeys: [`0x${"11".repeat(64)}`],
                                },
                            },
                        },
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
        ];
        for (const [name, encoded] of legs) {
            expect(`${name} ${encoded}`).toBe(`${name} ${snapshot.get(name)}`);
        }
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

    it("sends the v2 calls and reads the host's answers as the Rust snapshot pins them", async () => {
        const metadata = { name: "Room", description: "About" };
        const notRoomHost: CallErrorValue<T.VersionedHostChatRoomError> = {
            tag: "Domain",
            value: { tag: "V1", value: { tag: "NotRoomHost" } },
        };
        const notOwner: CallErrorValue<T.VersionedHostChatUpdateFeedError> = {
            tag: "Domain",
            value: { tag: "V1", value: { tag: "NotOwner" } },
        };

        const set = await answered(
            "8.set_room_metadata.request",
            "8.set_room_metadata.response",
            (client) => client.chat.setRoomMetadata({ roomId: "room", metadata }),
        );
        expect(set._unsafeUnwrap()).toBeUndefined();
        const refused = await answered(
            "8.set_room_metadata.request",
            "8.set_room_metadata.error",
            (client) => client.chat.setRoomMetadata({ roomId: "room", metadata }),
        );
        expect(refused._unsafeUnwrapErr()).toEqual(notRoomHost);

        const previewed = await answered(
            "9.set_room_preview.request",
            "9.set_room_preview.response",
            (client) => client.chat.setRoomPreview({ roomId: "room", preview }),
        );
        expect(previewed._unsafeUnwrap()).toBeUndefined();

        const deleted = await answered(
            "10.delete_room.request",
            "10.delete_room.response",
            (client) => client.chat.deleteRoom({ roomId: "room" }),
        );
        expect(deleted._unsafeUnwrap()).toBeUndefined();

        const update: T.ChatFeedUpdate = { tag: "New", value: message };
        const fed = await answered("11.update_feed.request", "11.update_feed.response", (client) =>
            client.chat.updateFeed({ roomId: "room", update }),
        );
        expect(fed._unsafeUnwrap()).toBeUndefined();
        const notFed = await answered("11.update_feed.request", "11.update_feed.error", (client) =>
            client.chat.updateFeed({ roomId: "room", update }),
        );
        expect(notFed._unsafeUnwrapErr()).toEqual(notOwner);

        const configured = await answered(
            "7.set_room_config.request",
            "7.set_room_config.response",
            (client) => client.chat.setRoomConfig({ roomId: "room", config }),
        );
        expect(configured._unsafeUnwrap()).toBeUndefined();
    });

    it("answers the host's queries with the frames the Rust snapshot pins", async () => {
        expect(
            await answering("12.messages_query.start", (client) =>
                client.chat.onMessagesQuery((_query, send) =>
                    send({ messages: [message], next: "c0" }),
                ),
            ),
        ).toEqual([pinned("12.messages_query.receive")]);
        expect(
            await answering("12.messages_query.start", (client) =>
                client.chat.onMessagesQuery((_query, _send, interrupt) =>
                    interrupt({
                        tag: "Domain",
                        value: { tag: "V1", value: { tag: "NotAvailable" } },
                    }),
                ),
            ),
        ).toEqual([pinned("12.messages_query.error")]);

        expect(
            await answering("13.user_action.start", (client) =>
                client.chat.onUserAction((_action, send) =>
                    send({ tag: "MessageCreated", value: message }),
                ),
            ),
        ).toEqual([pinned("13.user_action.receive")]);
        expect(
            await answering("13.user_action.accept", (client) =>
                client.chat.onUserAction((_action, send) => send({ tag: "Acknowledged" })),
            ),
        ).toEqual([pinned("13.user_action.acknowledged")]);
        expect(
            await answering("13.user_action.pay", (client) =>
                client.chat.onUserAction((_action, _send, interrupt) =>
                    interrupt({
                        tag: "Domain",
                        value: { tag: "V1", value: { tag: "Rejected", value: { reason: "no" } } },
                    }),
                ),
            ),
        ).toEqual([pinned("13.user_action.error")]);

        expect(
            await answering("14.reactions_query.start", (client) =>
                client.chat.onReactionsQuery((_query, send) =>
                    send({
                        reactions: [
                            { participantId: "p1", emoji: "👍", timestamp: 1_700_000_000_000n },
                        ],
                    }),
                ),
            ),
        ).toEqual([pinned("14.reactions_query.receive")]);
    });

    it("decodes the first requests of createRoom, setRoomFooter and postMessage", () => {
        const payload = (name: string): Uint8Array => {
            // The request id "p:1", then the trait, method and message type.
            return hexToBytes(snapshot.get(name)!).slice(4 + 3);
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
});
