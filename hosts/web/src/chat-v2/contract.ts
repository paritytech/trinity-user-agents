import type { ChatActions, ContactHandle, HexString } from "@parity/truapi";
import type { CallErrorValue } from "@parity/truapi/scale";

/**
 * DRAFT, NOT TRUAPI. The HOST–PRODUCT contract of Chat Extension v2, as
 * `paritytech/trinity-user-agents` branch `lc/chat-product` defines it at
 * commit 3b2316285 (`rust/crates/truapi/src/api/chat.rs`,
 * `src/v01/chat.rs`, `api/call_media.rs`, `truapi-host/src/chat-queries.ts`,
 * and the RFCs `chat-extension-v2.md`, `chat-extension-v2-additions.md` and
 * `call-media.md`).
 *
 * The types below copy those shapes in the form the TrUAPI TypeScript codegen
 * gives Rust types: camelCase fields, `{ tag, value }` enums, unit enums as
 * strings, `Option` as an optional field, `u64` as `bigint`, and bytes as a
 * hex string. This repository's TrUAPI does not define them yet, and nothing
 * here changes it. They travel over a page-local `MessagePort` between the
 * host page and a product's `Worker` frame (see {@link ChatV2Envelope}), so
 * nothing here is a TrUAPI frame, nothing reaches the core, and the core
 * grants no authority for them.
 *
 * Errors are not versioned on the bridge: a domain error is `{ tag: "Domain",
 * value: <error> }` without the `V1`/`V2` wrapper of the real wire.
 */

// --- Resources and media -------------------------------------------------

/** Bytes the host shows: an avatar, an icon or a media item. */
export type ChatResource =
  | { tag: "Inline"; value: { mimeType: string; bytes: HexString } }
  | { tag: "Preimage"; value: { key: HexString } }
  | {
      tag: "Sealed";
      value: {
        parts: HexString[];
        key: HexString;
        nonce: HexString;
        size: bigint;
        digest: HexString;
      };
    }
  | {
      tag: "Hop";
      value: {
        identifier: HexString;
        ticket: HexString;
        node: string;
        size: bigint;
      };
    };

/** What a timeline needs to draw a media item before its bytes arrive. */
export interface ChatMediaMeta {
  mimeType: string;
  sizeBytes?: bigint;
  width?: number;
  height?: number;
  placeholder?: string;
  alt?: string;
}

/** One media item of a message. */
export type ChatMediaItem =
  | { tag: "Image"; value: { resource: ChatResource; meta: ChatMediaMeta } }
  | { tag: "Video"; value: { resource: ChatResource; meta: ChatMediaMeta } }
  | {
      tag: "File";
      value: { resource: ChatResource; meta: ChatMediaMeta; fileName: string };
    };

// --- Rooms ----------------------------------------------------------------

/** Name, avatar and description of a room. */
export interface ChatRoomMetadata {
  name: string;
  avatar?: ChatResource;
  description?: string;
}

export type ChatRoomOrder = "Timestamp" | "PinToTop";

export type ChatRoomBadge =
  | { tag: "Hidden"; value?: undefined }
  | { tag: "UnreadCount"; value: { count: number } }
  | { tag: "Image"; value: { resource: ChatResource } };

export interface ChatRoomPreviewText {
  text: string;
  /** The line is an activity that continues, such as "Alice is typing". */
  ongoing: boolean;
}

/** How a room shows in the chat list. A field left out keeps the host default. */
export interface ChatRoomPreview {
  order: ChatRoomOrder;
  timestamp?: bigint;
  badge?: ChatRoomBadge;
  text?: ChatRoomPreviewText;
}

/** The parts of a room the product serves itself. */
export interface ChatRoomConfig {
  /** The product keeps the messages and answers `messages_query` and `user_action`. */
  productSource: boolean;
  reactionDetails: boolean;
  /** The host asks `participants_query`. */
  participants: boolean;
  messageActions: boolean;
  allowedReactions?: string[];
  placeholder?: string;
  /** The host sends `Seen` when the user sees the newest message. */
  reportSeen: boolean;
}

/** `create_room` at `V2`: RFC Chat Extension v2, "Versioning". */
export interface HostChatCreateRoomRequest {
  roomId: string;
  metadata: ChatRoomMetadata;
  config: ChatRoomConfig;
  preview?: ChatRoomPreview;
}

export interface HostChatCreateRoomResponse {
  status: "New" | "Exists";
}

export type HostChatCreateRoomError =
  | { tag: "PermissionDenied"; value?: undefined }
  | { tag: "Unknown"; value: { reason: string } };

export interface HostChatSetRoomMetadataRequest {
  roomId: string;
  metadata: ChatRoomMetadata;
}

export interface HostChatSetRoomPreviewRequest {
  roomId: string;
  preview: ChatRoomPreview;
}

export interface HostChatDeleteRoomRequest {
  roomId: string;
}

/** Failure of a request that changes a room. */
export type HostChatRoomError =
  | { tag: "UnknownRoom"; value?: undefined }
  | { tag: "NotRoomHost"; value?: undefined }
  | { tag: "RateLimited"; value?: undefined }
  | { tag: "Unknown"; value: { reason: string } };

// --- Messages -------------------------------------------------------------

/** Who wrote a message. `"user"` is the host's user, `"product"` the product. */
export interface ChatMessageOrigin {
  participantId: string;
}

export type ChatMessageStatus =
  | "Preparing"
  | "Sent"
  | "Delivered"
  | "Read"
  | "Failed";

export type ChatMessageBody =
  | { tag: "Text"; value: { text: string } }
  | { tag: "RichText"; value: { text?: string; media: ChatMediaItem[] } }
  | { tag: "Actions"; value: ChatActions }
  | {
      tag: "Custom";
      value: { messageType: string; payload: HexString; previewText?: string };
    }
  /** A line about the room, drawn as a system row. */
  | { tag: "Notice"; value: { text: string } };

export interface ChatReactionSummary {
  emoji: string;
  count: number;
  reactedByUser: boolean;
}

export interface ChatMessage {
  /** Chosen by the product, unique in the room. */
  messageId: string;
  origin: ChatMessageOrigin;
  /** Milliseconds since the Unix epoch. */
  timestamp: bigint;
  body: ChatMessageBody;
  status: ChatMessageStatus;
  replyTo?: string;
  editedAt?: bigint;
  reactions: ChatReactionSummary[];
}

/** One change to the messages of a room. */
export type ChatFeedUpdate =
  | { tag: "New"; value: ChatMessage }
  | {
      tag: "Edited";
      value: { messageId: string; body: ChatMessageBody; editedAt: bigint };
    }
  | { tag: "Deleted"; value: { messageId: string } }
  | {
      tag: "ReactionPlaced";
      value: { messageId: string; participantId: string; emoji: string };
    }
  | {
      tag: "ReactionRemoved";
      value: { messageId: string; participantId: string; emoji: string };
    }
  | {
      tag: "StatusChanged";
      value: { messageId: string; status: ChatMessageStatus };
    };

export interface HostChatUpdateFeedRequest {
  roomId: string;
  update: ChatFeedUpdate;
}

export type HostChatUpdateFeedError =
  | { tag: "UnknownRoom"; value?: undefined }
  | { tag: "NotRoomHost"; value?: undefined }
  | { tag: "UnknownMessage"; value?: undefined }
  | { tag: "DuplicateMessage"; value?: undefined }
  | { tag: "MessageTooLarge"; value?: undefined }
  | { tag: "NotOwner"; value?: undefined }
  | { tag: "RateLimited"; value?: undefined }
  | { tag: "Unknown"; value: { reason: string } };

// --- Host-initiated queries -------------------------------------------------

export type ChatPageAnchor =
  /** Newer than the cursor. No cursor starts at the oldest message. */
  | { tag: "After"; value: { cursor?: string } }
  /** Older than the cursor. No cursor starts at the newest message. */
  | { tag: "Before"; value: { cursor?: string } };

export interface ProductChatMessagesQueryRequest {
  roomId: string;
  anchor: ChatPageAnchor;
  pageSize: number;
}

export interface ProductChatMessagesQueryItem {
  /** Oldest first. */
  messages: ChatMessage[];
  /** Cursor for the next page in the same direction. Absent ends the direction. */
  next?: string;
}

export type ProductChatQueryError =
  | { tag: "UnknownRoom"; value?: undefined }
  | { tag: "NotAvailable"; value?: undefined }
  | { tag: "Unknown"; value: { reason: string } };

export type ChatUserAction =
  | { tag: "NewMessage"; value: { body: ChatMessageBody; replyTo?: string } }
  | { tag: "EditMessage"; value: { messageId: string; body: ChatMessageBody } }
  | { tag: "DeleteMessage"; value: { messageId: string } }
  | { tag: "ReactionPlaced"; value: { messageId: string; emoji: string } }
  | { tag: "ReactionRemoved"; value: { messageId: string; emoji: string } }
  | { tag: "Seen"; value: { messageId: string } }
  | { tag: "Retry"; value: { messageId: string } };

export interface ProductChatUserActionRequest {
  roomId: string;
  action: ChatUserAction;
}

export type ProductChatUserActionItem =
  /** For `NewMessage`: the message as the product stored it. */
  | { tag: "MessageCreated"; value: ChatMessage }
  | { tag: "Acknowledged"; value?: undefined };

export type ProductChatUserActionError =
  | { tag: "UnknownRoom"; value?: undefined }
  | { tag: "UnknownMessage"; value?: undefined }
  /** The host shows the reason to the user. */
  | { tag: "Rejected"; value: { reason: string } }
  | { tag: "Unknown"; value: { reason: string } };

export type ChatParticipant =
  | { tag: "Person"; value: { personId: number } }
  | { tag: "LitePerson"; value: { account: HexString } }
  | { tag: "Contact"; value: { handle: ContactHandle } }
  | { tag: "Product"; value: { productId: string } }
  | { tag: "Custom"; value: { nickname: string; avatar?: ChatResource } };

export interface ChatParticipantEntry {
  participantId: string;
  participant: ChatParticipant;
}

export type ChatParticipantsQuery =
  | { tag: "Count"; value?: undefined }
  | { tag: "Page"; value: { cursor?: string; pageSize: number } }
  | { tag: "ByIds"; value: { participantIds: string[] } };

export interface ProductChatParticipantsQueryRequest {
  roomId: string;
  query: ChatParticipantsQuery;
}

export type ProductChatParticipantsQueryItem =
  | { tag: "Count"; value: { count: number } }
  | {
      tag: "Page";
      value: { participants: ChatParticipantEntry[]; next?: string };
    }
  /** An id the product does not know is absent. */
  | { tag: "Resolved"; value: { participants: ChatParticipantEntry[] } };

// --- Capabilities, features and calls -----------------------------------------

export interface HostChatCapabilitiesResponse {
  /** Names of the features this host serves from end to end. */
  features: string[];
  inlineAttachmentMaxBytes: number;
  preimageAttachmentMaxBytes: number;
  sealedAttachmentMaxBytes: number;
}

export interface ChatRoomCalls {
  audio: boolean;
  video: boolean;
}

export interface ChatRoomFeatures {
  preimageAttachments: boolean;
  search: boolean;
  /** The call buttons in the header. Absent shows none. */
  calls?: ChatRoomCalls;
  readAloud: boolean;
  sealedAttachments: boolean;
  hopAttachments: boolean;
}

export interface HostChatSetRoomFeaturesRequest {
  roomId: string;
  features: ChatRoomFeatures;
}

export type ChatRoomCall =
  /** An incoming call the product received. The host rings in the room. */
  | {
      tag: "Ringing";
      value: { callId: string; video: boolean; caller?: string };
    }
  /** A call that continues. With `mediaSession` the host shows that Media session. */
  | {
      tag: "Active";
      value: {
        callId: string;
        video: boolean;
        startedAt: bigint;
        mediaSession?: HexString;
      };
    };

export interface HostChatSetRoomCallRequest {
  roomId: string;
  /** Absent removes the call. */
  call?: ChatRoomCall;
}

/** What the user did with the optional features of a room. */
export type ChatRoomEvent =
  | {
      tag: "UserStateChanged";
      value: { mutedUntil?: bigint; archived: boolean };
    }
  | { tag: "Woken"; value: { watchId: string } }
  | { tag: "CallRequested"; value: { video: boolean } }
  | { tag: "CallAnswered"; value: { callId: string; accept: boolean } }
  | { tag: "CallOpened"; value: { callId: string } }
  | { tag: "CallEnded"; value: { callId: string } }
  /** A media signal of this host for the other side. The product carries it there. */
  | { tag: "CallSignal"; value: { callId: string; signal: HexString } };

export interface HostChatRoomEventsSubscribeItem {
  roomId: string;
  event: ChatRoomEvent;
}

export interface HostCallMediaStartRequest {
  roomId: string;
  /** The call the room shows as `Active`. */
  callId: string;
  video: boolean;
  /** This side makes the offer of the media. */
  offerer: boolean;
}

export interface HostCallMediaSignalRequest {
  roomId: string;
  callId: string;
  /** A `CallSignal` the host of the other side gave its product. */
  signal: HexString;
}

export interface HostCallMediaStopRequest {
  roomId: string;
  callId: string;
}

export type HostCallMediaError =
  /** The user pressed no call button and accepted no call in the room shortly before. */
  | { tag: "NotAllowed"; value?: undefined }
  | { tag: "InvalidRequest"; value: { reason: string } }
  | { tag: "Unknown"; value: { reason: string } };

// --- Limits and names ---------------------------------------------------------

/** Feature names of `capabilities`, from the additions RFC. */
export const FEATURE = {
  productSourceRooms: "product_source_rooms",
  participants: "participants",
  inlineAttachments: "inline_attachments",
  callButtons: "call_buttons",
  roomCalls: "room_calls",
} as const;

/** The largest inline media item, from the base RFC's limits. */
export const INLINE_MEDIA_MAX_BYTES = 1024 * 1024;
/** Messages of one room the host keeps in its page cache. */
export const ROOM_CACHE_LIMIT = 500;
/** Events kept for a product with no `room_events_subscribe` open. */
export const ROOM_EVENT_QUEUE = 64;
/** How long after a call press or an accept `callMedia.start` is allowed. */
export const CALL_PRESS_WINDOW_MS = 180_000;
/** Bounds of the call media RFC. */
export const CALL_ID_MAX_BYTES = 128;
export const CALL_SIGNAL_MAX_BYTES = 64 * 1024;

/** The time limits of the base RFC for each query, in milliseconds. */
export const QUERY_TIMEOUTS_MS = {
  messages: 10_000,
  newMessage: 30_000,
  userAction: 10_000,
  details: 5_000,
} as const;

// --- The draft bridge -------------------------------------------------------

/** Version of the page-local bridge. Bumped on any change to its messages. */
export const CHAT_V2_DRAFT = "truapi-chat-v2-draft/0";
/** `type` of the window message a product frame sends to hand the host its port. */
export const CHAT_V2_CONNECT = "truapi-chat-v2-draft";

/** The hello a product frame posts to its parent, with one `MessagePort` transferred. */
export interface ChatV2Connect {
  type: typeof CHAT_V2_CONNECT;
  protocol: typeof CHAT_V2_DRAFT;
}

/** Why a query gave no item, as `chat-queries.ts` names it. */
export type ChatQueryRefusal<E> =
  | CallErrorValue<E>
  | { tag: "Timeout"; value?: undefined }
  | { tag: "NoAnswer"; value?: undefined };

/** Calls a product makes on the host, by method, with their request, answer and domain error. */
export interface ProductCalls {
  createRoom: [
    HostChatCreateRoomRequest,
    HostChatCreateRoomResponse,
    HostChatCreateRoomError,
  ];
  setRoomMetadata: [
    HostChatSetRoomMetadataRequest,
    Record<string, never>,
    HostChatRoomError,
  ];
  setRoomPreview: [
    HostChatSetRoomPreviewRequest,
    Record<string, never>,
    HostChatRoomError,
  ];
  setRoomFeatures: [
    HostChatSetRoomFeaturesRequest,
    Record<string, never>,
    HostChatRoomError,
  ];
  setRoomCall: [
    HostChatSetRoomCallRequest,
    Record<string, never>,
    HostChatRoomError,
  ];
  deleteRoom: [
    HostChatDeleteRoomRequest,
    Record<string, never>,
    HostChatRoomError,
  ];
  updateFeed: [
    HostChatUpdateFeedRequest,
    Record<string, never>,
    HostChatUpdateFeedError,
  ];
  capabilities: [
    Record<string, never>,
    HostChatCapabilitiesResponse,
    { reason: string },
  ];
  /** Start the product's `room_events_subscribe` stream. A second one replaces the first. */
  roomEventsSubscribe: [
    Record<string, never>,
    Record<string, never>,
    { reason: string },
  ];
  callMediaStart: [
    HostCallMediaStartRequest,
    Record<string, never>,
    HostCallMediaError,
  ];
  callMediaSignal: [
    HostCallMediaSignalRequest,
    Record<string, never>,
    HostCallMediaError,
  ];
  callMediaStop: [
    HostCallMediaStopRequest,
    Record<string, never>,
    HostCallMediaError,
  ];
}

export type ProductMethod = keyof ProductCalls;

/** Queries a host asks a product, by method, with their request, item and domain error. */
export interface HostQueries {
  messagesQuery: [
    ProductChatMessagesQueryRequest,
    ProductChatMessagesQueryItem,
    ProductChatQueryError,
  ];
  userAction: [
    ProductChatUserActionRequest,
    ProductChatUserActionItem,
    ProductChatUserActionError,
  ];
  participantsQuery: [
    ProductChatParticipantsQueryRequest,
    ProductChatParticipantsQueryItem,
    ProductChatQueryError,
  ];
}

export type QueryMethod = keyof HostQueries;

/** Everything on the bridge port, both ways. */
export type ChatV2Envelope =
  // Product to host.
  | {
      [M in ProductMethod]: {
        kind: "call";
        id: number;
        method: M;
        request: ProductCalls[M][0];
      };
    }[ProductMethod]
  | { kind: "answer"; id: number; item: unknown }
  | { kind: "refusal"; id: number; error: CallErrorValue<unknown> }
  | { kind: "end"; id: number }
  // Host to product.
  | { kind: "result"; id: number; ok: true; value: unknown }
  | { kind: "result"; id: number; ok: false; error: CallErrorValue<unknown> }
  | {
      [M in QueryMethod]: {
        kind: "query";
        id: number;
        method: M;
        request: HostQueries[M][0];
      };
    }[QueryMethod]
  | { kind: "cancel"; id: number }
  | { kind: "roomEvent"; item: HostChatRoomEventsSubscribeItem };

/** One end of the bridge. */
export interface BridgePort {
  post(message: ChatV2Envelope): void;
  listen(handler: (message: ChatV2Envelope) => void): void;
  close(): void;
}

/** A bridge end over a `MessagePort`. */
export function messagePortBridge(port: MessagePort): BridgePort {
  return {
    post: (message) => port.postMessage(message),
    listen: (handler) => {
      port.onmessage = (event: MessageEvent<ChatV2Envelope>) => {
        if (typeof event.data === "object" && event.data !== null)
          handler(event.data);
      };
      port.start();
    },
    close: () => port.close(),
  };
}
