/**
 * Early capture of the call window's `__callInit` handshake.
 *
 * The main process transfers the call launch params + the MessagePort to the
 * call window via `webContents.postMessage('call:init', …, [port])`; the call
 * preload re-posts it into the renderer main world as
 * `window.postMessage({ __callInit }, origin, [port])` on `did-finish-load` —
 * which fires BEFORE React mounts. So this listener is registered at module
 * load (this module must be imported eagerly by the renderer entry) and buffers
 * the delivery until `CallWindowScreen` subscribes, so the transferred port is
 * never dropped by a listener that isn't up yet.
 *
 * In the main app window `__callInit` never arrives (that window receives
 * `__callBridgePort` instead), so this listener is an inert no-op there.
 */

import { type CallWindowLaunch } from '@/shared/call-bridge';
import { type IceConfigParams } from '@/shared/peer-channel';

export type CallInit = { launch: CallWindowLaunch; iceConfig: IceConfigParams };
export type CallInitDelivery = { callInit: CallInit; port: MessagePort };

let buffered: Nullable<CallInitDelivery> = null;
let subscriber: Nullable<(delivery: CallInitDelivery) => void> = null;

function parseCallInit(value: unknown): Nullable<CallInit> {
  if (typeof value !== 'object' || value === null) return null;
  const launch = 'launch' in value ? value.launch : undefined;
  const iceConfig = 'iceConfig' in value ? value.iceConfig : undefined;
  if (typeof launch !== 'object' || launch === null) return null;
  if (typeof iceConfig !== 'object' || iceConfig === null) return null;
  const direction = 'direction' in launch ? launch.direction : undefined;
  const purpose = 'purpose' in launch ? launch.purpose : undefined;
  const offerId = 'offerId' in launch ? launch.offerId : undefined;
  const peerName = 'peerName' in launch ? launch.peerName : undefined;
  if (direction !== 'outgoing' && direction !== 'incoming') return null;
  if (purpose !== 'audio' && purpose !== 'video') return null;
  if (typeof offerId !== 'string' || typeof peerName !== 'string') return null;
  // iceConfig is passed opaquely to createCallPeerConnection.
  // eslint-disable-next-line @typescript-eslint/consistent-type-assertions
  return { launch: { direction, purpose, offerId, peerName }, iceConfig: iceConfig as IceConfigParams };
}

if (typeof window !== 'undefined') {
  window.addEventListener('message', (event: MessageEvent) => {
    // Only accept the init+port from our own preload — `source === window` is what
    // establishes that: any other window or frame arrives with a different source.
    // The origin cannot be compared: packaged builds load `file://`, where
    // `location.origin` serializes to "file://" but the delivered `event.origin` is
    // the opaque "null", so an equality check drops every delivery.
    if (event.source !== window) return;
    const callInit = parseCallInit(event.data?.__callInit);
    const port = event.ports[0];
    if (!callInit || port === undefined) return;
    const delivery: CallInitDelivery = { callInit, port };
    if (subscriber) subscriber(delivery);
    else buffered = delivery;
  });
}

/** Subscribe to the call-init delivery. Fires immediately if it already arrived. */
export function onCallInit(callback: (delivery: CallInitDelivery) => void): void {
  if (buffered) {
    const delivery = buffered;
    buffered = null;
    callback(delivery);
    return;
  }
  subscriber = callback;
}

/**
 * Whether the current renderer is the dedicated call window (its own
 * BrowserWindow, loaded at the `#/call` route). Lets the app entry pin the call
 * window to its dark-only theme without the entry knowing the call route.
 */
export function isCallWindowLocation(): boolean {
  return typeof window !== 'undefined' && window.location.hash.startsWith('#/call');
}
