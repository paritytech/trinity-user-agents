import { type WireProvider } from '@parity/truapi';
import { type DidFailLoadEvent, type WebviewTag } from 'electron';
import { nanoid } from 'nanoid';

/**
 * The global the guest reads its host channel from. The core's own guest bootstrap
 * (`@parity/truapi/sandbox`) looks for this exact name, so a product built against
 * the core connects with no extra wiring.
 */
const HOST_PORT_GLOBAL = '__HOST_API_PORT__';

type Params = {
  webview: WebviewTag;
  openDevTools?: boolean;
};

/**
 * A `WireProvider` over an Electron webview, handed to `pipeProviders` as the
 * product end of the wire.
 *
 * The channel is a `MessageChannel` whose far port is transferred into the guest on
 * `dom-ready`. The one-time nonce in the handshake message is what stops an
 * unrelated `postMessage` from claiming the port: the guest listener only accepts
 * the message carrying the nonce this call generated, then removes itself.
 *
 * Ported from `@novasamatech/host-container` when the host retired that SDK; the
 * core ships no host-side transport of its own.
 */
export function createWebviewProvider({ webview, openDevTools }: Params): WireProvider {
  let disposed = false;
  let port: MessagePort | null = null;
  const subscribers = new Set<(message: Uint8Array) => void>();
  const closeSubscribers = new Set<(error: Error) => void>();

  // Fires once, for good. `pipeProviders` disposes the paired core provider from here,
  // so a second call would tear down a pipe that has already been rebuilt. A guest
  // NAVIGATION is not a close — the provider deliberately outlives it and rebinds a
  // fresh channel — so only `dispose` and a failed load reach this.
  // The terminal reason, kept so a late subscriber can be told immediately — the
  // contract says a registration after close fires straight away.
  let closeReason: Error | null = null;
  function notifyClosed(reason: Error) {
    if (closeReason) return;
    closeReason = reason;

    for (const subscriber of [...closeSubscribers]) subscriber(reason);
    closeSubscribers.clear();
  }

  const messageHandler = (event: MessageEvent) => {
    if (disposed) return;
    // Frames only. The guest can post anything; a non-`Uint8Array` is not ours.
    if (!(event.data instanceof Uint8Array)) return;

    for (const subscriber of subscribers) {
      subscriber(event.data);
    }
  };

  function closePort() {
    if (!port) return;

    port.removeEventListener('message', messageHandler);
    port.close();
    port = null;
  }

  let resolvePort: (port: MessagePort) => void;
  let rejectPort: (reason: Error) => void;
  const portReady = new Promise<MessagePort>((resolve, reject) => {
    resolvePort = resolve;
    rejectPort = reject;
  });

  // A failed load rejects `portReady` whether or not a send is parked on it. Nothing
  // attached is the common case, and an unhandled rejection in the renderer is noise
  // that outlives the webview — `subscribeClose` below is how the failure is reported.
  void portReady.catch(() => {});

  const onFailLoad = (event: DidFailLoadEvent) => {
    const reason = new Error(event.errorDescription);
    rejectPort(reason);
    notifyClosed(reason);
  };

  // `dom-ready` fires on every guest navigation, not once — and the host reloads
  // guests deliberately (a sign-in reloads every open product). Each fire hands the
  // guest a fresh channel, so the previous one has to be torn down or its port and
  // listener outlive it.
  const onDomReady = async () => {
    if (disposed) return;

    closePort();

    const { port1, port2 } = new MessageChannel();
    const handshake = `HOST_API_PORT_INIT_${nanoid(12)}`;

    try {
      // Install the guest listener BEFORE transferring the port. `executeJavaScript`
      // resolves only after the injected script has run, so awaiting it guarantees the
      // guest's `message` handler is registered when the port arrives. Posting the port
      // without the await races the listener install: when the transfer wins the race the
      // guest never captures the port, `__HOST_API_PORT__` stays unset, and the product
      // renders blank — non-deterministically, on roughly half of loads.
      await webview.executeJavaScript(
        `
          (function() {
            function handler(e) {
              if (e.data !== '${handshake}') return;
              window.removeEventListener('message', handler);
              const port = e.ports[0];
              if (port) {
                window['${HOST_PORT_GLOBAL}'] = port;
              }
            }
            window.addEventListener('message', handler);
          })();
        `,
      );
    } catch (error) {
      rejectPort(error instanceof Error ? error : new Error(String(error)));

      return;
    }

    // `dispose` (or a fresh navigation) can land while `executeJavaScript` is pending.
    if (disposed) return;

    // `contentWindow` is absent from Electron's `WebviewTag` type but present at
    // runtime once the guest document exists — which `dom-ready` is the signal for.
    const guest: Nullable<Window> = Reflect.get(webview, 'contentWindow');
    guest?.postMessage(handshake, '*', [port2]);

    if (openDevTools) {
      webview.openDevTools();
    }

    port = port1;
    port.start();
    port.addEventListener('message', messageHandler);
    resolvePort(port1);
  };

  webview.addEventListener('did-fail-load', onFailLoad);
  webview.addEventListener('dom-ready', onDomReady);

  function withPort(callback: (port: MessagePort) => void) {
    if (port) {
      callback(port);

      return;
    }

    void portReady.then(callback);
  }

  return {
    postMessage(message) {
      if (disposed) return;

      withPort(open => {
        // Checked again inside: `dispose` can land while the port promise is pending.
        if (disposed) return;

        open.postMessage(message, [message.buffer]);
      });
    },
    subscribe(callback) {
      subscribers.add(callback);

      return () => {
        subscribers.delete(callback);
      };
    },
    /**
     * Without this the host end of the pipe is silent on close: `pipeProviders` reaches
     * for `subscribeClose` optionally, so an absent one means a guest that crashes or
     * goes away never disposes the core provider paired with it.
     */
    subscribeClose(callback) {
      if (closeReason) {
        callback(closeReason);

        return () => {};
      }

      closeSubscribers.add(callback);

      return () => {
        closeSubscribers.delete(callback);
      };
    },
    dispose() {
      disposed = true;
      subscribers.clear();
      notifyClosed(new Error('webview provider disposed'));
      // The webview outlives this provider (a re-pipe builds a new one over the same
      // element). Leaving these attached lets a disposed provider keep handshaking on
      // the next navigation, racing dead ports against the live one.
      webview.removeEventListener('dom-ready', onDomReady);
      webview.removeEventListener('did-fail-load', onFailLoad);
      closePort();
    },
  };
}
