import { errAsync, okAsync, ResultAsync } from "neverthrow";

import {
  decodeWireMessage,
  encodeWireMessage,
  MESSAGE_TYPE_CANCEL,
  MESSAGE_TYPE_INTERRUPT,
  MESSAGE_TYPE_RECEIVE,
  MESSAGE_TYPE_REQUEST,
  MESSAGE_TYPE_RESPONSE,
  MESSAGE_TYPE_START,
  MESSAGE_TYPE_STOP,
  PROTOCOL_ERROR_METHOD_ID,
  PROTOCOL_ERROR_TRAIT_ID,
  type HostInitiatedSubscriptionHandler,
  type MethodIds,
  type ProtocolMessage,
  type RegisterHostInitiatedSubscriptionParams,
  type RequestParams,
  type SubscribeRawParams,
  type Subscription,
  type TrUApiTransport,
  type UnsupportedCallError,
  UnsupportedMessageError,
  type WireProvider,
} from "./transport.js";
import * as S from "./scale.js";
import { type ResultPayload } from "./scale.js";
import { TRUAPI_CODEC_VERSION } from "./generated/client.js";
import * as T from "./generated/types.js";
import * as W from "./generated/wire-table.js";

export type { Subscription, TrUApiTransport };

// Every method's request/response (or start/stop/interrupt/receive) frames
// share one (trait, method) address: which leg of the exchange a frame
// carries is the wire's own `messageType` byte, not part of the address. A
// late or duplicate *answer*-leg frame (Response/Stop/Interrupt/Receive) for
// a known method can legitimately arrive with no matching pending call or
// subscription (e.g. after a request already timed out, or after
// `unsubscribe`), so those are ignored rather than reported as a protocol
// violation. A *request*-leg frame (Request/Start) with nothing to route to
// is never expected — it means this build genuinely doesn't implement the
// pair (no client was ever created, or the specific host-initiated method
// has no registration) — so it still earns the same reply as an unknown
// pair.
const KNOWN_WIRE_IDS = new Set<string>(
  Object.values(W).map((ids) => `${ids.trait}:${ids.method}`),
);

const DEFAULT_REQUEST_TIMEOUT_MS = 120_000;

/** A request received no matching response before its transport deadline. */
export class RequestTimeoutError extends Error {
  /** Transport-assigned request identifier. */
  readonly requestId: string;
  /** Trait discriminant of the unanswered request. */
  readonly traitId: number;
  /** Method discriminant of the unanswered request. */
  readonly methodId: number;
  /** Deadline that elapsed, in milliseconds. */
  readonly timeoutMs: number;

  constructor(
    requestId: string,
    traitId: number,
    methodId: number,
    timeoutMs: number,
  ) {
    super(
      `TrUAPI request ${requestId} (wire ${traitId}, ${methodId}) timed out after ${timeoutMs}ms`,
    );
    this.name = "RequestTimeoutError";
    this.requestId = requestId;
    this.traitId = traitId;
    this.methodId = methodId;
    this.timeoutMs = timeoutMs;
  }
}

/**
 * Options accepted when constructing a transport.
 */
export interface CreateTransportOptions {
  /** Request ID namespace when multiple transports share a connection. Defaults to `p:`. */
  requestIdPrefix?: string;
  /** Wait for connection readiness before sending a request or starting a subscription. */
  prepare?: (ids: MethodIds) => Promise<void>;
  /** Replace a failed connection after a malformed frame; otherwise the transport closes permanently. */
  onProtocolError?: (error: Error) => void;
  /**
   * Maximum time to wait for a matching response before rejecting the request.
   *
   * Defaults to 120 seconds. This bounds dead hosts and missed transport
   * handshakes while leaving interactive approval flows enough time to finish.
   * The handshake keeps its own shorter deadline, since a codec mismatch means
   * no answer is ever coming.
   */
  requestTimeoutMs?: number;
}

/** A subscription cancellation: `Stop` carries no payload at all. **/
const STOP_FRAME = new Uint8Array();

/** A `Cancel` leg carries no payload, exactly as `Stop` does. */
const CANCEL_FRAME = new Uint8Array();

/**
 * Report a frame the transport received but cannot act on.
 *
 * Every such frame is a disagreement with the peer about the wire, and the
 * transport has no channel to answer on: the caller is left waiting and
 * "the host dropped it" is indistinguishable from "the host never sent it".
 * Warn so the mismatch is diagnosable from the console instead of presenting
 * as an unexplained hang.
 */
function reportProtocolViolation(detail: string): void {
  console.warn(`[truapi] ${detail}`);
}

/**
 * How long a `system_handshake` call waits for the host's answer. Matches the
 * allowance the protocol spec gives the handshake.
 */
const HANDSHAKE_TIMEOUT_MS = 10_000;

/**
 * Codec for `system_handshake`'s `Response`-leg payload:
 * `Result<HostHandshakeResponse, CallError<HostHandshakeError>>`.
 */
const HANDSHAKE_RESPONSE_CODEC = S.Result(
  T.VersionedHostHandshakeResponse,
  S.CallError(T.VersionedHostHandshakeError),
);

/**
 * Encode a successful host-handshake response frame:
 * `Ok(HostHandshakeResponse::V1)`.
 */
function encodeSuccessfulHandshakeResponse(): Uint8Array {
  return HANDSHAKE_RESPONSE_CODEC.enc({
    success: true,
    value: { tag: "V1" },
  });
}

/**
 * Encode a host-handshake response frame reporting an unsupported codec
 * version: `Err(CallError::Domain(HostHandshakeError::V1(
 * UnsupportedProtocolVersion)))`.
 */
function encodeUnsupportedHandshakeResponse(): Uint8Array {
  return HANDSHAKE_RESPONSE_CODEC.enc({
    success: false,
    value: {
      tag: "Domain",
      value: {
        tag: "V1",
        value: { tag: "UnsupportedProtocolVersion", value: undefined },
      },
    },
  });
}

/**
 * Map key for a `(trait, method)` wire discriminant pair. Both bytes together
 * identify a frame, so neither half alone is a usable key.
 */
function pairKey(traitId: number, methodId: number): string {
  return `${traitId}:${methodId}`;
}

/**
 * Decode `V1(UnsupportedMessage { trait_id, method_id })` from the reserved
 * `(255, 255)` address.
 *
 * `undefined` is a protocol error this build does not know: a version or a
 * variant index it has never heard of, from a peer built against a later
 * protocol. Returning it rather than throwing is deliberate. This is the one
 * address every peer answers on, and the caller answers a throw by closing the
 * transport, so a build that rejected an unfamiliar payload here could never be
 * told anything new without the whole connection dying.
 *
 * A payload this build does recognise stays strict: codec 2 addresses a frame
 * by a pair, so `V1(UnsupportedMessage)` is exactly four bytes (version index,
 * variant index, then the trait and method the peer could not handle), and a
 * truncated or over-long one is corruption rather than a newer peer.
 */
function decodeUnsupportedMessage(
  payload: Uint8Array,
): { traitId: number; methodId: number } | undefined {
  if (payload.length === 0) {
    throw new Error("Malformed protocol error payload: empty");
  }
  if (payload[0] !== 0 || (payload.length > 1 && payload[1] !== 0)) {
    // A version or variant from a later protocol. Its length is unknowable
    // here, so the remaining bytes are not validated.
    return undefined;
  }
  if (payload.length !== 4) {
    throw new Error(
      `Malformed protocol error payload: expected 4 bytes, received ${payload.length}`,
    );
  }
  return { traitId: payload[2], methodId: payload[3] };
}

/**
 * Build a `TrUApiTransport` on top of a `WireProvider`, adding request/response
 * correlation and subscription start/receive/stop lifecycle handling.
 */
export function createTransport(
  provider: WireProvider,
  options: CreateTransportOptions = {},
): TrUApiTransport {
  const requestTimeoutMs =
    options.requestTimeoutMs ?? DEFAULT_REQUEST_TIMEOUT_MS;
  if (!Number.isFinite(requestTimeoutMs) || requestTimeoutMs <= 0) {
    throw new RangeError("requestTimeoutMs must be a positive finite number");
  }
  let idCounter = 0;
  const requestIdPrefix = options.requestIdPrefix ?? "p:";
  let closedError: Error | null = null;
  type PendingRequest = {
    ids: MethodIds;
    sent: boolean;
    resolve: (value: Uint8Array) => void;
    resolveUnsupported: () => void;
    reject: (error: Error) => void;
    cancelTimeout: () => void;
    detachAbort: () => void;
  };
  const pending = new Map<string, PendingRequest>();
  const subscriptions = new Map<
    string,
    {
      ids: MethodIds;
      sent: boolean;
      onReceive: (payload: Uint8Array) => void;
      onInterrupt?: (payload: Uint8Array) => void;
      onClose?: (error: Error) => void;
    }
  >();
  type BufferedHostStart = { requestId: string; payload: Uint8Array };
  type HostRoute = {
    ids: MethodIds;
    decodeRequest: (payload: Uint8Array) => unknown;
    encodeItem: (item: unknown) => Uint8Array;
    encodeInterrupt: (reason?: unknown) => Uint8Array;
    declinePayload: Uint8Array;
    bufferCapacity: number;
    buffered: BufferedHostStart[];
    handler?: HostInitiatedSubscriptionHandler<unknown, unknown, unknown>;
    instances: Map<string, { unsubscribe(): void }>;
  };
  // Keyed by the full (trait, method) pair: a bare method id would collide
  // the moment two traits both number a subscription the same.
  const hostRoutes = new Map<string, HostRoute>();

  /**
   * Normalize arbitrary thrown values into `Error` instances.
   */
  function toError(error: unknown): Error {
    return error instanceof Error ? error : new Error(String(error));
  }

  /** Remove a pending request and cancel its deadline. */
  function takePending(requestId: string): PendingRequest | undefined {
    const entry = pending.get(requestId);
    if (!entry) return undefined;
    pending.delete(requestId);
    entry.cancelTimeout();
    entry.detachAbort();
    return entry;
  }

  /**
   * Withdraw an in-flight call. The frame rides the method's own address and
   * carries no payload; the host answers the request, never the cancel, so
   * nothing here settles the pending entry.
   *
   * Failing to send is not worth surfacing: the transport is already closing,
   * and `closeWithError` settles every pending call behind it.
   */
  function sendCancel(requestId: string, ids: MethodIds) {
    if (closedError) return;
    try {
      send({
        requestId,
        payload: {
          traitId: ids.trait,
          methodId: ids.method,
          messageType: MESSAGE_TYPE_CANCEL,
          value: CANCEL_FRAME,
        },
      });
    } catch {
      // provider already closed
    }
  }

  function interruptOperations(error: Error) {
    const requests = [...pending.keys()].map(
      (requestId) => takePending(requestId)!,
    );
    const streams = [...subscriptions.values()];
    subscriptions.clear();
    const instances: { unsubscribe(): void }[] = [];
    for (const route of hostRoutes.values()) {
      route.buffered.length = 0;
      instances.push(...route.instances.values());
      route.instances.clear();
    }

    for (const request of requests) request.reject(error);
    // Product callbacks must not stop the provider's remaining close listeners.
    for (const instance of instances) {
      try {
        instance.unsubscribe();
      } catch {}
    }
    for (const subscription of streams) {
      try {
        subscription.onClose?.(error);
      } catch {}
    }
  }

  /** Close permanently; a provider reset only interrupts current operations. */
  function closeWithError(error: unknown) {
    if (closedError) return;
    closedError = toError(error);
    interruptOperations(closedError);
  }

  const onProtocolError = options.onProtocolError ?? closeWithError;

  const unsubscribeClose = provider.subscribeClose?.((error) => {
    closeWithError(error);
  });
  const unsubscribeReset = provider.subscribeReset?.((error) => {
    if (!closedError) interruptOperations(error);
  });

  const unsubscribeMessage = provider.subscribe((message) => {
    if (closedError) {
      return;
    }

    const decoded = decodeWireMessage(message);
    if (decoded.isErr()) {
      onProtocolError(decoded.error);
      return;
    }
    const { requestId, payload } = decoded.value;

    if (
      payload.traitId === PROTOCOL_ERROR_TRAIT_ID &&
      payload.methodId === PROTOCOL_ERROR_METHOD_ID
    ) {
      let unsupported: { traitId: number; methodId: number } | undefined;
      try {
        unsupported = decodeUnsupportedMessage(payload.value);
      } catch (error) {
        onProtocolError(toError(error));
        return;
      }

      if (unsupported === undefined) {
        // A protocol error from a later peer. `requestId` still correlates it,
        // so settle that one call and leave the transport up: the pair cannot
        // be checked because this build cannot read the payload that carries
        // it.
        reportProtocolViolation(
          `unrecognised protocol error for request ${requestId}: discriminant ${payload.value[0]}`,
        );
        if (pending.has(requestId)) {
          takePending(requestId)?.resolveUnsupported();
          return;
        }
        const stream = subscriptions.get(requestId);
        if (stream) {
          subscriptions.delete(requestId);
          stream.onClose?.(
            new UnsupportedMessageError(stream.ids.trait, stream.ids.method),
          );
        }
        return;
      }

      // Match on the whole pair: a bare method id would alias across traits and
      // could resolve the wrong pending call.
      const request = pending.get(requestId);
      if (
        request?.ids.trait === unsupported.traitId &&
        request?.ids.method === unsupported.methodId
      ) {
        takePending(requestId)?.resolveUnsupported();
        return;
      }

      const subscription = subscriptions.get(requestId);
      if (
        subscription?.ids.trait === unsupported.traitId &&
        subscription?.ids.method === unsupported.methodId
      ) {
        subscriptions.delete(requestId);
        subscription.onClose?.(
          new UnsupportedMessageError(
            unsupported.traitId,
            unsupported.methodId,
          ),
        );
      }
      return;
    }

    if (
      payload.traitId === W.SYSTEM_HANDSHAKE.trait &&
      payload.methodId === W.SYSTEM_HANDSHAKE.method &&
      payload.messageType === MESSAGE_TYPE_REQUEST
    ) {
      // Auto-respond to inbound `host_handshake_request` frames. Hosts ping
      // the product at startup and repeat until they see a matching response,
      // so this handler must always answer and must never tear the transport
      // down: a host whose codec this client cannot speak is exactly the peer
      // that needs an answer it can act on.
      //
      // The messageType check above matters: request and response now share
      // this address, and a `Response` frame arriving here is the host's
      // answer to this client's own `system.handshake()` call, which must
      // fall through to the `pending` lookup below instead.
      //
      // Respond with the handshake method's selected wire version. The inner
      // request carries the wire codec version. A request body this client
      // cannot decode is itself a codec mismatch, so it earns the same
      // unsupported-version answer rather than a raw SCALE error.
      let response: Uint8Array;
      try {
        const request = T.VersionedHostHandshakeRequest.dec(payload.value);
        const requestedCodecVersion = request.value.codecVersion;
        response =
          requestedCodecVersion === TRUAPI_CODEC_VERSION
            ? encodeSuccessfulHandshakeResponse()
            : encodeUnsupportedHandshakeResponse();
      } catch (error) {
        reportProtocolViolation(
          `undecodable handshake request from the host (expected wire codec ${TRUAPI_CODEC_VERSION}): ${
            toError(error).message
          }`,
        );
        response = encodeUnsupportedHandshakeResponse();
      }
      try {
        send({
          requestId,
          payload: {
            traitId: W.SYSTEM_HANDSHAKE.trait,
            methodId: W.SYSTEM_HANDSHAKE.method,
            messageType: MESSAGE_TYPE_RESPONSE,
            value: response,
          },
        });
      } catch {
        // provider already closed
      }
      return;
    }

    const hostRoute = hostRoutes.get(
      pairKey(payload.traitId, payload.methodId),
    );
    if (hostRoute) {
      if (payload.messageType === MESSAGE_TYPE_START) {
        startHostSubscription(hostRoute, requestId, payload.value);
      } else if (payload.messageType === MESSAGE_TYPE_STOP) {
        const bufferedIndex = hostRoute.buffered.findIndex(
          (start) => start.requestId === requestId,
        );
        if (bufferedIndex >= 0) hostRoute.buffered.splice(bufferedIndex, 1);
        const instance = hostRoute.instances.get(requestId);
        if (instance) {
          hostRoute.instances.delete(requestId);
          instance.unsubscribe();
        }
      } else {
        reportProtocolViolation(
          `ignoring host-initiated frame for (${payload.traitId}, ${payload.methodId}): unexpected messageType ${payload.messageType}, expected Start (${MESSAGE_TYPE_START}) or Stop (${MESSAGE_TYPE_STOP})`,
        );
      }
      return;
    }

    const p = pending.get(requestId);
    if (p) {
      if (payload.traitId !== p.ids.trait || payload.methodId !== p.ids.method) {
        // The host answered this request id on a discriminant the method does
        // not own. Dropping it unreported leaves the caller waiting forever
        // with no clue why, and a whole-trait skew is what a codec mismatch
        // looks like from here.
        //
        // Report it, then fall through rather than returning: the request stays
        // pending (this frame is not its answer), and the frame itself is one
        // this build cannot route, so it earns the same protocol-error reply as
        // any other unroutable pair. A known method's answer-leg frame is
        // still filtered out below.
        reportProtocolViolation(
          `ignoring frame for request ${requestId}: got discriminant (${payload.traitId}, ${payload.methodId}), expected (${p.ids.trait}, ${p.ids.method})`,
        );
      } else if (payload.messageType !== MESSAGE_TYPE_RESPONSE) {
        // Right id, right address, wrong leg. Every leg of a method shares one
        // address, so the address alone cannot establish that this frame is
        // the answer: without this check a `Request`, `Interrupt`, `Stop` or
        // an out-of-range byte consumes the pending call and either resolves
        // it from the wrong bytes or fails its decoder, before the real
        // response ever arrives. The call stays pending, since this frame is
        // not its answer.
        reportProtocolViolation(
          `ignoring frame for request ${requestId}: unexpected messageType ${payload.messageType}, expected Response (${MESSAGE_TYPE_RESPONSE}) on (${p.ids.trait}, ${p.ids.method})`,
        );
        return;
      } else {
        takePending(requestId);
        try {
          p.resolve(payload.value);
        } catch (error) {
          p.reject(toError(error));
        }
        return;
      }
    }

    const subscription = subscriptions.get(requestId);
    if (subscription) {
      if (
        payload.traitId === subscription.ids.trait &&
        payload.methodId === subscription.ids.method &&
        payload.messageType === MESSAGE_TYPE_RECEIVE
      ) {
        try {
          subscription.onReceive(payload.value);
        } catch (error) {
          // A consumer-side decode/handler error must not tear down the
          // provider's message loop and silently break every other
          // subscription on the same transport. Surface via onClose and
          // drop this subscription; siblings stay alive.
          subscriptions.delete(requestId);
          subscription.onClose?.(toError(error));
        }
      } else if (
        payload.traitId === subscription.ids.trait &&
        payload.methodId === subscription.ids.method &&
        payload.messageType === MESSAGE_TYPE_INTERRUPT
      ) {
        subscriptions.delete(requestId);
        subscription.onInterrupt?.(payload.value);
      } else {
        reportProtocolViolation(
          `ignoring frame for subscription ${requestId}: got discriminant (${payload.traitId}, ${payload.methodId}) messageType ${payload.messageType}, expected receive (${MESSAGE_TYPE_RECEIVE}) or interrupt (${MESSAGE_TYPE_INTERRUPT}) on (${subscription.ids.trait}, ${subscription.ids.method})`,
        );
      }
      return;
    }

    if (KNOWN_WIRE_IDS.has(`${payload.traitId}:${payload.methodId}`)) {
      if (
        payload.messageType === MESSAGE_TYPE_STOP ||
        payload.messageType === MESSAGE_TYPE_INTERRUPT ||
        payload.messageType === MESSAGE_TYPE_RECEIVE
      ) {
        // A known method's answer-leg frame (Response/Stop/Interrupt/
        // Receive) with nothing to route to: a normal late/stale frame, not
        // a protocol violation.
        return;
      }
      if (payload.messageType !== MESSAGE_TYPE_REQUEST) {
        // Neither a plausible late answer nor a valid Request/Start: the
        // messageType byte itself is out of range. Still dropped (there is
        // nothing to route it to), but logged rather than silently
        // swallowed, matching the analogous case in the `hostRoute` branch
        // above.
        reportProtocolViolation(
          `ignoring frame for known pair (${payload.traitId}, ${payload.methodId}): unexpected messageType ${payload.messageType}`,
        );
        return;
      }
    }

    // Either an unknown pair, or a known method's request-leg frame
    // (Request/Start) with nothing to route to: this build does not
    // implement the pair.
    reportProtocolViolation(
      `unsupported frame with discriminant (${payload.traitId}, ${payload.methodId}): request ${requestId} is not pending and has no subscription`,
    );
    try {
      send({
        requestId,
        payload: {
          traitId: PROTOCOL_ERROR_TRAIT_ID,
          methodId: PROTOCOL_ERROR_METHOD_ID,
          messageType: MESSAGE_TYPE_RESPONSE,
          value: new Uint8Array([0, 0, payload.traitId, payload.methodId]),
        },
      });
    } catch {
      // provider already closed
    }
  });

  /**
   * Encode and post a protocol message through the underlying provider.
   */
  function send(message: ProtocolMessage) {
    if (closedError) {
      throw closedError;
    }

    const encoded = encodeWireMessage(message);
    if (encoded.isErr()) {
      closeWithError(encoded.error);
      throw encoded.error;
    }

    try {
      provider.postMessage(encoded.value);
    } catch (error) {
      closeWithError(error);
      throw toError(error);
    }
  }

  function prepare(
    ids: MethodIds,
    ready: () => void,
    failed: (error: unknown) => void,
  ): void {
    const onReady = () => {
      try {
        ready();
      } catch (error) {
        failed(error);
      }
    };
    try {
      if (options.prepare) void options.prepare(ids).then(onReady, failed);
      else onReady();
    } catch (error) {
      failed(error);
    }
  }

  /**
   * End one host-initiated stream, sending `payload` on its interrupt leg and
   * running the handler's teardown. The instance is dropped before the
   * teardown runs, so a handler that interrupts from inside its own teardown
   * cannot recurse.
   */
  function interruptHostSubscription(
    route: HostRoute,
    requestId: string,
    payload: Uint8Array,
  ) {
    const instance = route.instances.get(requestId);
    if (instance) {
      route.instances.delete(requestId);
      instance.unsubscribe();
    }
    try {
      send({
        requestId,
        payload: {
          traitId: route.ids.trait,
          methodId: route.ids.method,
          messageType: MESSAGE_TYPE_INTERRUPT,
          value: payload,
        },
      });
    } catch {
      // provider already closed
    }
  }

  function startHostSubscription(
    route: HostRoute,
    requestId: string,
    payload: Uint8Array,
  ) {
    const previous = route.instances.get(requestId);
    if (previous) {
      route.instances.delete(requestId);
      previous.unsubscribe();
    }

    const handler = route.handler;
    if (!handler) {
      if (route.buffered.length === route.bufferCapacity) {
        const evicted = route.buffered.shift();
        if (evicted)
          interruptHostSubscription(
            route,
            evicted.requestId,
            route.declinePayload,
          );
      }
      route.buffered.push({ requestId, payload });
      return;
    }

    let request: unknown;
    try {
      request = route.decodeRequest(payload);
    } catch {
      interruptHostSubscription(route, requestId, route.declinePayload);
      return;
    }

    let active = true;
    let teardown: (() => void) | void;
    const instance = {
      unsubscribe() {
        if (!active) return;
        active = false;
        teardown?.();
      },
    };
    route.instances.set(requestId, instance);

    const sendItem = (item: unknown) => {
      if (!active) return;
      try {
        send({
          requestId,
          payload: {
            traitId: route.ids.trait,
            methodId: route.ids.method,
            messageType: MESSAGE_TYPE_RECEIVE,
            value: route.encodeItem(item),
          },
        });
      } catch {
        interruptHostSubscription(route, requestId, route.declinePayload);
      }
    };

    const interrupt = (reason?: unknown) => {
      if (!active) return;
      let encoded: Uint8Array;
      try {
        encoded = route.encodeInterrupt(reason);
      } catch {
        encoded = route.declinePayload;
      }
      interruptHostSubscription(route, requestId, encoded);
    };

    try {
      teardown = handler(request, sendItem, interrupt);
      if (!active) teardown?.();
    } catch {
      interruptHostSubscription(route, requestId, route.declinePayload);
    }
  }

  return {
    /**
     * Send one request frame and resolve with the typed Ok/Err outcome
     * decoded from the response payload's `ResultPayload` envelope.
     */
    request<Ok, Err>({
      ids,
      payload,
      decodeResponse,
      signal,
    }: RequestParams<Ok, Err>): ResultAsync<Ok, Err | UnsupportedCallError> {
      const promise = new Promise<
        ResultPayload<Ok, Err | UnsupportedCallError>
      >((resolve, reject) => {
        if (closedError) {
          reject(closedError);
          return;
        }
        // Nothing to withdraw yet: sending a request only to cancel it in the
        // same turn asks the host to start work no one wants.
        if (signal?.aborted) {
          reject(toError(signal.reason));
          return;
        }

        const requestId = `${requestIdPrefix}${++idCounter}`;
        // Every call is bounded, so a dead host can never strand one. The
        // handshake takes a shorter window than the rest: it needs no
        // host-side confirmation and settles the codec question before any
        // real traffic, so the call that exists to detect a mismatch must not
        // wait out the general deadline for an answer that is never coming.
        const isHandshake =
          ids.trait === W.SYSTEM_HANDSHAKE.trait &&
          ids.method === W.SYSTEM_HANDSHAKE.method;
        const timeoutMs = isHandshake ? HANDSHAKE_TIMEOUT_MS : requestTimeoutMs;
        const deadline = setTimeout(() => {
          const entry = takePending(requestId);
          if (!entry) return;
          // The host is told even though this side has stopped waiting: a
          // deadline that only rejects locally is exactly the leak the
          // `Cancel` leg exists to close.
          if (entry.sent) sendCancel(requestId, ids);
          reject(
            isHandshake
              ? new Error(
                  `TrUAPI handshake timed out after ${HANDSHAKE_TIMEOUT_MS}ms; the host did not answer on wire codec ${TRUAPI_CODEC_VERSION}`,
                )
              : new RequestTimeoutError(
                  requestId,
                  ids.trait,
                  ids.method,
                  timeoutMs,
                ),
          );
        }, timeoutMs);

        // Sent calls settle on the host's response so callers can read Cancelled.
        const onAbort = () => {
          const entry = pending.get(requestId);
          if (!entry) return;
          if (entry.sent) sendCancel(requestId, ids);
          else takePending(requestId)?.reject(toError(signal?.reason));
        };
        signal?.addEventListener("abort", onAbort, { once: true });

        pending.set(requestId, {
          ids,
          sent: false,
          resolve: (response) => resolve(decodeResponse(response)),
          resolveUnsupported: () =>
            resolve({
              success: false,
              value: { tag: "Unsupported" },
            }),
          reject,
          // `takePending` cancels this for every settlement path, so no
          // deadline outlives the call it bounds.
          cancelTimeout: () => clearTimeout(deadline),
          detachAbort: () => signal?.removeEventListener("abort", onAbort),
        });
        prepare(
          ids,
          () => {
            const entry = pending.get(requestId);
            if (!entry) return;
            entry.sent = true;
            send({
              requestId,
              payload: {
                traitId: ids.trait,
                methodId: ids.method,
                messageType: MESSAGE_TYPE_REQUEST,
                value: payload,
              },
            });
          },
          (error) => takePending(requestId)?.reject(toError(error)),
        );
      });
      return ResultAsync.fromSafePromise(promise).andThen(
        (result): ResultAsync<Ok, Err | UnsupportedCallError> =>
          result.success ? okAsync(result.value) : errAsync(result.value),
      );
    },
    /**
     * Start a raw subscription and route incoming receive/interrupt frames to
     * the supplied callbacks.
     */
    subscribeRaw({
      ids,
      payload,
      onReceive,
      onInterrupt,
      onClose,
    }: SubscribeRawParams) {
      if (closedError) {
        onClose?.(closedError);
        return { unsubscribe: () => {}, subscriptionId: "" };
      }

      const requestId = `${requestIdPrefix}${++idCounter}`;
      subscriptions.set(requestId, {
        ids,
        sent: false,
        onReceive,
        onInterrupt,
        onClose,
      });
      prepare(
        ids,
        () => {
          const entry = subscriptions.get(requestId);
          if (!entry) return;
          entry.sent = true;
          send({
            requestId,
            payload: {
              traitId: ids.trait,
              methodId: ids.method,
              messageType: MESSAGE_TYPE_START,
              value: payload,
            },
          });
        },
        (error) => {
          if (subscriptions.delete(requestId)) onClose?.(toError(error));
        },
      );
      return {
        subscriptionId: requestId,
        unsubscribe: () => {
          // Skip the `_stop` frame when the host already terminated the stream
          // via `_interrupt` (which removes the entry from `subscriptions`).
          const entry = subscriptions.get(requestId);
          if (!entry) return;
          subscriptions.delete(requestId);
          if (!entry.sent) return;
          try {
            send({
              requestId,
              payload: {
                traitId: ids.trait,
                methodId: ids.method,
                messageType: MESSAGE_TYPE_STOP,
                value: STOP_FRAME,
              },
            });
          } catch {
            // provider already closed
          }
        },
      };
    },
    registerHostInitiatedSubscription<Request, Item, Reason>({
      ids,
      decodeRequest,
      encodeItem,
      encodeInterrupt,
      declinePayload,
      bufferCapacity,
    }: RegisterHostInitiatedSubscriptionParams<Request, Item, Reason>) {
      const key = pairKey(ids.trait, ids.method);
      if (hostRoutes.has(key)) {
        throw new Error(
          `host-initiated subscription (${ids.trait}, ${ids.method}) is already registered`,
        );
      }
      const route: HostRoute = {
        ids,
        decodeRequest: decodeRequest as (payload: Uint8Array) => unknown,
        encodeItem: encodeItem as (item: unknown) => Uint8Array,
        encodeInterrupt: encodeInterrupt as (reason?: unknown) => Uint8Array,
        declinePayload,
        bufferCapacity,
        buffered: [],
        instances: new Map(),
      };
      hostRoutes.set(key, route);
      return {
        setHandler(
          handler: HostInitiatedSubscriptionHandler<Request, Item, Reason>,
        ) {
          const installed = handler as HostInitiatedSubscriptionHandler<
            unknown,
            unknown,
            unknown
          >;
          route.handler = installed;
          for (const start of route.buffered.splice(0)) {
            startHostSubscription(route, start.requestId, start.payload);
          }
          return {
            unsubscribe() {
              if (route.handler === installed) route.handler = undefined;
            },
          };
        },
      };
    },
    /**
     * Close this transport and detach its provider listeners.
     */
    dispose() {
      // Idempotent: closeWithError is a no-op once closedError is set, and
      // unsubscribe handles tolerate being called twice.
      try {
        closeWithError(new Error("transport disposed"));
      } finally {
        unsubscribeMessage();
        unsubscribeClose?.();
        unsubscribeReset?.();
      }
    },
  };
}
