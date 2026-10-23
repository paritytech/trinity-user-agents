import {
  decodeWireMessage,
  MESSAGE_TYPE_INTERRUPT,
  MESSAGE_TYPE_RECEIVE,
  MESSAGE_TYPE_START,
  MESSAGE_TYPE_STOP,
  type WebSocketWireProvider,
} from '@parity/truapi';
import { CHAT_ACTION_SUBSCRIBE, RENDERER_ACTION_SUBSCRIBE } from '@parity/truapi/wire-table';

const ACTION_STREAMS = [
  { name: 'renderer', ids: RENDERER_ACTION_SUBSCRIBE },
  { name: 'chat', ids: CHAT_ACTION_SUBSCRIBE },
];

/**
 * Wrap the host connection's provider to log the product's action streams, the
 * host-fed streams that carry presses inside product-drawn bodies and chat input.
 *
 * A press reaches the product only while it holds one of these streams open, and
 * losing the connection ends them without opening new ones. Logging every start,
 * delivery, interruption and loss tells a press the product never received apart
 * from one it received and ignored.
 */
export function logActionStreams(
  createProvider: (url: string) => WebSocketWireProvider,
  log: (line: string) => void,
): (url: string) => WebSocketWireProvider {
  return (url) => {
    const provider = createProvider(url);
    const open = new Map<string, string>();

    function endAll(cause: string): void {
      if (open.size === 0) return;
      const ended = [...open].map(([requestId, name]) => `${name} ${requestId}`).join(', ');
      open.clear();
      log(`[truapi] ${cause}; ended action streams: ${ended}`);
    }

    return {
      opened: provider.opened,
      postMessage(frame) {
        const stream = actionStream(frame);
        if (stream?.messageType === MESSAGE_TYPE_START) {
          open.set(stream.requestId, stream.name);
          log(`[truapi] ${stream.name} action stream ${stream.requestId} started`);
        } else if (stream?.messageType === MESSAGE_TYPE_STOP) {
          open.delete(stream.requestId);
          log(`[truapi] ${stream.name} action stream ${stream.requestId} stopped by the product`);
        }
        provider.postMessage(frame);
      },
      subscribe(callback) {
        return provider.subscribe((frame) => {
          const stream = actionStream(frame);
          if (stream?.messageType === MESSAGE_TYPE_RECEIVE) {
            log(`[truapi] ${stream.name} action stream ${stream.requestId} delivered an action`);
          } else if (stream?.messageType === MESSAGE_TYPE_INTERRUPT) {
            open.delete(stream.requestId);
            log(`[truapi] ${stream.name} action stream ${stream.requestId} interrupted by the host`);
          }
          callback(frame);
        });
      },
      subscribeClose(callback) {
        return provider.subscribeClose?.((error) => {
          endAll(`host connection lost (${error.message})`);
          callback(error);
        }) ?? (() => {});
      },
      dispose() {
        endAll('host connection disposed');
        provider.dispose();
      },
    };
  };
}

function actionStream(frame: Uint8Array) {
  const decoded = decodeWireMessage(frame);
  if (decoded.isErr()) return undefined;
  const { requestId, payload } = decoded.value;
  const stream = ACTION_STREAMS.find(
    ({ ids }) => ids.trait === payload.traitId && ids.method === payload.methodId,
  );
  return stream && { name: stream.name, requestId, messageType: payload.messageType };
}
