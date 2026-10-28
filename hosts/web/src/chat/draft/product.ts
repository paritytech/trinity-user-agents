import type { CallErrorValue } from "@parity/truapi/scale";
import { err, ok, type Result } from "neverthrow";
import {
  CHAT_V2_CONNECT,
  CHAT_V2_DRAFT,
  messagePortBridge,
  type BridgePort,
  type ChatV2Connect,
  type ChatV2Envelope,
  type HostChatRoomEventsSubscribeItem,
  type HostQueries,
  type ProductCalls,
  type ProductMethod,
  type QueryMethod,
} from "./contract.js";

/**
 * DRAFT, NOT TRUAPI. A product worker's end of the Chat Extension v2 draft
 * bridge (`./contract.ts`), with the method names of the reference contract:
 * `truapi.chat.createRoom`, `updateFeed`, `onMessagesQuery` and so on become
 * `chatV2.createRoom`, `chatV2.updateFeed`, `chatV2.onMessagesQuery`, and
 * `truapi.callMedia.*` becomes `chatV2.callMedia.*`.
 *
 * It runs in the product's own `Worker` frame, which the host runs through
 * the ordinary worker loader. None of these calls are TrUAPI frames.
 */

/** What answers one host query: send one item, or interrupt. A second answer is ignored. */
export type QueryHandler<M extends QueryMethod> = (
  request: HostQueries[M][0],
  send: (item: HostQueries[M][1]) => void,
  interrupt: (error: CallErrorValue<HostQueries[M][2]>) => void,
) => void | Promise<void>;

type Answer<M extends ProductMethod> = Promise<
  Result<ProductCalls[M][1], CallErrorValue<ProductCalls[M][2]>>
>;

export interface ChatV2Product {
  createRoom(request: ProductCalls["createRoom"][0]): Answer<"createRoom">;
  setRoomMetadata(
    request: ProductCalls["setRoomMetadata"][0],
  ): Answer<"setRoomMetadata">;
  setRoomPreview(
    request: ProductCalls["setRoomPreview"][0],
  ): Answer<"setRoomPreview">;
  setRoomFeatures(
    request: ProductCalls["setRoomFeatures"][0],
  ): Answer<"setRoomFeatures">;
  setRoomCall(request: ProductCalls["setRoomCall"][0]): Answer<"setRoomCall">;
  /** DRAFT: the one person a room is a conversation with; the host names and draws them. */
  setRoomContact(
    request: ProductCalls["setRoomContact"][0],
  ): Answer<"setRoomContact">;
  deleteRoom(request: ProductCalls["deleteRoom"][0]): Answer<"deleteRoom">;
  updateFeed(request: ProductCalls["updateFeed"][0]): Answer<"updateFeed">;
  capabilities(): Answer<"capabilities">;
  /** Follow `room_events_subscribe`. A second subscription replaces the first. */
  roomEventsSubscribe(
    listener: (item: HostChatRoomEventsSubscribeItem) => void,
  ): Answer<"roomEventsSubscribe">;
  onMessagesQuery(handler: QueryHandler<"messagesQuery">): void;
  onUserAction(handler: QueryHandler<"userAction">): void;
  onParticipantsQuery(handler: QueryHandler<"participantsQuery">): void;
  /** DRAFT: the user chose someone in the host's picker and asked to start a conversation. */
  onStartConversation(handler: QueryHandler<"startConversation">): void;
  callMedia: {
    start(request: ProductCalls["callMediaStart"][0]): Answer<"callMediaStart">;
    signal(
      request: ProductCalls["callMediaSignal"][0],
    ): Answer<"callMediaSignal">;
    stop(request: ProductCalls["callMediaStop"][0]): Answer<"callMediaStop">;
  };
  close(): void;
}

/** Connect over `port`, an end of the bridge the host already holds. */
export function chatV2Over(port: BridgePort): ChatV2Product {
  let nextId = 1;
  const calls = new Map<
    number,
    (result: Result<unknown, CallErrorValue<unknown>>) => void
  >();
  const handlers = new Map<QueryMethod, QueryHandler<QueryMethod>>();
  const open = new Set<number>();
  let events: ((item: HostChatRoomEventsSubscribeItem) => void) | null = null;
  let closed = false;

  const call = <M extends ProductMethod>(
    method: M,
    request: ProductCalls[M][0],
  ): Answer<M> => {
    if (closed)
      return Promise.resolve(
        err({ tag: "HostFailure", value: { reason: "The bridge is closed." } }),
      );
    const id = nextId++;
    return new Promise((resolve) => {
      calls.set(id, (result) => resolve(result as Awaited<Answer<M>>));
      port.post({ kind: "call", id, method, request } as ChatV2Envelope);
    });
  };

  const answerQuery = (
    id: number,
    method: QueryMethod,
    request: unknown,
  ): void => {
    const handler = handlers.get(method);
    if (handler === undefined) {
      port.post({ kind: "refusal", id, error: { tag: "Unsupported" } });
      return;
    }
    open.add(id);
    const finish = (message: ChatV2Envelope) => {
      if (!open.delete(id)) return;
      port.post(message);
    };
    try {
      const done = handler(
        request as never,
        (item) => finish({ kind: "answer", id, item }),
        (error) => finish({ kind: "refusal", id, error }),
      );
      void Promise.resolve(done).catch((error: unknown) =>
        finish({
          kind: "refusal",
          id,
          error: {
            tag: "Domain",
            value: { tag: "Unknown", value: { reason: String(error) } },
          },
        }),
      );
    } catch (error) {
      finish({
        kind: "refusal",
        id,
        error: {
          tag: "Domain",
          value: { tag: "Unknown", value: { reason: String(error) } },
        },
      });
    }
  };

  port.listen((message) => {
    switch (message.kind) {
      case "result": {
        const resolve = calls.get(message.id);
        calls.delete(message.id);
        resolve?.(message.ok ? ok(message.value) : err(message.error));
        return;
      }
      case "query":
        answerQuery(message.id, message.method, message.request);
        return;
      case "cancel":
        // The host stopped waiting. An answer after this reaches nothing.
        open.delete(message.id);
        return;
      case "roomEvent":
        events?.(message.item);
        return;
    }
  });

  return {
    createRoom: (request) => call("createRoom", request),
    setRoomMetadata: (request) => call("setRoomMetadata", request),
    setRoomPreview: (request) => call("setRoomPreview", request),
    setRoomFeatures: (request) => call("setRoomFeatures", request),
    setRoomCall: (request) => call("setRoomCall", request),
    setRoomContact: (request) => call("setRoomContact", request),
    deleteRoom: (request) => call("deleteRoom", request),
    updateFeed: (request) => call("updateFeed", request),
    capabilities: () => call("capabilities", {}),
    roomEventsSubscribe(listener) {
      events = listener;
      return call("roomEventsSubscribe", {});
    },
    onMessagesQuery: (handler) =>
      void handlers.set("messagesQuery", handler as QueryHandler<QueryMethod>),
    onUserAction: (handler) =>
      void handlers.set("userAction", handler as QueryHandler<QueryMethod>),
    onStartConversation: (handler) =>
      void handlers.set(
        "startConversation",
        handler as QueryHandler<QueryMethod>,
      ),
    onParticipantsQuery: (handler) =>
      void handlers.set(
        "participantsQuery",
        handler as QueryHandler<QueryMethod>,
      ),
    callMedia: {
      start: (request) => call("callMediaStart", request),
      signal: (request) => call("callMediaSignal", request),
      stop: (request) => call("callMediaStop", request),
    },
    close() {
      closed = true;
      port.close();
      for (const resolve of calls.values())
        resolve(
          err({
            tag: "HostFailure",
            value: { reason: "The bridge is closed." },
          }),
        );
      calls.clear();
    },
  };
}

/**
 * Hand the host this frame's end of the bridge, and connect over it. The
 * host takes it only from a product `Worker` frame it opened, on its own
 * origin, where a worker runs.
 */
export function connectChatV2(
  parent: Window = window.parent,
  hostOrigin: string = location.origin,
): ChatV2Product {
  const channel = new MessageChannel();
  parent.postMessage(
    {
      type: CHAT_V2_CONNECT,
      protocol: CHAT_V2_DRAFT,
    } satisfies ChatV2Connect,
    hostOrigin,
    [channel.port2],
  );
  return chatV2Over(messagePortBridge(channel.port1));
}
