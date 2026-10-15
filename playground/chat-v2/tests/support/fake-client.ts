/**
 * A fake of the client parts the Worker uses. It records every call and lets
 * a test play the host: send events, ask queries, stop them.
 *
 * The end-to-end tests of P2.7 replace this fake with the client of the test
 * host (`createMockHost` of `@parity/truapi-host/testing`) and give it to the
 * same `startSampleProduct`.
 */
import { errAsync, okAsync } from "neverthrow";
import type {
  HostChatActionSubscribeItem,
  HostChatCapabilitiesResponse,
  HostChatRoomEventsSubscribeItem,
  HostRendererActionSubscribeItem,
} from "@parity/truapi";
import type { SampleClient } from "../../worker/context";

export interface RecordedCall {
  method: string;
  request: unknown;
}

type Handler = (request: never, send: (item: never) => void, interrupt: (reason?: never) => void) => (() => void) | void;

export interface QueryOutcome {
  item?: unknown;
  interrupt?: unknown;
}

interface Observer<Item> {
  next?: (item: Item) => void;
  error?: (error: unknown) => void;
}

function observable<Item>(refusal?: () => unknown) {
  const observers = new Set<Observer<Item>>();
  return {
    observable: {
      subscribe(observer: Observer<Item> = {}) {
        const refused = refusal?.();
        if (refused !== undefined) {
          observer.error?.(refused);
          return { unsubscribe: () => {}, subscriptionId: "fake" };
        }
        observers.add(observer);
        return { unsubscribe: () => observers.delete(observer), subscriptionId: "fake" };
      },
    },
    emit(item: Item) {
      for (const observer of observers) observer.next?.(item);
    },
  };
}

/**
 * How the fake fails one call: `error` plays a host that answers with that
 * `CallError`, and `reject` plays a client whose promise rejects, as a
 * closed connection does.
 */
export type FakeFailure = { error: unknown } | { reject: Error };

/** The `CallError` of a call that the core refuses with `variant`. */
export function domainError(variant: unknown): unknown {
  return { tag: "Domain", value: { tag: "V1", value: variant } };
}

export interface FakeClientOptions {
  /** Methods the fake refuses with `Unknown`. */
  refuse?: ReadonlySet<string>;
  /**
   * Methods the fake does not know, as a host before the Chat additions:
   * they answer `Unsupported`. `roomEventsSubscribe` ends at once.
   */
  unknown?: ReadonlySet<string>;
  /** The answer of `capabilities`. */
  capabilities?: HostChatCapabilitiesResponse;
  /**
   * Fail one call of `method`. `index` counts the calls of that method from
   * 1. Give `undefined` to let the call succeed.
   */
  fail?: (method: string, index: number, request: unknown) => FakeFailure | undefined;
  /** The product local storage; share it to play a worker restart. */
  storage?: Map<string, `0x${string}`>;
}

export function createFakeClient(options: FakeClientOptions = {}) {
  const calls: RecordedCall[] = [];
  const handlers = new Map<string, Handler>();
  const actions = observable<HostChatActionSubscribeItem>();
  const roomEvents = observable<HostChatRoomEventsSubscribeItem>(() =>
    options.unknown?.has("roomEventsSubscribe") ? { tag: "Unsupported" } : undefined,
  );
  const rendererActions = observable<HostRendererActionSubscribeItem>();
  const storage = options.storage ?? new Map<string, `0x${string}`>();
  let hostIds = 0;
  const counts = new Map<string, number>();

  const unary =
    (method: string, answer: (request: never) => unknown = () => undefined) =>
    (request: never) => {
      calls.push({ method, request });
      const index = (counts.get(method) ?? 0) + 1;
      counts.set(method, index);
      if (options.unknown?.has(method)) return errAsync({ tag: "Unsupported" });
      if (options.refuse?.has(method)) {
        return errAsync(domainError({ tag: "Unknown", value: { reason: "refused by the fake" } }));
      }
      const failure = options.fail?.(method, index, request);
      if (failure && "reject" in failure) return Promise.reject(failure.reject);
      if (failure) return errAsync(failure.error);
      return okAsync(answer(request));
    };

  const register = (name: string) => (handler: Handler) => {
    handlers.set(name, handler);
    return { unsubscribe: () => handlers.delete(name) };
  };

  const client = {
    chat: {
      createRoom: unary("createRoom", () => ({ status: "New" })),
      postMessage: unary("postMessage", () => ({ messageId: `host-assigned-${++hostIds}` })),
      setRoomFooter: unary("setRoomFooter"),
      setRoomConfig: unary("setRoomConfig"),
      setRoomMetadata: unary("setRoomMetadata"),
      setRoomPreview: unary("setRoomPreview"),
      deleteRoom: unary("deleteRoom"),
      updateFeed: unary("updateFeed"),
      setRoomToolbar: unary("setRoomToolbar"),
      setRoomSuggestions: unary("setRoomSuggestions"),
      setRoomCommands: unary("setRoomCommands"),
      setRoomActivity: unary("setRoomActivity"),
      capabilities: unary(
        "capabilities",
        () =>
          options.capabilities ?? {
            features: ["call_buttons", "search"],
            inlineAttachmentMaxBytes: 1_048_576,
            preimageAttachmentMaxBytes: 0,
          },
      ),
      setRoomFeatures: unary("setRoomFeatures"),
      actionSubscribe: () => actions.observable,
      roomEventsSubscribe: () => roomEvents.observable,
      onMessagesQuery: register("messages_query"),
      onUserAction: register("user_action"),
      onReactionsQuery: register("reactions_query"),
      onParticipantsQuery: register("participants_query"),
      onMessageActionsQuery: register("message_actions_query"),
      onParticipantActionsQuery: register("participant_actions_query"),
    },
    renderer: {
      onRender: register("render"),
      actionSubscribe: () => rendererActions.observable,
    },
    localStorage: {
      read: ({ key }: { key: string }) => okAsync({ value: storage.get(key) }),
      write: ({ key, value }: { key: string; value: `0x${string}` }) => {
        storage.set(key, value);
        return okAsync(undefined);
      },
    },
    preimage: {
      submit: unary("preimageSubmit", () => `0x${"ab".repeat(32)}`),
    },
  };

  return {
    client: client as unknown as SampleClient,
    calls,
    handlers,
    storage,
    /** Calls of one method, in order. */
    callsOf(method: string): unknown[] {
      return calls.filter((call) => call.method === method).map((call) => call.request);
    },
    emitEvent(item: HostChatActionSubscribeItem) {
      actions.emit(item);
    },
    emitRoomEvent(item: HostChatRoomEventsSubscribeItem) {
      roomEvents.emit(item);
    },
    emitRendererAction(item: HostRendererActionSubscribeItem) {
      rendererActions.emit(item);
    },
    /**
     * Start a query as the host does. The promise settles on the first item
     * or interrupt; `stop` plays the host that stops the stream.
     */
    ask(name: string, request: unknown): { outcome: Promise<QueryOutcome>; stop: () => void } {
      const handler = handlers.get(name);
      if (!handler) throw new Error(`no handler for ${name}`);
      let settle: (outcome: QueryOutcome) => void = () => {};
      const outcome = new Promise<QueryOutcome>((resolve) => (settle = resolve));
      const teardown = handler(
        request as never,
        ((item: unknown) => settle({ item })) as (item: never) => void,
        ((reason?: unknown) => settle({ interrupt: reason })) as (reason?: never) => void,
      );
      return { outcome, stop: () => teardown?.() };
    },
  };
}

export type FakeClient = ReturnType<typeof createFakeClient>;
