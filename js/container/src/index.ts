// Subframes need the same gates even though only the main frame has an endpoint.
import { installContainer } from './container.js';
import { createHostConnection, createMessagePortBridge } from '@parity/truapi/internal';
import { installMessagePortBootstrap } from './port-bootstrap.js';
import { freezeAndDelete, freezeValue } from './freeze.js';
import { reloadAfterMessagePortLoss } from './message-port-loss.js';
import { createPermissionAuthorization } from './network-transport.js';
import { freezePermissionRuntime } from './permission-runtime.js';

freezePermissionRuntime();

const config = (window as Window & {
  __truapi_localhost?: { url?: string; nativeHttp?: boolean };
}).__truapi_localhost;
freezeAndDelete(window, '__truapi_localhost');
// A web host cannot run a native bridge. It sets this before the container
// loads, to `true` or to its own origin, and hands over a MessagePort
// afterwards. A native host's WebSocket configuration always takes precedence.
const portFlag = (window as Window & { __truapi_message_port?: unknown })
  .__truapi_message_port;
const portMode = portFlag === true || typeof portFlag === 'string';
const parentOrigin = typeof portFlag === 'string' ? portFlag : undefined;
freezeAndDelete(window, '__truapi_message_port');
const portBridge = portMode && typeof config?.url !== 'string'
  ? createMessagePortBridge()
  : undefined;
const connection = typeof config?.url === 'string'
  ? createHostConnection(config.url)
  : portBridge
    ? createHostConnection('message-port:', portBridge.createProvider)
    : undefined;
if (portBridge) {
  installMessagePortBootstrap(window, portBridge, parentOrigin);
  window.addEventListener('pagehide', () => connection?.dispose());
} else if (connection) {
  freezeValue(window, '__HOST_WEBVIEW_MARK__', true);
  freezeValue(window, '__HOST_API_CLIENT__', Object.freeze({
    get client() { return connection.client; },
    subscribeConnectionStatus: connection.subscribeConnectionStatus,
  }));
  // TODO: Remove the port once deployed products adopt the injected client.
  Object.defineProperty(window, '__HOST_API_PORT__', {
    get: () => connection.legacyPort,
    set() {},
    configurable: false,
  });
  const stopWatchingMessagePorts = reloadAfterMessagePortLoss(
    window,
    connection.subscribeConnectionStatus,
  );
  window.addEventListener('pagehide', () => {
    stopWatchingMessagePorts();
    connection.dispose();
  });
}
installContainer(createPermissionAuthorization(window, connection?.internal), {
  nativeHttp: config?.nativeHttp === true,
});
