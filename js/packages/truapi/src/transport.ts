import { concatBytes } from "@noble/hashes/utils.js";
import { err, ok, type Result, type ResultAsync } from "neverthrow";

import { str, u8, type CallErrorValue, type ResultPayload } from "./scale.js";

/**
 * Wire trait discriminant reserved for method-independent protocol errors. No
 * API trait may declare it, so no method is ever addressed here.
 **/
export const PROTOCOL_ERROR_TRAIT_ID = 255 as const;

/** Wire method discriminant reserved for method-independent protocol errors. **/
export const PROTOCOL_ERROR_METHOD_ID = 255 as const;

/** The peer rejected an outbound frame because it does not support its API. **/
export class UnsupportedMessageError extends Error {
  /** Trait discriminant of the unsupported outbound frame. **/
  readonly traitId: number;

  /** Method discriminant of the unsupported outbound frame. **/
  readonly methodId: number;

  constructor(traitId: number, methodId: number) {
    super(`Peer does not support wire message (${traitId}, ${methodId})`);
    this.name = "UnsupportedMessageError";
    this.traitId = traitId;
    this.methodId = methodId;
  }
}

/** The host connection ended; interrupted operations are not retried. */
export class ConnectionResetError extends Error {
  constructor(options?: ErrorOptions) {
    super("TrUAPI host connection interrupted", options);
    this.name = "ConnectionResetError";
  }
}

/** Call result returned when the peer does not recognize a request frame. **/
export type UnsupportedCallError = Extract<
  CallErrorValue<never>,
  { tag: "Unsupported" }
>;

/**
 * Coerce an unknown thrown value into an `Error` instance.
 */
function toError(error: unknown): Error {
  return error instanceof Error ? error : new Error(String(error));
}

/**
 * Handle returned by TrUAPI subscription APIs.
 **/
export interface Subscription {
  /**
   * Stop the subscription. Calling this more than once has no additional effect.
   **/
  unsubscribe: () => void;

  /**
   * Transport-assigned request id for the subscription start frame.
   *
   * Methods that accept a `followSubscriptionId` use this value to scope
   * follow-up requests to a specific active subscription.
   **/
  subscriptionId: string;
}

/**
 * Terminal error delivered through `Observer.error` for every non-normal
 * subscription end. When the peer interrupted the stream with a typed payload,
 * `reason` carries the decoded `Reason`; otherwise `reason` is `undefined` and
 * the underlying transport/decode error is preserved on `cause`.
 *
 * Discriminate with `error.reason !== undefined` (or `'reason' in error`).
 **/
export class SubscriptionError<Reason = never> extends Error {
  /**
   * Typed payload supplied by the peer when it interrupted the subscription.
   * `undefined` when the stream ended for any other reason (transport close,
   * decode failure, malformed interrupt payload).
   **/
  readonly reason?: Reason;

  constructor(message: string, options?: { reason?: Reason; cause?: unknown }) {
    super(
      message,
      options?.cause !== undefined ? { cause: options.cause } : undefined,
    );
    this.name = "SubscriptionError";
    if (options?.reason !== undefined) this.reason = options.reason;
  }
}

/**
 * Minimal Observable-compatible observer shape used by generated subscription
 * APIs without depending on RxJS.
 *
 * `Reason` is the typed interrupt payload for the originating subscription.
 * Methods without a typed interrupt resolve `Reason` to `never`, leaving
 * `error.reason` typed as `undefined`.
 **/
export interface Observer<Item, Reason = never> {
  /**
   * Called with each successfully decoded subscription item.
   **/
  next(value: Item): void;

  /**
   * Called once when the stream terminates with an error. Inspect
   * `error.reason` to distinguish a typed peer interrupt from a transport or
   * decode failure (`error.cause` carries the underlying failure in the
   * latter case).
   **/
  error(error: SubscriptionError<Reason>): void;

  /**
   * Called once when the peer normally completes the stream.
   **/
  complete(): void;
}

declare global {
  interface SymbolConstructor {
    readonly observable: unique symbol;
  }
}

/**
 * Minimal Observable-compatible object returned by generated subscription APIs.
 *
 * Implements the ES Observable interop protocol so that consumers can pass
 * an instance straight to `rxjs.from(...)`.
 **/
export interface ObservableLike<Item, Reason = never> {
  /**
   * Start the stream and receive `next`, `error`, and `complete` callbacks.
   **/
  subscribe(observer?: Partial<Observer<Item, Reason>>): Subscription;
  /**
   * Observable interop hook. Returns `this`.
   **/
  [Symbol.observable](): ObservableLike<Item, Reason>;
}

/**
 * Product-side handler for a subscription the native host initiates.
 *
 * It receives the decoded request and two callbacks: `send` delivers one item
 * to the host, and `interrupt` ends the stream, cleanly when called with no
 * argument and with the method's interrupt value otherwise. The returned
 * teardown, if any, runs once the stream ends: on the host's stop frame, on
 * `interrupt`, when the transport closes, or when the host restarts the same
 * request id.
 **/
export type HostInitiatedSubscriptionHandler<Request, Item, Reason = never> = (
  request: Request,
  send: (item: Item) => void,
  interrupt: (reason?: Reason) => void,
) => (() => void) | void;

/**
 * Wire discriminant pair addressing a method. One id addresses a method
 * regardless of shape (request/response, or a subscription's four phases):
 * which leg of the exchange a frame carries is the wire's own `messageType`
 * byte, not a separate id per leg.
 **/
export interface MethodIds {
  /**
   * Wire trait discriminant.
   **/
  trait: number;

  /**
   * Wire method discriminant within the trait.
   **/
  method: number;

  /**
   * Whether this method's legs follow the request/response shape or the
   * subscription shape (`"subscription"` covers both plain and result
   * subscriptions, which share the same four-leg wire shape). The one piece
   * of shape a payload-blind reader needs to interpret a frame's own
   * `messageType` byte without decoding the payload.
   **/
  kind: "request" | "subscription";
}

/**
 * Per-call options every generated request method accepts as its last
 * argument.
 **/
export interface CallOptions {
  /** See {@link RequestParams.signal}. **/
  signal?: AbortSignal;
}

/**
 * Options accepted by `TrUApiTransport.request`.
 **/
export interface RequestParams<Ok, Err> {
  /**
   * Wire discriminants for this request method.
   **/
  ids: MethodIds;

  /**
   * SCALE-encoded request wrapper payload bytes (its own `V<N>` tag is the
   * wire's only version signal), constructed by the generated caller.
   **/
  payload: Uint8Array;

  /**
   * Decode a `Response`-leg frame's raw payload bytes into the typed Ok/Err
   * outcome. Implementations decode `Result<{Method}Response,
   * CallError<{Method}Error>>` directly. The transport unwraps the result
   * into `ResultAsync<Ok, Err | UnsupportedCallError>`.
   **/
  decodeResponse: (payload: Uint8Array) => ResultPayload<Ok, Err>;

  /**
   * Withdraw the call. Aborting sends a `Cancel` frame on this method's own
   * address; the promise still settles on the response the host sends, which
   * for a call the host stopped is `CallError::Cancelled`. A signal already
   * aborted when the call is made sends nothing and rejects immediately.
   *
   * A host that predates the `Cancel` leg drops the frame, so an aborted call
   * against one settles on its deadline instead. There is no way to detect that
   * first: `system.featureSupported` answers only about chains, so an abort
   * against an older host is indistinguishable from one it honoured.
   **/
  signal?: AbortSignal;
}

/**
 * Options accepted by `TrUApiTransport.subscribeRaw`.
 **/
export interface SubscribeRawParams {
  /**
   * Wire discriminants for this subscription method.
   **/
  ids: MethodIds;

  /**
   * SCALE-encoded `Start`-leg payload bytes: the request wrapper's own
   * encoding, or empty bytes for a method with no request at all,
   * constructed by the generated caller.
   **/
  payload: Uint8Array;

  /**
   * Called with a `Receive`-leg frame's raw payload bytes.
   **/
  onReceive: (payload: Uint8Array) => void;

  /**
   * Called with an `Interrupt`-leg frame's raw payload bytes.
   **/
  onInterrupt?: (payload: Uint8Array) => void;

  /**
   * Called when a transport-level error or unsupported start frame terminates
   * the subscription.
   **/
  onClose?: (error: Error) => void;
}

/** Product-side registration for one host-initiated subscription method. **/
export interface HostInitiatedSubscriptionRegistration<
  Request,
  Item,
  Reason = never,
> {
  /** Install or replace the handler used for future start frames. **/
  setHandler(
    handler: HostInitiatedSubscriptionHandler<Request, Item, Reason>,
  ): {
    unsubscribe(): void;
  };
}

/** Options used to register a host-initiated subscription method. **/
export interface RegisterHostInitiatedSubscriptionParams<
  Request,
  Item,
  Reason = never,
> {
  /** Wire discriminants for the host-initiated subscription. **/
  ids: MethodIds;
  /**
   * Decode a `Start`-leg frame's raw payload bytes into the typed request.
   **/
  decodeRequest(payload: Uint8Array): Request;
  /**
   * Encode one product emission as a `Receive`-leg frame's raw payload bytes.
   **/
  encodeItem(item: Item): Uint8Array;
  /**
   * Encode an `Interrupt`-leg frame's raw payload bytes: the stream's clean
   * end when `reason` is omitted, and the method's interrupt value otherwise.
   **/
  encodeInterrupt(reason?: Reason): Uint8Array;
  /**
   * Exact payload used when the transport ends a stream the product's handler
   * never got to serve.
   **/
  declinePayload: Uint8Array;
  /** Number of starts retained before a handler is installed. **/
  bufferCapacity: number;
}

/**
 * Byte-level transport used by generated client stubs.
 **/
export interface TrUApiTransport {
  /**
   * Send a one-shot request and resolve with the typed Ok/Err outcome.
   **/
  request<Ok, Err>(
    params: RequestParams<Ok, Err>,
  ): ResultAsync<Ok, Err | UnsupportedCallError>;

  /**
   * Start a subscription and return a handle that can stop it.
   **/
  subscribeRaw(params: SubscribeRawParams): Subscription;

  /** Register product-side handling for a host-initiated subscription. **/
  registerHostInitiatedSubscription<Request, Item, Reason>(
    params: RegisterHostInitiatedSubscriptionParams<Request, Item, Reason>,
  ): HostInitiatedSubscriptionRegistration<Request, Item, Reason>;

  /**
   * Tear down the transport and release the listeners it registered on the
   * underlying `WireProvider`. Pending requests reject and live subscriptions
   * receive `onClose`. Idempotent.
   *
   * The provider itself is left alone; the caller decides whether to also
   * call `provider.dispose()` (long-lived hosts that swap providers will
   * typically dispose the transport but keep the provider).
   **/
  dispose(): void;
}

/**
 * Tagged payload inside a TrUAPI wire frame.
 **/
export interface Payload {
  /**
   * Wire-table trait discriminant: first byte of the `(trait, method)` pair.
   **/
  traitId: number;

  /**
   * Wire-table method discriminant within the trait: second byte of the pair.
   **/
  methodId: number;

  /**
   * Which leg of the method's exchange this frame carries: `Request`/`Start`
   * = 0, `Response`/`Receive` = 1, `Interrupt` = 2, `Stop` = 3. Third byte of
   * the wire frame — readable generically, without decoding `value`.
   **/
  messageType: number;

  /**
   * SCALE-encoded payload body: that leg's own versioned wrapper, with no
   * further tag identifying direction or version beyond the wrapper's own.
   **/
  value: Uint8Array;
}

/** See {@link Payload.messageType}. */
export const MESSAGE_TYPE_REQUEST = 0;
/** See {@link Payload.messageType}. */
export const MESSAGE_TYPE_START = 0;
/** See {@link Payload.messageType}. */
export const MESSAGE_TYPE_RESPONSE = 1;
/** See {@link Payload.messageType}. */
export const MESSAGE_TYPE_RECEIVE = 1;
/** See {@link Payload.messageType}. */
export const MESSAGE_TYPE_INTERRUPT = 2;
/** See {@link Payload.messageType}. */
export const MESSAGE_TYPE_STOP = 3;
/**
 * A request's withdrawal, correlated by the same `requestId` and carrying no
 * payload. The call still settles with exactly one response; this only fires
 * the handler's cancellation token on the far side.
 **/
export const MESSAGE_TYPE_CANCEL = 4;

/**
 * Top-level TrUAPI wire message.
 **/
export interface ProtocolMessage {
  /**
   * Request id used to correlate request/response and subscription frames.
   **/
  requestId: string;

  /**
   * Tagged SCALE payload carried by this frame.
   **/
  payload: Payload;
}

/**
 * Raw SCALE-wire-frame pipe abstraction used by the transport. A `WireProvider`
 * is the low-level channel (a `MessagePort` or iframe `postMessage` link) that
 * carries encoded frames between the product and the host.
 **/
export interface WireProvider {
  /**
   * Send a complete SCALE-encoded wire frame to the peer.
   **/
  postMessage(message: Uint8Array): void;

  /**
   * Register a callback for inbound SCALE-encoded wire frames.
   **/
  subscribe(callback: (message: Uint8Array) => void): () => void;

  /**
   * Register a callback for provider-level close or failure events.
   *
   * Providers keep a terminal close reason. The callback fires at most once
   * for an active subscription, and fires immediately when registered after
   * the provider has already closed.
   **/
  subscribeClose?(callback: (error: Error) => void): () => void;

  /** End current operations while keeping the provider available for new work. */
  subscribeReset?(callback: (error: Error) => void): () => void;

  /**
   * Release provider resources and close the underlying pipe.
   **/
  dispose(): void;
}

/**
 * A {@link WireProvider} backed by a WebSocket, which reports when its socket
 * is up. Awaiting {@link WebSocketWireProvider.opened} is optional: frames
 * posted earlier are queued and flushed on open.
 **/
export interface WebSocketWireProvider extends WireProvider {
  /** Resolves once the socket is open, rejects if it never connects. */
  opened: Promise<void>;
}

/**
 * Encode a `ProtocolMessage` into a SCALE wire frame.
 **/
export function encodeWireMessage(
  message: ProtocolMessage,
): Result<Uint8Array, Error> {
  const { traitId, methodId, messageType } = message.payload;
  if (!Number.isInteger(traitId) || traitId < 0 || traitId > 255) {
    return err(new Error(`Invalid wire trait discriminant: ${traitId}`));
  }
  if (!Number.isInteger(methodId) || methodId < 0 || methodId > 255) {
    return err(new Error(`Invalid wire method discriminant: ${methodId}`));
  }
  if (!Number.isInteger(messageType) || messageType < 0 || messageType > 255) {
    return err(new Error(`Invalid wire message type: ${messageType}`));
  }
  return ok(
    concatBytes(
      str.enc(message.requestId),
      u8.enc(traitId),
      u8.enc(methodId),
      u8.enc(messageType),
      message.payload.value,
    ),
  );
}

/**
 * Decode a SCALE wire frame into a `ProtocolMessage`.
 **/
export function decodeWireMessage(
  message: Uint8Array,
): Result<ProtocolMessage, Error> {
  if (message.length < 1) {
    return err(new Error("Wire frame too short: empty buffer"));
  }
  let cursor = message;
  const requestIdEndResult = scanStrEnd(cursor);
  if (requestIdEndResult.isErr()) {
    return err(requestIdEndResult.error);
  }
  const requestIdEnd = requestIdEndResult.value;
  const requestId = str.dec(cursor.subarray(0, requestIdEnd));
  cursor = cursor.subarray(requestIdEnd);
  if (cursor.length < 1) {
    return err(
      new Error("Wire frame too short: missing trait discriminant byte"),
    );
  }
  if (cursor.length < 2) {
    return err(
      new Error("Wire frame too short: missing method discriminant byte"),
    );
  }
  if (cursor.length < 3) {
    return err(new Error("Wire frame too short: missing message-type byte"));
  }
  const traitId = cursor[0];
  const methodId = cursor[1];
  const messageType = cursor[2];
  const value = cursor.subarray(3);
  // Hand the value bytes back as a fresh slice so callers may safely retain
  // it even if the source buffer is reused by the transport.
  const valueCopy = new Uint8Array(value.length);
  valueCopy.set(value);
  return ok({
    requestId,
    payload: { traitId, methodId, messageType, value: valueCopy },
  });
}

/**
 * Return the byte offset just past the leading SCALE-encoded string.
 **/
function scanStrEnd(bytes: Uint8Array): Result<number, Error> {
  if (bytes.length < 1) {
    return err(new Error("compact-len: empty buffer"));
  }
  const first = bytes[0];
  const mode = first & 0b11;
  let lengthLen: number;
  let strLen: number;
  if (mode === 0) {
    lengthLen = 1;
    strLen = first >> 2;
  } else if (mode === 1) {
    if (bytes.length < 2) {
      return err(new Error("compact-len: truncated mode-1 prefix"));
    }
    lengthLen = 2;
    strLen = ((first >> 2) | (bytes[1] << 6)) & 0x3fff;
  } else if (mode === 2) {
    if (bytes.length < 4) {
      return err(new Error("compact-len: truncated mode-2 prefix"));
    }
    lengthLen = 4;
    strLen =
      ((first >> 2) | (bytes[1] << 6) | (bytes[2] << 14) | (bytes[3] << 22)) >>>
      0;
  } else {
    // big-int mode: not used for requestId in our protocol
    return err(
      new Error("compact big-int mode not supported in wire envelope"),
    );
  }
  const total = lengthLen + strLen;
  if (total > bytes.length) {
    return err(new Error("compact-len: declared length exceeds buffer"));
  }
  return ok(total);
}

/**
 * Internal listener bookkeeping and close-once state machine shared by the
 * built-in `WireProvider` implementations. Transport-specific code wires its
 * inbound source to `deliver`, registers cleanup via `onClose`, and exposes
 * `subscribe`/`subscribeClose` to callers.
 **/
function createBaseProvider() {
  const listeners = new Set<(message: Uint8Array) => void>();
  const closeListeners = new Set<(error: Error) => void>();
  const onCloseCleanup = new Set<() => void>();
  let closedError: Error | null = null;

  return {
    /** Current close error, or `null` while the provider is open. */
    closed: (): Error | null => closedError,

    /** Dispatch an inbound frame to every active subscriber. */
    deliver(message: Uint8Array) {
      if (closedError) return;
      for (const listener of [...listeners]) listener(message);
    },

    /** Transition to the closed state. Idempotent. */
    close(error: unknown) {
      if (closedError) return;
      closedError = toError(error);
      for (const fn of [...onCloseCleanup]) {
        try {
          fn();
        } catch {
          // ignore cleanup failure
        }
      }
      onCloseCleanup.clear();
      for (const listener of [...closeListeners]) listener(closedError);
      listeners.clear();
      closeListeners.clear();
    },

    /** Register a cleanup function to run exactly once when `close` fires. */
    onClose(fn: () => void) {
      if (closedError) {
        try {
          fn();
        } catch {
          // ignore cleanup failure
        }
        return;
      }
      onCloseCleanup.add(fn);
    },

    /** Register an inbound message listener. No-op after close. */
    subscribe(callback: (message: Uint8Array) => void): () => void {
      if (closedError) return () => {};
      listeners.add(callback);
      return () => {
        listeners.delete(callback);
      };
    },

    /**
     * Register a close listener. If the provider is already closed, the
     * callback fires immediately with the stored error.
     **/
    subscribeClose(callback: (error: Error) => void): () => void {
      if (closedError) {
        callback(closedError);
        return () => {};
      }
      closeListeners.add(callback);
      return () => {
        closeListeners.delete(callback);
      };
    },
  };
}

/**
 * Create a provider for the child side of an iframe `postMessage` channel.
 *
 * `target` is the `Window` the provider posts to (typically `window.parent`);
 * `hostOrigin` is the pinned `targetOrigin` for outbound frames and the
 * required `event.origin` of inbound frames. The provider only delivers
 * frames whose `event.source === target` and `event.origin === hostOrigin`,
 * so it cannot be coerced by an unrelated frame parent.
 **/
export function createIframeProvider(options: {
  target: Window;
  hostOrigin: string;
}): WireProvider {
  const base = createBaseProvider();
  const { target, hostOrigin } = options;

  const onMessage = (event: MessageEvent) => {
    if (event.source !== target) return;
    if (event.origin !== hostOrigin) return;
    if (!(event.data instanceof Uint8Array)) return;
    base.deliver(event.data);
  };
  window.addEventListener("message", onMessage);
  base.onClose(() => window.removeEventListener("message", onMessage));

  return {
    postMessage(message) {
      const error = base.closed();
      if (error) throw error;
      try {
        target.postMessage(message, hostOrigin);
      } catch (error) {
        base.close(error);
        throw toError(error);
      }
    },
    subscribe: base.subscribe,
    subscribeClose: base.subscribeClose,
    dispose() {
      base.close(new Error("iframe provider disposed"));
    },
  };
}

/**
 * Create a provider from a web or Electron `MessagePort`.
 **/
export function createMessagePortProvider(
  port: MessagePort | Promise<MessagePort>,
): WireProvider {
  const base = createBaseProvider();
  let resolvedPort: MessagePort | null = null;
  const pending: Uint8Array[] = [];

  void Promise.resolve(port)
    .then((p) => {
      if (base.closed()) {
        try {
          p.close();
        } catch {
          // ignore duplicate close during shutdown
        }
        return;
      }

      resolvedPort = p;
      p.onmessage = (event: MessageEvent) => {
        if (event.data instanceof Uint8Array) base.deliver(event.data);
      };
      if ("onmessageerror" in p) {
        p.onmessageerror = () => {
          base.close(new Error("message port closed unexpectedly"));
        };
      }
      p.start();
      for (const msg of pending) p.postMessage(msg);
      pending.length = 0;
      base.onClose(() => {
        try {
          p.close();
        } catch {
          // ignore duplicate close during shutdown
        }
      });
    })
    .catch((error: unknown) => {
      base.close(error);
    });

  return {
    postMessage(message) {
      const error = base.closed();
      if (error) throw error;
      if (resolvedPort) {
        try {
          resolvedPort.postMessage(message);
        } catch (error) {
          base.close(error);
          throw toError(error);
        }
      } else {
        pending.push(message);
      }
    },
    subscribe: base.subscribe,
    subscribeClose: base.subscribeClose,
    dispose() {
      base.close(new Error("message port provider disposed"));
      pending.length = 0;
    },
  };
}

/**
 * Wire provider over a binary WebSocket, one message per SCALE frame.
 *
 * This is the transport a host exposes on a loopback socket: the Rust core's
 * `ws-bridge`, and `truapi-host signing-host --frame-listen`. The frame bytes
 * are identical to what the {@link createMessagePortProvider} path carries, so
 * this is a pipe and nothing more.
 *
 * Frames posted before the socket opens are queued and flushed on open, so a
 * caller never has to await {@link WebSocketWireProvider.opened} first.
 **/
export function createWebSocketProvider(url: string): WebSocketWireProvider {
  return createWebSocketProviderFactory()(url);
}

/** Capture native socket APIs before product code installs network gates. */
export function createWebSocketProviderFactory(): (
  url: string,
) => WebSocketWireProvider {
  const NativeWebSocket = WebSocket;
  const socketSend = NativeWebSocket.prototype.send;
  const socketClose = NativeWebSocket.prototype.close;
  const addEventListener = EventTarget.prototype.addEventListener;
  const messageData = Object.getOwnPropertyDescriptor(
    MessageEvent.prototype,
    "data",
  )!.get!;
  const setBinaryType = Object.getOwnPropertyDescriptor(
    NativeWebSocket.prototype,
    "binaryType",
  )!.set!;
  const apply = Reflect.apply;

  const bufferLength = Object.getOwnPropertyDescriptor(
    ArrayBuffer.prototype,
    "byteLength",
  )!.get!;

  return (url) => {
    const base = createBaseProvider();
    const socket = new NativeWebSocket(url);
    apply(setBinaryType, socket, ["arraybuffer"]);
    const pending: Uint8Array[] = [];
    let open = false;

    // `send` types its view as ArrayBuffer-backed. Frames never come from a
    // SharedArrayBuffer, and only the view's own bytes go on the wire, so a
    // frame that is a window into a larger buffer stays correct.
    const send = (frame: Uint8Array) =>
      apply(socketSend, socket, [frame as Uint8Array<ArrayBuffer>]);

    let resolveOpened!: () => void;
    let rejectOpened!: (error: Error) => void;
    const opened = new Promise<void>((resolve, reject) => {
      resolveOpened = resolve;
      rejectOpened = reject;
    });
    // `opened` is optional for callers, so a failed connection must not surface as
    // an unhandled rejection. Close still reaches every `subscribeClose`.
    opened.catch(() => {});

    apply(addEventListener, socket, [
      "open",
      () => {
        open = true;
        for (const frame of pending.splice(0)) send(frame);
        resolveOpened();
      },
    ]);
    apply(addEventListener, socket, [
      "message",
      (event: MessageEvent) => {
        let frame: Uint8Array;
        try {
          const buffer = apply(messageData, event, []);
          frame = new Uint8Array(buffer, 0, apply(bufferLength, buffer, []));
        } catch (error) {
          base.close(error);
          return;
        }
        base.deliver(frame);
      },
    ]);
    apply(addEventListener, socket, [
      "error",
      () => {
        const error = new Error(`websocket error (${url})`);
        rejectOpened(error);
        base.close(error);
      },
    ]);
    apply(addEventListener, socket, [
      "close",
      () => {
        const error = new Error(`websocket closed (${url})`);
        rejectOpened(error);
        base.close(error);
      },
    ]);
    base.onClose(() => {
      try {
        apply(socketClose, socket, []);
      } catch {
        // ignore duplicate close during shutdown
      }
    });

    return {
      opened,
      postMessage(message) {
        const error = base.closed();
        if (error) throw error;
        if (open) {
          try {
            send(message);
          } catch (error) {
            base.close(error);
            throw toError(error);
          }
        } else {
          pending.push(message);
        }
      },
      subscribe: base.subscribe,
      subscribeClose: base.subscribeClose,
      dispose() {
        base.close(new Error("websocket provider disposed"));
        pending.length = 0;
      },
    };
  };
}
