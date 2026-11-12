import { expect, mock, test } from "bun:test";
import { okAsync } from "neverthrow";
import type {
  ChatMessageContent,
  HostChatActionSubscribeItem,
  HostChatCreateRoomRequest,
  HostChatCreateRoomResponse,
  HostChatListSubscribeItem,
  HostChatPostMessageRequest,
  HostChatPostMessageResponse,
  HostChatRegisterBotResponse,
  HostPocketListSubscribeItem,
  HostRendererActionSubscribeItem,
  Observer,
  ProductRendererRenderRequest,
  RendererNode,
  TrUApiClient,
} from "@parity/truapi";

/**
 * Mirrors the ES Observable interop key each generated subscription method
 * exposes (`Symbol.observable`, falling back to `"@@observable"` when the
 * well-known symbol is absent), so these fakes are recognized by `rxjs`'
 * `from(...)` exactly like a real generated `ObservableLike`.
 */
const OBSERVABLE_INTEROP: symbol | string =
  (typeof Symbol === "function" && (Symbol as { observable?: symbol }).observable) ||
  "@@observable";

function fakeObservable<Item>() {
  const observers = new Set<Partial<Observer<Item>>>();
  const observable = {
    subscribe(observer: Partial<Observer<Item>> = {}) {
      observers.add(observer);
      return {
        unsubscribe: () => observers.delete(observer),
        subscriptionId: "fake-subscription",
      };
    },
    [OBSERVABLE_INTEROP as typeof Symbol.observable]() {
      return observable;
    },
  };
  return {
    observable,
    emit(value: Item): void {
      for (const observer of observers) observer.next?.(value);
    },
  };
}

/**
 * Guards the one row no automated run can satisfy: if
 * `Renderer/action_subscribe` ever waits for a delivered action instead of the
 * open subscription, it stays `running` and the final report never posts.
 */
test("worker diagnosis completes after !diagnose alone, without a renderer action", async () => {
  const chatActions = fakeObservable<HostChatActionSubscribeItem>();
  const chatRooms = fakeObservable<HostChatListSubscribeItem>();
  const rendererActions = fakeObservable<HostRendererActionSubscribeItem>();
  const pocketCards = fakeObservable<HostPocketListSubscribeItem>();

  const postMessageCalls: HostChatPostMessageRequest[] = [];
  let createRoomCalls = 0;
  let registerBotCalls = 0;
  let onRenderHandler:
    | ((
        request: ProductRendererRenderRequest,
        send: (node: RendererNode) => void,
        interrupt: (reason?: unknown) => void,
      ) => (() => void) | void)
    | undefined;

  const client = {
    chat: {
      createRoom(request: HostChatCreateRoomRequest) {
        createRoomCalls += 1;
        // First call is the static Playground room; the rest are the
        // per-run diagnostic room (New, then Exists).
        if (createRoomCalls === 1) {
          return okAsync<HostChatCreateRoomResponse, never>({ status: "Exists" });
        }
        const status = createRoomCalls === 2 ? "New" : "Exists";
        if (status === "New") {
          chatRooms.emit({
            rooms: [{ roomId: request.roomId, participatingAs: "RoomHost" }],
          });
        }
        return okAsync<HostChatCreateRoomResponse, never>({ status });
      },
      registerBot() {
        registerBotCalls += 1;
        return okAsync<HostChatRegisterBotResponse, never>({
          status: registerBotCalls === 1 ? "New" : "Exists",
        });
      },
      listSubscribe: () => chatRooms.observable,
      setRoomFooter: () => okAsync<undefined, never>(undefined),
      postMessage(request: HostChatPostMessageRequest) {
        postMessageCalls.push(request);
        const payload: ChatMessageContent = request.payload;
        if (payload.tag === "Custom") {
          const messageId = "custom-message-id";
          // Stand in for the host asking the product to render the custom
          // message it just posted.
          onRenderHandler?.(
            {
              context: {
                tag: "ChatMessage",
                value: {
                  roomId: request.roomId,
                  messageId,
                  messageType: payload.value.messageType,
                },
              },
              payload: payload.value.payload,
            },
            () => {},
            () => {},
          );
          return okAsync<HostChatPostMessageResponse, never>({ messageId });
        }
        return okAsync<HostChatPostMessageResponse, never>({
          messageId: `text-message-${postMessageCalls.length}`,
        });
      },
      actionSubscribe: () => chatActions.observable,
    },
    renderer: {
      onRender(handler: typeof onRenderHandler) {
        onRenderHandler = handler;
        return { unsubscribe() {} };
      },
      actionSubscribe: () => rendererActions.observable,
    },
    // The worker observes its Pocket collection on startup. The generated
    // client always exposes the surface, so a stub without it diverges from
    // what the worker actually runs against.
    pocket: {
      listSubscribe: () => pocketCards.observable,
    },
  };

  mock.module("@parity/truapi/sandbox", () => ({
    getClientSync: () => client as unknown as TrUApiClient,
  }));

  await import("../../worker/index");

  // Startup already posted the text and custom messages; no renderer action
  // has been delivered by anyone.
  expect(postMessageCalls.length).toBe(2);

  const chatRoomId = postMessageCalls[0]?.roomId;
  chatActions.emit({
    roomId: chatRoomId ?? "",
    peer: "diagnosis-peer",
    payload: {
      tag: "MessagePosted",
      value: { tag: "Text", value: { text: "!diagnose" } },
    },
  });

  const finalReport = postMessageCalls.at(-1);
  if (finalReport?.payload.tag !== "Text") {
    throw new Error("expected the final diagnosis report as a Text message");
  }
  const report = finalReport.payload.value.text;

  expect(report).toContain("## Truapi Chat Diagnosis");
  expect(report).not.toContain("❌");
  expect(report).toMatch(/`Renderer\/action_subscribe` \| ✅ \| renderer action stream is open/);
  expect(report).not.toContain("received a renderer action for the diagnosis message");
});
