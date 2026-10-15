/**
 * The answers to the host-initiated queries, as pure functions of the state.
 * `product.ts` adds the delay, the failure injection, the log and the wire.
 */
import type {
  ProductChatMessageActionsQueryItem,
  ProductChatMessageActionsQueryRequest,
  ProductChatMessagesQueryItem,
  ProductChatMessagesQueryRequest,
  ProductChatParticipantActionsQueryItem,
  ProductChatParticipantsQueryItem,
  ProductChatParticipantsQueryRequest,
  ProductChatQueryError,
  ProductChatReactionsQueryItem,
  ProductChatReactionsQueryRequest,
  ProductChatSearchQueryItem,
  ProductChatSearchQueryRequest,
} from "@parity/truapi";
import { isThreadRoom, type RoomId } from "./clock";
import {
  ACTION_MENTION,
  ACTION_PIN,
  ACTION_REPORT,
  ACTION_THREAD,
  actionOpenApp,
  participantAt,
  participantCount,
  participantIndex,
} from "./fixtures";
import type { ProductState } from "./state";
import { StoreError, type SourceStore } from "./store";

export type Answer<Item> = { ok: Item } | { error: ProductChatQueryError };

const ROOMS: readonly RoomId[] = ["host", "source", "host-empty"];

/** The most participants one page holds. */
export const MAX_PARTICIPANT_PAGE = 100;
/** The most ids one `ByIds` question holds, the bound of the RFC. */
export const MAX_BY_IDS = 100;

/** A room of the product: a fixed one, or a thread room it created. */
export function knownRoom(roomId: string, rooms?: Partial<Record<RoomId, unknown>>): RoomId | undefined {
  const fixed = ROOMS.find((room) => room === roomId);
  if (fixed) return fixed;
  return isThreadRoom(roomId) && rooms?.[roomId] !== undefined ? roomId : undefined;
}

const UNKNOWN_ROOM: { error: ProductChatQueryError } = { error: { tag: "UnknownRoom" } };
const NOT_AVAILABLE: { error: ProductChatQueryError } = { error: { tag: "NotAvailable" } };

function unknown(reason: string): { error: ProductChatQueryError } {
  return { error: { tag: "Unknown", value: { reason } } };
}

export interface QueryContext {
  rooms: ProductState["rooms"];
  store: SourceStore;
  product?: string;
}

export function answerMessages(
  context: QueryContext,
  request: ProductChatMessagesQueryRequest,
): Answer<ProductChatMessagesQueryItem> {
  if (request.roomId !== "source") return UNKNOWN_ROOM;
  try {
    return { ok: context.store.page(request.anchor, request.pageSize) };
  } catch (error) {
    if (error instanceof StoreError) return unknown(error.message);
    throw error;
  }
}

export function answerReactions(
  context: QueryContext,
  request: ProductChatReactionsQueryRequest,
): Answer<ProductChatReactionsQueryItem> {
  const room = knownRoom(request.roomId, context.rooms);
  if (!room) return UNKNOWN_ROOM;
  const settings = context.rooms[room]!;
  if (settings.reactions !== "on") return NOT_AVAILABLE;
  if (room !== "source") {
    return { ok: { reactions: [...(settings.reactionBook[request.messageId] ?? [])] } };
  }
  try {
    return { ok: { reactions: context.store.reactionDetails(request.messageId) } };
  } catch (error) {
    if (error instanceof StoreError) return unknown(error.message);
    throw error;
  }
}

export function answerParticipants(
  context: QueryContext,
  request: ProductChatParticipantsQueryRequest,
): Answer<ProductChatParticipantsQueryItem> {
  const room = knownRoom(request.roomId, context.rooms);
  if (!room) return UNKNOWN_ROOM;
  const guests = context.rooms[room]!.guests;
  const total = participantCount(guests);
  const { query } = request;
  switch (query.tag) {
    case "Count":
      return { ok: { tag: "Count", value: { count: total } } };
    case "Page": {
      const cursor = query.value.cursor;
      if (cursor !== undefined && !/^(0|[1-9][0-9]*)$/.test(cursor)) return unknown(`bad cursor ${cursor}`);
      const start = cursor === undefined ? 0 : Number(cursor);
      const size = Math.max(1, Math.min(query.value.pageSize, MAX_PARTICIPANT_PAGE));
      const end = Math.min(total, start + size);
      const participants = [];
      for (let index = start + 1; index <= end; index++) participants.push(participantAt(index));
      return { ok: { tag: "Page", value: { participants, ...(end < total ? { next: String(end) } : {}) } } };
    }
    case "ByIds": {
      const ids = query.value.participantIds.slice(0, MAX_BY_IDS);
      const participants = ids.flatMap((id) => {
        const index = participantIndex(id, guests);
        return index === undefined ? [] : [participantAt(index)];
      });
      return { ok: { tag: "Resolved", value: { participants } } };
    }
  }
}

export function answerMessageActions(
  context: QueryContext,
  request: ProductChatMessageActionsQueryRequest,
): Answer<ProductChatMessageActionsQueryItem> {
  const room = knownRoom(request.roomId, context.rooms);
  if (!room) return UNKNOWN_ROOM;
  const settings = context.rooms[room]!;
  if (settings.deeplinkMessages.includes(request.message.messageId)) {
    return { ok: { menu: { tag: "Custom", value: { actions: [actionOpenApp(context.product)] } } } };
  }
  switch (settings.menu) {
    case "custom":
      return { ok: { menu: { tag: "Custom", value: { actions: [ACTION_PIN, ACTION_REPORT] } } } };
    case "extend":
      return { ok: { menu: { tag: "ExtendBuiltIn", value: { actions: [ACTION_PIN], hidden: [] } } } };
    case "hide-edit":
      return { ok: { menu: { tag: "ExtendBuiltIn", value: { actions: [ACTION_PIN], hidden: ["Edit"] } } } };
    case "thread":
      return { ok: { menu: { tag: "ExtendBuiltIn", value: { actions: [ACTION_THREAD], hidden: [] } } } };
    case "none":
      // The host asks only after `/deeplink` turned the menu on: answer the
      // host's own menu.
      return { ok: { menu: { tag: "ExtendBuiltIn", value: { actions: [], hidden: [] } } } };
  }
}

/** The page of `search_query`: newest first, with the cursor `<index>`. */
export function answerSearch(
  context: QueryContext,
  request: ProductChatSearchQueryRequest,
): Answer<ProductChatSearchQueryItem> {
  if (request.roomId !== "source") return UNKNOWN_ROOM;
  const settings = context.rooms.source;
  if (!settings.features.search || settings.search === "na") return NOT_AVAILABLE;
  try {
    const page = context.store.search(request.text, request.cursor, request.pageSize);
    return { ok: { hits: page.messages, ...(page.next === undefined ? {} : { next: page.next }) } };
  } catch (error) {
    if (error instanceof StoreError) return unknown(error.message);
    throw error;
  }
}

export function answerParticipantActions(
  context: QueryContext,
  request: { roomId: string },
): Answer<ProductChatParticipantActionsQueryItem> {
  if (!knownRoom(request.roomId, context.rooms)) return UNKNOWN_ROOM;
  return { ok: { menu: { tag: "ExtendBuiltIn", value: { actions: [ACTION_MENTION] } } } };
}
