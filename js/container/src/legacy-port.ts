import {
  ConnectionResetError,
  decodeWireMessage,
  encodeWireMessage,
  MESSAGE_TYPE_INTERRUPT,
  MESSAGE_TYPE_REQUEST,
  MESSAGE_TYPE_RESPONSE,
  MESSAGE_TYPE_STOP,
  scale,
  type ProtocolMessage,
} from "@parity/truapi";
import * as wireTable from "@parity/truapi/wire-table";

/** How the port reaches the host's current connection. */
export interface LegacyPortLink {
  /** Send a frame on the verified connection; `false` when there is none. */
  send(frame: Uint8Array): boolean;
  /** Start connecting, so that waiting frames can be sent. */
  dial(): void;
}

/**
 * The host's end of the `MessagePort` that SDKs predating the injected
 * transport encode their frames over.
 *
 * Such an SDK learns nothing about the connection behind the port, so the port
 * stands in for it: frames sent without a connection wait for the next one,
 * and whatever a lost connection carried is ended with frames every SDK
 * version decodes.
 */
export interface LegacyPort {
  /** The product's end. */
  readonly port: MessagePort;
  /** Deliver a frame the host addressed to the product. */
  receive(frame: Uint8Array, message: ProtocolMessage): void;
  /** Send the frames that waited for a connection. */
  connected(): void;
  /** End every call and stream the product has open; none of them survives. */
  lost(): void;
}

type Address = Pick<ProtocolMessage["payload"], "traitId" | "methodId">;

const METHOD_KINDS = new Map(
  Object.values(wireTable).map(({ trait, method, kind }) => [
    `${trait}:${method}`,
    kind,
  ]),
);

// A call's response and a stream's interrupt carry the same result, and
// `HostFailure` is a call error every method shares, so no method codec is needed.
const LOST = scale.Result(scale._void, scale.CallError(scale._void)).enc({
  success: false,
  value: {
    tag: "HostFailure",
    value: { reason: new ConnectionResetError().message },
  },
});
const STOPPED = new Uint8Array();

/** Open the port and serve it over `link`. */
export function createLegacyPort(link: LegacyPortLink): LegacyPort {
  const { port1: port, port2: host } = new MessageChannel();
  const calls = new Map<string, Address>();
  const streams = new Map<string, Address>();
  const hostStreams = new Map<string, Address>();
  const waiting: Uint8Array[] = [];

  function kindOf({ traitId, methodId }: Address) {
    return METHOD_KINDS.get(`${traitId}:${methodId}`);
  }

  function end(
    open: Map<string, Address>,
    messageType: number,
    value: Uint8Array,
  ): void {
    for (const [requestId, address] of open) {
      const frame = encodeWireMessage({
        requestId,
        payload: { ...address, messageType, value },
      });
      if (frame.isOk()) host.postMessage(frame.value);
    }
    open.clear();
  }

  host.addEventListener("message", (event: MessageEvent) => {
    const { data: frame } = event;
    if (!(frame instanceof Uint8Array)) return;
    const decoded = decodeWireMessage(frame);
    if (decoded.isErr() || decoded.value.requestId.startsWith("host:")) return;
    const { requestId, payload } = decoded.value;
    if (payload.messageType === MESSAGE_TYPE_REQUEST) {
      const kind = kindOf(payload);
      if (kind === "request") calls.set(requestId, payload);
      else if (kind === "subscription") streams.set(requestId, payload);
    } else if (payload.messageType === MESSAGE_TYPE_STOP)
      streams.delete(requestId);
    else if (payload.messageType === MESSAGE_TYPE_INTERRUPT)
      hostStreams.delete(requestId);
    if (link.send(frame)) return;
    waiting.push(frame);
    link.dial();
  });
  host.start();

  return {
    port,
    receive(frame, { requestId, payload }) {
      if (payload.messageType === MESSAGE_TYPE_RESPONSE)
        calls.delete(requestId);
      else if (payload.messageType === MESSAGE_TYPE_INTERRUPT)
        streams.delete(requestId);
      else if (payload.messageType === MESSAGE_TYPE_STOP)
        hostStreams.delete(requestId);
      else if (
        payload.messageType === MESSAGE_TYPE_REQUEST &&
        kindOf(payload) === "subscription"
      )
        hostStreams.set(requestId, payload);
      host.postMessage(frame);
    },
    connected() {
      for (const frame of waiting.splice(0)) link.send(frame);
    },
    lost() {
      waiting.length = 0;
      end(calls, MESSAGE_TYPE_RESPONSE, LOST);
      end(streams, MESSAGE_TYPE_INTERRUPT, LOST);
      end(hostStreams, MESSAGE_TYPE_STOP, STOPPED);
    },
  };
}
