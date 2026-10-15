// The chat part of the mock host.
//
// It has two halves. The callbacks are the host seam the core calls: they
// keep every room, its declarations and its messages in memory. A room the
// host stores keeps its messages ("persist"); a room the product keeps has
// only the messages on its screen, the pages the host asked for and the feed
// updates the product pushed. The user half plays the person and the host
// screen: it opens rooms, sends, edits, reacts, opens menus and presses
// buttons. In a room the product keeps, it asks the product through the
// chat queries; in a room the host stores, it changes the store and sends the
// matching event. It also plays the user of the optional room features: the
// call buttons and the call bar, the branch control, the search field, mute
// and archive, and wakes. It applies the host rules of RFC §3.6, §4 and §5.3: time
// limits, one retry after `NotAvailable`, the host defaults, the optimistic
// message and its `Failed` and `Rejected` ends.
//
// The user half needs the host runtime of the product, which only the code
// that builds the core has. That code attaches it with `attach`.

import type {
  ChatBuiltInMessageAction,
  ChatCommand,
  ChatEvent,
  ChatFeedUpdate,
  ChatMessage,
  ChatMessageBody,
  ChatMessageContent,
  ChatMessageMeta,
  ChatParticipantEntry,
  ChatParticipantsQuery,
  ChatProductAction,
  ChatReactionDetail,
  ChatReactionSummary,
  ChatRoom,
  ChatRoomBadge,
  ChatRoomCall,
  ChatRoomConfig,
  ChatRoomEvent,
  ChatRoomFeatures,
  ChatRoomMetadata,
  ChatRoomOrder,
  ChatRoomPreview,
  ChatActivity,
  ChatCommandSpec,
  ChatSuggestion,
  ChatToolbarAction,
  ChatUserAction,
  HostChatActionSubscribeItem,
  HostChatListSubscribeItem,
  HostChatPostMessageResponse,
  HostChatRegisterBotRequest,
  HostChatRoomEventsSubscribeItem,
  ProductChatParticipantsQueryItem,
  ProductChatQueryError,
  ProductChatUserActionError,
  V02ChatRoomFooter,
} from "@parity/truapi";
import type { GenericError, Result } from "@parity/truapi";

import type {
  ChatHostRuntime,
  ChatQueryName,
  ChatQueryRefusal,
  ChatQueryResult,
  ChatWakeOutcome,
  ChatWakeReason,
} from "../chat-queries.js";
import { CHAT_QUERY_TIMEOUTS_MS } from "../chat-queries.js";
import type { ChatPlatform } from "../generated/host-callbacks.js";

/** The participant id of the host's user. */
export const MOCK_CHAT_USER = "user";
/** The participant id of the product itself. */
export const MOCK_CHAT_PRODUCT = "product";
/** The `peer` of every event the mock host sends, as on iOS (open item O5). */
export const MOCK_CHAT_PEER = "native";
/** The page size the mock host asks for, the host default of RFC §3.5. */
export const MOCK_CHAT_PAGE_SIZE = 40;
/** The participant page size the mock host asks for (RFC §3.5). */
export const MOCK_CHAT_PARTICIPANT_PAGE_SIZE = 100;

/**
 * The Chat feature names of the additions RFC. The mock lists every one; the
 * core removes the names it cannot back.
 */
export const MOCK_CHAT_FEATURE_NAMES: readonly string[] = [
  "product_source_rooms",
  "reaction_details",
  "participants",
  "message_menus",
  "participant_menus",
  "toolbar",
  "suggestions",
  "commands",
  "activity",
  "custom_bodies",
  "custom_footer",
  "inline_attachments",
  "payment_button",
  "room_links",
  "preimage_attachments",
  "search",
  "bot_personas",
  "mute_archive",
  "threads",
  "branches",
  "reasoning_rows",
  "read_aloud",
  "wake",
  "call_buttons",
  "room_calls",
];

/** The page of search matches the host asks for (additions RFC limits). */
export const MOCK_CHAT_SEARCH_PAGE_SIZE = 30;

/** The features of a room that declared none. */
export function defaultChatRoomFeatures(): ChatRoomFeatures {
  return { preimageAttachments: false, search: false, readAloud: false };
}

/** The mute and archive state the user gave one room. */
export interface MockChatRoomUserState {
  /** Milliseconds since the epoch when the mute ends; `undefined`: not muted. */
  mutedUntil?: bigint;
  archived: boolean;
}

/** The configuration of a room that declared nothing (RFC §4). */
export function defaultChatRoomConfig(): ChatRoomConfig {
  return {
    productSource: false,
    reactionDetails: false,
    participants: false,
    messageActions: false,
    participantActions: false,
    reportSeen: false,
  };
}

/** One message the product posted through `post_message`. */
export interface ChatMessageRecord {
  /** Id the mock assigned and returned to the product. */
  messageId: string;
  /** Room the message was posted to. */
  roomId: string;
  /** What was posted. */
  payload: ChatMessageContent;
}

/** One feed update the core gave the host, with the mode of the room. */
export interface MockChatFeedUpdate {
  update: ChatFeedUpdate;
  /** `true`: the host stores the room, so it stored the change. */
  persist: boolean;
}

/** What the product declared for one room. */
export interface MockChatRoom {
  roomId: string;
  participatingAs: ChatRoom["participatingAs"];
  metadata: ChatRoomMetadata;
  /** The `https` icon of a caller at the first protocol version, or `""`. */
  iconUrl: string;
  /** The configuration in use: the last one declared, or the default. */
  config: ChatRoomConfig;
  /** The last preview, or `undefined` before the product set one. */
  preview?: ChatRoomPreview;
  /** The last footer, or `undefined` for the host default input. */
  footer?: V02ChatRoomFooter;
  toolbar: ChatToolbarAction[];
  suggestions: ChatSuggestion[];
  commands: ChatCommandSpec[];
  /** The activity line, or `undefined` when there is none. */
  activity?: ChatActivity;
  /** Every feed update, in order. */
  updates: MockChatFeedUpdate[];
  /** The optional features in use: the last ones declared, or the default. */
  features: ChatRoomFeatures;
  /** The meta of each message, by message id. */
  meta: Record<string, ChatMessageMeta>;
  /** The call of the room, or `undefined` for none. */
  call?: ChatRoomCall;
}

/** The chat list row of a room, as the host draws it. */
export interface MockChatListRow {
  roomId: string;
  order: ChatRoomOrder;
  /** The sort key: the declared time, or the time of the newest message. */
  timestamp?: bigint;
  /**
   * The declared badge, or the host unread count of a room the host stores.
   * `undefined` shows no badge.
   */
  badge?: ChatRoomBadge;
  /** The line: the declared text, or the line of the newest message. */
  text?: string;
  /** `true` draws the line as an activity, not as the newest message. */
  ongoing: boolean;
}

/** One query the user half asked, with its result. */
export interface MockChatQueryRecord {
  name: ChatQueryName;
  request: unknown;
  result: ChatQueryResult<unknown, unknown>;
}

/** A line the host showed to the user, such as the reason of a rejection. */
export interface MockChatNotice {
  roomId: string;
  text: string;
}

/** A message menu, as the host draws it. */
export interface MockMessageMenu {
  /** The host entries, in host order. */
  builtIn: ChatBuiltInMessageAction[];
  /** The product entries, in their own section. */
  actions: ChatProductAction[];
  /** `product` when the product answered, `default` for the host menu. */
  source: "product" | "default";
}

/** The host entry of a participant menu for each participant kind (RFC §4). */
export type MockParticipantBuiltIn =
  | "OpenProfile"
  | "CopyAddress"
  | "OpenContact"
  | "OpenProduct";

/** A participant menu, as the host draws it. */
export interface MockParticipantMenu {
  /** The host entry for the participant kind, or `undefined` for none. */
  builtIn?: MockParticipantBuiltIn;
  actions: ChatProductAction[];
  source: "product" | "default";
}

/** What became of a message the user sent. */
export type MockSendOutcome =
  /** The message is in the room: stored by the host, or created by the product. */
  | { tag: "Posted"; message: ChatMessage }
  /** The text was a declared command: the product got `Command`, no message. */
  | { tag: "Command"; command: ChatCommand }
  /** The product rejected the message. The host removed it and showed `reason`. */
  | { tag: "Rejected"; reason: string }
  /** No answer came, or the product failed. The message shows `Failed`. */
  | {
      tag: "Failed";
      messageId: string;
      refusal: ChatQueryRefusal<ProductChatUserActionError>;
    };

/** What became of an edit, delete, reaction, seen report or retry. */
export type MockActionOutcome =
  | { tag: "Applied" }
  /** The product refused or did not answer. The host reverted the change. */
  | {
      tag: "Reverted";
      refusal: ChatQueryRefusal<ProductChatUserActionError>;
    };

/** The time limits and the retry delay of the user half, in milliseconds. */
export interface MockChatTimings {
  /** `messages_query` (RFC §3.6: 10 s). */
  messages: number;
  /** `user_action` with `NewMessage` (30 s). */
  newMessage: number;
  /** `user_action` with any other action (10 s). */
  userAction: number;
  /** The reactions, participants and menu queries (5 s). */
  details: number;
  /** The wait before the one retry after `NotAvailable` (RFC §5.1: 1 s). */
  retryDelay: number;
  /** `search_query` (10 s). */
  search: number;
}

/** The default timings, from RFC §3.6 and §5.1. */
export const MOCK_CHAT_TIMINGS: MockChatTimings = {
  messages: CHAT_QUERY_TIMEOUTS_MS.messages,
  newMessage: CHAT_QUERY_TIMEOUTS_MS.newMessage,
  userAction: CHAT_QUERY_TIMEOUTS_MS.userAction,
  details: CHAT_QUERY_TIMEOUTS_MS.details,
  retryDelay: 1_000,
  search: CHAT_QUERY_TIMEOUTS_MS.search,
};

/**
 * The user and the host screen. Every method needs a product attached with
 * `attach`, except where its documentation says otherwise.
 */
export interface MockChatUser {
  /**
   * Give the host runtime of the product to the user half, in place of the
   * one it has. The result removes this runtime again, if no other runtime
   * replaced it. The test client of `@parity/truapi-host/testing/client`
   * attaches its own runtime.
   */
  attach(product: ChatHostRuntime): () => void;
  /** Change the time limits and the retry delay. */
  setTimings(timings: Partial<MockChatTimings>): void;
  /**
   * Open the room screen: send `RoomOpened`, and in a room the product keeps,
   * ask for the newest page. Gives the messages on the screen.
   */
  open(roomId: string): Promise<ChatMessage[]>;
  /** Close the room screen: send `RoomClosed`. The page cache stays. */
  close(roomId: string): Promise<void>;
  /** `true` while the room screen is open. Needs no product. */
  isOpen(roomId: string): boolean;
  /**
   * Ask for the page before the oldest one on the screen. Gives `false`, with
   * no query, when the product said there is no older page.
   */
  loadOlder(roomId: string): Promise<boolean>;
  /**
   * Send a text or a body. A text `/name rest` with a declared command name
   * sends `Command` and makes no message.
   */
  send(
    roomId: string,
    input: string | ChatMessageBody,
    options?: { replyTo?: string },
  ): Promise<MockSendOutcome>;
  edit(
    roomId: string,
    messageId: string,
    input: string | ChatMessageBody,
  ): Promise<MockActionOutcome>;
  remove(roomId: string, messageId: string): Promise<MockActionOutcome>;
  react(
    roomId: string,
    messageId: string,
    emoji: string,
    placed?: boolean,
  ): Promise<MockActionOutcome>;
  /**
   * Report the newest message on the screen as seen. The product gets the
   * report only when the room sets `reportSeen`.
   */
  seen(roomId: string, messageId?: string): Promise<MockActionOutcome>;
  /** Ask the product to send a `Failed` message again. */
  retry(roomId: string, messageId: string): Promise<MockActionOutcome>;
  /** Press a button of an `Actions` message. */
  pressAction(
    roomId: string,
    messageId: string,
    actionId: string,
    payload?: `0x${string}`,
  ): Promise<void>;
  /** Press a toolbar button. A deeplink opens; a callback sends its event. */
  pressToolbar(roomId: string, actionId: string): Promise<void>;
  /**
   * Press a quick reply. A `sendsText` reply sends its text; the other sends
   * `SuggestionSelected`.
   */
  pickSuggestion(
    roomId: string,
    suggestionId: string,
  ): Promise<MockSendOutcome | undefined>;
  /** The menu of one message on the screen or in the store. */
  messageMenu(roomId: string, messageId: string): Promise<MockMessageMenu>;
  /** Select a product entry of a message menu. */
  pickMessageAction(
    roomId: string,
    messageId: string,
    action: ChatProductAction,
  ): Promise<void>;
  /**
   * The participants of a room. A room that does not declare participants
   * has two: the user and the product. These reserved ids resolve with no
   * query, so the count is 2, and a page or a lookup gives no entry.
   */
  participants(
    roomId: string,
    query: ChatParticipantsQuery,
  ): Promise<ProductChatParticipantsQueryItem>;
  /** The menu of one participant. */
  participantMenu(
    roomId: string,
    entry: ChatParticipantEntry,
  ): Promise<MockParticipantMenu>;
  /** Select a product entry of a participant menu. */
  pickParticipantAction(
    roomId: string,
    participantId: string,
    action: ChatProductAction,
  ): Promise<void>;
  /**
   * Who reacted to a message, or `undefined` when the host hides the
   * affordance: the room does not set `reactionDetails`, or the product
   * answers `NotAvailable` twice.
   */
  whoReacted(
    roomId: string,
    messageId: string,
  ): Promise<ChatReactionDetail[] | undefined>;
  /** Send one event as it is. */
  publish(roomId: string, event: ChatEvent): Promise<void>;
  /**
   * Press the audio or the video call button of the header. Throws when the
   * room declares no such button.
   */
  pressCall(roomId: string, video: boolean): Promise<void>;
  /** Accept or decline the call that rings in the room. */
  answerCall(roomId: string, accept: boolean): Promise<void>;
  /** Tap the call bar of an active call without a media session. */
  openCall(roomId: string): Promise<void>;
  /** Press "End" on the call bar of an active call. */
  endCall(roomId: string): Promise<void>;
  /**
   * Pick the alternative `index` of a message with a branch position. Throws
   * when the message has no branch or `index` is out of its range.
   */
  selectBranch(roomId: string, messageId: string, index: number): Promise<void>;
  /**
   * Search the room. The host searches a room it stores itself. It asks a
   * room the product keeps through `search_query` when the room features
   * set `search`. Gives the matches newest first, or `undefined` when the
   * room shows no search or the product gives no matches ("Search is not
   * available").
   */
  search(
    roomId: string,
    text: string,
    cursor?: string,
  ): Promise<{ hits: ChatMessage[]; next?: string } | undefined>;
  /** Wake the product, as a host does for a message or a notification. */
  wake(reason: ChatWakeReason): Promise<ChatWakeOutcome>;
  /** Send one room event as it is. */
  publishRoomEvent(roomId: string, event: ChatRoomEvent): Promise<void>;
  /** The queries the user half asked, in order. Needs no product. */
  queries(): MockChatQueryRecord[];
  /** The lines the host showed, in order. Needs no product. */
  notices(): MockChatNotice[];
  /** The deeplinks the host opened, in order. Needs no product. */
  deeplinks(): string[];
}

/** The chat state of the mock host and its two halves. */
export interface MockChat {
  /** The host seam the core calls. */
  callbacks: Required<ChatPlatform>;
  user: MockChatUser;
  /** The rooms, in id order, as `subscribeChatRooms` lists them. */
  chatRooms(): ChatRoom[];
  chatBots(): HostChatRegisterBotRequest[];
  postedMessages(): ChatMessageRecord[];
  /** What the product declared for every room, in room id order. */
  rooms(): MockChatRoom[];
  /** What the product declared for one room, or `undefined` for none. */
  room(roomId: string): MockChatRoom | undefined;
  /**
   * The messages of a room: the store of a room the host stores, the screen
   * and page cache of a room the product keeps. Oldest first.
   */
  feed(roomId: string): ChatMessage[];
  /** The chat list row of a room, or `undefined` for no room. */
  preview(roomId: string): MockChatListRow | undefined;
  /** Every event the user half sent to the product, in order. */
  events(): HostChatActionSubscribeItem[];
  /** Every room event the user half sent to the product, in order. */
  roomEvents(): HostChatRoomEventsSubscribeItem[];
  /** The Chat feature names the host lists, from its configuration. */
  features(): string[];
  /**
   * Play the user: mute a room until `mutedUntil` and archive it or not. A
   * product attached gets `UserStateChanged` at once. A product that starts
   * later gets the state when its room event subscription starts.
   */
  setRoomUserState(roomId: string, state: MockChatRoomUserState): Promise<void>;
  /** The mute and archive state of a room, or `undefined` for none. */
  roomUserState(roomId: string): MockChatRoomUserState | undefined;
  /** Remove every room, bot, message, event and recording. */
  clear(): void;
}

interface RoomState extends MockChatRoom {
  /** Store (host-store) or screen and page cache (product-source). */
  messages: ChatMessage[];
  /** The id of the newest message the user saw, for the host unread count. */
  lastSeen?: string;
  /** The cursor of the next older page, or `null` after the oldest page. */
  olderCursor?: string | null;
  /** Every page anchor the host asked for, as `Before:<cursor>` text. */
  pagesAsked: Set<string>;
  open: boolean;
}

/** Entries in key order, as the Rust mock's `BTreeMap` gives them. */
function byKey<T>(entries: Map<string, T>): T[] {
  return [...entries.entries()]
    .sort(([left], [right]) => (left < right ? -1 : left > right ? 1 : 0))
    .map(([, value]) => value);
}

function textBody(text: string): ChatMessageBody {
  return { tag: "Text", value: { text } };
}

/** The line a chat list shows for a message (RFC §4). */
function previewLine(message: ChatMessage): string {
  const { body } = message;
  switch (body.tag) {
    case "Text":
      return body.value.text;
    case "RichText":
      return (
        body.value.text ??
        body.value.media.find((item) => item.value.meta.alt !== undefined)
          ?.value.meta.alt ??
        "Sent a message"
      );
    case "Actions":
      return body.value.text ?? "Sent a message";
    case "Custom":
      return body.value.previewText ?? "Sent a message";
  }
}

/** Add or remove one reaction of one participant in a summary list. */
function adjustReactions(
  reactions: ChatReactionSummary[],
  participantId: string,
  emoji: string,
  placed: boolean,
): ChatReactionSummary[] {
  const byUser = participantId === MOCK_CHAT_USER;
  const next = reactions.map((summary) => ({ ...summary }));
  const at = next.findIndex((summary) => summary.emoji === emoji);
  if (placed) {
    if (at < 0) {
      next.push({ emoji, count: 1, reactedByUser: byUser });
    } else if (!(byUser && next[at]!.reactedByUser)) {
      next[at]!.count += 1;
      if (byUser) next[at]!.reactedByUser = true;
    }
    return next;
  }
  if (at < 0) return next;
  if (byUser && !next[at]!.reactedByUser) return next;
  next[at]!.count -= 1;
  if (byUser) next[at]!.reactedByUser = false;
  return next.filter((summary) => summary.count > 0);
}

/** A V1 `post_message` content as a message of the current version (RFC §13.10). */
function contentBody(content: ChatMessageContent): ChatMessageBody | undefined {
  switch (content.tag) {
    case "Text":
      return content;
    case "RichText":
      // The URL media of the first version is not a `ChatResource`.
      return {
        tag: "RichText",
        value: { text: content.value.text, media: [] },
      };
    case "Actions":
      return content;
    case "File":
      return {
        tag: "RichText",
        value: {
          text: content.value.text ?? content.value.fileName,
          media: [],
        },
      };
    case "Custom":
      return {
        tag: "Custom",
        value: {
          messageType: content.value.messageType,
          payload: content.value.payload,
        },
      };
    case "Reaction":
    case "ReactionRemoved":
      return undefined;
  }
}

/** Is this refusal a `NotAvailable` the host retries once (RFC §5.1)? */
function isNotAvailable(refusal: ChatQueryRefusal<{ tag: string }>): boolean {
  return refusal.tag === "Domain" && refusal.value.tag === "NotAvailable";
}

function describeRefusal(refusal: ChatQueryRefusal<unknown>): string {
  if (refusal.tag !== "Domain") return refusal.tag;
  const domain = refusal.value as { tag?: string; value?: { reason?: string } };
  return domain.value?.reason ?? domain.tag ?? "Domain";
}

/** How the mock host builds the room list subscription. */
export interface MockChatOptions {
  /** When set, every chat call fails with this reason. */
  chatError?: string;
  /**
   * The Chat feature names the host lists. Defaults to every name of
   * {@link MOCK_CHAT_FEATURE_NAMES}.
   */
  chatFeatures?: readonly string[];
  /**
   * Open a live room list: `first`, then every item `register` pushes. The
   * mock host gives the helper it uses for every live subscription.
   */
  liveRooms(
    first: HostChatListSubscribeItem,
    register: (push: (item: HostChatListSubscribeItem) => void) => () => void,
  ): AsyncIterable<Result<HostChatListSubscribeItem, GenericError>>;
  /** A room list subscription that fails at once with `reason`. */
  failedRooms(
    reason: string,
  ): AsyncIterable<Result<HostChatListSubscribeItem, GenericError>>;
}

/** Build the chat state of one mock host. */
export function createMockChat(options: MockChatOptions): MockChat {
  const { chatError } = options;
  const rooms = new Map<string, RoomState>();
  const bots = new Map<string, HostChatRegisterBotRequest>();
  const posted: ChatMessageRecord[] = [];
  const events: HostChatActionSubscribeItem[] = [];
  const roomEventLog: HostChatRoomEventsSubscribeItem[] = [];
  const userStates = new Map<string, MockChatRoomUserState>();
  const featureNames = [...(options.chatFeatures ?? MOCK_CHAT_FEATURE_NAMES)];
  const queryLog: MockChatQueryRecord[] = [];
  const noticeLog: MockChatNotice[] = [];
  const deeplinkLog: string[] = [];
  const roomSubscribers = new Set<(item: HostChatListSubscribeItem) => void>();
  let timings: MockChatTimings = { ...MOCK_CHAT_TIMINGS };
  let product: ChatHostRuntime | undefined;
  let nextHostMessage = 0;
  let nextPending = 0;

  const fail = (): void => {
    if (chatError) throw new Error(chatError);
  };

  const chatRooms = (): ChatRoom[] =>
    byKey(rooms).map(({ roomId, participatingAs }) => ({
      roomId,
      participatingAs,
    }));

  const publishRooms = (): void => {
    const item = { rooms: chatRooms() };
    for (const push of roomSubscribers) push(item);
  };

  /** The room, or an error the core turns into a refusal. */
  const hosted = (roomId: string): RoomState => {
    const room = rooms.get(roomId);
    if (!room) throw new Error(`unknown chat room ${roomId}`);
    return room;
  };

  const declarations = (room: RoomState): MockChatRoom => {
    const {
      messages: _messages,
      lastSeen: _lastSeen,
      olderCursor: _olderCursor,
      pagesAsked: _pagesAsked,
      open: _open,
      ...declared
    } = room;
    return structuredClone(declared);
  };

  const hostTimestamp = (): bigint => BigInt(Date.now());
  const hostMessageId = (): string => `mock-message:${nextHostMessage++}`;

  const find = (room: RoomState, messageId: string): number =>
    room.messages.findIndex((message) => message.messageId === messageId);

  /** Put a message of a room the product keeps in time order. */
  const insertByTime = (room: RoomState, message: ChatMessage): void => {
    const at = find(room, message.messageId);
    if (at >= 0) {
      room.messages[at] = message;
      return;
    }
    let index = room.messages.length;
    while (
      index > 0 &&
      room.messages[index - 1]!.timestamp > message.timestamp
    ) {
      index--;
    }
    room.messages.splice(index, 0, message);
  };

  /**
   * Apply one feed update. A room the host stores refuses an update it cannot
   * apply, with the error name as the reason. The screen of a room the
   * product keeps applies what it can and ignores a message it does not show.
   */
  const applyUpdate = (
    room: RoomState,
    update: ChatFeedUpdate,
    persist: boolean,
  ): void => {
    const refuse = (reason: string): void => {
      if (persist) throw new Error(reason);
    };
    if (update.tag === "New") {
      if (!persist) {
        insertByTime(room, structuredClone(update.value));
        return;
      }
      if (find(room, update.value.messageId) >= 0) {
        return refuse("DuplicateMessage");
      }
      room.messages.push(structuredClone(update.value));
      return;
    }
    const at = find(room, update.value.messageId);
    if (at < 0) return refuse("UnknownMessage");
    const message = room.messages[at]!;
    switch (update.tag) {
      case "Edited":
        if (persist && message.origin.participantId === MOCK_CHAT_USER) {
          return refuse("NotOwner");
        }
        message.body = structuredClone(update.value.body);
        message.editedAt = update.value.editedAt;
        return;
      case "Deleted":
        room.messages.splice(at, 1);
        return;
      case "ReactionPlaced":
      case "ReactionRemoved":
        message.reactions = adjustReactions(
          message.reactions,
          update.value.participantId,
          update.value.emoji,
          update.tag === "ReactionPlaced",
        );
        return;
      case "StatusChanged":
        message.status = update.value.status;
        return;
    }
  };

  /** Change the declarations of a room the product hosts. */
  const declare = (roomId: string, change: (room: RoomState) => void) => {
    fail();
    change(hosted(roomId));
  };

  const callbacks: Required<ChatPlatform> = {
    async createChatRoom(_product, request, iconUrl) {
      fail();
      if (rooms.has(request.roomId)) return { status: "Exists" };
      rooms.set(request.roomId, {
        roomId: request.roomId,
        // A product that creates a room hosts it. A product that is a bot
        // in a room registers the bot instead.
        participatingAs: "RoomHost",
        metadata: structuredClone(request.metadata),
        iconUrl,
        config: structuredClone(request.config),
        ...(request.preview === undefined
          ? {}
          : { preview: structuredClone(request.preview) }),
        toolbar: [],
        suggestions: [],
        commands: [],
        updates: [],
        features: defaultChatRoomFeatures(),
        meta: {},
        messages: [],
        pagesAsked: new Set(),
        open: false,
      });
      publishRooms();
      return { status: "New" };
    },
    async registerChatBot(_product, request) {
      fail();
      // A repeat keeps the persona and replaces its name and icon.
      const exists = bots.has(request.botId);
      bots.set(request.botId, structuredClone(request));
      return { status: exists ? "Exists" : "New" };
    },
    async postChatMessage(
      _product,
      request,
    ): Promise<HostChatPostMessageResponse> {
      fail();
      // A post to a room the product did not register is a product bug,
      // and a mock that accepts it hides the bug.
      const room = hosted(request.roomId);
      const messageId = hostMessageId();
      posted.push({
        messageId,
        roomId: request.roomId,
        payload: request.payload,
      });
      const body = contentBody(request.payload);
      if (body) {
        room.messages.push({
          messageId,
          origin: { participantId: MOCK_CHAT_PRODUCT },
          timestamp: hostTimestamp(),
          body:
            body.tag === "Custom" && request.alt !== undefined
              ? {
                  tag: "Custom",
                  value: { ...body.value, previewText: request.alt },
                }
              : body,
          status: "Sent",
          reactions: [],
        });
      } else if (
        request.payload.tag === "Reaction" ||
        request.payload.tag === "ReactionRemoved"
      ) {
        // A V1 reaction is a reaction of the product (RFC §13.10).
        applyUpdate(
          room,
          {
            tag:
              request.payload.tag === "Reaction"
                ? "ReactionPlaced"
                : "ReactionRemoved",
            value: {
              messageId: request.payload.value.messageId,
              participantId: MOCK_CHAT_PRODUCT,
              emoji: request.payload.value.emoji,
            },
          },
          true,
        );
      }
      return { messageId };
    },
    async setChatRoomFooter(_product, request) {
      declare(request.roomId, (room) => {
        room.footer = structuredClone(request.footer);
      });
    },
    subscribeChatRooms() {
      // The other chat calls fail with this reason, so the subscription
      // fails with it too. A healthy stream with no rooms would hide the
      // fault.
      if (chatError) return options.failedRooms(chatError);
      return options.liveRooms({ rooms: chatRooms() }, (push) => {
        roomSubscribers.add(push);
        return () => roomSubscribers.delete(push);
      });
    },
    async readChatRoomConfig(_product, roomId) {
      fail();
      return structuredClone(
        rooms.get(roomId)?.config ?? defaultChatRoomConfig(),
      );
    },
    async setChatRoomConfig(_product, request) {
      declare(request.roomId, (room) => {
        // A room that changes who keeps its messages starts with none.
        if (room.config.productSource !== request.config.productSource) {
          room.messages = [];
          room.olderCursor = undefined;
          room.pagesAsked.clear();
        }
        room.config = structuredClone(request.config);
      });
    },
    async setChatRoomMetadata(_product, request) {
      declare(request.roomId, (room) => {
        room.metadata = structuredClone(request.metadata);
      });
    },
    async setChatRoomPreview(_product, request) {
      declare(request.roomId, (room) => {
        room.preview = structuredClone(request.preview);
      });
    },
    async deleteChatRoom(_product, request) {
      fail();
      const room = rooms.get(request.roomId);
      // Delete is idempotent: a room that is gone already is not an error.
      if (!room) return;
      rooms.delete(request.roomId);
      if (room.open) {
        noticeLog.push({ roomId: request.roomId, text: "room deleted" });
      }
      publishRooms();
    },
    async updateChatFeed(_product, request, persist) {
      declare(request.roomId, (room) => {
        applyUpdate(room, request.update, persist);
        room.updates.push({ update: structuredClone(request.update), persist });
      });
    },
    async setChatRoomToolbar(_product, request) {
      declare(request.roomId, (room) => {
        room.toolbar = structuredClone(request.actions);
      });
    },
    async setChatRoomSuggestions(_product, request) {
      declare(request.roomId, (room) => {
        room.suggestions = structuredClone(request.suggestions);
      });
    },
    async setChatRoomCommands(_product, request) {
      declare(request.roomId, (room) => {
        room.commands = structuredClone(request.commands);
      });
    },
    async setChatRoomActivity(_product, request) {
      declare(request.roomId, (room) => {
        if (request.activity === undefined) delete room.activity;
        else room.activity = structuredClone(request.activity);
      });
    },
    async chatCapabilities() {
      fail();
      return { names: [...featureNames] };
    },
    async readChatRoomFeatures(_product, roomId) {
      fail();
      return structuredClone(
        rooms.get(roomId)?.features ?? defaultChatRoomFeatures(),
      );
    },
    async setChatRoomFeatures(_product, request) {
      // The host keeps the declaration whole, a feature it does not serve
      // included.
      declare(request.roomId, (room) => {
        room.features = structuredClone(request.features);
      });
    },
    async setChatMessageMeta(_product, request) {
      declare(request.roomId, (room) => {
        // A room the host stores answers `UnknownMessage` for a message it
        // does not hold. A room the product keeps keeps the meta for later.
        if (!room.config.productSource && find(room, request.messageId) < 0) {
          throw new Error("UnknownMessage");
        }
        room.meta[request.messageId] = structuredClone(request.meta);
      });
    },
    async setChatRoomCall(_product, request) {
      declare(request.roomId, (room) => {
        if (request.call === undefined) delete room.call;
        else room.call = structuredClone(request.call);
      });
    },
    async readChatRoomUserStates() {
      fail();
      return {
        rooms: [...userStates.keys()].sort().map((roomId) => {
          const state = userStates.get(roomId)!;
          return {
            roomId,
            ...(state.mutedUntil === undefined
              ? {}
              : { mutedUntil: state.mutedUntil }),
            archived: state.archived,
          };
        }),
      };
    },
    async readChatBots() {
      fail();
      return { botIds: byKey(bots).map((bot) => bot.botId) };
    },
  };

  // ------------------------------------------------------------ user half

  const attached = (): ChatHostRuntime => {
    if (!product) {
      throw new Error(
        "no product is attached to the mock chat user; attach the host runtime first",
      );
    }
    return product;
  };

  const room = (roomId: string): RoomState => {
    const found = rooms.get(roomId);
    if (!found) throw new Error(`the mock chat user has no room ${roomId}`);
    return found;
  };

  const publish = async (roomId: string, event: ChatEvent): Promise<void> => {
    const item: HostChatActionSubscribeItem = {
      roomId,
      peer: MOCK_CHAT_PEER,
      event,
    };
    events.push(item);
    await attached().publishChatEvent(item);
  };

  const publishRoomEvent = async (
    roomId: string,
    event: ChatRoomEvent,
  ): Promise<void> => {
    const item: HostChatRoomEventsSubscribeItem = { roomId, event };
    roomEventLog.push(item);
    await attached().publishChatRoomEvent(item);
  };

  const delay = (ms: number) =>
    new Promise<void>((resolve) => setTimeout(resolve, ms));

  /**
   * Ask one query, and ask once more after the retry delay when the answer
   * is `NotAvailable` (RFC §5.1). Every answer goes in the query log.
   */
  async function ask<Item, E extends { tag: string }>(
    name: ChatQueryName,
    request: unknown,
    timeoutMs: number,
    retry = true,
  ): Promise<ChatQueryResult<Item, E>> {
    const runtime = attached();
    const once = async () => {
      const result = (await (
        runtime[name] as (
          request: unknown,
          options: { timeoutMs: number },
        ) => Promise<ChatQueryResult<unknown, unknown>>
      )(request, { timeoutMs })) as ChatQueryResult<Item, E>;
      queryLog.push({ name, request: structuredClone(request), result });
      return result;
    };
    const first = await once();
    if (!retry || first.isOk() || !isNotAvailable(first.error)) return first;
    await delay(timings.retryDelay);
    return once();
  }

  const userAction = (
    roomId: string,
    action: ChatUserAction,
  ): Promise<ChatQueryResult<unknown, ProductChatUserActionError>> =>
    ask(
      "chatUserAction",
      { roomId, action },
      action.tag === "NewMessage" ? timings.newMessage : timings.userAction,
      false,
    );

  /** Ask for one page and put it on the screen. */
  const loadPage = async (
    state: RoomState,
    cursor: string | undefined,
  ): Promise<boolean> => {
    const key = `Before:${cursor ?? ""}`;
    state.pagesAsked.add(key);
    const result = await ask<
      { messages: ChatMessage[]; next?: string },
      ProductChatQueryError
    >(
      "chatMessagesQuery",
      {
        roomId: state.roomId,
        anchor: {
          tag: "Before",
          value: cursor === undefined ? {} : { cursor },
        },
        pageSize: MOCK_CHAT_PAGE_SIZE,
      },
      timings.messages,
    );
    if (result.isErr()) {
      // The host keeps what it has and shows that it could not load (§3.6).
      // `NotAvailable` is not a failure and shows nothing.
      if (!isNotAvailable(result.error)) {
        noticeLog.push({ roomId: state.roomId, text: "could not load" });
      }
      return false;
    }
    // The newest page replaces the cached screen: the cache shows only while
    // the page loads (§5.3). An older page goes above what the screen shows.
    if (cursor === undefined) state.messages = [];
    for (const message of result.value.messages) insertByTime(state, message);
    state.olderCursor = result.value.next ?? null;
    return true;
  };

  /** Apply an optimistic change and undo it when the product refuses it. */
  const optimistic = async (
    state: RoomState,
    action: ChatUserAction,
    change: () => void,
  ): Promise<MockActionOutcome> => {
    const saved = structuredClone(state.messages);
    change();
    const result = await userAction(state.roomId, action);
    if (result.isOk()) return { tag: "Applied" };
    state.messages = saved;
    noticeLog.push({
      roomId: state.roomId,
      text: `${action.tag} failed: ${describeRefusal(result.error)}`,
    });
    return { tag: "Reverted", refusal: result.error };
  };

  const bodyOf = (input: string | ChatMessageBody): ChatMessageBody =>
    typeof input === "string" ? textBody(input) : input;

  /** The declared command a typed text names, if any (RFC §13.12). */
  const declaredCommand = (
    state: RoomState,
    body: ChatMessageBody,
  ): ChatCommand | undefined => {
    if (body.tag !== "Text" || !body.value.text.startsWith("/"))
      return undefined;
    const match = /^\/(\S+)\s*([\s\S]*)$/.exec(body.value.text);
    if (!match) return undefined;
    const [, name, rest] = match;
    if (!state.commands.some((command) => command.name === name))
      return undefined;
    return { command: name!, payload: rest!.trim() };
  };

  const builtInMenu = (
    state: RoomState,
    message: ChatMessage,
  ): ChatBuiltInMessageAction[] => {
    const own = message.origin.participantId === MOCK_CHAT_USER;
    const menu: ChatBuiltInMessageAction[] = ["Copy", "Reply"];
    if (own) menu.push("Edit", "Delete");
    menu.push("React");
    if (message.status === "Failed") {
      // Retry exists only where the product keeps the messages (RFC §13.9).
      if (state.config.productSource) menu.push("Retry");
      else if (!own) menu.push("Delete");
    }
    return menu;
  };

  const participantBuiltIn = (
    entry: ChatParticipantEntry,
  ): MockParticipantBuiltIn | undefined => {
    switch (entry.participant.tag) {
      case "Person":
        return "OpenProfile";
      case "LitePerson":
        return "CopyAddress";
      case "Contact":
        return "OpenContact";
      case "Product":
        return "OpenProduct";
      case "Custom":
        return undefined;
    }
  };

  const message = (state: RoomState, messageId: string): ChatMessage => {
    const found = state.messages.find((each) => each.messageId === messageId);
    if (!found) {
      throw new Error(`the room ${state.roomId} shows no message ${messageId}`);
    }
    return found;
  };

  const user: MockChatUser = {
    attach(next) {
      product = next;
      return () => {
        if (product === next) product = undefined;
      };
    },
    setTimings(next) {
      timings = { ...timings, ...next };
    },
    async open(roomId) {
      const state = room(roomId);
      state.open = true;
      await publish(roomId, { tag: "RoomOpened" });
      if (state.config.productSource) {
        // The screen opens on the newest page (§5.3); the cache stays below.
        state.pagesAsked.clear();
        await loadPage(state, undefined);
      }
      return structuredClone(state.messages);
    },
    async close(roomId) {
      const state = room(roomId);
      state.open = false;
      await publish(roomId, { tag: "RoomClosed" });
    },
    isOpen(roomId) {
      return rooms.get(roomId)?.open ?? false;
    },
    async loadOlder(roomId) {
      const state = room(roomId);
      if (!state.config.productSource) return false;
      if (state.olderCursor === null) return false;
      if (state.olderCursor !== undefined) {
        // A page the host asked for once is never asked again (S16).
        if (state.pagesAsked.has(`Before:${state.olderCursor}`)) return false;
      }
      return loadPage(state, state.olderCursor);
    },
    async send(roomId, input, options = {}) {
      const state = room(roomId);
      const body = bodyOf(input);
      const command = declaredCommand(state, body);
      if (command) {
        await publish(roomId, { tag: "Command", value: command });
        return { tag: "Command", command };
      }
      const replyTo = options.replyTo;
      if (!state.config.productSource) {
        const stored: ChatMessage = {
          messageId: hostMessageId(),
          origin: { participantId: MOCK_CHAT_USER },
          timestamp: hostTimestamp(),
          body,
          status: "Sent",
          ...(replyTo === undefined ? {} : { replyTo }),
          reactions: [],
        };
        state.messages.push(stored);
        state.lastSeen = stored.messageId;
        await publish(roomId, {
          tag: "MessagePosted",
          value: {
            messageId: stored.messageId,
            body,
            ...(replyTo === undefined ? {} : { replyTo }),
          },
        });
        return { tag: "Posted", message: structuredClone(stored) };
      }
      // The optimistic message, drawn while the product answers (§5.3).
      const pending: ChatMessage = {
        messageId: `pending:${nextPending++}`,
        origin: { participantId: MOCK_CHAT_USER },
        timestamp: hostTimestamp(),
        body,
        status: "Preparing",
        ...(replyTo === undefined ? {} : { replyTo }),
        reactions: [],
      };
      state.messages.push(pending);
      const result = await userAction(roomId, {
        tag: "NewMessage",
        value: { body, ...(replyTo === undefined ? {} : { replyTo }) },
      });
      const at = find(state, pending.messageId);
      if (result.isOk()) {
        const item = result.value as {
          tag: "MessageCreated" | "Acknowledged";
          value?: ChatMessage;
        };
        const created =
          item.tag === "MessageCreated" && item.value
            ? item.value
            : { ...pending, status: "Sent" as const };
        if (at >= 0) state.messages.splice(at, 1);
        insertByTime(state, created);
        return { tag: "Posted", message: structuredClone(created) };
      }
      const refusal = result.error;
      if (refusal.tag === "Domain" && refusal.value.tag === "Rejected") {
        if (at >= 0) state.messages.splice(at, 1);
        noticeLog.push({ roomId, text: refusal.value.value.reason });
        return { tag: "Rejected", reason: refusal.value.value.reason };
      }
      if (at >= 0) state.messages[at]!.status = "Failed";
      return { tag: "Failed", messageId: pending.messageId, refusal };
    },
    async edit(roomId, messageId, input) {
      const state = room(roomId);
      const body = bodyOf(input);
      if (!state.config.productSource) {
        const target = message(state, messageId);
        target.body = body;
        target.editedAt = hostTimestamp();
        await publish(roomId, {
          tag: "UserEdited",
          value: { messageId, body },
        });
        return { tag: "Applied" };
      }
      return optimistic(
        state,
        { tag: "EditMessage", value: { messageId, body } },
        () => {
          const target = message(state, messageId);
          target.body = body;
          target.editedAt = hostTimestamp();
        },
      );
    },
    async remove(roomId, messageId) {
      const state = room(roomId);
      const drop = () => {
        state.messages.splice(find(state, messageId), 1);
      };
      message(state, messageId);
      if (!state.config.productSource) {
        drop();
        await publish(roomId, { tag: "UserDeleted", value: { messageId } });
        return { tag: "Applied" };
      }
      return optimistic(
        state,
        { tag: "DeleteMessage", value: { messageId } },
        drop,
      );
    },
    async react(roomId, messageId, emoji, placed = true) {
      const state = room(roomId);
      const allowed = state.config.allowedReactions;
      if (placed && allowed !== undefined && !allowed.includes(emoji)) {
        throw new Error(`the picker of ${roomId} does not offer ${emoji}`);
      }
      const change = () => {
        const target = message(state, messageId);
        target.reactions = adjustReactions(
          target.reactions,
          MOCK_CHAT_USER,
          emoji,
          placed,
        );
      };
      if (!state.config.productSource) {
        change();
        await publish(roomId, {
          tag: "UserReacted",
          value: { messageId, emoji, placed },
        });
        return { tag: "Applied" };
      }
      return optimistic(
        state,
        {
          tag: placed ? "ReactionPlaced" : "ReactionRemoved",
          value: { messageId, emoji },
        },
        change,
      );
    },
    async seen(roomId, messageId) {
      const state = room(roomId);
      const id = messageId ?? state.messages.at(-1)?.messageId;
      if (id === undefined) return { tag: "Applied" };
      state.lastSeen = id;
      if (!state.config.reportSeen) return { tag: "Applied" };
      if (!state.config.productSource) {
        await publish(roomId, { tag: "Seen", value: { messageId: id } });
        return { tag: "Applied" };
      }
      return optimistic(
        state,
        { tag: "Seen", value: { messageId: id } },
        () => {},
      );
    },
    async retry(roomId, messageId) {
      const state = room(roomId);
      if (!state.config.productSource) {
        throw new Error("retry exists only in a room the product keeps");
      }
      message(state, messageId);
      return optimistic(
        state,
        { tag: "Retry", value: { messageId } },
        () => {},
      );
    },
    async pressAction(roomId, messageId, actionId, payload) {
      room(roomId);
      await publish(roomId, {
        tag: "ActionTriggered",
        value: {
          messageId,
          actionId,
          ...(payload === undefined ? {} : { payload }),
        },
      });
    },
    async pressToolbar(roomId, actionId) {
      const state = room(roomId);
      const action = state.toolbar.find((each) => each.actionId === actionId);
      if (!action)
        throw new Error(`the toolbar of ${roomId} has no ${actionId}`);
      if (action.handler.tag === "Deeplink") {
        deeplinkLog.push(action.handler.value.url);
        return;
      }
      await publish(roomId, {
        tag: "ToolbarActionTriggered",
        value: { actionId },
      });
    },
    async pickSuggestion(roomId, suggestionId) {
      const state = room(roomId);
      const suggestion = state.suggestions.find(
        (each) => each.suggestionId === suggestionId,
      );
      if (!suggestion) {
        throw new Error(
          `the room ${roomId} offers no suggestion ${suggestionId}`,
        );
      }
      if (suggestion.sendsText) return user.send(roomId, suggestion.text);
      await publish(roomId, {
        tag: "SuggestionSelected",
        value: { suggestionId },
      });
      return undefined;
    },
    async messageMenu(roomId, messageId) {
      const state = room(roomId);
      const target = message(state, messageId);
      const builtIn = builtInMenu(state, target);
      if (!state.config.messageActions) {
        return { builtIn, actions: [], source: "default" };
      }
      const result = await ask<
        { menu: import("@parity/truapi").ChatMessageMenu },
        ProductChatQueryError
      >(
        "chatMessageActionsQuery",
        { roomId, message: structuredClone(target) },
        timings.details,
      );
      // A refusal or a time-out shows the host menu (§3.6, S26).
      if (result.isErr()) return { builtIn, actions: [], source: "default" };
      const { menu } = result.value;
      if (menu.tag === "Custom") {
        return { builtIn: [], actions: menu.value.actions, source: "product" };
      }
      return {
        builtIn: builtIn.filter((entry) => !menu.value.hidden.includes(entry)),
        actions: menu.value.actions,
        source: "product",
      };
    },
    async pickMessageAction(roomId, messageId, action) {
      room(roomId);
      if (action.handler.tag === "Deeplink") {
        deeplinkLog.push(action.handler.value.url);
        return;
      }
      await publish(roomId, {
        tag: "MessageActionTriggered",
        value: { messageId, actionId: action.actionId },
      });
    },
    async participants(roomId, query) {
      const state = room(roomId);
      const two: ChatParticipantEntry[] = [];
      const fallback = (): ProductChatParticipantsQueryItem => {
        switch (query.tag) {
          case "Count":
            return { tag: "Count", value: { count: 2 } };
          case "Page":
            return { tag: "Page", value: { participants: two } };
          case "ByIds":
            return { tag: "Resolved", value: { participants: two } };
        }
      };
      if (!state.config.participants) return fallback();
      const result = await ask<
        ProductChatParticipantsQueryItem,
        ProductChatQueryError
      >("chatParticipantsQuery", { roomId, query }, timings.details);
      return result.isOk() ? result.value : fallback();
    },
    async participantMenu(roomId, entry) {
      const state = room(roomId);
      const builtIn = participantBuiltIn(entry);
      const host: MockParticipantMenu = {
        ...(builtIn === undefined ? {} : { builtIn }),
        actions: [],
        source: "default",
      };
      if (!state.config.participantActions) return host;
      const result = await ask<
        { menu: import("@parity/truapi").ChatParticipantMenu },
        ProductChatQueryError
      >("chatParticipantActionsQuery", { roomId, entry }, timings.details);
      if (result.isErr()) return host;
      const { menu } = result.value;
      if (menu.tag === "Custom") {
        return { actions: menu.value.actions, source: "product" };
      }
      return {
        ...(builtIn === undefined ? {} : { builtIn }),
        actions: menu.value.actions,
        source: "product",
      };
    },
    async pickParticipantAction(roomId, participantId, action) {
      room(roomId);
      if (action.handler.tag === "Deeplink") {
        deeplinkLog.push(action.handler.value.url);
        return;
      }
      await publish(roomId, {
        tag: "ParticipantActionTriggered",
        value: { participantId, actionId: action.actionId },
      });
    },
    async whoReacted(roomId, messageId) {
      const state = room(roomId);
      if (!state.config.reactionDetails) return undefined;
      const result = await ask<
        { reactions: ChatReactionDetail[] },
        ProductChatQueryError
      >("chatReactionsQuery", { roomId, messageId }, timings.details);
      return result.isOk() ? result.value.reactions : undefined;
    },
    publish,
    async pressCall(roomId, video) {
      const calls = room(roomId).features.calls;
      if (!calls || !(video ? calls.video : calls.audio)) {
        throw new Error(
          `the room ${roomId} shows no ${video ? "video" : "audio"} call button`,
        );
      }
      await publishRoomEvent(roomId, {
        tag: "CallRequested",
        value: { video },
      });
    },
    async answerCall(roomId, accept) {
      const call = room(roomId).call;
      if (call?.tag !== "Ringing") {
        throw new Error(`no call rings in the room ${roomId}`);
      }
      await publishRoomEvent(roomId, {
        tag: "CallAnswered",
        value: { callId: call.value.callId, accept },
      });
    },
    async openCall(roomId) {
      const call = room(roomId).call;
      if (call?.tag !== "Active" || call.value.mediaSession !== undefined) {
        throw new Error(
          `the room ${roomId} shows no call bar of a call without a media session`,
        );
      }
      await publishRoomEvent(roomId, {
        tag: "CallOpened",
        value: { callId: call.value.callId },
      });
    },
    async endCall(roomId) {
      const call = room(roomId).call;
      if (call?.tag !== "Active") {
        throw new Error(`the room ${roomId} shows no call bar`);
      }
      await publishRoomEvent(roomId, {
        tag: "CallEnded",
        value: { callId: call.value.callId },
      });
    },
    async selectBranch(roomId, messageId, index) {
      const branch = room(roomId).meta[messageId]?.branch;
      if (!branch) {
        throw new Error(`the message ${messageId} shows no branch control`);
      }
      if (index < 1 || index > branch.count) {
        throw new Error(
          `the message ${messageId} has no alternative ${index} of ${branch.count}`,
        );
      }
      await publishRoomEvent(roomId, {
        tag: "BranchSelected",
        value: { messageId, index },
      });
    },
    async search(roomId, text, cursor) {
      const state = room(roomId);
      if (!state.config.productSource) {
        // The host searches its own store, case insensitive, newest first.
        const needle = text.trim().toLowerCase();
        const hits = state.messages
          .filter((each) => previewLine(each).toLowerCase().includes(needle))
          .reverse();
        return { hits: structuredClone(hits) };
      }
      if (!state.features.search) return undefined;
      const result = await ask<
        { hits: ChatMessage[]; next?: string },
        ProductChatQueryError
      >(
        "chatSearchQuery",
        {
          roomId,
          text,
          ...(cursor === undefined ? {} : { cursor }),
          pageSize: MOCK_CHAT_SEARCH_PAGE_SIZE,
        },
        timings.search,
        false,
      );
      if (result.isErr()) {
        noticeLog.push({ roomId, text: "Search is not available" });
        return undefined;
      }
      return result.value;
    },
    wake: (reason) => attached().wakeProduct(reason),
    publishRoomEvent,
    queries: () => [...queryLog],
    notices: () => [...noticeLog],
    deeplinks: () => [...deeplinkLog],
  };

  const unread = (state: RoomState): number => {
    const seenAt =
      state.lastSeen === undefined ? -1 : find(state, state.lastSeen);
    return state.messages
      .slice(seenAt + 1)
      .filter((each) => each.origin.participantId !== MOCK_CHAT_USER).length;
  };

  return {
    callbacks,
    user,
    chatRooms,
    chatBots: () => byKey(bots),
    postedMessages: () => [...posted],
    rooms: () => byKey(rooms).map(declarations),
    room: (roomId) => {
      const state = rooms.get(roomId);
      return state && declarations(state);
    },
    feed: (roomId) => structuredClone(rooms.get(roomId)?.messages ?? []),
    preview: (roomId) => {
      const state = rooms.get(roomId);
      if (!state) return undefined;
      const declared = state.preview;
      const last = state.messages.at(-1);
      const timestamp = declared?.timestamp ?? last?.timestamp;
      const badge: ChatRoomBadge | undefined =
        declared?.badge ??
        (state.config.productSource
          ? undefined
          : { tag: "UnreadCount", value: { count: unread(state) } });
      const text = declared?.text?.text ?? (last && previewLine(last));
      return {
        roomId,
        order: declared?.order ?? "Timestamp",
        ...(timestamp === undefined ? {} : { timestamp }),
        ...(badge === undefined ? {} : { badge: structuredClone(badge) }),
        ...(text === undefined ? {} : { text }),
        ongoing: declared?.text?.ongoing ?? false,
      };
    },
    events: () => [...events],
    roomEvents: () => [...roomEventLog],
    features: () => [...featureNames],
    async setRoomUserState(roomId, state) {
      if (state.mutedUntil === undefined && !state.archived) {
        userStates.delete(roomId);
      } else {
        userStates.set(roomId, structuredClone(state));
      }
      if (!product) return;
      await publishRoomEvent(roomId, {
        tag: "UserStateChanged",
        value: {
          ...(state.mutedUntil === undefined
            ? {}
            : { mutedUntil: state.mutedUntil }),
          archived: state.archived,
        },
      });
    },
    roomUserState: (roomId) => structuredClone(userStates.get(roomId)),
    clear() {
      rooms.clear();
      bots.clear();
      posted.length = 0;
      events.length = 0;
      roomEventLog.length = 0;
      userStates.clear();
      queryLog.length = 0;
      noticeLog.length = 0;
      deeplinkLog.length = 0;
      nextHostMessage = 0;
      nextPending = 0;
      publishRooms();
    },
  };
}
