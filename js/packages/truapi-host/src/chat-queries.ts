// The chat queries a host asks a product, as typed functions.
//
// The core gives each query to the host as SCALE bytes: the host sends the
// encoded request, and the core answers once with the encoded item, the
// encoded `CallError` of the query, or the end of the query. This module
// encodes the request, decodes the answer, and adds the time limits of the
// RFC. The page host and the in-process test client use the same code, so the
// two give the same answers.

import { err, ok, type Result } from "neverthrow";

import {
  HostChatActionSubscribeItem as HostChatActionSubscribeItemCodec,
  HostChatRoomEventsSubscribeItem as HostChatRoomEventsSubscribeItemCodec,
  ProductChatMessageActionsQueryItem as MessageActionsItemCodec,
  ProductChatMessageActionsQueryRequest as MessageActionsRequestCodec,
  ProductChatMessagesQueryItem as MessagesItemCodec,
  ProductChatMessagesQueryRequest as MessagesRequestCodec,
  ProductChatParticipantsQueryItem as ParticipantsItemCodec,
  ProductChatParticipantsQueryRequest as ParticipantsRequestCodec,
  ProductChatQueryError as QueryErrorCodec,
  ProductChatReactionsQueryItem as ReactionsItemCodec,
  ProductChatReactionsQueryRequest as ReactionsRequestCodec,
  ProductChatSearchQueryItem as SearchItemCodec,
  ProductChatSearchQueryRequest as SearchRequestCodec,
  ProductChatUserActionError as UserActionErrorCodec,
  ProductChatUserActionItem as UserActionItemCodec,
  ProductChatUserActionRequest as UserActionRequestCodec,
} from "@parity/truapi";
import type {
  HostChatActionSubscribeItem,
  HostChatRoomEventsSubscribeItem,
  ProductChatMessageActionsQueryItem,
  ProductChatMessageActionsQueryRequest,
  ProductChatMessagesQueryItem,
  ProductChatMessagesQueryRequest,
  ProductChatParticipantsQueryItem,
  ProductChatParticipantsQueryRequest,
  ProductChatQueryError,
  ProductChatReactionsQueryItem,
  ProductChatReactionsQueryRequest,
  ProductChatSearchQueryItem,
  ProductChatSearchQueryRequest,
  ProductChatUserActionError,
  ProductChatUserActionItem,
  ProductChatUserActionRequest,
} from "@parity/truapi";
import {
  CallError,
  type CallErrorValue,
  type Codec,
} from "@parity/truapi/scale";

import { errorMessage } from "./error.js";

/**
 * The chat queries. Each name is also the name of the method on the WASM
 * product runtime that starts the query.
 */
export type ChatQueryName =
  | "chatMessagesQuery"
  | "chatUserAction"
  | "chatReactionsQuery"
  | "chatParticipantsQuery"
  | "chatMessageActionsQuery"
  | "chatSearchQuery";

/** Every chat query name, in wire order (method ids 16 to 21, then 27). */
export const CHAT_QUERY_NAMES: readonly ChatQueryName[] = [
  "chatMessagesQuery",
  "chatUserAction",
  "chatReactionsQuery",
  "chatParticipantsQuery",
  "chatMessageActionsQuery",
  "chatSearchQuery",
];

/**
 * Why a query gave no item.
 *
 * The `CallError` variants come from the product or from the core: `Domain`
 * carries the error of the query, such as `NotAvailable`. The core answers
 * `NotAvailable` itself for a room that does not declare the handler.
 * `Timeout` means that no answer came in the time limit, and the host stopped
 * the query. `NoAnswer` means that the product ended the query with no item.
 */
export type ChatQueryRefusal<E> =
  | CallErrorValue<E>
  | { tag: "Timeout"; value?: undefined }
  | { tag: "NoAnswer"; value?: undefined };

/** The answer of one query: the item, or the refusal. */
export type ChatQueryResult<Item, E> = Result<Item, ChatQueryRefusal<E>>;

/** Options of one query. */
export interface ChatQueryOptions {
  /**
   * How long the host waits for the answer, in milliseconds. The default is
   * the time limit of the RFC for the query, see {@link chatQueryTimeoutMs}.
   * `0` or `Infinity` waits with no limit.
   */
  timeoutMs?: number;
  /** Stops the query. The result is then the `Cancelled` refusal. */
  signal?: AbortSignal;
}

/** The time limits of RFC §3.6, in milliseconds. */
export const CHAT_QUERY_TIMEOUTS_MS = {
  /** `messages_query`. */
  messages: 10_000,
  /** `user_action` with `NewMessage`. */
  newMessage: 30_000,
  /** `user_action` with any other action. */
  userAction: 10_000,
  /** The reactions, participants and menu queries. */
  details: 5_000,
  /** `search_query`. */
  search: 10_000,
} as const;

/** The default time limit of a query, from RFC §3.6. */
export function chatQueryTimeoutMs(
  name: ChatQueryName,
  request: unknown,
): number {
  switch (name) {
    case "chatMessagesQuery":
      return CHAT_QUERY_TIMEOUTS_MS.messages;
    case "chatSearchQuery":
      return CHAT_QUERY_TIMEOUTS_MS.search;
    case "chatUserAction":
      return (request as ProductChatUserActionRequest).action.tag ===
        "NewMessage"
        ? CHAT_QUERY_TIMEOUTS_MS.newMessage
        : CHAT_QUERY_TIMEOUTS_MS.userAction;
    default:
      return CHAT_QUERY_TIMEOUTS_MS.details;
  }
}

/** What the core sends back for one query. Exactly one of these comes. */
export interface ChatQuerySink {
  /** The SCALE-encoded item. */
  onAnswer(item: Uint8Array): void;
  /** The SCALE-encoded `CallError` of the query. */
  onRefusal(error: Uint8Array): void;
  /** The product ended the query with no item. */
  onEnd(): void;
  /** The core could not start the query, or the connection closed. */
  onFailure(reason: string): void;
}

/**
 * Start one query on a core and return the function that stops it. The core
 * calls exactly one member of `sink`, and never during this call.
 */
export type StartChatQuery = (
  name: ChatQueryName,
  request: Uint8Array,
  sink: ChatQuerySink,
) => () => void;

/** The typed queries a host asks a product. */
export interface ChatHostQueries {
  /** One page of messages of a room the product keeps. */
  chatMessagesQuery(
    request: ProductChatMessagesQueryRequest,
    options?: ChatQueryOptions,
  ): Promise<
    ChatQueryResult<ProductChatMessagesQueryItem, ProductChatQueryError>
  >;
  /** What the user did in a room the product keeps. */
  chatUserAction(
    request: ProductChatUserActionRequest,
    options?: ChatQueryOptions,
  ): Promise<
    ChatQueryResult<ProductChatUserActionItem, ProductChatUserActionError>
  >;
  /** Who reacted to one message. */
  chatReactionsQuery(
    request: ProductChatReactionsQueryRequest,
    options?: ChatQueryOptions,
  ): Promise<
    ChatQueryResult<ProductChatReactionsQueryItem, ProductChatQueryError>
  >;
  /** The participants of a room: a count, a page, or some by id. */
  chatParticipantsQuery(
    request: ProductChatParticipantsQueryRequest,
    options?: ChatQueryOptions,
  ): Promise<
    ChatQueryResult<ProductChatParticipantsQueryItem, ProductChatQueryError>
  >;
  /** The menu of one message. */
  chatMessageActionsQuery(
    request: ProductChatMessageActionsQueryRequest,
    options?: ChatQueryOptions,
  ): Promise<
    ChatQueryResult<ProductChatMessageActionsQueryItem, ProductChatQueryError>
  >;
  /**
   * The messages of a room the product keeps that match what the user
   * typed, newest first. The core answers `NotAvailable` itself for a room
   * whose features do not set `search`.
   */
  chatSearchQuery(
    request: ProductChatSearchQueryRequest,
    options?: ChatQueryOptions,
  ): Promise<
    ChatQueryResult<ProductChatSearchQueryItem, ProductChatQueryError>
  >;
}

/** Why a host wakes a product. */
export type ChatWakeReason =
  /** The product started and has no room yet. */
  | { tag: "ProductStart" }
  /** A user message or a room event is on its way to the room. */
  | { tag: "MessageInFlight"; value: { roomId: string } }
  /** A notification on watch `watchId` arrived for the room. */
  | { tag: "Notification"; value: { roomId: string; watchId: string } };

/** What a wake did. */
export type ChatWakeOutcome =
  /** The core holds the worker until `untilMs`, milliseconds since the epoch. */
  | { tag: "Held"; value: { untilMs: number } }
  /** The product used its notification wakes of this hour. */
  | { tag: "BudgetSpent" };

/** The arguments of `wakeProduct` on a WASM product runtime. */
export function wakeArguments(
  reason: ChatWakeReason,
): [roomId: string | undefined, watchId: string | undefined] {
  switch (reason.tag) {
    case "ProductStart":
      return [undefined, undefined];
    case "MessageInFlight":
      return [reason.value.roomId, undefined];
    case "Notification":
      return [reason.value.roomId, reason.value.watchId];
  }
}

/** The outcome of `wakeProduct` on a WASM product runtime. */
export function wakeOutcome(untilMs: number | undefined): ChatWakeOutcome {
  return untilMs === undefined
    ? { tag: "BudgetSpent" }
    : { tag: "Held", value: { untilMs } };
}

/**
 * The chat surface of a host runtime: the queries, the event publication,
 * wakes and attachment staging. A test host attaches it to the mock host, so
 * that the mock host can play the user.
 */
export interface ChatHostRuntime extends ChatHostQueries {
  /**
   * Give one event to the product's `action_subscribe` stream. The core
   * keeps it until the product subscribes, and gives a subscriber at the
   * first protocol version only the events that version can carry.
   */
  publishChatEvent(item: HostChatActionSubscribeItem): Promise<void>;
  /**
   * Give one event to the product's `room_events_subscribe` stream: mute and
   * archive, a call button, a call answer, a branch. The core keeps the
   * newest 64 events of a product that has no subscriber.
   */
  publishChatRoomEvent(item: HostChatRoomEventsSubscribeItem): Promise<void>;
  /**
   * Hold the worker of the product for 30 s. A notification wake also sends
   * `Woken` to the room, at most 12 times in one hour.
   */
  wakeProduct(reason: ChatWakeReason): Promise<ChatWakeOutcome>;
  /**
   * Store a user attachment of a room as a preimage that the Bulletin
   * allowance of the product pays for, and give its key. The core rejects a
   * room whose features do not declare `preimageAttachments`, an attachment
   * above 8 MiB or above the Bulletin chain's per-transaction bound, and a
   * product the user refused `PreimageSubmit`.
   */
  stageChatAttachment(
    roomId: string,
    mimeType: string,
    bytes: Uint8Array,
  ): Promise<Uint8Array>;
}

interface QueryCodecs {
  request: Codec<unknown>;
  item: Codec<unknown>;
  error: Codec<CallErrorValue<unknown>>;
}

const queryError = CallError(QueryErrorCodec) as Codec<CallErrorValue<unknown>>;

const QUERY_CODECS: Record<ChatQueryName, QueryCodecs> = {
  chatMessagesQuery: {
    request: MessagesRequestCodec as Codec<unknown>,
    item: MessagesItemCodec as Codec<unknown>,
    error: queryError,
  },
  chatUserAction: {
    request: UserActionRequestCodec as Codec<unknown>,
    item: UserActionItemCodec as Codec<unknown>,
    error: CallError(UserActionErrorCodec) as Codec<CallErrorValue<unknown>>,
  },
  chatReactionsQuery: {
    request: ReactionsRequestCodec as Codec<unknown>,
    item: ReactionsItemCodec as Codec<unknown>,
    error: queryError,
  },
  chatParticipantsQuery: {
    request: ParticipantsRequestCodec as Codec<unknown>,
    item: ParticipantsItemCodec as Codec<unknown>,
    error: queryError,
  },
  chatMessageActionsQuery: {
    request: MessageActionsRequestCodec as Codec<unknown>,
    item: MessageActionsItemCodec as Codec<unknown>,
    error: queryError,
  },
  chatSearchQuery: {
    request: SearchRequestCodec as Codec<unknown>,
    item: SearchItemCodec as Codec<unknown>,
    error: queryError,
  },
};

/** Encode one chat event for `publishChatEvent` on a core. */
export function encodeChatEvent(item: HostChatActionSubscribeItem): Uint8Array {
  return HostChatActionSubscribeItemCodec.enc(item);
}

/** Encode one room event for `publishChatRoomEvent` on a core. */
export function encodeChatRoomEvent(
  item: HostChatRoomEventsSubscribeItem,
): Uint8Array {
  return HostChatRoomEventsSubscribeItemCodec.enc(item);
}

/**
 * Ask one query and wait for its answer.
 *
 * The result is never a rejected promise: a request that does not encode, a
 * core that refuses to start the query, and a closed connection all give the
 * `HostFailure` refusal.
 */
export function runChatQuery(
  start: StartChatQuery,
  name: ChatQueryName,
  request: unknown,
  options: ChatQueryOptions = {},
): Promise<ChatQueryResult<unknown, unknown>> {
  const codecs = QUERY_CODECS[name];
  return new Promise((resolve) => {
    let settled = false;
    let stop: (() => void) | undefined;
    let timer: ReturnType<typeof setTimeout> | undefined;
    const settle = (
      result: ChatQueryResult<unknown, unknown>,
      stopQuery: boolean,
    ): void => {
      if (settled) return;
      settled = true;
      if (timer !== undefined) clearTimeout(timer);
      options.signal?.removeEventListener("abort", onAbort);
      if (stopQuery) stop?.();
      resolve(result);
    };
    const failure = (reason: string) =>
      err({ tag: "HostFailure" as const, value: { reason } });
    const onAbort = () => settle(err({ tag: "Cancelled" }), true);

    if (options.signal?.aborted) {
      resolve(err({ tag: "Cancelled" }));
      return;
    }
    let encoded: Uint8Array;
    try {
      encoded = codecs.request.enc(request);
    } catch (error) {
      resolve(
        failure(`${name} request did not encode: ${errorMessage(error)}`),
      );
      return;
    }

    const sink: ChatQuerySink = {
      onAnswer(item) {
        let decoded: unknown;
        try {
          decoded = codecs.item.dec(item);
        } catch (error) {
          settle(
            failure(`${name} item did not decode: ${errorMessage(error)}`),
            false,
          );
          return;
        }
        settle(ok(decoded), false);
      },
      onRefusal(error) {
        let decoded: CallErrorValue<unknown>;
        try {
          decoded = codecs.error.dec(error);
        } catch (cause) {
          settle(
            failure(`${name} error did not decode: ${errorMessage(cause)}`),
            false,
          );
          return;
        }
        settle(err(decoded), false);
      },
      onEnd() {
        settle(err({ tag: "NoAnswer" }), false);
      },
      onFailure(reason) {
        settle(failure(reason), false);
      },
    };

    try {
      stop = start(name, encoded, sink);
    } catch (error) {
      settle(failure(errorMessage(error)), false);
      return;
    }
    // The sink can settle the query before `start` returns. Stop the query
    // here, because `settle` had no `stop` to call at that time.
    if (settled) return;

    const timeoutMs = options.timeoutMs ?? chatQueryTimeoutMs(name, request);
    if (timeoutMs > 0 && Number.isFinite(timeoutMs)) {
      timer = setTimeout(
        () => settle(err({ tag: "Timeout" }), true),
        timeoutMs,
      );
    }
    options.signal?.addEventListener("abort", onAbort, { once: true });
  });
}

/** The typed queries over one way to start a query on a core. */
export function createChatHostQueries(start: StartChatQuery): ChatHostQueries {
  const ask =
    <Request, Item, E>(name: ChatQueryName) =>
    (request: Request, options?: ChatQueryOptions) =>
      runChatQuery(start, name, request, options) as Promise<
        ChatQueryResult<Item, E>
      >;
  return {
    chatMessagesQuery: ask("chatMessagesQuery"),
    chatUserAction: ask("chatUserAction"),
    chatReactionsQuery: ask("chatReactionsQuery"),
    chatParticipantsQuery: ask("chatParticipantsQuery"),
    chatMessageActionsQuery: ask("chatMessageActionsQuery"),
    chatSearchQuery: ask("chatSearchQuery"),
  };
}

/** One query open on a WASM product runtime. */
export interface WasmChatQueryHandle {
  cancel(): void;
  free(): void;
}

/** The query entry points of a WASM product runtime. */
export type WasmChatQueryCore = {
  [Name in ChatQueryName]: (
    request: Uint8Array,
    onAnswer: (item: Uint8Array) => void,
    onComplete: () => void,
    onError: (error: Uint8Array) => void,
  ) => WasmChatQueryHandle;
};

/**
 * Start one query on a WASM product runtime in this thread. The handle is
 * cancelled and freed once, when the query settles or stops.
 */
export function startWasmChatQuery(
  core: WasmChatQueryCore,
  name: ChatQueryName,
  request: Uint8Array,
  sink: ChatQuerySink,
): () => void {
  let handle: WasmChatQueryHandle | undefined;
  let done = false;
  const release = () => {
    if (done) return;
    done = true;
    // The terminal callbacks of the core can come before the handle exists.
    // The release then occurs when `start` gives the handle.
    if (!handle) return;
    handle.cancel();
    handle.free();
  };
  handle = core[name](
    request,
    (item) => {
      release();
      sink.onAnswer(item);
    },
    () => {
      release();
      sink.onEnd();
    },
    (error) => {
      release();
      sink.onRefusal(error);
    },
  );
  if (done) {
    handle.cancel();
    handle.free();
  }
  return release;
}
