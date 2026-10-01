import {
  freezeAndDelete,
  freezeCustom,
  freezeValue,
  reportLockdownFailures,
} from './freeze.js';
import { installWebRtcPolicy } from './webrtc.js';
import { installFetchGate } from './network.js';
import { installXhrGate } from './xhr.js';
import { installWebSocketGate, type WebSocketBackendFactory } from './websocket.js';
import { installMediaPolicy } from './media.js';
import type { createPermissionAuthorization } from './network-transport.js';

export type PermissionAuthorization = ReturnType<typeof createPermissionAuthorization>;

export function installContainer(
  _authorize: PermissionAuthorization,
  options: { nativeHttp?: boolean } = {},
): void {
  const runtime = window as Window & {
    __truapi_websocket_connect__?: WebSocketBackendFactory;
  };
  const _webSocketBackend = runtime.__truapi_websocket_connect__;
  freezeAndDelete(window, '__truapi_websocket_connect__');
  installWebSocketGate(window, _authorize.network, _webSocketBackend);

  // Android's WebView interceptor already asks Rust to authorize HTTP.
  // A second check here would require another grant for "Allow once".
  if (!options.nativeHttp) {
    installFetchGate(window, _authorize.network);
    installXhrGate(window, _authorize.network);
  }
  installMediaPolicy(window, _authorize.media);

  // --- Network: delete (no future permission path) ---
  freezeAndDelete(window, 'EventSource');
  freezeAndDelete(window, 'WebTransport');

  freezeValue(navigator, 'sendBeacon', () => false);

  // --- Storage ---
  freezeAndDelete(window, 'indexedDB');
  freezeAndDelete(window, 'caches');

  // document.cookie — redefine as no-op getter/setter
  freezeCustom(
    document,
    'cookie',
    { get: () => '', set: () => {} },
    (current) => current === '',
  );

  // --- Workers ---
  freezeAndDelete(window, 'Worker');
  freezeAndDelete(window, 'SharedWorker');

  if (navigator.serviceWorker) {
    const _stubServiceWorker = Object.freeze({
      register: () => { throw new Error('ServiceWorker is not available'); },
    });
    freezeCustom(
      navigator,
      'serviceWorker',
      { value: _stubServiceWorker, writable: false },
      (current) => current === _stubServiceWorker,
    );
  }

  // --- DOM: block iframe creation ---
  const _createElement = document.createElement.bind(document);
  freezeValue(document, 'createElement', (tagName: string, options?: ElementCreationOptions) => {
    if (tagName.toLowerCase() === 'iframe') {
      throw new Error('iframe creation is not allowed');
    }
    return _createElement(tagName, options);
  });

  installWebRtcPolicy(window, _authorize.webRtc);

  // --- Report: every lock above has been attempted, so a failure can throw ---
  // A lock that did not take is a hole in the sandbox. Reporting last means the
  // throw costs no coverage, and it means the host learns rather than serving
  // products into a realm it believes is closed.
  reportLockdownFailures();
}
